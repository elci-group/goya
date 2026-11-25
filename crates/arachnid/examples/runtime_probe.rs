//! Runtime capability probe, not compiler output.
//!
//! Answers the questions a networked app must settle on the device before
//! its design is fixed:
//! * is the packaged app a secure context?
//! * which WebCrypto primitives work: ECDH P-256, HKDF, HMAC, SHA-256,
//!   AES-GCM and `getRandomValues`?
//! * can the app send a cross-origin `fetch`/XHR with a JSON body (which
//!   needs a CORS preflight) to a LAN host?
//! * can it decode an image from a Blob URL?
//! * can the app read a user-selected USB marker and create a scratch file?
//! * do origin storage and quota APIs work?
//!
//! Each result is shown on screen and POSTed to `--report URL` as text
//! lines prefixed `CAP`.
//!
//! ```text
//! cargo run -p goya-arachnid --example runtime_probe -- --out NEW_DIR --report http://HOST:PORT/
//! ```

use arachnid::appinfo::{json_string, AppInfo};
use arachnid::{emit, ipk};
use std::path::PathBuf;

const PAGE: &str = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Arachnid runtime probe</title>
<style>
*{box-sizing:border-box}
html,body{margin:0;background:#101722;color:#f5f7fc;font:26px system-ui,sans-serif}
main{max-width:1920px;margin:auto;padding:6vh 7vw}
.eyebrow{color:#8de0c9;letter-spacing:.13em;font-size:22px;font-weight:700}
h1{font-size:66px;line-height:1.05;margin:16px 0}
.lead{color:#c5cddd;margin:0 0 38px}
.actions{display:grid;grid-template-columns:1fr 1fr;gap:28px}
article{background:#1a2535;border:1px solid #3a4960;border-radius:22px;padding:32px;min-width:0}
h2{font-size:36px;margin:0}
.hint{min-height:3em;color:#c5cddd;margin:12px 0 24px}
code{font-size:.8em;overflow-wrap:anywhere}
button,summary{font:inherit;font-weight:700;cursor:pointer}
button{border:0;border-radius:12px;padding:18px 25px;background:#9ce9d4;color:#14242a}
button:disabled{opacity:.6}
.file-control{display:inline-block;position:relative;border-radius:12px;padding:18px 25px;background:#9ce9d4;color:#14242a;font-weight:700}
.file-control:focus-within{outline:4px solid #fff;outline-offset:5px}
.file-control input{position:absolute;inset:0;width:100%;height:100%;opacity:0;cursor:pointer}
:focus{outline:4px solid #fff;outline-offset:5px}
.result{font-size:23px;min-height:2.8em;margin:24px 0 0;color:#c5cddd;overflow-wrap:anywhere}
.result[data-tone="ok"]{color:#9ce9d4}
.result[data-tone="attention"]{color:#ffd591}
details{margin-top:32px;color:#c5cddd}
summary{width:max-content;max-width:100%;padding:10px 0}
#out{max-height:24vh;overflow:auto;white-space:pre-wrap;font:19px monospace}
</style>
</head><body><main>
<div class="eyebrow">ARACHNID DEVICE TOOLS</div>
<h1>USB access probe</h1>
<p class="lead">Check whether this app can access the drive connected to your TV.</p>
<div class="actions">
<article>
<h2>Read a USB file</h2>
<p class="hint">Select a text file containing <code>ARACHNID_USB_PROBE_V1</code>.</p>
<label class="file-control">Choose marker file<input id="marker" type="file"></label>
<p id="read-result" class="result" role="status">Read check not started</p>
</article>
<article>
<h2>Write to USB</h2>
<p class="hint">Choose the USB drive. The app creates a new test file and reads it back.</p>
<button id="write" type="button">Try a scratch write</button>
<p id="write-result" class="result" role="status">Write check not started</p>
</article>
</div>
<details><summary id="details">Runtime details</summary><pre id="out" tabindex="0"></pre></details>
</main>
<script>
(function () {
  var REPORT = __REPORT__;
  var out = document.getElementById("out"), lines = [];
  function post(body, type) {
    try { var x = new XMLHttpRequest(); x.open("POST", REPORT, true); x.setRequestHeader("Content-Type", type || "text/plain"); x.send(body); } catch (e) {}
  }
  function cap(name, value) {
    var line = "CAP " + name + " = " + value;
    lines.push(line); out.textContent = lines.join("\n");
    if (REPORT) post(line);
  }
  function err(e) { return "ERROR " + (e && (e.name + ": " + e.message) || e); }
  cap("href", location.href);
  cap("origin", location.origin);
  cap("isSecureContext", String(window.isSecureContext));
  cap("UA", navigator.userAgent);
  cap("hardwareConcurrency", String(navigator.hardwareConcurrency));
  cap("deviceMemory", String(navigator.deviceMemory));
  cap("performance.memory", window.performance && performance.memory ? JSON.stringify({ limit: performance.memory.jsHeapSizeLimit, used: performance.memory.usedJSHeapSize }) : "absent");
  cap("fetch", typeof fetch);
  cap("WebSocket", typeof WebSocket);
  cap("Promise", typeof Promise);
  cap("createImageBitmap", typeof createImageBitmap);
  try { localStorage.setItem("p", "1"); cap("localStorage", localStorage.getItem("p") === "1" ? "works" : "broken"); } catch (e) { cap("localStorage", err(e)); }
  cap("indexedDB", typeof indexedDB);
  var marker = "ARACHNID_USB_PROBE_V1";
  var input = document.getElementById("marker"), write = document.getElementById("write");
  var read = input;
  var pickerTimer;
  var readResult = document.getElementById("read-result");
  var writeResult = document.getElementById("write-result");
  function reportStorage(target, name, value, tone) {
    target.textContent = value;
    target.dataset.tone = tone || "attention";
    cap(name, value);
  }
  function pickerRequested() {
    clearTimeout(pickerTimer);
    reportStorage(readResult, "file picker", "File picker requested. Select a file if a picker opens.", "pending");
    pickerTimer = setTimeout(function () {
      reportStorage(readResult, "file picker", "No file received. If no picker appeared, this TV did not open one.");
    }, 6000);
  }
  input.addEventListener("click", pickerRequested);
  cap("directory picker", typeof window.showDirectoryPicker);
  cap("save picker", typeof window.showSaveFilePicker);
  if (navigator.storage && navigator.storage.estimate) {
    navigator.storage.estimate().then(function (e) {
      cap("origin storage bytes (usage/quota)", e.usage + "/" + e.quota);
    }, function (e) { cap("origin storage estimate", e.name); });
  }
  try {
    var previous = localStorage.getItem("storage-probe-marker");
    cap("internal marker from prior launch", previous === marker ? "present" : "absent");
    localStorage.setItem("storage-probe-marker", marker);
  } catch (e) { cap("internal marker", e.name); }

  input.addEventListener("change", function () {
    clearTimeout(pickerTimer);
    var file = input.files[0];
    if (!file) return;
    if (file.size > 128) {
      reportStorage(readResult, "selected file read", "Choose a marker file of 128 bytes or less.");
    } else {
      var reader = new FileReader();
      reader.onload = function () {
        var matched = reader.result.trim() === marker;
        reportStorage(readResult, "selected file read", matched ? "Marker read successfully." : "File read successfully. Marker text did not match.", matched ? "ok" : "attention");
      };
      reader.onerror = function () {
        reportStorage(readResult, "selected file read", "Could not read the selected file (" + (reader.error ? reader.error.name : "unknown error") + ").");
      };
      reader.readAsText(file);
    }
    input.value = "";
  });

  write.addEventListener("click", async function () {
    if (typeof window.showDirectoryPicker !== "function") {
      reportStorage(writeResult, "directory write", "This TV app does not provide a writable folder picker.");
      return;
    }
    write.disabled = true;
    var stream;
    try {
      var dir = await window.showDirectoryPicker({ mode: "readwrite" });
      var random = new Uint32Array(2);
      crypto.getRandomValues(random);
      var name = "arachnid-probe-" + random[0].toString(16) + "-" + random[1].toString(16) + ".txt";
      try {
        await dir.getFileHandle(name);
        throw new Error("scratch file already exists");
      } catch (e) {
        if (e.name !== "NotFoundError") throw e;
      }
      var file = await dir.getFileHandle(name, { create: true });
      stream = await file.createWritable();
      await stream.write(marker);
      await stream.close();
      stream = null;
      var reader = new FileReader();
      var data = await new Promise(function (resolve, reject) {
        reader.onload = function () { resolve(reader.result); };
        reader.onerror = function () { reject(reader.error); };
        file.getFile().then(function (item) { reader.readAsText(item); }, reject);
      });
      var matched = data === marker;
      reportStorage(writeResult, "selected directory write", matched ? "Write and readback succeeded. " + name : "The test file did not match on readback.", matched ? "ok" : "attention");
    } catch (e) {
      if (stream) await stream.abort().catch(function () {});
      reportStorage(writeResult, "selected directory write", e.name === "AbortError" ? "No folder selected." : "Could not complete the write (" + e.name + ").");
    } finally {
      write.disabled = false;
      write.focus();
    }
  });

  var details = document.getElementById("details");
  document.addEventListener("keydown", function (e) {
    var current = document.activeElement;
    var actions = [read, write, details];
    var index = actions.indexOf(current);
    if (index < 0) return;
    if (current === input && e.keyCode === 13) {
      e.preventDefault();
      if (!e.repeat) {
        try {
          if (typeof input.showPicker === "function") {
            input.showPicker();
            pickerRequested();
          } else {
            input.click();
          }
        } catch (error) {
          reportStorage(readResult, "file picker", "Could not open picker (" + error.name + ").");
        }
      }
      return;
    }
    var next;
    if (e.keyCode === 37 && current === write) next = read;
    if (e.keyCode === 39 && current === read && !write.disabled) next = write;
    if (e.keyCode === 40 && current !== details) next = details;
    if (e.keyCode === 38 && current === details) next = read;
    if (e.keyCode === 40 && current === details && details.parentNode.open) next = out;
    if (next) { e.preventDefault(); next.focus(); }
  });
  out.addEventListener("keydown", function (e) {
    if (e.keyCode === 38 && out.scrollTop === 0) {
      e.preventDefault();
      details.focus();
    }
  });
  read.focus();
  var c = window.crypto;
  cap("crypto.getRandomValues", c && c.getRandomValues ? (function () { try { c.getRandomValues(new Uint8Array(16)); return "works"; } catch (e) { return err(e); } })() : "absent");
  var s = c && c.subtle;
  cap("crypto.subtle", s ? "present" : "absent");
  var steps = [];
  function step(name, fn) { steps.push([name, fn]); }
  var shared = {};
  if (s) {
    step("SHA-256", function () { return s.digest("SHA-256", new Uint8Array([97, 98, 99])).then(function (d) { return hex(d).slice(0, 16) + "…"; }); });
    step("ECDH P-256 generate+export", function () {
      return s.generateKey({ name: "ECDH", namedCurve: "P-256" }, false, ["deriveBits"]).then(function (a) {
        shared.a = a; return s.generateKey({ name: "ECDH", namedCurve: "P-256" }, false, ["deriveBits"]);
      }).then(function (b) { shared.b = b; return s.exportKey("raw", shared.a.publicKey); }).then(function (raw) { return raw.byteLength + " bytes raw"; });
    });
    step("ECDH deriveBits", function () { return s.deriveBits({ name: "ECDH", public: shared.b.publicKey }, shared.a.privateKey, 256).then(function (bits) { shared.z = bits; return bits.byteLength + " bytes"; }); });
    step("HKDF-SHA256", function () {
      return s.importKey("raw", shared.z, "HKDF", false, ["deriveBits"]).then(function (k) {
        return s.deriveBits({ name: "HKDF", hash: "SHA-256", salt: new Uint8Array(16), info: new Uint8Array([1]) }, k, 256);
      }).then(function (okm) { shared.okm = okm; return okm.byteLength + " bytes"; });
    });
    step("HMAC-SHA256", function () {
      return s.importKey("raw", shared.okm, { name: "HMAC", hash: "SHA-256" }, false, ["sign", "verify"]).then(function (k) {
        return s.sign("HMAC", k, new Uint8Array([1, 2, 3]));
      }).then(function (m) { return m.byteLength + " bytes"; });
    });
    step("AES-GCM", function () {
      return s.importKey("raw", shared.okm, "AES-GCM", false, ["encrypt", "decrypt"]).then(function (k) {
        return s.encrypt({ name: "AES-GCM", iv: new Uint8Array(12) }, k, new Uint8Array(32));
      }).then(function (ct) { return ct.byteLength + " bytes"; });
    });
  }
  if (REPORT && typeof fetch === "function") {
    step("fetch JSON POST (preflight)", function () {
      return fetch(REPORT, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ probe: "cors" }) }).then(function (r) { return "HTTP " + r.status; });
    });
    step("fetch GET body", function () {
      return fetch(REPORT + "probe/").then(function (r) { return r.text().then(function (t) { return "HTTP " + r.status + ", " + t.length + " chars"; }); });
    });
  }
  step("Blob URL image decode", function () {
    // 1x1 PNG.
    var b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
    var bin = atob(b64), u = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) u[i] = bin.charCodeAt(i);
    var url = URL.createObjectURL(new Blob([u], { type: "image/png" }));
    return new Promise(function (ok, no) { var img = new Image(); img.onload = function () { ok(img.naturalWidth + "x" + img.naturalHeight); }; img.onerror = function () { no("decode failed"); }; img.src = url; });
  });
  function hex(buf) { var a = new Uint8Array(buf), h = ""; for (var i = 0; i < a.length; i++) h += (a[i] < 16 ? "0" : "") + a[i].toString(16); return h; }
  (function run(i) {
    if (i >= steps.length) { cap("done", "all steps ran"); return; }
    var name = steps[i][0], p;
    try { p = Promise.resolve(steps[i][1]()); } catch (e) { p = Promise.reject(e); }
    p.then(function (v) { cap(name, "ok " + v); }, function (e) { cap(name, err(e)); }).then(function () { run(i + 1); });
  })(0);
})();
</script></body></html>
"#;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut out, mut report) = (None, None);
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        match (flag.as_str(), it.next()) {
            ("--out", Some(v)) => out = Some(PathBuf::from(v)),
            ("--report", Some(v)) => report = Some(v.clone()),
            _ => usage(),
        }
    }
    let Some(out) = out else { usage() };
    let app = AppInfo {
        id: "com.goya.arachnid.runtimeprobe".into(),
        title: "Arachnid Runtime Probe".into(),
        version: "0.2.2".into(),
        vendor: Some("Goya".into()),
    };
    let report_js = report.as_deref().map_or("null".to_string(), json_string);
    let mut files = vec![
        ("appinfo.json".to_string(), app.to_json().into_bytes()),
        ("icon.png".to_string(), emit::ARACHNID_ICON.to_vec()),
        ("index.html".to_string(), PAGE.replace("__REPORT__", &report_js).into_bytes()),
    ];
    files.sort();
    let pkg = ipk::package(&app, &files).expect("package");
    std::fs::create_dir(&out).expect("output directory must not exist");
    std::fs::write(out.join(&pkg.file_name), &pkg.bytes).expect("write ipk");
    println!("{}", out.join(&pkg.file_name).display());
}

fn usage() -> ! {
    eprintln!("usage: runtime_probe --out NEW_DIR [--report http://HOST:PORT/]");
    std::process::exit(2);
}
