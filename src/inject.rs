// SPDX-License-Identifier: Apache-2.0

//! Paste keystroke injection through `zwp_virtual_keyboard_manager_v1`.
//!
//! cosmic-comp implements the protocol for clients that are not sandboxed, so a
//! native applet can press `Ctrl+V` without root, a helper daemon or a portal
//! dialog.
//!
//! cosmic-comp 1.10 only accepts a real XKB keymap (its Smithay revision
//! refuses the `no_keymap` form with a `no_keymap` protocol error), so a US
//! keymap is compiled with libxkbcommon and uploaded; the compositor applies it
//! to the focused client for the injected keystroke and restores the system
//! keymap afterwards. Keycodes are XKB keycodes of that uploaded keymap.

use std::io::Write;
use std::os::fd::AsFd;

use tempfile::NamedTempFile;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_keyboard::{KeyState, KeymapFormat};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{ConnectError, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1;
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1;
use xkbcommon::xkb;

/// XKB modifier mask bit for Control.
pub const MOD_CONTROL: u32 = 1 << 2;
/// XKB keycode of the physical V key in the uploaded US keymap (evdev 47 + 8).
pub const KEY_V: u32 = 55;

struct State;

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: wayland_client::protocol::wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardManagerV1,
        _: <ZwpVirtualKeyboardManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardV1,
        _: <ZwpVirtualKeyboardV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

#[derive(Debug)]
pub enum InjectError {
    Connect(ConnectError),
    Globals(wayland_client::globals::GlobalError),
    MissingProtocol,
    Keymap,
    Connection(String),
    Io(std::io::Error),
}

impl std::fmt::Display for InjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InjectError::Connect(err) => write!(f, "no Wayland session: {err}"),
            InjectError::Globals(err) => write!(f, "Wayland registry failed: {err}"),
            InjectError::MissingProtocol => {
                write!(f, "the compositor does not offer virtual-keyboard")
            }
            InjectError::Keymap => write!(f, "could not compile the US keymap"),
            InjectError::Connection(err) => write!(f, "Wayland connection failed: {err}"),
            InjectError::Io(err) => write!(f, "injection failed: {err}"),
        }
    }
}

impl std::error::Error for InjectError {}

impl From<std::io::Error> for InjectError {
    fn from(err: std::io::Error) -> Self {
        InjectError::Io(err)
    }
}

/// An XKB keymap for a plain US layout, its file, and the V keycode it assigns.
struct Keymap {
    file: NamedTempFile,
    size: u32,
}

fn us_keymap() -> Result<Keymap, InjectError> {
    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let keymap = xkb::Keymap::new_from_names(
        &context,
        "",
        "",
        "us",
        "",
        None,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
    .ok_or(InjectError::Keymap)?;
    let text = keymap.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);
    let mut file = NamedTempFile::new()?;
    file.write_all(text.as_bytes())?;
    file.flush()?;
    Ok(Keymap {
        file,
        size: text.len() as u32,
    })
}

/// Press and release `Ctrl+V`, leaving no modifier held.
pub fn press_ctrl_v() -> Result<(), InjectError> {
    let keymap = us_keymap()?;

    let connection = Connection::connect_to_env().map_err(InjectError::Connect)?;
    let (globals, mut queue) =
        registry_queue_init::<State>(&connection).map_err(InjectError::Globals)?;
    let queue_handle = queue.handle();

    let manager = globals
        .bind::<ZwpVirtualKeyboardManagerV1, _, _>(&queue_handle, 1..=1, ())
        .map_err(|_| InjectError::MissingProtocol)?;
    let seat = globals
        .bind::<WlSeat, _, _>(&queue_handle, 1..=1, ())
        .map_err(|_| InjectError::MissingProtocol)?;

    let keyboard = manager.create_virtual_keyboard(&seat, &queue_handle, ());
    keyboard.keymap(
        KeymapFormat::XkbV1 as u32,
        keymap.file.as_file().as_fd(),
        keymap.size,
    );

    keyboard.modifiers(MOD_CONTROL, 0, 0, 0);
    keyboard.key(0, KEY_V, KeyState::Pressed as u32);
    keyboard.key(0, KEY_V, KeyState::Released as u32);
    keyboard.modifiers(0, 0, 0, 0);

    connection
        .flush()
        .map_err(|err| InjectError::Connection(err.to_string()))?;
    queue
        .roundtrip(&mut State)
        .map_err(|err| InjectError::Connection(err.to_string()))?;
    keyboard.destroy();
    connection
        .flush()
        .map_err(|err| InjectError::Connection(err.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uploaded_keymap_puts_v_on_55() {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &context,
            "",
            "",
            "us",
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .expect("libxkbcommon compiles a US keymap");
        let state = xkb::State::new(&keymap);
        let v = xkb::keysym_from_name("v", xkb::KEYSYM_NO_FLAGS);
        assert_eq!(state.key_get_one_sym(xkb::Keycode::new(KEY_V)), v);
        // XKB modifier masks: shift 1, lock 2, control 4.
        assert_eq!(MOD_CONTROL, 4);
        assert!(us_keymap().expect("keymap file").size > 0);
    }
}
