// SPDX-License-Identifier: Apache-2.0

//! Pending-paste gate: decides when a click or `Enter` may inject the paste
//! keystroke.
//!
//! The gate is deliberately independent from iced and from the clipboard. It
//! only answers whether the clipboard write and the popup close have both
//! happened, and whether a stale signal from a superseded activation must be
//! ignored.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Wait,
    Inject,
}

#[derive(Debug, Default)]
struct Pending {
    clipboard_ready: bool,
    popup_closed: bool,
}

#[derive(Debug, Default)]
pub struct Gate {
    pending: Option<Pending>,
    epoch: u64,
}

impl Gate {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a new pending paste and return its epoch. Any previous pending
    /// paste is replaced.
    pub fn arm(&mut self) -> u64 {
        self.epoch = self.epoch.wrapping_add(1);
        self.pending = Some(Pending::default());
        self.epoch
    }

    /// Drop the pending paste, if any.
    pub fn disarm(&mut self) {
        self.pending = None;
    }

    /// Drop the pending paste after the injection failed.
    pub fn failure(&mut self) {
        self.disarm();
    }

    /// Whether a paste is still waiting for its conditions.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// The clipboard write finished for `epoch`.
    pub fn clipboard_ready(&mut self, epoch: u64) -> Decision {
        if epoch != self.epoch {
            return Decision::Wait;
        }
        self.ready(|pending| pending.clipboard_ready = true)
    }

    /// The popup window reported closed.
    pub fn popup_closed(&mut self) -> Decision {
        self.ready(|pending| pending.popup_closed = true)
    }

    /// The safety deadline expired for `epoch`: treat the popup as closed, but
    /// still require the clipboard write to have finished.
    pub fn deadline(&mut self, epoch: u64) -> Decision {
        if epoch != self.epoch {
            return Decision::Wait;
        }
        self.ready(|pending| pending.popup_closed = true)
    }

    fn ready(&mut self, update: impl FnOnce(&mut Pending)) -> Decision {
        let Some(pending) = self.pending.as_mut() else {
            return Decision::Wait;
        };
        update(pending);
        if pending.clipboard_ready && pending.popup_closed {
            self.pending = None;
            Decision::Inject
        } else {
            Decision::Wait
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_happens_before_an_activation() {
        let mut gate = Gate::new();
        assert_eq!(gate.popup_closed(), Decision::Wait);
        assert_eq!(gate.clipboard_ready(0), Decision::Wait);
        assert_eq!(gate.deadline(0), Decision::Wait);
        assert!(!gate.is_pending());
    }

    #[test]
    fn injects_once_clipboard_and_popup_are_ready() {
        let mut gate = Gate::new();
        let epoch = gate.arm();
        assert_eq!(gate.clipboard_ready(epoch), Decision::Wait);
        assert_eq!(gate.popup_closed(), Decision::Inject);
        assert!(!gate.is_pending());
        assert_eq!(gate.popup_closed(), Decision::Wait);
        assert_eq!(gate.clipboard_ready(epoch), Decision::Wait);
    }

    #[test]
    fn order_of_conditions_does_not_matter() {
        let mut gate = Gate::new();
        let epoch = gate.arm();
        assert_eq!(gate.popup_closed(), Decision::Wait);
        assert_eq!(gate.clipboard_ready(epoch), Decision::Inject);
    }

    #[test]
    fn stale_clipboard_signal_is_ignored() {
        let mut gate = Gate::new();
        let old = gate.arm();
        let current = gate.arm();
        assert_eq!(gate.clipboard_ready(old), Decision::Wait);
        assert_eq!(gate.clipboard_ready(current), Decision::Wait);
        assert_eq!(gate.popup_closed(), Decision::Inject);
    }

    #[test]
    fn deadline_needs_the_clipboard_write_too() {
        let mut gate = Gate::new();
        let epoch = gate.arm();
        assert_eq!(gate.deadline(epoch), Decision::Wait);
        assert!(gate.is_pending());
        assert_eq!(gate.clipboard_ready(epoch), Decision::Inject);
    }

    #[test]
    fn stale_deadline_does_not_inject() {
        let mut gate = Gate::new();
        let old = gate.arm();
        let current = gate.arm();
        assert_eq!(gate.deadline(old), Decision::Wait);
        assert_eq!(gate.clipboard_ready(current), Decision::Wait);
        assert_eq!(gate.deadline(current), Decision::Inject);
    }

    #[test]
    fn failure_clears_the_pending_paste() {
        let mut gate = Gate::new();
        let epoch = gate.arm();
        gate.failure();
        assert!(!gate.is_pending());
        assert_eq!(gate.clipboard_ready(epoch), Decision::Wait);
    }

    #[test]
    fn disarming_cancels_the_pending_paste() {
        let mut gate = Gate::new();
        let epoch = gate.arm();
        gate.disarm();
        assert!(!gate.is_pending());
        assert_eq!(gate.clipboard_ready(epoch), Decision::Wait);
        assert_eq!(gate.popup_closed(), Decision::Wait);
    }
}
