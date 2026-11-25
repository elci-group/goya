//! One materialisation of a session.
//!
//! Portable fields are the only bytes in a snapshot. Local, ephemeral, and
//! private values stay on the replica that wrote them. A gap in the journal
//! is rejected; the caller installs a snapshot or the missing tail. The
//! replica does not buffer a hole and it does not invent a writer.

use crate::authority::{admitted_writer, decode_checkpoint, encode_checkpoint, HandoffRecord};
use crate::canonical::{decode, encode, fingerprint_bytes, Value};
use crate::diagnostic::{Diagnostic, Trace};
use crate::mutation::{
    apply_operations, decode_envelope, encode_envelope, seal, Apply, Envelope, Operation,
};
use crate::schema::Schema;
use crate::surface::Permissions;
use crate::Error;
use std::collections::BTreeMap;

pub const MAX_JOURNAL: usize = 64;
pub const MAX_PENDING: usize = 32;
const INITIAL_EPOCH: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Intent {
    pub action: u32,
    pub argument: u64,
    pub origin: [u8; 16],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    pub session: [u8; 16],
    pub from: [u8; 16],
    pub to: [u8; 16],
    pub epoch: u64,
    pub revision: u64,
    pub fingerprint: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub session: [u8; 16],
    pub schema_version: u32,
    pub schema_fingerprint: [u8; 32],
    pub epoch: u64,
    pub revision: u64,
    pub authority: [u8; 16],
    pub body: Vec<u8>,
}

#[derive(Clone, Debug)]
struct Seen {
    revision: u64,
    hash: [u8; 32],
}

#[derive(Clone, Debug)]
struct Stored {
    revision: u64,
    bytes: Vec<u8>,
}

struct DecodedSnap {
    session: [u8; 16],
    epoch: u64,
    revision: u64,
    authority: [u8; 16],
    values: BTreeMap<u32, Value>,
    body_fingerprint: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct Replica {
    schema: Schema,
    session: [u8; 16],
    me: [u8; 16],
    authority: [u8; 16],
    epoch: u64,
    revision: u64,
    values: BTreeMap<u32, Value>,
    local: BTreeMap<u32, Value>,
    journal: Vec<Stored>,
    compacted_through: u64,
    seen: BTreeMap<[u8; 16], Seen>,
    handoff_to: Option<[u8; 16]>,
    pending: Vec<Intent>,
    permissions: Permissions,
    trace: Trace,
}

impl Replica {
    pub fn open(
        schema: Schema,
        me: [u8; 16],
        session: [u8; 16],
        values: BTreeMap<u32, Value>,
    ) -> Result<Self, Error> {
        schema.check()?;
        let _ = portable_record(schema, &values)?;
        let mut replica = Self {
            schema,
            session,
            me,
            authority: me,
            epoch: INITIAL_EPOCH,
            revision: 0,
            values,
            local: BTreeMap::new(),
            journal: Vec::new(),
            compacted_through: 0,
            seen: BTreeMap::new(),
            handoff_to: None,
            pending: Vec::new(),
            permissions: Permissions::authority(),
            trace: Trace::new(),
        };
        replica.trace.record(Diagnostic::SessionCreated);
        Ok(replica)
    }

    pub fn join(
        schema: Schema,
        me: [u8; 16],
        permissions: Permissions,
        bytes: &[u8],
    ) -> Result<Self, Error> {
        schema.check()?;
        let decoded = decode_snapshot(schema, bytes)?;
        Ok(Self::from_decoded(schema, me, permissions, decoded))
    }

    pub fn restore(schema: Schema, me: [u8; 16], bytes: &[u8]) -> Result<Self, Error> {
        let (snapshot, record) = decode_checkpoint(bytes)?;
        let (writer, epoch) = admitted_writer(&record);
        let replica = Self::join(schema, me, Permissions::reader(), &snapshot)?;
        if replica.epoch != epoch
            || replica.authority != writer
            || replica.revision != record.revision()
        {
            return Err(Error::new(
                "RMT-E5008",
                "authority record does not match the snapshot; writes stay suspended",
            ));
        }
        if let HandoffRecord::Granted { fingerprint, .. } = record {
            if replica.fingerprint()? != fingerprint {
                return Err(Error::new(
                    "RMT-E5008",
                    "authority record does not match the snapshot; writes stay suspended",
                ));
            }
        }
        let party = match record {
            HandoffRecord::Prepared { from, to, .. } | HandoffRecord::Granted { from, to, .. } => {
                me == from || me == to
            }
        };
        if !party {
            return Err(Error::new(
                "RMT-E4006",
                "device is not a party to this handoff",
            ));
        }
        let mut replica = replica;
        replica.permissions = if me == writer {
            Permissions::authority()
        } else {
            Permissions::follower()
        };
        Ok(replica)
    }

    pub fn me(&self) -> [u8; 16] {
        self.me
    }

    pub fn session(&self) -> [u8; 16] {
        self.session
    }

    pub fn authority(&self) -> [u8; 16] {
        self.authority
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn permissions(&self) -> Permissions {
        self.permissions
    }

    /// Replace this replica's permission set.
    ///
    /// A non-authority cannot receive mutate or transfer. The active writer
    /// cannot be lowered to a controller or a reader by this call.
    pub fn adopt_permissions(&mut self, permissions: Permissions) -> Result<(), Error> {
        let grants_writer = permissions.mutate || permissions.transfer;
        if grants_writer && self.me != self.authority {
            return Err(Error::new(
                "RMT-E4006",
                "this surface cannot take the writer",
            ));
        }
        if !grants_writer && self.me == self.authority && self.permissions.mutate {
            return Err(Error::new(
                "RMT-E4006",
                "the active writer cannot take a read or controller surface",
            ));
        }
        self.permissions = permissions;
        Ok(())
    }

    pub fn values(&self) -> &BTreeMap<u32, Value> {
        &self.values
    }

    pub fn local(&self, field: u32) -> Option<&Value> {
        self.local.get(&field)
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_writer(&self) -> bool {
        self.me == self.authority && self.permissions.mutate
    }

    /// Process-local outcomes. Absent from the snapshot, the journal, and the wire.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.trace.as_slice()
    }

    pub fn fingerprint(&self) -> Result<[u8; 32], Error> {
        let body = encode(&portable_record(self.schema, &self.values)?)?;
        Ok(fingerprint_bytes(&body))
    }

    pub fn set_local(&mut self, field: u32, value: Value) -> Result<(), Error> {
        let definition = self.schema.get(field).ok_or(Error::new(
            "RMT-E2002",
            "local write names a field outside the schema",
        ))?;
        if !definition.class.is_device_local() {
            return Err(Error::new(
                "RMT-E2003",
                "portable or derived state cannot be written as device-local",
            ));
        }
        self.local.insert(field, value);
        Ok(())
    }

    pub fn commit_authority<F>(
        &mut self,
        operations: Vec<Operation>,
        invariant: F,
    ) -> Result<Envelope, Error>
    where
        F: FnOnce(&BTreeMap<u32, Value>) -> bool,
    {
        if self.me != self.authority || !self.permissions.mutate {
            return self.reject(Error::new(
                "RMT-E5005",
                "this replica is not the active authority",
            ));
        }
        if self.handoff_to.is_some() {
            return self.reject(Error::new(
                "RMT-E5006",
                "writes are suspended while a handoff is prepared",
            ));
        }
        let next = match apply_operations(self.schema, &self.values, &operations) {
            Ok(next) => next,
            Err(error) => return self.reject(error),
        };
        if !invariant(&next) {
            return self.reject(Error::new(
                "RMT-E5004",
                "transaction rejected by the application invariant",
            ));
        }
        let envelope = match seal(
            self.session,
            self.me,
            self.epoch,
            self.revision,
            self.schema.version,
            operations,
        ) {
            Ok(envelope) => envelope,
            Err(error) => return self.reject(error),
        };
        let bytes = match encode_envelope(&envelope) {
            Ok(bytes) => bytes,
            Err(error) => return self.reject(error),
        };
        let hash = fingerprint_bytes(&bytes);
        self.values = next;
        self.revision = envelope.revision;
        self.remember(envelope.mutation, envelope.revision, hash, bytes);
        self.trace.record(Diagnostic::MutationCommitted {
            revision: self.revision,
        });
        Ok(envelope)
    }

    pub fn apply_remote<F>(&mut self, bytes: &[u8], invariant: F) -> Result<Apply, Error>
    where
        F: FnOnce(&BTreeMap<u32, Value>) -> bool,
    {
        let envelope = match decode_envelope(bytes) {
            Ok(envelope) => envelope,
            Err(error) => return self.reject(error),
        };
        if envelope.session != self.session {
            return self.reject(Error::new(
                "RMT-E5002",
                "mutation belongs to a different session",
            ));
        }
        if envelope.schema_version != self.schema.version {
            return self.reject(Error::new(
                "RMT-E2002",
                "mutation schema version does not match",
            ));
        }
        if envelope.epoch != self.epoch || envelope.origin != self.authority {
            return self.reject(Error::new(
                "RMT-E5001",
                "mutation epoch is not the active authority epoch",
            ));
        }
        let hash = fingerprint_bytes(bytes);
        let prior = self
            .seen
            .get(&envelope.mutation)
            .map(|seen| (seen.hash, seen.revision));
        if let Some((prior_hash, revision)) = prior {
            if prior_hash == hash {
                self.trace
                    .record(Diagnostic::MutationDeduplicated { revision });
                return Ok(Apply::Duplicate);
            }
            return self.reject(Error::new(
                "RMT-E5002",
                "mutation identity was already committed with a different body",
            ));
        }
        if envelope.revision <= self.compacted_through {
            self.trace.record(Diagnostic::MutationStale {
                revision: envelope.revision,
            });
            return Ok(Apply::Stale);
        }
        if envelope.revision <= self.revision {
            return self.reject(Error::new(
                "RMT-E5002",
                "revision is inside the retained window but the mutation is unknown",
            ));
        }
        if envelope.base_revision != self.revision {
            return self.reject(Error::new(
                "RMT-E5003",
                "mutation does not extend the committed revision",
            ));
        }
        let next = match apply_operations(self.schema, &self.values, &envelope.operations) {
            Ok(next) => next,
            Err(error) => return self.reject(error),
        };
        if !invariant(&next) {
            return self.reject(Error::new(
                "RMT-E5004",
                "transaction rejected by the application invariant",
            ));
        }
        self.values = next;
        self.revision = envelope.revision;
        self.remember(envelope.mutation, envelope.revision, hash, bytes.to_vec());
        let revision = self.revision;
        if self.is_writer() {
            self.trace
                .record(Diagnostic::MutationCommitted { revision });
        } else {
            self.trace
                .record(Diagnostic::MutationAcknowledged { revision });
        }
        Ok(Apply::Applied)
    }

    pub fn journal_after(&self, revision: u64) -> Result<Vec<Vec<u8>>, Error> {
        if revision > self.revision || revision < self.compacted_through {
            return Err(Error::new(
                "RMT-E6002",
                "replica is outside the retained journal window",
            ));
        }
        if revision < self.revision {
            let mut expect = revision + 1;
            for entry in &self.journal {
                if entry.revision < expect {
                    continue;
                }
                if entry.revision != expect {
                    return Err(Error::new(
                        "RMT-E6002",
                        "replica is outside the retained journal window",
                    ));
                }
                expect += 1;
            }
            if expect != self.revision + 1 {
                return Err(Error::new(
                    "RMT-E6002",
                    "replica is outside the retained journal window",
                ));
            }
        }
        Ok(self
            .journal
            .iter()
            .filter(|entry| entry.revision > revision)
            .map(|entry| entry.bytes.clone())
            .collect())
    }

    /// Canonical portable state. Local, ephemeral, private, and derived fields are absent.
    pub fn portable_document(&self) -> Result<Vec<u8>, Error> {
        encode(&portable_record(self.schema, &self.values)?)
    }

    pub fn encode_snapshot(&self) -> Result<Vec<u8>, Error> {
        let body = self.portable_document()?;
        let schema_fingerprint = self.schema.fingerprint()?;
        let len = u32::try_from(body.len())
            .map_err(|_| Error::new("RMT-E8001", "snapshot exceeds the byte budget"))?;
        let mut out = Vec::new();
        out.extend_from_slice(b"RSN1");
        out.extend_from_slice(&self.session);
        out.extend_from_slice(&self.schema.version.to_le_bytes());
        out.extend_from_slice(&schema_fingerprint);
        out.extend_from_slice(&self.epoch.to_le_bytes());
        out.extend_from_slice(&self.revision.to_le_bytes());
        out.extend_from_slice(&self.authority);
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&body);
        if out.len() > crate::canonical::MAX_BYTES {
            return Err(Error::new("RMT-E8001", "snapshot exceeds the byte budget"));
        }
        Ok(out)
    }

    pub fn install_snapshot(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let decoded = match decode_snapshot(self.schema, bytes) {
            Ok(decoded) => decoded,
            Err(error) => return self.reject_snapshot(error),
        };
        if decoded.session != self.session {
            return self.reject_snapshot(Error::new(
                "RMT-E5002",
                "snapshot belongs to a different session",
            ));
        }
        self.authority = decoded.authority;
        self.epoch = decoded.epoch;
        self.revision = decoded.revision;
        self.values = decoded.values;
        self.journal.clear();
        self.seen.clear();
        self.compacted_through = self.revision;
        self.handoff_to = None;
        self.trace.record(Diagnostic::SnapshotApplied {
            revision: self.revision,
        });
        Ok(())
    }

    pub fn prepare_handoff(&mut self, to: [u8; 16]) -> Result<(), Error> {
        if self.me != self.authority || !self.permissions.transfer {
            return Err(Error::new(
                "RMT-E5005",
                "only the active authority can prepare a handoff",
            ));
        }
        if to == self.me {
            return Err(Error::new(
                "RMT-E5007",
                "authority cannot be handed to itself",
            ));
        }
        if self.handoff_to.is_some() {
            return Err(Error::new("RMT-E5006", "a handoff is already in progress"));
        }
        self.handoff_to = Some(to);
        self.trace.record(Diagnostic::AuthorityTransferStarted);
        Ok(())
    }

    pub fn abort_handoff(&mut self) -> Result<(), Error> {
        if self.me != self.authority {
            return Err(Error::new(
                "RMT-E5005",
                "only the active authority can abort a handoff",
            ));
        }
        if self.handoff_to.take().is_none() {
            return Err(Error::new("RMT-E5007", "no handoff is in progress"));
        }
        self.trace.record(Diagnostic::AuthorityTransferFailed);
        Ok(())
    }

    pub fn prepared_record(&self) -> Result<HandoffRecord, Error> {
        let to = self
            .handoff_to
            .ok_or(Error::new("RMT-E5007", "no handoff is in progress"))?;
        Ok(HandoffRecord::Prepared {
            from: self.me,
            to,
            epoch: self.epoch,
            revision: self.revision,
        })
    }

    pub fn grant(&mut self, to: [u8; 16]) -> Result<Grant, Error> {
        if self.handoff_to != Some(to) {
            return Err(Error::new(
                "RMT-E5007",
                "handoff grant does not match the prepared device",
            ));
        }
        let fingerprint = self.fingerprint()?;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or(Error::new("RMT-E8001", "authority epoch would overflow"))?;
        let grant = Grant {
            session: self.session,
            from: self.me,
            to,
            epoch,
            revision: self.revision,
            fingerprint,
        };
        self.epoch = epoch;
        self.authority = to;
        self.handoff_to = None;
        self.permissions.mutate = false;
        self.permissions.transfer = false;
        self.trace
            .record(Diagnostic::AuthorityTransferCompleted { epoch: self.epoch });
        Ok(grant)
    }

    pub fn activate(&mut self, grant: &Grant, snapshot: &[u8]) -> Result<(), Error> {
        if grant.to != self.me {
            return self.fail_transfer(Error::new(
                "RMT-E5007",
                "handoff grant names a different device",
            ));
        }
        let decoded = match decode_snapshot(self.schema, snapshot) {
            Ok(decoded) => decoded,
            Err(error) => return self.reject_snapshot(error),
        };
        if decoded.session != grant.session
            || decoded.session != self.session
            || decoded.epoch != grant.epoch
            || decoded.revision != grant.revision
            || decoded.authority != self.me
            || decoded.body_fingerprint != grant.fingerprint
        {
            return self.reject_snapshot(Error::new(
                "RMT-E5008",
                "handoff snapshot does not match the grant",
            ));
        }
        self.authority = decoded.authority;
        self.epoch = decoded.epoch;
        self.revision = decoded.revision;
        self.values = decoded.values;
        self.journal.clear();
        self.seen.clear();
        self.compacted_through = self.revision;
        self.handoff_to = None;
        self.permissions = Permissions::authority();
        self.trace.record(Diagnostic::SnapshotApplied {
            revision: self.revision,
        });
        self.trace
            .record(Diagnostic::AuthorityTransferCompleted { epoch: self.epoch });
        Ok(())
    }

    pub fn granted_record(grant: &Grant) -> HandoffRecord {
        HandoffRecord::Granted {
            from: grant.from,
            to: grant.to,
            epoch: grant.epoch,
            revision: grant.revision,
            fingerprint: grant.fingerprint,
        }
    }

    pub fn checkpoint(snapshot: &[u8], record: &HandoffRecord) -> Result<Vec<u8>, Error> {
        encode_checkpoint(snapshot, record)
    }

    pub fn enqueue_intent(&mut self, intent: Intent) -> Result<(), Error> {
        if !self.permissions.submit_intent {
            return Err(Error::new(
                "RMT-E4006",
                "this surface cannot submit intents",
            ));
        }
        if self.pending.len() >= MAX_PENDING {
            return Err(Error::new("RMT-E8001", "pending intent queue is full"));
        }
        self.pending.push(intent);
        Ok(())
    }

    pub fn acknowledge_pending(&mut self, action: u32, argument: u64) -> bool {
        if let Some(index) = self
            .pending
            .iter()
            .position(|intent| intent.action == action && intent.argument == argument)
        {
            self.pending.remove(index);
            true
        } else {
            false
        }
    }

    fn reject<T>(&mut self, error: Error) -> Result<T, Error> {
        self.trace
            .record(Diagnostic::MutationRejected { code: error.code });
        Err(error)
    }

    fn reject_snapshot<T>(&mut self, error: Error) -> Result<T, Error> {
        self.trace
            .record(Diagnostic::SnapshotRejected { code: error.code });
        Err(error)
    }

    fn fail_transfer<T>(&mut self, error: Error) -> Result<T, Error> {
        self.trace.record(Diagnostic::AuthorityTransferFailed);
        Err(error)
    }

    fn remember(&mut self, id: [u8; 16], revision: u64, hash: [u8; 32], bytes: Vec<u8>) {
        self.seen.insert(id, Seen { revision, hash });
        self.journal.push(Stored { revision, bytes });
        while self.journal.len() > MAX_JOURNAL {
            let dropped = self.journal.remove(0);
            self.compacted_through = dropped.revision;
            self.seen
                .retain(|_, seen| seen.revision > self.compacted_through);
        }
    }

    fn from_decoded(
        schema: Schema,
        me: [u8; 16],
        permissions: Permissions,
        decoded: DecodedSnap,
    ) -> Self {
        let revision = decoded.revision;
        let mut replica = Self {
            schema,
            session: decoded.session,
            me,
            authority: decoded.authority,
            epoch: decoded.epoch,
            revision,
            values: decoded.values,
            local: BTreeMap::new(),
            journal: Vec::new(),
            compacted_through: revision,
            seen: BTreeMap::new(),
            handoff_to: None,
            pending: Vec::new(),
            permissions,
            trace: Trace::new(),
        };
        replica.trace.record(Diagnostic::SessionJoined);
        replica
    }
}

fn portable_record(schema: Schema, values: &BTreeMap<u32, Value>) -> Result<Value, Error> {
    let mut record = BTreeMap::new();
    for field in schema.fields {
        if !field.class.is_portable() {
            continue;
        }
        let value = values.get(&field.id).cloned().ok_or(Error::new(
            "RMT-E2001",
            "portable field is missing from the replica",
        ))?;
        record.insert(field.id, value);
    }
    for id in values.keys() {
        match schema.get(*id) {
            Some(field) if field.class.is_portable() => {}
            Some(_) => {
                return Err(Error::new(
                    "RMT-E2003",
                    "replica contains state that is not portable",
                ));
            }
            None => {
                return Err(Error::new(
                    "RMT-E2002",
                    "snapshot contains an unknown field",
                ));
            }
        }
    }
    Ok(Value::Record(record))
}

fn decode_snapshot(schema: Schema, bytes: &[u8]) -> Result<DecodedSnap, Error> {
    if bytes.len() < 4 + 16 + 4 + 32 + 8 + 8 + 16 + 4 || &bytes[..4] != b"RSN1" {
        return Err(Error::new("RMT-E6001", "snapshot is truncated or corrupt"));
    }
    let mut offset = 4;
    let session = read_array::<16>(bytes, &mut offset)?;
    let schema_version = read_u32(bytes, &mut offset)?;
    let schema_fingerprint = read_array::<32>(bytes, &mut offset)?;
    let epoch = read_u64(bytes, &mut offset)?;
    let revision = read_u64(bytes, &mut offset)?;
    let authority = read_array::<16>(bytes, &mut offset)?;
    let len = read_u32(bytes, &mut offset)? as usize;
    if schema_version != schema.version || schema_fingerprint != schema.fingerprint()? {
        return Err(Error::new(
            "RMT-E2002",
            "peer schema is incompatible and no migration is registered",
        ));
    }
    if epoch == 0 || offset.checked_add(len) != Some(bytes.len()) {
        return Err(Error::new("RMT-E6001", "snapshot is truncated or corrupt"));
    }
    let body = &bytes[offset..];
    let value = decode(body)?;
    if encode(&value)?.as_slice() != body {
        return Err(Error::new("RMT-E2001", "snapshot body is not canonical"));
    }
    let values = values_from_body(schema, &value)?;
    Ok(DecodedSnap {
        session,
        epoch,
        revision,
        authority,
        values,
        body_fingerprint: fingerprint_bytes(body),
    })
}

fn values_from_body(schema: Schema, body: &Value) -> Result<BTreeMap<u32, Value>, Error> {
    let Value::Record(fields) = body else {
        return Err(Error::new("RMT-E2001", "snapshot body is not a record"));
    };
    let mut values = BTreeMap::new();
    for (id, value) in fields {
        let definition = schema.get(*id).ok_or(Error::new(
            "RMT-E2002",
            "snapshot contains an unknown field",
        ))?;
        if !definition.class.is_portable() {
            return Err(Error::new(
                "RMT-E2003",
                "snapshot contains state that is not replicated",
            ));
        }
        values.insert(*id, value.clone());
    }
    for field in schema.fields {
        if field.class.is_portable() && !values.contains_key(&field.id) {
            return Err(Error::new(
                "RMT-E2001",
                "portable field is missing from the replica",
            ));
        }
    }
    Ok(values)
}

fn read_array<const N: usize>(bytes: &[u8], offset: &mut usize) -> Result<[u8; N], Error> {
    let end = offset
        .checked_add(N)
        .ok_or(Error::new("RMT-E6001", "snapshot is truncated or corrupt"))?;
    if end > bytes.len() {
        return Err(Error::new("RMT-E6001", "snapshot is truncated or corrupt"));
    }
    let mut raw = [0u8; N];
    raw.copy_from_slice(&bytes[*offset..end]);
    *offset = end;
    Ok(raw)
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(read_array::<4>(bytes, offset)?))
}

fn read_u64(bytes: &[u8], offset: &mut usize) -> Result<u64, Error> {
    Ok(u64::from_le_bytes(read_array::<8>(bytes, offset)?))
}
