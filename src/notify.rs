// SPDX-License-Identifier: Apache-2.0

//! Desktop notification for a paste the compositor would not accept.

use std::collections::HashMap;

use crate::fl;

/// The localized summary and body. Both are static copy: clipboard content is
/// never part of a notification.
#[must_use]
pub fn failure_copy() -> (String, String) {
    (fl!("paste-failed-summary"), fl!("paste-failed-body"))
}

/// Ask the desktop to show the failure notification.
pub async fn paste_failed() -> Result<(), String> {
    let (summary, body) = failure_copy();
    let connection = zbus::Connection::session()
        .await
        .map_err(|err| format!("no session bus for the notification: {err}"))?;
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .await
    .map_err(|err| format!("no notification service: {err}"))?;
    let hints: HashMap<&str, zbus::zvariant::Variant<'_>> = HashMap::new();
    proxy
        .call_method(
            "Notify",
            &(
                fl!("app-title"),
                0u32,
                "edit-paste-symbolic",
                summary,
                body,
                Vec::<String>::new(),
                hints,
                5000i32,
            ),
        )
        .await
        .map_err(|err| format!("could not show the notification: {err}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_failure_copy_tells_the_user_how_to_paste_manually() {
        let (summary, body) = failure_copy();
        assert!(!summary.is_empty());
        assert!(body.contains("Ctrl+V"));
        assert!(!body.contains('{'));
    }
}
