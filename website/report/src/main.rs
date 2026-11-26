//! Loopback server for the plain-language Goya report.
//!
//! Bind address defaults to 127.0.0.1:4188. The page is compiled into the
//! binary. There are no third-party dependencies.

use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

const INDEX: &str = include_str!("../public/index.html");
const CSS: &str = include_str!("../public/report.css");
const FAVICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" fill="#1a1714"/><path d="M7 11h18M7 16h13M7 21h8" stroke="#f3f0e8" stroke-width="1.7"/></svg>"##;
const FONT_DISPLAY: &[u8] = include_bytes!("../public/fonts/FiraSansCompressed-ExtraBold.otf");
const FONT_COMPRESSED: &[u8] = include_bytes!("../public/fonts/FiraSansCompressed-Medium.otf");
const FONT_BOOK: &[u8] = include_bytes!("../public/fonts/FiraSans-Book.otf");
const FONT_MEDIUM: &[u8] = include_bytes!("../public/fonts/FiraSans-Medium.otf");
const FONT_ITALIC: &[u8] = include_bytes!("../public/fonts/FiraSans-BookItalic.otf");

const SECTIONS: [&str; 6] = [
    "What Goya is",
    "How a typical frontend is put together",
    "The promises Goya is built to keep",
    "What becomes possible",
    "A plain map of the parts",
    "What is still ahead",
];

struct Response {
    status: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

fn main() {
    let mut args = env::args().skip(1);
    let mut bind = "127.0.0.1".to_string();
    let mut port: u16 = 4188;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--self-check" => {
                let missing = missing_sections();
                if !missing.is_empty() {
                    eprintln!("missing sections: {}", missing.join(", "));
                    std::process::exit(1);
                }
                println!("report sections {} css {}", SECTIONS.len(), CSS.len());
                return;
            }
            "--bind" => {
                bind = args.next().unwrap_or_else(|| usage());
            }
            "--port" => {
                port = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| usage());
            }
            "--help" | "-h" => {
                println!("goya-report [--bind ADDR] [--port PORT] [--self-check]");
                return;
            }
            other => {
                eprintln!("unknown argument: {other}");
                usage();
            }
        }
    }

    let listener = TcpListener::bind((bind.as_str(), port)).unwrap_or_else(|err| {
        eprintln!("could not bind {bind}:{port}: {err}");
        std::process::exit(1);
    });
    eprintln!("Goya report at http://{bind}:{port}/");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || handle(stream));
            }
            Err(err) => eprintln!("accept failed: {err}"),
        }
    }
}

fn usage() -> ! {
    eprintln!("goya-report [--bind ADDR] [--port PORT] [--self-check]");
    std::process::exit(2);
}

fn missing_sections() -> Vec<&'static str> {
    SECTIONS
        .into_iter()
        .filter(|title| !INDEX.contains(title))
        .collect()
}

fn handle(mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let Some(headers) = read_headers(&mut stream) else {
        return;
    };
    let Some(start) = headers.lines().next() else {
        return;
    };
    let mut parts = start.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let (path, _) = target.split_once('?').unwrap_or((target, ""));
    if path.contains("..") || path.contains('\\') {
        write_response(&mut stream, method, not_found());
        return;
    }
    write_response(&mut stream, method, route(method, path));
}

fn read_headers(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::with_capacity(512);
    let mut tmp = [0u8; 512];
    loop {
        if buf.len() > 8192 {
            return None;
        }
        if buf.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        match stream.read(&mut tmp) {
            Ok(0) => return None,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(_) => return None,
        }
    }
    String::from_utf8(buf).ok()
}

fn route(method: &str, path: &str) -> Response {
    if method != "GET" && method != "HEAD" {
        return Response {
            status: "405 Method Not Allowed",
            content_type: "text/plain; charset=utf-8",
            body: b"method not allowed\n".to_vec(),
        };
    }
    match path {
        "/" | "/index.html" => html(INDEX),
        "/report.css" => text("text/css; charset=utf-8", CSS),
        "/favicon.svg" => text("image/svg+xml", FAVICON),
        "/fonts/FiraSansCompressed-ExtraBold.otf" => bytes("font/otf", FONT_DISPLAY),
        "/fonts/FiraSansCompressed-Medium.otf" => bytes("font/otf", FONT_COMPRESSED),
        "/fonts/FiraSans-Book.otf" => bytes("font/otf", FONT_BOOK),
        "/fonts/FiraSans-Medium.otf" => bytes("font/otf", FONT_MEDIUM),
        "/fonts/FiraSans-BookItalic.otf" => bytes("font/otf", FONT_ITALIC),
        _ => not_found(),
    }
}

fn html(body: &str) -> Response {
    text("text/html; charset=utf-8", body)
}

fn text(content_type: &'static str, body: &str) -> Response {
    bytes(content_type, body.as_bytes())
}

fn bytes(content_type: &'static str, body: &[u8]) -> Response {
    Response {
        status: "200 OK",
        content_type,
        body: body.to_vec(),
    }
}

fn not_found() -> Response {
    Response {
        status: "404 Not Found",
        content_type: "text/plain; charset=utf-8",
        body: b"not found\n".to_vec(),
    }
}

fn write_response(stream: &mut TcpStream, method: &str, response: Response) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {len}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        status = response.status,
        content_type = response.content_type,
        len = response.body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    if method != "HEAD" {
        let _ = stream.write_all(&response.body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_contains_the_report() {
        assert!(missing_sections().is_empty());
        let page = route("GET", "/");
        assert_eq!(page.status, "200 OK");
        let body = String::from_utf8(page.body).unwrap();
        assert!(body.contains("A report in plain language"));
        assert!(body.contains("No official release has been published"));
        let css = route("GET", "/report.css");
        assert_eq!(css.status, "200 OK");
        let css_body = String::from_utf8(css.body).unwrap();
        assert!(css_body.contains("--ink"));
        assert!(css_body.contains("@font-face"));
        assert_eq!(
            route("GET", "/fonts/FiraSans-Book.otf").status,
            "200 OK"
        );
    }

    #[test]
    fn rejects_post_and_escape() {
        assert_eq!(route("POST", "/").status, "405 Method Not Allowed");
        assert_eq!(route("GET", "/../Cargo.toml").status, "404 Not Found");
        assert_eq!(route("HEAD", "/favicon.svg").status, "200 OK");
    }
}
