# Tests and validation

What was actually run, on which machine, with which result. Nothing here is
projected.

## Environment

- Fedora Linux 44, COSMIC session on Wayland (`wayland-1`), `cosmic-comp`
  1.10.0, `cosmic-panel` 1.10.0.
- Rust 1.99.1 via rustup; libcosmic at git `430f3d4` (the revision
  `Cargo.lock` pins), `wl-clipboard-rs` 0.9.4, `rusqlite` 0.40.2 (bundled
  SQLite), `zbus` 6.0.0, `xkbcommon` 0.9.0.
- Build headers: `wayland-devel` 1.26.0, `libxkbcommon-devel` 1.13.1.

## Build and static checks

| Command | Result |
| --- | --- |
| `cargo check --all-targets` | clean |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `just build-release` | success, 41.5 MB release binary |
| `PREFIX=$HOME/.local just install` | binary, `.desktop`, metainfo and icon installed |

`just check` uses the template's pedantic level and prints its suggestions
without failing; CI runs the `-D warnings` level above.

## Automated tests

`cargo test --all-features`:

- 24 unit tests (store, sensitive rules, grouping and preview, watcher gates) —
  pass.
- 3 corpus tests against the original's `sensitive_corpus.json` (568 benign
  clips, 167 synthetic secrets) — pass: no benign clip flagged, the only missed
  categories are the four the detector skips on purpose, recall stays above the
  original's floor.
- 2 live-session tests — ignored by default.

`cargo test --test live_session -- --ignored --test-threads=1` on the live
COSMIC session:

- `clipboard_write_reaches_the_watcher` — pass: a `copy::Options` write is
  observed by the watcher thread and delivered as a text event.
- `ctrl_v_injection_does_not_error` — pass: the compositor accepts the
  virtual-keyboard device, the uploaded keymap and the `Ctrl+V` pair.

## In the running session

| Check | How | Result |
| --- | --- | --- |
| Applet starts from the panel | added `io.github.enzofagundz.CosmicClipman` to `plugins_center` in the panel config | panel spawned `cosmic-clipman`, process stayed up |
| Panel icon and popup | opened with `cosmic-clipman --toggle` | popup renders under the applet: search field, "Today" section, rows with time, footer with Pin / Copy / Delete / Incognito / Clear |
| Toggle CLI | `cosmic-clipman --toggle` | exit 0; opens and closes the popup |
| D-Bus service | `busctl --user list` | `io.github.enzofagundz.CosmicClipman` owned by the applet pid |
| Capture | `printf 'panel-check-…' \| wl-copy` | row stored within a second |
| Startup does not record the old clipboard | inspected the table after the applet started | only copies made after startup are present |
| Deduplication | copied the same text twice | one row, `accessed_at` moved |
| Sensitive detection | copied an AWS-shaped access key | row stored with `sensitive = 1`, masked in the popup |
| Sensitive purge | waited 30 s | sensitive row deleted by the one-second tick |
| Permissions | `stat` on the data directory and database | directory `700`, database, `-wal` and `-shm` `600` |
| Clicking a row pastes | manual, with the popup open | **pending** — requires a human click; the injection itself is covered by the live test |

## What failed on the way, and why

- The first injection attempt used the `no_keymap` form of the protocol. The
  compositor rejected `modifiers` with the `no_keymap` protocol error: the
  Smithay revision cosmic-comp 1.10 ships (`e3d461a`) only accepts `xkb_v1`
  keymaps and ignores the rest, so `has_keymap` stayed false. The injector now
  compiles a US keymap with libxkbcommon, uploads it, and derives the V keycode
  from it.
- The watcher initially skipped the first `Changed` event to avoid recording the
  clipboard present at startup. When the clipboard was empty at startup the
  first event was `Cleared` instead, and the next real copy was skipped. It now
  ignores exactly the first event of either kind.
