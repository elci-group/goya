//! Caprichio selection for a COSMIC desktop host.
//!
//! The Caprichio catalog (`goya_caprichios::catalog`) holds 80 draft
//! definitions. They are design-stage entries, not implementations. Nebula
//! selects only the handful whose relevance to a desktop Wayland host is
//! clear from the protocol report, and says why. A selection is a
//! "candidate on this host" statement about protocol presence; it is not an
//! implementation, a lowering recipe, or conformance.
//!
//! None of the entries are COSMIC-specific: every rule keys on a generic
//! Wayland interface. Nebula has no COSMIC-only Caprichio, and the table does
//! not invent one. COSMIC-private protocols are never a basis for selection.
//!
//! Deliberately left out: `power.idle_policy` (the catalog entry sets system
//! idle timeouts; `zwp_idle_inhibit_manager_v1` only inhibits idle for a
//! visible surface, which is a different contract), `display.configuration`
//! and `display.modes` (privileged `zwlr_output_manager_v1` /
//! `zcosmic_output_manager_v1`, meant for settings tools, not applications),
//! and every media, power, network and storage entry (no Wayland basis).

use crate::protocols::{ProtocolReport, SeatCapabilities};
use serde::Serialize;

/// Which seat capability a rule needs, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatNeed {
    Keyboard,
    Pointer,
    Touch,
}

impl SeatNeed {
    fn on(self, s: &SeatCapabilities) -> bool {
        match self {
            SeatNeed::Keyboard => s.keyboard,
            SeatNeed::Pointer => s.pointer,
            SeatNeed::Touch => s.touch,
        }
    }

    fn name(self) -> &'static str {
        match self {
            SeatNeed::Keyboard => "keyboard",
            SeatNeed::Pointer => "pointer",
            SeatNeed::Touch => "touch",
        }
    }
}

/// One selection rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    /// Catalog id; checked against `goya_caprichios::catalog::find`.
    pub id: &'static str,
    /// Every interface that must be advertised.
    pub interfaces: &'static [&'static str],
    pub seat: Option<SeatNeed>,
    pub why: &'static str,
}

/// The desktop table.
pub const DESKTOP_RULES: &[Rule] = &[
    Rule {
        id: "devices.input.keyboard",
        interfaces: &["wl_seat"],
        seat: Some(SeatNeed::Keyboard),
        why: "key events arrive through a wl_seat keyboard",
    },
    Rule {
        id: "devices.input.pointer",
        interfaces: &["wl_seat"],
        seat: Some(SeatNeed::Pointer),
        why: "pointer events arrive through a wl_seat pointer",
    },
    Rule {
        id: "devices.input.touch",
        interfaces: &["wl_seat"],
        seat: Some(SeatNeed::Touch),
        why: "touch contacts arrive through a wl_seat touch",
    },
    Rule {
        id: "display.surfaces",
        interfaces: &["wl_output"],
        seat: None,
        why: "wl_output advertises output geometry and scale to clients",
    },
    Rule {
        id: "display.color_profile",
        interfaces: &["wp_color_manager_v1"],
        seat: None,
        why: "colour profiles reach clients only through wp_color_manager_v1",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SelectionStatus {
    /// The basis is present on this host.
    Candidate,
    /// The protocol is present but the seat capability was not read.
    CandidateUnverified,
    /// The basis is absent.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Selection {
    pub id: &'static str,
    /// The catalog's proposed contract string (`goya.<id>.v1`).
    pub proposed_contract: String,
    pub status: SelectionStatus,
    pub basis: &'static [&'static str],
    pub reason: String,
}

/// Apply [`DESKTOP_RULES`] to a protocol report. Pure.
pub fn select_for_cosmic(report: &ProtocolReport) -> Vec<Selection> {
    DESKTOP_RULES
        .iter()
        .map(|rule| {
            let missing: Vec<&str> = rule
                .interfaces
                .iter()
                .copied()
                .filter(|i| report.advertised(i).is_none())
                .collect();
            let (status, reason) = if !missing.is_empty() {
                (
                    SelectionStatus::Unavailable,
                    format!("not advertised: {}", missing.join(", ")),
                )
            } else {
                match (rule.seat, &report.seats) {
                    (None, _) => (SelectionStatus::Candidate, rule.why.to_string()),
                    (Some(need), None) => (
                        SelectionStatus::CandidateUnverified,
                        format!("{}; seat {} capability not read", rule.why, need.name()),
                    ),
                    (Some(need), Some(seats)) if seats.iter().any(|s| need.on(s)) => (
                        SelectionStatus::Candidate,
                        format!("{}; a seat reports {}", rule.why, need.name()),
                    ),
                    (Some(need), Some(_)) => (
                        SelectionStatus::Unavailable,
                        format!("no seat reports the {} capability", need.name()),
                    ),
                }
            };
            Selection {
                id: rule.id,
                proposed_contract: format!("goya.{}.v1", rule.id),
                status,
                basis: rule.interfaces,
                reason,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocols::{classify, Global};

    #[test]
    fn every_rule_names_a_catalog_entry() {
        for rule in DESKTOP_RULES {
            let def = goya_caprichios::catalog::find(rule.id)
                .unwrap_or_else(|| panic!("{} is not in the catalog", rule.id));
            assert_eq!(def.proposed_contract(), format!("goya.{}.v1", rule.id));
        }
    }

    #[test]
    fn no_rule_uses_a_cosmic_private_protocol() {
        for rule in DESKTOP_RULES {
            assert!(rule
                .interfaces
                .iter()
                .all(|i| !crate::protocols::is_cosmic_private(i)));
        }
    }

    fn report(globals: &[(&str, u32)], seats: Option<Vec<SeatCapabilities>>) -> ProtocolReport {
        let g: Vec<Global> = globals.iter().map(|(i, v)| Global::new(i, *v)).collect();
        let mut r = classify(&g);
        r.seats = seats;
        r
    }

    fn status(sel: &[Selection], id: &str) -> SelectionStatus {
        sel.iter().find(|s| s.id == id).unwrap().status
    }

    #[test]
    fn desktop_seat_without_touch() {
        let r = report(
            &[("wl_seat", 9), ("wl_output", 4)],
            Some(vec![SeatCapabilities {
                pointer: true,
                keyboard: true,
                touch: false,
            }]),
        );
        let s = select_for_cosmic(&r);
        assert_eq!(
            status(&s, "devices.input.keyboard"),
            SelectionStatus::Candidate
        );
        assert_eq!(
            status(&s, "devices.input.pointer"),
            SelectionStatus::Candidate
        );
        assert_eq!(
            status(&s, "devices.input.touch"),
            SelectionStatus::Unavailable
        );
        assert_eq!(status(&s, "display.surfaces"), SelectionStatus::Candidate);
        assert_eq!(
            status(&s, "display.color_profile"),
            SelectionStatus::Unavailable
        );
    }

    #[test]
    fn unread_seat_is_unverified_and_missing_seat_is_unavailable() {
        let s = select_for_cosmic(&report(&[("wl_seat", 1)], None));
        assert_eq!(
            status(&s, "devices.input.keyboard"),
            SelectionStatus::CandidateUnverified
        );
        let s = select_for_cosmic(&report(&[], None));
        assert!(s.iter().all(|s| s.status == SelectionStatus::Unavailable));
    }
}
