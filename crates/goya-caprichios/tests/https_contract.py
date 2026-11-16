"""Chrome integration against a loopback HTTPS server; certificate trust is bypassed."""
import http.server
import json
import os
from pathlib import Path
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

import websocket


def main(source):
    counts = {}

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_GET(self):
            counts[self.path] = counts.get(self.path, 0) + 1
            if self.path == "/":
                body = ("<!doctype html><body>pending<script type=module>\n" + source + r"""
try {
  const ok = await get('/ok');
  if (ok.status !== 200 || new TextDecoder().decode(ok.body) !== 'hello') throw Error('response');
  const missing = await get(location.origin + '/missing');
  if (missing.status !== 404) throw Error('status');
  for (const path of ['/redirect', '/slow']) {
    let rejected = false;
    try { await get(location.origin + path); } catch (_) { rejected = true; }
    if (!rejected) throw Error('accepted ' + path);
  }
  const other = 'https://localhost:' + location.port;
  const cors = await get(other + '/cors');
  if (cors.status !== 200) throw Error('CORS allowed');
  let denied = false;
  try { await get(other + '/no-cors'); } catch (_) { denied = true; }
  if (!denied) throw Error('CORS denied');
  document.body.textContent = 'PASS';
} catch(e) { document.body.textContent = 'FAIL: ' + e; }
</script></body>""").replace("get('/ok')", "get(location.origin + '/ok')").encode()
                self.send_response(200)
                self.send_header("Content-Type", "text/html")
            elif self.path == "/redirect":
                body = b""
                self.send_response(302)
                self.send_header("Location", "/destination")
            else:
                if self.path == "/slow":
                    time.sleep(0.7)
                body = b"hello"
                self.send_response(404 if self.path == "/missing" else 200)
                if self.path == "/cors":
                    self.send_header("Access-Control-Allow-Origin", "*")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ssl.SSLError, ConnectionResetError):
                pass

    with tempfile.TemporaryDirectory(prefix="caprichios-https-") as tmp:
        cert, key = Path(tmp) / "cert.pem", Path(tmp) / "key.pem"
        subprocess.run(["bank", str(cert), str(key)], check=True, stdout=subprocess.DEVNULL)
        subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1", "-subj", "/CN=localhost", "-keyout", str(key), "-out", str(cert)], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(cert, key)
        server.socket = context.wrap_socket(server.socket, server_side=True)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        chrome = None
        sock = None
        try:
            profile = Path(tmp) / "profile"
            subprocess.run(["bank", "-d", str(profile)], check=True, stdout=subprocess.DEVNULL)
            chrome = subprocess.Popen([os.environ.get("MAJA_CHROME", "google-chrome"), "--headless", "--disable-gpu", "--no-first-run", "--ignore-certificate-errors", "--remote-debugging-port=0", "--remote-allow-origins=*", "--user-data-dir=" + str(profile), "about:blank"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
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

            call("Page.navigate", {"url": f"https://127.0.0.1:{server.server_port}/"})
            deadline = time.monotonic() + 10
            while True:
                result = call("Runtime.evaluate", {"expression": "document.body && document.body.textContent", "returnByValue": True})
                value = result["result"].get("value")
                if value == "PASS":
                    break
                if value and value.startswith("FAIL:"):
                    raise AssertionError(value)
                if time.monotonic() > deadline:
                    raise TimeoutError(f"browser test did not finish: {value!r}")
                time.sleep(0.05)
            if counts.get("/destination", 0):
                raise AssertionError("browser followed redirect")
            print("PASS: Chrome HTTPS response, status, redirect, timeout and CORS checks; certificate trust bypassed")

            # Start a fresh Chrome profile with normal certificate validation.
            # Network.loadingFailed distinguishes a certificate rejection from
            # a generic CORS or connection failure in the generated function.
            sock.close()
            sock = None
            chrome.terminate()
            chrome.wait(timeout=5)
            profile = Path(tmp) / "strict-profile"
            subprocess.run(["bank", "-d", str(profile)], check=True, stdout=subprocess.DEVNULL)
            chrome = subprocess.Popen([os.environ.get("MAJA_CHROME", "google-chrome"), "--headless", "--disable-gpu", "--no-first-run", "--remote-debugging-port=0", "--remote-allow-origins=*", "--user-data-dir=" + str(profile), "about:blank"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            port_file = profile / "DevToolsActivePort"
            deadline = time.monotonic() + 15
            while not port_file.exists():
                if chrome.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("strict Chrome did not expose DevTools")
                time.sleep(0.05)
            port = int(port_file.read_text().splitlines()[0])
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/json", timeout=5) as response:
                page = next(item for item in json.load(response) if item["type"] == "page")
            sock = websocket.create_connection(page["webSocketDebuggerUrl"], timeout=10, http_proxy_host=None)
            sock.send(json.dumps({"id": 1, "method": "Network.enable", "params": {}}))
            while json.loads(sock.recv()).get("id") != 1:
                pass
            expression = "(async () => { " + source.replace("export async function get", "async function get") + f"\ntry {{ await get('https://localhost:{server.server_port}/untrusted'); return 'accepted'; }} catch (_) {{ return 'rejected'; }} }})()"
            sock.send(json.dumps({"id": 2, "method": "Runtime.evaluate", "params": {"expression": expression, "awaitPromise": True, "returnByValue": True}}))
            failures = []
            while True:
                reply = json.loads(sock.recv())
                if reply.get("method") == "Network.loadingFailed":
                    failures.append(reply["params"].get("errorText", ""))
                if reply.get("id") == 2:
                    if reply.get("result", {}).get("result", {}).get("value") != "rejected":
                        raise AssertionError(f"untrusted certificate accepted: {reply}")
                    break
            if not any(error.startswith("net::ERR_CERT_") for error in failures):
                raise AssertionError(f"expected certificate error, got {failures}")
            if counts.get("/untrusted", 0):
                raise AssertionError("request reached server despite invalid certificate")
            print("PASS: Chrome rejected the untrusted certificate without a bypass")
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
    main(sys.stdin.read())
