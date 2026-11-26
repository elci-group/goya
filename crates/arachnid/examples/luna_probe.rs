//! Device qualification probe for the launcher (docs/arachnid/launcher.md
//! §4), not compiler output.
//!
//! Packages a hand-written page with Arachnid's own IPK writer. The page:
//! * says which service bridge the page sees (`PalmServiceBridge`,
//!   `WebOSServiceBridge`);
//! * on load, makes **read-only** Luna calls and logs each answer:
//!   `applicationManager/listLaunchPoints`, `listApps`,
//!   `getForegroundAppInfo`, `tv.systemproperty/getSystemInfo`, and two
//!   telemetry candidates;
//! * logs every key (code, `key`, repeat), so the four colour keys can be
//!   seen, and consumes 403–406 so that the platform's own use of them, if
//!   any, shows as a missing event;
//! * launches `com.goya.arachnid.pairs` through
//!   `applicationManager/launch` **only when OK is pressed** on its tile.
//!
//! With `--report URL`, every log line, and each full Luna answer, is also
//! POSTed there as plain text.
//!
//! ```text
//! cargo run -p goya-arachnid --example luna_probe -- --out NEW_DIR [--report http://HOST:PORT/]
//! ```

use arachnid::appinfo::{json_string, AppInfo};
use arachnid::{emit, ipk};
use std::path::PathBuf;

const PAGE: &str = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Arachnid Luna probe</title>
<style>
html,body{margin:0;width:1920px;height:1080px;overflow:hidden;background:#111;color:#eee;font:24px sans-serif}
#info{position:absolute;left:60px;top:40px;width:1800px;height:150px;white-space:pre-wrap;font-size:22px}
.t{position:absolute;top:200px;width:700px;height:160px;background:#345;border:6px solid #fd0;box-sizing:border-box;text-align:center;line-height:148px;font-size:40px;left:610px}
#keys{position:absolute;left:60px;top:390px;width:1800px;height:60px;font:28px monospace}
#log{position:absolute;left:60px;top:470px;width:1800px;height:580px;white-space:pre;font:19px monospace;overflow:hidden}
</style></head>
<body>
<div id="info"></div>
<div class="t" id="launch">OK: launch Goya Pairs</div>
<div id="keys">Press RED, GREEN, YELLOW, BLUE.</div>
<div id="log"></div>
<script>
(function () {
  var REPORT = __REPORT__;
  var lines = [], pending = [], t0 = Date.now(), keep = [];
  function now() { return ((Date.now() - t0) / 1000).toFixed(3); }
  function post(body) {
    if (!REPORT) return;
    try { var x = new XMLHttpRequest(); x.open("POST", REPORT, true); x.setRequestHeader("Content-Type", "text/plain"); x.send(body); } catch (e) {}
  }
  function log(s) {
    var line = now() + " " + s;
    lines.push(line); if (lines.length > 26) lines.shift();
    document.getElementById("log").textContent = lines.join("\n");
    pending.push(line);
  }
  setInterval(function () { if (pending.length) { post(pending.join("\n")); pending = []; } }, 1000);

  var Bridge = window.PalmServiceBridge || window.WebOSServiceBridge;
  var info = [
    "PalmServiceBridge: " + typeof window.PalmServiceBridge + ", WebOSServiceBridge: " + typeof window.WebOSServiceBridge,
    "UA: " + navigator.userAgent,
    "Press each colour key, then OK to try launching Pairs. Back exits."
  ];
  document.getElementById("info").textContent = info.join("\n");
  log("INFO " + info[0]);

  function call(name, uri, params) {
    if (typeof Bridge !== "function") { log(name + ": no bridge"); return; }
    var b = new Bridge(), done = false;
    keep.push(b);
    var timer = setTimeout(function () { if (!done) { done = true; log(name + ": no answer in 8 s"); } }, 8000);
    b.onservicecallback = function (msg) {
      if (done) return;
      done = true; clearTimeout(timer);
      var r = null;
      try { r = JSON.parse(msg); } catch (e) {}
      var summary = r ? "returnValue=" + r.returnValue + (r.errorCode !== undefined ? " errorCode=" + r.errorCode : "") + (r.errorText ? " errorText=" + r.errorText : "") : "unparsed";
      if (r && r.launchPoints) summary += " launchPoints=" + r.launchPoints.length + " [" + r.launchPoints.map(function (p) { return p.id + "|" + p.title; }).join(", ").slice(0, 600) + "]";
      if (r && r.apps) summary += " apps=" + r.apps.length;
      log(name + ": " + summary);
      post("FULL " + name + " " + msg);
    };
    try { b.call(uri, JSON.stringify(params)); log(name + ": called " + uri); }
    catch (e) { done = true; clearTimeout(timer); log(name + ": threw " + e); }
  }

  call("listLaunchPoints", "luna://com.webos.applicationManager/listLaunchPoints", {});
  call("listApps", "luna://com.webos.applicationManager/listApps", {});
  call("getForegroundAppInfo", "luna://com.webos.applicationManager/getForegroundAppInfo", {});
  call("getSystemInfo", "luna://com.webos.service.tv.systemproperty/getSystemInfo", { keys: ["modelName", "sdkVersion", "firmwareVersion", "UHD"] });
  call("memorymanager", "luna://com.webos.memorymanager/getCurrentMemState", {});
  call("resourcemonitor", "luna://com.webos.service.resourcemonitor/getCpuUsage", {});

  var names = { 403: "RED", 404: "GREEN", 405: "YELLOW", 406: "BLUE" }, seen = {};
  document.addEventListener("keydown", function (e) {
    log("keydown " + e.keyCode + " key=" + e.key + (e.repeat ? " repeat" : ""));
    if (names[e.keyCode]) {
      e.preventDefault();
      seen[e.keyCode] = true;
      document.getElementById("keys").textContent = "Seen: " + Object.keys(seen).map(function (k) { return names[k]; }).join(" ");
    }
    if (e.keyCode === 13 && !e.repeat) call("launch", "luna://com.webos.applicationManager/launch", { id: "com.goya.arachnid.pairs" });
  }, true);
  document.addEventListener("keyup", function (e) { if (names[e.keyCode]) log("keyup " + e.keyCode); }, true);
  window.addEventListener("blur", function () { log("window blur"); });
  window.addEventListener("focus", function () { log("window focus"); });
  document.addEventListener("visibilitychange", function () { log("visibility " + document.visibilityState); });
  log("probe ready");
})();
</script>
</body></html>
"#;

const USAGE: &str = "usage: luna_probe --out NEW_DIR [--report http://HOST:PORT/]";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = None;
    let mut report = None;
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        match (flag.as_str(), it.next()) {
            ("--out", Some(v)) => out = Some(PathBuf::from(v)),
            ("--report", Some(v)) => report = Some(v.clone()),
            _ => {
                eprintln!("{USAGE}");
                std::process::exit(2);
            }
        }
    }
    let Some(out) = out else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    let app = AppInfo {
        id: "com.goya.arachnid.lunaprobe".into(),
        title: "Arachnid Luna Probe".into(),
        version: "0.1.0".into(),
        vendor: Some("Goya".into()),
    };
    let report_js = report.as_deref().map_or("null".to_string(), json_string);
    let page = PAGE.replace("__REPORT__", &report_js);
    let mut files = vec![
        ("appinfo.json".to_string(), app.to_json().into_bytes()),
        ("icon.png".to_string(), emit::ARACHNID_ICON.to_vec()),
        ("index.html".to_string(), page.into_bytes()),
    ];
    files.sort();
    let pkg = ipk::package(&app, &files).expect("package");
    std::fs::create_dir(&out).expect("output directory must not exist");
    std::fs::write(out.join(&pkg.file_name), &pkg.bytes).expect("write ipk");
    println!("{}", out.join(&pkg.file_name).display());
}
