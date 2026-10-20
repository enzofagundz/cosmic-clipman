# Architecture

## Overview

Cosmic Clipman is one process: a COSMIC panel applet that owns the clipboard
watcher, the SQLite history, the popup and a small session-bus interface. That
is the whole architecture, and it is a deliberate departure from the original's
shell-extension-plus-daemon split (see `docs/adr/0001-single-applet-process.md`).

```
                 ┌─────────────────────────────── cosmic-clipman ───┐
                 │                                                   │
  wl-clipboard-rs│  watcher thread ──events──▶ iced subscription     │
  (data-control) │                                    │              │
                 │                                    ▼              │
  wl-clipboard-rs│  copy::Options  ◀── applet model ──▶ popup (iced)  │
  (serves data)  │        ▲                 │       │                 │
                 │        │                 ▼       ▼                 │
  zwp_virtual_   │  inject (Ctrl+V)     SQLite   zbus interface ──────┼──▶ `--toggle`
  keyboard       │                     (rusqlite)                   │
                 └───────────────────────────────────────────────────┘
```

## Components

| Module | Responsibility |
| --- | --- |
| `app` | The libcosmic `Application`: model, messages, popup and panel views, subscriptions |
| `watcher` | Clipboard subscription thread, MIME preference, payload cap, repeat and self-copy rules; clipboard writes |
| `inject` | `zwp_virtual_keyboard_manager_v1` client that presses `Ctrl+V` |
| `paste` | Pending-paste gate: clipboard-ready plus popup-closed, with a deadline |
| `notify` | Desktop notification for a rejected paste keystroke |
| `shortcut` | Installs and removes the `Super+V` binding in cosmic-config |
| `store` | SQLite schema, migrations, deduplication, pruning, sensitive purge, permissions |
| `sensitive` | Secret-shape detector ported from the original, with its corpus |
| `dbus` | `io.github.enzofagundz.CosmicClipman` interface (`Toggle`/`Show`/`Hide`) and the `--toggle` client |
| `domain` | `Item`, preview elision, search matching, date grouping, fixed defaults |

## Capture flow

1. `watcher::spawn` starts a detached thread with its own Wayland connection and
   a `wl-clipboard-rs` `watch::Watcher` on the regular clipboard. Its first event
   reports whatever was already on the clipboard and is used only to seed the
   watcher: the history begins with the first copy made while the applet runs.
2. On a change, the thread picks the first MIME type from the original's
   preference list (`text/plain;charset=utf-8`, `UTF8_STRING`, `text/plain`,
   `STRING`), reads the offer with a 10 MiB cap, and drops the event when it is
   empty, the same content within 100 ms, a self-copy younger than 2 s, or while
   incognito is on.
3. Accepted text crosses into the applet through an unbounded futures channel;
   the applet runs `sensitive::is_sensitive`, inserts through `Store::insert_text`
   (SHA-256 dedup, bump on duplicate), and reloads the list.

## Paste flow

1. The user presses Enter, clicks a row, or uses the Copy action.
2. The applet touches the row's `accessed_at`, arms the self-copy mark and
   copies the text with `copy::Options`; the crate keeps a thread serving the
   selection so the clipboard stays populated after the applet moves on.
3. For a paste, the popup closes and a pending-paste gate waits for two
   conditions: the clipboard write finished and the popup window reported
   closed. A safety deadline supplies the close condition if the event is
   missing, and a new activation replaces the pending one.
4. When both conditions hold, the applet waits a 400 ms focus-return grace
   (cosmic-comp 1.10 took 117–311 ms to return keyboard focus to the target in
   live measurements), then the injector compiles a US keymap with
   libxkbcommon, opens its own Wayland connection, binds the virtual-keyboard
   manager, uploads that keymap (cosmic-comp 1.10 accepts only `xkb_v1`, not
   `no_keymap`), presses Ctrl (XKB mask 4) and the evdev V keycode (47),
   releases both, and destroys the device. The keymap supplies modifier state;
   the compositor forwards the evdev key number to the focused client.
5. An injection failure leaves the clip on the clipboard and sends a desktop
   notification through `org.freedesktop.Notifications`; it never includes clip
   content. A terminal that ignores `Ctrl+V` is not detectable.

## Storage

One SQLite database at `$XDG_DATA_HOME/cosmic-clipman/history.db`
(`~/.local/share` when unset), WAL mode, `PRAGMA user_version = 1`. The `entries`
table mirrors the original's columns minus images; `content_hash` is `UNIQUE`.
Indexes: `accessed_at DESC`, `content_hash`. The directory is `0700`, the
database and its sidecars `0600`. Retention (500 unpinned) is enforced inside
the insert transaction; the sensitive purge runs on a one-second tick.

## Toggle and singleton behaviour

The applet serves `io.github.enzofagundz.CosmicClipman` at
`/io/github/enzofagundz/CosmicClipman` with `Toggle`, `Show` and `Hide`. The
`--toggle` mode of the same binary connects and calls `Toggle`. A user
`just install` runs `cosmic-clipman --install-shortcut`, which writes the
`Super+V` binding to COSMIC's custom shortcut configuration; a root install
prints the command instead. The bus name is also the singleton guard: a second
applet instance fails to own the name, reports it, and runs without capture so
two watchers never fight over the clipboard.

## Testing surface

Everything that decides what happens to a clip — the sensitive rules, the store,
the grouping and preview rules, the repeat and self-copy gates, the pending-paste
gate and the shortcut parser — is a plain module with no D-Bus, compositor or
window dependency, and is unit-tested. The shortcut CLI is tested end to end
against a temporary cosmic-config home. The applet wiring itself is validated on
a live COSMIC session with the ignored tests in `tests/live_session.rs` plus a
manual pass documented in `docs/tests.md`.
