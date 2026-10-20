# 7. v1 scope: text core; images and snippets in phase 2

## Status

Accepted.

## Context

The original ships text and images, snippet templates with `${date}`,
`${time}` and `${clipboard}`, filter tabs, date grouping, link and code
badges, per-item character counts, six settings panes, three themes, opacity
and font controls, backup/restore, an update checker, and twenty designed edge
states. Porting all of it before proving capture, persistence and paste on
COSMIC would invert the risk order: the ports that depend on system integration
would arrive last.

## Decision

v1 is the text core:

- capture of text clips with SHA-256 deduplication, the 10 MiB payload limit and
  the original's MIME preference order (`text/plain*`, then `UTF8_STRING`,
  `STRING`);
- search over text, pinned-first ordering, pin/unpin, delete one, clear
  unpinned with confirmation, retention at 500;
- incognito mode, sensitive detection with masking and the 30-second purge;
- date grouping in the list and the empty / no-results states;
- paste by `Ctrl+V` (ADR 3) and copy-only.

Phase 2, in this order: images (`image/*` to files `0600`, hash-named), then
snippets with variable expansion.

Deliberately out of scope, and listed with reasons in
`docs/compatibility.md`: backup/restore UI, update checker, appearance and
font and opacity settings, filter tabs and link/code badges and character
counts, terminal-aware paste, i18n beyond the template's English strings.

## Consequences

- Every acceptance criterion in v1 is testable on the development session
  without building UI scaffolding for features not yet wired.
- The store keeps `content_type` so phase 2 adds images without a migration
  rewrite; the image path column exists from the start.
- Nothing is dropped silently: each omission has a row in the compatibility
  document.
