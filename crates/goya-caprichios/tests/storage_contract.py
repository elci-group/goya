"""Real Chrome IndexedDB checks on loopback, with an isolated browser profile.

This checks best-effort operations, not persistence across host restarts.
"""
import http.server
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

import websocket


def stop_chrome(chrome):
    if chrome is None:
        return
    try:
        os.killpg(chrome.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        chrome.wait(timeout=5)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(chrome.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        chrome.wait()


def main(sources):
    checks = Path(__file__).with_suffix(".js").read_text()
    html = ("<!doctype html><body>pending<script type=module>\ntry {\n"
            + "const sources = " + json.dumps(sources) + ";\n" + checks
            + "\ndocument.body.textContent='PASS';\n} catch(error) {"
            + "document.body.textContent='FAIL: '+error.stack;}</script></body>").encode()

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_GET(self):
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(html)))
            self.end_headers()
            self.wfile.write(html)

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    chrome = None
    sock = None
    try:
        with tempfile.TemporaryDirectory(
            prefix="caprichios-storage-", ignore_cleanup_errors=True
        ) as tmp:
            profile = Path(tmp) / "profile"
            subprocess.run(["bank", "-d", str(profile)], check=True, stdout=subprocess.DEVNULL)
            chrome = subprocess.Popen([
                os.environ.get("MAJA_CHROME", "google-chrome"), "--headless", "--disable-gpu",
                "--no-first-run", "--remote-debugging-port=0", "--remote-allow-origins=*",
                "--user-data-dir=" + str(profile), "about:blank",
            ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
            port_file = profile / "DevToolsActivePort"
            deadline = time.monotonic() + 15
            while not port_file.exists():
                if chrome.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("Chrome did not expose DevTools")
                time.sleep(0.05)
            port = int(port_file.read_text().splitlines()[0])
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/json", timeout=5) as response:
                page = next(item for item in json.load(response) if item["type"] == "page")
            sock = websocket.create_connection(page["webSocketDebuggerUrl"], timeout=10, http_proxy_host=None)
            sequence = 0

            def call(method, params):
                nonlocal sequence
                sequence += 1
                sock.send(json.dumps({"id": sequence, "method": method, "params": params}))
                while True:
                    reply = json.loads(sock.recv())
                    if reply.get("id") == sequence:
                        if "error" in reply:
                            raise RuntimeError(reply["error"])
                        return reply["result"]

            call("Page.navigate", {"url": f"http://127.0.0.1:{server.server_port}/"})
            deadline = time.monotonic() + 15
            while True:
                result = call("Runtime.evaluate", {
                    "expression": "document.body && document.body.textContent", "returnByValue": True,
                })
                value = result["result"].get("value")
                if value == "PASS":
                    break
                if value and value.startswith("FAIL:"):
                    raise AssertionError(value)
                if time.monotonic() >= deadline:
                    raise TimeoutError(f"storage contract did not finish: {value!r}")
                time.sleep(0.05)
            print("PASS: Chrome IndexedDB bytes, missing/empty values, size limit, committed writes, namespace isolation and fresh-module visibility")
            sock.close()
            sock = None
            stop_chrome(chrome)
            chrome = None
    finally:
        if sock:
            sock.close()
        stop_chrome(chrome)
        server.shutdown()
        server.server_close()
        thread.join()


if __name__ == "__main__":
    size = int(sys.stdin.buffer.readline())
    first = sys.stdin.buffer.read(size).decode()
    second = sys.stdin.buffer.read().decode()
    main([first, second])
