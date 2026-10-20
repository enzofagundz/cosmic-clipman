// SPDX-License-Identifier: Apache-2.0

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|arg| arg == "--toggle") {
        return match toggle_applet() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("cosmic-clipman: {err}");
                ExitCode::FAILURE
            }
        };
    }

    if args.iter().any(|arg| arg == "--install-shortcut") {
        let binding = args
            .windows(2)
            .find(|pair| pair[0] == "--shortcut")
            .map(|pair| pair[1].as_str())
            .unwrap_or(cosmic_clipman::shortcut::DEFAULT_BINDING);
        return match cosmic_clipman::shortcut::install(binding) {
            Ok(cosmic_clipman::shortcut::InstallOutcome::Installed) => {
                println!("cosmic-clipman: {binding} now opens the popup");
                ExitCode::SUCCESS
            }
            Ok(cosmic_clipman::shortcut::InstallOutcome::AlreadyInstalled) => {
                println!("cosmic-clipman: {binding} is already installed");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("cosmic-clipman: {err}");
                ExitCode::FAILURE
            }
        };
    }

    if args.iter().any(|arg| arg == "--uninstall-shortcut") {
        return match cosmic_clipman::shortcut::remove() {
            Ok(cosmic_clipman::shortcut::RemoveOutcome::Removed) => {
                println!("cosmic-clipman: shortcut removed");
                ExitCode::SUCCESS
            }
            Ok(cosmic_clipman::shortcut::RemoveOutcome::Absent) => {
                println!("cosmic-clipman: no shortcut to remove");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("cosmic-clipman: {err}");
                ExitCode::FAILURE
            }
        };
    }

    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    cosmic_clipman::i18n::init(&requested_languages);
    match cosmic::applet::run::<cosmic_clipman::App>(()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("cosmic-clipman: {err}");
            ExitCode::FAILURE
        }
    }
}

fn toggle_applet() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("cannot start the async runtime: {err}"))?;
    runtime.block_on(cosmic_clipman::dbus::request_toggle())
}
