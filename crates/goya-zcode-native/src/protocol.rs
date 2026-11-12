//! ZCode v4 wire and conversation projection adapter.
//!
//! This module deliberately owns no transport IO or input queue. The agent
//! thread feeds physical frames into this bounded adapter and sends its
//! derived display lines to `SessionView`.

use crate::view::{
    WorkflowActiveWork, WorkflowAvailability, WorkflowBackgroundWork, WorkflowControlState,
    WorkflowGoal, WorkflowPlanItem,
};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{BuildHasher, RandomState};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub const V4_WIRE_PROTOCOL_VERSION: u64 = 3;
pub const MAX_ASSEMBLY_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_ASSEMBLIES: usize = 32;
pub const MAX_STAGED_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_FRAGMENTS: usize = 1024;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_ROWS: usize = 2000;
pub const MAX_QUERY_COMMANDS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolError(pub String);

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProtocolError {}

type Result<T> = std::result::Result<T, ProtocolError>;

fn error(code: &str) -> ProtocolError {
    ProtocolError(code.to_string())
}

fn object<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| error(&format!("proto.{context}.notObject")))
}

fn check_keys(value: &Value, required: &[&str], allowed: &[&str], context: &str) -> Result<()> {
    let map = object(value, context)?;
    for key in required {
        if !map.contains_key(*key) {
            return Err(error(&format!("proto.{context}.missing.{key}")));
        }
    }
    if map.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(error(&format!("proto.{context}.unknownField")));
    }
    Ok(())
}

fn string<'a>(value: &'a Value, context: &str) -> Result<&'a str> {
    value
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| error(&format!("proto.{context}.invalidString")))
}

fn nonnegative_u64(value: &Value, context: &str) -> Result<u64> {
    value
        .as_u64()
        .ok_or_else(|| error(&format!("proto.{context}.invalidInteger")))
}

fn protocol_version(value: &Value) -> Result<()> {
    if value.as_u64() == Some(V4_WIRE_PROTOCOL_VERSION) {
        Ok(())
    } else {
        Err(error("proto.wireVersion.unsupported"))
    }
}

/// Workspace topic key identity rule shared with the desktop Host.
pub fn workspace_key(workspace_path: &str, workspace_identity: Option<&str>) -> String {
    workspace_identity
        .map(str::trim)
        .filter(|identity| !identity.is_empty())
        .unwrap_or(workspace_path)
        .to_string()
}

/// Build one strict v4 subscribe request. Goya's local app-server connection
/// is trusted as desktop-continuous; the UI never chooses this profile.
pub fn subscribe_params(
    topic: &str,
    connection_id: &str,
    workspace_path: &str,
    workspace_key: &str,
) -> Value {
    json!({
        "topic": topic,
        "connectionId": connection_id,
        "clientMode": "desktop-continuous",
        "workspace": { "workspacePath": workspace_path, "workspaceKey": workspace_key },
    })
}

/// Parse the v4 subscribe ACK; the initial frame is a later notification.
pub fn parse_subscribe_ack(value: &Value) -> Result<SubscribeAck> {
    check_keys(value, &["ack"], &["ack"], "subscribeResult")?;
    let ack = &value["ack"];
    check_keys(
        ack,
        &["subscriptionId", "mode", "logEpoch"],
        &["subscriptionId", "mode", "logEpoch", "openTiming"],
        "subscribeAck",
    )?;
    let mode = string(&ack["mode"], "subscribeAck.mode")?;
    if mode != "snapshot" && mode != "resume" {
        return Err(error("proto.subscribeAck.invalidMode"));
    }
    Ok(SubscribeAck {
        subscription_id: string(&ack["subscriptionId"], "subscribeAck.subscriptionId")?.into(),
        mode: mode.into(),
        log_epoch: string(&ack["logEpoch"], "subscribeAck.logEpoch")?.into(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscribeAck {
    pub subscription_id: String,
    pub mode: String,
    pub log_epoch: String,
}

#[derive(Clone, Debug)]
struct FragmentAssembly {
    wire_version: u64,
    delivery_kind: String,
    ordinal: u64,
    topic: String,
    subscription_id: String,
    count: usize,
    logical_bytes: usize,
    checksum: String,
    pieces: Vec<Option<Vec<u8>>>,
    staged_bytes: usize,
}

/// Reassembles bounded physical frames. Incomplete assemblies return `None`;
/// callers must keep them staged and must never advance the projection cursor.
#[derive(Debug, Default)]
pub struct FrameAssembler {
    pending: HashMap<String, FragmentAssembly>,
    staged_bytes: usize,
    last_ordinal: HashMap<(String, String), u64>,
    reanchor: HashSet<(String, String)>,
}

impl FrameAssembler {
    pub fn push(&mut self, wire: &Value) -> Result<Option<Value>> {
        let physical_bytes = serde_json::to_vec(wire)
            .map_err(|_| error("proto.wire.invalidJson"))?
            .len();
        if physical_bytes > MAX_FRAME_BYTES {
            return Err(error("proto.frameEnvelopeTooLarge"));
        }
        let kind = string(&wire["kind"], "wire.kind")?;
        match kind {
            "complete" => self.complete(wire).map(Some),
            "fragment" => self.fragment(wire),
            _ => Err(error("proto.wire.invalidKind")),
        }
    }

    fn check_outer(&self, wire: &Value, kind: &str) -> Result<()> {
        let (required, allowed): (&[&str], &[&str]) = if kind == "complete" {
            (
                &[
                    "wireVersion",
                    "kind",
                    "deliveryKind",
                    "logicalFrameId",
                    "logicalFrameOrdinal",
                    "topic",
                    "subscriptionId",
                    "frame",
                ],
                &[
                    "wireVersion",
                    "kind",
                    "deliveryKind",
                    "logicalFrameId",
                    "logicalFrameOrdinal",
                    "topic",
                    "subscriptionId",
                    "frame",
                ],
            )
        } else {
            (
                &[
                    "wireVersion",
                    "kind",
                    "deliveryKind",
                    "logicalFrameId",
                    "logicalFrameOrdinal",
                    "topic",
                    "subscriptionId",
                    "fragmentIndex",
                    "fragmentCount",
                    "logicalBytes",
                    "checksum",
                    "dataBase64",
                ],
                &[
                    "wireVersion",
                    "kind",
                    "deliveryKind",
                    "logicalFrameId",
                    "logicalFrameOrdinal",
                    "topic",
                    "subscriptionId",
                    "fragmentIndex",
                    "fragmentCount",
                    "logicalBytes",
                    "checksum",
                    "dataBase64",
                ],
            )
        };
        check_keys(wire, required, allowed, "wire")?;
        protocol_version(&wire["wireVersion"])?;
        let delivery = string(&wire["deliveryKind"], "wire.deliveryKind")?;
        if !matches!(delivery, "initial" | "online" | "recovery") {
            return Err(error("proto.wire.invalidDeliveryKind"));
        }
        let ordinal = nonnegative_u64(&wire["logicalFrameOrdinal"], "wire.logicalFrameOrdinal")?;
        if ordinal == 0 {
            return Err(error("proto.wire.invalidOrdinal"));
        }
        let topic = string(&wire["topic"], "wire.topic")?;
        let sub = string(&wire["subscriptionId"], "wire.subscriptionId")?;
        let logical_id = string(&wire["logicalFrameId"], "wire.logicalFrameId")?;
        if topic.len() > 2048 || sub.len() > 1024 || logical_id.len() > 256 {
            return Err(error("proto.wire.metadataTooLarge"));
        }
        Ok(())
    }

    fn complete(&mut self, wire: &Value) -> Result<Value> {
        self.check_outer(wire, "complete")?;
        let frame = wire["frame"].clone();
        let bytes = serde_json::to_vec(&frame).map_err(|_| error("proto.frame.invalidJson"))?;
        if bytes.len() > MAX_ASSEMBLY_BYTES {
            return Err(error("proto.frameAssemblyTooLarge"));
        }
        validate_frame_envelope(&frame, &wire["topic"], &wire["subscriptionId"])?;
        self.settle_ordinal(
            wire["topic"].as_str().unwrap_or_default(),
            wire["subscriptionId"].as_str().unwrap_or_default(),
            nonnegative_u64(&wire["logicalFrameOrdinal"], "wire.logicalFrameOrdinal")?,
        )?;
        Ok(frame)
    }

    fn fragment(&mut self, wire: &Value) -> Result<Option<Value>> {
        self.check_outer(wire, "fragment")?;
        let id = string(&wire["logicalFrameId"], "wire.logicalFrameId")?.to_string();
        let index = usize::try_from(nonnegative_u64(
            &wire["fragmentIndex"],
            "wire.fragmentIndex",
        )?)
        .map_err(|_| error("proto.frameFragmentCountExceeded"))?;
        let count = usize::try_from(nonnegative_u64(
            &wire["fragmentCount"],
            "wire.fragmentCount",
        )?)
        .map_err(|_| error("proto.frameFragmentCountExceeded"))?;
        let logical_bytes =
            usize::try_from(nonnegative_u64(&wire["logicalBytes"], "wire.logicalBytes")?)
                .map_err(|_| error("proto.frameAssemblyTooLarge"))?;
        if count == 0
            || count > MAX_FRAGMENTS
            || index >= count
            || logical_bytes == 0
            || logical_bytes > MAX_ASSEMBLY_BYTES
            || count > logical_bytes
        {
            return Err(error("proto.frameFragmentCountExceeded"));
        }
        let checksum = string(&wire["checksum"]["value"], "wire.checksum.value")?.to_string();
        if wire["checksum"]["algorithm"].as_str() != Some("crc32")
            || checksum.len() != 8
            || !checksum
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(error("proto.frameAssemblyInvalidChecksum"));
        }
        let encoded = string(&wire["dataBase64"], "wire.dataBase64")?;
        if encoded.len() > MAX_FRAME_BYTES {
            return Err(error("proto.frameEnvelopeTooLarge"));
        }
        let piece = decode_base64(encoded)?;
        if piece.len() > MAX_FRAME_BYTES || piece.is_empty() {
            return Err(error("proto.frameAssemblyTooLarge"));
        }
        if !self.pending.contains_key(&id) {
            if self.pending.len() >= MAX_ASSEMBLIES {
                return Err(error("proto.frameAssemblyConcurrencyExceeded"));
            }
            self.pending.insert(
                id.clone(),
                FragmentAssembly {
                    wire_version: V4_WIRE_PROTOCOL_VERSION,
                    delivery_kind: wire["deliveryKind"].as_str().unwrap().to_string(),
                    ordinal: wire["logicalFrameOrdinal"].as_u64().unwrap(),
                    topic: wire["topic"].as_str().unwrap().to_string(),
                    subscription_id: wire["subscriptionId"].as_str().unwrap().to_string(),
                    count,
                    logical_bytes,
                    checksum: checksum.clone(),
                    pieces: vec![None; count],
                    staged_bytes: 0,
                },
            );
        }
        let assembly = self.pending.get_mut(&id).expect("inserted above");
        if assembly.wire_version != V4_WIRE_PROTOCOL_VERSION
            || assembly.delivery_kind != wire["deliveryKind"].as_str().unwrap_or_default()
            || assembly.ordinal != wire["logicalFrameOrdinal"].as_u64().unwrap_or_default()
            || assembly.topic != wire["topic"].as_str().unwrap_or_default()
            || assembly.subscription_id != wire["subscriptionId"].as_str().unwrap_or_default()
            || assembly.count != count
            || assembly.logical_bytes != logical_bytes
            || assembly.checksum != checksum
        {
            self.discard(&id);
            return Err(error("proto.frameAssemblyMetadataMismatch"));
        }
        if let Some(previous) = &assembly.pieces[index] {
            if previous != &piece {
                self.discard(&id);
                return Err(error("proto.frameAssemblyFragmentConflict"));
            }
        } else {
            assembly.staged_bytes += piece.len();
            self.staged_bytes += piece.len();
            if assembly.staged_bytes > logical_bytes || self.staged_bytes > MAX_STAGED_BYTES {
                self.discard(&id);
                return Err(error("proto.frameAssemblyTooLarge"));
            }
            assembly.pieces[index] = Some(piece);
        }
        if assembly.pieces.iter().any(Option::is_none) {
            return Ok(None);
        }
        if assembly.staged_bytes != logical_bytes {
            self.discard(&id);
            return Err(error("proto.frameAssemblyLengthMismatch"));
        }
        let assembly = self.pending.remove(&id).expect("complete assembly");
        self.staged_bytes = self.staged_bytes.saturating_sub(assembly.staged_bytes);
        let mut bytes = Vec::with_capacity(logical_bytes);
        for part in assembly.pieces.into_iter().flatten() {
            bytes.extend(part);
        }
        if format!("{:08x}", crc32(&bytes)) != assembly.checksum {
            return Err(error("proto.frameAssemblyChecksumMismatch"));
        }
        let frame: Value =
            serde_json::from_slice(&bytes).map_err(|_| error("proto.frameAssemblyInvalidJson"))?;
        validate_frame_envelope(
            &frame,
            &Value::String(assembly.topic),
            &Value::String(assembly.subscription_id),
        )?;
        self.settle_ordinal(
            wire["topic"].as_str().unwrap_or_default(),
            wire["subscriptionId"].as_str().unwrap_or_default(),
            nonnegative_u64(&wire["logicalFrameOrdinal"], "wire.logicalFrameOrdinal")?,
        )?;
        Ok(Some(frame))
    }

    fn settle_ordinal(&mut self, topic: &str, subscription_id: &str, ordinal: u64) -> Result<()> {
        let key = (topic.to_string(), subscription_id.to_string());
        if self.reanchor.remove(&key) {
            self.last_ordinal.insert(key, ordinal);
            return Ok(());
        }
        let previous = self.last_ordinal.get(&key).copied().unwrap_or(0);
        if ordinal <= previous {
            return Err(error("proto.frame.staleOrdinal"));
        }
        if ordinal != previous + 1 {
            return Err(error("proto.frame.ordinalGap"));
        }
        self.last_ordinal.insert(key, ordinal);
        Ok(())
    }

    fn discard(&mut self, id: &str) {
        if let Some(assembly) = self.pending.remove(id) {
            self.staged_bytes = self.staged_bytes.saturating_sub(assembly.staged_bytes);
        }
    }

    /// Drop incomplete fragments after a malformed route and let the next
    /// same-subscription recovery snapshot establish the ordinal baseline.
    pub fn prepare_subscription_recovery(&mut self, topic: &str, subscription_id: &str) {
        let keys: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, assembly)| {
                assembly.topic == topic && assembly.subscription_id == subscription_id
            })
            .map(|(id, _)| id.clone())
            .collect();
        for key in keys {
            self.discard(&key);
        }
        self.reanchor
            .insert((topic.to_string(), subscription_id.to_string()));
    }
}

fn validate_frame_envelope(frame: &Value, topic: &Value, subscription_id: &Value) -> Result<()> {
    check_keys(
        frame,
        &[
            "topic",
            "subscriptionId",
            "fromSeq",
            "toSeq",
            "sentAt",
            "payload",
        ],
        &[
            "topic",
            "subscriptionId",
            "fromSeq",
            "toSeq",
            "sentAt",
            "payload",
            "ttft",
            "ttftRelated",
        ],
        "frame",
    )?;
    if &frame["topic"] != topic || &frame["subscriptionId"] != subscription_id {
        return Err(error("proto.frame.metadataMismatch"));
    }
    let from = nonnegative_u64(&frame["fromSeq"], "frame.fromSeq")?;
    let to = nonnegative_u64(&frame["toSeq"], "frame.toSeq")?;
    if to < from {
        return Err(error("proto.frame.invalidRange"));
    }
    let payload = &frame["payload"];
    check_keys(
        payload,
        &["kind"],
        &["kind", "snapshot", "deltas"],
        "frame.payload",
    )?;
    match string(&payload["kind"], "frame.payload.kind")? {
        "snapshot" => {
            check_keys(
                payload,
                &["kind", "snapshot"],
                &["kind", "snapshot"],
                "frame.snapshotPayload",
            )?;
            if frame["topic"]
                .as_str()
                .unwrap_or_default()
                .starts_with("sessions-index/")
            {
                validate_sessions_index_snapshot(&payload["snapshot"])?;
            } else {
                validate_snapshot(&payload["snapshot"])?;
            }
        }
        "deltas" => {
            check_keys(
                payload,
                &["kind", "deltas"],
                &["kind", "deltas"],
                "frame.deltaPayload",
            )?;
            if !payload["deltas"].is_array() {
                return Err(error("proto.frame.invalidDeltas"));
            }
        }
        _ => return Err(error("proto.frame.invalidPayloadKind")),
    }
    Ok(())
}

fn validate_snapshot(snapshot: &Value) -> Result<()> {
    for key in [
        "protocolVersion",
        "sessionId",
        "logEpoch",
        "seq",
        "revision",
        "rows",
    ] {
        if snapshot.get(key).is_none() {
            return Err(error(&format!("proto.snapshot.missing.{key}")));
        }
    }
    if snapshot["protocolVersion"].as_u64() != Some(1) {
        return Err(error("proto.snapshot.unsupportedVersion"));
    }
    string(&snapshot["sessionId"], "snapshot.sessionId")?;
    string(&snapshot["logEpoch"], "snapshot.logEpoch")?;
    nonnegative_u64(&snapshot["seq"], "snapshot.seq")?;
    nonnegative_u64(&snapshot["revision"], "snapshot.revision")?;
    let window = snapshot["rows"]
        .get("window")
        .and_then(Value::as_array)
        .ok_or_else(|| error("proto.snapshot.invalidRows"))?;
    for row in window {
        validate_row(row)?;
    }
    if let Some(queue) = snapshot.get("queue") {
        validate_workflow_queue(queue, "snapshot.queue")?;
    }
    if let Some(interactions) = snapshot.get("pendingInteractions") {
        validate_pending_interactions(interactions, "snapshot.pendingInteractions")?;
    }
    if let Some(control) = snapshot.get("control") {
        parse_session_control(control)?;
    }
    if let Some(availability) = snapshot.get("availability") {
        parse_availability(availability)?;
    }
    if let Some(goal) = snapshot.get("goal") {
        parse_goal(goal)?;
    }
    if let Some(plan) = snapshot.get("plan") {
        parse_plan(plan)?;
    }
    if let Some(works) = snapshot.get("backgroundWorks") {
        parse_background_works(works)?;
    }
    Ok(())
}

fn validate_workflow_queue(queue: &Value, context: &str) -> Result<()> {
    if !queue["items"].is_array() || !queue["autoDrain"].is_boolean() {
        return Err(error(&format!("proto.{context}.invalid")));
    }
    for item in queue["items"].as_array().unwrap() {
        string(&item["queueItemId"], "queueItem.queueItemId")?;
        string_allow_empty(&item["text"], "queueItem.text")?;
    }
    Ok(())
}

fn validate_pending_interactions(interactions: &Value, context: &str) -> Result<()> {
    let interactions = interactions
        .as_array()
        .ok_or_else(|| error(&format!("proto.{context}.invalid")))?;
    for interaction in interactions {
        string(&interaction["interactionId"], "interaction.interactionId")?;
        string(&interaction["kind"], "interaction.kind")?;
        string(&interaction["payload"]["kind"], "interaction.payload.kind")?;
        if !interaction["payload"]["options"].is_null()
            && !interaction["payload"]["options"].is_array()
        {
            return Err(error("proto.interaction.invalidOptions"));
        }
    }
    Ok(())
}

fn validate_sessions_index_snapshot(snapshot: &Value) -> Result<()> {
    if snapshot["protocolVersion"].as_u64() != Some(1) {
        return Err(error("proto.sessionsIndex.unsupportedVersion"));
    }
    string(&snapshot["workspaceId"], "sessionsIndex.workspaceId")?;
    string(&snapshot["logEpoch"], "sessionsIndex.logEpoch")?;
    let sessions = snapshot["sessions"]
        .as_array()
        .ok_or_else(|| error("proto.sessionsIndex.invalidSessions"))?;
    for session in sessions {
        string(&session["sessionId"], "sessionSummary.sessionId")?;
        string(&session["workspaceId"], "sessionSummary.workspaceId")?;
        string(&session["phase"], "sessionSummary.phase")?;
        if session["lastActivityAt"].as_f64().is_none() {
            return Err(error("proto.sessionSummary.invalidActivity"));
        }
    }
    Ok(())
}

fn validate_row(row: &Value) -> Result<()> {
    if !row.is_object() {
        return Err(error("proto.row.notObject"));
    }
    nonnegative_u64(&row["rowId"], "row.rowId")?;
    string(&row["kind"], "row.kind")?;
    string(&row["turnId"], "row.turnId")?;
    if row["createdAt"].as_f64().is_none() {
        return Err(error("proto.row.invalidCreatedAt"));
    }
    nonnegative_u64(&row["createdAtSeq"], "row.createdAtSeq")?;
    let kind = row["kind"].as_str().unwrap_or_default();
    match kind {
        "turnHeader" => {
            string(&row["origin"], "row.origin")?;
            string(&row["state"], "row.state")?;
            if row["startedAt"].as_f64().is_none() {
                return Err(error("proto.row.invalidStartedAt"));
            }
        }
        "userInput" => {
            string_allow_empty(&row["text"], "row.text")?;
            if !matches!(
                row["origin"].as_str(),
                Some(
                    "realUser"
                        | "backgroundResult"
                        | "goalContinuation"
                        | "mailbox"
                        | "synthetic"
                        | "workflowLaunch"
                )
            ) {
                return Err(error("proto.row.invalidOrigin"));
            }
        }
        "assistantText" => {
            string_allow_empty(&row["text"], "row.text")?;
            if !matches!(
                row["state"].as_str(),
                Some("streaming" | "complete" | "interrupted" | "failed")
            ) {
                return Err(error("proto.row.invalidState"));
            }
        }
        "reasoning" => {
            string_allow_empty(&row["text"], "row.text")?;
            if !matches!(
                row["state"].as_str(),
                Some("streaming" | "complete" | "interrupted")
            ) {
                return Err(error("proto.row.invalidState"));
            }
        }
        "toolCall" => {
            string_allow_empty(&row["toolCallId"], "row.toolCallId")?;
            string_allow_empty(&row["toolName"], "row.toolName")?;
            string_allow_empty(&row["inputText"], "row.inputText")?;
            if !matches!(
                row["status"].as_str(),
                Some(
                    "inputStreaming"
                        | "pendingApproval"
                        | "running"
                        | "success"
                        | "error"
                        | "cancelled"
                )
            ) {
                return Err(error("proto.row.invalidToolStatus"));
            }
        }
        "artifact" => {
            for key in [
                "artifactVersionId",
                "logicalArtifactKey",
                "displayName",
                "artifactType",
                "mimeType",
                "sha256",
                "ref",
            ] {
                string(&row[key], &format!("row.{key}"))?;
            }
            nonnegative_u64(&row["sizeBytes"], "row.sizeBytes")?;
        }
        "subagent" => {
            string_allow_empty(&row["subagentType"], "row.subagentType")?;
            string_allow_empty(&row["summaryText"], "row.summaryText")?;
            if !matches!(
                row["status"].as_str(),
                Some("running" | "success" | "failed" | "cancelled")
            ) {
                return Err(error("proto.row.invalidSubagentStatus"));
            }
        }
        "hookInvocation" => {
            string(&row["hookInvocationId"], "row.hookInvocationId")?;
            string(&row["hookEventName"], "row.hookEventName")?;
            string(&row["lane"], "row.lane")?;
            if row["executions"].as_array().is_none() {
                return Err(error("proto.row.invalidHookExecutions"));
            }
        }
        "timelineMarker" => {
            if !row["marker"].is_object() {
                return Err(error("proto.row.invalidMarker"));
            }
        }
        _ => return Err(error("proto.row.unknownKind")),
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSummary {
    pub session_id: String,
    pub title: String,
    pub status: String,
    pub last_activity_at: i64,
}

#[derive(Clone, Debug, Default)]
pub struct SessionsIndexProjection {
    pub workspace_id: String,
    pub log_epoch: String,
    pub seq: u64,
    pub sessions: Vec<SessionSummary>,
    pub subscription_id: String,
}

impl SessionsIndexProjection {
    pub fn apply_frame(&mut self, frame: &Value) -> Result<()> {
        let mut candidate = self.clone();
        candidate.apply_frame_unpublished(frame)?;
        *self = candidate;
        Ok(())
    }

    fn apply_frame_unpublished(&mut self, frame: &Value) -> Result<()> {
        let topic = string(&frame["topic"], "frame.topic")?;
        if !topic.starts_with("sessions-index/") {
            return Err(error("proto.sessionsIndex.invalidTopic"));
        }
        let from = nonnegative_u64(&frame["fromSeq"], "frame.fromSeq")?;
        let to = nonnegative_u64(&frame["toSeq"], "frame.toSeq")?;
        if to < from {
            return Err(error("proto.frame.invalidRange"));
        }
        let workspace_id = topic.trim_start_matches("sessions-index/");
        let payload = &frame["payload"];
        match string(&payload["kind"], "frame.payload.kind")? {
            "snapshot" => {
                let snapshot = &payload["snapshot"];
                if snapshot["protocolVersion"].as_u64() != Some(1)
                    || snapshot["workspaceId"].as_str() != Some(workspace_id)
                {
                    return Err(error("proto.sessionsIndex.snapshotMismatch"));
                }
                if !self.subscription_id.is_empty()
                    && self.subscription_id == frame["subscriptionId"].as_str().unwrap_or_default()
                    && self.log_epoch == snapshot["logEpoch"].as_str().unwrap_or_default()
                    && to < self.seq
                {
                    return Err(error("proto.sessionsIndex.staleSnapshot"));
                }
                let sessions = snapshot["sessions"]
                    .as_array()
                    .ok_or_else(|| error("proto.sessionsIndex.invalidSessions"))?;
                let mut parsed = Vec::with_capacity(sessions.len());
                for entry in sessions {
                    let id = string(&entry["sessionId"], "sessionSummary.sessionId")?;
                    parsed.push(SessionSummary {
                        session_id: id.to_string(),
                        title: entry
                            .get("title")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        status: entry
                            .get("phase")
                            .and_then(Value::as_str)
                            .unwrap_or("idle")
                            .to_string(),
                        last_activity_at: entry
                            .get("lastActivityAt")
                            .and_then(Value::as_i64)
                            .unwrap_or_default(),
                    });
                }
                self.workspace_id = workspace_id.to_string();
                self.log_epoch =
                    string(&snapshot["logEpoch"], "sessionsIndex.logEpoch")?.to_string();
                self.subscription_id =
                    string(&frame["subscriptionId"], "frame.subscriptionId")?.to_string();
                self.seq = to;
                self.sessions = parsed;
            }
            "deltas" => {
                if self.workspace_id != workspace_id
                    || self.subscription_id != frame["subscriptionId"].as_str().unwrap_or_default()
                {
                    return Err(error("proto.sessionsIndex.staleSubscription"));
                }
                if from != self.seq {
                    return Err(error("proto.sessionsIndex.gapOrOverlap"));
                }
                for delta in payload["deltas"]
                    .as_array()
                    .ok_or_else(|| error("proto.sessionsIndex.invalidDeltas"))?
                {
                    match string(&delta["op"], "sessionsIndex.delta.op")? {
                        "session.upserted" => {
                            let value = &delta["session"];
                            let id = string(&value["sessionId"], "sessionSummary.sessionId")?
                                .to_string();
                            let summary = SessionSummary {
                                session_id: id.clone(),
                                title: value["title"].as_str().unwrap_or_default().to_string(),
                                status: value["phase"].as_str().unwrap_or("idle").to_string(),
                                last_activity_at: value["lastActivityAt"]
                                    .as_i64()
                                    .unwrap_or_default(),
                            };
                            if let Some(existing) = self
                                .sessions
                                .iter_mut()
                                .find(|session| session.session_id == id)
                            {
                                *existing = summary;
                            } else {
                                self.sessions.push(summary);
                            }
                        }
                        "session.removed" => {
                            let id = string(&delta["sessionId"], "sessionsIndex.sessionId")?;
                            self.sessions.retain(|session| session.session_id != id);
                        }
                        _ => return Err(error("proto.sessionsIndex.unknownDelta")),
                    }
                }
                self.seq = to;
            }
            _ => return Err(error("proto.sessionsIndex.invalidPayloadKind")),
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ConversationProjection {
    pub session_id: String,
    pub topic: String,
    pub subscription_id: String,
    pub log_epoch: String,
    pub seq: u64,
    pub revision: u64,
    rows: BTreeMap<u64, Value>,
    /// These workflow fields are copied from the canonical v4 snapshot and
    /// replaced by `state.updated`; they remain raw only until the view
    /// adapter extracts safe display fields.
    queue: Value,
    pending_interactions: Vec<Value>,
    control: WorkflowControlState,
    availability: WorkflowAvailability,
    goal: Option<WorkflowGoal>,
    plan: Option<Vec<WorkflowPlanItem>>,
    background_works: Vec<WorkflowBackgroundWork>,
    pub status: String,
}

impl ConversationProjection {
    pub fn new(session_id: &str, subscription: &SubscribeAck) -> Self {
        Self {
            session_id: session_id.into(),
            topic: format!("conversation/{session_id}"),
            subscription_id: subscription.subscription_id.clone(),
            log_epoch: subscription.log_epoch.clone(),
            seq: 0,
            revision: 0,
            rows: BTreeMap::new(),
            queue: json!({"items": [], "autoDrain": true}),
            pending_interactions: Vec::new(),
            control: WorkflowControlState::default(),
            availability: WorkflowAvailability::default(),
            goal: None,
            plan: None,
            background_works: Vec::new(),
            status: "subscribing".into(),
        }
    }

    pub fn apply_frame(&mut self, frame: &Value) -> Result<()> {
        let mut candidate = self.clone();
        candidate.apply_frame_unpublished(frame)?;
        *self = candidate;
        Ok(())
    }

    /// Apply to a private candidate. Any invalid operation abandons the
    /// whole frame, matching the CLI's atomic projection transaction.
    fn apply_frame_unpublished(&mut self, frame: &Value) -> Result<()> {
        validate_frame_envelope(
            frame,
            &Value::String(self.topic.clone()),
            &Value::String(self.subscription_id.clone()),
        )?;
        let from = nonnegative_u64(&frame["fromSeq"], "frame.fromSeq")?;
        let to = nonnegative_u64(&frame["toSeq"], "frame.toSeq")?;
        let payload = &frame["payload"];
        match payload["kind"].as_str() {
            Some("snapshot") => {
                let snapshot = &payload["snapshot"];
                if snapshot["sessionId"].as_str() != Some(self.session_id.as_str())
                    || snapshot["logEpoch"].as_str() != Some(self.log_epoch.as_str())
                    || nonnegative_u64(&snapshot["seq"], "snapshot.seq")? != to
                {
                    return Err(error("proto.snapshot.cursorMismatch"));
                }
                let rows = snapshot["rows"]["window"]
                    .as_array()
                    .ok_or_else(|| error("proto.snapshot.invalidRows"))?;
                self.rows.clear();
                for row in rows {
                    self.rows
                        .insert(nonnegative_u64(&row["rowId"], "row.rowId")?, row.clone());
                }
                while self.rows.len() > MAX_ROWS {
                    let first = *self.rows.keys().next().unwrap();
                    self.rows.remove(&first);
                }
                self.seq = to;
                self.revision = nonnegative_u64(&snapshot["revision"], "snapshot.revision")?;
                self.queue = snapshot
                    .get("queue")
                    .cloned()
                    .unwrap_or_else(|| json!({"items": [], "autoDrain": true}));
                self.pending_interactions = snapshot
                    .get("pendingInteractions")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                self.status = snapshot_status(snapshot);
                self.control = match snapshot.get("control") {
                    Some(control) => parse_session_control(control)?,
                    None => WorkflowControlState::default(),
                };
                self.availability = match snapshot.get("availability") {
                    Some(availability) => parse_availability(availability)?,
                    None => WorkflowAvailability::default(),
                };
                self.goal = match snapshot.get("goal") {
                    Some(goal) => parse_goal(goal)?,
                    None => None,
                };
                self.plan = match snapshot.get("plan") {
                    Some(plan) => parse_plan(plan)?,
                    None => None,
                };
                self.background_works = match snapshot.get("backgroundWorks") {
                    Some(works) => parse_background_works(works)?,
                    None => Vec::new(),
                };
            }
            Some("deltas") => {
                if self.seq != from {
                    return Err(error("proto.frame.gapOrOverlap"));
                }
                if from == 0 && to != 0 && !self.rows.is_empty() {
                    return Err(error("proto.frame.invalidSnapshotBase"));
                }
                for delta in payload["deltas"].as_array().unwrap() {
                    self.apply_delta(delta)?;
                }
                self.seq = to;
            }
            _ => return Err(error("proto.frame.invalidPayloadKind")),
        }
        Ok(())
    }

    fn apply_delta(&mut self, delta: &Value) -> Result<()> {
        match string(&delta["op"], "delta.op")? {
            "row.appended" => {
                let row = &delta["row"];
                validate_row(row)?;
                let id = nonnegative_u64(&row["rowId"], "row.rowId")?;
                if self.rows.contains_key(&id)
                    || self.rows.keys().next_back().is_some_and(|last| id <= *last)
                {
                    return Err(error("proto.row.appendedNotMonotonic"));
                }
                self.rows.insert(id, row.clone());
                while self.rows.len() > MAX_ROWS {
                    let first = *self.rows.keys().next().unwrap();
                    self.rows.remove(&first);
                }
            }
            "row.upserted" => {
                let row = &delta["row"];
                validate_row(row)?;
                let id = nonnegative_u64(&row["rowId"], "row.rowId")?;
                // Upsert of an unloaded row is a no-op; rows/range owns
                // loading history outside the current projection window.
                if let Some(existing) = self.rows.get_mut(&id) {
                    *existing = row.clone();
                }
            }
            "row.delta" => {
                let id = nonnegative_u64(&delta["rowId"], "delta.rowId")?;
                if let Some(row) = self.rows.get_mut(&id) {
                    let append = string_allow_empty(&delta["append"], "delta.append")?;
                    match delta["path"].as_str() {
                        Some("text")
                            if row["kind"] == "assistantText" || row["kind"] == "reasoning" =>
                        {
                            append_string(row, "text", append)
                        }
                        Some("inputText") if row["kind"] == "toolCall" => {
                            append_string(row, "inputText", append)
                        }
                        Some("output.text") if row["kind"] == "toolCall" => {
                            if row["output"].is_object() {
                                append_string(&mut row["output"], "text", append);
                            }
                        }
                        Some("summaryText") if row["kind"] == "subagent" => {
                            append_string(row, "summaryText", append)
                        }
                        _ => return Err(error("proto.delta.invalidPath")),
                    }
                }
            }
            "row.removed" => {
                let from = nonnegative_u64(&delta["fromRowId"], "delta.fromRowId")?;
                self.rows.retain(|id, _| *id < from);
            }
            "state.updated" => {
                let patch = &delta["patch"];
                if let Some(revision) = patch.get("revision").and_then(Value::as_u64) {
                    self.revision = revision;
                }
                if let Some(control) = patch.get("control") {
                    let parsed = parse_session_control(control)?;
                    self.status = control_status(control);
                    self.control = parsed;
                }
                if let Some(queue) = patch.get("queue") {
                    validate_workflow_queue(queue, "delta.queue")?;
                    self.queue = queue.clone();
                }
                if let Some(interactions) = patch.get("pendingInteractions") {
                    validate_pending_interactions(interactions, "delta.pendingInteractions")?;
                    self.pending_interactions = interactions.as_array().unwrap().clone();
                }
                if let Some(availability) = patch.get("availability") {
                    self.availability = parse_availability(availability)?;
                }
                if let Some(goal) = patch.get("goal") {
                    self.goal = parse_goal(goal)?;
                }
                if let Some(plan) = patch.get("plan") {
                    self.plan = parse_plan(plan)?;
                }
                if let Some(works) = patch.get("backgroundWorks") {
                    self.background_works = parse_background_works(works)?;
                }
            }
            "pluginUi.resourceUpdated"
            | "pluginUi.resourceListChanged"
            | "pluginUi.instanceClosed"
            | "pluginUi.appToolCall"
            | "workflowRun.updated"
            | "workflowRun.removed" => {}
            _ => return Err(error("proto.delta.unknownOperation")),
        }
        Ok(())
    }

    pub fn display_lines(&self) -> Vec<String> {
        self.rows.values().filter_map(render_row).collect()
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Convert the latest CLI-owned workflow projection into render-ready
    /// text and stable owner IDs. No local action mutates this result.
    pub fn workflow_state(&self) -> crate::view::WorkflowState {
        use crate::view::{
            WorkflowInteraction, WorkflowInteractionOption, WorkflowQueueItem, WorkflowState,
        };
        let queue_items = self.queue["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| {
                Some(WorkflowQueueItem {
                    queue_item_id: item["queueItemId"].as_str()?.to_owned(),
                    text: item["text"].as_str().unwrap_or_default().to_owned(),
                    delivery: item["delivery"]["admitted"]
                        .as_str()
                        .unwrap_or("queue")
                        .to_owned(),
                })
            })
            .take(64)
            .collect();
        let interactions = self
            .pending_interactions
            .iter()
            .filter_map(|interaction| {
                let payload = &interaction["payload"];
                let kind = interaction["kind"].as_str()?.to_owned();
                let questions = payload["questions"].as_array();
                let question = questions.and_then(|questions| questions.first());
                let raw_options = payload["options"]
                    .as_array()
                    .or_else(|| question.and_then(|q| q["options"].as_array()))
                    .cloned()
                    .unwrap_or_default();
                let mut raw_options = raw_options;
                if !payload["fullAccessOption"].is_null() {
                    raw_options.push(payload["fullAccessOption"].clone());
                }
                let raw_option_count = raw_options.len();
                let options: Vec<WorkflowInteractionOption> = raw_options
                    .iter()
                    .map(|option| WorkflowInteractionOption {
                        option_id: option["optionId"]
                            .as_str()
                            .or_else(|| option["value"].as_str())
                            .unwrap_or_default()
                            .to_owned(),
                        label: option["label"].as_str().unwrap_or_default().to_owned(),
                        kind: option["kind"].as_str().unwrap_or_default().to_owned(),
                    })
                    .collect();
                let unsupported_reason = if kind == "workspaceHookReview" {
                    Some("workspace hook review requires the full client".to_owned())
                } else if questions.is_some_and(|questions| questions.len() > 1) {
                    Some(format!(
                        "{}-question forms are not supported by the native client",
                        questions.unwrap().len()
                    ))
                } else if question.and_then(|q| q["multiSelect"].as_bool()) == Some(true) {
                    Some("multi-select questions are not supported by the native client".to_owned())
                } else if raw_option_count > 4 {
                    Some("this interaction has more than four choices".to_owned())
                } else if options.iter().any(|option| option.option_id.is_empty()) {
                    Some("this interaction contains an option without an ID".to_owned())
                } else {
                    None
                };
                Some(WorkflowInteraction {
                    interaction_id: interaction["interactionId"].as_str()?.to_owned(),
                    kind: kind.clone(),
                    title: if kind == "permission" {
                        payload["summary"]
                            .as_str()
                            .unwrap_or("Permission required")
                            .to_owned()
                    } else {
                        payload["prompt"]
                            .as_str()
                            .or_else(|| question.and_then(|q| q["question"].as_str()))
                            .or_else(|| question.and_then(|q| q["header"].as_str()))
                            .unwrap_or("Input required")
                            .to_owned()
                    },
                    summary: payload["toolName"].as_str().unwrap_or_default().to_owned(),
                    options,
                    free_text: payload["freeText"].as_bool().unwrap_or(false),
                    unsupported_reason,
                })
            })
            .take(16)
            .collect();
        WorkflowState {
            queue_items,
            auto_drain: self.queue["autoDrain"].as_bool().unwrap_or(true),
            interactions,
            control: self.control.clone(),
            availability: self.availability.clone(),
            goal: self.goal.clone(),
            plan: self.plan.clone(),
            background_works: self.background_works.clone(),
        }
    }
    pub fn first_row_id(&self) -> Option<u64> {
        self.rows.keys().next().copied()
    }

    /// Merge one durable older page only when it was read from this exact
    /// projection waterline. A raced query is discarded and can be retried.
    pub fn merge_rows_range(&mut self, result: &Value) -> Result<bool> {
        let epoch = string(&result["atLogEpoch"], "rowsRange.atLogEpoch")?;
        let seq = nonnegative_u64(&result["atSeq"], "rowsRange.atSeq")?;
        if epoch != self.log_epoch || seq != self.seq {
            return Ok(false);
        }
        let rows = result["rows"]
            .as_array()
            .ok_or_else(|| error("proto.rowsRange.invalidRows"))?;
        let original_first = self.rows.keys().next().copied();
        let mut previous_id = None;
        let mut candidate = self.rows.clone();
        for row in rows {
            validate_row(row)?;
            let id = nonnegative_u64(&row["rowId"], "rowsRange.rowId")?;
            if previous_id.is_some_and(|previous| id <= previous) {
                return Err(error("proto.rowsRange.rowsNotAscending"));
            }
            previous_id = Some(id);
            if original_first.is_none_or(|first| id < first) {
                candidate.entry(id).or_insert_with(|| row.clone());
            }
        }
        while candidate.len() > MAX_ROWS {
            let last = *candidate.keys().next_back().unwrap();
            candidate.remove(&last);
        }
        self.rows = candidate;
        Ok(true)
    }
}

fn append_string(value: &mut Value, key: &str, append: &str) {
    if let Some(target) = value.get_mut(key) {
        if let Some(text) = target.as_str() {
            *target = Value::String(format!("{text}{append}"));
        }
    } else {
        value[key] = Value::String(append.to_string());
    }
}

fn string_allow_empty<'a>(value: &'a Value, context: &str) -> Result<&'a str> {
    value
        .as_str()
        .ok_or_else(|| error(&format!("proto.{context}.invalidString")))
}

fn snapshot_status(snapshot: &Value) -> String {
    snapshot
        .get("control")
        .map(control_status)
        .unwrap_or_else(|| "idle".into())
}

fn control_status(control: &Value) -> String {
    match control
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or("idle")
    {
        "running" | "prewarming" => "running".into(),
        "waiting" => "waiting".into(),
        "completedSuccess" | "completedInterrupted" => "completed".into(),
        "failed" => "error".into(),
        "paused" => "paused".into(),
        _ => "idle".into(),
    }
}

fn render_row(row: &Value) -> Option<String> {
    let text = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or_default();
    match row["kind"].as_str()? {
        "userInput" => Some(format!("> {}", text("text"))),
        "assistantText" => Some(text("text").to_string()),
        "toolCall" => {
            let mark = match row["status"].as_str().unwrap_or("running") {
                "success" => "✓",
                "error" => "✗",
                "cancelled" => "■",
                _ => "⚙",
            };
            let name = text("toolName");
            let output = row
                .get("output")
                .and_then(|output| output.get("text"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            Some(if output.is_empty() {
                format!("{mark} {name}")
            } else {
                format!("{mark} {name}: {output}")
            })
        }
        "subagent" => Some(format!("↳ {}", text("summaryText"))),
        "artifact" => Some(format!("▧ {}", text("displayName"))),
        "hookInvocation" => Some(format!("⚙ {}", text("hookEventName"))),
        "timelineMarker" => Some(format!(
            "[{}]",
            row["marker"]["type"].as_str().unwrap_or("event")
        )),
        "turnHeader" | "reasoning" => None,
        _ => Some(format!("[{}]", row["kind"].as_str().unwrap_or("row"))),
    }
}

static COMMAND_COUNTER: AtomicU64 = AtomicU64::new(0);
static PROCESS_ID_SEED: OnceLock<u64> = OnceLock::new();

pub fn new_connection_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let counter = COMMAND_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("goya-connection-{}-{millis}-{counter}", std::process::id())
}

/// IDs are created once at UI admission and kept with AgentCmd across request
/// retries. `clientId` is stable for the process; IDs never encode user data.
pub fn new_command_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
        & 0x0000_ffff_ffff_ffff;
    let counter = COMMAND_COUNTER.fetch_add(1, Ordering::Relaxed);
    let process_seed = *PROCESS_ID_SEED.get_or_init(|| {
        RandomState::new().hash_one((
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        ))
    });
    let random_tail = (process_seed ^ (counter >> 12)) & 0x0000_ffff_ffff_ffff;
    // RFC 9562 UUIDv7 layout: 48-bit Unix milliseconds, version 7, RFC
    // variant, and process-local monotonic uniqueness for synthetic entropy.
    format!(
        "{:08x}-{:04x}-7{:03x}-8{:03x}-{:012x}",
        millis >> 16,
        millis & 0xffff,
        counter & 0x0fff,
        (random_tail >> 36) & 0x0fff,
        random_tail & 0x0000_000f_ffff_ffff,
    )
}

pub fn client_id() -> String {
    format!("goya-{}", std::process::id())
}

pub fn send_text_command(
    command_id: &str,
    client_id: &str,
    session_id: &str,
    revision: u64,
    text: &str,
) -> Value {
    json!({
        "inputOrigin": "desktop",
        "commandId": command_id,
        "clientId": client_id,
        "sessionId": session_id,
        "baseRevision": revision,
        "type": "sendText",
        "payload": { "text": text },
        "issuedAt": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
    })
}

pub fn commands_query(session_id: &str, command_id: &str) -> Value {
    json!({ "commands": [{ "sessionId": session_id, "commandId": command_id }] })
}

pub fn rows_range_query(session_id: &str, before_row_id: u64, limit: usize) -> Value {
    json!({ "sessionId": session_id, "beforeRowId": before_row_id, "limit": limit.min(200) })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutcome {
    pub command_id: String,
    pub status: String,
    pub reason_code: Option<String>,
    pub revision_at_decision: u64,
}

pub fn parse_command_ack(value: &Value, expected_id: &str) -> Result<CommandOutcome> {
    let ack = value.get("ack").unwrap_or(value);
    let status = string(&ack["status"], "commandAck.status")?;
    if !matches!(
        status,
        "accepted" | "rejected" | "stale" | "duplicate" | "noop" | "failed"
    ) {
        return Err(error("proto.commandAck.invalidStatus"));
    }
    let command_id = string(&ack["commandId"], "commandAck.commandId")?;
    if command_id != expected_id {
        return Err(error("proto.commandAck.idMismatch"));
    }
    Ok(CommandOutcome {
        command_id: command_id.into(),
        status: status.into(),
        reason_code: ack
            .get("reasonCode")
            .and_then(Value::as_str)
            .map(str::to_string),
        revision_at_decision: nonnegative_u64(
            &ack["revisionAtDecision"],
            "commandAck.revisionAtDecision",
        )?,
    })
}

/// `None` means the authoritative CLI has no recorded outcome. It must not
/// be interpreted as permission to replay the input automatically.
pub fn parse_command_query(
    value: &Value,
    session_id: &str,
    expected_id: &str,
) -> Result<Option<CommandOutcome>> {
    check_keys(
        value,
        &["results"],
        &["results", "clock"],
        "commandsQueryResult",
    )?;
    let results = value["results"]
        .as_array()
        .ok_or_else(|| error("proto.commandsQuery.invalidResults"))?;
    if results.len() != 1 {
        return Err(error("proto.commandsQuery.resultCount"));
    }
    let item = &results[0];
    check_keys(
        item,
        &["key", "result"],
        &["key", "result"],
        "commandQueryItem",
    )?;
    if item["key"]["sessionId"].as_str() != Some(session_id)
        || item["key"]["commandId"].as_str() != Some(expected_id)
    {
        return Err(error("proto.commandsQuery.keyMismatch"));
    }
    if item["result"].as_str() == Some("unknown") {
        return Ok(None);
    }
    parse_command_ack(&item["result"], expected_id).map(Some)
}

fn decode_base64(encoded: &str) -> Result<Vec<u8>> {
    if !encoded.len().is_multiple_of(4) {
        return Err(error("proto.frameAssemblyInvalidBase64"));
    }
    let mut output = Vec::with_capacity(encoded.len() / 4 * 3);
    let bytes = encoded.as_bytes();
    for (chunk_index, chunk) in bytes.chunks_exact(4).enumerate() {
        let last = chunk_index + 1 == bytes.len() / 4;
        let a = base64_value(chunk[0]).ok_or_else(|| error("proto.frameAssemblyInvalidBase64"))?;
        let b = base64_value(chunk[1]).ok_or_else(|| error("proto.frameAssemblyInvalidBase64"))?;
        let c_pad = chunk[2] == b'=';
        let d_pad = chunk[3] == b'=';
        if c_pad && !d_pad || d_pad && !last {
            return Err(error("proto.frameAssemblyInvalidBase64"));
        }
        let c = if c_pad {
            0
        } else {
            base64_value(chunk[2]).ok_or_else(|| error("proto.frameAssemblyInvalidBase64"))?
        };
        let d = if d_pad {
            0
        } else {
            base64_value(chunk[3]).ok_or_else(|| error("proto.frameAssemblyInvalidBase64"))?
        };
        output.push((a << 2) | (b >> 4));
        if !c_pad {
            output.push((b << 4) | (c >> 2));
        }
        if !d_pad {
            output.push((c << 6) | d);
        }
    }
    Ok(output)
}

fn base64_value(value: u8) -> Option<u8> {
    match value {
        b'A'..=b'Z' => Some(value - b'A'),
        b'a'..=b'z' => Some(value - b'a' + 26),
        b'0'..=b'9' => Some(value - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(id: &str) -> Value {
        let cases: Value =
            serde_json::from_str(include_str!("../../../fixtures/zcode-v4/cases.json"))
                .expect("root-provided v4 fixture JSON");
        cases
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["id"].as_str() == Some(id))
            .unwrap_or_else(|| panic!("missing v4 fixture {id}"))["value"]
            .clone()
    }

    fn snapshot_frame(sub: &SubscribeAck) -> Value {
        let snapshot = fixture("conversation.snapshot");
        let topic = format!("conversation/{}", snapshot["sessionId"].as_str().unwrap());
        json!({"topic":topic,"subscriptionId":sub.subscription_id,"fromSeq":0,"toSeq":snapshot["seq"],"sentAt":1,
            "payload":{"kind":"snapshot","snapshot":snapshot}})
    }

    fn index_snapshot_frame(subscription_id: &str) -> Value {
        let mut snapshot = fixture("index.snapshot");
        snapshot["workspaceId"] = json!("/synthetic/workspace");
        json!({"topic":"sessions-index//synthetic/workspace","subscriptionId":subscription_id,
            "fromSeq":0,"toSeq":0,"sentAt":1,"payload":{"kind":"snapshot","snapshot":snapshot}})
    }

    #[test]
    fn snapshot_hydrates_persisted_rows_and_status() {
        let ack = SubscribeAck {
            subscription_id: "sub-1".into(),
            mode: "snapshot".into(),
            log_epoch: "synthetic-epoch".into(),
        };
        let mut projection = ConversationProjection::new("synthetic-session", &ack);
        projection.apply_frame(&snapshot_frame(&ack)).unwrap();
        assert_eq!(
            projection.display_lines(),
            vec!["> Persisted synthetic input"]
        );
        assert_eq!(projection.revision, 1);
        assert_eq!(projection.status, "completed");
    }

    #[test]
    fn multi_question_projection_is_visible_but_never_submitted_as_first_answer() {
        let ack = SubscribeAck {
            subscription_id: "sub-1".into(),
            mode: "snapshot".into(),
            log_epoch: "synthetic-epoch".into(),
        };
        let mut frame = snapshot_frame(&ack);
        frame["payload"]["snapshot"]["pendingInteractions"] = json!([{
            "interactionId": "interaction-multi",
            "kind": "userInput",
            "anchorRowId": null,
            "createdAt": 1,
            "payload": {
                "kind": "userInput",
                "prompt": "Choose a direction",
                "freeText": false,
                "questions": [
                    {"question":"Direction?","header":"Direction","options":[{"value":"north","label":"North"},{"value":"south","label":"South"}]},
                    {"question":"Speed?","header":"Speed","options":[{"value":"fast","label":"Fast"},{"value":"slow","label":"Slow"}]}
                ],
                "currentQuestionIndex": 0,
                "answerDrafts": {}
            }
        }]);
        let mut projection = ConversationProjection::new("synthetic-session", &ack);
        projection.apply_frame(&frame).unwrap();
        let workflow = projection.workflow_state();
        let interaction = workflow.interactions.first().unwrap();
        assert_eq!(interaction.title, "Choose a direction");
        assert!(interaction
            .unsupported_reason
            .as_deref()
            .unwrap()
            .contains("2-question"));
        let mut view = crate::view::SessionView::default();
        view.sessions.push(crate::view::SessionEntry {
            session_id: "synthetic-session".into(),
            title: String::new(),
            status: String::new(),
        });
        view.replace_workflow(workflow);
        assert!(!view
            .workflow_controls()
            .iter()
            .any(|control| control.id.starts_with("question_option_")));
    }

    #[test]
    fn applies_upsert_text_delta_and_branch_removal_by_stable_row_id() {
        let ack = SubscribeAck {
            subscription_id: "sub-1".into(),
            mode: "snapshot".into(),
            log_epoch: "synthetic-epoch".into(),
        };
        let mut projection = ConversationProjection::new("synthetic-session", &ack);
        projection.apply_frame(&snapshot_frame(&ack)).unwrap();
        let frame = |from, to, deltas| json!({"topic":"conversation/synthetic-session","subscriptionId":"sub-1","fromSeq":from,"toSeq":to,"sentAt":2,"payload":{"kind":"deltas","deltas":deltas}});
        let assistant = |text: &str| json!({"rowId":2,"turnId":"synthetic-turn","createdAt":0,"createdAtSeq":2,"kind":"assistantText","text":text,"state":"complete"});
        projection
            .apply_frame(&frame(
                1,
                2,
                vec![json!({"op":"row.appended","row":assistant("world")})],
            ))
            .unwrap();
        projection
            .apply_frame(&frame(
                2,
                3,
                vec![json!({"op":"row.delta","rowId":2,"path":"text","append":"!"})],
            ))
            .unwrap();
        assert_eq!(projection.display_lines()[1], "world!");
        projection
            .apply_frame(&frame(
                3,
                4,
                vec![json!({"op":"row.removed","fromRowId":2})],
            ))
            .unwrap();
        assert_eq!(
            projection.display_lines(),
            vec!["> Persisted synthetic input"]
        );
    }

    #[test]
    fn rejects_cursor_gap_stale_subscription_and_bad_envelope() {
        let ack = SubscribeAck {
            subscription_id: "sub-1".into(),
            mode: "snapshot".into(),
            log_epoch: "synthetic-epoch".into(),
        };
        let mut projection = ConversationProjection::new("synthetic-session", &ack);
        projection.apply_frame(&snapshot_frame(&ack)).unwrap();
        let gap = json!({"topic":"conversation/synthetic-session","subscriptionId":"sub-1","fromSeq":0,"toSeq":2,"sentAt":2,"payload":{"kind":"deltas","deltas":[]}});
        assert_eq!(
            projection.apply_frame(&gap).unwrap_err().0,
            "proto.frame.gapOrOverlap"
        );
        let stale = json!({"topic":"conversation/synthetic-session","subscriptionId":"old-sub","fromSeq":1,"toSeq":2,"sentAt":2,"payload":{"kind":"deltas","deltas":[]}});
        assert_eq!(
            projection.apply_frame(&stale).unwrap_err().0,
            "proto.frame.metadataMismatch"
        );
        let mut malformed = snapshot_frame(&ack);
        malformed["extra"] = json!(true);
        assert_eq!(
            validate_frame_envelope(
                &malformed,
                &json!("conversation/synthetic-session"),
                &json!("sub-1")
            )
            .unwrap_err()
            .0,
            "proto.frame.unknownField"
        );
        assert_eq!(projection.seq, 1);
    }

    #[test]
    fn rejected_delta_frame_is_atomic_and_unloaded_upsert_does_not_resurrect_row() {
        let ack = SubscribeAck {
            subscription_id: "sub-1".into(),
            mode: "snapshot".into(),
            log_epoch: "synthetic-epoch".into(),
        };
        let mut projection = ConversationProjection::new("synthetic-session", &ack);
        projection.apply_frame(&snapshot_frame(&ack)).unwrap();
        let original_lines = projection.display_lines();
        let valid_append = json!({"rowId":2,"turnId":"synthetic-turn","createdAt":0,"createdAtSeq":2,"kind":"assistantText","text":"partial","state":"complete"});
        let rejected = json!({"topic":"conversation/synthetic-session","subscriptionId":"sub-1","fromSeq":1,"toSeq":2,"sentAt":2,"payload":{"kind":"deltas","deltas":[
            {"op":"row.appended","row":valid_append}, {"op":"not-a-v4-operation"}
        ]}});
        assert_eq!(
            projection.apply_frame(&rejected).unwrap_err().0,
            "proto.delta.unknownOperation"
        );
        assert_eq!(projection.seq, 1);
        assert_eq!(projection.display_lines(), original_lines);

        let missing_upsert = json!({"topic":"conversation/synthetic-session","subscriptionId":"sub-1","fromSeq":1,"toSeq":2,"sentAt":2,"payload":{"kind":"deltas","deltas":[
            {"op":"row.upserted","row":{"rowId":99,"turnId":"synthetic-turn","createdAt":0,"createdAtSeq":2,"kind":"assistantText","text":"unloaded","state":"complete"}}
        ]}});
        projection.apply_frame(&missing_upsert).unwrap();
        assert_eq!(projection.seq, 2);
        assert_eq!(projection.display_lines(), original_lines);
    }

    #[test]
    fn sessions_index_rejects_gaps_without_mutating_cursor() {
        let mut index = SessionsIndexProjection::default();
        index
            .apply_frame(&index_snapshot_frame("index-sub"))
            .unwrap();
        let gap = json!({"topic":"sessions-index//synthetic/workspace","subscriptionId":"index-sub","fromSeq":2,"toSeq":3,"sentAt":2,"payload":{"kind":"deltas","deltas":[]}});
        assert_eq!(
            index.apply_frame(&gap).unwrap_err().0,
            "proto.sessionsIndex.gapOrOverlap"
        );
        assert_eq!(index.seq, 0);
        assert_eq!(index.sessions.len(), 1);
    }

    #[test]
    fn rows_range_merges_ascending_page_and_rejects_malformed_page_atomically() {
        let ack = SubscribeAck {
            subscription_id: "sub-1".into(),
            mode: "snapshot".into(),
            log_epoch: "synthetic-epoch".into(),
        };
        let mut projection = ConversationProjection::new("synthetic-session", &ack);
        projection.apply_frame(&snapshot_frame(&ack)).unwrap();
        let mut row_60 = fixture("row.user");
        row_60["rowId"] = json!(60);
        let mut row_61 = row_60.clone();
        row_61["rowId"] = json!(61);
        projection.rows.clear();
        projection.rows.insert(60, row_60);
        projection.rows.insert(61, row_61);

        let mut row_1 = fixture("row.user");
        row_1["text"] = json!("older one");
        let mut row_2 = row_1.clone();
        row_2["rowId"] = json!(2);
        row_2["text"] = json!("older two");
        let page = json!({"atLogEpoch":"synthetic-epoch","atSeq":1,"rows":[row_1, row_2]});
        assert!(projection.merge_rows_range(&page).unwrap());
        assert_eq!(projection.first_row_id(), Some(1));
        assert_eq!(projection.row_count(), 4);

        let before_lines = projection.display_lines();
        let mut row_3 = fixture("row.user");
        row_3["rowId"] = json!(3);
        let malformed_page =
            json!({"atLogEpoch":"synthetic-epoch","atSeq":1,"rows":[row_3, {"rowId":4}]});
        assert_eq!(
            projection.merge_rows_range(&malformed_page).unwrap_err().0,
            "proto.row.kind.invalidString"
        );
        assert_eq!(projection.display_lines(), before_lines);
        assert_eq!(projection.row_count(), 4);
    }

    #[test]
    fn command_id_is_stable_in_envelope_and_query_targets_same_key() {
        let command = fixture("command.send");
        let id = command["commandId"].as_str().unwrap();
        let envelope = send_text_command(
            id,
            command["clientId"].as_str().unwrap(),
            "synthetic-session",
            0,
            "synthetic input",
        );
        assert_eq!(envelope["commandId"], command["commandId"]);
        assert_eq!(envelope["baseRevision"], command["baseRevision"]);
        assert_eq!(
            commands_query("synthetic-session", id),
            fixture("query.command")
        );
        let ack = fixture("command.accepted");
        assert_eq!(parse_command_ack(&ack, id).unwrap().status, "accepted");
    }

    #[test]
    fn decodes_fragmented_frame_with_bounds_and_crc() {
        let bytes = br#"{"topic":"conversation/s-1","subscriptionId":"sub-1","fromSeq":0,"toSeq":0,"sentAt":1,"payload":{"kind":"deltas","deltas":[]}}"#;
        let checksum = format!("{:08x}", crc32(bytes));
        let encoded = encode_test_base64(bytes);
        let mut assembler = FrameAssembler::default();
        let mut completed = None;
        for (index, part) in encoded.as_bytes().chunks(4).enumerate() {
            let wire = json!({"wireVersion":3,"kind":"fragment","deliveryKind":"initial","logicalFrameId":"f1","logicalFrameOrdinal":1,"topic":"conversation/s-1","subscriptionId":"sub-1","fragmentIndex":index,"fragmentCount":encoded.len()/4,"logicalBytes":bytes.len(),"checksum":{"algorithm":"crc32","value":checksum},"dataBase64":std::str::from_utf8(part).unwrap()});
            if let Some(frame) = assembler.push(&wire).unwrap() {
                completed = Some(frame);
            }
        }
        assert!(completed.is_some());
    }

    #[test]
    fn rejects_stale_ordinals_and_oversized_physical_envelopes() {
        let mut assembler = FrameAssembler::default();
        let frame = snapshot_frame(&SubscribeAck {
            subscription_id: "sub-1".into(),
            mode: "snapshot".into(),
            log_epoch: "synthetic-epoch".into(),
        });
        let wire = json!({"wireVersion":3,"kind":"complete","deliveryKind":"initial","logicalFrameId":"f1","logicalFrameOrdinal":1,"topic":"conversation/synthetic-session","subscriptionId":"sub-1","frame":frame});
        assembler.push(&wire).unwrap();
        assert_eq!(
            assembler.push(&wire).unwrap_err().0,
            "proto.frame.staleOrdinal"
        );
        assembler.prepare_subscription_recovery("conversation/synthetic-session", "sub-1");
        let mut recovery = wire.clone();
        recovery["deliveryKind"] = json!("recovery");
        recovery["logicalFrameId"] = json!("f3");
        recovery["logicalFrameOrdinal"] = json!(3);
        assembler.push(&recovery).unwrap();
        let mut stale_after_recovery = recovery.clone();
        stale_after_recovery["logicalFrameId"] = json!("f2");
        stale_after_recovery["logicalFrameOrdinal"] = json!(2);
        assert_eq!(
            assembler.push(&stale_after_recovery).unwrap_err().0,
            "proto.frame.staleOrdinal"
        );

        let oversized = json!({"wireVersion":3,"kind":"fragment","deliveryKind":"online","logicalFrameId":"giant","logicalFrameOrdinal":1,"topic":"conversation/synthetic-session","subscriptionId":"sub-1","fragmentIndex":0,"fragmentCount":1,"logicalBytes":1,"checksum":{"algorithm":"crc32","value":"00000000"},"dataBase64":"x".repeat(MAX_FRAME_BYTES)});
        assert_eq!(
            FrameAssembler::default().push(&oversized).unwrap_err().0,
            "proto.frameEnvelopeTooLarge"
        );
    }

    fn encode_test_base64(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut output = String::new();
        for chunk in bytes.chunks(3) {
            let a = chunk[0];
            let b = *chunk.get(1).unwrap_or(&0);
            let c = *chunk.get(2).unwrap_or(&0);
            output.push(ALPHABET[(a >> 2) as usize] as char);
            output.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
            output.push(if chunk.len() > 1 {
                ALPHABET[(((b & 15) << 2) | (c >> 6)) as usize] as char
            } else {
                '='
            });
            output.push(if chunk.len() > 2 {
                ALPHABET[(c & 63) as usize] as char
            } else {
                '='
            });
        }
        output
    }

    #[test]
    fn workspace_identity_fallback_is_trimmed_and_path_remains_fallback() {
        assert_eq!(
            workspace_key("/work/tree", Some("  remote:key  ")),
            "remote:key"
        );
        assert_eq!(workspace_key("/work/tree", Some("  ")), "/work/tree");
    }
}
