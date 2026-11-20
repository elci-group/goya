/* Headless-browser harness for the generated remote runtime. Used by
   scripts/check-arachnid.sh against fixtures/media-centre.gscene. Prints
   PASS/FAIL lines into <pre id="results">. */
(function () {
  var log = [];
  var acts = [];
  document.addEventListener("goya:activate", function (e) { acts.push(e.detail.id + "/" + e.detail.via); });
  function key(type, code) { var e = document.createEvent("Event"); e.initEvent(type, true, true); Object.defineProperty(e, "keyCode", { value: code }); document.dispatchEvent(e); }
  function press(code) { key("keydown", code); key("keyup", code); }
  function focused() { var f = document.querySelector(".b.f"); return f ? f.getAttribute("data-goya-id") : "none"; }
  function mouse(type, el) { var e = document.createEvent("MouseEvents"); e.initMouseEvent(type, true, true, window, 1, 0, 0, 0, 0, false, false, false, false, 0, null); (el || document).dispatchEvent(e); }
  function el(id) { return document.querySelector('[data-goya-id="' + id + '"]'); }
  function check(name, got, want) { log.push((got === want ? "PASS " : "FAIL ") + name + ": got " + got + ", want " + want); }

  check("initial focus", focused(), "films");
  press(39); press(39); press(39);
  check("right x3", focused(), "photos");
  press(39);
  check("right at edge stays", focused(), "photos");
  press(40); check("down", focused(), "settings");
  press(37); check("left", focused(), "search");
  press(38); check("up", focused(), "music");
  // 5-way OK: one activation even if the platform also sends a click during the press.
  key("keydown", 13); mouse("click", el("music")); key("keydown", 13); key("keyup", 13);
  check("5-way OK activations", acts.join(","), "music/key");
  // Pointer mode: hover moves focus, click activates, OK keydown is left to the click.
  mouse("mousemove"); mouse("mouseover", el("library"));
  check("pointer hover focuses", focused(), "library");
  key("keydown", 13); mouse("click", el("library")); key("keyup", 13);
  check("pointer OK activations", acts.join(","), "music/key,library/pointer");
  // cursorStateChange hidden + arrow returns to 5-way from the hovered element.
  var c = document.createEvent("CustomEvent"); c.initCustomEvent("cursorStateChange", false, false, { visibility: false }); document.dispatchEvent(c);
  press(39); check("arrow after pointer", focused(), "games");
  press(13); check("5-way OK after pointer", acts[acts.length - 1], "games/key");
  // Back is not consumed by the app.
  var back = document.createEvent("Event"); back.initEvent("keydown", true, true); Object.defineProperty(back, "keyCode", { value: 461 });
  check("back not prevented", String(document.dispatchEvent(back)), "true");
  var pre = document.createElement("pre"); pre.id = "results"; pre.textContent = log.join("\n"); document.body.appendChild(pre);
})();
