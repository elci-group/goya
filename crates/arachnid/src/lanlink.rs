//! `goya.lanlink/0`: Goya's platform link to a peer on the LAN, lowered for
//! webOS. Contract `arachnid.lanlink/0.0.1`.
//!
//! A scene declares the link in Goya (`@link`, GSSH/3 live admission). It
//! says which `host` keys the link writes (`status`, `code`, `detail`,
//! `error`, `outbox`, `mode` and one image and label per slot) and
//! addresses the link with D2 `network` requests whose first word is the
//! link's name:
//!
//! | Request | Effect |
//! | --- | --- |
//! | `NAME pair` | start pairing |
//! | `NAME confirm` | confirm the code shown |
//! | `NAME cancel` / `NAME forget` | drop the pairing, staged images and queued decisions |
//! | `NAME retry` | resume after an error |
//! | `NAME decide APPROVE\|REJECT\|SELECT\|DEFER\|RATE_n [SLOT]` | queue a decision on the shown presentation |
//! | `NAME playback toggle\|back\|forward\|replay\|mute SLOT` | control the slot's video locally |
//! | `NAME act ACT [ARG]` | send a declared act; the peer answers with `field` values or a new revision |
//!
//! Every request returns text. Arachnid checks each one at compile time
//! (`GY-AR-650`), and the scene itself contains no protocol code. The
//! runtime below is compiler output, like `app.js`, and is the same for
//! every app apart from the configuration table.
//!
//! **Protocol** (peer side: `tapestry-desktop` `tv_link.rs`):
//! * Pairing: ECDH P-256 with a hash commitment from the TV first. Both
//!   sides show a 6-digit code (SAS), and a person confirms it on both.
//! * Keys: HKDF-SHA256.
//! * Messages: AES-256-GCM with a replay window.
//! * The peer offers presentations of named image slots. The TV fetches
//!   each image sealed, and checks its size, SHA-256, signature and decoded
//!   dimensions before publishing it.
//! * Decisions name a slot; the peer maps slots to its artefacts. They
//!   leave the outbox only when the peer acknowledges them.
//! * Credentials are session-only.
//!
//! **Status values** written to the `status` role: 0 unpaired,
//! 1 pairing (code shown), 2 waiting for the peer to confirm, 3 ready,
//! 4 receiving, 5 showing, 6 peer unreachable, 7 error.
//!
//! **Proof of concept.** This is not a reviewed pairing protocol, and it
//! uses no TLS. The endpoint must be a literal `http://IPv4:port`.

use crate::appinfo::json_string;
use crate::diagnostics::{Code, Diagnostic};
use goya_lang::{EffectValueType, InteractionClass, LinkDecl, ParsedScene};

pub const CONTRACT: &str = "arachnid.lanlink/0.0.1";
pub const PROTOCOL: &str = "goya.lanlink/0";
/// The runtime needs async/await, fetch and WebCrypto ECDH/HKDF/AES-GCM.
/// Verified on webOS 22 (Chromium 87); older engines are untested, so
/// they are refused.
pub const MIN_CHROMIUM: u16 = 87;
pub const FILE: &str = "lanlink.js";

const DECISIONS: &[&str] = &["APPROVE", "REJECT", "SELECT", "DEFER", "RATE_1", "RATE_2", "RATE_3", "RATE_4", "RATE_5"];

fn endpoint_ok(e: &str) -> bool {
    let Some(rest) = e.strip_prefix("http://") else { return false };
    let Some((host, port)) = rest.split_once(':') else { return false };
    let octets: Vec<&str> = host.split('.').collect();
    octets.len() == 4
        && octets.iter().all(|o| !o.is_empty() && o.len() <= 3 && o.parse::<u8>().is_ok())
        && port.parse::<u16>().is_ok_and(|p| p > 0)
}

/// Check the scene's links and every request addressed to them.
pub fn check(parsed: &ParsedScene) -> Vec<Diagnostic> {
    let links = &parsed.declarations.links;
    let mut out = Vec::new();
    let refuse = |m: String| Diagnostic::new(Code::HostContract, m);
    if links.is_empty() {
        for d in parsed.declarations.state.iter().flat_map(|(d, _)| d) {
            if d.lifetime == goya_lang::StateLifetime::Host {
                out.push(refuse(format!("host state '{}' has no writer; declare a @link that binds it", d.key)).at(Some(d.range)));
            }
        }
    }
    if links.len() > 1 {
        out.push(refuse("only one @link per scene is lowered to webOS".into()));
    }
    for l in links {
        if l.protocol != PROTOCOL {
            out.push(refuse(format!("link {} speaks {}; Arachnid lowers only {PROTOCOL}", l.name, l.protocol)).at(Some(l.range)));
        }
        if !endpoint_ok(&l.endpoint) {
            out.push(refuse(format!("link {} endpoint must be http://IPv4:port on the LAN, not {}", l.name, l.endpoint)).at(Some(l.range)));
        }
    }
    for i in parsed.scene.interactions.iter().filter(|i| i.class == InteractionClass::D2) {
        for r in &i.requests {
            let words: Vec<&str> = r.request_utf8.split(' ').collect();
            // Requests to an `@apps` declaration are checked by apps.rs.
            if parsed.declarations.apps.iter().any(|a| words.first() == Some(&a.name.as_str())) {
                continue;
            }
            let link = links.iter().find(|l| words.first() == Some(&l.name.as_str()));
            let Some(link) = link else {
                out.push(refuse(format!(
                    "request \"{}\" in {} is addressed to no @link or @apps; nothing on webOS would serve it",
                    r.request_utf8, i.name
                )));
                continue;
            };
            if r.effect_kind.as_str() != "network" || r.result_type != EffectValueType::Text {
                out.push(refuse(format!("requests to link {} are network effects with a text result ({})", link.name, i.name)));
            }
            let slot_ok = |s: &str| link.slots.iter().any(|x| x.name == s);
            let act_ok = |a: &str| link.acts.iter().any(|x| x == a);
            let arg_ok = |a: &str| !a.is_empty() && a.len() <= 16 && a.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
            let ok = match words.as_slice() {
                [_, "act", a] => act_ok(a),
                [_, "act", a, arg] => act_ok(a) && arg_ok(arg),
                [_, "pair" | "confirm" | "cancel" | "forget" | "retry"] => true,
                [_, "decide", d] => DECISIONS.contains(d) && *d == "DEFER",
                [_, "playback", "toggle" | "replay" | "back" | "forward" | "mute", s] => slot_ok(s),
                [_, "decide", d, s] => DECISIONS.contains(d) && slot_ok(s),
                _ => false,
            };
            if !ok {
                out.push(refuse(format!(
                    "request \"{}\" in {} is not a {PROTOCOL} request (pair, confirm, cancel, forget, retry, decide APPROVE|REJECT|SELECT|RATE_n SLOT / DEFER [SLOT] with a declared slot, playback ACTION SLOT, or act NAME [ARG] with a declared act)",
                    r.request_utf8, i.name
                )));
            }
        }
    }
    out
}

fn config_js(l: &LinkDecl) -> String {
    let roles: Vec<String> = l
        .roles
        .iter()
        .map(|(r, k)| format!("{}:{}", json_string(r), json_string(k)))
        .collect();
    let slots: Vec<String> = l
        .slots
        .iter()
        .map(|s| {
            format!(
                "{}:{{image:{},label:{}}}",
                json_string(&s.name),
                json_string(&s.image),
                s.label.as_deref().map_or("null".into(), json_string)
            )
        })
        .collect();
    let fields: Vec<String> = l
        .fields
        .iter()
        .map(|(n, k)| format!("{}:{}", json_string(n), json_string(k)))
        .collect();
    format!(
        "{{name:{},endpoint:{},roles:{{{}}},slots:{{{}}},fields:{{{}}}}}",
        json_string(&l.name),
        json_string(&l.endpoint),
        roles.join(","),
        slots.join(","),
        fields.join(",")
    )
    .replace('<', "\\u003c")
}

/// The runtime for the scene's link, or `None` when it declares none.
pub fn runtime_js(parsed: &ParsedScene) -> Option<String> {
    let l = parsed.declarations.links.first()?;
    Some(
        RUNTIME
            .replace("__CONTRACT__", CONTRACT)
            .replace("__PROTOCOL__", PROTOCOL)
            .replace("__CONFIG__", &config_js(l)),
    )
}

const RUNTIME: &str = r#"/* Generated by Arachnid (__CONTRACT__): the __PROTOCOL__ link runtime.
   Compiler output, identical for every app apart from CONFIG.
   Proof of concept: not a reviewed pairing protocol, no TLS. */
(function () {
  "use strict";
  var CONFIG = __CONFIG__;
  var PROTOCOL = "__PROTOCOL__";
  var MSG = "/lanlink/v0/msg";
  var MAX_VIDEO_BYTES = 64 * 1024 * 1024, MAX_BYTES = 16 * 1024 * 1024, MAX_PIXELS = 16000000, WINDOW = 64;
  var ST = { UNPAIRED: 0, PAIRING: 1, WAITING: 2, READY: 3, RECEIVING: 4, SHOWING: 5, UNREACHABLE: 6, ERROR: 7 };
  var S = window.crypto && window.crypto.subtle, A = window.Arachnid;
  var enc = new TextEncoder(), dec = new TextDecoder();
  var peer = CONFIG.endpoint.replace(/^http:\/\//, "");

  function concat(a, b) { var o = new Uint8Array(a.length + b.length); o.set(a, 0); o.set(b, a.length); return o; }
  function hex(buf) { var a = new Uint8Array(buf), h = ""; for (var i = 0; i < a.length; i++) h += (a[i] < 16 ? "0" : "") + a[i].toString(16); return h; }
  function b64(buf) { var a = new Uint8Array(buf), s = ""; for (var i = 0; i < a.length; i += 0x8000) s += String.fromCharCode.apply(null, a.subarray(i, i + 0x8000)); return btoa(s); }
  function unb64(s) { var bin = atob(s), a = new Uint8Array(bin.length); for (var i = 0; i < bin.length; i++) a[i] = bin.charCodeAt(i); return a; }
  function nonce(ctr) { var n = new Uint8Array(12), v = ctr; for (var i = 11; i >= 4; i--) { n[i] = v % 256; v = Math.floor(v / 256); } return n; }
  function aad(pid, path) { return concat(concat(enc.encode(PROTOCOL), new Uint8Array([0])), concat(enc.encode(pid), concat(new Uint8Array([0]), enc.encode(path)))); }

  async function derive(z, tvPub, peerPub) {
    var salt = await S.digest("SHA-256", concat(tvPub, peerPub));
    var ikm = await S.importKey("raw", z, "HKDF", false, ["deriveBits"]);
    var okm = new Uint8Array(await S.deriveBits({ name: "HKDF", hash: "SHA-256", salt: salt, info: enc.encode(PROTOCOL + " keys") }, ikm, 512));
    var sb = new Uint8Array(await S.deriveBits({ name: "HKDF", hash: "SHA-256", salt: salt, info: enc.encode(PROTOCOL + " sas") }, ikm, 32));
    var n = ((sb[0] * 16777216) + (sb[1] << 16) + (sb[2] << 8) + sb[3]) % 1000000;
    return { tx: okm.slice(0, 32), rx: okm.slice(32, 64), sas: ("00000" + n).slice(-6) };
  }
  async function aesKey(raw, use) { return S.importKey("raw", raw, "AES-GCM", false, [use]); }
  async function seal(k, ctr, ad, p) { return new Uint8Array(await S.encrypt({ name: "AES-GCM", iv: nonce(ctr), additionalData: ad }, k, p)); }
  async function open(k, ctr, ad, c) { return new Uint8Array(await S.decrypt({ name: "AES-GCM", iv: nonce(ctr), additionalData: ad }, k, c)); }
  /* Exposed for the shared golden-vector check only; holds no keys. */
  window.GoyaLanlinkCrypto = { derive: derive, aesKey: aesKey, seal: seal, aad: aad, hex: hex, b64: b64 };
  if (!A) return;

  function role(r, v) { if (CONFIG.roles[r]) A.set(CONFIG.roles[r], v); }
  function pauseVideos() { var v = document.querySelectorAll("video"); for (var i = 0; i < v.length; i++) v[i].pause(); }
  function status(v) { if (v !== ST.SHOWING) pauseVideos(); role("status", v); }
  document.addEventListener("visibilitychange", function () { if (document.hidden) pauseVideos(); });
  function current() { return CONFIG.roles.status ? A.get(CONFIG.roles.status) : null; }
  function fail(m) { role("error", m); status(ST.ERROR); }
  function LinkLost(m) { this.message = m; }
  function sleep(ms) { return new Promise(function (ok) { setTimeout(ok, ms); }); }

  var link = null, pending = null, generation = 0, shown = null, lastShown = null, outbox = [], flushing = null;

  function clearSlots() {
    for (var s in CONFIG.slots) {
      if (CONFIG.slots.hasOwnProperty(s)) {
        A.clearImage(CONFIG.slots[s].image);
        if (CONFIG.slots[s].label) A.set(CONFIG.slots[s].label, "");
      }
    }
  }
  function showOutbox() {
    var n = outbox.length;
    role("outbox", n === 0 ? "" : n === 1 ? "1 decision waiting for " + peer : n + " decisions waiting for " + peer);
  }
  /* Forget clears credentials, staged images and queued decisions. */
  function forget() {
    generation++;
    link = null; pending = null; shown = null; lastShown = null; outbox = [];
    clearSlots(); role("code", ""); role("mode", ""); showOutbox();
    role("detail", peer + " · not paired");
    status(ST.UNPAIRED);
  }

  async function postJson(path, body) {
    var r = await fetch(CONFIG.endpoint + path, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body), cache: "no-store" });
    var j = null; try { j = JSON.parse(await r.text()); } catch (e) { /* refused below */ }
    if (!r.ok) throw new Error((j && j.error) || ("HTTP " + r.status));
    return j;
  }
  function accept(ctr) { if (!(ctr >= 1) || ctr + WINDOW <= link.rxMax || link.rxSeen[ctr]) throw new Error("replayed or stale reply"); }
  function note(ctr) { link.rxSeen[ctr] = true; if (ctr > link.rxMax) link.rxMax = ctr; for (var k in link.rxSeen) if (+k + WINDOW <= link.rxMax) delete link.rxSeen[k]; }

  async function send(msg) {
    var l = link;
    if (!l) throw new LinkLost("not paired");
    var ctr = ++l.tx, ad = aad(l.pid, MSG);
    var ct = await seal(l.kTx, ctr, ad, enc.encode(JSON.stringify(msg)));
    var r = await fetch(CONFIG.endpoint + MSG, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ pairing_id: l.pid, ctr: ctr, ct: b64(ct) }), cache: "no-store" });
    if (link !== l) throw new LinkLost("forgotten");
    if (r.status === 401) throw new LinkLost(peer + " no longer recognises this TV");
    if (!r.ok) { var e = null; try { e = await r.json(); } catch (x) { /* none */ } throw new Error((e && e.error) || ("HTTP " + r.status)); }
    if ((r.headers.get("Content-Type") || "").indexOf("application/octet-stream") === 0) {
      var rc = parseInt(r.headers.get("X-Goya-Ctr"), 10);
      accept(rc);
      var bytes = await open(l.kRx, rc, ad, new Uint8Array(await r.arrayBuffer()));
      note(rc);
      return { bytes: bytes };
    }
    var env = await r.json();
    if (env.pairing_id !== l.pid) throw new Error("reply for another pairing");
    accept(env.ctr);
    var plain = await open(l.kRx, env.ctr, ad, unb64(env.ct));
    note(env.ctr);
    return JSON.parse(dec.decode(plain));
  }

  async function pair() {
    var gen = ++generation;
    link = null;
    role("detail", peer + " · pairing");
    var kp = await S.generateKey({ name: "ECDH", namedCurve: "P-256" }, false, ["deriveBits"]);
    var tvPub = new Uint8Array(await S.exportKey("raw", kp.publicKey));
    var c = await postJson("/lanlink/v0/pair/commit", { protocol: PROTOCOL, commitment: hex(await S.digest("SHA-256", tvPub)) });
    var peerPub = unb64(c.desktop_pub);
    await postJson("/lanlink/v0/pair/reveal", { pairing_id: c.pairing_id, tv_pub: b64(tvPub) });
    var pk = await S.importKey("raw", peerPub, { name: "ECDH", namedCurve: "P-256" }, false, []);
    var d = await derive(new Uint8Array(await S.deriveBits({ name: "ECDH", public: pk }, kp.privateKey, 256)), tvPub, peerPub);
    if (gen !== generation) return "Cancelled";
    pending = { pid: c.pairing_id, kTx: await aesKey(d.tx, "encrypt"), kRx: await aesKey(d.rx, "decrypt") };
    role("code", d.sas.slice(0, 3) + " " + d.sas.slice(3));
    status(ST.PAIRING);
    return "Compare the codes";
  }

  async function confirm() {
    if (!pending) throw new Error("no pairing to confirm");
    var gen = generation;
    link = { pid: pending.pid, kTx: pending.kTx, kRx: pending.kRx, tx: 0, rxMax: 0, rxSeen: {} };
    pending = null;
    status(ST.WAITING);
    var m = await send({ type: "confirm" });
    if (m.type !== "pairing") throw new Error("unexpected reply");
    waitForPeer(gen);
    return "Confirmed here";
  }

  async function waitForPeer(gen) {
    while (gen === generation) {
      try {
        var m = await send({ type: "poll", after: null });
        if (gen !== generation) return;
        if (m.type === "fields") applyFields(m.values);
        if ((m.type === "pairing" && m.confirmed_desktop && m.confirmed_tv) || m.type === "presentation" || m.type === "idle" || m.type === "fields") break;
      } catch (e) {
        if (gen !== generation) return;
        if (e instanceof LinkLost) { forget(); fail("Pairing ended: " + e.message + "."); return; }
      }
      await sleep(1000);
    }
    if (gen !== generation) return;
    role("detail", peer + " · paired");
    status(ST.READY);
    poll(gen);
  }

  function sigOk(b, mime) {
    if (mime === "video/mp4") return b.length >= 12 && b[4] === 102 && b[5] === 116 && b[6] === 121 && b[7] === 112;
    if (mime === "image/png") return b.length > 8 && b[0] === 0x89 && b[1] === 0x50 && b[2] === 0x4e && b[3] === 0x47;
    return mime === "image/jpeg" && b.length > 3 && b[0] === 0xff && b[1] === 0xd8 && b[2] === 0xff;
  }
  function valid(p) {
    if (p.mode !== "PRESENT" && p.mode !== "COMPARE" && p.mode !== "WEB") return false;
    if (!p.slots || p.slots.map(function (s) { return s.slot; }).join(",") !== (p.mode === "COMPARE" ? "left,right" : "main")) return false;
    if (p.mode === "WEB" && p.slots[0].mime !== "image/png") return false;
    if (typeof p.id !== "string" || typeof p.mode !== "string" || !p.slots || !p.slots.length) return false;
    for (var i = 0; i < p.slots.length; i++) {
      var a = p.slots[i];
      if (!CONFIG.slots.hasOwnProperty(a.slot)) return false;
      var video = a.mime === "video/mp4";
      if (video && p.mode !== "PRESENT") return false;
      if ((a.mime !== "image/png" && a.mime !== "image/jpeg" && !video) || !(a.byte_length > 0 && a.byte_length <= (video ? MAX_VIDEO_BYTES : MAX_BYTES))) return false;
      if (!(a.width > 0 && a.height > 0 && a.width * a.height <= MAX_PIXELS) || !/^[0-9a-f]{64}$/.test(a.sha256)) return false;
    }
    return true;
  }
  function decoded(blob) {
    return new Promise(function (ok, no) {
      var url = URL.createObjectURL(blob), video = blob.type === "video/mp4";
      var img = video ? document.createElement("video") : new Image();
      var timer = setTimeout(function () { done(new Error("media decode timed out")); }, 15000);
      function done(error) {
        clearTimeout(timer);
        var size = video ? [img.videoWidth, img.videoHeight] : [img.naturalWidth, img.naturalHeight];
        img.onload = img.onloadedmetadata = img.onerror = null;
        if (video) { img.pause(); img.removeAttribute("src"); img.load(); }
        URL.revokeObjectURL(url);
        if (error) no(error); else ok(size);
      }
      if (video) {
        if (!img.canPlayType("video/mp4")) { done(new Error("MP4 playback is unavailable on this TV")); return; }
        img.preload = "metadata";
        img.onloadedmetadata = function () { done(); };
        img.onerror = function () { done(new Error("This TV cannot decode the MP4 video")); };
        img.src = url;
        return;
      }
      img.onload = function () { done(); };
      img.onerror = function () { done(new Error("an image does not decode")); };
      img.src = url;
    });
  }

  /* Peer-set values for the scene's `field` bindings. A value of the wrong
     type for its key is refused by Arachnid.set and reported. */
  function applyFields(values) {
    if (!values) return;
    for (var name in values) {
      if (values.hasOwnProperty(name) && CONFIG.fields.hasOwnProperty(name)) A.set(CONFIG.fields[name], values[name]);
    }
  }

  /* Stage and verify every slot; publish only complete, verified images.
     A new revision of the item already shown is swapped in place: the
     screen stays put while the re-render is fetched and verified. */
  async function receive(p, gen) {
    var inPlace = !!(shown && p.item && shown.item === p.item);
    if (!inPlace) status(ST.RECEIVING);
    var blobs = [];
    for (var i = 0; i < p.slots.length; i++) {
      var a = p.slots[i];
      role("detail", peer + (inPlace ? " · re-rendering" : " · receiving " + (i + 1) + " of " + p.slots.length));
      var r = await send({ type: "artefact", id: a.id });
      if (gen !== generation) return;
      if (!r.bytes) throw new Error(peer + " refused an image: " + (r.reason || r.type));
      if (r.bytes.length !== a.byte_length) throw new Error("an image's size does not match its manifest");
      if (hex(await S.digest("SHA-256", r.bytes)) !== a.sha256) throw new Error("an image's digest does not match its manifest");
      if (!sigOk(r.bytes, a.mime)) throw new Error("an image is not of its declared type");
      var blob = new Blob([r.bytes], { type: a.mime }), size = await decoded(blob);
      if (size[0] !== a.width || size[1] !== a.height) throw new Error("an image's dimensions do not match its manifest");
      blobs.push(blob);
    }
    if (gen !== generation) return;
    if (!inPlace) clearSlots();
    applyFields(p.fields);
    for (i = 0; i < p.slots.length; i++) {
      var s = CONFIG.slots[p.slots[i].slot];
      A.setImage(s.image, blobs[i], p.slots[i].id);
      if (s.label) A.set(s.label, p.slots[i].label + " · " + p.slots[i].width + "×" + p.slots[i].height);
    }
    role("mode", p.mode === "PRESENT" && p.slots[0].mime === "video/mp4" ? "VIDEO" : p.mode);
    shown = p; lastShown = p.id;
    role("detail", peer + " · showing");
    status(ST.SHOWING);
  }

  async function poll(gen) {
    var backoff = 1000;
    while (gen === generation) {
      try {
        await flush(gen);
        var m = await send({ type: "poll", after: lastShown });
        if (gen !== generation) return;
        backoff = 1000;
        if (current() === ST.UNREACHABLE) { role("detail", peer + " · reconnected"); status(shown ? ST.SHOWING : ST.READY); }
        if (m.type === "pairing") { forget(); fail(peer + " ended the pairing."); return; }
        if (m.type === "fields") applyFields(m.values);
        if (m.type === "presentation") {
          if (!valid(m)) throw new Error(peer + " offered a presentation outside the limits or with an undeclared slot");
          if (outbox.length === 0) await receive(m, gen);
        }
      } catch (e) {
        if (gen !== generation) return;
        if (e instanceof LinkLost) { forget(); fail("Pairing ended: " + e.message + "."); return; }
        if (e instanceof TypeError) {
          role("detail", peer + " · unreachable, retrying");
          status(ST.UNREACHABLE);
          await sleep(backoff);
          backoff = Math.min(backoff * 2, 10000);
        } else { shown = null; clearSlots(); fail(e.message || String(e)); return; }
      }
    }
  }

  function clientId() { var a = new Uint8Array(16); crypto.getRandomValues(a); return hex(a); }
  /* Decisions leave the outbox only once the peer has persisted and
     acknowledged them. One flush at a time. */
  function flush(gen) {
    var run = (flushing || Promise.resolve()).then(function () { return doFlush(gen); }, function () { return doFlush(gen); });
    flushing = run.then(function () { flushing = null; }, function () { flushing = null; });
    return run;
  }
  async function doFlush(gen) {
    while (outbox.length && gen === generation) {
      var item = outbox[0];
      var m = await send({ type: "decide", client_review_id: item.client_review_id, presentation_id: item.presentation_id, decision: item.decision, slot: item.slot });
      if (outbox[0] !== item) continue;
      if (m.type !== "review_ack" || m.client_review_id !== item.client_review_id) throw new Error(peer + " refused the decision: " + (m.reason || m.type));
      outbox.shift();
      showOutbox();
      if (shown && shown.id === item.presentation_id) { shown = null; clearSlots(); role("mode", ""); status(ST.READY); }
    }
  }
  async function decide(decision, slot) {
    if (!shown || !link) throw new Error("nothing is shown");
    if (slot && !shown.slots.some(function (s) { return s.slot === slot; })) throw new Error("slot " + slot + " is not shown");
    if (outbox.some(function (r) { return r.presentation_id === shown.id; })) return "Decision already waiting for " + peer;
    outbox.push({ client_review_id: clientId(), presentation_id: shown.id, decision: decision, slot: slot || null });
    showOutbox();
    var word = /^RATE_[1-5]$/.test(decision) ? "Rated " + decision.slice(-1) + "/5" : { APPROVE: "Approved", REJECT: "Rejected", SELECT: "Chosen", DEFER: "Deferred" }[decision];
    try { await flush(generation); return word + " · acknowledged by " + peer; }
    catch (e) {
      if (e instanceof LinkLost) { forget(); fail("Pairing ended: " + e.message + "."); return word + " · not delivered"; }
      return word + " · waiting for " + peer;
    }
  }

  async function playback(action, slot) {
    if (!shown || current() !== ST.SHOWING) return "No video is showing";
    var binding = CONFIG.slots[slot];
    if (!binding) return "Unknown video slot";
    var all = document.querySelectorAll("video[data-image-from]"), v = null;
    for (var i = 0; i < all.length; i++) {
      if (all[i].getAttribute("data-image-from") === binding.image && !all[i].hidden && all[i].offsetParent !== null) v = all[i];
    }
    if (!v) return "No video in this slot";
    if (action === "toggle") {
      if (v.paused || v.ended) { if (v.ended) v.currentTime = 0; try { await v.play(); } catch (e) { return "Playback could not start; try Play again"; } }
      else v.pause();
      return v.paused ? "Paused" : "Playing";
    }
    if (action === "mute") { v.muted = !v.muted; return v.muted ? "Muted" : "Sound on"; }
    if (!(v.readyState >= 1) || !isFinite(v.duration)) return "Video is loading";
    v.currentTime = action === "replay" ? 0 : Math.max(0, Math.min(v.duration, v.currentTime + (action === "back" ? -10 : 10)));
    if (action === "replay") { try { await v.play(); } catch (e) { return "Ready at start; press Play"; } }
    return action === "replay" ? "Replaying" : Math.floor(v.currentTime) + " seconds";
  }

  /* A declared act: the peer answers with fields, or with a new revision
     of the shown item (for example a re-render). An act needs a pairing,
     not a presentation: with nothing shown, presentation_id is null (a
     dashboard peer such as a household link shows no items). */
  async function act(name, arg) {
    if (!link) throw new Error("not paired");
    var m = await send({ type: "act", presentation_id: shown ? shown.id : null, name: name, arg: arg || null });
    if (m.type === "refused") throw new Error(peer + " refused: " + m.reason);
    if (m.type === "fields") { applyFields(m.values); return m.notice || "Updated"; }
    if (m.type === "presentation") {
      if (!valid(m)) throw new Error(peer + " offered a presentation outside the limits");
      await receive(m, generation);
      return (m.fields && m.fields.notice) || "Updated";
    }
    throw new Error("unexpected reply " + m.type);
  }

  A.provide("network", async function (request) {
    var w = request.split(" ");
    if (w[0] !== CONFIG.name) throw new Error("no link named " + w[0]);
    try {
      if (w[1] === "pair") return await pair();
      if (w[1] === "confirm") return await confirm();
      if (w[1] === "cancel") { forget(); return "Pairing cancelled"; }
      if (w[1] === "forget") { forget(); return peer + " forgotten"; }
      if (w[1] === "retry") { if (link) { status(shown ? ST.SHOWING : ST.READY); poll(generation); return "Retrying"; } forget(); return "Pair again"; }
      if (w[1] === "playback") return await playback(w[2], w[3]);
      if (w[1] === "act") return await act(w[2], w[3]);
      if (w[1] === "decide") return await decide(w[2], w[3]);
      throw new Error("unknown request " + request);
    } catch (e) {
      if (e instanceof LinkLost) { forget(); fail("Pairing ended: " + e.message + "."); return "Pairing ended"; }
      fail(e instanceof TypeError ? peer + " cannot be reached." : (e.message || String(e)));
      return "Failed";
    }
  }, CONFIG.name);

  if (!S) { fail("No WebCrypto in this runtime; the link cannot pair."); return; }
  forget();
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../fixtures/lanlink.gscene");

    fn diags(src: &str) -> Vec<String> {
        let parsed = goya_lang::parse_strict_gssh3_live(src).unwrap_or_else(|e| panic!("{e}"));
        check(&parsed).iter().map(|d| d.message.clone()).collect()
    }

    #[test]
    fn fixture_link_is_lowered() {
        assert!(diags(FIXTURE).is_empty());
        let js = runtime_js(&goya_lang::parse_strict_gssh3_live(FIXTURE).unwrap()).unwrap();
        assert!(js.contains(r#"var CONFIG = {name:"peer",endpoint:"http://127.0.0.1:8771",roles:{"status":"link","detail":"detail"},slots:{"main":{image:"main",label:null}},fields:{}};"#), "{js}");
    }

    #[test]
    fn acts_must_be_declared() {
        let with_act = FIXTURE
            .replace("slot main { image: state.main; }", "act web_row; slot main { image: state.main; }")
            .replace("peer decide APPROVE main", "peer act web_row 3");
        assert!(diags(&with_act).is_empty(), "{:?}", diags(&with_act));
        assert!(diags(&with_act.replace("peer act web_row 3", "peer act web_col 3"))[0].contains("not a goya.lanlink/0 request"));
        assert!(diags(&with_act.replace("peer act web_row 3", "peer act web_row a-b"))[0].contains("not a goya.lanlink/0 request"));
    }

    #[test]
    fn requests_and_links_are_checked_at_compile_time() {
        let undeclared_slot = FIXTURE.replace("peer decide APPROVE main", "peer decide APPROVE left");
        assert!(diags(&undeclared_slot)[0].contains("not a goya.lanlink/0 request"));
        let no_slot = FIXTURE.replace("peer decide APPROVE main", "peer decide APPROVE");
        assert!(diags(&no_slot)[0].contains("not a goya.lanlink/0 request"));
        let stranger = FIXTURE.replace("peer pair", "mailer send");
        assert!(diags(&stranger)[0].contains("addressed to no @link"));
        let hostname = FIXTURE.replace("http://127.0.0.1:8771", "http://desktop.lan:8771");
        assert!(diags(&hostname)[0].contains("endpoint must be http://IPv4:port"));
        let protocol = FIXTURE.replace(r#"protocol: "goya.lanlink/0""#, r#"protocol: "acme.sync/9""#);
        assert!(diags(&protocol)[0].contains("Arachnid lowers only goya.lanlink/0"));
    }

    #[test]
    fn endpoints_are_literal_lan_addresses() {
        assert!(endpoint_ok("http://192.168.1.129:8770"));
        for bad in ["https://192.168.1.129:8770", "http://desktop.lan:8770", "http://192.168.1.129", "http://192.168.1.300:80", "http://1.2.3.4:0"] {
            assert!(!endpoint_ok(bad), "{bad}");
        }
    }
}
