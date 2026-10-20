# Cosmic Clipman

Clipboard history for the COSMIC desktop: a panel applet that records what you
copy, lets you search and pin it, and puts an item back on the clipboard — with
`Super+V` to open it, like `Win+V`.

This is an independent port of [Clipman](https://github.com/MohammedEl-sayedAhmed/clipman)
to Rust and libcosmic. It does not use GNOME Shell, GTK or a Python daemon; the
differences are listed in [`docs/compatibility.md`](docs/compatibility.md).

## Status

v1 is the text core: capture, deduplication, search, pin, delete, clear,
retention, incognito, sensitive-clip detection, and paste by `Ctrl+V`. Images
and snippets are phase 2; see `docs/plan.md`.

## Requirements

- COSMIC desktop with `cosmic-comp` 1.10 or newer (it serves data-control and
  the virtual-keyboard protocol to native clients)
- Rust 1.93 or newer (`rust-toolchain` via rustup recommended)
- Build headers: `wayland-devel` and `libxkbcommon-devel` on Fedora
  (`libwayland-dev`, `libxkbcommon-dev` on Debian/Ubuntu)

## Build

```sh
cargo build --release
just build-release     # same thing, if just is installed
```

## Install

Install into your home and put the binary on `PATH`:

```sh
PREFIX="$HOME/.local" just install
```

or into the system prefix:

```sh
sudo just install
```

Then:

1. Add the applet to the panel: COSMIC Settings → Desktop → Panel, or the
   panel's *Add applets* dialog, and pick **Cosmic Clipman**.
2. Bind the shortcut: COSMIC Settings → Keyboard → Shortcuts → *Custom*,
   command `cosmic-clipman --toggle`, key `Super+V`.

The applet records as soon as the panel starts it. `cosmic-clipman --toggle`
opens or closes the popup from anywhere; without a running applet it exits
non-zero with a message.

## Using it

| Key | Action |
| --- | --- |
| `Super+V` | Open or close the popup |
| Type | Filter the history |
| `↑` / `↓` | Move the selection |
| `Enter` | Put the selected item on the clipboard and paste it |
| `P` | Pin or unpin the selection |
| `Delete` | Delete the selection |
| `Esc` | Close the popup |

Clicking a row pastes it. The **Copy** button only copies, without sending a
keystroke. **Clear** removes unpinned items after a confirmation. **Incognito**
pauses recording; the history and pasting keep working.

Sensitive clips (labelled passwords, vendor tokens, private keys, JWTs,
credential URLs, TOTP seeds, card numbers that pass Luhn) are shown masked and
deleted 30 seconds after they were copied, unless pinned. The detector never
guesses: a bare unlabelled password is not flagged, matching the original.

Paste sends `Ctrl+V` through the compositor's virtual-keyboard protocol with a
US keymap, so it does not depend on your layout. Terminals are the known
exception — see `docs/compatibility.md` for the workaround.

## Data and privacy

- History: `$XDG_DATA_HOME/cosmic-clipman/history.db` (`~/.local/share` when
  unset), directory `0700`, database and sidecars `0600`, WAL mode.
- No network access of any kind, no telemetry, no update check.
- Clipboard contents are never logged and never executed.

## Uninstall

```sh
PREFIX="$HOME/.local" just uninstall   # or: sudo just uninstall
rm -r "${XDG_DATA_HOME:-$HOME/.local/share}/cosmic-clipman"
```

Remove the shortcut in COSMIC Settings and the applet from the panel.

## Tests

```sh
cargo test --all-features     # or: just test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

The corpus tests generate the original's sensitive-clip corpus at test time and
need `python3`; no synthetic secret is committed.

Tests that need the live session (clipboard and paste) are ignored by default:

```sh
cargo test --test live_session -- --ignored --test-threads=1
```

Results from the development session are recorded in
[`docs/tests.md`](docs/tests.md).

## Documentation

- [`docs/architecture.md`](docs/architecture.md) — components and flows
- [`docs/adr/`](docs/adr/) — decisions
- [`docs/compatibility.md`](docs/compatibility.md) — original vs port
- [`docs/rea-investigation.md`](docs/rea-investigation.md) — what was learned
  about the original, with REA
- [`docs/plan.md`](docs/plan.md) — steps and state
- [`GLOSSARY.md`](GLOSSARY.md) — terms

## License

Apache-2.0. Behavior, the sensitive-clip rules and their test corpus are derived
from Clipman, Copyright 2025-2026 Mohammed El-sayed Ahmed, also Apache-2.0. See
[`NOTICE`](NOTICE) and [`LICENSE`](LICENSE). Not affiliated with or endorsed by
the Clipman authors or by System76.
