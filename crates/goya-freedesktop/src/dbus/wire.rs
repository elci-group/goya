//! D-Bus wire format: signatures, marshalling, message framing.
//!
//! Outgoing data is always little-endian. Incoming messages may be either
//! endianness (the bus forwards a peer's bytes unchanged). Every read is
//! bounds-checked; malformed input yields [`Error::Malformed`] or
//! [`Error::InvalidSignature`], never a panic.

use super::{Error, Value};

/// Largest message this crate will read or write, in bytes (1 MiB).
///
/// The D-Bus specification allows 128 MiB. Notification and portal
/// replies are a few hundred bytes, so a much smaller cap bounds the
/// allocation a hostile or broken peer can cause.
pub const MAX_MESSAGE_SIZE: usize = 1 << 20;

/// Maximum combined container nesting (arrays, structs, dict entries,
/// variants), as in the specification.
const MAX_DEPTH: usize = 64;
const MAX_ARRAY_DEPTH: usize = 32;
const MAX_STRUCT_DEPTH: usize = 32;
/// Specification limit on a single array's byte length.
const MAX_ARRAY_LEN: u32 = 1 << 26;
/// Cap on the number of decoded values per message. Every value consumes
/// at least one byte, so this is mainly a guard on `Value` memory, which is
/// larger than the wire bytes.
const MAX_VALUES: usize = 1 << 17;

/// A parsed single complete type.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Type {
    Byte,
    Bool,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Int64,
    Uint64,
    Double,
    String,
    ObjectPath,
    Signature,
    Variant,
    Array(Box<Type>),
    Dict(Box<Type>, Box<Type>),
    Struct(Vec<Type>),
}

impl Type {
    fn alignment(&self) -> usize {
        match self {
            Type::Byte | Type::Signature | Type::Variant => 1,
            Type::Int16 | Type::Uint16 => 2,
            Type::Bool
            | Type::Int32
            | Type::Uint32
            | Type::String
            | Type::ObjectPath
            | Type::Array(_)
            | Type::Dict(..) => 4,
            Type::Int64 | Type::Uint64 | Type::Double | Type::Struct(_) => 8,
        }
    }

    fn is_basic(&self) -> bool {
        !matches!(
            self,
            Type::Variant | Type::Array(_) | Type::Dict(..) | Type::Struct(_)
        )
    }

    pub(crate) fn write_signature(&self, out: &mut String) {
        match self {
            Type::Byte => out.push('y'),
            Type::Bool => out.push('b'),
            Type::Int16 => out.push('n'),
            Type::Uint16 => out.push('q'),
            Type::Int32 => out.push('i'),
            Type::Uint32 => out.push('u'),
            Type::Int64 => out.push('x'),
            Type::Uint64 => out.push('t'),
            Type::Double => out.push('d'),
            Type::String => out.push('s'),
            Type::ObjectPath => out.push('o'),
            Type::Signature => out.push('g'),
            Type::Variant => out.push('v'),
            Type::Array(t) => {
                out.push('a');
                t.write_signature(out);
            }
            Type::Dict(k, v) => {
                out.push_str("a{");
                k.write_signature(out);
                v.write_signature(out);
                out.push('}');
            }
            Type::Struct(fields) => {
                out.push('(');
                for f in fields {
                    f.write_signature(out);
                }
                out.push(')');
            }
        }
    }

    fn signature(&self) -> String {
        let mut s = String::new();
        self.write_signature(&mut s);
        s
    }
}

fn sig_err(sig: &[u8], reason: &'static str) -> Error {
    Error::InvalidSignature {
        signature: String::from_utf8_lossy(sig).into_owned(),
        reason,
    }
}

/// Parse a D-Bus signature (zero or more complete types).
pub(crate) fn parse_signature(sig: &str) -> Result<Vec<Type>, Error> {
    parse_signature_bytes(sig.as_bytes())
}

fn parse_signature_bytes(sig: &[u8]) -> Result<Vec<Type>, Error> {
    if sig.len() > 255 {
        return Err(sig_err(sig, "longer than 255 bytes"));
    }
    let mut pos = 0;
    let mut out = Vec::new();
    while pos < sig.len() {
        out.push(parse_one(sig, &mut pos, 0, 0)?);
    }
    Ok(out)
}

/// Parse a signature that must contain exactly one complete type.
pub(crate) fn parse_single(sig: &str) -> Result<Type, Error> {
    let mut types = parse_signature(sig)?;
    if types.len() != 1 {
        return Err(sig_err(
            sig.as_bytes(),
            "expected exactly one complete type",
        ));
    }
    Ok(types.remove(0))
}

fn parse_one(sig: &[u8], pos: &mut usize, arrays: usize, structs: usize) -> Result<Type, Error> {
    let c = *sig
        .get(*pos)
        .ok_or_else(|| sig_err(sig, "incomplete type"))?;
    *pos += 1;
    Ok(match c {
        b'y' => Type::Byte,
        b'b' => Type::Bool,
        b'n' => Type::Int16,
        b'q' => Type::Uint16,
        b'i' => Type::Int32,
        b'u' => Type::Uint32,
        b'x' => Type::Int64,
        b't' => Type::Uint64,
        b'd' => Type::Double,
        b's' => Type::String,
        b'o' => Type::ObjectPath,
        b'g' => Type::Signature,
        b'v' => Type::Variant,
        b'h' => return Err(sig_err(sig, "unix fds are not supported")),
        b'a' => {
            if arrays + 1 > MAX_ARRAY_DEPTH {
                return Err(sig_err(sig, "array nesting too deep"));
            }
            if sig.get(*pos) == Some(&b'{') {
                *pos += 1;
                if structs + 1 > MAX_STRUCT_DEPTH {
                    return Err(sig_err(sig, "struct nesting too deep"));
                }
                let k = parse_one(sig, pos, arrays + 1, structs + 1)?;
                if !k.is_basic() {
                    return Err(sig_err(sig, "dict key must be a basic type"));
                }
                let v = parse_one(sig, pos, arrays + 1, structs + 1)?;
                if sig.get(*pos) != Some(&b'}') {
                    return Err(sig_err(sig, "dict entry must have exactly two types"));
                }
                *pos += 1;
                Type::Dict(Box::new(k), Box::new(v))
            } else {
                Type::Array(Box::new(parse_one(sig, pos, arrays + 1, structs)?))
            }
        }
        b'(' => {
            if structs + 1 > MAX_STRUCT_DEPTH {
                return Err(sig_err(sig, "struct nesting too deep"));
            }
            let mut fields = Vec::new();
            loop {
                match sig.get(*pos) {
                    None => return Err(sig_err(sig, "unterminated struct")),
                    Some(b')') => {
                        *pos += 1;
                        break;
                    }
                    Some(_) => fields.push(parse_one(sig, pos, arrays, structs + 1)?),
                }
            }
            if fields.is_empty() {
                return Err(sig_err(sig, "empty struct"));
            }
            Type::Struct(fields)
        }
        _ => return Err(sig_err(sig, "unexpected character")),
    })
}

/// Light syntactic check of an object path ("/" or "/a/b_c").
fn valid_object_path(p: &str) -> bool {
    if p == "/" {
        return true;
    }
    let Some(rest) = p.strip_prefix('/') else {
        return false;
    };
    rest.split('/')
        .all(|seg| !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
}

// ---------------------------------------------------------------- encoding

struct Encoder {
    buf: Vec<u8>,
}

impl Encoder {
    fn pad(&mut self, align: usize) {
        while !self.buf.len().is_multiple_of(align) {
            self.buf.push(0);
        }
    }

    fn put_u32(&mut self, v: u32) {
        self.pad(4);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn put_str(&mut self, s: &str) -> Result<(), Error> {
        if s.as_bytes().contains(&0) {
            return Err(Error::InvalidArgument("string contains a NUL byte".into()));
        }
        let len =
            u32::try_from(s.len()).map_err(|_| Error::InvalidArgument("string too long".into()))?;
        self.put_u32(len);
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
        Ok(())
    }

    fn put_sig(&mut self, s: &str) -> Result<(), Error> {
        parse_signature(s)?;
        self.buf.push(s.len() as u8); // <= 255 checked by parse_signature
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
        Ok(())
    }

    fn value(&mut self, t: &Type, v: &Value, depth: usize) -> Result<(), Error> {
        if depth > MAX_DEPTH {
            return Err(Error::InvalidArgument("value nesting too deep".into()));
        }
        if self.buf.len() > MAX_MESSAGE_SIZE {
            return Err(Error::MessageTooLarge(self.buf.len() as u64));
        }
        match (t, v) {
            (Type::Byte, Value::Byte(x)) => self.buf.push(*x),
            (Type::Bool, Value::Bool(x)) => self.put_u32(u32::from(*x)),
            (Type::Int16, Value::Int16(x)) => {
                self.pad(2);
                self.buf.extend_from_slice(&x.to_le_bytes());
            }
            (Type::Uint16, Value::Uint16(x)) => {
                self.pad(2);
                self.buf.extend_from_slice(&x.to_le_bytes());
            }
            (Type::Int32, Value::Int32(x)) => {
                self.pad(4);
                self.buf.extend_from_slice(&x.to_le_bytes());
            }
            (Type::Uint32, Value::Uint32(x)) => self.put_u32(*x),
            (Type::Int64, Value::Int64(x)) => {
                self.pad(8);
                self.buf.extend_from_slice(&x.to_le_bytes());
            }
            (Type::Uint64, Value::Uint64(x)) => {
                self.pad(8);
                self.buf.extend_from_slice(&x.to_le_bytes());
            }
            (Type::Double, Value::Double(x)) => {
                self.pad(8);
                self.buf.extend_from_slice(&x.to_bits().to_le_bytes());
            }
            (Type::String, Value::String(s)) => self.put_str(s)?,
            (Type::ObjectPath, Value::ObjectPath(s)) => {
                if !valid_object_path(s) {
                    return Err(Error::InvalidArgument(format!("invalid object path {s:?}")));
                }
                self.put_str(s)?;
            }
            (Type::Signature, Value::Signature(s)) => self.put_sig(s)?,
            (Type::Variant, Value::Variant(inner)) => {
                let sig = inner.signature();
                let inner_t = parse_single(&sig)?;
                self.put_sig(&sig)?;
                self.value(&inner_t, inner, depth + 1)?;
            }
            (Type::Array(et), Value::Array { elem_sig, items }) => {
                if *elem_sig != et.signature() {
                    return Err(mismatch(t, v));
                }
                let (len_pos, start) = self.begin_array(et.alignment());
                for item in items {
                    self.value(et, item, depth + 1)?;
                }
                self.end_array(len_pos, start)?;
            }
            (
                Type::Dict(kt, vt),
                Value::Dict {
                    key_sig,
                    value_sig,
                    entries,
                },
            ) => {
                if *key_sig != kt.signature() || *value_sig != vt.signature() {
                    return Err(mismatch(t, v));
                }
                let (len_pos, start) = self.begin_array(8);
                for (k, val) in entries {
                    self.pad(8);
                    self.value(kt, k, depth + 2)?;
                    self.value(vt, val, depth + 2)?;
                }
                self.end_array(len_pos, start)?;
            }
            (Type::Struct(ts), Value::Struct(vs)) => {
                if ts.len() != vs.len() {
                    return Err(mismatch(t, v));
                }
                self.pad(8);
                for (ft, fv) in ts.iter().zip(vs) {
                    self.value(ft, fv, depth + 1)?;
                }
            }
            _ => return Err(mismatch(t, v)),
        }
        Ok(())
    }

    fn begin_array(&mut self, elem_align: usize) -> (usize, usize) {
        self.pad(4);
        let len_pos = self.buf.len();
        self.buf.extend_from_slice(&[0; 4]);
        // Padding to the first element is present even for empty arrays
        // and is not counted in the length.
        self.pad(elem_align);
        (len_pos, self.buf.len())
    }

    fn end_array(&mut self, len_pos: usize, start: usize) -> Result<(), Error> {
        let len = self.buf.len() - start;
        if len > MAX_ARRAY_LEN as usize {
            return Err(Error::MessageTooLarge(len as u64));
        }
        self.buf[len_pos..len_pos + 4].copy_from_slice(&(len as u32).to_le_bytes());
        Ok(())
    }
}

fn mismatch(t: &Type, v: &Value) -> Error {
    Error::TypeMismatch {
        expected: t.signature(),
        found: v.signature(),
    }
}

/// Marshal `values` against `signature` as a message body (offset 0 is
/// 8-aligned, as a body is). Little-endian.
pub fn marshal_body(signature: &str, values: &[Value]) -> Result<Vec<u8>, Error> {
    let types = parse_signature(signature)?;
    if types.len() != values.len() {
        return Err(Error::InvalidArgument(format!(
            "signature {signature:?} has {} types but {} values were given",
            types.len(),
            values.len()
        )));
    }
    let mut enc = Encoder { buf: Vec::new() };
    for (t, v) in types.iter().zip(values) {
        enc.value(t, v, 0)?;
    }
    if enc.buf.len() > MAX_MESSAGE_SIZE {
        return Err(Error::MessageTooLarge(enc.buf.len() as u64));
    }
    Ok(enc.buf)
}

// ---------------------------------------------------------------- decoding

struct Decoder<'a> {
    buf: &'a [u8],
    pos: usize,
    big: bool,
    values: usize,
}

impl<'a> Decoder<'a> {
    fn err(&self, reason: &'static str) -> Error {
        Error::Malformed {
            offset: self.pos,
            reason,
        }
    }

    fn align(&mut self, align: usize) -> Result<(), Error> {
        let new = self.pos.div_ceil(align) * align;
        if new > self.buf.len() {
            return Err(self.err("truncated padding"));
        }
        if self.buf[self.pos..new].iter().any(|&b| b != 0) {
            return Err(self.err("non-zero padding"));
        }
        self.pos = new;
        Ok(())
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.buf.len())
            .ok_or_else(|| self.err("truncated"))?;
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        self.align(N)?;
        let s = self.take(N)?;
        let mut a = [0u8; N];
        a.copy_from_slice(s);
        if self.big {
            a.reverse();
        }
        Ok(a) // now little-endian
    }

    fn get_u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.fixed::<4>()?))
    }

    fn get_string(&mut self) -> Result<String, Error> {
        let len = self.get_u32()? as usize;
        let start = self.pos;
        let bytes = self.take(len)?;
        let nul = self.take(1)?;
        if nul[0] != 0 {
            return Err(Error::Malformed {
                offset: start + len,
                reason: "string not NUL-terminated",
            });
        }
        if bytes.contains(&0) {
            return Err(Error::Malformed {
                offset: start,
                reason: "string contains NUL",
            });
        }
        String::from_utf8(bytes.to_vec()).map_err(|_| Error::Malformed {
            offset: start,
            reason: "string is not UTF-8",
        })
    }

    fn get_signature(&mut self) -> Result<String, Error> {
        let len = self.take(1)?[0] as usize;
        let start = self.pos;
        let bytes = self.take(len)?;
        if self.take(1)?[0] != 0 {
            return Err(Error::Malformed {
                offset: start + len,
                reason: "signature not NUL-terminated",
            });
        }
        parse_signature_bytes(bytes)?;
        // parse_signature_bytes only accepts ASCII type codes.
        String::from_utf8(bytes.to_vec()).map_err(|_| Error::Malformed {
            offset: start,
            reason: "signature is not ASCII",
        })
    }

    fn value(&mut self, t: &Type, depth: usize) -> Result<Value, Error> {
        if depth > MAX_DEPTH {
            return Err(self.err("nesting too deep"));
        }
        self.values += 1;
        if self.values > MAX_VALUES {
            return Err(self.err("too many values"));
        }
        Ok(match t {
            Type::Byte => Value::Byte(self.take(1)?[0]),
            Type::Bool => match self.get_u32()? {
                0 => Value::Bool(false),
                1 => Value::Bool(true),
                _ => return Err(self.err("boolean not 0 or 1")),
            },
            Type::Int16 => Value::Int16(i16::from_le_bytes(self.fixed()?)),
            Type::Uint16 => Value::Uint16(u16::from_le_bytes(self.fixed()?)),
            Type::Int32 => Value::Int32(i32::from_le_bytes(self.fixed()?)),
            Type::Uint32 => Value::Uint32(self.get_u32()?),
            Type::Int64 => Value::Int64(i64::from_le_bytes(self.fixed()?)),
            Type::Uint64 => Value::Uint64(u64::from_le_bytes(self.fixed()?)),
            Type::Double => Value::Double(f64::from_bits(u64::from_le_bytes(self.fixed()?))),
            Type::String => Value::String(self.get_string()?),
            Type::ObjectPath => {
                let s = self.get_string()?;
                if !valid_object_path(&s) {
                    return Err(self.err("invalid object path"));
                }
                Value::ObjectPath(s)
            }
            Type::Signature => Value::Signature(self.get_signature()?),
            Type::Variant => {
                let sig = self.get_signature()?;
                let inner = parse_single(&sig)
                    .map_err(|_| self.err("variant signature is not a single type"))?;
                Value::Variant(Box::new(self.value(&inner, depth + 1)?))
            }
            Type::Array(et) => {
                let end = self.array_bounds(et.alignment())?;
                let mut items = Vec::new();
                while self.pos < end {
                    items.push(self.value(et, depth + 1)?);
                }
                if self.pos != end {
                    return Err(self.err("array element overruns array length"));
                }
                Value::Array {
                    elem_sig: et.signature(),
                    items,
                }
            }
            Type::Dict(kt, vt) => {
                let end = self.array_bounds(8)?;
                let mut entries = Vec::new();
                while self.pos < end {
                    self.align(8)?;
                    let k = self.value(kt, depth + 2)?;
                    let v = self.value(vt, depth + 2)?;
                    entries.push((k, v));
                }
                if self.pos != end {
                    return Err(self.err("dict entry overruns array length"));
                }
                Value::Dict {
                    key_sig: kt.signature(),
                    value_sig: vt.signature(),
                    entries,
                }
            }
            Type::Struct(ts) => {
                self.align(8)?;
                let mut fields = Vec::with_capacity(ts.len());
                for ft in ts {
                    fields.push(self.value(ft, depth + 1)?);
                }
                Value::Struct(fields)
            }
        })
    }

    /// Read an array length and the padding to its first element; return the
    /// end offset of the element data.
    fn array_bounds(&mut self, elem_align: usize) -> Result<usize, Error> {
        let len = self.get_u32()?;
        if len > MAX_ARRAY_LEN {
            return Err(self.err("array longer than 64 MiB"));
        }
        self.align(elem_align)?;
        self.pos
            .checked_add(len as usize)
            .filter(|&e| e <= self.buf.len())
            .ok_or_else(|| self.err("array length exceeds data"))
    }
}

fn decode_values(sig: &str, body: &[u8], big: bool) -> Result<Vec<Value>, Error> {
    let types = parse_signature(sig)?;
    let mut dec = Decoder {
        buf: body,
        pos: 0,
        big,
        values: 0,
    };
    let mut out = Vec::with_capacity(types.len());
    for t in &types {
        out.push(dec.value(t, 0)?);
    }
    if dec.pos != body.len() {
        return Err(dec.err("trailing bytes after body"));
    }
    Ok(out)
}

/// Unmarshal a little-endian body against `signature`. Trailing bytes are
/// an error.
pub fn unmarshal_body(signature: &str, body: &[u8]) -> Result<Vec<Value>, Error> {
    decode_values(signature, body, false)
}

// ---------------------------------------------------------------- messages

/// D-Bus message type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    MethodCall,
    MethodReturn,
    Error,
    Signal,
}

impl MessageKind {
    fn code(self) -> u8 {
        match self {
            MessageKind::MethodCall => 1,
            MessageKind::MethodReturn => 2,
            MessageKind::Error => 3,
            MessageKind::Signal => 4,
        }
    }
}

/// Message header fields. `signature` describes the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub kind: MessageKind,
    pub flags: u8,
    pub serial: u32,
    pub path: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub error_name: Option<String>,
    pub reply_serial: Option<u32>,
    pub destination: Option<String>,
    pub sender: Option<String>,
    pub signature: String,
}

impl Header {
    /// A header with only `kind` and `serial` set.
    pub fn new(kind: MessageKind, serial: u32) -> Self {
        Header {
            kind,
            flags: 0,
            serial,
            path: None,
            interface: None,
            member: None,
            error_name: None,
            reply_serial: None,
            destination: None,
            sender: None,
            signature: String::new(),
        }
    }

    fn check_required(&self) -> Result<(), &'static str> {
        if self.serial == 0 {
            return Err("serial is zero");
        }
        match self.kind {
            MessageKind::MethodCall if self.path.is_none() || self.member.is_none() => {
                Err("method call needs path and member")
            }
            MessageKind::Signal
                if self.path.is_none() || self.interface.is_none() || self.member.is_none() =>
            {
                Err("signal needs path, interface and member")
            }
            MessageKind::MethodReturn if self.reply_serial.is_none() => {
                Err("method return needs reply serial")
            }
            MessageKind::Error if self.reply_serial.is_none() || self.error_name.is_none() => {
                Err("error needs reply serial and error name")
            }
            _ => Ok(()),
        }
    }
}

const FIELD_PATH: u8 = 1;
const FIELD_INTERFACE: u8 = 2;
const FIELD_MEMBER: u8 = 3;
const FIELD_ERROR_NAME: u8 = 4;
const FIELD_REPLY_SERIAL: u8 = 5;
const FIELD_DESTINATION: u8 = 6;
const FIELD_SENDER: u8 = 7;
const FIELD_SIGNATURE: u8 = 8;

/// Encode a complete little-endian message.
pub fn encode_message(header: &Header, body: &[Value]) -> Result<Vec<u8>, Error> {
    header
        .check_required()
        .map_err(|r| Error::InvalidArgument(r.into()))?;
    let body_bytes = marshal_body(&header.signature, body)?;

    let mut fields = Vec::new();
    let mut field = |code: u8, v: Value| {
        fields.push(Value::Struct(vec![
            Value::Byte(code),
            Value::Variant(Box::new(v)),
        ]));
    };
    if let Some(p) = &header.path {
        field(FIELD_PATH, Value::ObjectPath(p.clone()));
    }
    if let Some(s) = &header.interface {
        field(FIELD_INTERFACE, Value::String(s.clone()));
    }
    if let Some(s) = &header.member {
        field(FIELD_MEMBER, Value::String(s.clone()));
    }
    if let Some(s) = &header.error_name {
        field(FIELD_ERROR_NAME, Value::String(s.clone()));
    }
    if let Some(r) = header.reply_serial {
        field(FIELD_REPLY_SERIAL, Value::Uint32(r));
    }
    if let Some(s) = &header.destination {
        field(FIELD_DESTINATION, Value::String(s.clone()));
    }
    if let Some(s) = &header.sender {
        field(FIELD_SENDER, Value::String(s.clone()));
    }
    if !header.signature.is_empty() {
        field(FIELD_SIGNATURE, Value::Signature(header.signature.clone()));
    }

    let body_len = u32::try_from(body_bytes.len())
        .map_err(|_| Error::MessageTooLarge(body_bytes.len() as u64))?;
    let mut enc = Encoder {
        buf: Vec::with_capacity(64 + body_bytes.len()),
    };
    enc.buf
        .extend_from_slice(&[b'l', header.kind.code(), header.flags, 1]);
    enc.put_u32(body_len);
    enc.put_u32(header.serial);
    let fields_t = Type::Array(Box::new(Type::Struct(vec![Type::Byte, Type::Variant])));
    enc.value(
        &fields_t,
        &Value::Array {
            elem_sig: "(yv)".into(),
            items: fields,
        },
        0,
    )?;
    enc.pad(8);
    enc.buf.extend_from_slice(&body_bytes);
    if enc.buf.len() > MAX_MESSAGE_SIZE {
        return Err(Error::MessageTooLarge(enc.buf.len() as u64));
    }
    Ok(enc.buf)
}

/// Largest message the D-Bus specification allows (128 MiB). Frames over
/// [`MAX_MESSAGE_SIZE`] but within this limit can be skipped without
/// losing framing; anything larger is a protocol violation.
pub(crate) const SPEC_MAX_MESSAGE_SIZE: u64 = 1 << 27;

/// Given at least the first 16 bytes of a message, return the length of
/// its header (fixed part plus fields, padded to 8) and its total length.
/// Applies only the specification limit, not [`MAX_MESSAGE_SIZE`]. Fails
/// for unknown endianness or a total above [`SPEC_MAX_MESSAGE_SIZE`].
pub(crate) fn frame_lengths(prefix: &[u8]) -> Result<(u64, u64), Error> {
    if prefix.len() < 16 {
        return Err(Error::Malformed {
            offset: prefix.len(),
            reason: "truncated fixed header",
        });
    }
    let big = match prefix[0] {
        b'l' => false,
        b'B' => true,
        _ => {
            return Err(Error::Malformed {
                offset: 0,
                reason: "unknown endianness marker",
            })
        }
    };
    let rd = |at: usize| {
        let mut a = [prefix[at], prefix[at + 1], prefix[at + 2], prefix[at + 3]];
        if big {
            a.reverse();
        }
        u64::from(u32::from_le_bytes(a))
    };
    let body_len = rd(4);
    let fields_len = rd(12);
    let header_len = (16 + fields_len).div_ceil(8) * 8;
    let total = header_len + body_len;
    if total > SPEC_MAX_MESSAGE_SIZE {
        return Err(Error::MessageTooLarge(total));
    }
    Ok((header_len, total))
}

/// Given at least the first 16 bytes of a message, return its total length.
/// Fails for unknown endianness or a length above [`MAX_MESSAGE_SIZE`].
pub(crate) fn frame_length(prefix: &[u8]) -> Result<usize, Error> {
    let (_, total) = frame_lengths(prefix)?;
    if total > MAX_MESSAGE_SIZE as u64 {
        return Err(Error::MessageTooLarge(total));
    }
    Ok(total as usize)
}

/// Decode a message header; return it with the body offset and endianness.
pub(crate) fn decode_header(bytes: &[u8]) -> Result<(Header, usize, bool), Error> {
    let total = frame_length(bytes)?;
    if bytes.len() != total {
        return Err(Error::Malformed {
            offset: bytes.len().min(total),
            reason: if bytes.len() < total {
                "truncated message"
            } else {
                "trailing bytes after message"
            },
        });
    }
    let (h, body_start, big, body_len) = parse_header(bytes)?;
    if body_start + body_len != bytes.len() {
        return Err(Error::Malformed {
            offset: body_start,
            reason: "body length mismatch",
        });
    }
    Ok((h, body_start, big))
}

/// Decode only the header of a message whose body was not read: `bytes`
/// is exactly the header (fixed part plus fields and padding). Used to
/// decide whether a frame over [`MAX_MESSAGE_SIZE`] is the reply we wait for.
pub(crate) fn decode_header_only(bytes: &[u8]) -> Result<Header, Error> {
    let (h, body_start, _, _) = parse_header(bytes)?;
    if body_start != bytes.len() {
        return Err(Error::Malformed {
            offset: body_start.min(bytes.len()),
            reason: "header length mismatch",
        });
    }
    Ok(h)
}

/// Parse the fixed header and header fields at the start of `bytes`.
/// Returns the header, body offset, endianness and declared body length;
/// the caller checks how much of `bytes` the body occupies.
fn parse_header(bytes: &[u8]) -> Result<(Header, usize, bool, usize), Error> {
    if bytes.len() < 16 {
        return Err(Error::Malformed {
            offset: bytes.len(),
            reason: "truncated fixed header",
        });
    }
    let big = bytes[0] == b'B';
    let kind = match bytes[1] {
        1 => MessageKind::MethodCall,
        2 => MessageKind::MethodReturn,
        3 => MessageKind::Error,
        4 => MessageKind::Signal,
        _ => {
            return Err(Error::Malformed {
                offset: 1,
                reason: "unknown message type",
            })
        }
    };
    if bytes[3] != 1 {
        return Err(Error::Malformed {
            offset: 3,
            reason: "unsupported protocol version",
        });
    }
    let mut dec = Decoder {
        buf: bytes,
        pos: 4,
        big,
        values: 0,
    };
    let body_len = dec.get_u32()? as usize;
    let serial = dec.get_u32()?;
    let fields_t = Type::Array(Box::new(Type::Struct(vec![Type::Byte, Type::Variant])));
    let fields = dec.value(&fields_t, 0)?;
    dec.align(8)?;
    let body_start = dec.pos;

    let mut h = Header::new(kind, serial);
    h.flags = bytes[2];
    let mut have_sig = false;
    let items = match &fields {
        Value::Array { items, .. } => items.as_slice(),
        _ => &[],
    };
    for item in items {
        let Value::Struct(f) = item else { continue };
        let (Some(Value::Byte(code)), Some(Value::Variant(v))) = (f.first(), f.get(1)) else {
            continue;
        };
        let bad = || Error::Malformed {
            offset: 16,
            reason: "header field has the wrong type",
        };
        match (*code, v.as_ref()) {
            (FIELD_PATH, Value::ObjectPath(s)) => h.path = Some(s.clone()),
            (FIELD_INTERFACE, Value::String(s)) => h.interface = Some(s.clone()),
            (FIELD_MEMBER, Value::String(s)) => h.member = Some(s.clone()),
            (FIELD_ERROR_NAME, Value::String(s)) => h.error_name = Some(s.clone()),
            (FIELD_REPLY_SERIAL, Value::Uint32(r)) => h.reply_serial = Some(*r),
            (FIELD_DESTINATION, Value::String(s)) => h.destination = Some(s.clone()),
            (FIELD_SENDER, Value::String(s)) => h.sender = Some(s.clone()),
            (FIELD_SIGNATURE, Value::Signature(s)) => {
                h.signature = s.clone();
                have_sig = true;
            }
            (1..=8, _) => return Err(bad()),
            _ => {} // unknown fields (e.g. UNIX_FDS) are ignored
        }
    }
    if !have_sig && body_len > 0 {
        return Err(dec.err("non-empty body without signature"));
    }
    h.check_required()
        .map_err(|reason| Error::Malformed { offset: 0, reason })?;
    Ok((h, body_start, big, body_len))
}

/// Decode a body that follows a header from [`decode_header`].
pub(crate) fn decode_body(h: &Header, body: &[u8], big: bool) -> Result<Vec<Value>, Error> {
    decode_values(&h.signature, body, big)
}

/// Decode a complete message (either endianness).
pub fn decode_message(bytes: &[u8]) -> Result<(Header, Vec<Value>), Error> {
    let (h, body_start, big) = decode_header(bytes)?;
    let body = decode_body(&h, &bytes[body_start..], big)?;
    Ok((h, body))
}
