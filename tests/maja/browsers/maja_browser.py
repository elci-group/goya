#!/usr/bin/env python3
"""Exercise a `maja.web.local.v2` build in a real headless browser.

Usage: python3 tests/maja/browsers/maja_browser.py chrome|firefox counter|scroll|durable|initial|display OUTPUT_DIRECTORY

`counter`: OUTPUT_DIRECTORY is `maja build fixtures/nevada/counter.gscene
--profile maja.web.local.v2 --out ...`. `scroll`: a build of
`tests/maja/fixtures/scroll.gscene`. `durable`: a build of
`tests/maja/fixtures/durable-counter.gscene` (the counter with `count`
durable). `initial`: a build of
`tests/maja/fixtures/initial-content.gscene` (the counter's field has
`text: "7"`, its initial content under decision D3). `display`: a `maja.web.local.v3`
build of `tests/maja/fixtures/display-counter.gscene` (`text-from`
display bindings, decision D9). Requires websocket-client and the browser.
Chrome is driven through the DevTools Protocol, Firefox through WebDriver
BiDi; neither needs a separate driver binary. `MAJA_CHROME` and
`MAJA_FIREFOX` override the executables.

Expected outcomes are the canonical D1 results for `set count = state.count +
input.amount` from count 0, revision 0 (checked i64 addition; a failure
leaves state and revision unchanged). The same semantics are differentially
tested against the canonical executor in crates/maja-runtime/tests.

Observation uses the shell's read-only `goya:transition` DOM event. The
page's CSP admits no other script that could listen.
"""
import functools
import http.server
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

import websocket

I64_MAX = "9223372036854775807"


class Chrome:
    """Chrome DevTools Protocol driver."""

    name = "chrome"
    # Why keyboard activation cannot be verified with this driver, or None.
    keyboard_activation_gap = None

    def __init__(self, profile):
        self.errors = []
        self.sequence = 0
        self.process = subprocess.Popen([
            os.environ.get("MAJA_CHROME", "google-chrome"), "--headless", "--disable-gpu",
            "--no-first-run", "--no-default-browser-check", "--remote-debugging-port=0",
            "--remote-allow-origins=*", "--window-size=400,600",
            "--force-device-scale-factor=1", "--user-data-dir=" + profile, "about:blank",
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        port_file = Path(profile) / "DevToolsActivePort"
        deadline = time.monotonic() + 15
        while not port_file.exists():
            if self.process.poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError("Chrome did not expose a DevTools port")
            time.sleep(0.05)
        port = int(port_file.read_text().splitlines()[0])
        with urllib.request.urlopen(f"http://127.0.0.1:{port}/json", timeout=5) as response:
            pages = json.load(response)
        page = next(item for item in pages if item["type"] == "page")
        self.sock = websocket.create_connection(page["webSocketDebuggerUrl"], timeout=10, http_proxy_host=None)
        for domain in ("Runtime", "Log", "Page", "Accessibility"):
            self.call(f"{domain}.enable")

    def call(self, method, params=None):
        self.sequence += 1
        request_id = self.sequence
        self.sock.send(json.dumps({"id": request_id, "method": method, "params": params or {}}))
        while True:
            message = json.loads(self.sock.recv())
            event, params_ = message.get("method"), message.get("params", {})
            if event == "Runtime.exceptionThrown":
                self.errors.append(params_)
            if event == "Log.entryAdded" and params_["entry"]["level"] == "error":
                self.errors.append(params_)
            if event == "Runtime.consoleAPICalled" and params_["type"] == "error":
                self.errors.append(params_)
            if message.get("id") == request_id:
                if "error" in message:
                    raise RuntimeError(str(message["error"]))
                return message.get("result", {})

    def evaluate(self, expression, await_promise=False):
        result = self.call("Runtime.evaluate", {
            "expression": expression, "returnByValue": True, "awaitPromise": await_promise,
        })
        if "exceptionDetails" in result:
            raise RuntimeError(str(result["exceptionDetails"]))
        return result["result"].get("value")

    def navigate(self, url):
        self.call("Page.navigate", {"url": url})

    def reload(self):
        self.call("Page.reload", {"ignoreCache": True})

    KEYS = {"Tab": ("Tab", 9, None), "Enter": ("Enter", 13, "\r"), " ": ("Space", 32, " "), "Delete": ("Delete", 46, None)}

    def press(self, key):
        code, vk, text = self.KEYS[key]
        down = {"type": "keyDown", "key": key, "code": code, "windowsVirtualKeyCode": vk}
        if text is not None:
            down["text"] = text
        self.call("Input.dispatchKeyEvent", down)
        self.call("Input.dispatchKeyEvent", {"type": "keyUp", "key": key, "code": code, "windowsVirtualKeyCode": vk})

    def type_text(self, text):
        self.call("Input.insertText", {"text": text})

    def click(self, x, y):
        for kind in ("mousePressed", "mouseReleased"):
            self.call("Input.dispatchMouseEvent", {"type": kind, "x": x, "y": y, "button": "left", "clickCount": 1})

    def wheel(self, x, y, dy):
        self.call("Input.dispatchMouseEvent", {"type": "mouseWheel", "x": x, "y": y, "deltaX": 0, "deltaY": dy})

    def accessibility(self):
        nodes = self.call("Accessibility.getFullAXTree")["nodes"]
        return [(n.get("role", {}).get("value"), n.get("name", {}).get("value")) for n in nodes if not n.get("ignored")]

    def version(self):
        return self.call("Browser.getVersion")["product"]

    def close(self):
        self.sock.close()


class Firefox:
    """WebDriver BiDi driver (no geckodriver)."""

    name = "firefox"
    keyboard_activation_gap = (
        "headless Firefox 150 over BiDi delivers keydown/keypress/keyup to the focused "
        "native button but synthesizes no click (document.hasFocus() stays false, even "
        "with focusmanager.testmode); a programmatic or pointer click works"
    )
    KEYS = {"Tab": "", "Enter": "", " ": " ", "Delete": ""}

    def __init__(self, profile):
        self.errors = []
        self.sequence = 0
        # Headless windows never gain OS focus; Gecko's own test harnesses
        # set this so focus, and keyboard activation that depends on it,
        # behave as in a focused window.
        (Path(profile) / "user.js").write_text('user_pref("focusmanager.testmode", true);\n')
        self.log = open(Path(profile) / "stderr.log", "w+")
        self.process = subprocess.Popen([
            os.environ.get("MAJA_FIREFOX", "firefox"), "--headless", "--no-remote",
            "--remote-debugging-port=0", "--profile", profile,
            "--width=400", "--height=600", "about:blank",
        ], stdout=subprocess.DEVNULL, stderr=self.log)
        deadline = time.monotonic() + 30
        url = None
        while url is None:
            if self.process.poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError("Firefox did not expose WebDriver BiDi")
            self.log.seek(0)
            match = re.search(r"WebDriver BiDi listening on (ws://\S+)", self.log.read())
            url = match.group(1) if match else None
            time.sleep(0.05)
        self.sock = websocket.create_connection(url.rstrip("/") + "/session", timeout=15, http_proxy_host=None, suppress_origin=True)
        self.session = self.call("session.new", {"capabilities": {}})
        self.call("session.subscribe", {"events": ["log.entryAdded"]})
        self.context = self.call("browsingContext.getTree", {})["contexts"][0]["context"]

    def call(self, method, params):
        self.sequence += 1
        request_id = self.sequence
        self.sock.send(json.dumps({"id": request_id, "method": method, "params": params}))
        while True:
            message = json.loads(self.sock.recv())
            if message.get("method") == "log.entryAdded" and message["params"].get("level") == "error":
                self.errors.append(message["params"])
            if message.get("id") == request_id:
                if message.get("type") == "error":
                    raise RuntimeError(f"{message.get('error')}: {message.get('message')}")
                return message["result"]

    def evaluate(self, expression, await_promise=False):
        # Serialize in the page so results arrive as plain JSON.
        wrapped = f"(async () => JSON.stringify(await ({expression})))()" if await_promise else f"JSON.stringify({expression})"
        result = self.call("script.evaluate", {
            "expression": wrapped, "target": {"context": self.context}, "awaitPromise": True,
        })
        if result["type"] == "exception":
            raise RuntimeError(str(result["exceptionDetails"]))
        value = result["result"]
        if value["type"] == "undefined":
            return None
        return json.loads(value["value"])

    def navigate(self, url):
        self.call("browsingContext.navigate", {"context": self.context, "url": url, "wait": "complete"})

    def reload(self):
        self.call("browsingContext.reload", {"context": self.context, "wait": "complete"})

    def keys(self, values):
        actions = []
        for value in values:
            actions += [{"type": "keyDown", "value": value}, {"type": "keyUp", "value": value}]
        self.call("input.performActions", {"context": self.context, "actions": [{"type": "key", "id": "keyboard", "actions": actions}]})

    def press(self, key):
        self.keys([self.KEYS[key]])

    def type_text(self, text):
        self.keys(list(text))

    def click(self, x, y):
        self.call("input.performActions", {"context": self.context, "actions": [{
            "type": "pointer", "id": "mouse", "parameters": {"pointerType": "mouse"},
            "actions": [
                {"type": "pointerMove", "x": int(x), "y": int(y)},
                {"type": "pointerDown", "button": 0},
                {"type": "pointerUp", "button": 0},
            ],
        }]})

    def wheel(self, x, y, dy):
        self.call("input.performActions", {"context": self.context, "actions": [{
            "type": "wheel", "id": "wheel",
            "actions": [{"type": "scroll", "x": int(x), "y": int(y), "deltaX": 0, "deltaY": int(dy)}],
        }]})

    def accessibility(self):
        return None  # WebDriver BiDi exposes no accessibility tree.

    def version(self):
        caps = self.session["capabilities"]
        return f"{caps['browserName']}/{caps['browserVersion']}"

    def close(self):
        try:
            self.call("session.end", {})
        except Exception:
            pass
        self.sock.close()
        self.log.close()


GEOMETRY = """(async () => {
    const { CELLS } = await import('./scene.js');
    const { resolve } = await import('./layout.js');
    const vw = document.documentElement.clientWidth, vh = document.documentElement.clientHeight;
    const out = [];
    resolve(CELLS, vw, vh).forEach(({ rect }, i) => {
      const r = document.querySelector(`[data-goya-id="${CELLS[i].id}"]`).getBoundingClientRect();
      const want = [rect.x, rect.y, rect.width, rect.height].map((v) => v / 1000);
      const got = [r.x, r.y, r.width, r.height];
      if (want.some((v, j) => Math.abs(v - got[j]) > 1 / 30)) out.push({ id: CELLS[i].id, want, got });
    });
    return { vw, vh, n: CELLS.length, out };
})()"""


def check_geometry(browser, passed, label=""):
    geometry = browser.evaluate(GEOMETRY, await_promise=True)
    if geometry["out"] or geometry["vw"] <= 0 or geometry["vh"] <= 0:
        raise AssertionError(f"geometry mismatch: {geometry!r}")
    passed.append(f"geometry of {geometry['n']} elements at {geometry['vw']}x{geometry['vh']} CSS px{label}")


def wait_for_scene(browser, selector):
    deadline = time.monotonic() + 10
    while not browser.evaluate(f"document.readyState === 'complete' && !!document.querySelector({json.dumps(selector)})"):
        if time.monotonic() >= deadline:
            raise RuntimeError("page did not build the scene")
        time.sleep(0.05)


def scroll_scenario(browser, url):
    """Scroll viewports against Frame::scroll_limits and scrolled_nodes."""
    passed, not_verified = [], []
    ev = browser.evaluate
    browser.navigate(url)
    wait_for_scene(browser, '[data-goya-id="list"] .goya-scroller')
    check_geometry(browser, passed, " (scroll offsets 0)")
    # Browsers round scrollWidth/scrollHeight to whole pixels.
    limits = ev("""(async () => {
        const { CELLS } = await import('./scene.js');
        const { resolve, scrollLimits } = await import('./layout.js');
        const r = resolve(CELLS, document.documentElement.clientWidth, document.documentElement.clientHeight);
        return scrollLimits(CELLS, r).map((l, i) => {
          if (l === null) return null;
          const s = document.querySelector(`[data-goya-id="${CELLS[i].id}"] > .goya-scroller`);
          return { id: CELLS[i].id, want: l.map((v) => v / 1000), got: [s.scrollWidth - s.clientWidth, s.scrollHeight - s.clientHeight] };
        }).filter((x) => x !== null);
    })()""", await_promise=True)
    for item in limits:
        if any(abs(w - g) > 1 for w, g in zip(item["want"], item["got"])):
            raise AssertionError(f"scroll limits differ: {limits!r}")
    if not any(item["want"][1] > 0 for item in limits) or not any(item["want"] == [0, 0] for item in limits):
        raise AssertionError(f"fixture must have an overflowing and a fitting viewport: {limits!r}")
    passed.append("scroll extents equal Frame::scroll_limits within 1 px: " + ", ".join(f"{i['id']} {i['want'][0]:g}x{i['want'][1]:g} px" for i in limits))

    scroller = "document.querySelector('[data-goya-id=\"list\"] > .goya-scroller')"
    box = ev(f"(() => {{ const r = {scroller}.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; }})()")
    browser.wheel(box[0], box[1], 120)
    deadline = time.monotonic() + 3
    while ev(f"{scroller}.scrollTop") == 0 and time.monotonic() < deadline:
        time.sleep(0.05)
    top = ev(f"{scroller}.scrollTop")
    if not 0 < top:
        raise AssertionError("wheel did not scroll the viewport")
    passed.append(f"wheel over the viewport scrolls it (scrollTop {top:g} px)")

    # Scroll fully: content moves by exactly the offset, the viewport's own
    # text and stroke do not (Frame::scrolled_nodes translates descendants only).
    moved = ev(f"""(() => {{
        const s = {scroller};
        s.scrollTop = 0; s.scrollLeft = 0;
        const label = document.querySelector('[data-goya-id="list"] > .goya-text').getBoundingClientRect().y;
        const before = Object.fromEntries(['row1', 'row2a', 'wide', 'tap'].map((id) => [id, document.querySelector(`[data-goya-id="${{id}}"]`).getBoundingClientRect().y]));
        s.scrollTop = 1e9; s.scrollLeft = 1e9;
        const dy = s.scrollTop;
        const after = Object.fromEntries(Object.keys(before).map((id) => [id, document.querySelector(`[data-goya-id="${{id}}"]`).getBoundingClientRect().y]));
        return {{ dy, max: s.scrollHeight - s.clientHeight, label, labelAfter: document.querySelector('[data-goya-id="list"] > .goya-text').getBoundingClientRect().y,
                 shifts: Object.keys(before).map((id) => before[id] - after[id]) }};
    }})()""")
    if moved["dy"] <= 0 or abs(moved["dy"] - moved["max"]) > 1 or any(abs(d - moved["dy"]) > 1 / 30 for d in moved["shifts"]) or moved["label"] != moved["labelAfter"]:
        raise AssertionError(f"scrolled positions: {moved!r}")
    passed.append(f"at the limit every descendant moved by the offset ({moved['dy']:g} px); the viewport's own text stayed put")
    hidden = ev("""(() => { const r = document.querySelector('[data-goya-id="row1"]').getBoundingClientRect(); const v = document.querySelector('[data-goya-id="list"]').getBoundingClientRect();
        const e = document.elementFromPoint(v.x + 2, v.y + 2); return e && e.closest('[data-goya-id]') ? e.closest('[data-goya-id]').dataset.goyaId : null; })()""")
    if hidden == "row1":
        raise AssertionError("content scrolled out of the viewport is still hit")
    passed.append("content scrolled past the viewport edge is clipped (not hit-testable)")
    ev("0")
    if browser.errors:
        raise AssertionError(f"browser errors: {browser.errors!r}")
    passed.append("no console errors or uncaught exceptions")
    not_verified.append("keyboard and touch scrolling; scroll offsets across resize (Frame reconcile)")
    return passed, not_verified


def scenario(browser, url):
    passed = []
    not_verified = []
    ev = browser.evaluate

    def load(action):
        action()
        deadline = time.monotonic() + 10
        while not ev("document.readyState === 'complete' && !!document.querySelector('[data-goya-id=\"window.add\"] button')"):
            if time.monotonic() >= deadline:
                raise RuntimeError("page did not build the scene")
            time.sleep(0.05)
        ev("(window.__events = [], document.addEventListener('goya:transition', (e) => window.__events.push(e.detail)), 0)")

    def focused():
        return ev("document.activeElement && document.activeElement.closest('[data-goya-id]') ? document.activeElement.closest('[data-goya-id]').dataset.goyaId : null")

    field = "document.querySelector('[data-goya-id=\"window.amount\"] input')"

    def set_field(text):
        ev(f"({field}.focus(), {field}.select(), 0)")
        if text:
            browser.type_text(text)
        else:
            browser.press("Delete")
        if ev(f"{field}.value") != text:
            raise AssertionError(f"field text was not entered: {ev(f'{field}.value')!r}")

    def click_add():
        box = ev("(() => { const r = document.querySelector('[data-goya-id=\"window.add\"] button').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()")
        browser.click(box[0], box[1])

    def last_event(expected_count):
        events = ev("window.__events")
        if len(events) != expected_count:
            raise AssertionError(f"expected {expected_count} transition events, saw {events!r}")
        return events[-1]

    def expect(event, ok, revision, count=None, code=None):
        if event["ok"] is not ok or event["revision"] != revision or event.get("code") != code:
            raise AssertionError(f"unexpected outcome {event!r}")
        if count is not None and event["state"] != {"count": ["int", count]}:
            raise AssertionError(f"unexpected state {event!r}")

    load(lambda: browser.navigate(url))

    # Geometry against the layout mirror, which is diff-tested against
    # goya_lang::layout under Node. Engines lay out in 1/64 (Blink) or
    # 1/60 (Gecko) px.
    geometry = ev("""(async () => {
        const { CELLS } = await import('./scene.js');
        const { resolve } = await import('./layout.js');
        const vw = document.documentElement.clientWidth, vh = document.documentElement.clientHeight;
        const out = [];
        resolve(CELLS, vw, vh).forEach(({ rect }, i) => {
          const r = document.querySelector(`[data-goya-id="${CELLS[i].id}"]`).getBoundingClientRect();
          const want = [rect.x, rect.y, rect.width, rect.height].map((v) => v / 1000);
          const got = [r.x, r.y, r.width, r.height];
          if (want.some((v, j) => Math.abs(v - got[j]) > 1 / 30)) out.push({ id: CELLS[i].id, want, got });
        });
        return { vw, vh, out };
    })()""", await_promise=True)
    if geometry["out"] or geometry["vw"] <= 0 or geometry["vh"] <= 0:
        raise AssertionError(f"geometry mismatch: {geometry!r}")
    passed.append(f"geometry of 4 elements at {geometry['vw']}x{geometry['vh']} CSS px")

    ev("(document.activeElement && document.activeElement.blur(), 0)")
    browser.press("Tab")
    if focused() != "window.amount":
        raise AssertionError(f"first Tab focused {focused()!r}")
    browser.type_text("5")
    browser.press("Tab")
    if focused() != "window.add":
        raise AssertionError(f"second Tab focused {focused()!r}")
    ev("(window.__keys = [], document.addEventListener('keyup', (e) => window.__keys.push(e.key + ':' + e.target.tagName), true), 0)")
    browser.press("Enter")
    if ev("window.__events").__len__() == 0 and browser.keyboard_activation_gap:
        if ev("window.__keys") != ["Enter:BUTTON"]:
            raise AssertionError(f"Enter did not reach the button: {ev('window.__keys')!r}")
        not_verified.append(f"Enter/Space activation: {browser.keyboard_activation_gap}")
        click_add()
        expect(last_event(1), True, "1", "5")
        passed.append("Tab order field -> button; Enter reaches the button; pointer activates: count 0 + 5 = 5, revision 1")
    else:
        expect(last_event(1), True, "1", "5")
        passed.append("Tab order field -> button; Enter activates: count 0 + 5 = 5, revision 1")

    set_field("-2")
    click_add()
    expect(last_event(2), True, "2", "3")
    passed.append("pointer click: 5 + -2 = 3, revision 2")

    for text in ("x", "+1", "01", " 1", ""):
        set_field(text)
        click_add()
    events = ev("window.__events")[2:]
    if len(events) != 5 or any(e["ok"] or e["code"] != "NOT_CANONICAL_INT" or e["field"] != "window.amount" or e["revision"] != "2" for e in events):
        raise AssertionError(f"conversion failures: {events!r}")
    if ev(f"{field}.getAttribute('aria-invalid')") != "true":
        raise AssertionError("invalid field is not marked aria-invalid")
    passed.append("non-canonical field text ('x', '+1', '01', ' 1', '') refused before dispatch; revision stays 2")

    set_field(I64_MAX)
    if ev(f"{field}.getAttribute('aria-invalid')") is not None:
        raise AssertionError("editing the field did not clear aria-invalid")
    click_add()
    expect(last_event(8), False, "2", "3", "ARITHMETIC_OVERFLOW")
    if ev("document.getElementById('goya-status').textContent") != "Action failed: ARITHMETIC_OVERFLOW":
        raise AssertionError("D1 failure was not reported in the status region")
    passed.append("3 + i64::MAX refused with ARITHMETIC_OVERFLOW; count 3, revision 2 unchanged; reported in role=status")

    set_field("0")
    ev("(document.querySelector('[data-goya-id=\"window.add\"] button').focus(), 0)")
    if browser.keyboard_activation_gap:
        click_add()
    else:
        browser.press(" ")
    expect(last_event(9), True, "3", "3")
    if ev("document.getElementById('goya-status').textContent") != "":
        raise AssertionError("status was not cleared after success")
    passed.append(("pointer" if browser.keyboard_activation_gap else "Space") + " activates: 3 + 0 = 3, revision 3; status cleared")

    roles = browser.accessibility()
    if roles is None:
        not_verified.append("accessibility tree: not exposed by WebDriver BiDi")
    else:
        if ("button", "Add") not in roles or ("StaticText", "Counter") not in roles:
            raise AssertionError(f"AX tree: {roles!r}")
        textboxes = [name for role, name in roles if role == "textbox"]
        if len(textboxes) != 1:
            raise AssertionError(f"expected one textbox: {roles!r}")
        passed.append(f"AX tree: button 'Add', StaticText 'Counter', one textbox (accessible name: {textboxes[0]!r})")

    load(browser.reload)
    set_field("1")
    click_add()
    expect(last_event(1), True, "1", "1")
    passed.append("reload restarts session state from the authored initial value: 0 + 1 = 1, revision 1")

    ev("0")
    if browser.errors:
        raise AssertionError(f"browser errors: {browser.errors!r}")
    passed.append("no console errors or uncaught exceptions")
    return passed, not_verified


def durable_scenario(browser, url):
    """Durable state: restore across reload, write-before-commit, restore failure."""
    passed = []
    not_verified = []
    ev = browser.evaluate
    record_key = "goya.state.v1/counter"
    field = "document.querySelector('[data-goya-id=\"window.amount\"] input')"

    def load(action):
        action()
        deadline = time.monotonic() + 10
        while not ev("document.readyState === 'complete' && !!document.querySelector('[data-goya-id=\"window.add\"] button')"):
            if time.monotonic() >= deadline:
                raise RuntimeError("page did not build the scene")
            time.sleep(0.05)
        ev("(window.__events = [], document.addEventListener('goya:transition', (e) => window.__events.push(e.detail)), 0)")
        return ev("document.documentElement.dataset.goyaRestore"), ev("document.getElementById('goya-status').textContent")

    def add(text):
        ev(f"({field}.focus(), {field}.select(), 0)")
        browser.type_text(text)
        if ev(f"{field}.value") != text:
            raise AssertionError(f"field text was not entered: {ev(f'{field}.value')!r}")
        box = ev("(() => { const r = document.querySelector('[data-goya-id=\"window.add\"] button').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()")
        browser.click(box[0], box[1])
        events = ev("window.__events")
        if not events:
            raise AssertionError("no transition event")
        return events[-1]

    def expect(event, ok, revision, count, code=None):
        if event["ok"] is not ok or event["revision"] != revision or event.get("code") != code or event["state"] != {"count": ["int", count]}:
            raise AssertionError(f"unexpected outcome {event!r}")

    def record():
        return ev(f"localStorage.getItem({json.dumps(record_key)})")

    def stored(count):
        return json.dumps({"format": "goya.state.v1", "values": {"count": ["int", count]}}, separators=(",", ":"))

    restored, notice = load(lambda: browser.navigate(url))
    if restored != "ok" or notice != "" or record() is not None:
        raise AssertionError(f"first load: restore {restored!r}, status {notice!r}, record {record()!r}")
    expect(add("5"), True, "1", "5")
    if record() != stored("5"):
        raise AssertionError(f"record after commit: {record()!r}")
    passed.append(f"missing record starts from the authored value; 0 + 5 = 5 commits and stores {record()}")

    restored, notice = load(browser.reload)
    if restored != "ok":
        raise AssertionError(f"reload restore: {restored!r}")
    expect(add("1"), True, "1", "6")
    passed.append("reload restores count 5 before any input: 5 + 1 = 6 (revision restarts at 1 per page)")

    expect(add(I64_MAX), False, "1", "6", "ARITHMETIC_OVERFLOW")
    if record() != stored("6"):
        raise AssertionError(f"a refused transition changed the record: {record()!r}")
    passed.append("a D1 failure (ARITHMETIC_OVERFLOW) leaves the stored record at 6")

    # Simulated storage failure: the page's own Storage.setItem is replaced
    # for this document only (cleared by the next reload).
    ev("(Storage.prototype.setItem = function () { throw new DOMException('simulated', 'QuotaExceededError'); }, 0)")
    expect(add("1"), False, "1", "6", "STORAGE_FAILED")
    if ev("document.getElementById('goya-status').textContent") != "Action failed: STORAGE_FAILED" or record() != stored("6"):
        raise AssertionError("storage failure was not reported or changed the record")
    passed.append("a throwing setItem refuses the commit: STORAGE_FAILED, count 6 and revision 1 unchanged, reported in role=status (failure simulated)")
    not_verified.append("a real QuotaExceededError (setItem failure was simulated)")

    restored, _ = load(browser.reload)
    expect(add("0"), True, "1", "6")
    passed.append("after the refused write, reload still restores 6")

    ev(f"(localStorage.setItem({json.dumps(record_key)}, '{{'), 0)")
    restored, notice = load(browser.reload)
    if restored != "RECORD_UNDECODABLE" or "RECORD_UNDECODABLE" not in notice or record() != "{":
        raise AssertionError(f"corrupt record: restore {restored!r}, status {notice!r}, record {record()!r}")
    passed.append(f"an undecodable record fails restore, is reported ({notice!r}) and is not overwritten on load")
    expect(add("2"), True, "1", "2")
    if record() != stored("2") or ev("document.getElementById('goya-status').textContent") != "":
        raise AssertionError(f"first durable commit after a failed restore: record {record()!r}")
    passed.append("the first commit after a failed restore starts from the authored value (0 + 2 = 2), replaces the record and clears the notice")

    ev(f"(localStorage.setItem({json.dumps(record_key)}, {json.dumps(json.dumps({'format': 'goya.state.v1', 'values': {'count': ['text', '9']}}))}), 0)")
    restored, _ = load(browser.reload)
    if restored != "RECORD_TYPE_MISMATCH":
        raise AssertionError(f"retyped record: {restored!r}")
    expect(add("0"), True, "1", "0")
    passed.append("a retyped record fails restore with RECORD_TYPE_MISMATCH")

    # Another window's write (decision D8), simulated by writing the record
    # from DevTools: the next durable commit is refused, not overwritten.
    ev(f"(localStorage.setItem({json.dumps(record_key)}, {json.dumps(stored('40'))}), 0)")
    expect(add("1"), False, "1", "0", "STORAGE_CONFLICT")
    status = ev("document.getElementById('goya-status').textContent")
    if record() != stored("40") or not status.startswith("Action failed: STORAGE_CONFLICT") or "reload" not in status:
        raise AssertionError(f"conflict: record {record()!r}, status {status!r}")
    passed.append("a record changed by another window refuses the next commit with STORAGE_CONFLICT; the other window's 40 survives; reported with reload advice")
    load(browser.reload)
    expect(add("1"), True, "1", "41")
    passed.append("reload adopts the other window's value: 40 + 1 = 41")
    not_verified.append("a second real tab (the other window's write was made from DevTools in the same tab)")
    not_verified.append("crash between setItem and commit (the write precedes the commit by construction, not by test)")
    ev("0")
    if browser.errors:
        raise AssertionError(f"browser errors: {browser.errors!r}")
    passed.append("no console errors or uncaught exceptions")
    return passed, not_verified


def initial_scenario(browser, url):
    """`text:` on @input is the field's initial content (decision D3)."""
    passed = []
    ev = browser.evaluate
    field = "document.querySelector('[data-goya-id=\"window.amount\"] input')"

    def load(action):
        action()
        deadline = time.monotonic() + 10
        while not ev("document.readyState === 'complete' && !!document.querySelector('[data-goya-id=\"window.add\"] button')"):
            if time.monotonic() >= deadline:
                raise RuntimeError("page did not build the scene")
            time.sleep(0.05)
        ev("(window.__events = [], document.addEventListener('goya:transition', (e) => window.__events.push(e.detail)), 0)")

    def click_add():
        box = ev("(() => { const r = document.querySelector('[data-goya-id=\"window.add\"] button').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()")
        browser.click(box[0], box[1])
        return ev("window.__events")[-1]

    load(lambda: browser.navigate(url))
    if ev(f"{field}.value") != "7" or ev(f"{field}.placeholder") != "":
        raise AssertionError(f"field: value {ev(f'{field}.value')!r}, placeholder {ev(f'{field}.placeholder')!r}")
    passed.append("the field starts with its authored text '7' as content, not as a placeholder")
    event = click_add()
    if not event["ok"] or event["state"] != {"count": ["int", "7"]}:
        raise AssertionError(f"activation with initial content: {event!r}")
    passed.append("activating without typing converts the initial content: 0 + 7 = 7")
    ev(f"({field}.focus(), {field}.select(), 0)")
    browser.type_text("3")
    load(browser.reload)
    if ev(f"{field}.value") != "7":
        raise AssertionError(f"after reload the field holds {ev(f'{field}.value')!r}")
    passed.append("after editing to '3', reload shows the authored '7' again (no browser form restoration)")
    ev("0")
    if browser.errors:
        raise AssertionError(f"browser errors: {browser.errors!r}")
    passed.append("no console errors or uncaught exceptions")
    return passed, []


def display_scenario(browser, url):
    """`text-from` display bindings (decision D9, profile maja.web.local.v3)."""
    passed = []
    not_verified = []
    ev = browser.evaluate

    def load(action):
        action()
        deadline = time.monotonic() + 10
        while not ev("document.readyState === 'complete' && !!document.querySelector('[data-goya-id=\"raise\"] button')"):
            if time.monotonic() >= deadline:
                raise RuntimeError("page did not build the scene")
            time.sleep(0.05)
        ev("(window.__events = [], document.addEventListener('goya:transition', (e) => window.__events.push(e.detail)), 0)")

    def shown():
        return ev("""({
            total: document.querySelector('[data-goya-id="total"] .goya-text').textContent,
            shown: document.querySelector('[data-goya-id="shown"] .goya-text').textContent,
            raise: document.querySelector('[data-goya-id="raise"] button').textContent,
        })""")

    def activate(button, field=None, text=None):
        if field is not None:
            sel = f"document.querySelector('[data-goya-id=\"{field}\"] input')"
            ev(f"({sel}.focus(), {sel}.select(), 0)")
            browser.type_text(text)
            if ev(f"{sel}.value") != text:
                raise AssertionError(f"field text was not entered: {ev(f'{sel}.value')!r}")
        box = ev(f"(() => {{ const r = document.querySelector('[data-goya-id=\"{button}\"] button').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; }})()")
        browser.click(box[0], box[1])
        return ev("window.__events")[-1]

    def expect(want, what):
        got = shown()
        if got != want:
            raise AssertionError(f"{what}: shown {got!r}, expected {want!r}")

    def announced():
        return ev("document.getElementById('goya-announce').textContent")

    def expect_announced(want, what):
        got = announced()
        if got != want:
            raise AssertionError(f"{what}: announced {got!r}, expected {want!r}")

    initial = {"total": "Count: 0", "shown": "none", "raise": "Raised: false"}
    load(lambda: browser.navigate(url))
    expect(initial, "first load")
    if ev("document.documentElement.dataset.goyaRestore") != "ok":
        raise AssertionError("restore did not run")
    region = ev("""(() => { const a = document.getElementById('goya-announce'); const r = a.getBoundingClientRect();
        return [a.getAttribute('aria-live'), a.getAttribute('aria-atomic'), r.width <= 1 && r.height <= 1, a.textContent]; })()""")
    if region != ["polite", "true", True, ""]:
        raise AssertionError(f"announcement region: {region!r}")
    passed.append("a visually hidden polite live region exists and announces nothing at load")
    passed.append("on load (set in the task that creates the elements) the authored values follow static text: 'Count: 0', 'none', button 'Raised: false'")

    if not activate("add", "amount", "-12")["ok"]:
        raise AssertionError("add failed")
    expect({**initial, "total": "Count: -12"}, "after add")
    expect_announced("Count decreased by 12, from 0 to -12", "after add")
    passed.append("a committed int transition is shown as canonical decimal ('Count: -12') and announced as 'Count decreased by 12, from 0 to -12'")

    tricky = "<b>&amp; γειά 🌍  x"
    if not activate("rename", "name", tricky)["ok"]:
        raise AssertionError("rename failed")
    expect({**initial, "total": "Count: -12", "shown": tricky}, "after rename")
    if ev("document.querySelector('[data-goya-id=\"shown\"] b')") is not None:
        raise AssertionError("text value was parsed as markup")
    expect_announced(f'label changed from "none" to "{tricky}"', "after rename")
    passed.append("a text value is shown verbatim, never as markup, keeping double spaces and non-ASCII; announced with the key as context when the element has no text")

    long = " ".join(["word"] * 40)
    if not activate("rename", "name", long)["ok"]:
        raise AssertionError("long rename failed")
    wrapped = ev("""(() => { const t = document.querySelector('[data-goya-id="shown"] .goya-text');
        const v = t.querySelector('[data-goya-text-from]'); const box = t.parentElement.getBoundingClientRect();
        const rects = [...v.getClientRects()];
        return [getComputedStyle(t).whiteSpace, rects.length, rects.every((r) => r.right <= box.right + 0.5)]; })()""")
    if wrapped[0] != "pre-wrap" or wrapped[1] < 2 or not wrapped[2]:
        raise AssertionError(f"long value is not wrapped at the element edge: {wrapped!r}")
    passed.append(f"a long value wraps at the element's edge (white-space: pre-wrap, {wrapped[1]} line boxes, none past the right edge)")
    if not activate("rename", "name", tricky)["ok"]:
        raise AssertionError("rename back failed")

    if not activate("raise")["ok"]:
        raise AssertionError("raise failed")
    expect({"total": "Count: -12", "shown": tricky, "raise": "Raised: true"}, "after raise")
    expect_announced("Raised changed from false to true", "after raise")
    passed.append("a bool value is shown as 'true' on a button after its static label and announced as 'Raised changed from false to true'")

    event = activate("add", "amount", "-9223372036854775807")
    if event["ok"] or event["code"] != "ARITHMETIC_OVERFLOW":
        raise AssertionError(f"overflow: {event!r}")
    expect({"total": "Count: -12", "shown": tricky, "raise": "Raised: true"}, "after refused add")
    expect_announced("Raised changed from false to true", "after refused add")
    passed.append("a refused transition (ARITHMETIC_OVERFLOW) changes nothing on screen and announces nothing new (the failure goes to role=status)")

    load(browser.reload)
    expect({**initial, "total": "Count: -12"}, "after reload")
    passed.append("reload: the durable count is shown restored (-12) before any input; session values show their authored values")

    roles = browser.accessibility()
    if roles is None:
        not_verified.append("accessibility tree: not exposed by WebDriver BiDi")
    else:
        if ("button", "Raised: false") not in roles:
            raise AssertionError(f"AX tree: {roles!r}")
        passed.append("AX tree: the button's accessible name includes its displayed value ('Raised: false')")
    not_verified.append("that a screen reader speaks the live region (no assistive technology on this host)")

    ev("0")
    if browser.errors:
        raise AssertionError(f"browser errors: {browser.errors!r}")
    passed.append("no console errors or uncaught exceptions")
    return passed, not_verified


def main(kind, name, output):
    output = Path(output).resolve(strict=True)
    required = ("index.html", "app.js", "scene.js", "layout.js") + (("d1.js",) if name != "scroll" else ()) + (("storage.js",) if name in ("durable", "display") else ())
    for file in required:
        if not (output / file).is_file():
            raise RuntimeError(f"output does not contain {file}")

    class Quiet(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *args):
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Quiet, directory=str(output)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    browser = None
    try:
        # Browser helper processes can still be flushing storage into the
        # profile when the main process exits; leftovers are not a failure.
        with tempfile.TemporaryDirectory(prefix=f"maja-{kind}-", ignore_cleanup_errors=True) as profile:
            browser = {"chrome": Chrome, "firefox": Firefox}[kind](profile)
            try:
                passed, not_verified = {"counter": scenario, "scroll": scroll_scenario, "durable": durable_scenario, "initial": initial_scenario, "display": display_scenario}[name](browser, f"http://127.0.0.1:{server.server_port}/index.html")
                version = browser.version()
            finally:
                browser.close()
                browser.process.terminate()
                try:
                    browser.process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    browser.process.kill()
                    browser.process.wait()
            for line in passed:
                print("PASS:", line)
            for line in not_verified:
                print("NOT VERIFIED:", line)
            print("BROWSER:", version)
            print("NOT TESTED: assistive technology, IME composition, touch, visual typography" + ("" if name in ("durable", "display") else ", durable state"))
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


if __name__ == "__main__":
    if len(sys.argv) != 4 or sys.argv[1] not in ("chrome", "firefox") or sys.argv[2] not in ("counter", "scroll", "durable", "initial", "display"):
        raise SystemExit("usage: maja_browser.py chrome|firefox counter|scroll|durable|initial|display OUTPUT_DIRECTORY")
    main(sys.argv[1], sys.argv[2], sys.argv[3])
