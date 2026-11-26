//! Device qualification probe, not compiler output.
//!
//! Packages a hand-written page with Arachnid's own IPK writer. The page:
//! * records what the TV reports about itself: `PalmSystem.deviceInfo`,
//!   user agent, viewport;
//! * logs every remote event in order: key down/up, mouse down/up/click,
//!   `cursorStateChange`, focus/blur, visibility;
//! * groups each OK press into one sequence such as
//!   `keydown13 > mousedown > mouseup > click > keyup13`.
//!
//! If `--report URL` is given, each batch of log lines is also POSTed there
//! as plain text, so the result can be read without looking at the TV.
//!
//! ```text
//! cargo run -p goya-arachnid --example input_probe -- --out NEW_DIR [--report http://HOST:PORT/]
//! ```

use arachnid::appinfo::{json_string, AppInfo};
use arachnid::{emit, ipk};
use std::path::PathBuf;

const PAGE: &str = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Arachnid probe</title>
<style>
html,body{margin:0;width:1920px;height:1080px;overflow:hidden;background:#111;color:#eee;font:24px sans-serif}
#info{position:absolute;left:60px;top:40px;width:1800px;height:200px;white-space:pre-wrap;font-size:22px}
.t{position:absolute;top:280px;width:500px;height:260px;background:#345;border:6px solid #678;box-sizing:border-box;text-align:center;line-height:248px;font-size:48px}
.t.f{border-color:#fd0}
#a{left:160px}#b{left:710px}#c{left:1260px}
#log{position:absolute;left:60px;top:580px;width:1800px;height:460px;white-space:pre;font:20px monospace;overflow:hidden}
</style></head>
<body>
<div id="info"></div>
<div class="t" id="a">A</div><div class="t" id="b">B</div><div class="t" id="c">C</div>
<div id="log"></div>
<script>
(function () {
  var REPORT = __REPORT__;
  var lines = [], pending = [], seq = null, presses = 0;
  var t0 = Date.now();
  function now() { return ((Date.now() - t0) / 1000).toFixed(3); }
  function send() {
    if (!REPORT || !pending.length) return;
    var body = pending.join("\n"); pending = [];
    try { var x = new XMLHttpRequest(); x.open("POST", REPORT, true); x.setRequestHeader("Content-Type", "text/plain"); x.send(body); } catch (e) {}
  }
  function log(s) {
    var line = now() + " " + s;
    lines.push(line); if (lines.length > 18) lines.shift();
    document.getElementById("log").textContent = lines.join("\n");
    pending.push(line);
  }
  setInterval(send, 1000);
  function step(s) { if (seq) seq.push(s); }
  var info = [];
  try { info.push("deviceInfo: " + (window.PalmSystem ? window.PalmSystem.deviceInfo : "no PalmSystem")); } catch (e) { info.push("deviceInfo error: " + e); }
  info.push("UA: " + navigator.userAgent);
  info.push("viewport: " + window.innerWidth + "x" + window.innerHeight + " dpr " + window.devicePixelRatio + " screen " + screen.width + "x" + screen.height);
  info.push("Press OK with the pointer visible and with arrows. Then press 0 through 9, hold one digit, and try the on-screen number pad if available. Back exits.");
  document.getElementById("info").textContent = info.join("\n");
  for (var i = 0; i < info.length - 1; i++) log("INFO " + info[i]);

  var tiles = ["a", "b", "c"], focus = 0;
  function show() { for (var i = 0; i < 3; i++) document.getElementById(tiles[i]).className = "t" + (i === focus ? " f" : ""); }
  show();
  document.addEventListener("keydown", function (e) {
    if (e.keyCode === 13 && !e.repeat) { seq = ["keydown13"]; presses++; }
    else if (e.keyCode !== 13) log("keydown " + e.keyCode + " key=" + e.key + (e.repeat ? " repeat" : ""));
    if (e.keyCode === 37 && focus > 0) { focus--; show(); }
    if (e.keyCode === 39 && focus < 2) { focus++; show(); }
  }, true);
  document.addEventListener("keyup", function (e) {
    if (e.keyCode === 13) {
      step("keyup13");
      var s = seq; seq = null;
      setTimeout(function () { log("OK#" + presses + " " + (s ? s.join(" > ") : "keyup13 without keydown")); }, 400);
    } else log("keyup " + e.keyCode + " key=" + e.key);
  }, true);
  ["mousedown", "mouseup", "click"].forEach(function (type) {
    document.addEventListener(type, function (e) {
      var id = e.target && e.target.id ? e.target.id : e.target.nodeName;
      if (seq) step(type + "(" + id + ")"); else log(type + " on " + id + " button=" + e.button + " (no OK key in flight)");
    }, true);
  });
  var moves = 0;
  document.addEventListener("mousemove", function () { moves++; }, true);
  setInterval(function () { if (moves) { log("mousemove x" + moves); moves = 0; } }, 2000);
  tiles.forEach(function (id, i) { document.getElementById(id).addEventListener("mouseover", function () { focus = i; show(); }); });
  document.addEventListener("cursorStateChange", function (e) { log("cursorStateChange visibility=" + (e.detail && e.detail.visibility)); }, true);
  window.addEventListener("blur", function () { log("window blur"); });
  window.addEventListener("focus", function () { log("window focus"); });
  document.addEventListener("visibilitychange", function () { log("visibility " + document.visibilityState); });
  document.addEventListener("webOSRelaunch", function () { log("webOSRelaunch"); });
  log("probe ready");
})();
</script>
</body></html>
"#;

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
                eprintln!("usage: input_probe --out NEW_DIR [--report http://HOST:PORT/]");
                std::process::exit(2);
            }
        }
    }
    let Some(out) = out else {
        eprintln!("usage: input_probe --out NEW_DIR [--report http://HOST:PORT/]");
        std::process::exit(2);
    };
    let app = AppInfo {
        id: "com.goya.arachnid.probe".into(),
        title: "Arachnid Probe".into(),
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
