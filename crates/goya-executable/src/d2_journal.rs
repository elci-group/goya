//! Bounded deterministic request/result journaling for declared D2 effects.
//!
//! This module only stores and replays bytes supplied by its caller. It never
//! performs network, filesystem, clock, random, or other host I/O.

use std::collections::BTreeSet;
use std::fmt;

use goya_determinism::proofs::ExecutionProof;
use goya_determinism::{classify, DeterminismClass, ProcessDecl};
use sha2::{Digest, Sha256};

/// Wire/chain format version.
pub const D2_JOURNAL_VERSION: u32 = 1;

const MAX_ENTRIES: usize = 100_000;
const MAX_FIELD_BYTES: usize = 1024 * 1024;
const MAX_JOURNAL_BYTES: usize = 32 * 1024 * 1024;
const DEFAULT_MAX_ENTRIES: usize = 10_000;
const DEFAULT_MAX_FIELD_BYTES: usize = 64 * 1024;
const DEFAULT_MAX_JOURNAL_BYTES: usize = 8 * 1024 * 1024;
const JOURNAL_DOMAIN: &[u8] = b"GOYA-D2-EFFECT-JOURNAL/1\0";
const GENESIS_DOMAIN: &[u8] = b"GOYA-D2-EFFECT-JOURNAL-GENESIS/1\0";

/// Validated, normalized capability label used to authorize journal entries.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapabilityLabel(String);

impl CapabilityLabel {
    /// Construct a lowercase ASCII label. Allowed bytes are `a-z`, `0-9`,
    /// `.`, `_`, `:`, and `-`; the label must be 1–128 bytes long.
    pub fn new(value: impl Into<String>) -> Result<Self, JournalError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 128
            || !value.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || matches!(b, b'.' | b'_' | b':' | b'-')
            })
        {
            return Err(JournalError::InvalidCapabilityLabel);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Hard-bounded journal capacity settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalLimits {
    max_entries: usize,
    max_field_bytes: usize,
    max_journal_bytes: usize,
}

impl JournalLimits {
    pub fn new(
        max_entries: usize,
        max_field_bytes: usize,
        max_journal_bytes: usize,
    ) -> Result<Self, JournalError> {
        if max_entries == 0
            || max_entries > MAX_ENTRIES
            || max_field_bytes == 0
            || max_field_bytes > MAX_FIELD_BYTES
            || max_journal_bytes == 0
            || max_journal_bytes > MAX_JOURNAL_BYTES
        {
            return Err(JournalError::InvalidLimits);
        }
        Ok(Self {
            max_entries,
            max_field_bytes,
            max_journal_bytes,
        })
    }

    pub fn max_entries(self) -> usize {
        self.max_entries
    }

    pub fn max_field_bytes(self) -> usize {
        self.max_field_bytes
    }

    pub fn max_journal_bytes(self) -> usize {
        self.max_journal_bytes
    }
}

impl Default for JournalLimits {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_MAX_ENTRIES,
            max_field_bytes: DEFAULT_MAX_FIELD_BYTES,
            max_journal_bytes: DEFAULT_MAX_JOURNAL_BYTES,
        }
    }
}

/// One immutable recorded host-effect request/result pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    pub sequence: u64,
    pub capability: CapabilityLabel,
    pub request: Vec<u8>,
    pub result: Vec<u8>,
    pub previous_hash: [u8; 32],
    pub hash: [u8; 32],
}

/// Errors from D2 declaration validation, bounded recording, or replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalError {
    InvalidCapabilityLabel,
    InvalidLimits,
    DeclarationRejected(String),
    NotEffectDeterministic,
    ProofProcessMismatch,
    ProofClassMismatch,
    GrantNotDeclared(String),
    UnauthorizedCapability(String),
    PayloadTooLarge,
    EntryLimit,
    JournalLimit,
    SequenceOverflow,
    LengthOverflow,
    ChainMismatch { sequence: u64 },
    ReplayExhausted,
    RequestMismatch { sequence: u64 },
    UnconsumedEntries { remaining: usize },
}

impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCapabilityLabel => f.write_str("invalid capability label"),
            Self::InvalidLimits => f.write_str("journal limits are outside hard bounds"),
            Self::DeclarationRejected(e) => write!(f, "D2 declaration rejected: {e}"),
            Self::NotEffectDeterministic => f.write_str("process is not classified D2"),
            Self::ProofProcessMismatch => f.write_str("proof process differs from declaration"),
            Self::ProofClassMismatch => {
                f.write_str("proof determinism class differs from declaration")
            }
            Self::GrantNotDeclared(label) => write!(f, "grant {label} is not declared by process"),
            Self::UnauthorizedCapability(label) => write!(f, "capability {label} is not granted"),
            Self::PayloadTooLarge => f.write_str("request or result exceeds the field limit"),
            Self::EntryLimit => f.write_str("journal entry limit reached"),
            Self::JournalLimit => f.write_str("journal byte limit reached"),
            Self::SequenceOverflow => f.write_str("journal sequence counter exhausted"),
            Self::LengthOverflow => f.write_str("journal encoded length overflow"),
            Self::ChainMismatch { sequence } => {
                write!(f, "journal hash chain mismatch at {sequence}")
            }
            Self::ReplayExhausted => f.write_str("journal has no next entry"),
            Self::RequestMismatch { sequence } => write!(f, "replay request differs at {sequence}"),
            Self::UnconsumedEntries { remaining } => {
                write!(f, "{remaining} journal entries remain unreplayed")
            }
        }
    }
}

impl std::error::Error for JournalError {}

/// A D2-only host-effect journal. Entries are caller-supplied data; this
/// type does not call a host service when recording or replaying.
#[derive(Debug, Clone)]
pub struct EffectJournal {
    grants: BTreeSet<CapabilityLabel>,
    limits: JournalLimits,
    entries: Vec<JournalEntry>,
    encoded_bytes: usize,
}

impl EffectJournal {
    /// Create a journal after verifying the declaration and execution proof.
    /// A grant must match an effect declared by the D2 process.
    pub fn new_d2(
        declaration: &ProcessDecl,
        proof: &ExecutionProof,
        grants: impl IntoIterator<Item = CapabilityLabel>,
        limits: JournalLimits,
    ) -> Result<Self, JournalError> {
        let verified = classify(declaration)
            .map_err(|err| JournalError::DeclarationRejected(err.to_string()))?;
        if verified.class != DeterminismClass::EffectDeterministic {
            return Err(JournalError::NotEffectDeterministic);
        }
        if proof.process != verified.process {
            return Err(JournalError::ProofProcessMismatch);
        }
        if proof.class != verified.class {
            return Err(JournalError::ProofClassMismatch);
        }
        let declared: BTreeSet<&str> = verified
            .declared_effects
            .iter()
            .map(|effect| effect.as_str())
            .collect();
        let mut allowed = BTreeSet::new();
        for grant in grants {
            if !declared.contains(grant.as_str()) {
                return Err(JournalError::GrantNotDeclared(grant.0));
            }
            allowed.insert(grant);
        }
        Ok(Self {
            grants: allowed,
            limits,
            entries: Vec::new(),
            encoded_bytes: 0,
        })
    }

    /// Restore persisted entries only after validating their sequence, grants,
    /// size bounds, previous hashes, and full SHA-256 chain.
    pub fn restore_d2(
        declaration: &ProcessDecl,
        proof: &ExecutionProof,
        grants: impl IntoIterator<Item = CapabilityLabel>,
        limits: JournalLimits,
        entries: Vec<JournalEntry>,
    ) -> Result<Self, JournalError> {
        let mut journal = Self::new_d2(declaration, proof, grants, limits)?;
        journal.entries = entries;
        journal.encoded_bytes = journal.verify_chain()?;
        Ok(journal)
    }

    /// Record a result already obtained by the caller. This method performs
    /// no I/O and does not infer or execute the effect.
    pub fn record(
        &mut self,
        capability: &CapabilityLabel,
        request: &[u8],
        result: &[u8],
    ) -> Result<&JournalEntry, JournalError> {
        if !self.grants.contains(capability) {
            return Err(JournalError::UnauthorizedCapability(
                capability.as_str().to_string(),
            ));
        }
        if request.len() > self.limits.max_field_bytes || result.len() > self.limits.max_field_bytes
        {
            return Err(JournalError::PayloadTooLarge);
        }
        if self.entries.len() >= self.limits.max_entries {
            return Err(JournalError::EntryLimit);
        }
        let sequence =
            u64::try_from(self.entries.len()).map_err(|_| JournalError::SequenceOverflow)?;
        let previous_hash = self
            .entries
            .last()
            .map_or_else(genesis_hash, |entry| entry.hash);
        let entry = JournalEntry {
            sequence,
            capability: capability.clone(),
            request: request.to_vec(),
            result: result.to_vec(),
            previous_hash,
            hash: [0; 32],
        };
        let encoded_len = encoded_len(&entry)?;
        let new_total = self
            .encoded_bytes
            .checked_add(encoded_len)
            .ok_or(JournalError::LengthOverflow)?;
        if new_total > self.limits.max_journal_bytes {
            return Err(JournalError::JournalLimit);
        }
        let mut entry = entry;
        entry.hash = entry_hash(&entry);
        self.entries.push(entry);
        self.encoded_bytes = new_total;
        Ok(self.entries.last().expect("entry inserted"))
    }

    pub fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }

    pub fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }

    /// Recheck the entire chain and produce a strict request-matching replay
    /// cursor. Entries cannot be consumed out of order.
    pub fn replay(&self) -> Result<JournalReplay<'_>, JournalError> {
        self.verify_chain()?;
        Ok(JournalReplay {
            journal: self,
            next: 0,
        })
    }

    fn verify_chain(&self) -> Result<usize, JournalError> {
        if self.entries.len() > self.limits.max_entries {
            return Err(JournalError::EntryLimit);
        }
        let mut total = 0usize;
        let mut previous = genesis_hash();
        for (index, entry) in self.entries.iter().enumerate() {
            let expected_sequence =
                u64::try_from(index).map_err(|_| JournalError::SequenceOverflow)?;
            if entry.sequence != expected_sequence
                || entry.previous_hash != previous
                || entry.hash != entry_hash(entry)
                || !self.grants.contains(&entry.capability)
                || entry.request.len() > self.limits.max_field_bytes
                || entry.result.len() > self.limits.max_field_bytes
            {
                return Err(JournalError::ChainMismatch {
                    sequence: expected_sequence,
                });
            }
            total = total
                .checked_add(encoded_len(entry)?)
                .ok_or(JournalError::LengthOverflow)?;
            if total > self.limits.max_journal_bytes {
                return Err(JournalError::JournalLimit);
            }
            previous = entry.hash;
        }
        Ok(total)
    }
}

/// Sequential verifier/replayer. Mismatches leave the cursor unchanged.
pub struct JournalReplay<'a> {
    journal: &'a EffectJournal,
    next: usize,
}

impl<'a> JournalReplay<'a> {
    /// Verify the next request and return its recorded result bytes. No host
    /// effect is executed. The cursor advances only on an exact match.
    pub fn replay(
        &mut self,
        capability: &CapabilityLabel,
        request: &[u8],
    ) -> Result<&'a [u8], JournalError> {
        let entry = self
            .journal
            .entries
            .get(self.next)
            .ok_or(JournalError::ReplayExhausted)?;
        if &entry.capability != capability || entry.request != request {
            return Err(JournalError::RequestMismatch {
                sequence: entry.sequence,
            });
        }
        self.next += 1;
        Ok(&entry.result)
    }

    /// Require that every journal entry was consumed.
    pub fn finish(self) -> Result<(), JournalError> {
        let remaining = self.journal.entries.len() - self.next;
        if remaining == 0 {
            Ok(())
        } else {
            Err(JournalError::UnconsumedEntries { remaining })
        }
    }
}

fn genesis_hash() -> [u8; 32] {
    Sha256::digest(GENESIS_DOMAIN).into()
}

fn entry_hash(entry: &JournalEntry) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(JOURNAL_DOMAIN);
    hasher.update(entry.previous_hash);
    hasher.update(entry.sequence.to_le_bytes());
    hash_field(&mut hasher, entry.capability.as_str().as_bytes());
    hash_field(&mut hasher, &entry.request);
    hash_field(&mut hasher, &entry.result);
    hasher.finalize().into()
}

fn hash_field(hasher: &mut Sha256, field: &[u8]) {
    hasher.update((field.len() as u64).to_le_bytes());
    hasher.update(field);
}

fn encoded_len(entry: &JournalEntry) -> Result<usize, JournalError> {
    [
        8usize,
        32,
        32,
        8,
        entry.capability.as_str().len(),
        8,
        entry.request.len(),
        8,
        entry.result.len(),
    ]
    .into_iter()
    .try_fold(0usize, |sum, n| {
        sum.checked_add(n).ok_or(JournalError::LengthOverflow)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_determinism::EffectKind;

    fn d2() -> (
        ProcessDecl,
        ExecutionProof,
        CapabilityLabel,
        CapabilityLabel,
    ) {
        let declaration = ProcessDecl::new(
            "renderer",
            DeterminismClass::EffectDeterministic,
            vec![EffectKind::Network, EffectKind::FileSystem],
        );
        let proof = ExecutionProof {
            process: "renderer".into(),
            class: DeterminismClass::EffectDeterministic,
            compiler_version: "0.1.0".into(),
            schema_version: 2,
            policy_version: 3,
            target: "goya-bytecode-v1".into(),
        };
        (
            declaration,
            proof,
            CapabilityLabel::new("network").unwrap(),
            CapabilityLabel::new("filesystem").unwrap(),
        )
    }

    #[test]
    fn records_and_replays_in_order_without_calling_hosts() {
        let (declaration, proof, network, filesystem) = d2();
        let mut journal = EffectJournal::new_d2(
            &declaration,
            &proof,
            [network.clone(), filesystem.clone()],
            JournalLimits::default(),
        )
        .unwrap();
        let first = journal
            .record(&network, b"GET /version", b"v1")
            .unwrap()
            .clone();
        let second = journal
            .record(&filesystem, b"read config", b"theme=dark")
            .unwrap()
            .clone();
        assert_eq!(first.sequence, 0);
        assert_eq!(second.sequence, 1);
        assert_eq!(second.previous_hash, first.hash);

        let mut replay = journal.replay().unwrap();
        assert_eq!(replay.replay(&network, b"GET /version").unwrap(), b"v1");
        assert_eq!(
            replay.replay(&filesystem, b"read config").unwrap(),
            b"theme=dark"
        );
        replay.finish().unwrap();
    }

    #[test]
    fn replay_mismatch_does_not_advance_cursor() {
        let (declaration, proof, network, filesystem) = d2();
        let mut journal = EffectJournal::new_d2(
            &declaration,
            &proof,
            [network.clone(), filesystem.clone()],
            JournalLimits::default(),
        )
        .unwrap();
        journal.record(&network, b"request", b"result").unwrap();
        let mut replay = journal.replay().unwrap();
        assert_eq!(
            replay.replay(&filesystem, b"request"),
            Err(JournalError::RequestMismatch { sequence: 0 })
        );
        assert_eq!(replay.replay(&network, b"request").unwrap(), b"result");
        replay.finish().unwrap();
    }

    #[test]
    fn restored_journal_detects_mutation_and_wrong_sequence() {
        let (declaration, proof, network, filesystem) = d2();
        let mut journal = EffectJournal::new_d2(
            &declaration,
            &proof,
            [network.clone(), filesystem],
            JournalLimits::default(),
        )
        .unwrap();
        journal.record(&network, b"request", b"result").unwrap();
        let mut entries = journal.entries().to_vec();
        entries[0].result[0] ^= 1;
        assert!(matches!(
            EffectJournal::restore_d2(
                &declaration,
                &proof,
                [network.clone()],
                JournalLimits::default(),
                entries,
            ),
            Err(JournalError::ChainMismatch { sequence: 0 })
        ));
    }

    #[test]
    fn validates_d2_classification_proof_and_granted_capabilities() {
        let (declaration, mut proof, network, filesystem) = d2();
        proof.process = "different".into();
        assert_eq!(
            EffectJournal::new_d2(
                &declaration,
                &proof,
                [network.clone()],
                JournalLimits::default(),
            )
            .err(),
            Some(JournalError::ProofProcessMismatch)
        );
        proof.process = "renderer".into();
        let undeclared = CapabilityLabel::new("clock").unwrap();
        assert_eq!(
            EffectJournal::new_d2(&declaration, &proof, [undeclared], JournalLimits::default(),)
                .err(),
            Some(JournalError::GrantNotDeclared("clock".into()))
        );
        assert!(EffectJournal::new_d2(
            &declaration,
            &proof,
            [network, filesystem],
            JournalLimits::default(),
        )
        .is_ok());
    }

    #[test]
    fn limits_bound_entries_fields_and_total_encoded_bytes() {
        let (declaration, proof, network, _) = d2();
        let limits = JournalLimits::new(1, 4, 128).unwrap();
        let mut journal =
            EffectJournal::new_d2(&declaration, &proof, [network.clone()], limits).unwrap();
        assert_eq!(
            journal.record(&network, b"12345", b"ok"),
            Err(JournalError::PayloadTooLarge)
        );
        journal.record(&network, b"req", b"res").unwrap();
        assert_eq!(
            journal.record(&network, b"req", b"res"),
            Err(JournalError::EntryLimit)
        );
        assert!(journal.encoded_bytes() <= limits.max_journal_bytes());
        assert_eq!(
            JournalLimits::new(1, 4, MAX_JOURNAL_BYTES + 1),
            Err(JournalError::InvalidLimits)
        );
    }

    #[test]
    fn capability_labels_are_canonical_ascii() {
        assert!(CapabilityLabel::new("network.read-1").is_ok());
        assert_eq!(
            CapabilityLabel::new("Network"),
            Err(JournalError::InvalidCapabilityLabel)
        );
        assert_eq!(
            CapabilityLabel::new(""),
            Err(JournalError::InvalidCapabilityLabel)
        );
    }
}
