# 9. Install the popup shortcut from an explicit CLI mode

## Status

Accepted. Supersedes
[ADR-0006](0006-popup-toggle-over-dbus.md) on how the shortcut reaches COSMIC.

## Context

ADR-0006 left the `Super+V` binding to the user because an applet cannot
register a global shortcut at runtime. That decision kept the applet out of the
user's settings, but a fresh install did not behave like `Win+V`: the user had
to find COSMIC Settings, create a custom shortcut, and type the command.

COSMIC stores custom shortcuts in cosmic-config under
`com.system76.CosmicSettings.Shortcuts`, version 1, in the `custom` key. The
settings daemon reads that key and watches it for changes. The format is a RON
map from a binding to an action; a command binding is a `Spawn("...")` action.

## Decision

- Add explicit `--install-shortcut` and `--uninstall-shortcut` CLI modes to the
  same binary. The applet still never writes shortcuts while it runs.
- The install mode writes `Super+V` by default, or a binding passed with
  `--shortcut`, through the cosmic-config API already available from libcosmic.
- The binding carries a description naming Cosmic Clipman and runs
  `cosmic-clipman --toggle`.
- Installation is idempotent. It preserves every unrelated entry. If the
  requested trigger already belongs to another command, it fails and prints the
  current owner instead of overwriting it.
- Uninstall removes only entries whose action is this applet's exact toggle
  command, even if the user moved it to another key.
- `just install` runs the install mode after placing the binary. A root install
  skips it and prints the command, because root cannot reliably know the
  invoking user's config directory. `just uninstall` removes the binding before
  deleting the binary.

## Consequences

- A normal user install gets a working shortcut without opening Settings.
- The user's shortcut table stays authoritative: the applet does not re-add a
  binding the user removed, and an install never overwrites another command.
- The install recipe and the shortcut module are coupled to COSMIC's config
  format. If the format changes, this is the one place to update.
- Removing the binary without using the recipe can leave a dead shortcut; that
  is visible in Settings and easy to remove.
