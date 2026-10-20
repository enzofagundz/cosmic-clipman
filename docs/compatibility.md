# Compatibility with the original Clipman

What the port keeps, what it changes, and what it leaves out. A row is
"confirmed" only where a test or a live-session check exercised it; the
evidence is in `docs/tests.md`.

## Kept

| Original behaviour | Here | Notes |
| --- | --- | --- |
| Text capture from the clipboard | Yes | Native data-control, no shell extension |
| MIME preference order | Yes | `text/plain;charset=utf-8`, `UTF8_STRING`, `text/plain`, `STRING` |
| 10 MiB payload cap | Yes | Dropped silently, like the original |
| SHA-256 deduplication | Yes | Re-copying bumps `accessed_at` instead of duplicating |
| Repeat suppression | Yes | 100 ms window per content |
| Self-copy suppression | Yes | 2 s, so pasting does not re-enter the history |
| Search | Yes | Case-insensitive substring over text |
| Pin items | Yes | Pinned items first, exempt from retention and sensitive purge |
| Delete one item | Yes | |
| Clear unpinned items | Yes | With confirmation |
| Retention limit | Yes, fixed at 500 | Original: 50–5000, configurable |
| Date grouping | Yes | Pinned, Today, Yesterday, Earlier this week, Older |
| Incognito mode | Yes | Pauses recording only |
| Sensitive detection | Yes | Same rules and corpus; masked display; 30 s purge |
| Restrictive permissions | Yes | Directory `0700`, database and sidecars `0600` |
| Parameterised SQL | Yes | |
| No shell execution | Yes | No subprocesses at all |
| Event-driven capture | Yes | One 1 s tick, used only for the sensitive purge |
| Only copies made while the applet runs are recorded | Yes | The watcher's first event (the pre-existing clipboard) is ignored |
| Popup from a shortcut | Yes | `cosmic-clipman --toggle`; `just install` registers `Super+V`. Terminals still need `Ctrl+Shift+V` for the paste itself |
| Clicking a clip pastes it | Yes | The popup closes, focus returns, then `Ctrl+V` is injected |

## Changed

| Original | Here | Why |
| --- | --- | --- |
| GNOME Shell extension reads `Meta.Selection` | Native `wl-clipboard-rs` watcher | COSMIC serves data-control; no shell to extend (`docs/adr/0002-capture-with-wl-clipboard-rs.md`) |
| Daemon + GTK window | One panel applet | The applet process is persistent (`docs/adr/0001-single-applet-process.md`) |
| Extension types `Ctrl+V` via Clutter | Applet injects via `zwp_virtual_keyboard_manager_v1` with an uploaded US keymap | No shell in COSMIC (`docs/adr/0003-paste-by-virtual-keyboard.md`) |
| Paste recipe `auto` picks `Ctrl+Shift+V` in terminals | Always `Ctrl+V` | Focus tracking inside the shell has no COSMIC equivalent; terminals need the manual recipe below |
| Retention and privacy values in settings | Fixed 500 items, 30 s purge | No settings UI in v1 (`docs/adr/0004-rusqlite-fixed-defaults.md`) |
| Shortcut installed by `install.sh` into gsettings | `just install` runs `cosmic-clipman --install-shortcut` | COSMIC stores custom shortcuts in cosmic-config; the explicit CLI mode preserves the user's other entries and refuses conflicts |
| Catppuccin themes, font size, opacity | Follows the COSMIC theme | Appearance settings are out of v1 scope |
| Autostart through a systemd user service | Panel starts the applet | COSMIC applets are panel processes |
| Backup and restore in settings | Not implemented | Out of v1 scope; the database is a plain SQLite file |
| In-app update check | Not implemented | No network access at all |
| Search matches sensitive text | Same | Same known issue as the original |
| System clipboard is not cleared when a sensitive clip expires | Same | Same known limitation as the original |

## Not implemented (phase 2)

- Image clips (`image/*`, hash-named files `0600`). The schema keeps a
  `content_type` column so this is additive.
- Snippets with `${date}`, `${time}` and `${clipboard}` expansion.

## Not implemented (out of scope)

- Filter tabs, link/code badges, per-item character counts.
- Typo-tolerant search, image text search.
- Backup/restore UI, update notifications.
- i18n beyond the template's English strings (the Fluent scaffolding is in
  place).
- Flatpak packaging: data-control and the virtual keyboard are not offered to
  sandboxed clients (`docs/adr/0008-native-install-no-flatpak.md`).

## Known limitations

- **Terminal paste.** `Ctrl+V` in a terminal does not paste on most terminal
  emulators. Workarounds today: use the **Copy** action and `Ctrl+Shift+V`
  manually. A future version can either track the focused toplevel through
  COSMIC's toplevel protocols or expose an alternate paste recipe.
- **Multiple applet instances.** The instance that owns the D-Bus name records;
  a second one shows its own popup and writes to the same database but does not
  capture.
- **XWayland copies.** The MIME fallback list includes the X11 types, and
  data-control carries XWayland offers, but this path has not been exercised
  with a real XWayland application in this port.
