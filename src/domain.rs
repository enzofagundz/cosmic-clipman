// SPDX-License-Identifier: Apache-2.0

//! Clipboard history types and the rules that do not need a database.

use chrono::{DateTime, Datelike, Local};

use crate::fl;

/// Fixed defaults; see `docs/adr/0004-rusqlite-fixed-defaults.md`.
pub const RETENTION_LIMIT: i64 = 500;
pub const SENSITIVE_TIMEOUT_SECS: i64 = 30;

/// Same limit as the original, in UTF-8 bytes.
pub const MAX_PAYLOAD_BYTES: usize = 10 * 1024 * 1024;

/// How much of a clip the popup shows before eliding.
pub const PREVIEW_CHARS: usize = 200;

/// One row of the history.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: i64,
    pub text: String,
    pub pinned: bool,
    pub sensitive: bool,
    pub created_at: f64,
    pub accessed_at: f64,
}

impl Item {
    /// The heading of a row: the first line of the clip, elided, or the
    /// placeholder for a sensitive or empty clip.
    pub fn preview(&self) -> String {
        if self.sensitive {
            return fl!("sensitive");
        }
        preview(&self.text)
    }

    /// Whether the query matches the clip. Search sees sensitive text as the
    /// original does; only the display is masked.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty() || self.text.to_lowercase().contains(&query)
    }

    pub fn age_seconds(&self, now: f64) -> i64 {
        (now - self.created_at) as i64
    }
}

/// First non-blank line, elided to [`PREVIEW_CHARS`].
pub fn preview(text: &str) -> String {
    let Some(line) = text.lines().find(|line| !line.trim().is_empty()) else {
        return String::new();
    };
    let mut out: String = line.chars().take(PREVIEW_CHARS).collect();
    if line.chars().count() > PREVIEW_CHARS {
        out.push('…');
    }
    out
}

/// Date sections of the list, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Pinned,
    Today,
    Yesterday,
    EarlierThisWeek,
    Older,
}

impl Group {
    pub fn label(self) -> String {
        match self {
            Group::Pinned => fl!("pinned"),
            Group::Today => fl!("today"),
            Group::Yesterday => fl!("yesterday"),
            Group::EarlierThisWeek => fl!("earlier-this-week"),
            Group::Older => fl!("older"),
        }
    }
}

pub fn group_of(item: &Item, now: DateTime<Local>) -> Group {
    if item.pinned {
        return Group::Pinned;
    }
    let stamp = DateTime::from_timestamp(item.accessed_at as i64, 0)
        .map(|utc| utc.with_timezone(&Local))
        .unwrap_or(now);
    let item_date = stamp.date_naive();
    let today = now.date_naive();
    if item_date.year() == today.year() && item_date.ordinal() == today.ordinal() {
        return Group::Today;
    }
    let days = (today - item_date).num_days();
    match days {
        1 => Group::Yesterday,
        2..=6 => Group::EarlierThisWeek,
        _ => Group::Older,
    }
}

/// Items in display order, bucketed into sections with the pinned items first.
pub fn grouped<'a>(items: &[&'a Item], now: DateTime<Local>) -> Vec<(Group, Vec<&'a Item>)> {
    let mut buckets: Vec<(Group, Vec<&'a Item>)> = Vec::new();
    for item in items {
        let group = group_of(item, now);
        match buckets.iter_mut().find(|(bucket, _)| *bucket == group) {
            Some((_, rows)) => rows.push(item),
            None => buckets.push((group, vec![item])),
        }
    }
    buckets
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(seconds_ago: i64, pinned: bool, sensitive: bool) -> Item {
        let now = Local::now();
        let stamp = now.timestamp() as f64 - seconds_ago as f64;
        Item {
            id: seconds_ago,
            text: format!("clip {seconds_ago}"),
            pinned,
            sensitive,
            created_at: stamp,
            accessed_at: stamp,
        }
    }

    #[test]
    fn preview_uses_the_first_line_and_elides() {
        assert_eq!(preview("one\ntwo"), "one");
        assert_eq!(preview("  \nsecond"), "second");
        let long = "a".repeat(PREVIEW_CHARS + 10);
        assert!(preview(&long).ends_with('…'));
        assert_eq!(preview(&long).chars().count(), PREVIEW_CHARS + 1);
    }

    #[test]
    fn search_is_case_insensitive_and_ignores_surrounding_space() {
        let mut clip = item(0, false, false);
        clip.text = "Hello World".into();
        assert!(clip.matches(" hello "));
        assert!(clip.matches("world"));
        assert!(!clip.matches("hola"));
        assert!(clip.matches(""));
    }

    #[test]
    fn pinned_items_come_first_in_their_own_group() {
        // The caller hands items in display order (pinned first), as the store does.
        let items = [item(20, true, false), item(10, false, false)];
        let items: Vec<&Item> = items.iter().collect();
        let now = Local::now();
        let buckets = grouped(&items, now);
        assert_eq!(buckets[0].0, Group::Pinned);
        assert_eq!(buckets[0].1.len(), 1);
        assert_eq!(buckets[1].0, Group::Today);
    }

    #[test]
    fn older_clips_land_in_the_older_group() {
        let old = item(10 * 24 * 3600, false, false);
        assert_eq!(group_of(&old, Local::now()), Group::Older);
    }

    #[test]
    fn sensitive_preview_is_masked() {
        let clip = item(0, false, true);
        assert_eq!(clip.preview(), fl!("sensitive"));
    }
}
