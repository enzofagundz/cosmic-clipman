# 3. Paste by native virtual-keyboard injection

## Status

Accepted.

## Context

Pasting an item needs two steps: putting the item on the clipboard (ADR 2) and
sending the paste keystroke to the focused window. In the original the second
step happens inside the GNOME Shell extension: it creates a Clutter virtual
keyboard device and calls `notify_keyval` with the recipe keys, choosing
`Ctrl+V`, or `Ctrl+Shift+V` when the focused window's WM class is a terminal.

On COSMIC there is no shell to type for us. `cosmic-comp` 1.10 does implement
`zwp_virtual_keyboard_manager_v1` (Smithay `VirtualKeyboardManagerState`, state
initialisation in `src/state.rs`), but only for clients that are not sandboxed.
Alternative routes exist: `libei` through the RemoteDesktop portal (what
`cosmic-osk` uses) or `ydotool`/uinput (needs root or a udev rule and a group
membership).

## Decision

Bind `zwp_virtual_keyboard_manager_v1` ourselves, in the applet process, using
the protocol bindings from `wayland-protocols-misc`, with a second Wayland
connection of our own. Release the modifier in a `finally`-equivalent path so a
modifier is never left held.

cosmic-comp 1.10's Smithay revision (`e3d461a`) only accepts `xkb_v1` keymaps:
a `no_keymap` request is ignored and the following `modifiers` request is
rejected with the protocol's `no_keymap` error. So compile a US keymap with
libxkbcommon and upload it. The compositor uses that keymap for modifier state,
but forwards the `key` argument to the focused client unchanged as a
`wl_keyboard.key`, which carries an **evdev** keycode. Send evdev keycode 47 for
V, not the XKB keycode 55; sending the XKB number produces a different key in
the focused client and Ctrl+V never arrives. The uploaded keymap keeps the
keystroke independent of the user's layout.

Send `Ctrl+V` unconditionally in v1. Do not attempt terminal detection.

## Consequences

- No root, no udev rule, no helper daemon, no consent dialog, no extra
  process; the dependency is a protocol the compositor already speaks.
- The injection is validated on the real session (acceptance requires it), with
  a manual check in a text editor and a terminal.
- Nothing works from a sandbox: Flatpak packaging stays out (ADR 8).
- A US keymap is compiled at injection time; libxkbcommon is already a
  dependency of the desktop, and the compilation is unit-tested to put `v` on
  XKB keycode 55, which is evdev keycode 47 plus XKB's offset of 8.
- Paste failures keep the clip on the clipboard and notify the user; see
  [ADR-0010](0010-paste-failure-notification.md).
- The focus-return grace is a timing constant, not a compositor signal. If a
  future cosmic-comp returns focus faster, the grace can be shortened; injecting
  too early drops the keystroke silently rather than failing the injection.
- Terminals stay a known difference from the original and are recorded in
  `docs/compatibility.md`, with the two mechanisms that could lift the
  limitation (toplevel-info focus tracking, or a manual alternate recipe).
