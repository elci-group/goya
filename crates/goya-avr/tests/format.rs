//! Phase 7 format stabilisation (GOYA-GFX-AVR-001 §13.1, §13.2): the
//! §13.1 collision and compatibility review, executed as a gate. The
//! proposed AVR section tags must not collide with the existing `.gvi`
//! tag namespace, must be unique among themselves, and must respect the
//! container charset. Adoption into a binary format remains gated on
//! §13.2 feature negotiation — nothing here adopts them.

use goya_avr::tags::{is_tag_charset_valid, EXISTING_GVI_TAGS, PROPOSED_AVR_TAGS};
use std::collections::BTreeSet;

/// §13.1: no proposed AVR tag collides with an existing `.gvi` tag, and
/// no two proposed tags collide with each other.
#[test]
fn proposed_tags_pass_the_collision_review() {
    let mut seen = BTreeSet::new();
    for tag in PROPOSED_AVR_TAGS {
        let key = tag.0;
        assert!(seen.insert(key), "duplicate proposed tag {}", tag.as_str());
        assert!(
            !EXISTING_GVI_TAGS.contains(tag),
            "proposed tag {} collides with an existing .gvi tag",
            tag.as_str()
        );
    }
}

/// §13.1: every tag (proposed and existing) respects the container's
/// `A–Z`/`0–9` charset constraint.
#[test]
fn all_tags_respect_the_container_charset_gate() {
    for tag in PROPOSED_AVR_TAGS.iter().chain(EXISTING_GVI_TAGS.iter()) {
        assert!(is_tag_charset_valid(tag), "tag {} violates the charset", tag.as_str());
    }
}

/// §13.2: the contract version is stamped and the existing-`.gvi`
/// baseline is non-empty — upgraded implementations have something to
/// remain compatible with.
#[test]
fn contract_identity_is_stamped() {
    assert_eq!(goya_avr::DIRECTIVE_ID, "GOYA-GFX-AVR-001");
    assert!(goya_avr::CONTRACT_VERSION.starts_with("avr-contract/"));
    assert!(!EXISTING_GVI_TAGS.is_empty());
}
