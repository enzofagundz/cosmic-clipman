# REA investigation of the original Clipman

What was investigated, with which tool, and what came out. Confirmed findings
are separated from hypotheses, and limitations are stated where they exist.

## Method

The reference checkout is `/tmp/opencode/refs/clipman` at commit
`36784cc7b710a9d17828cb7c52d272ed45f66304` (2026-10-12). Release artifacts of
v1.3.0 were downloaded to `/tmp/opencode/refs/artifacts` and inspected with the
REA MCP server (the `rea` CLI is not installed on this machine; only the MCP
server is used, and no command is invented for it).

REA operations actually used:

| Operation | Target | Result |
| --- | --- | --- |
| `open_binary` | `clipman-extension-v1.3.0.zip` | format `zip`, kind `archive`, sha256 `1ab68eb6…` |
| `inspect_artifact` | same | inventory of the archive graph, `complete-within-substeps` |
| `extract_artifact` | same | 2 files materialized, `containment_verified: true` |
| `analyze_javascript_application` | extracted extension directory | 2 files, 2390 AST nodes, 13 findings, 17-node module graph, 0 parse failures |
| `open_binary` + `inspect_artifact` + `extract_artifact` | `clipman_clipboard-1.3.0-py3-none-any.whl` | format `zip`, 17 module files + `dist-info`, extracted |
| `open_binary` | `clipman_1.3.0_all.deb` | **refused**: `unsupported binary format` — REA has no deb/ar reader |
| `inspect_keyed_archive` | extension zip | **refused**: requires an active plist or `.app` bundle |

### Artifact digests

| Artifact | sha256 |
| --- | --- |
| `clipman-extension-v1.3.0.zip` | `1ab68eb622bdac76532b32dae5d4a46d7f1f21a2d2c2f9844b789e2e80582054` |
| `clipman_clipboard-1.3.0-py3-none-any.whl` | `0fdfbbab13b44eecb3fa8319988f0972417cf64fac47736ac832a9f90960fa18` |
| `clipman_1.3.0_all.deb` | `f461f2aa554651e1f26f3605c21095772527992383a2dd7a6cbf42f4007df830` |
| `extension.js` inside all three copies | `602de741ae6da1bfa7163fef0bf3367a0ba2a4886707e3c65acdf1cfeab8f6b9` |

The shipped `extension.js` in the release zip, in the `.deb` payload, and in the
repository at the reference commit are byte-identical. **Confirmed:** for
v1.3.0 the distribution artifacts carry the tagged source, so reading the
repository does not diverge from what users run.

## What REA established

- The GNOME Shell extension is a single ES module plus `metadata.json`
  (uuid `clipman@clipman.com`, `shell-version` 45–51) — no build step, nothing
  compiled, nothing minified. The JS analyzer's module graph resolved only
  module-level imports (14 module nodes, 12 `imports` edges, 1 `exposes`); the
  `resource:///org/gnome/shell/...` imports carry no symbol labels, which is
  the analyzer's documented limit for these modules.
- The wheel is pure Python: 17 `clipman/*.py` files plus `dist-info`; no
  extension modules, no bundled binaries. Same code as the repository.
- The `.deb` payload, read with `ar` + `tar` after REA refused the format:
  `/usr/bin/clipman` (launcher), `/usr/lib/clipman/clipman.py`, the
  `clipman/` package, `/usr/lib/clipman/extension/`, a `.desktop`, an
  AppStream metainfo file, an icon and documentation. Its control metadata
  declares the system dependencies: `python3-gi`, `python3-dbus`,
  `gir1.2-gtk-4.0`, `gir1.2-adw-1`, `libadwaita-1-0`, `wl-clipboard`.
- **Confirmed:** every GNOME-specific capability is in the extension, and the
  daemon talks to the shell through one D-Bus interface.

## Behaviour ported into this project

Confirmed from source, with locations:

- **Capture on GNOME** (`extension/extension.js:114`, `:488`, `:494`) — the
  extension subscribes to `Meta.Selection` `owner-changed`, reads the new owner
  with the MIME preference list `text/plain;charset=utf-8`, `UTF8_STRING`,
  `text/plain`, `STRING` (`:25`), caps a read at 10 MiB (`:17`) and abandons it
  after 5 s (`:21`). Nothing is read while incognito is on.
- **Daemon D-Bus contract** (`ARCHITECTURE.md`, `docs/dbus-api.md`,
  `clipman/dbus_service.py`) — `com.clipman.Daemon` at `/com/clipman/Daemon`
  with `Toggle()`, `Show()`, `Hide()`, `Quit()` and `NewEntry(ss)`. This is the
  model for our popup toggle (ADR 6).
- **Paste** (`extension/extension.js:304`, `:360`–`:400`) — a Clutter virtual
  keyboard device is created in the shell process, and `_dispatchKeystroke`
  presses the recipe's modifiers, then the key, then releases modifiers in a
  `finally`. Recipes are `ctrl-v`, `ctrl-shift-v`, `shift-insert`; mode `auto`
  picks `ctrl-shift-v` when the focused window's WM class is a terminal. On
  COSMIC the injection has no shell to live in, which is why ADR 3 binds the
  Wayland virtual-keyboard protocol instead, and why terminal detection is a
  documented difference (ADR 3, `docs/compatibility.md`).
- **Store** (`clipman/database.py`) — one SQLite database in WAL mode;
  `entries` with `content_hash TEXT NOT NULL UNIQUE`; deduplication bumps
  `accessed_at` instead of inserting (`add_entry`); prune keeps the newest
  unpinned entries up to the limit and never touches pinned ones
  (`enforce_max_entries`); sensitive rows are deleted once older than the
  timeout unless pinned (`delete_expired_sensitive`); data directory `0700`,
  database and sidecars `0600`.
- **Sensitive detection** (`clipman/sensitive.py`) — matches known secret
  shapes only: vendor token prefixes, `sk-` bodies with a random-looking run,
  private key headers, credential URLs, JOSE-verified JWTs, labelled values
  filtered by `_plausible_value`, authorization headers, inline CLI passwords,
  TOTP seeds, and card numbers that pass both Luhn and an issuer-prefix check.
  Public keys are deliberately not flagged; a bare unlabelled password is
  deliberately not detected. Scan is capped at 64 KiB. The stated reason matters
  for the port: a false positive deletes user data, so the detector never
  guesses from randomness.
- **Event hygiene** (`clipman/clipboard_monitor.py:14-17`) — same-content
  events within 100 ms are dropped; a self-copy (an item we put back on the
  clipboard) is ignored for 2 s, so pasting does not re-enter the history;
  payload cap 10 MiB.
- **Fallback watcher** (`clipman/clipboard_monitor.py:20-160`) — where
  data-control exists, `wl-paste --watch` plus a sentinel line, restarted at
  most five times before giving up. Superseded in this port by the native
  watcher (ADR 2).

## Environment evidence on this machine

Measured, not assumed:

- COSMIC session running (`XDG_CURRENT_DESKTOP=COSMIC`, Wayland), Fedora 44,
  `cosmic-comp` 1.10 present.
- `wl-paste 2.2.1` listed the live selection's MIME types, i.e. data-control is
  served to a normal client here.
- `cosmic-comp` implements both data-control variants
  (`src/wayland/handlers/data_control/{ext,wlr}.rs`) and
  `zwp_virtual_keyboard_manager_v1` (`src/state.rs:712`,
  `src/wayland/handlers/virtual_keyboard.rs`), the latter only for
  non-sandboxed clients.
- No clipboard history is built into COSMIC and no first-party clipboard applet
  exists, so nothing conflicts with this applet.

## Hypotheses carried into implementation

Stated as hypotheses because they were not executed end-to-end before the port:

- A `wl-clipboard-rs` `Watcher` thread inside a libcosmic applet subscribes and
  streams events cleanly on COSMIC. (Validated by the running applet; see
  `docs/tests.md`.)
- `Ctrl+V` keycodes through the virtual keyboard are accepted without a
  sandbox, using the no-keymap form or an uploaded keymap. (Validated by the
  paste test in `docs/tests.md`.)
- Terminal-aware paste could later be restored by tracking the focused toplevel
  through COSMIC's toplevel-info protocols; not attempted in v1.

## Limitations of this investigation

- REA cannot read `deb`, `rpm` or `ar` containers; those inventories used
  `ar`/`tar`/`rpm -qlp` instead, and are marked as such.
- The JS analyzer's module graph does not resolve GNOME Shell's `resource://`
  imports to symbols, so the extension's method bodies were read as source.
- Nothing was executed at runtime: GNOME Shell is absent here, so the
  extension's behaviour is read from source, and the artifact digests prove
  that the source matches what shipped.
