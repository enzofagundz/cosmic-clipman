# 4. SQLite through rusqlite, fixed defaults, no settings UI

## Status

Accepted.

## Context

The original stores history in one SQLite database (`entries`, `snippets`,
`settings`) with WAL mode, SHA-256 deduplication, a retention limit between 50
and 5000 configurable in a six-pane settings dialog, an auto-clear delay for
sensitive clips (10–300 s, default 30), and a small key/value table holding the
preferences.

The port's v1 is a text-only core. A settings surface, an appearance section, a
theme system and an update checker are all out of scope (ADR 7), and the two
numbers that matter have a defensible default: a history of 500 and a 30-second
sensitive timeout.

## Decision

- Store through `rusqlite` with the bundled SQLite: synchronous API, no async
  runtime, no second connection, WAL mode, and `PRAGMA user_version` for
  migrations.
- Keep the original's schema shape where it costs nothing: `entries` with a
  `content_hash TEXT NOT NULL UNIQUE`, `pinned`, `created_at`, `accessed_at`,
  `sensitive`, plus indexes on `accessed_at DESC` and `content_hash`.
- Fixed constants in one module: retention limit 500, sensitive timeout 30 s,
  scan limit 64 KiB, maximum payload 10 MiB (the original's own limit).
- Data directory `~/.local/share/cosmic-clipman/` created `0700`, database file
  and sidecars `0600`.

## Consequences

- Deduplication, pruning and the timed purge are pure store operations and are
  unit-tested against a temporary directory, no compositor involved.
- A future settings surface reads the same constants through one accessor; the
  schema already tolerates a `settings` table if it is needed later.
- Behaviour differences from the original (no configurable limit, no
  configurable auto-clear) belong in `docs/compatibility.md`.
