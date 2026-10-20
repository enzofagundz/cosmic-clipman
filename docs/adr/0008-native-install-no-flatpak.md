# 8. Native install only; no Flatpak in v1

## Status

Accepted.

## Context

cosmic-comp filters both protocols this applet depends on to clients that are
not sandboxed: data-control (the watcher) and the virtual keyboard (the paste
keystroke). A Flatpak applet would have to reach both through xdg-desktop-portal
instead — the clipboard through the portal's selection proxy, the keystroke
through the RemoteDesktop portal (`ashpd` + `reis`, as `cosmic-osk` does).

The COSMIC applet template installs natively with `just install` into a prefix
(`/usr` by default), and the panel discovers applets from the installed
`.desktop` file.

## Decision

v1 installs natively: `just install` (binary, `.desktop`, metainfo, icon) plus
the documented steps to add the applet to the panel and bind the shortcut. No
Flatpak manifest, no portal code paths.

## Consequences

- Both protocols work unmodified, with no consent dialogs and no portal
  dependency.
- A future Flatpak would be a separate ADR and a second code path for clipboard
  and input, so the architecture keeps those two behind small traits
  (`Watcher`, `Injector`) instead of scattering protocol calls through the app.
- The README's uninstall section covers `just uninstall` and where user data
  lives, since there is no package manager to lean on.
