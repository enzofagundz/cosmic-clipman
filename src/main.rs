// SPDX-License-Identifier: Apache-2.0

use std::process::ExitCode;

fn main() -> ExitCode {
    let toggle = std::env::args().skip(1).any(|arg| arg == "--toggle");
    if toggle {
        return match toggle_applet() {
            Ok(()) => ExitCode::SUCCESS,
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
