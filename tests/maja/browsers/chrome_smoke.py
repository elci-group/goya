#!/usr/bin/env python3
"""Serve an existing output and exercise it in real headless Chrome via CDP.

Usage: python3 tests/maja/browsers/chrome_smoke.py OUTPUT_DIRECTORY EXPECTED_TEXT
Requires Chrome and the Python websocket-client package. Failure is nonzero.
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


def main(output, expected_text):
    output = Path(output).resolve(strict=True)
    if not (output / "index.html").is_file():
        raise RuntimeError("output does not contain index.html")
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(output))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    chrome = None
    sock = None
    try:
        with tempfile.TemporaryDirectory(prefix="maja-chrome-") as profile:
            chrome = subprocess.Popen([
                os.environ.get("MAJA_CHROME", "google-chrome"), "--headless", "--disable-gpu",
                "--no-first-run", "--no-default-browser-check", "--remote-debugging-port=0",
                "--remote-allow-origins=*", "--user-data-dir=" + profile, "about:blank",
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
                    if message.get("id") == request_id:
                        if "error" in message:
                            raise RuntimeError(str(message["error"]))
                        return message.get("result", {})

            def evaluate(expression):
                result = call("Runtime.evaluate", {"expression": expression, "returnByValue": True})
                if "exceptionDetails" in result:
                    raise RuntimeError(str(result["exceptionDetails"]))
                return result["result"].get("value")

            call("Runtime.enable")
            call("Log.enable")
            call("Page.enable")
            call("Accessibility.enable")
            call("Page.navigate", {"url": f"http://127.0.0.1:{server.server_port}/index.html"})
            deadline = time.monotonic() + 10
            while evaluate("document.readyState") != "complete":
                if time.monotonic() >= deadline:
                    raise RuntimeError("page did not finish loading")
                time.sleep(0.05)
            observed = evaluate("(() => { const e = document.querySelector('[data-goya-id=\"root\"]'); return e && {text:e.textContent, visible:e.getClientRects().length > 0}; })()")
            if not observed or observed["text"] != expected_text or not observed["visible"]:
                raise AssertionError(f"root text or visibility mismatch: {observed!r}")
            nodes = call("Accessibility.getFullAXTree")["nodes"]
            if not any(n.get("name", {}).get("value") == expected_text for n in nodes):
                raise AssertionError("root text missing from browser accessibility tree")
            evaluate("0")
            if errors:
                raise AssertionError(f"browser errors: {errors!r}")
            print("PASS: Chrome loaded output and exposed the visible root text in its accessibility tree")
            print("NOT TESTED: interactions, focus, overflow, reload, assistive technology, or multi-browser compatibility")
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
    if len(sys.argv) != 3:
        raise SystemExit("usage: chrome_smoke.py OUTPUT_DIRECTORY EXPECTED_TEXT")
    main(sys.argv[1], sys.argv[2])
