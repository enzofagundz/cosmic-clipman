# Glossary

Terms this project uses. One entry per term; the definition says what the
term means here, not what it means in general.

- **Applet** — the panel process. A single `cosmic-clipman` binary that owns
  the clipboard watcher, the SQLite store, the popup and the D-Bus interface.
  See `docs/adr/0001-single-applet-process.md`.

- **Applet button** — the icon the panel draws for the applet (`AppModel::view`).
  Clicking it toggles the popup.

- **Clip item** — one row of the history: a text payload with its content hash,
  pin flag, timestamps and sensitive flag. Named `Item` in the code.

- **Content hash** — SHA-256 of the clip payload, stored `UNIQUE`. A second copy
  of the same payload bumps the existing row's `accessed_at` instead of
  inserting a duplicate.

- **History** — the ordered set of clip items, pinned first, then by
  `accessed_at` descending.

- **Incognito mode** — the user-visible pause of recording. While on, the
  watcher still receives clipboard events but nothing is written. The popup,
  search and paste keep working.

- **Panel button** — see *Applet button*.

- **Pinned item** — a clip item the user marked to keep. Pinned items are
  exempt from the retention limit and from the timed purge of sensitive clips.

- **Popup** — the window the applet opens under its panel button. Search field,
  grouped history list, actions and the incognito toggle live here.

- **Paste** — putting a clip item back on the clipboard and injecting `Ctrl+V`
  into the previously focused window. The popup closes first; the keystroke
  waits for the clipboard write and the popup close event. See
  `docs/adr/0003-paste-by-virtual-keyboard.md`.

- **Copy-only** — putting a clip item back on the clipboard without injecting a
  keystroke. The **Copy** action does this.

- **Paste failure notification** — the desktop notification shown when the
  compositor rejects the paste keystroke. It never contains clip content; the
  clip stays on the clipboard for a manual `Ctrl+V`.

- **Shortcut binding** — the entry in COSMIC's custom shortcut configuration
  that runs `cosmic-clipman --toggle`. `Super+V` is the default; the explicit
  install mode writes it. See `docs/adr/0009-shortcut-installed-by-cli.md`.

- **Retention limit** — the maximum number of unpinned items kept; older
  unpinned items beyond it are pruned on insert. Fixed at 500 in v1. See
  `docs/adr/0004-rusqlite-fixed-defaults.md`.

- **Self-copy** — a clipboard change this applet caused by putting an item back
  on the clipboard. It is skipped by the watcher so pasting an item does not
  re-enter the history.

- **Sensitive clip** — a clip item whose text matches a known secret shape. It
  is masked in the popup and deleted after the sensitive timeout (30 s),
  unless pinned. See `docs/adr/0004-rusqlite-fixed-defaults.md`.

- **Injected keymap** — the US XKB keymap compiled with libxkbcommon at paste
  time and uploaded to the compositor with the virtual keyboard, because
  cosmic-comp 1.10 rejects the protocol's `no_keymap` form. See
  `docs/adr/0003-paste-by-virtual-keyboard.md`.

- **Toggle** — `cosmic-clipman --toggle`, the CLI mode that asks the running
  applet over D-Bus to open or close the popup. The COSMIC shortcut the user
  binds points at it. See `docs/adr/0006-popup-toggle-over-dbus.md`.

- **Watcher** — the thread inside the applet that subscribes to clipboard and
  primary-selection changes through `wl-clipboard-rs`
  (`watch::Watcher`). See `docs/adr/0002-capture-with-wl-clipboard-rs.md`.
