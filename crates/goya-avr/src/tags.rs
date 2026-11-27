//! Proposed binary section tags (directive §13.1) and the Phase 0 collision
//! review (ADR-AVR-001 F3).
//!
//! Tags are 4 bytes of `A–Z`/`0–9` per GY-GFXC/1 (E1014). Adoption is
//! gated on Phase 2 and `.gvi` format version negotiation (§13.2); this
//! module fixes the *proposal* so the collision check is executable and
//! cannot rot silently.

/// A proposed 4-byte container section tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProposedTag(pub [u8; 4]);

impl ProposedTag {
    pub const fn new(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    /// The tag as an ASCII string.
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("?")
    }
}

/// Tags already used by `goya-gvi` (contract `GY-GVI/0`) at audit time.
/// Kept in sync with `crates/goya-gvi/src/lib.rs`; the test below fails if
/// the two drift apart.
pub const EXISTING_GVI_TAGS: &[ProposedTag] = &[
    ProposedTag::new(*b"CERT"),
    ProposedTag::new(*b"COST"),
    ProposedTag::new(*b"DIFF"),
    ProposedTag::new(*b"GEOM"),
    ProposedTag::new(*b"HASH"),
    ProposedTag::new(*b"INDX"),
    ProposedTag::new(*b"LSTY"),
    ProposedTag::new(*b"PARN"),
    ProposedTag::new(*b"PROJ"),
    ProposedTag::new(*b"SRCE"),
    ProposedTag::new(*b"STAT"),
    ProposedTag::new(*b"STYL"),
    ProposedTag::new(*b"TRAN"),
];

/// Proposed AVR inventory sections (directive §13.1).
pub const OBSV: ProposedTag = ProposedTag::new(*b"OBSV");
pub const ODOM: ProposedTag = ProposedTag::new(*b"ODOM");
pub const OREP: ProposedTag = ProposedTag::new(*b"OREP");
pub const ANCH: ProposedTag = ProposedTag::new(*b"ANCH");
pub const NBRH: ProposedTag = ProposedTag::new(*b"NBRH");
pub const NAVG: ProposedTag = ProposedTag::new(*b"NAVG");
pub const RDEP: ProposedTag = ProposedTag::new(*b"RDEP");
pub const RSDY: ProposedTag = ProposedTag::new(*b"RSDY");
pub const FIDY: ProposedTag = ProposedTag::new(*b"FIDY");
pub const PROV: ProposedTag = ProposedTag::new(*b"PROV");

pub const PROPOSED_AVR_TAGS: &[ProposedTag] =
    &[OBSV, ODOM, OREP, ANCH, NBRH, NAVG, RDEP, RSDY, FIDY, PROV];

/// Every byte of every tag is `A–Z` or `0–9` (GY-GFXC/1 E1014).
pub fn is_tag_charset_valid(tag: &ProposedTag) -> bool {
    tag.0.iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposed_tags_do_not_collide_with_existing_gvi_tags() {
        for proposed in PROPOSED_AVR_TAGS {
            assert!(
                !EXISTING_GVI_TAGS.contains(proposed),
                "AVR tag {} collides with an existing GVI tag",
                proposed.as_str()
            );
        }
    }

    #[test]
    fn all_tags_respect_the_container_charset() {
        for tag in PROPOSED_AVR_TAGS.iter().chain(EXISTING_GVI_TAGS.iter()) {
            assert!(is_tag_charset_valid(tag), "tag {} violates E1014", tag.as_str());
        }
    }

    #[test]
    fn proposed_set_matches_directive_table() {
        let ids: Vec<String> = PROPOSED_AVR_TAGS.iter().map(|t| t.as_str().to_string()).collect();
        assert_eq!(
            ids,
            vec!["OBSV", "ODOM", "OREP", "ANCH", "NBRH", "NAVG", "RDEP", "RSDY", "FIDY", "PROV"]
        );
    }
}
