// SPDX-License-Identifier: Apache-2.0

//! The shortcut CLI as the user runs it: a temporary XDG config home, the real
//! binary, and the cosmic-config file the settings daemon reads.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const CUSTOM: &str = "cosmic/com.system76.CosmicSettings.Shortcuts/v1/custom";

fn run(config_home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cosmic-clipman"))
        .args(args)
        .env("XDG_CONFIG_HOME", config_home)
        .env("HOME", config_home)
        .env_remove("HOST_XDG_CONFIG_HOME")
        .output()
        .expect("run cosmic-clipman")
}

fn custom_path(config_home: &Path) -> PathBuf {
    config_home.join(CUSTOM)
}

fn write_custom(config_home: &Path, body: &str) {
    let path = custom_path(config_home);
    fs::create_dir_all(path.parent().expect("parent")).expect("create config dir");
    fs::write(path, body).expect("write custom shortcuts");
}

fn read_custom(config_home: &Path) -> String {
    fs::read_to_string(custom_path(config_home)).expect("read custom shortcuts")
}

#[test]
fn install_writes_one_default_binding_and_is_idempotent() {
    let home = TempDir::new().expect("temp home");

    let output = run(home.path(), &["--install-shortcut"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let installed = read_custom(home.path());
    assert!(installed.contains("Super"));
    assert!(installed.contains("key: \"v\""));
    assert_eq!(installed.matches("Spawn(").count(), 1);
    assert!(installed.contains("cosmic-clipman --toggle"));

    let output = run(home.path(), &["--install-shortcut"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let again = read_custom(home.path());
    assert_eq!(again.matches("Spawn(").count(), 1);
}

#[test]
fn install_refuses_a_conflict_without_touching_the_file() {
    let home = TempDir::new().expect("temp home");
    let body = r#"{
        (
            modifiers: [Super],
            key: "v",
        ): Spawn("other-app"),
    }"#;
    write_custom(home.path(), body);

    let output = run(home.path(), &["--install-shortcut"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already bound"), "{stderr}");
    assert!(stderr.contains("other-app"), "{stderr}");
    assert_eq!(read_custom(home.path()), body);
}

#[test]
fn install_honors_a_custom_binding() {
    let home = TempDir::new().expect("temp home");

    let output = run(
        home.path(),
        &["--install-shortcut", "--shortcut", "Super+Shift+V"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let installed = read_custom(home.path());
    assert!(installed.contains("Super"));
    assert!(installed.contains("Shift"));
    assert!(installed.contains("key: \"v\""));
}

#[test]
fn uninstall_removes_our_binding_and_keeps_unrelated_entries() {
    let home = TempDir::new().expect("temp home");
    let body = r#"{
        (
            modifiers: [Super],
            key: "v",
        ): Spawn("cosmic-clipman --toggle"),
        (
            modifiers: [Super, Shift],
            key: "s",
        ): System(Screenshot),
    }"#;
    write_custom(home.path(), body);

    let output = run(home.path(), &["--uninstall-shortcut"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let remaining = read_custom(home.path());
    assert!(!remaining.contains("cosmic-clipman --toggle"));
    assert!(remaining.contains("System(Screenshot)"));
}

#[test]
fn uninstall_keeps_a_binding_repointed_to_another_command() {
    let home = TempDir::new().expect("temp home");
    let body = r#"{
        (
            modifiers: [Super],
            key: "v",
        ): Spawn("other-app"),
    }"#;
    write_custom(home.path(), body);

    let output = run(home.path(), &["--uninstall-shortcut"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let remaining = read_custom(home.path());
    assert!(remaining.contains("Spawn(\"other-app\")"));
}

#[test]
fn uninstall_without_a_binding_succeeds() {
    let home = TempDir::new().expect("temp home");

    let output = run(home.path(), &["--uninstall-shortcut"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!custom_path(home.path()).exists());
}
