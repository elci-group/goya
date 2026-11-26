/* Headless-browser harness for apps/launcher.gscene built by Arachnid. Used
   by scripts/check-arachnid.sh. It drives the stock remote only (arrows, OK
   and the four colour keys) and prints PASS/FAIL lines into
   <pre id="results">.

   A stand-in PalmServiceBridge records the Luna calls the generated @apps
   runtime makes and answers them; it is test tooling, not app code. The
   harness checks profiles, dealt colour keys (on each screen the first four
   included tiles get red, green, yellow and blue; omitting an app deals
   again; a colour with no app is left to the platform), Settings, the
   channel bar, launch requests and their D9 announcements, a refused
   launch, a TV with no bridge, and the durable profile and omission (D8).
   Launched again with "#persist", it checks both are restored. */
(function () {
  var log = [], steps = [], calls = [];
  var STORE = "goya.state.v1/goya_launcher";
  function key(type, code) { var e = document.createEvent("Event"); e.initEvent(type, true, true); Object.defineProperty(e, "keyCode", { value: code }); return document.dispatchEvent(e); }
  /* Returns true when the runtime consumed the keydown. */
  function press(code) { var free = key("keydown", code); key("keyup", code); return !free; }
  function check(name, got, want) {
    var g = typeof got === "string" ? got : JSON.stringify(got), w = typeof want === "string" ? want : JSON.stringify(want);
    log.push((g === w ? "PASS " : "FAIL ") + name + ": got " + g + ", want " + w);
  }
  function S() { return window.Arachnid.state(); }
  function el(id) { return document.querySelector('[data-goya-id="' + id + '"]'); }
  function focusedId() { var f = document.querySelector(".b.f"); return f ? f.getAttribute("data-goya-id") : "none"; }
  function shown(id) { var e = el(id); return !!e && e.offsetParent !== null && getComputedStyle(e).visibility !== "hidden"; }
  /* Greedy 5-way walk towards a button: vertical first, then horizontal. */
  function seek(id) {
    for (var guard = 0; guard < 24 && focusedId() !== id; guard++) {
      var f = document.querySelector(".b.f");
      if (!f) { press(39); continue; }
      var a = f.getBoundingClientRect(), b = el(id).getBoundingClientRect();
      var ay = a.top + a.height / 2, by = b.top + b.height / 2, ax = a.left + a.width / 2, bx = b.left + b.width / 2;
      if (Math.abs(by - ay) > a.height / 2) press(by > ay ? 40 : 38);
      else press(bx > ax ? 39 : 37);
    }
    check("focus reaches " + id, focusedId(), id);
  }
  /* Logo canvas pixel at a point given in the graphic's 240-point canvas. */
  function logoPixel(x, y) {
    var c = document.querySelector('[data-goya-id="logo"] canvas');
    var d = c.getContext("2d").getImageData(Math.floor(c.width * x / 240), Math.floor(c.height * y / 240), 1, 1).data;
    return [d[0], d[1], d[2]];
  }
  function near(a, b) { for (var i = 0; i < 3; i++) if (Math.abs(a[i] - b[i]) > 3) return false; return true; }
  var RED = [214, 69, 80], GREEN = [46, 158, 79];
  function then(ms, f) { steps.push([ms, f]); }
  function run(i) {
    if (i >= steps.length) return finish();
    setTimeout(function () {
      try { steps[i][1](); } catch (e) { log.push("FAIL step " + i + ": " + e.message); }
      run(i + 1);
    }, steps[i][0]);
  }
  function finish() {
    check("runtime errors", window.Arachnid.errors().join(" | "), "");
    var pre = document.createElement("pre");
    pre.id = "results";
    pre.textContent = log.join("\n");
    document.body.appendChild(pre);
  }

  /* Stand-in for webOS's bridge: netflix is "not installed". */
  function FakeBridge() {}
  FakeBridge.prototype.call = function (uri, params) {
    var self = this, p = JSON.parse(params);
    calls.push(uri + " " + params);
    setTimeout(function () {
      self.onservicecallback(JSON.stringify(p.id === "netflix"
        ? { returnValue: false, errorCode: -101, errorText: "app not installed" }
        : { returnValue: true }));
    }, 10);
  };
  window.PalmServiceBridge = FakeBridge;

  if (location.hash === "#persist") {
    then(0, function () {
      check("durable profile restored at relaunch", S().profile, 2);
      check("relaunch opens the profile picker", S().screen, 0);
      check("durable omission restored at relaunch", S().cc_netflix, false);
    });
    document.addEventListener("arachnid:splash-end", function () { run(0); });
    return;
  }

  /* The live sky: top-centre pixel of the sky canvas, read straight after
     a pinned draw (WebGL or the 2D path). */
  function skyPixel() {
    var c = document.querySelector('[data-goya-id="root"] canvas.sky'), d;
    var gl = c.getContext("webgl") || c.getContext("experimental-webgl");
    if (gl) { d = new Uint8Array(4); gl.readPixels(Math.floor(c.width / 2), c.height - 4, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, d); }
    else d = c.getContext("2d").getImageData(Math.floor(c.width / 2), 3, 1, 1).data;
    return [d[0], d[1], d[2]];
  }
  then(0, function () {
    var st = window.GoyaSky && window.GoyaSky.state();
    check("sky runtime present and drawing", !!st && (st.mode === "webgl" || st.mode === "2d") && st.frames > 0, true);
    log.push("INFO sky mode " + (st && st.mode) + (st && st.error ? " (" + st.error + ")" : ""));
    window.GoyaSky.pin(Date.parse("2026-11-26T12:00:00Z"));
    var noon = skyPixel();
    check("noon sky is blue (" + noon.join(",") + ")", noon[2] > noon[0] + 30 && noon[2] > 120, true);
    window.GoyaSky.pin(Date.parse("2026-11-26T00:30:00Z"));
    var night = skyPixel();
    check("midnight sky is dark (" + night.join(",") + ")", night[0] + night[1] + night[2] < noon[0] + noon[1] + noon[2] - 150, true);
    window.GoyaSky.pin(null);
  });
  then(0, function () {
    try { window.localStorage.removeItem(STORE); } catch (e) { /* fresh profile */ }
    check("starts on the profile picker", S().screen, 0);
    check("logo red square painted", near(logoPixel(74, 74), RED), true);
    check("logo green square painted at full strength", near(logoPixel(166, 74), GREEN), true);
    check("initial focus", focusedId(), "pick.1");
    check("colour bar hidden on the picker", shown("channel.apps"), false);
    check("red with no claimant is left to the platform", press(403), false);
    check("screen unchanged by unclaimed red", S().screen, 0);
  });
  then(0, function () {
    seek("pick.2");
    press(13);
  });
  then(300, function () {
    check("profile chosen", S().profile, 2);
    check("home shown", S().screen, 1);
    check("D8 profile written", window.localStorage.getItem(STORE), '{"format":"goya.state.v1","values":{"profile":["int","2"],"cc_youtube":["bool",true],"cc_netflix":["bool",true],"cc_prime":["bool",true],"cc_browser":["bool",true],"cc_livetv":["bool",true],"cc_pairs":["bool",true]}}');
    check("Family favourites shown", shown("fav.2.prime"), true);
    check("Owner favourites hidden", shown("fav.1.pairs"), false);
    check("profile name shown", shown("who.2") && !shown("who.1"), true);
    check("red consumed on Home", press(403), true);
  });
  then(100, function () {
    check("red picks the first tile", focusedId(), "fav.2.youtube");
    check("red does not leave Home", S().screen, 1);
    press(403);
  });
  then(100, function () {
    check("each colour has one app: red again stays", focusedId(), "fav.2.youtube");
    press(404);
  });
  then(100, function () {
    check("green picks the second tile", focusedId(), "fav.2.netflix");
    press(405);
  });
  then(100, function () {
    check("yellow picks the third tile", focusedId(), "fav.2.prime");
    press(406);
  });
  then(100, function () {
    check("blue picks the fourth tile", focusedId(), "fav.2.livetv");
    seek("channel.apps");
    press(13);
  });
  then(300, function () {
    check("channel bar opens Applications", S().screen, 2);
    check("logo keeps red at full strength on Applications", near(logoPixel(74, 74), RED), true);
    check("logo dims green on Applications", near(logoPixel(166, 74), GREEN), false);
    press(406);
  });
  then(100, function () {
    check("colours are dealt again per screen: blue is Web Browser here", focusedId(), "apps.browser");
    press(404);
  });
  then(100, function () {
    check("green is Netflix on Applications", focusedId(), "apps.netflix");
    var reached = {};
    [403, 404, 405, 406].forEach(function (k) { press(k); reached[focusedId()] = true; });
    check("a fifth and sixth app get no key", !!reached["apps.livetv"] || !!reached["apps.pairs"], false);
    seek("apps.youtube");
    press(13);
  });
  then(300, function () {
    check("launch call", calls[0], 'luna://com.webos.applicationManager/launch {"id":"youtube.leanback.v4"}');
    check("launch result", S().notice, "Opening youtube");
    check("D9 announcement for a launch", window.Arachnid.announcement(), 'notice changed from "" to "Opening youtube"');
    seek("channel.household");
    press(13);
  });
  then(300, function () {
    check("channel bar opens Household", S().screen, 3);
    check("colour keys are not consumed on Household", press(404), false);
    seek("channel.settings");
    press(13);
  });
  then(300, function () {
    check("channel bar opens Settings", S().screen, 6);
    check("colour keys are not consumed on Settings", press(403), false);
    seek("settings.netflix.omit");
    press(13);
  });
  then(300, function () {
    check("Netflix omitted", S().cc_netflix, false);
    seek("channel.home");
    press(13);
  });
  then(300, function () {
    check("Home returns to favourites", S().screen, 1);
    check("omitted Netflix keeps its tile", shown("fav.2.netflix"), true);
    press(404);
  });
  then(100, function () {
    check("colours re-deal without Netflix: green is Prime Video", focusedId(), "fav.2.prime");
    press(405);
  });
  then(100, function () {
    check("yellow is Live TV", focusedId(), "fav.2.livetv");
    check("blue now has no app here and is left to the platform", press(406), false);
    seek("fav.2.netflix");
    press(13);
  });
  then(300, function () {
    check("refused launch call", calls[1], 'luna://com.webos.applicationManager/launch {"id":"netflix"}');
    check("refused launch is reported, not thrown", S().notice, "Could not open netflix: app not installed");
    delete window.PalmServiceBridge;
    seek("fav.2.youtube");
    press(13);
  });
  then(300, function () {
    check("no bridge is reported", S().notice, "Could not open youtube: this TV offers no service bridge to the app");
    check("no call without a bridge", calls.length, 2);
    seek("channel.profiles");
    press(13);
  });
  then(300, function () {
    check("Profiles returns to the picker", S().screen, 0);
    check("profile kept while choosing", S().profile, 2);
  });
  document.addEventListener("arachnid:splash-end", function () { run(0); });
})();
