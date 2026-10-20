# 6. Popup toggle over D-Bus, shortcut bound by the user

## Status

Superseded by [ADR-0009](0009-shortcut-installed-by-cli.md). The D-Bus toggle
interface and the single code path remain; the shortcut is now installed by an
explicit CLI mode instead of being left to the user.

## Context

The original's Super+V shortcut is a GNOME custom keybinding that runs
`launcher.sh`, which calls `Toggle()` on the daemon's D-Bus name; the daemon
shows the window. On COSMIC, custom shortcuts are configured in COSMIC Settings
and run a command of the user's choosing — there is no way for an applet to
register a global keybinding itself.

A panel applet can only open its own popup from within its process, and the
process is started by the panel, so the shortcut must reach the applet over a
channel. libcosmic has no built-in activation for this; pop-os' own
`cosmic-applet-status-area` shows the pattern: serve a zbus interface from a
subscription, and offer a second mode of the same binary that acts as the
client.

## Decision

- The applet serves `io.github.enzofagundz.CosmicClipman` on the session bus at
  `/io/github/enzofagundz/CosmicClipman`, with `Toggle()`, `Show()` and
  `Hide()`; the same interface carries the singleton signal from ADR 1.
- `cosmic-clipman --toggle` connects, calls `Toggle()` and exits. If no applet
  is running it exits non-zero with a one-line message on stderr.
- The README documents the manual step: COSMIC Settings → Shortcuts → Custom →
  command `cosmic-clipman --toggle`, key `Super+V`. Nothing is written into
  cosmic-config by an installer.

## Consequences

- The popup can be opened from a shortcut, a script or the panel button, with
  one code path.
- The user keeps control of their shortcut table; uninstalling cannot leave a
  stale binding behind.
- Popup visibility is app state (the applet tracks its popup `Id`), so Toggle
  means: open when closed, close when open.
