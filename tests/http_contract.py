#!/usr/bin/env python3
"""Local Chrome integration probe for the browser HTTP prototype.

Only loopback endpoints are used. The test creates a temporary self-signed
certificate and launches an isolated Chrome profile with
``--ignore-certificate-errors`` solely so that certificate can be exercised.
This is not a certificate-validation test or a browser conformance claim.

Requirements: ``google-chrome`` (or ``GOYA_CHROME``), ``rustc``, ``openssl``,
and Python ``websocket-client``. Run with ``python3 tests/http_contract.py``.
"""

import http.server
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import ssl
import subprocess
import tempfile
import threading
import time
import unittest
import urllib.request

try:
    import websocket
except ImportError:  # pragma: no cover - environment dependent
    websocket = None


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


REPO = Path(__file__).resolve().parents[1]
MODULE = REPO / "crates/goya-caprichios/src/http_caprichio.rs"


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


class ApiHandler(http.server.BaseHTTPRequestHandler):
    page_origin = ""
    observed_cookies = None
    observed_lock = None

    def log_message(self, *_args):
        pass

    def _record(self):
        with self.observed_lock:
            self.observed_cookies.setdefault(self.path, []).append(
                self.headers.get("Cookie", "")
            )

    def _cors(self):
        self.send_header("Access-Control-Allow-Origin", self.page_origin)
        self.send_header("Access-Control-Allow-Credentials", "true")
        self.send_header("Access-Control-Expose-Headers", "X-Visible")
        self.send_header("Vary", "Origin")

    def do_OPTIONS(self):
        self._record()
        self.send_response(204)
        self._cors()
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.end_headers()

    def do_GET(self):
        self._record()
        if self.path == "/redirect":
            self.send_response(302)
            self._cors()
            self.send_header("Location", "/visible")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if self.path != "/visible":
            self.send_error(404)
            return
        self.send_response(201)
        self._cors()
        self.send_header("X-Visible", "shown")
        self.send_header("X-Hidden", "secret")
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_POST(self):
        self._record()
        if self.path != "/post":
            self.send_error(404)
            return
        size = int(self.headers.get("Content-Length", "0"))
        if size:
            self.rfile.read(size)
        self.send_response(202)
        self._cors()
        self.send_header("X-Visible", "posted")
        self.send_header("X-Hidden", "secret-post")
        self.send_header("Content-Length", "0")
        self.end_headers()


class PageHandler(http.server.BaseHTTPRequestHandler):
    get_source = ""
    post_source = ""
    redirect_source = ""
    api_url = ""

    def log_message(self, *_args):
        pass

    def do_GET(self):
        if self.path == "/get.js":
            self._send(200, self.get_source, "text/javascript")
        elif self.path == "/post.js":
            self._send(200, self.post_source, "text/javascript")
        elif self.path == "/redirect.js":
            self._send(200, self.redirect_source, "text/javascript")
        elif self.path == "/":
            html = f"""<!doctype html><meta charset=utf-8>
<script type=module>
import {{request as generatedGet}} from './get.js';
import {{request as generatedPost}} from './post.js';
import {{request as generatedRedirect}} from './redirect.js';
async function run() {{
  const checks = {{}};
  try {{
    document.cookie = 'probe=do-not-send; Secure; SameSite=None; Path=/';
    const credentialControl = await fetch('{self.api_url}/visible', {{
      mode: 'cors', credentials: 'include'
    }});
    checks.controlStatus = credentialControl.status;
    const response = await generatedGet();
    checks.getStatus = response.status;
    checks.getHeaders = Object.fromEntries(response.headers);
    const post = await generatedPost();
    checks.postStatus = post.status;
    checks.postHeaders = Object.fromEntries(post.headers);
    try {{ await generatedRedirect(); checks.redirectRejected = false; }}
    catch (_) {{ checks.redirectRejected = true; }}
    checks.cookie = document.cookie;
    document.documentElement.dataset.result = JSON.stringify(checks);
    document.body.textContent = 'complete';
  }} catch (error) {{
    document.documentElement.dataset.error = String(error && error.stack || error);
  }}
}}
run();
</script><body>starting</body>"""
            self._send(200, html, "text/html; charset=utf-8")
        else:
            self.send_error(404)

    def _send(self, status, content, content_type):
        body = content.encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def make_factory_binary(tempdir):
    binary = Path(tempdir) / "http-factory"
    source_path = str(MODULE).replace('"', '\\"')
    helper = f'''#[path = "{source_path}"]
mod http_caprichio;
use http_caprichio::{{CredentialPolicy, HttpDeclaration, Method, RedirectPolicy, RuntimeCapabilities, Target}};
fn emit(method: Method, target: String) {{
    let recipe = HttpDeclaration {{ method, target, redirect: RedirectPolicy::Error,
        credentials: CredentialPolicy::Omit }}.normalize().unwrap();
    let caps = RuntimeCapabilities {{ fetch: true, url: true, response_headers: true }};
    print!("{{}}", recipe.lower_checked(Target::Browser, caps).unwrap().source);
}}
fn main() {{
    let port = std::env::args().nth(1).unwrap();
    println!("<GET>"); emit(Method::Get, format!("https://127.0.0.1:{{port}}/visible"));
    println!("<POST>"); emit(Method::Post, format!("https://127.0.0.1:{{port}}/post"));
    println!("<REDIRECT>"); emit(Method::Get, format!("https://127.0.0.1:{{port}}/redirect"));
}}
'''
    compiled = subprocess.run(
        ["rustc", "--edition=2021", "--crate-name", "http_contract_probe", "-o", str(binary), "-"],
        input=helper,
        text=True,
        capture_output=True,
        check=False,
    )
    if compiled.returncode:
        raise AssertionError(f"could not compile constructor helper:\n{compiled.stderr}")
    return binary


def generated_modules(binary, api_port):
    emitted = subprocess.run(
        [str(binary), str(api_port)], text=True, capture_output=True, check=True
    ).stdout
    sections = emitted.split("<GET>\n", 1)[1].split("<POST>\n", 1)
    get_source = sections[0].split("<POST>\n", 1)[0]
    post_and_redirect = sections[1].split("<REDIRECT>\n", 1)
    return get_source, post_and_redirect[0], post_and_redirect[1]


class HttpContractChromeTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        chrome = os.environ.get("GOYA_CHROME", "google-chrome")
        if not shutil.which(chrome):
            raise unittest.SkipTest(f"Chrome executable unavailable: {chrome}")
        if websocket is None:
            raise unittest.SkipTest("Python websocket-client is required")
        if not shutil.which("rustc") or not shutil.which("openssl"):
            raise unittest.SkipTest("rustc and openssl are required")

    def test_generated_fetch_get_post_cors_redirect_and_credentials(self):
        with tempfile.TemporaryDirectory(
            prefix="goya-http-contract-", ignore_cleanup_errors=True
        ) as tempdir:
            cert = Path(tempdir) / "cert.pem"
            key = Path(tempdir) / "key.pem"
            openssl = subprocess.run(
                [
                    "openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
                    "-keyout", str(key), "-out", str(cert), "-days", "1",
                    "-subj", "/CN=127.0.0.1",
                    "-addext", "subjectAltName=IP:127.0.0.1",
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(openssl.returncode, 0, openssl.stderr)

            api_port, page_port = free_port(), free_port()
            while page_port == api_port:
                page_port = free_port()
            api_url = f"https://127.0.0.1:{api_port}"
            page_url = f"https://127.0.0.1:{page_port}/"
            lock = threading.Lock()
            observed_cookies = {}
            api_handler = type(
                "BoundApiHandler", (ApiHandler,),
                {"page_origin": f"https://127.0.0.1:{page_port}",
                 "observed_cookies": observed_cookies, "observed_lock": lock},
            )
            api_server = http.server.ThreadingHTTPServer(("127.0.0.1", api_port), api_handler)
            api_tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            api_tls.load_cert_chain(cert, key)
            api_server.socket = api_tls.wrap_socket(api_server.socket, server_side=True)

            factory = make_factory_binary(tempdir)
            get_source, post_source, redirect_source = generated_modules(factory, api_port)
            page_handler = type(
                "BoundPageHandler", (PageHandler,),
                {"get_source": get_source, "post_source": post_source,
                 "redirect_source": redirect_source, "api_url": api_url},
            )
            page_server = http.server.ThreadingHTTPServer(("127.0.0.1", page_port), page_handler)
            page_tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            page_tls.load_cert_chain(cert, key)
            page_server.socket = page_tls.wrap_socket(page_server.socket, server_side=True)

            threads = [
                threading.Thread(target=api_server.serve_forever, daemon=True),
                threading.Thread(target=page_server.serve_forever, daemon=True),
            ]
            for thread in threads:
                thread.start()
            chrome = None
            browser_socket = None
            try:
                profile = Path(tempdir) / "chrome-profile"
                chrome = subprocess.Popen(
                    [
                        os.environ.get("GOYA_CHROME", "google-chrome"),
                        "--headless", "--disable-gpu", "--no-first-run",
                        "--no-default-browser-check", "--ignore-certificate-errors",
                        "--remote-debugging-port=0", "--remote-allow-origins=*",
                        "--user-data-dir=" + str(profile), "about:blank",
                    ],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    start_new_session=True,
                )
                port_file = profile / "DevToolsActivePort"
                deadline = time.monotonic() + 20
                while not port_file.exists():
                    if chrome.poll() is not None or time.monotonic() >= deadline:
                        self.fail("Chrome did not expose a DevTools port")
                    time.sleep(0.05)
                devtools_port = int(port_file.read_text().splitlines()[0])
                with urllib.request.urlopen(f"http://127.0.0.1:{devtools_port}/json", timeout=5) as response:
                    page = next(item for item in json.load(response) if item["type"] == "page")
                browser_socket = websocket.create_connection(
                    page["webSocketDebuggerUrl"], timeout=10, http_proxy_host=None
                )
                sequence = 0

                def cdp(method, params=None):
                    nonlocal sequence
                    sequence += 1
                    request_id = sequence
                    browser_socket.send(json.dumps({"id": request_id, "method": method, "params": params or {}}))
                    while True:
                        message = json.loads(browser_socket.recv())
                        if message.get("id") == request_id:
                            if "error" in message:
                                raise AssertionError(message["error"])
                            return message.get("result", {})

                def evaluate(expression):
                    result = cdp("Runtime.evaluate", {"expression": expression, "returnByValue": True})
                    if "exceptionDetails" in result:
                        raise AssertionError(result["exceptionDetails"])
                    return result["result"].get("value")

                cdp("Runtime.enable")
                cdp("Page.enable")
                cdp("Page.navigate", {"url": page_url})
                deadline = time.monotonic() + 20
                result = None
                while time.monotonic() < deadline:
                    error = evaluate("document.documentElement.dataset.error || null")
                    if error:
                        self.fail(f"browser recipe failed: {error}")
                    encoded = evaluate("document.documentElement.dataset.result || null")
                    if encoded:
                        result = json.loads(encoded)
                        break
                    time.sleep(0.05)
                self.assertIsNotNone(result, "browser integration page did not finish")
                self.assertEqual(result["controlStatus"], 201)
                self.assertEqual(result["getStatus"], 201)
                self.assertEqual(result["getHeaders"].get("x-visible"), "shown")
                self.assertNotIn("x-hidden", result["getHeaders"])
                self.assertEqual(result["postStatus"], 202)
                self.assertEqual(result["postHeaders"].get("x-visible"), "posted")
                self.assertNotIn("x-hidden", result["postHeaders"])
                self.assertTrue(result["redirectRejected"])

                with lock:
                    visible_requests = list(observed_cookies.get("/visible", []))
                    post_requests = list(observed_cookies.get("/post", []))
                self.assertGreaterEqual(len(visible_requests), 2)
                self.assertIn("probe=do-not-send", visible_requests[0], "include control proves cookie exists")
                self.assertTrue(
                    all("probe=do-not-send" not in cookie for cookie in visible_requests[1:]),
                    f"generated credentials:omit request leaked cookie: {visible_requests!r}",
                )
                self.assertTrue(post_requests, "generated POST did not reach local server")
                print(
                    "PASS: generated browser GET/POST report status and CORS-exposed headers; "
                    "hidden header is absent; redirect errors; credentials are omitted."
                )
                print(
                    "QUALIFICATION: local self-signed certificate was bypassed with "
                    "--ignore-certificate-errors; TLS trust validation was not tested."
                )
            finally:
                if browser_socket:
                    browser_socket.close()
                stop_chrome(chrome)
                for server in (page_server, api_server):
                    server.shutdown()
                    server.server_close()
                for thread in threads:
                    thread.join(timeout=5)


if __name__ == "__main__":
    unittest.main(verbosity=2)
