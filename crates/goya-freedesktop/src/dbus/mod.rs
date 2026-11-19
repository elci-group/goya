//! A minimal, blocking session-bus D-Bus client written against `std`.
//!
//! Scope: connect to a `unix:path=` or (Linux only) `unix:abstract=`
//! address, authenticate with SASL `EXTERNAL`, send `Hello`, then make
//! method calls and wait for the matching `METHOD_RETURN` or `ERROR`.
//! Messages that are not the reply being waited for (signals such as
//! `NameAcquired`, replies to other serials) are read and discarded.
//! Frames over [`MAX_MESSAGE_SIZE`] (but within the specification's
//! 128 MiB) are drained in bounded chunks without being allocated; only
//! an oversized reply to the call in flight fails it, with
//! [`Error::MessageTooLarge`], and the connection stays usable.
//!
//! Not implemented: unix fd passing (`h`), signal subscription
//! (`AddMatch`), serving objects, TCP and `launchd`/`autolaunch`
//! transports, `ANONYMOUS`/`DBUS_COOKIE_SHA1` authentication.

mod wire;

pub use wire::{
    decode_message, encode_message, marshal_body, unmarshal_body, Header, MessageKind,
    MAX_MESSAGE_SIZE,
};

use std::fmt;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Errors from this crate. Every failure mode is a variant; nothing panics.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// `DBUS_SESSION_BUS_ADDRESS` is unset or empty.
    NoSessionBus,
    /// The address has no entry this client can connect to.
    UnsupportedAddress(String),
    /// Socket error.
    Io(io::Error),
    /// SASL authentication failed or the server sent an unexpected line.
    Auth(String),
    /// No reply before the configured timeout.
    Timeout,
    /// The peer closed the socket, or an earlier partial read/write left the
    /// stream unusable.
    Disconnected,
    /// A message exceeds [`MAX_MESSAGE_SIZE`] (or an array exceeds 64 MiB).
    /// For an incoming reply this leaves the connection usable unless the
    /// declared length also exceeds the specification's 128 MiB.
    MessageTooLarge(u64),
    /// Bytes on the wire do not form a valid message.
    Malformed { offset: usize, reason: &'static str },
    /// A signature string is invalid or unsupported.
    InvalidSignature {
        signature: String,
        reason: &'static str,
    },
    /// A caller-supplied argument cannot be sent.
    InvalidArgument(String),
    /// A value does not match its signature.
    TypeMismatch { expected: String, found: String },
    /// The remote side replied with a D-Bus `ERROR` message.
    Remote {
        name: String,
        message: Option<String>,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoSessionBus => write!(f, "DBUS_SESSION_BUS_ADDRESS is not set"),
            Error::UnsupportedAddress(a) => {
                write!(f, "no supported transport in bus address {a:?}")
            }
            Error::Io(e) => write!(f, "D-Bus socket error: {e}"),
            Error::Auth(m) => write!(f, "D-Bus authentication failed: {m}"),
            Error::Timeout => write!(f, "timed out waiting for D-Bus reply"),
            Error::Disconnected => write!(f, "D-Bus connection closed or unusable"),
            Error::MessageTooLarge(n) => write!(f, "D-Bus message of {n} bytes exceeds the limit"),
            Error::Malformed { offset, reason } => {
                write!(f, "malformed D-Bus data at byte {offset}: {reason}")
            }
            Error::InvalidSignature { signature, reason } => {
                write!(f, "invalid D-Bus signature {signature:?}: {reason}")
            }
            Error::InvalidArgument(m) => write!(f, "invalid D-Bus argument: {m}"),
            Error::TypeMismatch { expected, found } => {
                write!(
                    f,
                    "D-Bus type mismatch: expected {expected:?}, found {found:?}"
                )
            }
            Error::Remote { name, message } => match message {
                Some(m) => write!(f, "{name}: {m}"),
                None => write!(f, "{name}"),
            },
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

/// A D-Bus value. Container variants carry their element signatures so
/// that empty containers and variants can be marshalled.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Byte(u8),
    Bool(bool),
    Int16(i16),
    Uint16(u16),
    Int32(i32),
    Uint32(u32),
    Int64(i64),
    Uint64(u64),
    Double(f64),
    String(String),
    ObjectPath(String),
    Signature(String),
    /// `a<elem_sig>` for any element type other than a dict entry.
    Array {
        elem_sig: String,
        items: Vec<Value>,
    },
    /// `a{<key_sig><value_sig>}`.
    Dict {
        key_sig: String,
        value_sig: String,
        entries: Vec<(Value, Value)>,
    },
    Struct(Vec<Value>),
    Variant(Box<Value>),
}

impl Value {
    /// `s` value.
    pub fn str(s: impl Into<String>) -> Value {
        Value::String(s.into())
    }

    /// `v` wrapping `v`.
    pub fn variant(v: Value) -> Value {
        Value::Variant(Box::new(v))
    }

    /// `as` value.
    pub fn string_array<I, S>(items: I) -> Value
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Value::Array {
            elem_sig: "s".into(),
            items: items.into_iter().map(|s| Value::String(s.into())).collect(),
        }
    }

    /// `a{sv}` value; each value is wrapped in a variant.
    pub fn vardict<I, K>(entries: I) -> Value
    where
        I: IntoIterator<Item = (K, Value)>,
        K: Into<String>,
    {
        Value::Dict {
            key_sig: "s".into(),
            value_sig: "v".into(),
            entries: entries
                .into_iter()
                .map(|(k, v)| (Value::String(k.into()), Value::variant(v)))
                .collect(),
        }
    }

    /// The D-Bus signature of this value (not validated).
    pub fn signature(&self) -> String {
        match self {
            Value::Byte(_) => "y".into(),
            Value::Bool(_) => "b".into(),
            Value::Int16(_) => "n".into(),
            Value::Uint16(_) => "q".into(),
            Value::Int32(_) => "i".into(),
            Value::Uint32(_) => "u".into(),
            Value::Int64(_) => "x".into(),
            Value::Uint64(_) => "t".into(),
            Value::Double(_) => "d".into(),
            Value::String(_) => "s".into(),
            Value::ObjectPath(_) => "o".into(),
            Value::Signature(_) => "g".into(),
            Value::Array { elem_sig, .. } => format!("a{elem_sig}"),
            Value::Dict {
                key_sig, value_sig, ..
            } => format!("a{{{key_sig}{value_sig}}}"),
            Value::Struct(fields) => {
                let mut s = String::from("(");
                for f in fields {
                    s.push_str(&f.signature());
                }
                s.push(')');
                s
            }
            Value::Variant(_) => "v".into(),
        }
    }

    /// Contents of a `s` or `o` value.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) | Value::ObjectPath(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Value::Uint32(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Double(v) => Some(*v),
            _ => None,
        }
    }

    /// Items of an array (not a dict).
    pub fn as_items(&self) -> Option<&[Value]> {
        match self {
            Value::Array { items, .. } => Some(items),
            _ => None,
        }
    }

    /// Fields of a struct.
    pub fn as_fields(&self) -> Option<&[Value]> {
        match self {
            Value::Struct(f) => Some(f),
            _ => None,
        }
    }

    /// Strip any number of variant layers.
    pub fn peel_variants(&self) -> &Value {
        let mut v = self;
        while let Value::Variant(inner) = v {
            v = inner;
        }
        v
    }

    /// Look up a string key in a dict value.
    pub fn lookup(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Dict { entries, .. } => entries
                .iter()
                .find(|(k, _)| k.as_str() == Some(key))
                .map(|(_, v)| v),
            _ => None,
        }
    }
}

/// A parsed bus address entry this client can connect to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusAddress {
    /// `unix:path=...`
    Path(PathBuf),
    /// `unix:abstract=...` (Linux abstract socket namespace).
    Abstract(Vec<u8>),
}

fn unescape_address_value(v: &str) -> Option<Vec<u8>> {
    let b = v.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = b.get(i + 1..i + 3)?;
            let s = std::str::from_utf8(hex).ok()?;
            out.push(u8::from_str_radix(s, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Some(out)
}

/// Parse a D-Bus server address list (`;`-separated) and return the first
/// entry with a supported transport (`unix:path=` or `unix:abstract=`).
pub fn parse_address(address: &str) -> Result<BusAddress, Error> {
    use std::os::unix::ffi::OsStringExt;
    for entry in address.split(';') {
        let Some(params) = entry.strip_prefix("unix:") else {
            continue;
        };
        for kv in params.split(',') {
            let Some((k, v)) = kv.split_once('=') else {
                continue;
            };
            let Some(val) = unescape_address_value(v) else {
                continue;
            };
            if val.is_empty() {
                continue;
            }
            match k {
                "path" => {
                    return Ok(BusAddress::Path(PathBuf::from(
                        std::ffi::OsString::from_vec(val),
                    )))
                }
                "abstract" => return Ok(BusAddress::Abstract(val)),
                _ => {}
            }
        }
    }
    Err(Error::UnsupportedAddress(address.to_owned()))
}

/// Default reply timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

/// Longest SASL line accepted from the server.
const MAX_AUTH_LINE: usize = 1024;

/// A blocking connection to a message bus.
pub struct Connection {
    stream: UnixStream,
    serial: u32,
    timeout: Duration,
    unique_name: String,
    broken: bool,
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection")
            .field("unique_name", &self.unique_name)
            .field("timeout", &self.timeout)
            .field("broken", &self.broken)
            .finish()
    }
}

/// Effective uid of this process, as the bus sees it via `SO_PEERCRED`.
fn current_uid() -> Result<u32, Error> {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("Uid:") {
                // real, effective, saved, filesystem
                if let Some(euid) = rest.split_whitespace().nth(1).and_then(|s| s.parse().ok()) {
                    return Ok(euid);
                }
            }
        }
    }
    use std::os::unix::fs::MetadataExt;
    Ok(std::fs::metadata("/proc/self")?.uid())
}

/// One frame read from the socket.
enum Frame {
    /// A complete message within [`MAX_MESSAGE_SIZE`].
    Message(Vec<u8>),
    /// A frame over [`MAX_MESSAGE_SIZE`] (but within the specification
    /// limit) whose bytes were read and dropped. `header` is its decoded
    /// header when that fitted under the cap and parsed.
    Skipped { total: u64, header: Option<Header> },
}

enum ReadFail {
    /// Deadline passed; `partial` is true if some bytes were consumed.
    Timeout {
        partial: bool,
    },
    Eof,
    Io(io::Error),
}

impl Connection {
    /// Connect to the session bus named by `DBUS_SESSION_BUS_ADDRESS`.
    pub fn session() -> Result<Connection, Error> {
        let addr = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default();
        if addr.is_empty() {
            return Err(Error::NoSessionBus);
        }
        Connection::connect_address(&addr)
    }

    /// Connect to a bus address string, authenticate and send `Hello`.
    pub fn connect_address(address: &str) -> Result<Connection, Error> {
        let stream = match parse_address(address)? {
            BusAddress::Path(p) => UnixStream::connect(p)?,
            #[cfg(any(target_os = "linux", target_os = "android"))]
            BusAddress::Abstract(name) => {
                #[cfg(target_os = "android")]
                use std::os::android::net::SocketAddrExt;
                #[cfg(target_os = "linux")]
                use std::os::linux::net::SocketAddrExt;
                let sa = std::os::unix::net::SocketAddr::from_abstract_name(name)?;
                UnixStream::connect_addr(&sa)?
            }
            #[cfg(not(any(target_os = "linux", target_os = "android")))]
            BusAddress::Abstract(_) => return Err(Error::UnsupportedAddress(address.to_owned())),
        };
        let mut conn = Connection::from_stream(stream, DEFAULT_TIMEOUT);
        conn.authenticate(current_uid()?)?;
        let reply = conn.call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "Hello",
            "",
            &[],
        )?;
        conn.unique_name = match reply.first() {
            Some(Value::String(s)) => s.clone(),
            _ => {
                return Err(Error::TypeMismatch {
                    expected: "s".into(),
                    found: sig_of(&reply),
                })
            }
        };
        Ok(conn)
    }

    fn from_stream(stream: UnixStream, timeout: Duration) -> Connection {
        Connection {
            stream,
            serial: 0,
            timeout,
            unique_name: String::new(),
            broken: false,
        }
    }

    /// Unique bus name assigned by `Hello` (e.g. `:1.42`).
    pub fn unique_name(&self) -> &str {
        &self.unique_name
    }

    /// Reply timeout for [`Connection::call`] (default 2 s).
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Set the reply timeout. A zero duration is treated as 1 ms.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout.max(Duration::from_millis(1));
    }

    fn authenticate(&mut self, uid: u32) -> Result<(), Error> {
        let deadline = Instant::now() + self.timeout;
        let hex: String = uid
            .to_string()
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect();
        let line = format!("\0AUTH EXTERNAL {hex}\r\n");
        self.write_all(line.as_bytes(), deadline)?;
        let reply = self.read_auth_line(deadline)?;
        if let Some(guid) = reply.strip_prefix("OK ") {
            if guid.is_empty() || !guid.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(Error::Auth(format!("bad server guid in {reply:?}")));
            }
        } else if reply.starts_with("REJECTED") {
            return Err(Error::Auth(format!("EXTERNAL rejected ({reply})")));
        } else {
            return Err(Error::Auth(format!("unexpected server line {reply:?}")));
        }
        self.write_all(b"BEGIN\r\n", deadline)
    }

    fn read_auth_line(&mut self, deadline: Instant) -> Result<String, Error> {
        let mut line = Vec::new();
        loop {
            let mut b = [0u8; 1];
            match self.read_full(&mut b, deadline) {
                Ok(()) => {}
                Err(ReadFail::Timeout { .. }) => return Err(Error::Timeout),
                Err(ReadFail::Eof) => return Err(Error::Disconnected),
                Err(ReadFail::Io(e)) => return Err(Error::Io(e)),
            }
            line.push(b[0]);
            if line.ends_with(b"\r\n") {
                line.truncate(line.len() - 2);
                return String::from_utf8(line)
                    .map_err(|_| Error::Auth("server line is not UTF-8".into()));
            }
            if line.len() > MAX_AUTH_LINE {
                return Err(Error::Auth("server line too long".into()));
            }
        }
    }

    fn write_all(&mut self, mut buf: &[u8], deadline: Instant) -> Result<(), Error> {
        let mut wrote_any = false;
        while !buf.is_empty() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.broken |= wrote_any;
                return Err(Error::Timeout);
            }
            self.stream.set_write_timeout(Some(remaining))?;
            match self.stream.write(buf) {
                Ok(0) => {
                    self.broken = true;
                    return Err(Error::Disconnected);
                }
                Ok(n) => {
                    wrote_any = true;
                    buf = &buf[n..];
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    self.broken |= wrote_any;
                    return Err(Error::Timeout);
                }
                Err(e) => {
                    self.broken = true;
                    return Err(Error::Io(e));
                }
            }
        }
        Ok(())
    }

    fn read_full(&mut self, buf: &mut [u8], deadline: Instant) -> Result<(), ReadFail> {
        let mut n = 0;
        while n < buf.len() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ReadFail::Timeout { partial: n > 0 });
            }
            self.stream
                .set_read_timeout(Some(remaining))
                .map_err(ReadFail::Io)?;
            match self.stream.read(&mut buf[n..]) {
                Ok(0) => return Err(ReadFail::Eof),
                Ok(k) => n += k,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return Err(ReadFail::Timeout { partial: n > 0 })
                }
                Err(e) => return Err(ReadFail::Io(e)),
            }
        }
        Ok(())
    }

    /// Read one framed message. A timeout before the first byte leaves the
    /// connection usable; any failure after that marks it broken.
    ///
    /// A frame over [`MAX_MESSAGE_SIZE`] but within the specification limit
    /// is not allocated: its header is decoded if it fits under the cap,
    /// the rest is read and discarded in bounded chunks, and
    /// [`Frame::Skipped`] is returned with the connection still in sync.
    /// A declared length over the specification limit breaks the connection.
    fn read_message(&mut self, deadline: Instant) -> Result<Frame, Error> {
        let mut fixed = [0u8; 16];
        if let Err(f) = self.read_full(&mut fixed, deadline) {
            return Err(self.read_fail(f));
        }
        let (header_len, total) = match wire::frame_lengths(&fixed) {
            Ok(t) => t,
            Err(e) => {
                self.broken = true;
                return Err(e);
            }
        };
        if total > MAX_MESSAGE_SIZE as u64 {
            let mut consumed = 16;
            let header = if header_len <= MAX_MESSAGE_SIZE as u64 {
                let mut hb = vec![0u8; header_len as usize];
                hb[..16].copy_from_slice(&fixed);
                if let Err(f) = self.read_full(&mut hb[16..], deadline) {
                    let e = self.read_fail(f);
                    self.broken = true;
                    return Err(e);
                }
                consumed = header_len;
                wire::decode_header_only(&hb).ok()
            } else {
                None
            };
            self.discard(total - consumed, deadline)?;
            return Ok(Frame::Skipped { total, header });
        }
        let mut msg = vec![0u8; total as usize];
        msg[..16].copy_from_slice(&fixed);
        if let Err(f) = self.read_full(&mut msg[16..], deadline) {
            let e = self.read_fail(f);
            self.broken = true;
            return Err(e);
        }
        Ok(Frame::Message(msg))
    }

    /// Read and drop `n` bytes of the current frame without allocating them.
    fn discard(&mut self, mut n: u64, deadline: Instant) -> Result<(), Error> {
        let mut chunk = [0u8; 8192];
        while n > 0 {
            let k = n.min(chunk.len() as u64) as usize;
            if let Err(f) = self.read_full(&mut chunk[..k], deadline) {
                let e = self.read_fail(f);
                self.broken = true;
                return Err(e);
            }
            n -= k as u64;
        }
        Ok(())
    }

    fn read_fail(&mut self, f: ReadFail) -> Error {
        match f {
            ReadFail::Timeout { partial } => {
                self.broken |= partial;
                Error::Timeout
            }
            ReadFail::Eof => {
                self.broken = true;
                Error::Disconnected
            }
            ReadFail::Io(e) => {
                self.broken = true;
                Error::Io(e)
            }
        }
    }

    fn next_serial(&mut self) -> u32 {
        self.serial = self.serial.wrapping_add(1);
        if self.serial == 0 {
            self.serial = 1;
        }
        self.serial
    }

    /// Call `member` on `iface` at `path` of `dest` with `args` marshalled
    /// against `signature`, and wait for the reply.
    ///
    /// Returns the reply body, or [`Error::Remote`] for a D-Bus error reply.
    /// Unrelated incoming messages are discarded. Waits at most
    /// [`Connection::timeout`] in total.
    pub fn call(
        &mut self,
        dest: &str,
        path: &str,
        iface: &str,
        member: &str,
        signature: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, Error> {
        if self.broken {
            return Err(Error::Disconnected);
        }
        let serial = self.next_serial();
        let mut h = Header::new(MessageKind::MethodCall, serial);
        h.destination = Some(dest.to_owned());
        h.path = Some(path.to_owned());
        h.interface = Some(iface.to_owned());
        h.member = Some(member.to_owned());
        h.signature = signature.to_owned();
        let bytes = encode_message(&h, args)?;
        let deadline = Instant::now() + self.timeout;
        self.write_all(&bytes, deadline)?;
        loop {
            let msg = match self.read_message(deadline)? {
                Frame::Message(m) => m,
                Frame::Skipped { total, header } => {
                    let ours = header.is_some_and(|h| {
                        h.reply_serial == Some(serial)
                            && matches!(h.kind, MessageKind::MethodReturn | MessageKind::Error)
                    });
                    if ours {
                        return Err(Error::MessageTooLarge(total));
                    }
                    continue;
                }
            };
            let (rh, body_start, big) = wire::decode_header(&msg)?;
            if rh.reply_serial != Some(serial)
                || !matches!(rh.kind, MessageKind::MethodReturn | MessageKind::Error)
            {
                continue;
            }
            let body = wire::decode_body(&rh, &msg[body_start..], big);
            if rh.kind == MessageKind::Error {
                let message = body
                    .ok()
                    .and_then(|b| b.into_iter().next())
                    .and_then(|v| match v {
                        Value::String(s) => Some(s),
                        _ => None,
                    });
                return Err(Error::Remote {
                    name: rh.error_name.unwrap_or_default(),
                    message,
                });
            }
            return body;
        }
    }
}

fn sig_of(values: &[Value]) -> String {
    values.iter().map(Value::signature).collect()
}

/// Check that `values` has exactly the signature `expected`.
pub(crate) fn expect_signature(values: &[Value], expected: &str) -> Result<(), Error> {
    let found = sig_of(values);
    if found == expected {
        Ok(())
    } else {
        Err(Error::TypeMismatch {
            expected: expected.into(),
            found,
        })
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    //! A fake bus peer over a socketpair, for unit tests.
    use super::*;

    pub(crate) fn pair() -> (Connection, UnixStream) {
        let (a, b) = UnixStream::pair().expect("socketpair");
        b.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        (Connection::from_stream(a, Duration::from_millis(500)), b)
    }

    /// Read one message from the server side.
    pub(crate) fn server_read(s: &mut UnixStream) -> (Header, Vec<Value>) {
        let mut fixed = [0u8; 16];
        s.read_exact(&mut fixed).unwrap();
        let total = wire::frame_length(&fixed).unwrap();
        let mut msg = vec![0u8; total];
        msg[..16].copy_from_slice(&fixed);
        s.read_exact(&mut msg[16..]).unwrap();
        decode_message(&msg).unwrap()
    }

    pub(crate) fn reply(serial: u32, reply_to: u32, sig: &str, body: &[Value]) -> Vec<u8> {
        let mut h = Header::new(MessageKind::MethodReturn, serial);
        h.reply_serial = Some(reply_to);
        h.signature = sig.into();
        encode_message(&h, body).unwrap()
    }

    pub(crate) fn error(serial: u32, reply_to: u32, name: &str, text: &str) -> Vec<u8> {
        let mut h = Header::new(MessageKind::Error, serial);
        h.reply_serial = Some(reply_to);
        h.error_name = Some(name.into());
        h.signature = "s".into();
        encode_message(&h, &[Value::str(text)]).unwrap()
    }

    pub(crate) fn signal(serial: u32) -> Vec<u8> {
        let mut h = Header::new(MessageKind::Signal, serial);
        h.path = Some("/org/freedesktop/DBus".into());
        h.interface = Some("org.freedesktop.DBus".into());
        h.member = Some("NameAcquired".into());
        h.signature = "s".into();
        encode_message(&h, &[Value::str(":1.7")]).unwrap()
    }
}

#[cfg(test)]
mod tests;
