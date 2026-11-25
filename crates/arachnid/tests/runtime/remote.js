/* Headless-browser harness for the generated remote runtime. Used by
   scripts/check-arachnid.sh against fixtures/media-centre.gscene. Prints
   PASS/FAIL lines into <pre id="results">.

   Stock remote (no pointer) is the baseline: it sends only keydown/keyup,
   as observed on XR55N21WEB24 / webOS TV Lite 7.5.8. The Magic Remote
   pointer is exercised only after the platform reports a visible cursor. */
(function () {
  var log = [];
  var acts = [];
  document.addEventListener("goya:activate", function (e) { acts.push(e.detail.id + "/" + e.detail.via); });
  function key(type, code) { var e = document.createEvent("Event"); e.initEvent(type, true, true); Object.defineProperty(e, "keyCode", { value: code }); return document.dispatchEvent(e); }
  function press(code) { key("keydown", code); key("keyup", code); }
  function focused() { var f = document.querySelector(".b.f"); return f ? f.getAttribute("data-goya-id") : "none"; }
  function mouse(type, el) { var e = document.createEvent("MouseEvents"); e.initMouseEvent(type, true, true, window, 1, 0, 0, 0, 0, false, false, false, false, 0, null); (el || document).dispatchEvent(e); }
  function el(id) { return document.querySelector('[data-goya-id="' + id + '"]'); }
  function cursor(visible) { var c = document.createEvent("CustomEvent"); c.initCustomEvent("cursorStateChange", false, false, { visibility: visible }); document.dispatchEvent(c); }
  function plain(target, type) { var e = document.createEvent("Event"); e.initEvent(type, false, false); target.dispatchEvent(e); }
  function last(n) { return acts.slice(-n).join(","); }
  function check(name, got, want) { log.push((got === want ? "PASS " : "FAIL ") + name + ": got " + got + ", want " + want); }

  // Enforced splash: present at launch, holding the 5-way keys.
  check("splash shown at launch", String(!!document.getElementById("arachnid-splash")), "true");
  check("splash logo packaged", (document.querySelector("#arachnid-splash img") || {}).getAttribute ? document.querySelector("#arachnid-splash img").getAttribute("src") : "none", "arachnid-splash.png");
  press(39);
  check("keys held during splash", focused(), "films");
  check("back passes during splash", String(key("keydown", 461)), "true");
  document.addEventListener("arachnid:splash-end", run);
  function run() {
  check("splash removed after it ends", String(!!document.getElementById("arachnid-splash")), "false");
  // 5-way traversal.
  check("initial focus", focused(), "films");
  press(39); press(39); press(39);
  check("right x3", focused(), "photos");
  press(39);
  check("right at edge stays", focused(), "photos");
  press(40); check("down", focused(), "settings");
  press(37); check("left", focused(), "search");
  press(38); check("up", focused(), "music");
  // Held arrow: auto-repeat keydowns without keyups each move one step.
  key("keydown", 37); key("keydown", 37); key("keyup", 37);
  check("held left repeats", focused(), "films");
  press(39); press(39);
  // 5-way OK: one activation even if the platform also sends a click during the press.
  key("keydown", 13); mouse("click", el("music")); key("keydown", 13); key("keyup", 13);
  check("5-way OK activations", acts.join(","), "music/key");

  // Stock remote: a stray mouse event must not make OK wait for a click.
  mouse("mousemove"); press(13);
  check("OK after stray mousemove", last(1), "music/key");
  // Hover with no platform cursor still moves focus, and OK activates it.
  mouse("mouseover", el("library")); press(13);
  check("OK after hover without cursor", last(1), "library/key");
  // A press cut short by blur, hiding or relaunch does not swallow the next OK.
  key("keydown", 13); plain(window, "blur"); press(13);
  check("OK after blur mid-press", last(2), "library/key,library/key");
  key("keydown", 13); plain(document, "visibilitychange"); press(13);
  check("OK after hide mid-press", last(2), "library/key,library/key");
  key("keydown", 13); plain(document, "webOSRelaunch"); press(13);
  check("OK after relaunch mid-press", last(2), "library/key,library/key");
  var before = acts.length;

  // Magic Remote: once the platform shows the cursor, OK is left to the click.
  cursor(true);
  mouse("mouseover", el("music"));
  check("pointer hover focuses", focused(), "music");
  key("keydown", 13); mouse("click", el("music")); key("keyup", 13);
  check("pointer OK activations", acts.slice(before).join(","), "music/pointer");
  // Cursor hidden + arrow returns to 5-way from the hovered element.
  cursor(false);
  press(39); check("arrow after pointer", focused(), "photos");
  press(13); check("5-way OK after pointer", last(1), "photos/key");

  // Keys outside the 5-way contract keep their platform behaviour.
  check("back not prevented", String(key("keydown", 461)), "true");
  check("colour key not prevented", String(key("keydown", 403)), "true");
  check("digit not prevented", String(key("keydown", 49)), "true");
  check("other keys do not move focus", focused(), "photos");

  var pre = document.createElement("pre"); pre.id = "results"; pre.textContent = log.join("\n"); document.body.appendChild(pre);
  }
})();
