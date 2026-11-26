//! The peer's HTTP server: pairing, sealed messages and long polls,
//! generic over the application behind it ([`App`]).
//!
//! Taken from Tapestry's `tv_link.rs`, with Tapestry's presentations and
//! reviews moved behind [`App`]. Two additions:
//! * **Field pushes.** On a poll, when [`App::fields`] returns values that
//!   differ from the last values sent on this pairing, the poll answers
//!   with `fields`. This is how a dashboard keeps the TV's `field` state
//!   current.
//! * **Acts without a presentation** (`presentation_id: null`).
//!
//! The desktop confirmation arrives as a file `confirm` in the state
//! directory ([`confirm`]), written by a separate command, so the server
//! never reads a terminal it does not own.
//!
//! **Proof of concept, LAN only:** not a reviewed pairing protocol, no TLS.

use crate::crypto::{aad, derive, hex, hex_decode, open, seal, sha256, Derived};
use crate::wire::*;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ring::agreement;
use ring::rand::{SecureRandom, SystemRandom};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The application behind the link. Every method has a refusing or empty
/// default, so a dashboard implements only [`App::fields`] and [`App::act`].
pub trait App: Send + 'static {
    /// Current values for the TV's `field` bindings. Called on every poll
    /// tick (four times a second), so it should return cached values.
    fn fields(&mut self) -> BTreeMap<String, FieldValue> {
        BTreeMap::new()
    }
    /// A presentation newer than `after`, if there is one.
    fn presentation(&mut self, _after: Option<&str>) -> Option<Presentation> {
        None
    }
    /// The bytes of one artefact of the current presentation.
    fn artefact(&mut self, _id: &str) -> Option<Vec<u8>> {
        None
    }
    /// A scene-declared act.
    fn act(&mut self, _presentation_id: Option<&str>, name: &str, _arg: Option<&str>) -> DesktopMessage {
        DesktopMessage::Refused { reason: format!("this peer has no act {name}") }
    }
    /// A decision on a presentation.
    fn decide(&mut self, _decision: Decide) -> DesktopMessage {
        DesktopMessage::Refused { reason: "this peer takes no decisions".into() }
    }
}

struct Pairing {
    id: String,
    created: Instant,
    last_seen: Instant,
    commitment: Vec<u8>,
    private: Option<agreement::EphemeralPrivateKey>,
    desk_pub: Vec<u8>,
    derived: Option<Derived>,
    confirmed_tv: bool,
    confirmed_desktop: bool,
    rx_max: u64,
    rx_seen: BTreeSet<u64>,
    tx_ctr: u64,
    sent_fields: Option<BTreeMap<String, FieldValue>>,
}

impl Pairing {
    fn paired(&self) -> bool {
        self.confirmed_tv && self.confirmed_desktop
    }
}

/// One HTTP answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

impl Response {
    fn json(body: Vec<u8>) -> Self {
        Response { status: 200, content_type: "application/json", headers: vec![], body }
    }
    fn error(status: u16, m: &str) -> Self {
        let body = serde_json::to_vec(&serde_json::json!({ "error": m })).expect("serialise");
        Response { status, content_type: "application/json", headers: vec![], body }
    }
}

type Fail = (u16, String);

pub struct Server<A: App> {
    state_dir: PathBuf,
    /// Shown next to the code, e.g. `bloom tv confirm --code`.
    confirm_hint: String,
    app: A,
    pairing: Option<Pairing>,
    attempts: u32,
    rng: SystemRandom,
}

impl<A: App> Server<A> {
    /// `confirm_hint` is the command a person runs with the code, which the
    /// server prints when a code is derived.
    pub fn new(state_dir: &Path, confirm_hint: &str, app: A) -> io::Result<Self> {
        fs::create_dir_all(state_dir)?;
        let _ = fs::remove_file(state_dir.join("confirm"));
        Ok(Server {
            state_dir: state_dir.to_path_buf(),
            confirm_hint: confirm_hint.to_string(),
            app,
            pairing: None,
            attempts: 0,
            rng: SystemRandom::new(),
        })
    }

    pub fn app(&mut self) -> &mut A {
        &mut self.app
    }

    pub fn paired(&self) -> bool {
        self.pairing.as_ref().is_some_and(Pairing::paired)
    }

    fn expire(&mut self) {
        let drop = self.pairing.as_ref().is_some_and(|p| {
            (!p.paired() && p.created.elapsed() > Duration::from_secs(PAIRING_TTL_SECS))
                || (p.paired() && p.last_seen.elapsed() > Duration::from_secs(IDLE_REVOKE_SECS))
        });
        if drop {
            println!("PAIRING ended (expired or idle); the TV must pair again");
            self.pairing = None;
        }
    }

    fn poll_confirm_file(&mut self) {
        let path = self.state_dir.join("confirm");
        let Ok(code) = fs::read_to_string(&path) else { return };
        let _ = fs::remove_file(&path);
        let code = code.trim();
        match &mut self.pairing {
            Some(p) if p.derived.as_ref().is_some_and(|d| d.sas == code) => {
                p.confirmed_desktop = true;
                println!("CONFIRMED on this machine");
                if p.paired() {
                    println!("PAIRED");
                }
            }
            Some(_) => {
                println!("CODE MISMATCH: the pairing is discarded; pair again from the TV");
                self.pairing = None;
            }
            None => println!("no pairing is waiting for confirmation"),
        }
    }

    fn pair_commit(&mut self, body: &[u8]) -> Result<Vec<u8>, Fail> {
        self.expire();
        let m: PairCommit = serde_json::from_slice(body).map_err(|_| (400, "malformed commit".into()))?;
        if m.protocol != PROTOCOL {
            return Err((400, format!("this peer speaks {PROTOCOL}")));
        }
        if self.paired() {
            return Err((409, "a TV is already paired".into()));
        }
        if self.attempts >= MAX_PAIRING_ATTEMPTS {
            return Err((429, "pairing attempts exhausted; restart the server".into()));
        }
        let commitment = hex_decode(&m.commitment)
            .filter(|c| c.len() == 32)
            .ok_or((400, "malformed commitment".to_string()))?;
        self.attempts += 1;
        let private = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &self.rng)
            .map_err(|_| (500, "key generation failed".to_string()))?;
        let desk_pub = private
            .compute_public_key()
            .map_err(|_| (500, "key generation failed".to_string()))?
            .as_ref()
            .to_vec();
        let mut id = [0u8; 16];
        self.rng.fill(&mut id).map_err(|_| (500, "randomness failed".to_string()))?;
        let pairing_id = hex(&id);
        println!("PAIRING attempt {} of {MAX_PAIRING_ATTEMPTS} started", self.attempts);
        self.pairing = Some(Pairing {
            id: pairing_id.clone(),
            created: Instant::now(),
            last_seen: Instant::now(),
            commitment,
            private: Some(private),
            desk_pub: desk_pub.clone(),
            derived: None,
            confirmed_tv: false,
            confirmed_desktop: false,
            rx_max: 0,
            rx_seen: BTreeSet::new(),
            tx_ctr: 0,
            sent_fields: None,
        });
        let reply = PairCommitReply { pairing_id, desktop_pub: B64.encode(desk_pub) };
        Ok(serde_json::to_vec(&reply).expect("serialise"))
    }

    fn pair_reveal(&mut self, body: &[u8]) -> Result<Vec<u8>, Fail> {
        self.expire();
        let m: PairReveal = serde_json::from_slice(body).map_err(|_| (400, "malformed reveal".into()))?;
        let p = self
            .pairing
            .as_mut()
            .filter(|p| p.id == m.pairing_id && p.private.is_some())
            .ok_or((403, "no such pairing".to_string()))?;
        let tv_pub = B64.decode(&m.tv_pub).map_err(|_| (400, "malformed key".to_string()))?;
        if sha256(&tv_pub) != p.commitment {
            self.pairing = None;
            println!("PAIRING refused: the revealed key does not match the commitment");
            return Err((403, "the key does not match its commitment".into()));
        }
        let private = p.private.take().expect("checked above");
        let peer = agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, tv_pub.clone());
        let desk_pub = p.desk_pub.clone();
        let derived = agreement::agree_ephemeral(private, &peer, |z| derive(z, &tv_pub, &desk_pub))
            .map_err(|_| (400, "invalid public key".to_string()))?;
        println!(
            "CODE {}: check that the TV shows the same code, press Confirm on the TV, then run\n  {} {}",
            derived.sas, self.confirm_hint, derived.sas
        );
        p.derived = Some(derived);
        Ok(br#"{"ok":true}"#.to_vec())
    }

    fn unseal(&mut self, body: &[u8]) -> Result<TvMessage, Fail> {
        self.expire();
        self.poll_confirm_file();
        let s: Sealed = serde_json::from_slice(body).map_err(|_| (400, "malformed envelope".into()))?;
        let p = self
            .pairing
            .as_mut()
            .filter(|p| p.id == s.pairing_id && p.derived.is_some())
            .ok_or((401, "not paired".to_string()))?;
        if s.ctr == 0 || s.ctr.saturating_add(REPLAY_WINDOW) <= p.rx_max || p.rx_seen.contains(&s.ctr) {
            return Err((409, "replayed or stale message".into()));
        }
        let ct = B64.decode(&s.ct).map_err(|_| (400, "malformed ciphertext".to_string()))?;
        let d = p.derived.as_ref().expect("checked above");
        let plain = open(&d.tv_to_desktop, s.ctr, &aad(&p.id, MSG_PATH), &ct)
            .ok_or((401, "authentication failed".to_string()))?;
        p.rx_seen.insert(s.ctr);
        p.rx_max = p.rx_max.max(s.ctr);
        let floor = p.rx_max.saturating_sub(REPLAY_WINDOW);
        p.rx_seen = p.rx_seen.split_off(&floor);
        p.last_seen = Instant::now();
        serde_json::from_slice(&plain).map_err(|_| (400, "malformed message".into()))
    }

    fn reply(&mut self, m: &DesktopMessage) -> Result<Response, Fail> {
        let p = self.pairing.as_mut().ok_or((401, "not paired".to_string()))?;
        p.tx_ctr += 1;
        let d = p.derived.as_ref().ok_or((401, "not paired".to_string()))?;
        let plain = serde_json::to_vec(m).expect("serialise");
        let ct = seal(&d.desktop_to_tv, p.tx_ctr, &aad(&p.id, MSG_PATH), &plain);
        let env = Sealed { pairing_id: p.id.clone(), ctr: p.tx_ctr, ct: B64.encode(ct) };
        Ok(Response::json(serde_json::to_vec(&env).expect("serialise")))
    }

    fn seal_bytes(&mut self, bytes: &[u8]) -> Result<Response, Fail> {
        let p = self.pairing.as_mut().ok_or((401, "not paired".to_string()))?;
        p.tx_ctr += 1;
        let d = p.derived.as_ref().ok_or((401, "not paired".to_string()))?;
        Ok(Response {
            status: 200,
            content_type: "application/octet-stream",
            headers: vec![(CTR_HEADER, p.tx_ctr.to_string())],
            body: seal(&d.desktop_to_tv, p.tx_ctr, &aad(&p.id, MSG_PATH), bytes),
        })
    }

    fn pairing_status(&self) -> DesktopMessage {
        let p = self.pairing.as_ref();
        DesktopMessage::Pairing {
            confirmed_tv: p.is_some_and(|p| p.confirmed_tv),
            confirmed_desktop: p.is_some_and(|p| p.confirmed_desktop),
        }
    }

    /// Something for a poll to answer with now, if anything is new.
    fn news(&mut self, after: Option<&str>) -> Option<DesktopMessage> {
        if let Some(p) = self.app.presentation(after) {
            return Some(DesktopMessage::Presentation(p));
        }
        let fields = self.app.fields();
        let pairing = self.pairing.as_mut()?;
        if fields.is_empty() || pairing.sent_fields.as_ref() == Some(&fields) {
            return None;
        }
        pairing.sent_fields = Some(fields.clone());
        Some(DesktopMessage::Fields { values: fields, notice: None })
    }
}

/// Answer one request. `serve` calls this for each connection; it is
/// public so another transport, or a test, can drive the server directly.
pub fn handle<A: App>(server: &Arc<Mutex<Server<A>>>, method: &str, path: &str, body: &[u8]) -> Response {
    if method == "OPTIONS" {
        return Response { status: 204, content_type: "text/plain", headers: vec![], body: vec![] };
    }
    if method != "POST" {
        return Response::error(404, "not found");
    }
    let result = match path {
        "/lanlink/v0/pair/commit" => server.lock().unwrap().pair_commit(body).map(Response::json),
        "/lanlink/v0/pair/reveal" => server.lock().unwrap().pair_reveal(body).map(Response::json),
        MSG_PATH => message(server, body),
        _ => Err((404, "not found".into())),
    };
    result.unwrap_or_else(|(s, m)| Response::error(s, &m))
}

fn message<A: App>(server: &Arc<Mutex<Server<A>>>, body: &[u8]) -> Result<Response, Fail> {
    let msg = server.lock().unwrap().unseal(body)?;
    let need_pairing = |s: &Server<A>| if s.paired() { Ok(()) } else { Err((401, "not paired".to_string())) };
    match msg {
        TvMessage::Confirm => {
            let mut s = server.lock().unwrap();
            if let Some(p) = s.pairing.as_mut() {
                p.confirmed_tv = true;
                println!("CONFIRMED on TV");
                if p.paired() {
                    println!("PAIRED");
                }
            }
            let status = s.pairing_status();
            s.reply(&status)
        }
        TvMessage::Poll { after } => {
            let deadline = Instant::now() + Duration::from_secs(POLL_WAIT_SECS);
            loop {
                {
                    let mut s = server.lock().unwrap();
                    s.poll_confirm_file();
                    if !s.paired() {
                        let status = s.pairing_status();
                        return s.reply(&status);
                    }
                    if let Some(m) = s.news(after.as_deref()) {
                        return s.reply(&m);
                    }
                    if Instant::now() >= deadline {
                        return s.reply(&DesktopMessage::Idle);
                    }
                    if let Some(p) = s.pairing.as_mut() {
                        p.last_seen = Instant::now();
                    }
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
        TvMessage::Artefact { id } => {
            let mut s = server.lock().unwrap();
            need_pairing(&s)?;
            match s.app.artefact(&id) {
                Some(b) => s.seal_bytes(&b),
                None => s.reply(&DesktopMessage::Refused { reason: "not an artefact of the current presentation".into() }),
            }
        }
        TvMessage::Act { presentation_id, name, arg } => {
            let mut s = server.lock().unwrap();
            need_pairing(&s)?;
            let m = s.app.act(presentation_id.as_deref(), &name, arg.as_deref());
            // Fields an act returns count as sent, so the next poll does
            // not repeat them.
            if let (DesktopMessage::Fields { values, .. }, Some(p)) = (&m, s.pairing.as_mut()) {
                let mut merged = p.sent_fields.clone().unwrap_or_default();
                merged.extend(values.clone());
                p.sent_fields = Some(merged);
            }
            s.reply(&m)
        }
        TvMessage::Decide(d) => {
            let mut s = server.lock().unwrap();
            need_pairing(&s)?;
            let m = s.app.decide(d);
            s.reply(&m)
        }
    }
}

// ------------------------------------------------------------------ HTTP

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Result<Request, Fail> {
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    let mut reader = BufReader::new(stream.try_clone().map_err(|_| (500, "io".to_string()))?);
    let mut line = String::new();
    reader.by_ref().take(8192).read_line(&mut line).map_err(|_| (400, "bad request line".to_string()))?;
    let mut parts = line.split_whitespace();
    let (method, path) = match (parts.next(), parts.next()) {
        (Some(m), Some(p)) => (m.to_string(), p.to_string()),
        _ => return Err((400, "bad request line".into())),
    };
    let mut length = 0usize;
    for _ in 0..64 {
        let mut h = String::new();
        reader.by_ref().take(8192).read_line(&mut h).map_err(|_| (400, "bad header".to_string()))?;
        let h = h.trim_end();
        if h.is_empty() {
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).map_err(|_| (400, "short body".to_string()))?;
            return Ok(Request { method, path, body });
        }
        if let Some((name, value)) = h.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().map_err(|_| (400, "bad length".to_string()))?;
                if length > MAX_REQUEST_BYTES {
                    return Err((413, "request too large".into()));
                }
            }
        }
    }
    Err((431, "too many headers".into()))
}

fn write_response(stream: &mut TcpStream, r: &Response) {
    let reason = match r.status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        _ => "Error",
    };
    let mut head = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nAccess-Control-Expose-Headers: {CTR_HEADER}\r\n",
        r.status,
        r.content_type,
        r.body.len()
    );
    for (k, v) in &r.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&r.body);
}

/// Serve until the process is stopped.
pub fn serve<A: App>(bind: &str, server: Server<A>) -> io::Result<()> {
    let listener = TcpListener::bind(bind)?;
    println!("SERVING {PROTOCOL} on http://{bind}/ (POC: not a reviewed pairing protocol, no TLS)");
    let server = Arc::new(Mutex::new(server));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let s = Arc::clone(&server);
        std::thread::spawn(move || {
            let r = match read_request(&mut stream) {
                Ok(req) => handle(&s, &req.method, &req.path, &req.body),
                Err((status, m)) => Response::error(status, &m),
            };
            write_response(&mut stream, &r);
        });
    }
    Ok(())
}

/// Hand the code read on the TV to a running server.
pub fn confirm(state_dir: &Path, code: &str) -> io::Result<()> {
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the code is six digits"));
    }
    fs::write(state_dir.join("confirm"), code)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter {
        n: i64,
    }
    impl App for Counter {
        fn fields(&mut self) -> BTreeMap<String, FieldValue> {
            BTreeMap::from([("n".to_string(), FieldValue::Int(self.n))])
        }
        fn act(&mut self, presentation_id: Option<&str>, name: &str, arg: Option<&str>) -> DesktopMessage {
            if name != "add" || presentation_id.is_some() {
                return DesktopMessage::Refused { reason: "no".into() };
            }
            self.n += arg.and_then(|a| a.parse::<i64>().ok()).unwrap_or(1);
            DesktopMessage::Fields { values: self.fields(), notice: Some("Added".into()) }
        }
    }

    /// The TV end, written with ring as the generated runtime does with
    /// WebCrypto.
    struct Tv {
        pid: String,
        keys: Derived,
        tx: u64,
    }
    impl Tv {
        fn send(&mut self, s: &Arc<Mutex<Server<Counter>>>, m: &TvMessage) -> Response {
            self.tx += 1;
            self.send_ctr(s, m, self.tx)
        }
        fn send_ctr(&self, s: &Arc<Mutex<Server<Counter>>>, m: &TvMessage, ctr: u64) -> Response {
            let ct = seal(&self.keys.tv_to_desktop, ctr, &aad(&self.pid, MSG_PATH), &serde_json::to_vec(m).unwrap());
            let env = Sealed { pairing_id: self.pid.clone(), ctr, ct: B64.encode(ct) };
            handle(s, "POST", MSG_PATH, &serde_json::to_vec(&env).unwrap())
        }
        fn ask(&mut self, s: &Arc<Mutex<Server<Counter>>>, m: &TvMessage) -> DesktopMessage {
            let r = self.send(s, m);
            self.read(&r)
        }
        fn read(&self, r: &Response) -> DesktopMessage {
            assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
            let env: Sealed = serde_json::from_slice(&r.body).unwrap();
            let plain = open(&self.keys.desktop_to_tv, env.ctr, &aad(&self.pid, MSG_PATH), &B64.decode(env.ct).unwrap()).unwrap();
            serde_json::from_slice(&plain).unwrap()
        }
    }

    fn pair(dir: &Path) -> (Arc<Mutex<Server<Counter>>>, Tv) {
        let s = Arc::new(Mutex::new(Server::new(dir, "test confirm", Counter { n: 1 }).unwrap()));
        let rng = SystemRandom::new();
        let tv_key = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let tv_pub = tv_key.compute_public_key().unwrap().as_ref().to_vec();
        let commit = PairCommit { protocol: PROTOCOL.into(), commitment: hex(&sha256(&tv_pub)) };
        let r = handle(&s, "POST", "/lanlink/v0/pair/commit", &serde_json::to_vec(&commit).unwrap());
        let reply: PairCommitReply = serde_json::from_slice(&r.body).unwrap();
        let desk_pub = B64.decode(&reply.desktop_pub).unwrap();
        let keys = agreement::agree_ephemeral(
            tv_key,
            &agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, desk_pub.clone()),
            |z| derive(z, &tv_pub, &desk_pub),
        )
        .unwrap();
        let reveal = PairReveal { pairing_id: reply.pairing_id.clone(), tv_pub: B64.encode(&tv_pub) };
        assert_eq!(handle(&s, "POST", "/lanlink/v0/pair/reveal", &serde_json::to_vec(&reveal).unwrap()).status, 200);
        (s, Tv { pid: reply.pairing_id, keys, tx: 0 })
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("goya-lanlink-peer-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn pairs_pushes_fields_and_takes_acts_without_a_presentation() {
        let dir = temp("flow");
        let (s, mut tv) = pair(&dir);
        let m = tv.ask(&s, &TvMessage::Confirm);
        assert_eq!(m, DesktopMessage::Pairing { confirmed_tv: true, confirmed_desktop: false });
        // Before the desktop confirms, an act is refused outright.
        let act = TvMessage::Act { presentation_id: None, name: "add".into(), arg: Some("2".into()) };
        assert_eq!(tv.send(&s, &act).status, 401);
        let sas = s.lock().unwrap().pairing.as_ref().unwrap().derived.as_ref().unwrap().sas.clone();
        confirm(&dir, &sas).unwrap();
        // The first poll after pairing pushes the current fields.
        let m = tv.ask(&s, &TvMessage::Poll { after: None });
        assert_eq!(m, DesktopMessage::Fields { values: BTreeMap::from([("n".into(), FieldValue::Int(1))]), notice: None });
        let m = tv.ask(&s, &act);
        assert_eq!(m, DesktopMessage::Fields { values: BTreeMap::from([("n".into(), FieldValue::Int(3))]), notice: Some("Added".into()) });
        // A replayed counter is refused.
        assert_eq!(tv.send_ctr(&s, &act, tv.tx).status, 409);
        // A value that changes on the peer side is pushed on the next poll.
        s.lock().unwrap().app().n = 10;
        let m = tv.ask(&s, &TvMessage::Poll { after: None });
        assert_eq!(m, DesktopMessage::Fields { values: BTreeMap::from([("n".into(), FieldValue::Int(10))]), notice: None });
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_wrong_code_discards_the_pairing() {
        let dir = temp("mismatch");
        let (s, mut tv) = pair(&dir);
        tv.ask(&s, &TvMessage::Confirm);
        confirm(&dir, "000000").unwrap();
        let sas = s.lock().unwrap().pairing.as_ref().unwrap().derived.as_ref().unwrap().sas.clone();
        if sas != "000000" {
            assert_eq!(tv.send(&s, &TvMessage::Poll { after: None }).status, 401);
            assert!(!s.lock().unwrap().paired());
        }
        assert!(confirm(&dir, "12345").is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_reveal_must_match_its_commitment() {
        let dir = temp("commit");
        let s = Arc::new(Mutex::new(Server::new(&dir, "x", Counter { n: 0 }).unwrap()));
        let commit = PairCommit { protocol: PROTOCOL.into(), commitment: hex(&[7u8; 32]) };
        let reply: PairCommitReply =
            serde_json::from_slice(&handle(&s, "POST", "/lanlink/v0/pair/commit", &serde_json::to_vec(&commit).unwrap()).body).unwrap();
        let reveal = PairReveal { pairing_id: reply.pairing_id, tv_pub: B64.encode([4u8; 65]) };
        assert_eq!(handle(&s, "POST", "/lanlink/v0/pair/reveal", &serde_json::to_vec(&reveal).unwrap()).status, 403);
        let wrong = PairCommit { protocol: "acme/1".into(), commitment: hex(&[7u8; 32]) };
        assert_eq!(handle(&s, "POST", "/lanlink/v0/pair/commit", &serde_json::to_vec(&wrong).unwrap()).status, 400);
        fs::remove_dir_all(&dir).unwrap();
    }
}
