// SPDX-License-Identifier: Apache-2.0

//! The SQLite store.
//!
//! Ported from Clipman's `clipman/database.py` (Apache-2.0, see NOTICE): the
//! same deduplication by content hash, the same retention rule (pinned items
//! exempt), the same timed purge of sensitive clips, and the same permissions
//! on the data directory and its files.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::domain::{Item, RETENTION_LIMIT};

pub const DATA_DIR_NAME: &str = "cosmic-clipman";
pub const DB_FILE_NAME: &str = "history.db";

const SCHEMA_VERSION: i64 = 1;

#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    NoDataDir,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Sqlite(err) => write!(f, "database error: {err}"),
            StoreError::Io(err) => write!(f, "data directory error: {err}"),
            StoreError::NoDataDir => write!(f, "no usable data directory"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(err: rusqlite::Error) -> Self {
        StoreError::Sqlite(err)
    }
}

impl From<std::io::Error> for StoreError {
    fn from(err: std::io::Error) -> Self {
        StoreError::Io(err)
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// What an insert did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Insert {
    /// A new row.
    New(i64),
    /// The same content was already stored; its `accessed_at` moved.
    Bumped(i64),
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (creating if needed) the database inside `dir`, tightening the
    /// directory and file permissions.
    pub fn open_in(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        let path = dir.join(DB_FILE_NAME);
        let conn = Connection::open(&path)?;
        let store = Store { conn };
        store.conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        tighten(&path);
        for sidecar in ["-wal", "-shm"] {
            tighten(&path.with_file_name(format!("{DB_FILE_NAME}{sidecar}")));
        }
        store.migrate()?;
        Ok(store)
    }

    /// Open the database in the user's data directory
    /// (`$XDG_DATA_HOME/cosmic-clipman`, `~/.local/share` by default).
    pub fn open_default() -> Result<Self> {
        let dir = data_dir().ok_or(StoreError::NoDataDir)?;
        Store::open_in(&dir)
    }

    /// An in-memory database, for tests and for a degraded start.
    pub fn open_in_memory() -> Result<Self> {
        let store = Store {
            conn: Connection::open_in_memory()?,
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version >= SCHEMA_VERSION {
            return Ok(());
        }
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content_type TEXT NOT NULL,
                content_text TEXT,
                content_hash TEXT NOT NULL UNIQUE,
                pinned INTEGER NOT NULL DEFAULT 0,
                sensitive INTEGER NOT NULL DEFAULT 0,
                created_at REAL NOT NULL,
                accessed_at REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_entries_accessed_at
                ON entries(accessed_at DESC);
            CREATE INDEX IF NOT EXISTS idx_entries_content_hash
                ON entries(content_hash);
            ",
        )?;
        self.conn
            .execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        Ok(())
    }

    pub fn insert_text(&mut self, text: &str, sensitive: bool, now: f64) -> Result<Insert> {
        let hash = hex(Sha256::digest(text.as_bytes()));
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM entries WHERE content_hash = ?1",
                params![hash],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            self.conn.execute(
                "UPDATE entries SET accessed_at = ?1 WHERE id = ?2",
                params![now, id],
            )?;
            return Ok(Insert::Bumped(id));
        }
        self.conn.execute(
            "INSERT INTO entries
             (content_type, content_text, content_hash, pinned, sensitive, created_at, accessed_at)
             VALUES ('text', ?1, ?2, 0, ?3, ?4, ?4)",
            params![text, hash, i64::from(sensitive), now],
        )?;
        let id = self.conn.last_insert_rowid();
        self.prune(RETENTION_LIMIT)?;
        Ok(Insert::New(id))
    }

    pub fn list(&self, limit: i64) -> Result<Vec<Item>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content_text, pinned, sensitive, created_at, accessed_at
             FROM entries
             ORDER BY pinned DESC, accessed_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], row_to_item)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn search(&self, query: &str, limit: i64) -> Result<Vec<Item>> {
        // Escape LIKE wildcards so user input is treated literally.
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let mut stmt = self.conn.prepare(
            "SELECT id, content_text, pinned, sensitive, created_at, accessed_at
             FROM entries
             WHERE content_type = 'text' AND content_text LIKE ?1 ESCAPE '\\'
             ORDER BY pinned DESC, accessed_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![format!("%{escaped}%"), limit], row_to_item)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: i64) -> Result<Option<Item>> {
        let item = self
            .conn
            .query_row(
                "SELECT id, content_text, pinned, sensitive, created_at, accessed_at
                 FROM entries WHERE id = ?1",
                params![id],
                row_to_item,
            )
            .optional()?;
        Ok(item)
    }

    pub fn set_pin(&mut self, id: i64, pinned: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE entries SET pinned = ?1 WHERE id = ?2",
            params![i64::from(pinned), id],
        )?;
        Ok(())
    }

    pub fn touch(&mut self, id: i64, now: f64) -> Result<()> {
        self.conn.execute(
            "UPDATE entries SET accessed_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        Ok(())
    }

    pub fn delete(&mut self, id: i64) -> Result<bool> {
        let removed = self
            .conn
            .execute("DELETE FROM entries WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }

    pub fn clear_unpinned(&mut self) -> Result<usize> {
        Ok(self
            .conn
            .execute("DELETE FROM entries WHERE pinned = 0", [])?)
    }

    /// Delete sensitive clips older than the timeout. Pinned items are kept,
    /// because a pin means "keep this" — often a clip the detector got wrong.
    pub fn purge_expired_sensitive(&mut self, now: f64, timeout_secs: i64) -> Result<usize> {
        let cutoff = now - timeout_secs as f64;
        Ok(self.conn.execute(
            "DELETE FROM entries WHERE sensitive = 1 AND pinned = 0 AND created_at < ?1",
            params![cutoff],
        )?)
    }

    pub fn count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?)
    }

    /// Keep the newest `limit` unpinned items, deleting older ones.
    pub fn prune(&mut self, limit: i64) -> Result<usize> {
        let total: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM entries WHERE pinned = 0", [], |row| {
                    row.get(0)
                })?;
        if total <= limit {
            return Ok(0);
        }
        let excess = total - limit;
        Ok(self.conn.execute(
            "DELETE FROM entries WHERE id IN (
                 SELECT id FROM entries WHERE pinned = 0
                 ORDER BY accessed_at ASC LIMIT ?1
             )",
            params![excess],
        )?)
    }
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
    Ok(Item {
        id: row.get(0)?,
        text: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
        pinned: row.get::<_, i64>(2)? != 0,
        sensitive: row.get::<_, i64>(3)? != 0,
        created_at: row.get(4)?,
        accessed_at: row.get(5)?,
    })
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

fn tighten(path: &Path) {
    if path.exists() {
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
}

pub fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME")
        && !dir.is_empty()
    {
        return Some(PathBuf::from(dir).join(DATA_DIR_NAME));
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".local/share").join(DATA_DIR_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = Store::open_in(dir.path()).expect("store opens");
        (dir, store)
    }

    #[test]
    fn inserts_and_dedupes_by_content_hash() {
        let (_dir, mut store) = temp_store();
        assert_eq!(
            store.insert_text("hello", false, 1.0).unwrap(),
            Insert::New(1)
        );
        assert_eq!(
            store.insert_text("hello", false, 2.0).unwrap(),
            Insert::Bumped(1)
        );
        assert_eq!(store.count().unwrap(), 1);
        assert_eq!(store.list(10).unwrap()[0].accessed_at, 2.0);
        assert_eq!(
            store.insert_text("other", false, 3.0).unwrap(),
            Insert::New(2)
        );
        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn pins_survive_the_retention_limit() {
        let (_dir, mut store) = temp_store();
        for i in 0..10 {
            store
                .insert_text(&format!("clip {i}"), false, i as f64)
                .unwrap();
        }
        store.set_pin(1, true).unwrap();
        store.prune(3).unwrap();
        let items = store.list(100).unwrap();
        assert!(items.iter().any(|item| item.id == 1 && item.pinned));
        assert_eq!(items.len(), 4);
    }

    #[test]
    fn clear_unpinned_keeps_pinned_items() {
        let (_dir, mut store) = temp_store();
        store.insert_text("keep", false, 1.0).unwrap();
        store.insert_text("drop", false, 2.0).unwrap();
        store.set_pin(1, true).unwrap();
        assert_eq!(store.clear_unpinned().unwrap(), 1);
        let items = store.list(10).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "keep");
    }

    #[test]
    fn sensitive_purge_respects_the_timeout_and_pins() {
        let (_dir, mut store) = temp_store();
        store.insert_text("secret", true, 0.0).unwrap();
        store.insert_text("pinned secret", true, 0.0).unwrap();
        store.set_pin(2, true).unwrap();
        assert_eq!(store.purge_expired_sensitive(10.0, 30).unwrap(), 0);
        assert_eq!(store.purge_expired_sensitive(40.0, 30).unwrap(), 1);
        let items = store.list(10).unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].pinned);
    }

    #[test]
    fn search_escapes_like_wildcards() {
        let (_dir, mut store) = temp_store();
        store.insert_text("100% done", false, 1.0).unwrap();
        store.insert_text("nothing here", false, 2.0).unwrap();
        assert_eq!(store.search("%", 10).unwrap().len(), 1);
        assert_eq!(store.search("_", 10).unwrap().len(), 0);
    }

    #[test]
    fn data_directory_and_files_are_private() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("data");
        let store = Store::open_in(&nested).unwrap();
        assert_eq!(
            fs::metadata(&nested).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(nested.join(DB_FILE_NAME))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        drop(store);
    }
}
