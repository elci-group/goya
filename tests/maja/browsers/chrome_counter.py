#!/usr/bin/env python3
"""Exercise a `maja.web.local.v2` counter build in real headless Chrome via CDP.

Usage: python3 tests/maja/browsers/chrome_counter.py OUTPUT_DIRECTORY

OUTPUT_DIRECTORY is `maja build fixtures/nevada/counter.gscene --profile
maja.web.local.v2 --out ...`. Requires Chrome and websocket-client.

Expected outcomes are the canonical D1 results for `set count = state.count +
input.amount` from count 0, revision 0 (checked i64 addition; a failure
leaves state and revision unchanged). The same semantics are differentially
tested against the canonical executor in crates/maja-runtime/tests.

Observation uses the shell's read-only `goya:transition` DOM event, read
through CDP. The page's CSP admits no other script that could listen.
"""
import functools
import http.server
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

import websocket

I64_MAX = "9223372036854775807"


def main(output):
    output = Path(output).resolve(strict=True)
    for name in ("index.html", "app.js", "d1.js", "scene.js", "layout.js"):
        if not (output / name).is_file():
            raise RuntimeError(f"output does not contain {name}")
    class Quiet(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *args):
            pass

    handler = functools.partial(Quiet, directory=str(output))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    chrome = None
    sock = None
    passed = []
    try:
        with tempfile.TemporaryDirectory(prefix="maja-chrome-") as profile:
            chrome = subprocess.Popen([
                os.environ.get("MAJA_CHROME", "google-chrome"), "--headless", "--disable-gpu",
                "--no-first-run", "--no-default-browser-check", "--remote-debugging-port=0",
                "--remote-allow-origins=*", "--window-size=400,600",
                "--force-device-scale-factor=1", "--user-data-dir=" + profile, "about:blank",
            ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            port_file = Path(profile) / "DevToolsActivePort"
            deadline = time.monotonic() + 15
            while not port_file.exists():
                if chrome.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("Chrome did not expose a DevTools port")
                time.sleep(0.05)
            port = int(port_file.read_text().splitlines()[0])
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/json", timeout=5) as response:
                pages = json.load(response)
            page = next(item for item in pages if item["type"] == "page")
            sock = websocket.create_connection(page["webSocketDebuggerUrl"], timeout=10, http_proxy_host=None)
            sequence = 0
            errors = []

            def call(method, params=None):
                nonlocal sequence
                sequence += 1
                request_id = sequence
                sock.send(json.dumps({"id": request_id, "method": method, "params": params or {}}))
                while True:
                    message = json.loads(sock.recv())
                    if message.get("method") == "Runtime.exceptionThrown":
                        errors.append(message["params"])
                    if message.get("method") == "Log.entryAdded" and message["params"]["entry"]["level"] == "error":
                        errors.append(message["params"])
                    if message.get("method") == "Runtime.consoleAPICalled" and message["params"]["type"] == "error":
                        errors.append(message["params"])
                    if message.get("id") == request_id:
                        if "error" in message:
                            raise RuntimeError(str(message["error"]))
                        return message.get("result", {})

            def evaluate(expression, await_promise=False):
                result = call("Runtime.evaluate", {
                    "expression": expression, "returnByValue": True, "awaitPromise": await_promise,
                })
                if "exceptionDetails" in result:
                    raise RuntimeError(str(result["exceptionDetails"]))
                return result["result"].get("value")

            def load(navigate):
                navigate()
                deadline = time.monotonic() + 10
                while not evaluate("document.readyState === 'complete' && !!document.querySelector('[data-goya-id=\"window.add\"] button')"):
                    if time.monotonic() >= deadline:
                        raise RuntimeError("page did not build the scene")
                    time.sleep(0.05)
                evaluate("window.__events = []; document.addEventListener('goya:transition', (e) => window.__events.push(e.detail)); 0")

            def key(name, code, vk, text=None):
                down = {"type": "keyDown", "key": name, "code": code, "windowsVirtualKeyCode": vk}
                if text is not None:
                    down["text"] = text
                call("Input.dispatchKeyEvent", down)
                call("Input.dispatchKeyEvent", {"type": "keyUp", "key": name, "code": code, "windowsVirtualKeyCode": vk})

            def set_field(text):
                evaluate("(() => { const f = document.querySelector('[data-goya-id=\"window.amount\"] input'); f.focus(); f.select(); return 0; })()")
                if text:
                    call("Input.insertText", {"text": text})
                else:
                    key("Delete", "Delete", 46)
                if evaluate("document.querySelector('[data-goya-id=\"window.amount\"] input').value") != text:
                    raise AssertionError("field text was not entered")

            def click_add():
                box = evaluate("(() => { const r = document.querySelector('[data-goya-id=\"window.add\"] button').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()")
                for kind in ("mousePressed", "mouseReleased"):
                    call("Input.dispatchMouseEvent", {"type": kind, "x": box[0], "y": box[1], "button": "left", "clickCount": 1})

            def last_event(expected_count):
                events = evaluate("window.__events")
                if len(events) != expected_count:
                    raise AssertionError(f"expected {expected_count} transition events, saw {events!r}")
                return events[-1]

            def expect(event, ok, revision, count=None, code=None):
                if event["ok"] is not ok or event["revision"] != revision or event.get("code") != code:
                    raise AssertionError(f"unexpected outcome {event!r}")
                if count is not None and event["state"] != {"count": ["int", count]}:
                    raise AssertionError(f"unexpected state {event!r}")

            call("Runtime.enable")
            call("Log.enable")
            call("Page.enable")
            call("Accessibility.enable")
            url = f"http://127.0.0.1:{server.server_port}/index.html"
            load(lambda: call("Page.navigate", {"url": url}))

            # Geometry: every element's DOM box equals the layout mirror's
            # rect for this viewport (the mirror is diff-tested against
            # goya_lang::layout under Node). Chrome lays out in 1/64 px.
            mismatches = evaluate("""(async () => {
                const { CELLS } = await import('./scene.js');
                const { resolve } = await import('./layout.js');
                const vw = document.documentElement.clientWidth, vh = document.documentElement.clientHeight;
                const out = [];
                resolve(CELLS, vw, vh).forEach(({ rect }, i) => {
                  const r = document.querySelector(`[data-goya-id="${CELLS[i].id}"]`).getBoundingClientRect();
                  const want = [rect.x, rect.y, rect.width, rect.height].map((v) => v / 1000);
                  const got = [r.x, r.y, r.width, r.height];
                  if (want.some((v, j) => Math.abs(v - got[j]) > 1 / 64)) out.push({ id: CELLS[i].id, want, got });
                });
                return { vw, vh, out };
            })()""", await_promise=True)
            if mismatches["out"] or mismatches["vw"] <= 0 or mismatches["vh"] <= 0:
                raise AssertionError(f"geometry mismatch: {mismatches!r}")
            passed.append(f"geometry of 4 elements at {mismatches['vw']}x{mismatches['vh']} CSS px")

            # Keyboard: Tab to the field, type, Tab to the button, Enter.
            evaluate("document.activeElement.blur(); 0")
            key("Tab", "Tab", 9)
            if evaluate("document.activeElement.closest('[data-goya-id]').dataset.goyaId") != "window.amount":
                raise AssertionError("first Tab did not focus the field")
            call("Input.insertText", {"text": "5"})
            key("Tab", "Tab", 9)
            if evaluate("document.activeElement.closest('[data-goya-id]').dataset.goyaId") != "window.add":
                raise AssertionError("second Tab did not focus the button")
            key("Enter", "Enter", 13, "\r")
            expect(last_event(1), True, "1", "5")
            passed.append("Tab order field -> button; Enter activates: count 0 + 5 = 5, revision 1")

            # Pointer.
            set_field("-2")
            click_add()
            expect(last_event(2), True, "2", "3")
            passed.append("pointer click: 5 + -2 = 3, revision 2")

            # Conversion failure: no dispatch, field marked invalid.
            for text in ("x", "+1", "01", " 1", ""):
                set_field(text)
                click_add()
            events = evaluate("window.__events")[2:]
            if len(events) != 5 or any(e["ok"] or e["code"] != "NOT_CANONICAL_INT" or e["field"] != "window.amount" or e["revision"] != "2" for e in events):
                raise AssertionError(f"conversion failures: {events!r}")
            if evaluate("document.querySelector('[data-goya-id=\"window.amount\"] input').getAttribute('aria-invalid')") != "true":
                raise AssertionError("invalid field is not marked aria-invalid")
            passed.append("non-canonical field text ('x', '+1', '01', ' 1', '') refused before dispatch; revision stays 2")

            # Arithmetic overflow: D1 failure, state and revision unchanged.
            set_field(I64_MAX)
            if evaluate("document.querySelector('[data-goya-id=\"window.amount\"] input').getAttribute('aria-invalid')") is not None:
                raise AssertionError("editing the field did not clear aria-invalid")
            click_add()
            expect(last_event(8), False, "2", "3", "ARITHMETIC_OVERFLOW")
            if evaluate("document.getElementById('goya-status').textContent") != "Action failed: ARITHMETIC_OVERFLOW":
                raise AssertionError("D1 failure was not reported in the status region")
            passed.append("3 + i64::MAX refused with ARITHMETIC_OVERFLOW; count 3, revision 2 unchanged; reported in role=status")

            # Space on the focused button; recovery after a failure.
            set_field("0")
            evaluate("document.querySelector('[data-goya-id=\"window.add\"] button').focus(); 0")
            key(" ", "Space", 32, " ")
            expect(last_event(9), True, "3", "3")
            if evaluate("document.getElementById('goya-status').textContent") != "":
                raise AssertionError("status was not cleared after success")
            passed.append("Space activates: 3 + 0 = 3, revision 3; status cleared")

            # Accessibility tree (Chrome's, not an assistive technology).
            nodes = call("Accessibility.getFullAXTree")["nodes"]
            roles = [(n.get("role", {}).get("value"), n.get("name", {}).get("value")) for n in nodes if not n.get("ignored")]
            if ("button", "Add") not in roles:
                raise AssertionError(f"no button named Add in AX tree: {roles!r}")
            textboxes = [name for role, name in roles if role == "textbox"]
            if len(textboxes) != 1:
                raise AssertionError(f"expected one textbox: {roles!r}")
            if ("StaticText", "Counter") not in roles:
                raise AssertionError(f"title text missing from AX tree: {roles!r}")
            passed.append(f"AX tree: button 'Add', StaticText 'Counter', one textbox (accessible name: {textboxes[0]!r})")

            # Reload: session state lives for one page load.
            load(lambda: call("Page.reload", {"ignoreCache": True}))
            set_field("1")
            click_add()
            expect(last_event(1), True, "1", "1")
            passed.append("reload restarts session state from the authored initial value: 0 + 1 = 1, revision 1")

            evaluate("0")
            if errors:
                raise AssertionError(f"browser errors: {errors!r}")
            passed.append("no console errors or uncaught exceptions")
            version = call("Browser.getVersion")["product"]
            for line in passed:
                print("PASS:", line)
            print("BROWSER:", version)
            print("NOT TESTED: other browsers, assistive technology, IME composition, touch, visual typography, durable state")
    finally:
        if sock:
            sock.close()
        if chrome:
            chrome.terminate()
            try:
                chrome.wait(timeout=5)
            except subprocess.TimeoutExpired:
                chrome.kill()
                chrome.wait()
        server.shutdown()
        server.server_close()
        thread.join()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: chrome_counter.py OUTPUT_DIRECTORY")
    main(sys.argv[1])
