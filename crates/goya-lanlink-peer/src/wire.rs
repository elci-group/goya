//! Wire types of `goya.lanlink/0`. Keep them in step with Arachnid's
//! generated TV runtime (`crates/arachnid/src/lanlink.rs`).
//!
//! Taken from Tapestry's `tapestry_core::protocol::tvlink`. One change:
//! [`TvMessage::Act`] carries `presentation_id: Option<String>`, because a
//! dashboard peer (Bloom's household link) shows no presentation, and the
//! TV runtime then sends `null`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PROTOCOL: &str = "goya.lanlink/0";
pub const INFO_KEYS: &[u8] = b"goya.lanlink/0 keys";
pub const INFO_SAS: &[u8] = b"goya.lanlink/0 sas";
/// Path prefix of every endpoint.
pub const PATH_PREFIX: &str = "/lanlink/v0";
/// Path of sealed messages (also bound into the AAD).
pub const MSG_PATH: &str = "/lanlink/v0/msg";
/// Response header carrying the counter of a sealed binary body.
pub const CTR_HEADER: &str = "X-Goya-Ctr";
/// Slot names: `main` for PRESENT and WEB, `left` and `right` for COMPARE.
pub const SLOT_MAIN: &str = "main";
pub const SLOT_LEFT: &str = "left";
pub const SLOT_RIGHT: &str = "right";

/// Largest request body the peer reads, in bytes.
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
/// Largest encoded image a peer may offer.
pub const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
/// Largest encoded video.
pub const MAX_VIDEO_BYTES: u64 = 64 * 1024 * 1024;
/// Largest decoded image.
pub const MAX_IMAGE_PIXELS: u64 = 16_000_000;
/// A pairing must be confirmed on both sides within this many seconds.
pub const PAIRING_TTL_SECS: u64 = 120;
/// Pairing attempts accepted per run.
pub const MAX_PAIRING_ATTEMPTS: u32 = 3;
/// How long a poll waits for something new, in seconds.
pub const POLL_WAIT_SECS: u64 = 20;
/// A paired TV that has not polled for this long is dropped.
pub const IDLE_REVOKE_SECS: u64 = 60;
/// Each side accepts each counter at most once, within this window below
/// the highest seen.
pub const REPLAY_WINDOW: u64 = 64;

/// Step 1, TV → peer, plain JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairCommit {
    pub protocol: String,
    /// Hex SHA-256 of the TV's raw (65-byte, uncompressed) public key.
    pub commitment: String,
}

/// Step 2, peer → TV, plain JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairCommitReply {
    pub pairing_id: String,
    /// Base64 of the peer's raw public key.
    pub desktop_pub: String,
}

/// Step 3, TV → peer, plain JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairReveal {
    pub pairing_id: String,
    /// Base64 of the TV's raw public key.
    pub tv_pub: String,
}

/// A sealed body in either direction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sealed {
    pub pairing_id: String,
    pub ctr: u64,
    /// Base64 AES-256-GCM ciphertext with tag.
    pub ct: String,
}

/// Plaintext inside a TV → peer [`Sealed`] body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TvMessage {
    /// The person pressed Confirm on the TV with the code showing.
    Confirm,
    /// Wait for something newer than `after`: a presentation or new fields.
    Poll { after: Option<String> },
    /// Fetch one artefact of the current presentation.
    Artefact { id: String },
    /// A decision on the shown presentation.
    Decide(Decide),
    /// A scene-declared act; `presentation_id` is `None` when nothing is
    /// shown.
    Act {
        presentation_id: Option<String>,
        name: String,
        arg: Option<String>,
    },
}

/// A value for a TV `field` binding: Goya's state types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldValue {
    Bool(bool),
    Int(i64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decide {
    /// 32 lowercase hex characters generated on the TV.
    pub client_review_id: String,
    pub presentation_id: String,
    /// `APPROVE`, `REJECT`, `SELECT`, `DEFER`, or `RATE_1` through `RATE_5`.
    pub decision: String,
    pub slot: Option<String>,
}

/// Plaintext inside a peer → TV [`Sealed`] body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DesktopMessage {
    /// Pairing state after a TV confirm or a poll.
    Pairing {
        confirmed_tv: bool,
        confirmed_desktop: bool,
    },
    /// Nothing new within the wait.
    Idle,
    Presentation(Presentation),
    /// Accepted and persisted before this reply was sent.
    ReviewAck {
        client_review_id: String,
        review_id: String,
        duplicate: bool,
    },
    /// The message was understood and refused.
    Refused { reason: String },
    /// New values for the TV's `field` bindings: after an act, or pushed
    /// on a poll when they change.
    Fields {
        values: BTreeMap<String, FieldValue>,
        notice: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PresentationMode {
    Present,
    Compare,
    Web,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    pub id: String,
    pub session: String,
    pub mode: PresentationMode,
    pub slots: Vec<ArtefactRef>,
    #[serde(default)]
    pub item: String,
    #[serde(default)]
    pub revision: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, FieldValue>,
}

/// What the TV must verify before it shows the bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtefactRef {
    pub slot: String,
    pub id: String,
    pub mime: String,
    pub byte_length: u64,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
    pub label: String,
}

impl Presentation {
    /// Structural limits the peer enforces before offering, and the TV
    /// enforces again before fetching.
    pub fn validate(&self) -> Result<(), String> {
        let want: &[&str] = match self.mode {
            PresentationMode::Present | PresentationMode::Web => &[SLOT_MAIN],
            PresentationMode::Compare => &[SLOT_LEFT, SLOT_RIGHT],
        };
        let have: Vec<&str> = self.slots.iter().map(|a| a.slot.as_str()).collect();
        if have != want {
            return Err(format!("{:?} needs slots {want:?}, not {have:?}", self.mode));
        }
        for a in &self.slots {
            let video = a.mime == "video/mp4";
            if video && self.mode != PresentationMode::Present {
                return Err("video is supported in single presentations only".into());
            }
            if video && (a.width > 3840 || a.height > 2160) {
                return Err("video exceeds 3840x2160".into());
            }
            if a.mime != "image/png" && a.mime != "image/jpeg" && !video {
                return Err(format!("{} has unsupported type {}", a.id, a.mime));
            }
            let max = if video { MAX_VIDEO_BYTES } else { MAX_IMAGE_BYTES };
            if a.byte_length == 0 || a.byte_length > max {
                return Err(format!("{} has {} bytes", a.id, a.byte_length));
            }
            if u64::from(a.width) * u64::from(a.height) > MAX_IMAGE_PIXELS || a.width == 0 || a.height == 0 {
                return Err(format!("{} is {}x{}", a.id, a.width, a.height));
            }
            if a.sha256.len() != 64 || !a.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("{} has a malformed digest", a.id));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_have_stable_tags() {
        assert_eq!(
            serde_json::to_string(&TvMessage::Poll { after: None }).unwrap(),
            r#"{"type":"poll","after":null}"#
        );
        assert_eq!(serde_json::from_str::<TvMessage>(r#"{"type":"confirm"}"#).unwrap(), TvMessage::Confirm);
        assert!(serde_json::from_str::<TvMessage>(r#"{"type":"poll","after":null,"extra":1}"#).is_err());
        // An act with nothing shown, as the TV runtime sends it.
        let act: TvMessage =
            serde_json::from_str(r#"{"type":"act","presentation_id":null,"name":"wake","arg":"2"}"#).unwrap();
        assert_eq!(act, TvMessage::Act { presentation_id: None, name: "wake".into(), arg: Some("2".into()) });
        let fields = DesktopMessage::Fields {
            values: BTreeMap::from([("d1_cpu".to_string(), FieldValue::Int(23))]),
            notice: None,
        };
        assert_eq!(serde_json::to_string(&fields).unwrap(), r#"{"type":"fields","values":{"d1_cpu":23},"notice":null}"#);
    }

    #[test]
    fn presentation_limits() {
        let a = ArtefactRef {
            slot: "main".into(),
            id: "art_x".into(),
            mime: "image/png".into(),
            byte_length: 10,
            sha256: "0".repeat(64),
            width: 4000,
            height: 4000,
            label: "A".into(),
        };
        let mut p = Presentation {
            id: "p1".into(),
            session: "s".into(),
            mode: PresentationMode::Present,
            slots: vec![a.clone()],
            item: String::new(),
            revision: 0,
            fields: Default::default(),
        };
        assert!(p.validate().is_ok());
        p.mode = PresentationMode::Compare;
        assert!(p.validate().is_err());
        p.slots = vec![
            ArtefactRef { slot: "left".into(), ..a.clone() },
            ArtefactRef { slot: "right".into(), width: 5000, ..a },
        ];
        assert!(p.validate().unwrap_err().contains("5000x4000"));
    }
}
