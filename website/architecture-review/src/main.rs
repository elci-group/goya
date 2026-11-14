//! Loopback server for the Goya architecture review.
//!
//! Bind address defaults to 127.0.0.1:4187. The process serves the page and
//! the JSON model from memory. It has no third-party dependencies.

mod model;

use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

const INDEX: &str = include_str!("../public/index.html");
const CSS: &str = include_str!("../public/review.css");
const JS: &str = include_str!("../public/review.js");
const FAVICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect fill="#17140f" width="32" height="32"/><path stroke="#9ad8c8" stroke-width="2" d="M4 22h24"/><path stroke="#e4b56a" stroke-width="1.5" d="M8 22v-8M16 22V8M24 22v-5"/></svg>"#;

struct Response {
    status: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

fn main() {
    let mut args = env::args().skip(1);
    let mut bind = "127.0.0.1".to_string();
    let mut port: u16 = 4187;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--self-check" => {
                self_check();
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
                println!("goya-architecture-review [--bind ADDR] [--port PORT] [--self-check]");
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
    eprintln!("Goya architecture review at http://{bind}:{port}/");
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
    eprintln!("goya-architecture-review [--bind ADDR] [--port PORT] [--self-check]");
    std::process::exit(2);
}

fn self_check() {
    let dangling = model::dangling_edges();
    println!(
        "crates {} edges {} waves {} traces {} dangling {}",
        model::crates().len(),
        model::edges().len(),
        model::waves().len(),
        model::traces().len(),
        dangling.len()
    );
    if !dangling.is_empty() {
        std::process::exit(1);
    }
    let model = model::model_json();
    if !model.starts_with('{') || !model.ends_with('}') {
        std::process::exit(1);
    }
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
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path.contains("..") || path.contains('\\') {
        write_response(&mut stream, method, not_found());
        return;
    }
    write_response(&mut stream, method, route(method, path, query));
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

fn route(method: &str, path: &str, query: &str) -> Response {
    if method != "GET" && method != "HEAD" {
        return Response {
            status: "405 Method Not Allowed",
            content_type: "text/plain; charset=utf-8",
            body: b"method not allowed\n".to_vec(),
        };
    }
    match path {
        "/" | "/index.html" => html(INDEX),
        "/review.css" => text("text/css; charset=utf-8", CSS),
        "/review.js" => text("text/javascript; charset=utf-8", JS),
        "/favicon.svg" => text("image/svg+xml", FAVICON),
        "/api/model" => text("application/json", &model::model_json()),
        "/api/trace" => trace_response(query),
        _ => not_found(),
    }
}

fn trace_response(query: &str) -> Response {
    let event = query_value(query, "event");
    match event {
        None => text("application/json", &model::trace_catalog_json()),
        Some(id) => match model::trace_json(id) {
            Some(body) => text("application/json", &body),
            None => Response {
                status: "404 Not Found",
                content_type: "application/json",
                body: br#"{"error":"unknown event"}"#.to_vec(),
            },
        },
    }
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key && !value.is_empty()).then_some(value)
    })
}

fn html(body: &str) -> Response {
    text("text/html; charset=utf-8", body)
}

fn text(content_type: &'static str, body: &str) -> Response {
    Response {
        status: "200 OK",
        content_type,
        body: body.as_bytes().to_vec(),
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
    fn page_and_model_routes() {
        let page = route("GET", "/", "");
        assert_eq!(page.status, "200 OK");
        let body = String::from_utf8(page.body).unwrap();
        assert!(body.contains("Architecture review"));
        let model = String::from_utf8(route("GET", "/api/model", "").body).unwrap();
        assert!(model.contains("goya-runtime"));
        assert!(model.contains("goya-vee"));
    }

    #[test]
    fn scroll_trace_and_unknown_event() {
        let scroll = String::from_utf8(route("GET", "/api/trace", "event=scroll").body).unwrap();
        assert!(scroll.contains("\"revision\":\"unchanged\""));
        let missing = route("GET", "/api/trace", "event=nope");
        assert_eq!(missing.status, "404 Not Found");
        let catalog = String::from_utf8(route("GET", "/api/trace", "").body).unwrap();
        assert!(catalog.contains("pointer-button"));
    }

    #[test]
    fn rejects_post_and_escape() {
        assert_eq!(route("POST", "/", "").status, "405 Method Not Allowed");
        assert_eq!(route("GET", "/../Cargo.toml", "").status, "404 Not Found");
    }
}
