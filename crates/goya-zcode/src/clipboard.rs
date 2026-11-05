//! Wave 4 (GY-ZCODE-004) clipboard ring — wraps `goya_clipboard`'s
//! [`goya_clipboard::MemoryClipboard`] with a permissive capability policy
//! and a local bounded ring of the last 8 entries.
//!
//! The monitor is a single-principal, single-process tool, so the policy
//! allows everything (`AllowAllPolicy`); the capability check still runs
//! (a `DenyAll` policy denies copies, see tests). System (Wayland
//! data-device) clipboard integration is OUT of scope — blocked upstream
//! (GY-WAYLAND-001 follow-through); Ctrl+C/Ctrl+V operate purely on this
//! in-process ring.

use goya_clipboard::{
    CapabilityPolicy, ClipboardProvider, CopyMode, CopyRequest, MemoryClipboard, PrincipalId,
};
use goya_provenance::SourceReference;
use std::cell::RefCell;
use std::collections::VecDeque;

/// Ring capacity: the monitor remembers the last 8 copied lines locally.
pub const RING_CAP: usize = 8;

/// Permissive capability policy: the monitor has one principal (the user
/// at the keyboard) and no multi-tenant trust boundary, so every capability
/// is granted. The policy hook stays so the crate's gating contract is
/// exercised on every copy.
pub struct AllowAllPolicy;

impl CapabilityPolicy for AllowAllPolicy {
    fn permits(&self, _principal: PrincipalId, _capability: goya_clipboard::ClipboardCapability) -> bool {
        true
    }
}

/// Bounded clipboard ring over [`MemoryClipboard`]. `Copy` places the line
/// on the provider (capability-gated) and into the local ring; `Paste`
/// returns the most recent entry's plain text.
pub struct MonitorClipboard<'a> {
    inner: MemoryClipboard<'a>,
    ring: RefCell<VecDeque<String>>,
    source: SourceReference,
}

impl<'a> MonitorClipboard<'a> {
    pub fn new(policy: &'a dyn CapabilityPolicy) -> Self {
        MonitorClipboard {
            inner: MemoryClipboard::new(policy),
            ring: RefCell::new(VecDeque::new()),
            source: SourceReference {
                object_id: String::from("zcode-monitor/transcript"),
                component: String::from("goya-zcode"),
                state_revision: 0,
                module_version: env!("CARGO_PKG_VERSION").to_string(),
            },
        }
    }

    /// Copy one transcript line into the ring. Returns true when the
    /// provider accepted the copy (policy granted, payload rendered).
    pub fn copy_line(&self, principal: PrincipalId, text: &str) -> bool {
        let request = CopyRequest {
            selection: goya_provenance::SelectionRange::new(0, text.len()),
            mode: CopyMode::Verbatim,
            principal,
        };
        let ok = self
            .inner
            .copy(request, text, &self.source, &[])
            .is_ok();
        if ok {
            let mut ring = self.ring.borrow_mut();
            ring.push_back(text.to_string());
            while ring.len() > RING_CAP {
                ring.pop_front();
            }
        }
        ok
    }

    /// The most recent clipboard entry, if any.
    pub fn paste_text(&self) -> Option<String> {
        self.inner.last_entry().map(|e| e.plain_text)
    }

    /// Snapshot of the local ring, oldest first (diagnostics/tests).
    pub fn ring(&self) -> Vec<String> {
        self.ring.borrow().iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_clipboard::DenyAll;

    const USER: PrincipalId = PrincipalId(1);

    #[test]
    fn copy_then_paste_round_trip() {
        let policy = AllowAllPolicy;
        let clip = MonitorClipboard::new(&policy);
        assert!(clip.copy_line(USER, "hello transcript"));
        assert_eq!(clip.paste_text().as_deref(), Some("hello transcript"));
    }

    #[test]
    fn paste_empty_without_copy() {
        let policy = AllowAllPolicy;
        let clip = MonitorClipboard::new(&policy);
        assert_eq!(clip.paste_text(), None);
    }

    #[test]
    fn ring_is_bounded_and_ordered() {
        let policy = AllowAllPolicy;
        let clip = MonitorClipboard::new(&policy);
        for i in 0..(RING_CAP + 3) {
            assert!(clip.copy_line(USER, &format!("line {i}")));
        }
        let ring = clip.ring();
        assert_eq!(ring.len(), RING_CAP);
        assert_eq!(ring[0], "line 3");
        assert_eq!(ring[RING_CAP - 1], format!("line {}", RING_CAP + 2));
        // Paste always returns the most recent entry.
        assert_eq!(clip.paste_text().as_deref(), Some("line 10"));
    }

    #[test]
    fn policy_denial_leaves_ring_and_provider_empty() {
        let policy = DenyAll;
        let clip = MonitorClipboard::new(&policy);
        assert!(!clip.copy_line(USER, "denied"));
        assert_eq!(clip.paste_text(), None);
        assert!(clip.ring().is_empty());
    }
}
