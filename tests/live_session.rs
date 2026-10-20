// SPDX-License-Identifier: Apache-2.0

//! Checks that need the live COSMIC Wayland session.
//!
//! Run them by hand on a COSMIC session:
//!
//! ```sh
//! cargo test --test live_session -- --ignored --test-threads=1
//! ```
//!
//! They touch the real clipboard, so they prove capture and paste end to end,
//! which no headless test can.

use std::sync::Arc;
use std::time::Duration;

use cosmic_clipman::inject;
use cosmic_clipman::watcher::{self, ClipEvent, Shared};
use futures::StreamExt;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
}

#[test]
#[ignore = "needs a live COSMIC Wayland session"]
fn clipboard_write_reaches_the_watcher() {
    let shared = Arc::new(Shared::default());
    let (sender, mut events) = futures::channel::mpsc::unbounded();
    watcher::spawn(shared, sender);

    std::thread::sleep(Duration::from_millis(400));
    watcher::set_text("cosmic-clipman live watcher check").expect("write the clipboard");

    let runtime = runtime();
    let text = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(event) = events.next().await {
                if let ClipEvent::Text(text) = event {
                    return Some(text);
                }
            }
            None
        })
        .await
        .ok()
        .flatten()
    });
    assert_eq!(
        text.as_deref(),
        Some("cosmic-clipman live watcher check"),
        "the watcher reported no text event"
    );
}

#[test]
#[ignore = "needs a live COSMIC Wayland session and a focused window"]
fn ctrl_v_injection_does_not_error() {
    inject::press_ctrl_v().expect("virtual-keyboard injection is accepted");
}
