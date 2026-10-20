# 10. Notify when the paste keystroke is rejected

## Status

Accepted.

## Context

The paste path copies the clip, closes the popup and injects `Ctrl+V` through
the virtual-keyboard protocol (ADR-0003). The compositor can reject the
injection when the protocol is missing or the device cannot be created. Before
this decision the popup reported the error in its status line, but the popup is
already closing when the injection runs, so the user saw nothing.

## Decision

- A failed injection keeps the clip on the clipboard and sends a desktop
  notification through `org.freedesktop.Notifications` using the existing D-Bus
  dependency. No subprocess, no shell command.
- The notification uses static localized copy that tells the user to press
  `Ctrl+V`. It never includes clipboard content, a preview, or a hash.
- The notification has no action buttons and expires after five seconds.
- The app cannot detect a terminal that ignores `Ctrl+V`. The notification is
  only for a protocol-level injection error; the terminal limitation stays
  documented.
- Notification errors are ignored: failing to notify must not affect the
  clipboard or the applet.

## Consequences

- A rejected paste is visible even though the popup closed.
- The failure path carries no clip content into the notification service.
- There is no retry: the user pastes manually, which is the documented fallback.
