//! Wave 4 (GY-ZCODE-004) clipboard ring — wraps `goya_clipboard`'s
//! [`goya_clipboard::MemoryClipboard`] with a permissive capability policy
//! and a local bounded ring of the last 8 entries.
//!
//! Wave 9 (GY-ZCODE-009) wires the system (Wayland data-device) clipboard:
//! Copy places the line on BOTH the compositor selection
//! (`win.set_clipboard_text`) and this ring; Paste asks the compositor
//! (`win.request_clipboard_text`) and inserts the `ClipboardText` payload at
//! the composer cursor when it arrives, falling back to the ring whenever
//! the compositor answers `None` (or has no data device at all —
//! `set_clipboard_text`/`request_clipboard_text` are safe no-ops there).
//! The ring stays as the offline fallback everywhere the compositor does
//! not answer.
//!
//! **None-ambiguity**: goya-wayland's `request_clipboard_text` collapses
//! three different situations into `ClipboardText(None)` — no selection
//! offer at all (absent/empty data device), a selection whose offer has no
//! `text/plain` mime, and a selection whose payload is empty. The
//! upstream event carries no way to distinguish them (checked: the
//! data-device protocol would allow it, but the frozen
//! `PlatformEvent::ClipboardText(Option<String>)` shape does not), so the
//! monitor treats every `None` as "not usable" and falls back to the ring.
//! The practical difference is nil: an empty selection inserts nothing
//! anyway, and the ring is the record of what the user last copied here.

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
    fn permits(
        &self,
        _principal: PrincipalId,
        _capability: goya_clipboard::ClipboardCapability,
    ) -> bool {
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
        let ok = self.inner.copy(request, text, &self.source, &[]).is_ok();
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

/// Wave 9 (GY-ZCODE-009): where a Copy action's payload goes.
/// `RingAndSystem` is the normal path (ring record + compositor
/// selection); `RingOnly` is the offline fallback when no system clipboard
/// exists (no compositor / no data-device global).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopyTarget {
    Ring(String),
    RingAndSystem(String),
}

/// Wave 9: pure Copy decision — pick the copy payload and its targets
/// from the transcript rows. The payload is the last non-empty row
/// (Wave 4 selection rule); `Some` when there is one. The bin applies the
/// plan (`MonitorClipboard::copy_line` for the ring, `set_clipboard_text`
/// for the system half) so the shortcut path stays testable without a
/// window.
pub fn plan_copy(rows: &[String], system_available: bool) -> Option<CopyTarget> {
    let line = rows.iter().rev().find(|row| !row.is_empty())?.clone();
    Some(if system_available {
        CopyTarget::RingAndSystem(line)
    } else {
        CopyTarget::Ring(line)
    })
}

/// Wave 9: pure Paste decision — given the compositor's `ClipboardText`
/// answer (or `None` when absent/no data device) and the ring's most
/// recent entry, what text is inserted at the composer cursor? The system
/// payload wins; every `None` (see the None-ambiguity note in the module
/// docs) falls back to the ring; both empty yields nothing to insert.
pub fn resolve_paste(system: Option<&str>, ring: Option<&str>) -> Option<String> {
    system
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .or_else(|| ring.map(str::to_string))
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

    // --- Wave 9 (GY-ZCODE-009): system-clipboard-backed ring ---

    #[test]
    fn copy_picks_last_non_empty_row_and_targets_both_clipboards() {
        use super::{plan_copy, CopyTarget};
        let rows = vec![String::new(), "first".into(), String::new(), "last".into()];
        assert_eq!(
            plan_copy(&rows, true),
            Some(CopyTarget::RingAndSystem("last".into()))
        );
        // Offline (no data-device): ring only — the fallback target.
        assert_eq!(
            plan_copy(&rows, false),
            Some(CopyTarget::Ring("last".into()))
        );
        assert_eq!(plan_copy(&[String::new()], true), None);
        assert_eq!(plan_copy(&[], true), None);
    }

    #[test]
    fn paste_prefers_system_and_falls_back_to_ring_on_none() {
        use super::resolve_paste;
        // Compositor answered: system wins even when the ring is stale.
        assert_eq!(
            resolve_paste(Some("from system"), Some("from ring")),
            Some("from system".into())
        );
        // None = no selection / no text mime / empty payload (ambiguous,
        // see module docs): ring fallback.
        assert_eq!(
            resolve_paste(None, Some("from ring")),
            Some("from ring".into())
        );
        assert_eq!(resolve_paste(None, None), None);
        // An empty system payload is as unusable as None.
        assert_eq!(
            resolve_paste(Some(""), Some("from ring")),
            Some("from ring".into())
        );
    }

    #[test]
    fn copy_plan_round_trips_through_the_ring() {
        use super::{plan_copy, CopyTarget};
        let policy = AllowAllPolicy;
        let clip = MonitorClipboard::new(&policy);
        let rows = vec!["transcript row".to_string()];
        // The bin applies exactly this: record on the ring (always) and
        // push to the compositor (set_clipboard_text, window side effect —
        // untestable headless, hence the pure plan above).
        if let Some(CopyTarget::RingAndSystem(line)) = plan_copy(&rows, true) {
            assert!(clip.copy_line(USER, &line));
            // set_clipboard_text(&line) would go here in the live path.
        } else {
            panic!("copy plan must carry a payload");
        }
        assert_eq!(clip.paste_text().as_deref(), Some("transcript row"));
    }
}
