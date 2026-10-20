// SPDX-License-Identifier: Apache-2.0

//! Clipboard capture: a subscription thread on top of `wl-clipboard-rs`, plus
//! writing an item back to the clipboard.
//!
//! The rules ported from the original live here: the MIME preference order, the
//! 10 MiB payload cap, dropping a repeat of the same content within 100 ms, and
//! ignoring a clipboard change this applet caused for two seconds.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use futures::channel::mpsc::UnboundedSender;
use wl_clipboard_rs::copy::{MimeType, Options, Source};
use wl_clipboard_rs::paste::Seat;
use wl_clipboard_rs::watch::{ClipboardEvent, ClipboardType, Watcher};

use crate::domain::MAX_PAYLOAD_BYTES;

pub const REPEAT_WINDOW: Duration = Duration::from_millis(100);
pub const SELF_COPY_TTL: Duration = Duration::from_secs(2);

const TEXT_MIME_TYPES: [&str; 4] = [
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "text/plain",
    "STRING",
];

const MAX_RESTARTS: u32 = 5;
const RESTART_DELAY: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipEvent {
    Text(String),
    Failed(String),
}

/// State shared between the applet and the watcher thread. Atomics because the
/// watcher must be able to check both flags on every clipboard event.
#[derive(Default)]
pub struct Shared {
    incognito: AtomicBool,
    self_copy: Mutex<Option<Instant>>,
}

impl Shared {
    pub fn incognito(&self) -> bool {
        self.incognito.load(Ordering::Relaxed)
    }

    pub fn set_incognito(&self, on: bool) {
        self.incognito.store(on, Ordering::Relaxed);
    }

    /// Mark a clipboard change this applet is about to cause.
    pub fn arm_self_copy(&self, now: Instant) {
        let mut slot = self.self_copy.lock().unwrap_or_else(|err| err.into_inner());
        *slot = Some(now + SELF_COPY_TTL);
    }

    /// Consume the self-copy mark, reporting whether it is still fresh. A mark
    /// that no clipboard event followed expires, so it cannot swallow the next
    /// real copy.
    pub fn consume_self_copy(&self, now: Instant) -> bool {
        let mut slot = self.self_copy.lock().unwrap_or_else(|err| err.into_inner());
        match slot.take() {
            Some(deadline) => now < deadline,
            None => false,
        }
    }
}

/// Drops the same content arriving again inside the repeat window; one copy can
/// fire more than one event. A different clip always passes.
#[derive(Default)]
pub struct Gate {
    last_key: Option<String>,
    last_at: Option<Instant>,
}

impl Gate {
    pub fn accept(&mut self, now: Instant, key: &str) -> bool {
        let repeat = self.last_key.as_deref() == Some(key)
            && self
                .last_at
                .is_some_and(|last| now.duration_since(last) < REPEAT_WINDOW);
        self.last_key = Some(key.to_owned());
        self.last_at = Some(now);
        !repeat
    }
}

/// Start the watcher on a detached thread. Fatal errors are reported once as
/// [`ClipEvent::Failed`] after the retry budget is spent.
pub fn spawn(shared: Arc<Shared>, sender: UnboundedSender<ClipEvent>) {
    thread::Builder::new()
        .name("clipboard-watcher".into())
        .spawn(move || {
            let mut attempts = 0;
            loop {
                match watch_once(&shared, &sender) {
                    Ok(()) => return,
                    Err(err) => {
                        attempts += 1;
                        if attempts > MAX_RESTARTS {
                            let _ = sender.unbounded_send(ClipEvent::Failed(err));
                            return;
                        }
                        thread::sleep(RESTART_DELAY);
                    }
                }
            }
        })
        .expect("spawn the clipboard watcher thread");
}

fn watch_once(shared: &Shared, sender: &UnboundedSender<ClipEvent>) -> Result<(), String> {
    let mut watcher = Watcher::new(ClipboardType::Regular, Seat::Unspecified)
        .map_err(|err| format!("clipboard watch failed: {err}"))?;
    let mut gate = Gate::default();
    let mut seeded = false;
    loop {
        let event = match watcher.next_event() {
            Ok(Some(event)) => event,
            Ok(None) => return Ok(()),
            Err(err) => return Err(format!("clipboard watch failed: {err}")),
        };
        // The first event reports the state the watcher joined in on, whatever
        // it is: the clipboard contents already present, or a cleared
        // selection. Only copies made while the applet runs belong in the
        // history, as with the original's owner-changed signal.
        if !seeded {
            seeded = true;
            continue;
        }
        match event {
            ClipboardEvent::Changed {
                mime_types,
                mut offer,
                ..
            } => {
                let Some(mime) = pick_mime(&mime_types) else {
                    continue;
                };
                let Ok(mut pipe) = offer.receive(mime) else {
                    continue;
                };
                let Some(bytes) = read_limited(&mut pipe) else {
                    continue;
                };
                let text = String::from_utf8_lossy(&bytes).into_owned();
                if text.is_empty() {
                    continue;
                }
                let now = Instant::now();
                if shared.incognito() || shared.consume_self_copy(now) {
                    continue;
                }
                if !gate.accept(now, &text) {
                    continue;
                }
                let _ = sender.unbounded_send(ClipEvent::Text(text));
            }
            ClipboardEvent::Cleared { .. } => continue,
        }
    }
}

fn pick_mime(offered: &[String]) -> Option<&str> {
    TEXT_MIME_TYPES
        .iter()
        .copied()
        .find(|preferred| offered.iter().any(|mime| mime == preferred))
}

fn read_limited(pipe: &mut impl std::io::Read) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                if data.len() + read > MAX_PAYLOAD_BYTES {
                    return None;
                }
                data.extend_from_slice(&chunk[..read]);
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
    }
    if data.is_empty() { None } else { Some(data) }
}

/// Put text back on the clipboard. The crate keeps a thread serving the
/// selection until another client takes it over.
pub fn set_text(text: &str) -> Result<(), String> {
    let mut options = Options::new();
    options.foreground(false);
    options
        .copy(
            Source::Bytes(text.as_bytes().to_vec().into_boxed_slice()),
            MimeType::Text,
        )
        .map_err(|err| format!("could not write the clipboard: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_window_drops_only_the_same_content() {
        let mut gate = Gate::default();
        let start = Instant::now();
        assert!(gate.accept(start, "same"));
        assert!(!gate.accept(start + Duration::from_millis(50), "same"));
        assert!(gate.accept(start + Duration::from_millis(150), "same"));
        assert!(gate.accept(start + Duration::from_millis(160), "other"));
    }

    #[test]
    fn self_copy_mark_is_consumed_and_expires() {
        let shared = Shared::default();
        let now = Instant::now();
        shared.arm_self_copy(now);
        assert!(shared.consume_self_copy(now + Duration::from_millis(100)));
        assert!(!shared.consume_self_copy(now + Duration::from_millis(200)));

        shared.arm_self_copy(now);
        assert!(!shared.consume_self_copy(now + SELF_COPY_TTL + Duration::from_millis(1)));
        assert!(!shared.consume_self_copy(now));
    }

    #[test]
    fn mime_preference_matches_the_original() {
        assert_eq!(
            pick_mime(&["text/plain".into(), "UTF8_STRING".into()]),
            Some("UTF8_STRING")
        );
        assert_eq!(pick_mime(&["image/png".into()]), None);
    }

    #[test]
    fn oversized_payloads_are_dropped() {
        let oversized = vec![b'x'; MAX_PAYLOAD_BYTES + 1];
        assert_eq!(read_limited(&mut oversized.as_slice()), None);
        assert_eq!(
            read_limited(&mut b"hello".as_slice()),
            Some(b"hello".to_vec())
        );
    }
}
