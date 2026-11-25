/* Headless-browser harness for apps/pairs.gscene built by Arachnid. Used by
   scripts/check-arachnid.sh. It plays with the stock remote only (arrow keys
   and OK) and prints PASS/FAIL lines into <pre id="results">.

   It checks the game state, the D9 announcements, the GVI painter (canvas
   pixels after the flip animations), focus slots (one present button per
   card), and the D8 durable record. Launched again with "#persist" on the
   same profile, it checks that the record is restored. */
(function () {
  var log = [];
  var steps = [];
  var BACK = [43, 76, 126], FRONT = [255, 250, 240];
  function key(type, code) { var e = document.createEvent("Event"); e.initEvent(type, true, true); Object.defineProperty(e, "keyCode", { value: code }); return document.dispatchEvent(e); }
  function press(code) { key("keydown", code); key("keyup", code); }
  function check(name, got, want) {
    var g = typeof got === "string" ? got : JSON.stringify(got), w = typeof want === "string" ? want : JSON.stringify(want);
    log.push((g === w ? "PASS " : "FAIL ") + name + ": got " + g + ", want " + w);
  }
  function S() { return window.Arachnid.state(); }
  function focusedEl() { return document.querySelector(".b.f"); }
  function focusedId() { var f = focusedEl(); return f ? f.getAttribute("data-goya-id") : "none"; }
  function cardOf(id) { var m = /^card\.(\d+)\./.exec(id); return m ? Number(m[1]) : -1; }
  function picOf(j, deal) {
    var b = document.querySelector('[data-goya-id="card.' + j + '.c' + j + "_d" + deal + '_done0"]');
    return /: ([a-z ]+), matched$/.exec(b.getAttribute("aria-label"))[1];
  }
  function goTo(j) {
    for (var guard = 0; guard < 16; guard++) {
      var c = cardOf(focusedId());
      if (c === j) return;
      if (c < 0) { press(37); continue; }
      var r = Math.floor(c / 4), q = c % 4, tr = Math.floor(j / 4), tq = j % 4;
      press(tr > r ? 40 : tr < r ? 38 : tq > q ? 39 : 37);
    }
  }
  function pick(j) { goTo(j); press(13); }
  function pixel(j) {
    var c = document.querySelector('[data-goya-id="card.' + j + '.art"] canvas');
    var d = c.getContext("2d").getImageData(Math.floor(c.width / 2), Math.floor(c.height * 0.12), 1, 1).data;
    return [d[0], d[1], d[2]];
  }
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

  if (location.hash === "#persist") {
    then(0, function () {
      check("durable games restored at relaunch", S().games, 1);
      check("session keys start fresh", [S().pairs, S().moves, S().deal], [0, 0, 0]);
      check("restored value is shown", document.querySelector('[data-goya-id="status.games"]').textContent, "Games won: 1");
    });
    document.addEventListener("arachnid:splash-end", function () { run(0); });
    return;
  }

  var pics = [], groups = {}, j;
  then(0, function () {
    try { window.localStorage.removeItem("goya.state.v1/goya_pairs"); } catch (e) { /* fresh profile */ }
    for (j = 0; j < 16; j++) { pics.push(picOf(j, 0)); (groups[pics[j]] = groups[pics[j]] || []).push(j); }
    check("eight pictures, two cards each", Object.keys(groups).filter(function (p) { return groups[p].length === 2; }).length, 8);
    check("initial focus on the first card", focusedId(), "card.0.c0_d0_first");
    check("card 0 painted face down", pixel(0), BACK);
    check("initial state", [S().phase, S().pairs, S().moves, S().games], [0, 0, 0, 0]);
  });
  var miss;
  then(0, function () {
    press(13);
    check("first pick turns card 0", [S().phase, S().sel_0, S().first_pic >= 0], [1, true, true]);
    check("focus moves to the held variant", focusedEl().getAttribute("aria-label"), "Card 1, row 1, column 1: " + pics[0] + ", turned over");
  });
  then(400, function () {
    check("card 0 painted face up after the flip", pixel(0), FRONT);
    for (j = 1; j < 16; j++) if (pics[j] !== pics[0]) { miss = j; break; }
    pick(miss);
    check("a miss shows both cards", [S().phase, S()["sec_" + miss], S().moves], [2, true, 1]);
    check("D9 announcement for the miss", window.Arachnid.announcement(), "Moves increased by 1, from 0 to 1");
    check("hint for a miss is shown", !document.querySelector('[data-goya-id="hint.2"]').hidden, true);
  });
  then(400, function () {
    check("missed card painted face up", pixel(miss), FRONT);
    press(13);
    check("OK turns them back", [S().phase, S().sel_0, S()["sec_" + miss]], [0, false, false]);
  });
  then(400, function () {
    check("both cards painted face down again", [pixel(0), pixel(miss)], [BACK, BACK]);
  });
  [0, 1, 2, 3, 4, 5, 6, 7].forEach(function (n) {
    then(0, function () {
      var p = Object.keys(groups)[n];
      pick(groups[p][0]);
      pick(groups[p][1]);
      check("pair " + (n + 1) + " (" + p + ") matched", [S().pairs, S().phase], [n + 1, 0]);
      if (n === 0) check("D9 announcement for a match", window.Arachnid.announcement(), "Pairs increased by 1, from 0 to 1; Moves increased by 1, from 1 to 2");
    });
    then(450, function () {
      var p = Object.keys(groups)[n];
      check("pair " + (n + 1) + " painted face up", [pixel(groups[p][0]), pixel(groups[p][1])], [FRONT, FRONT]);
      check("matched card's focus variant", focusedEl().getAttribute("aria-label"), "Card " + (groups[p][1] + 1) + ", row " + (Math.floor(groups[p][1] / 4) + 1) + ", column " + (groups[p][1] % 4 + 1) + ": " + p + ", matched");
    });
  });
  then(0, function () {
    check("all pairs found", [S().pairs, S().moves], [8, 9]);
    check("win panel shown", !document.querySelector('[data-goya-id="won"]').hidden, true);
    for (var k = 0; k < 6 && cardOf(focusedId()) >= 0; k++) press(39);
    check("control offers Play again", focusedId(), "control.again.0");
    press(13);
    check("Play again counts the win and deals again", [S().games, S().deal, S().pairs, S().moves], [1, 1, 0, 0]);
    check("D8 record written to the webOS store", window.localStorage.getItem("goya.state.v1/goya_pairs"), '{"format":"goya.state.v1","values":{"games":["int","1"]}}');
    check("D9 announcement for Play again", window.Arachnid.announcement(), "Pairs decreased by 8, from 8 to 0; Moves decreased by 9, from 9 to 0; Games won increased by 1, from 0 to 1");
  });
  then(450, function () {
    check("new deal painted face down", [pixel(0), pixel(15)], [BACK, BACK]);
    goTo(0);
    press(13);
    check("deal 1 has its own layout", focusedEl().getAttribute("aria-label"), "Card 1, row 1, column 1: " + picOf(0, 1) + ", turned over");
  });
  document.addEventListener("arachnid:splash-end", function () { run(0); });
})();
