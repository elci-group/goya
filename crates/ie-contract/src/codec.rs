//! Canonical field-level wire encoding for contract v0.1 messages.
//! Frozen per ADR-IE-001 §Canonical encoding: little-endian integers,
//! u32-length-prefixed UTF-8 strings, u32-length-prefixed byte blobs.
//! Encoders never exceed MAX_MESSAGE_BYTES; decoders validate every count
//! and length against the remaining buffer before allocating.

use crate::errors::TypedError;
use crate::framing::MAX_MESSAGE_BYTES;
use crate::messages::{CapabilityDescriptor, Request, Response, Status};
use crate::tensors::{DType, TensorData, TensorDesc};
use crate::PROTOCOL_VERSION;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodecError {
    /// Buffer ended where a complete field was required.
    Truncated { need: usize, have: usize },
    /// A count/length exceeded what the remaining buffer could contain;
    /// rejected before allocation.
    ImplausibleLength { declared: u64, remaining: usize },
    UnknownDType(u8),
    UnknownStatus(u8),
    UnknownErrorCode(u16),
    InvalidUtf8,
    /// Protocol version mismatch is a contract violation, not a decode error
    /// variant: capability descriptors carry it and receivers must reject.
    VersionMismatch { local: u32, remote: u32 },
    /// Encoded message exceeds MAX_MESSAGE_BYTES.
    TooLarge(u64),
}

type Result<T> = core::result::Result<T, CodecError>;

// ---------- primitive writers ----------

fn put_u8(out: &mut Vec<u8>, v: u8) {
    out.push(v);
}

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_i32(out: &mut Vec<u8>, v: i32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, v: &str) -> Result<()> {
    let b = v.as_bytes();
    if out.len() + 4 + b.len() > MAX_MESSAGE_BYTES as usize {
        return Err(CodecError::TooLarge(out.len() as u64 + 4 + b.len() as u64));
    }
    put_u32(out, b.len() as u32);
    out.extend_from_slice(b);
    Ok(())
}

fn put_bytes32(out: &mut Vec<u8>, v: &[u8; 32]) {
    out.extend_from_slice(v);
}

// ---------- primitive readers ----------

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(CodecError::Truncated { need: n, have: self.remaining() });
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes(b.try_into().unwrap()))
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    /// Length-prefixed slice. The declared length is checked against the
    /// remaining buffer before any copy/allocation.
    fn bytes(&mut self) -> Result<&'a [u8]> {
        let len = self.u32()? as u64;
        if len > self.remaining() as u64 {
            return Err(CodecError::ImplausibleLength { declared: len, remaining: self.remaining() });
        }
        self.take(len as usize)
    }

    fn string(&mut self) -> Result<String> {
        let b = self.bytes()?;
        String::from_utf8(b.to_vec()).map_err(|_| CodecError::InvalidUtf8)
    }

    fn bytes32(&mut self) -> Result<[u8; 32]> {
        Ok(self.take(32)?.try_into().unwrap())
    }

    /// Count-prefixed sequence: the element count is checked so that a
    /// plausible minimum element size cannot exceed the remaining buffer.
    fn counted(&mut self, min_elem: usize) -> Result<u32> {
        let n = self.u32()? as u64;
        if n.saturating_mul(min_elem as u64) > self.remaining() as u64 {
            return Err(CodecError::ImplausibleLength { declared: n, remaining: self.remaining() });
        }
        Ok(n as u32)
    }
}

// ---------- tensors ----------

pub fn encode_tensor(out: &mut Vec<u8>, t: &TensorData) -> Result<()> {
    put_u8(out, t.desc.dtype.code());
    put_u32(out, t.desc.dims.len() as u32);
    for &d in &t.desc.dims {
        put_u64(out, d);
    }
    match t.desc.quant_scale {
        Some(s) => {
            put_u8(out, 1);
            put_f32(out, s);
        }
        None => put_u8(out, 0),
    }
    match t.desc.quant_zero_point {
        Some(z) => {
            put_u8(out, 1);
            put_i32(out, z);
        }
        None => put_u8(out, 0),
    }
    if out.len() + 8 + t.data.len() > MAX_MESSAGE_BYTES as usize {
        return Err(CodecError::TooLarge(out.len() as u64 + 8 + t.data.len() as u64));
    }
    put_u64(out, t.data.len() as u64);
    out.extend_from_slice(&t.data);
    Ok(())
}

pub fn decode_tensor(r: &mut Reader<'_>) -> Result<TensorData> {
    let raw = r.u8()?;
    let dtype = DType::from_code(raw).ok_or(CodecError::UnknownDType(raw))?;
    let rank = r.counted(8)? as usize;
    let mut dims = Vec::with_capacity(rank);
    for _ in 0..rank {
        dims.push(r.u64()?);
    }
    let quant_scale = if r.u8()? == 1 { Some(r.f32()?) } else { None };
    let quant_zero_point = if r.u8()? == 1 { Some(r.i32()?) } else { None };
    let payload_len = r.u64()? as usize;
    if payload_len > r.remaining() {
        return Err(CodecError::ImplausibleLength {
            declared: payload_len as u64,
            remaining: r.remaining(),
        });
    }
    let data = r.take(payload_len)?.to_vec();
    let desc = TensorDesc { dtype, dims, quant_scale, quant_zero_point };
    TensorData::new(desc, data).map_err(|_| CodecError::ImplausibleLength {
        declared: payload_len as u64,
        remaining: 0,
    })
}

// ---------- capability descriptor ----------

pub fn encode_capability(out: &mut Vec<u8>, c: &CapabilityDescriptor) -> Result<()> {
    put_u32(out, c.protocol_version);
    put_str(out, &c.endpoint_id)?;
    put_u64(out, c.epoch);
    put_u32(out, c.supported_ops.len() as u32);
    for op in &c.supported_ops {
        put_str(out, op)?;
    }
    put_u64(out, c.max_tensor_bytes);
    put_u32(out, c.queue_limit);
    put_u64(out, c.expiry_ms);
    Ok(())
}

pub fn decode_capability(buf: &[u8]) -> Result<(CapabilityDescriptor, usize)> {
    let mut r = Reader { buf, pos: 0 };
    let protocol_version = r.u32()?;
    if protocol_version != PROTOCOL_VERSION {
        return Err(CodecError::VersionMismatch { local: PROTOCOL_VERSION, remote: protocol_version });
    }
    let endpoint_id = r.string()?;
    let epoch = r.u64()?;
    let n = r.counted(1)?;
    let mut supported_ops = Vec::with_capacity(n as usize);
    for _ in 0..n {
        supported_ops.push(r.string()?);
    }
    let max_tensor_bytes = r.u64()?;
    let queue_limit = r.u32()?;
    let expiry_ms = r.u64()?;
    let c = CapabilityDescriptor {
        protocol_version,
        endpoint_id,
        epoch,
        supported_ops,
        max_tensor_bytes,
        queue_limit,
        expiry_ms,
    };
    Ok((c, r.pos))
}

// ---------- request ----------

pub fn encode_request(out: &mut Vec<u8>, req: &Request) -> Result<()> {
    put_u64(out, req.request_id);
    put_bytes32(out, &req.model_sha256);
    put_u32(out, req.inputs.len() as u32);
    for t in &req.inputs {
        encode_tensor(out, t)?;
    }
    put_u64(out, req.deadline_ms);
    put_u64(out, req.lease_id);
    put_bytes32(out, &req.policy_digest);
    put_u32(out, req.requested_outputs.len() as u32);
    for o in &req.requested_outputs {
        put_str(out, o)?;
    }
    Ok(())
}

pub fn decode_request(buf: &[u8]) -> Result<(Request, usize)> {
    let mut r = Reader { buf, pos: 0 };
    let request_id = r.u64()?;
    let model_sha256 = r.bytes32()?;
    let n = r.counted(1)?;
    let mut inputs = Vec::with_capacity(n as usize);
    for _ in 0..n {
        inputs.push(decode_tensor(&mut r)?);
    }
    let deadline_ms = r.u64()?;
    let lease_id = r.u64()?;
    let policy_digest = r.bytes32()?;
    let m = r.counted(1)?;
    let mut requested_outputs = Vec::with_capacity(m as usize);
    for _ in 0..m {
        requested_outputs.push(r.string()?);
    }
    let req = Request {
        request_id,
        model_sha256,
        inputs,
        deadline_ms,
        lease_id,
        policy_digest,
        requested_outputs,
    };
    Ok((req, r.pos))
}

// ---------- response ----------

pub fn encode_response(out: &mut Vec<u8>, resp: &Response) -> Result<()> {
    put_u64(out, resp.request_id);
    match &resp.status {
        Status::Pending => put_u8(out, 0),
        Status::Completed => put_u8(out, 1),
        Status::Failed(e) => {
            put_u8(out, 2);
            put_u16(out, e.code());
            let detail = match e {
                TypedError::UnsupportedOp { op } => op.clone(),
                TypedError::InvalidTensor { reason } | TypedError::PolicyDenied { reason }
                | TypedError::ResourceExhausted { reason } | TypedError::Internal { reason } => {
                    reason.clone()
                }
                TypedError::ModelMissing { digest } => digest.clone(),
                TypedError::VersionMismatch { local, remote } => {
                    format!("{local}:{remote}")
                }
                _ => String::new(),
            };
            put_str(out, &detail)?;
        }
    }
    put_str(out, &resp.backend)?;
    put_str(out, &resp.provider_id)?;
    put_u64(out, resp.epoch);
    put_u32(out, resp.outputs.len() as u32);
    for t in &resp.outputs {
        encode_tensor(out, t)?;
    }
    for &t in &resp.timing_us {
        put_u64(out, t);
    }
    Ok(())
}

pub fn decode_response(buf: &[u8]) -> Result<(Response, usize)> {
    let mut r = Reader { buf, pos: 0 };
    let request_id = r.u64()?;
    let status = match r.u8()? {
        0 => Status::Pending,
        1 => Status::Completed,
        2 => {
            let code = r.u16()?;
            let detail = r.string()?;
            let e = TypedError::from_code(code, &detail)
                .ok_or(CodecError::UnknownErrorCode(code))?;
            Status::Failed(e)
        }
        other => return Err(CodecError::UnknownStatus(other)),
    };
    let backend = r.string()?;
    let provider_id = r.string()?;
    let epoch = r.u64()?;
    let n = r.counted(1)?;
    let mut outputs = Vec::with_capacity(n as usize);
    for _ in 0..n {
        outputs.push(decode_tensor(&mut r)?);
    }
    let t0 = r.u64()?;
    let t1 = r.u64()?;
    let t2 = r.u64()?;
    let resp = Response {
        request_id,
        status,
        backend,
        provider_id,
        epoch,
        outputs,
        timing_us: [t0, t1, t2],
    };
    Ok((resp, r.pos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::GoldenFixture;

    fn sample_tensor(vals: &[f32]) -> TensorData {
        GoldenFixture::tensor(vals, &[vals.len() as u64])
    }

    fn sample_request() -> Request {
        Request {
            request_id: 0xA1B2C3D4,
            model_sha256: [0xABu8; 32],
            inputs: vec![sample_tensor(&[1.0, -2.5, 3.25])],
            deadline_ms: 1500,
            lease_id: 77,
            policy_digest: [0x11u8; 32],
            requested_outputs: vec!["logits".into()],
        }
    }

    #[test]
    fn request_roundtrip() {
        let req = sample_request();
        let mut buf = Vec::new();
        encode_request(&mut buf, &req).unwrap();
        let (back, consumed) = decode_request(&buf).unwrap();
        assert_eq!(back, req);
        assert_eq!(consumed, buf.len());
    }

    #[test]
    fn response_roundtrip_all_statuses() {
        let base = Response {
            request_id: 5,
            status: Status::Pending,
            backend: "cpu0".into(),
            provider_id: "ie://local/cpu0".into(),
            epoch: 12,
            outputs: vec![],
            timing_us: [1, 2, 3],
        };
        for status in [
            Status::Pending,
            Status::Completed,
            Status::Failed(TypedError::UnsupportedOp { op: "gelu".into() }),
            Status::Failed(TypedError::UntrustedPeer),
            Status::Failed(TypedError::VersionMismatch { local: 1, remote: 2 }),
        ] {
            let resp = Response { status: status.clone(), ..base.clone() };
            let mut buf = Vec::new();
            encode_response(&mut buf, &resp).unwrap();
            let (back, _) = decode_response(&buf).unwrap();
            assert_eq!(back, resp);
        }
    }

    #[test]
    fn capability_roundtrip_and_version_rejection() {
        let c = CapabilityDescriptor {
            protocol_version: PROTOCOL_VERSION,
            endpoint_id: "ie://local/cpu0".into(),
            epoch: 3,
            supported_ops: vec!["add".into(), "matmul".into()],
            max_tensor_bytes: 1 << 20,
            queue_limit: 8,
            expiry_ms: 30_000,
        };
        let mut buf = Vec::new();
        encode_capability(&mut buf, &c).unwrap();
        let (back, consumed) = decode_capability(&buf).unwrap();
        assert_eq!(back, c);
        assert_eq!(consumed, buf.len());

        let mut bad = c.clone();
        bad.protocol_version = PROTOCOL_VERSION + 1;
        let mut buf2 = Vec::new();
        encode_capability(&mut buf2, &bad).unwrap();
        assert!(matches!(
            decode_capability(&buf2),
            Err(CodecError::VersionMismatch { .. })
        ));
    }

    #[test]
    fn truncated_buffers_rejected() {
        let mut buf = Vec::new();
        encode_request(&mut buf, &sample_request()).unwrap();
        for cut in [0usize, 1, 8, 40, buf.len() - 1] {
            assert!(decode_request(&buf[..cut]).is_err(), "cut at {cut} must fail");
        }
    }

    /// Known answer vector: freezing the exact canonical bytes of a fixed
    /// request so future revisions notice any encoding change.
    #[test]
    fn known_answer_vector() {
        let req = Request {
            request_id: 1,
            model_sha256: [0u8; 32],
            inputs: vec![sample_tensor(&[1.5])],
            deadline_ms: 100,
            lease_id: 2,
            policy_digest: [0u8; 32],
            requested_outputs: vec![],
        };
        let mut buf = Vec::new();
        encode_request(&mut buf, &req).unwrap();
        let expected: Vec<u8> = [
            1u64.to_le_bytes().as_slice(),      // request_id
            &[0u8; 32],                         // model_sha256
            1u32.to_le_bytes().as_slice(),      // input count
            &[1u8],                             // dtype F32
            1u32.to_le_bytes().as_slice(),      // rank
            1u64.to_le_bytes().as_slice(),      // dim
            &[0u8],                             // quant_scale none
            &[0u8],                             // quant_zero_point none
            4u64.to_le_bytes().as_slice(),      // payload len
            1.5f32.to_le_bytes().as_slice(),    // payload
            100u64.to_le_bytes().as_slice(),    // deadline
            2u64.to_le_bytes().as_slice(),      // lease
            &[0u8; 32],                         // policy digest
            0u32.to_le_bytes().as_slice(),      // output count
        ]
        .concat();
        assert_eq!(buf, expected);
    }

    /// Implausible lengths are rejected before allocation: a huge declared
    /// string length on a tiny buffer must not reserve memory.
    #[test]
    fn implausible_length_rejected_before_allocation() {
        // count of 4 billion strings in a 4-byte buffer
        let buf = u32::MAX.to_le_bytes();
        let mut r = Reader { buf: &buf, pos: 0 };
        assert!(matches!(
            r.counted(1),
            Err(CodecError::ImplausibleLength { .. })
        ));
    }
}
