# 1. Single applet process owns capture, store and popup

## Status

Accepted.

## Context

The original Clipman runs two processes: a GNOME Shell extension that observes
clipboard changes inside the shell process, and a Python/GTK daemon that stores
history and shows the popup. The split exists because a GNOME Shell extension
cannot keep a GTK window, and the daemon cannot see `Meta.Selection` events.

On COSMIC neither constraint applies. `cosmic-comp` implements the
data-control protocol, so an ordinary client can subscribe to selection
changes, and a panel applet is a long-lived process of its own that the panel
starts and keeps running.

## Decision

One binary, `cosmic-clipman`, running as a COSMIC panel applet. Its process owns:

- the clipboard watcher (`wl-clipboard-rs`), in a dedicated thread;
- the SQLite store, accessed through one connection on the applet's executor
  context;
- the popup window;
- a session-bus interface used by the `--toggle` CLI mode.

## Consequences

- No IPC between a store and a UI; the model in the applet holds the history.
- If the panel is not running, nothing records. Accepted: the panel is the
  applet's home and restarts with the session.
- A second instance (for example the applet added to a second panel) must not
  run a second watcher. The D-Bus name doubles as the singleton guard: the
  instance that owns `io.github.enzofagundz.CosmicClipman` captures; another
  instance runs without capture and still shows its own popup reading the same
  database.
- Testing the store and the domain rules no longer needs a compositor or D-Bus,
  because they live in plain modules that the applet merely calls.
