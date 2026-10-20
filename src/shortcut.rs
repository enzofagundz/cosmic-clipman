// SPDX-License-Identifier: Apache-2.0

//! Installation and removal of the `Super+V` shortcut in COSMIC's custom
//! shortcut configuration.
//!
//! COSMIC does not let an applet register a global shortcut at runtime. The
//! explicit CLI modes write one binding through the same cosmic-config file the
//! settings daemon reads, preserving every unrelated entry.

use std::collections::BTreeMap;

use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};
use ron::value::RawValue;
use serde::{Deserialize, Serialize};

pub const CONFIG_ID: &str = "com.system76.CosmicSettings.Shortcuts";
pub const CONFIG_VERSION: u64 = 1;
pub const CONFIG_KEY: &str = "custom";
pub const TOGGLE_COMMAND: &str = "cosmic-clipman --toggle";
pub const DEFAULT_BINDING: &str = "Super+V";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
enum Modifier {
    Super,
    Ctrl,
    Alt,
    Shift,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    #[serde(default)]
    modifiers: Vec<Modifier>,
    #[serde(
        default,
        deserialize_with = "deserialize_key",
        serialize_with = "serialize_key",
        skip_serializing_if = "Option::is_none"
    )]
    key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keycode: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

impl Binding {
    fn same_trigger(&self, other: &Self) -> bool {
        let mut left = self.modifiers.clone();
        let mut right = other.modifiers.clone();
        left.sort();
        right.sort();
        left == right && self.key == other.key
    }
}

fn deserialize_key<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let name = String::deserialize(deserializer)?;
    Ok(if name == "None" { None } else { Some(name) })
}

fn serialize_key<S>(key: &Option<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(key.as_deref().unwrap_or("None"))
}

type ShortcutMap = BTreeMap<Binding, Box<RawValue>>;

#[derive(Debug, Deserialize, Serialize)]
enum SpawnOnly {
    Spawn(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallOutcome {
    Installed,
    AlreadyInstalled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveOutcome {
    Removed,
    Absent,
}

/// Write `binding` (for example `Super+V`) as the popup toggle shortcut.
pub fn install(binding: &str) -> Result<InstallOutcome, String> {
    let wanted = parse_binding(binding)?;
    let config = config()?;
    let mut map = load(&config)?;

    if let Some((existing, action)) = map.iter().find(|(key, _)| key.same_trigger(&wanted)) {
        if is_toggle_action(action) {
            return Ok(InstallOutcome::AlreadyInstalled);
        }
        return Err(format!(
            "{} is already bound to {}",
            existing.describe(),
            action.get_ron()
        ));
    }

    map.insert(wanted, toggle_action()?);
    save(&config, &map)?;
    Ok(InstallOutcome::Installed)
}

/// Remove every binding that runs this applet's toggle command.
pub fn remove() -> Result<RemoveOutcome, String> {
    let config = config()?;
    let mut map = load(&config)?;
    let before = map.len();
    map.retain(|_, action| !is_toggle_action(action));
    if map.len() == before {
        return Ok(RemoveOutcome::Absent);
    }
    save(&config, &map)?;
    Ok(RemoveOutcome::Removed)
}

fn config() -> Result<Config, String> {
    Config::new(CONFIG_ID, CONFIG_VERSION)
        .map_err(|err| format!("cannot open the shortcut configuration: {err}"))
}

fn load(config: &Config) -> Result<ShortcutMap, String> {
    match config.get_local::<ShortcutMap>(CONFIG_KEY) {
        Ok(map) => Ok(map),
        Err(cosmic::cosmic_config::Error::NotFound) => Ok(ShortcutMap::new()),
        Err(err) => Err(format!("cannot read custom shortcuts: {err}")),
    }
}

fn save(config: &Config, map: &ShortcutMap) -> Result<(), String> {
    config
        .set(CONFIG_KEY, map)
        .map_err(|err| format!("cannot write custom shortcuts: {err}"))
}

fn toggle_action() -> Result<Box<RawValue>, String> {
    let text = ron::to_string(&SpawnOnly::Spawn(TOGGLE_COMMAND.to_owned()))
        .map_err(|err| format!("cannot encode the shortcut action: {err}"))?;
    ron::from_str(&text).map_err(|err| format!("cannot encode the shortcut action: {err}"))
}

fn is_toggle_action(action: &RawValue) -> bool {
    matches!(
        ron::from_str::<SpawnOnly>(action.get_ron()),
        Ok(SpawnOnly::Spawn(command)) if command == TOGGLE_COMMAND
    )
}

impl Binding {
    fn describe(&self) -> String {
        let key = self.key.as_deref().unwrap_or("(keycode)");
        let mut parts: Vec<&str> = self
            .modifiers
            .iter()
            .map(|modifier| match modifier {
                Modifier::Super => "Super",
                Modifier::Ctrl => "Ctrl",
                Modifier::Alt => "Alt",
                Modifier::Shift => "Shift",
            })
            .collect();
        parts.push(key);
        parts.join("+")
    }
}

fn parse_binding(value: &str) -> Result<Binding, String> {
    let mut super_modifier = false;
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut key: Option<String> = None;

    for token in value.split('+').map(str::trim) {
        if token.is_empty() {
            continue;
        }
        match token.to_ascii_lowercase().as_str() {
            "super" => super_modifier = true,
            "ctrl" | "control" => ctrl = true,
            "alt" => alt = true,
            "shift" => shift = true,
            _ => {
                if key.is_some() {
                    return Err(format!(
                        "only one key is allowed in a shortcut, got {value}"
                    ));
                }
                key = Some(normalize_key(token)?);
            }
        }
    }

    let Some(key) = key else {
        return Err(format!("no key in shortcut {value}"));
    };
    if !(super_modifier || ctrl || alt || shift) {
        return Err(format!("shortcut {value} needs at least one modifier"));
    }

    let mut modifiers = Vec::new();
    if super_modifier {
        modifiers.push(Modifier::Super);
    }
    if ctrl {
        modifiers.push(Modifier::Ctrl);
    }
    if alt {
        modifiers.push(Modifier::Alt);
    }
    if shift {
        modifiers.push(Modifier::Shift);
    }

    Ok(Binding {
        modifiers,
        key: Some(key),
        keycode: None,
        description: Some("Cosmic Clipman".to_owned()),
    })
}

fn normalize_key(token: &str) -> Result<String, String> {
    let name = if token.chars().count() == 1 {
        token.to_lowercase()
    } else {
        token.to_owned()
    };
    let keysym = xkbcommon::xkb::keysym_from_name(&name, xkbcommon::xkb::KEYSYM_NO_FLAGS);
    let keysym = if keysym.raw() == xkbcommon::xkb::Keysym::NoSymbol.raw() {
        xkbcommon::xkb::keysym_from_name(&name, xkbcommon::xkb::KEYSYM_CASE_INSENSITIVE)
    } else {
        keysym
    };
    if keysym.raw() == xkbcommon::xkb::Keysym::NoSymbol.raw() {
        return Err(format!("'{token}' is not a valid key"));
    }
    Ok(xkbcommon::xkb::keysym_get_name(keysym))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_binding_is_super_v() {
        let binding = parse_binding(DEFAULT_BINDING).expect("parse");
        assert_eq!(binding.modifiers, vec![Modifier::Super]);
        assert_eq!(binding.key.as_deref(), Some("v"));
        assert_eq!(binding.keycode, None);
        assert!(binding.description.is_some());
    }

    #[test]
    fn modifiers_and_named_keys_are_normalized() {
        let binding = parse_binding("shift+super+f5").expect("parse");
        assert_eq!(binding.modifiers, vec![Modifier::Super, Modifier::Shift]);
        assert_eq!(binding.key.as_deref(), Some("F5"));
        assert_eq!(binding.describe(), "Super+Shift+F5");
    }

    #[test]
    fn a_shortcut_needs_one_key_and_one_modifier() {
        assert!(parse_binding("Super").is_err());
        assert!(parse_binding("v").is_err());
        assert!(parse_binding("Super+v+x").is_err());
        assert!(parse_binding("Hyper+v").is_err());
    }

    #[test]
    fn same_trigger_ignores_the_description_and_keycode() {
        let mut left = parse_binding("Super+V").expect("parse");
        let mut right = parse_binding("Super+v").expect("parse");
        left.description = None;
        left.keycode = Some(55);
        right.description = None;
        assert!(left.same_trigger(&right));
    }

    #[test]
    fn same_trigger_ignores_modifier_order() {
        let mut left = parse_binding("Super+Shift+V").expect("parse");
        let right = parse_binding("Shift+Super+v").expect("parse");
        left.modifiers.reverse();
        assert!(left.same_trigger(&right));
    }

    #[test]
    fn a_system_action_is_not_the_toggle_action() {
        let action: Box<RawValue> = ron::from_str("System(Screenshot)").expect("raw");
        assert!(!is_toggle_action(&action));
        let action: Box<RawValue> =
            ron::from_str("Spawn(\"cosmic-clipman --toggle\")").expect("raw");
        assert!(is_toggle_action(&action));
    }
}
