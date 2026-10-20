# 2. Capture with data-control through wl-clipboard-rs

## Status

Accepted.

## Context

The original sees copies only through its GNOME Shell extension, and falls back
to `wl-paste --watch` where data-control exists — but GNOME Shell does not offer
data-control, so on GNOME the fallback cannot run. Its own README, ADR and
architecture notes treat the fallback as second class.

On COSMIC we measured the opposite: `cosmic-comp` 1.10 advertises
`ext_data_control_manager_v1` (v1) and `zwlr_data_control_manager_v1` (v2), both
to non-sandboxed clients. `wl-paste` read the live selection on the development
session.

## Decision

Subscribe to selection changes directly in-process with the `wl-clipboard-rs`
crate: `watch::Watcher` on a dedicated thread, `ClipboardType::Regular`, one
event stream. Put an item back on the clipboard with `copy::Options`, whose
serving thread keeps ownership until the selection changes — the same mechanism
`wl-copy` uses, without the extra process.

## Consequences

- No shell extension, no `wl-paste` subprocess, no output parsing, no orphan
  processes; the watcher is a thread that exits with the applet.
- The watcher API is blocking, so it cannot run on the iced executor. Events
  cross into the applet through a futures channel; the same channel shape the
  applet's subscription already uses for config updates.
- Reading the current content is the watcher's own job (`ClipboardEvent`),
  keeping text and images behind the same interface for phase 2.
- Flatpak packaging is excluded by this ADR for data-control reasons alone; the
  virtual-keyboard decision adds a second reason.
