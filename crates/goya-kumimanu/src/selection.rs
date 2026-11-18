//! ICCCM `CLIPBOARD` text, including INCR transfers.
//!
//! This module is pure. The backend interns the atoms, sends the requests,
//! and feeds the replies here. Payload bytes are never logged or counted
//! into diagnostics.
//!
//! Limits match the Wayland host: at most [`MAX_CLIPBOARD_BYTES`], valid
//! UTF-8, and not empty. The cap holds across INCR chunks, so a large
//! transfer is drained and discarded rather than buffered. The same rules
//! apply to `CLIPBOARD` and `PRIMARY`; this module is selection-agnostic
//! and the backend routes per [`Selection`].

use std::time::{Duration, Instant};

/// Largest clipboard payload served or accepted, in bytes.
pub const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;

/// The two selections this backend serves.
///
/// Both follow identical rules: the same cap, targets, INCR handling,
/// timeouts, and transfer limits. They are owned and tracked independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Selection {
    /// The desktop clipboard, set on copy or cut.
    Clipboard,
    /// The primary selection, set by selecting text.
    Primary,
}

impl Selection {
    pub(crate) const COUNT: usize = 2;

    /// Index into the backend's per-selection state array.
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Clipboard => 0,
            Self::Primary => 1,
        }
    }

    /// The event variant that carries this selection's answer.
    pub(crate) fn answered(self, text: Option<String>) -> crate::Event {
        match self {
            Self::Clipboard => crate::Event::ClipboardText(text),
            Self::Primary => crate::Event::PrimaryText(text),
        }
    }
}

/// A conversion that has not answered by then resolves as no text.
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// Largest property this owner writes in one piece. Longer text goes by
/// INCR even when BIG-REQUESTS would allow one request. GTK 3 uses the
/// same 262144-byte bound, so peers never see a larger single property.
pub(crate) const INCR_THRESHOLD: usize = 256 * 1024;

/// Largest INCR chunk this owner writes.
pub(crate) const INCR_CHUNK: usize = 64 * 1024;

/// INCR transfers served at once. A request past this is refused.
pub(crate) const MAX_TRANSFERS: usize = 4;

/// An INCR transfer whose peer has not moved for this long is dropped.
pub(crate) const TRANSFER_IDLE: Duration = Duration::from_secs(5);

/// X11 `CurrentTime`.
pub(crate) const CURRENT_TIME: u32 = 0;

/// `STRING`, predefined by the core protocol.
pub(crate) const STRING: u32 = 31;

/// Interned atoms the selection path needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Atoms {
    /// The `CLIPBOARD`-role selection: `CLIPBOARD` itself, or a private
    /// test name under `connect_with_selection`.
    pub selection: u32,
    /// The `PRIMARY` selection.
    pub primary: u32,
    pub targets: u32,
    pub utf8_string: u32,
    pub text_plain_utf8: u32,
    pub text_plain: u32,
    pub incr: u32,
    /// Conversion property for the `CLIPBOARD`-role selection.
    pub property: u32,
    /// Conversion property for `PRIMARY`. Distinct from `property` so two
    /// concurrent conversions never write the same window property.
    pub primary_property: u32,
}

/// What the owner sends back for one `SelectionRequest`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Answer<'a> {
    /// A list of atoms, written with type `ATOM` and format 32.
    Targets(Vec<u32>),
    /// Text bytes, written with format 8 and this type.
    Text { type_: u32, bytes: &'a [u8] },
    /// Text longer than one request: an `INCR` marker, then chunks.
    Incr { type_: u32, bytes: &'a [u8] },
    /// `SelectionNotify` with property `None`.
    Refuse,
}

/// Decide the answer for a request against the text this client owns.
///
/// `owned_at` is the timestamp passed to `SetSelectionOwner`. A request
/// stamped before it is refused, as ICCCM asks. `CurrentTime` on either side
/// skips that check. `single_limit` is the largest property written in one
/// request; a longer payload is sent with INCR.
pub(crate) fn answer<'a>(
    atoms: &Atoms,
    owned: Option<&'a str>,
    owned_at: u32,
    request_time: u32,
    target: u32,
    single_limit: usize,
) -> Answer<'a> {
    let Some(text) = owned else {
        return Answer::Refuse;
    };
    if owned_at != CURRENT_TIME && request_time != CURRENT_TIME && request_time < owned_at {
        return Answer::Refuse;
    }
    if target == atoms.targets {
        let mut list = vec![
            atoms.targets,
            atoms.utf8_string,
            atoms.text_plain_utf8,
            atoms.text_plain,
        ];
        if text.is_ascii() {
            list.push(STRING);
        }
        return Answer::Targets(list);
    }
    let type_ = if target == atoms.utf8_string
        || target == atoms.text_plain_utf8
        || target == atoms.text_plain
    {
        target
    } else if target == STRING && text.is_ascii() {
        STRING
    } else {
        return Answer::Refuse;
    };
    if text.is_empty() || text.len() > MAX_CLIPBOARD_BYTES {
        return Answer::Refuse;
    }
    let bytes = text.as_bytes();
    if bytes.len() > single_limit {
        Answer::Incr { type_, bytes }
    } else {
        Answer::Text { type_, bytes }
    }
}

/// One INCR transfer this owner is serving.
///
/// The peer deletes the property to ask for the next chunk. After the last
/// data chunk a zero-length chunk ends the transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Outgoing {
    pub requestor: u32,
    pub property: u32,
    pub type_: u32,
    bytes: Vec<u8>,
    offset: usize,
    chunk: usize,
    ended: bool,
    pub deadline: Instant,
}

impl Outgoing {
    pub fn new(
        requestor: u32,
        property: u32,
        type_: u32,
        bytes: &[u8],
        chunk: usize,
        now: Instant,
    ) -> Self {
        Self {
            requestor,
            property,
            type_,
            bytes: bytes.to_vec(),
            offset: 0,
            chunk: chunk.max(1),
            ended: false,
            deadline: now + TRANSFER_IDLE,
        }
    }

    /// The `INCR` marker value: a lower bound on the total size.
    pub fn total(&self) -> u32 {
        u32::try_from(self.bytes.len()).unwrap_or(u32::MAX)
    }

    /// The next chunk to write. The empty chunk ends the transfer. `None`
    /// once the empty chunk has been handed out.
    pub fn next_chunk(&mut self, now: Instant) -> Option<&[u8]> {
        if self.ended {
            return None;
        }
        self.deadline = now + TRANSFER_IDLE;
        let start = self.offset;
        let end = self.bytes.len().min(start.saturating_add(self.chunk));
        if start == end {
            self.ended = true;
        }
        self.offset = end;
        Some(&self.bytes[start..end])
    }

    pub fn ended(&self) -> bool {
        self.ended
    }

    pub fn expired(&self, now: Instant) -> bool {
        now >= self.deadline
    }
}

/// An INCR transfer this requestor is reading.
///
/// Every chunk past the cap, or with a different type or format, poisons the
/// transfer. A poisoned transfer keeps draining so the owner can finish, and
/// ends as no text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Incoming {
    bytes: Vec<u8>,
    type_: Option<u32>,
    poisoned: bool,
}

/// Progress of an [`Incoming`] transfer after one chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Progress {
    More,
    Done(Option<String>),
}

impl Incoming {
    pub fn push(
        &mut self,
        atoms: &Atoms,
        type_: u32,
        format: u8,
        bytes_after: u32,
        value: Vec<u8>,
    ) -> Progress {
        if value.is_empty() && bytes_after == 0 {
            if self.poisoned {
                return Progress::Done(None);
            }
            let Some(type_) = self.type_ else {
                return Progress::Done(None);
            };
            let bytes = std::mem::take(&mut self.bytes);
            return Progress::Done(decode(atoms, type_, 8, 0, bytes));
        }
        if self.poisoned {
            return Progress::More;
        }
        let same_type = *self.type_.get_or_insert(type_) == type_;
        if !same_type
            || format != 8
            || bytes_after != 0
            || self.bytes.len().saturating_add(value.len()) > MAX_CLIPBOARD_BYTES
        {
            self.poisoned = true;
            self.bytes = Vec::new();
            return Progress::More;
        }
        self.bytes.extend_from_slice(&value);
        Progress::More
    }
}

/// Words to ask `GetProperty` for so that one byte past the limit is visible.
pub(crate) fn property_words() -> u32 {
    (MAX_CLIPBOARD_BYTES / 4 + 1) as u32
}

/// Decode a converted property. Anything partial, oversized, INCR, not
/// format 8, empty, or invalid for its type is `None`.
pub(crate) fn decode(
    atoms: &Atoms,
    type_: u32,
    format: u8,
    bytes_after: u32,
    value: Vec<u8>,
) -> Option<String> {
    if type_ == atoms.incr || format != 8 || bytes_after != 0 {
        return None;
    }
    if value.is_empty() || value.len() > MAX_CLIPBOARD_BYTES {
        return None;
    }
    if type_ == STRING {
        // ISO 8859-1 maps each byte to the code point of the same value.
        return Some(value.into_iter().map(char::from).collect());
    }
    if type_ == atoms.utf8_string || type_ == atoms.text_plain_utf8 || type_ == atoms.text_plain {
        return String::from_utf8(value).ok();
    }
    None
}

/// One outstanding conversion. A second request while this exists is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pending {
    pub target: u32,
    pub deadline: Instant,
}

impl Pending {
    pub fn new(target: u32, now: Instant) -> Self {
        Self {
            target,
            deadline: now + REQUEST_TIMEOUT,
        }
    }

    pub fn expired(&self, now: Instant) -> bool {
        now >= self.deadline
    }

    /// An INCR chunk arrived. The peer is alive.
    pub fn touch(&mut self, now: Instant) {
        self.deadline = now + REQUEST_TIMEOUT;
    }
}

/// The next target to try after the owner refused `target`, if any.
///
/// UTF-8 first, then `STRING`. Nothing after `STRING`.
pub(crate) fn fallback(atoms: &Atoms, target: u32) -> Option<u32> {
    (target == atoms.utf8_string).then_some(STRING)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ATOMS: Atoms = Atoms {
        selection: 100,
        primary: 107,
        targets: 101,
        utf8_string: 102,
        text_plain_utf8: 103,
        text_plain: 104,
        incr: 105,
        property: 106,
        primary_property: 108,
    };

    #[test]
    fn selections_have_distinct_indexes_and_answer_events() {
        assert_ne!(Selection::Clipboard.index(), Selection::Primary.index());
        assert_eq!(
            Selection::Clipboard.answered(None),
            crate::Event::ClipboardText(None)
        );
        assert_eq!(
            Selection::Primary.answered(Some("x".into())),
            crate::Event::PrimaryText(Some("x".into()))
        );
    }

    #[test]
    fn nothing_owned_refuses_every_target() {
        assert_eq!(
            answer(&ATOMS, None, 0, 0, ATOMS.utf8_string, usize::MAX),
            Answer::Refuse
        );
        assert_eq!(
            answer(&ATOMS, None, 0, 0, ATOMS.targets, usize::MAX),
            Answer::Refuse
        );
    }

    #[test]
    fn targets_offer_string_only_for_ascii() {
        let Answer::Targets(ascii) = answer(&ATOMS, Some("12"), 0, 0, ATOMS.targets, usize::MAX)
        else {
            panic!("targets");
        };
        assert!(ascii.contains(&STRING));
        assert!(ascii.contains(&ATOMS.utf8_string));
        let Answer::Targets(wide) = answer(&ATOMS, Some("é"), 0, 0, ATOMS.targets, usize::MAX)
        else {
            panic!("targets");
        };
        assert!(!wide.contains(&STRING));
        assert_eq!(
            answer(&ATOMS, Some("é"), 0, 0, STRING, usize::MAX),
            Answer::Refuse
        );
    }

    #[test]
    fn utf8_is_served_with_its_own_type() {
        assert_eq!(
            answer(&ATOMS, Some("é"), 0, 0, ATOMS.utf8_string, usize::MAX),
            Answer::Text {
                type_: ATOMS.utf8_string,
                bytes: "é".as_bytes(),
            }
        );
    }

    #[test]
    fn a_request_older_than_ownership_is_refused() {
        assert_eq!(
            answer(&ATOMS, Some("1"), 50, 49, ATOMS.utf8_string, usize::MAX),
            Answer::Refuse
        );
        assert!(matches!(
            answer(&ATOMS, Some("1"), 50, 0, ATOMS.utf8_string, usize::MAX),
            Answer::Text { .. }
        ));
    }

    #[test]
    fn unknown_targets_empty_text_and_oversize_are_refused() {
        assert_eq!(
            answer(&ATOMS, Some("1"), 0, 0, 999, usize::MAX),
            Answer::Refuse
        );
        assert_eq!(
            answer(&ATOMS, Some(""), 0, 0, ATOMS.utf8_string, usize::MAX),
            Answer::Refuse
        );
        let big = "a".repeat(MAX_CLIPBOARD_BYTES + 1);
        assert_eq!(
            answer(&ATOMS, Some(&big), 0, 0, ATOMS.utf8_string, usize::MAX),
            Answer::Refuse
        );
    }

    #[test]
    fn text_over_the_single_limit_is_incr() {
        assert_eq!(
            answer(&ATOMS, Some("12345"), 0, 0, ATOMS.utf8_string, 4),
            Answer::Incr {
                type_: ATOMS.utf8_string,
                bytes: b"12345",
            }
        );
        assert!(matches!(
            answer(&ATOMS, Some("1234"), 0, 0, ATOMS.utf8_string, 4),
            Answer::Text { .. }
        ));
    }

    #[test]
    fn outgoing_chunks_then_ends_with_an_empty_chunk() {
        let now = Instant::now();
        let mut out = Outgoing::new(1, 2, ATOMS.utf8_string, b"12345", 2, now);
        assert_eq!(out.total(), 5);
        assert_eq!(out.next_chunk(now), Some(&b"12"[..]));
        assert_eq!(out.next_chunk(now), Some(&b"34"[..]));
        assert_eq!(out.next_chunk(now), Some(&b"5"[..]));
        assert!(!out.ended());
        assert_eq!(out.next_chunk(now), Some(&b""[..]));
        assert!(out.ended());
        assert_eq!(out.next_chunk(now), None);
    }

    #[test]
    fn outgoing_idles_out() {
        let now = Instant::now();
        let mut out = Outgoing::new(1, 2, ATOMS.utf8_string, b"1", 1, now);
        assert!(!out.expired(now));
        assert!(out.expired(now + TRANSFER_IDLE));
        let _ = out.next_chunk(now + TRANSFER_IDLE);
        assert!(!out.expired(now + TRANSFER_IDLE));
    }

    #[test]
    fn incoming_joins_chunks_split_inside_a_scalar() {
        let mut incoming = Incoming::default();
        let bytes = "aé".as_bytes();
        let t = ATOMS.utf8_string;
        assert_eq!(
            incoming.push(&ATOMS, t, 8, 0, bytes[..2].to_vec()),
            Progress::More
        );
        assert_eq!(
            incoming.push(&ATOMS, t, 8, 0, bytes[2..].to_vec()),
            Progress::More
        );
        assert_eq!(
            incoming.push(&ATOMS, t, 8, 0, Vec::new()),
            Progress::Done(Some("aé".to_owned()))
        );
    }

    #[test]
    fn incoming_poisons_on_overflow_type_change_and_partial_then_drains() {
        let t = ATOMS.utf8_string;
        let mut over = Incoming::default();
        assert_eq!(
            over.push(&ATOMS, t, 8, 0, vec![b'a'; MAX_CLIPBOARD_BYTES]),
            Progress::More
        );
        assert_eq!(over.push(&ATOMS, t, 8, 0, vec![b'a']), Progress::More);
        assert_eq!(over.push(&ATOMS, t, 8, 0, vec![b'a']), Progress::More);
        assert_eq!(over.push(&ATOMS, t, 8, 0, Vec::new()), Progress::Done(None));

        let mut mixed = Incoming::default();
        let _ = mixed.push(&ATOMS, t, 8, 0, b"1".to_vec());
        let _ = mixed.push(&ATOMS, STRING, 8, 0, b"2".to_vec());
        assert_eq!(
            mixed.push(&ATOMS, t, 8, 0, Vec::new()),
            Progress::Done(None)
        );

        let mut partial = Incoming::default();
        let _ = partial.push(&ATOMS, t, 8, 3, b"1".to_vec());
        assert_eq!(
            partial.push(&ATOMS, t, 8, 0, Vec::new()),
            Progress::Done(None)
        );
    }

    #[test]
    fn incoming_with_no_data_is_no_text() {
        let mut empty = Incoming::default();
        assert_eq!(
            empty.push(&ATOMS, ATOMS.utf8_string, 8, 0, Vec::new()),
            Progress::Done(None)
        );
    }

    #[test]
    fn decode_refuses_incr_partial_wrong_format_and_bad_utf8() {
        assert_eq!(decode(&ATOMS, ATOMS.incr, 32, 0, vec![0; 4]), None);
        assert_eq!(decode(&ATOMS, ATOMS.utf8_string, 8, 1, b"1".to_vec()), None);
        assert_eq!(
            decode(&ATOMS, ATOMS.utf8_string, 16, 0, b"12".to_vec()),
            None
        );
        assert_eq!(decode(&ATOMS, ATOMS.utf8_string, 8, 0, vec![0xff]), None);
        assert_eq!(decode(&ATOMS, ATOMS.utf8_string, 8, 0, Vec::new()), None);
        assert_eq!(decode(&ATOMS, 999, 8, 0, b"1".to_vec()), None);
        let big = vec![b'a'; MAX_CLIPBOARD_BYTES + 1];
        assert_eq!(decode(&ATOMS, ATOMS.utf8_string, 8, 0, big), None);
    }

    #[test]
    fn decode_reads_utf8_and_latin1() {
        assert_eq!(
            decode(&ATOMS, ATOMS.utf8_string, 8, 0, "é7".as_bytes().to_vec()),
            Some("é7".to_owned())
        );
        assert_eq!(
            decode(&ATOMS, STRING, 8, 0, vec![0xe9, b'7']),
            Some("é7".to_owned())
        );
    }

    #[test]
    fn fallback_is_utf8_then_string_then_nothing() {
        assert_eq!(fallback(&ATOMS, ATOMS.utf8_string), Some(STRING));
        assert_eq!(fallback(&ATOMS, STRING), None);
    }

    #[test]
    fn pending_expires_at_the_deadline() {
        let now = Instant::now();
        let pending = Pending::new(ATOMS.utf8_string, now);
        assert!(!pending.expired(now));
        assert!(pending.expired(now + REQUEST_TIMEOUT));
    }

    #[test]
    fn single_limit_equal_to_length_is_text_one_over_is_incr() {
        let text = "12345";
        assert!(matches!(
            answer(&ATOMS, Some(text), 0, 0, ATOMS.utf8_string, text.len()),
            Answer::Text { .. }
        ));
        assert_eq!(
            answer(&ATOMS, Some(text), 0, 0, ATOMS.utf8_string, text.len() + 1),
            Answer::Text {
                type_: ATOMS.utf8_string,
                bytes: text.as_bytes(),
            }
        );
        assert_eq!(
            answer(&ATOMS, Some(text), 0, 0, ATOMS.utf8_string, text.len() - 1),
            Answer::Incr {
                type_: ATOMS.utf8_string,
                bytes: text.as_bytes(),
            }
        );
    }

    #[test]
    fn tiny_single_limits_still_answer_targets_and_incr() {
        // A limit of 0 or 1 cannot carry any non-empty payload in one piece,
        // but TARGETS itself is a small atom list and must still be served.
        for limit in [0, 1] {
            assert!(matches!(
                answer(&ATOMS, Some("abc"), 0, 0, ATOMS.targets, limit),
                Answer::Targets(_)
            ));
            assert_eq!(
                answer(&ATOMS, Some("abc"), 0, 0, ATOMS.utf8_string, limit),
                Answer::Incr {
                    type_: ATOMS.utf8_string,
                    bytes: b"abc".as_slice(),
                }
            );
        }
        // A one-byte payload fits exactly in a limit of 1.
        assert!(matches!(
            answer(&ATOMS, Some("a"), 0, 0, ATOMS.utf8_string, 1),
            Answer::Text { .. }
        ));
    }

    #[test]
    fn targets_answer_even_when_conversion_would_refuse() {
        // Empty text: TARGETS is answered (the empty string is ASCII, so
        // `STRING` still appears in the list), but any text conversion refuses.
        let Answer::Targets(list) = answer(&ATOMS, Some(""), 0, 0, ATOMS.targets, usize::MAX)
        else {
            panic!("targets with empty text");
        };
        assert!(list.contains(&STRING));
        assert_eq!(
            answer(&ATOMS, Some(""), 0, 0, ATOMS.utf8_string, usize::MAX),
            Answer::Refuse
        );
        assert_eq!(
            answer(&ATOMS, Some(""), 0, 0, STRING, usize::MAX),
            Answer::Refuse
        );
        // Exactly the cap: still served, both by TARGETS and as text.
        let big = "a".repeat(MAX_CLIPBOARD_BYTES);
        assert!(matches!(
            answer(&ATOMS, Some(&big), 0, 0, ATOMS.targets, usize::MAX),
            Answer::Targets(_)
        ));
        assert!(matches!(
            answer(&ATOMS, Some(&big), 0, 0, STRING, usize::MAX),
            Answer::Text { .. }
        ));
    }

    #[test]
    fn outgoing_clamps_zero_chunk_to_one_byte() {
        let now = Instant::now();
        let mut out = Outgoing::new(1, 2, ATOMS.utf8_string, b"abc", 0, now);
        assert_eq!(out.next_chunk(now), Some(&b"a"[..]));
        assert_eq!(out.next_chunk(now), Some(&b"b"[..]));
        assert_eq!(out.next_chunk(now), Some(&b"c"[..]));
        assert_eq!(out.next_chunk(now), Some(&b""[..]));
        assert!(out.ended());
    }

    #[test]
    fn outgoing_splits_multibyte_scalars_at_chunk_edges() {
        // "é" is two bytes; a chunk of 1 splits it mid-scalar. The owner is a
        // byte pump, so the peer reassembles the scalar from both chunks.
        let now = Instant::now();
        let mut out = Outgoing::new(1, 2, ATOMS.utf8_string, "é".as_bytes(), 1, now);
        assert_eq!(out.next_chunk(now), Some(&[0xc3][..]));
        assert_eq!(out.next_chunk(now), Some(&[0xa9][..]));
        assert_eq!(out.next_chunk(now), Some(&b""[..]));

        // "aé3": chunk of 2 ends after the first byte of the scalar.
        let bytes = "aé3".as_bytes();
        let mut out = Outgoing::new(1, 2, ATOMS.utf8_string, bytes, 2, now);
        assert_eq!(out.next_chunk(now), Some(&bytes[..2]));
        assert_eq!(out.next_chunk(now), Some(&bytes[2..]));
        assert_eq!(out.next_chunk(now), Some(&b""[..]));
    }

    #[test]
    fn outgoing_exact_chunk_sized_payload_ends_after_one_data_chunk() {
        let now = Instant::now();
        let mut out = Outgoing::new(1, 2, ATOMS.utf8_string, b"abcd", 4, now);
        assert_eq!(out.next_chunk(now), Some(&b"abcd"[..]));
        assert!(!out.ended());
        assert_eq!(out.next_chunk(now), Some(&b""[..]));
        assert!(out.ended());
        assert_eq!(out.next_chunk(now), None);
    }

    #[test]
    fn incoming_bytes_after_on_first_chunk_poisons_then_drains() {
        let t = ATOMS.utf8_string;
        let mut incoming = Incoming::default();
        // A first chunk that claims more bytes behind it poisons at once.
        assert_eq!(
            incoming.push(&ATOMS, t, 8, 5, b"1".to_vec()),
            Progress::More
        );
        // Poisoned transfers keep draining data chunks as `More`.
        assert_eq!(
            incoming.push(&ATOMS, t, 8, 0, b"2".to_vec()),
            Progress::More
        );
        // The closing empty chunk resolves as no text.
        assert_eq!(
            incoming.push(&ATOMS, t, 8, 0, Vec::new()),
            Progress::Done(None)
        );
    }

    #[test]
    fn incoming_format_change_poisons_the_transfer() {
        let t = ATOMS.utf8_string;
        let mut incoming = Incoming::default();
        assert_eq!(
            incoming.push(&ATOMS, t, 8, 0, b"1".to_vec()),
            Progress::More
        );
        // A format-16 chunk of the same type is still a protocol break.
        assert_eq!(incoming.push(&ATOMS, t, 16, 0, vec![0; 4]), Progress::More);
        assert_eq!(
            incoming.push(&ATOMS, t, 8, 0, Vec::new()),
            Progress::Done(None)
        );
    }

    #[test]
    fn decode_string_maps_high_bytes_through_latin1() {
        let bytes: Vec<u8> = (0x80..=0xff).collect();
        let expected: String = bytes.iter().copied().map(char::from).collect();
        assert_eq!(decode(&ATOMS, STRING, 8, 0, bytes), Some(expected));
    }

    #[test]
    fn decode_accepts_text_exactly_at_the_cap() {
        let big = vec![b'a'; MAX_CLIPBOARD_BYTES];
        assert_eq!(
            decode(&ATOMS, ATOMS.utf8_string, 8, 0, big.clone()),
            Some("a".repeat(MAX_CLIPBOARD_BYTES))
        );
        assert_eq!(
            decode(&ATOMS, STRING, 8, 0, big),
            Some("a".repeat(MAX_CLIPBOARD_BYTES))
        );
    }

    #[test]
    fn only_utf8_string_falls_back_to_string() {
        assert_eq!(fallback(&ATOMS, ATOMS.utf8_string), Some(STRING));
        assert_eq!(fallback(&ATOMS, ATOMS.text_plain_utf8), None);
        assert_eq!(fallback(&ATOMS, ATOMS.text_plain), None);
        assert_eq!(fallback(&ATOMS, ATOMS.targets), None);
        assert_eq!(fallback(&ATOMS, ATOMS.incr), None);
    }

    #[test]
    fn pending_touch_pushes_the_deadline_forward() {
        let now = Instant::now();
        let mut pending = Pending::new(ATOMS.utf8_string, now);
        let later = now + REQUEST_TIMEOUT;
        assert!(pending.expired(later));
        pending.touch(later);
        assert!(!pending.expired(later));
        assert!(pending.expired(later + REQUEST_TIMEOUT));
    }
}
