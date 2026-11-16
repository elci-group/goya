"""Manual Chrome HTTPS adapter latency benchmark on loopback only."""

import http.server
import json
import os
from pathlib import Path
import signal
import ssl
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


def main(source):
    payload = bytes(index & 0xff for index in range(256))
    requests = 0
    request_lock = threading.Lock()
    benchmark = Path(__file__).with_suffix(".js").read_text()
    html = (
        "<!doctype html><body>pending<script type=module>\ntry {\n"
        + "const sources = " + json.dumps([source]) + ";\n"
        + benchmark
        + "\n} catch(error) {document.body.textContent='FAIL: '+error.stack;}"
        + "</script></body>"
    ).encode()

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_GET(self):
            nonlocal requests
            if self.path == "/":
                body = html
                content_type = "text/html; charset=utf-8"
            elif self.path == "/benchmark":
                with request_lock:
                    requests += 1
                body = payload
                content_type = "application/octet-stream"
            else:
                body = b"not found"
                content_type = "text/plain"
            self.send_response(200 if self.path in ("/", "/benchmark") else 404)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError, ssl.SSLError):
                pass

    with tempfile.TemporaryDirectory(
        prefix="caprichios-network-bench-", ignore_cleanup_errors=True
    ) as tmp:
        tmp_path = Path(tmp)
        cert, key = tmp_path / "cert.pem", tmp_path / "key.pem"
        subprocess.run(["bank", str(cert), str(key)], check=True, stdout=subprocess.DEVNULL)
        subprocess.run(
            [
                "openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                "-subj", "/CN=127.0.0.1", "-keyout", str(key), "-out", str(cert),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(cert, key)
        server.socket = context.wrap_socket(server.socket, server_side=True)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        chrome = None
        sock = None
        try:
            profile = tmp_path / "profile"
            subprocess.run(["bank", "-d", str(profile)], check=True, stdout=subprocess.DEVNULL)
            chrome = subprocess.Popen(
                [
                    os.environ.get("MAJA_CHROME", "google-chrome"),
                    "--headless", "--disable-gpu", "--no-first-run",
                    "--disable-background-networking", "--disable-component-update",
                    "--ignore-certificate-errors", "--remote-debugging-port=0",
                    "--remote-allow-origins=*", "--user-data-dir=" + str(profile), "about:blank",
                ],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            port_file = profile / "DevToolsActivePort"
            deadline = time.monotonic() + 15
            while not port_file.exists():
                if chrome.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("Chrome did not expose DevTools")
                time.sleep(0.05)
            port = int(port_file.read_text().splitlines()[0])
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/json", timeout=5) as response:
                page = next(item for item in json.load(response) if item["type"] == "page")
            sock = websocket.create_connection(
                page["webSocketDebuggerUrl"], timeout=10, http_proxy_host=None
            )
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

            call("Page.navigate", {"url": f"https://127.0.0.1:{server.server_port}/"})
            deadline = time.monotonic() + 30
            while True:
                result = call(
                    "Runtime.evaluate",
                    {"expression": "document.body && document.body.textContent", "returnByValue": True},
                )
                value = result["result"].get("value")
                if value and value.startswith("BENCHMARK:"):
                    report = json.loads(value[len("BENCHMARK:"):])
                    expected = 10 + 200
                    with request_lock:
                        actual = requests
                    if actual != expected:
                        raise AssertionError(f"expected {expected} endpoint requests, observed {actual}")
                    report["server_observed_requests"] = actual
                    report["certificate_trust_bypass"] = "only this isolated benchmark Chrome process"
                    print("Local HTTPS adapter benchmark; no external endpoint used; no speed thresholds")
                    print(json.dumps(report, indent=2, sort_keys=True))
                    break
                if value and value.startswith("FAIL:"):
                    raise AssertionError(value)
                if time.monotonic() >= deadline:
                    raise TimeoutError(f"network benchmark did not finish: {value!r}")
                time.sleep(0.05)

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
    length = int(sys.stdin.buffer.readline())
    source = sys.stdin.buffer.read(length).decode()
    main(source)
