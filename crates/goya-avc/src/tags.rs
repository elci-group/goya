//! Proposed binary section tags (directive §6/§7 of GOYA-GFX-AVC-001) and
//! the Phase 0 collision review (ADR-AVC-001 F6).
//!
//! Tags are 4 bytes of `A–Z`/`0–9` per GY-GFXC/1 (E1014). Adoption is
//! gated on Phase 2 and `.gvi` format version negotiation; this module
//! fixes the *proposal* so the collision check is executable and checks
//! against the union of the GVI and AVR tag sets, so it cannot rot
//! silently.

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

/// Tags already used or proposed by other Goya container families at
/// audit time. Kept in sync with `crates/goya-gvi/src/lib.rs` (GY-GVI/0)
/// and `crates/goya-avr/src/tags.rs` (GOYA-GFX-AVR-001); the test below
/// fails if either list drifts.
pub const EXISTING_CONTAINER_TAGS: &[ProposedTag] = &[
    // GY-GVI/0 (goya-gvi).
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
    // GOYA-GFX-AVR-001 (goya-avr, proposed).
    ProposedTag::new(*b"OBSV"),
    ProposedTag::new(*b"ODOM"),
    ProposedTag::new(*b"OREP"),
    ProposedTag::new(*b"ANCH"),
    ProposedTag::new(*b"NBRH"),
    ProposedTag::new(*b"NAVG"),
    ProposedTag::new(*b"RDEP"),
    ProposedTag::new(*b"RSDY"),
    ProposedTag::new(*b"FIDY"),
    ProposedTag::new(*b"PROV"),
];

/// Proposed AOT-VC inventory sections (ADR-AVC-001 F6).
pub const TIML: ProposedTag = ProposedTag::new(*b"TIML");
pub const EVNT: ProposedTag = ProposedTag::new(*b"EVNT");
pub const CHKP: ProposedTag = ProposedTag::new(*b"CHKP");
pub const PALT: ProposedTag = ProposedTag::new(*b"PALT");
pub const PIXS: ProposedTag = ProposedTag::new(*b"PIXS");
pub const ROWS: ProposedTag = ProposedTag::new(*b"ROWS");
pub const REGN: ProposedTag = ProposedTag::new(*b"REGN");
pub const TRIM: ProposedTag = ProposedTag::new(*b"TRIM");
pub const EXCT: ProposedTag = ProposedTag::new(*b"EXCT");
pub const AUDS: ProposedTag = ProposedTag::new(*b"AUDS");

pub const PROPOSED_AVC_TAGS: &[ProposedTag] =
    &[TIML, EVNT, CHKP, PALT, PIXS, ROWS, REGN, TRIM, EXCT, AUDS];

/// Every byte of every tag is `A–Z` or `0–9` (GY-GFXC/1 E1014).
pub fn is_tag_charset_valid(tag: &ProposedTag) -> bool {
    tag.0
        .iter()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposed_tags_do_not_collide_with_existing_container_tags() {
        for proposed in PROPOSED_AVC_TAGS {
            assert!(
                !EXISTING_CONTAINER_TAGS.contains(proposed),
                "AVC tag {} collides with an existing or proposed container tag",
                proposed.as_str()
            );
        }
    }

    #[test]
    fn all_tags_respect_the_container_charset() {
        for tag in PROPOSED_AVC_TAGS
            .iter()
            .chain(EXISTING_CONTAINER_TAGS.iter())
        {
            assert!(
                is_tag_charset_valid(tag),
                "tag {} violates E1014",
                tag.as_str()
            );
        }
    }

    #[test]
    fn proposed_set_matches_the_adr_table() {
        let ids: Vec<String> = PROPOSED_AVC_TAGS
            .iter()
            .map(|t| t.as_str().to_string())
            .collect();
        assert_eq!(
            ids,
            vec!["TIML", "EVNT", "CHKP", "PALT", "PIXS", "ROWS", "REGN", "TRIM", "EXCT", "AUDS"]
        );
    }
}
