use goya_sdk::{SceneProject, Viewport};
use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

const TEMPLATE: &str = "@ontology my_app {\n    @cell root {\n        position: (0, 0, 0)\n        extent: (800, 600, 0)\n        fill: #20202a\n\n        @button hello {\n            position: (40, 40, 0)\n            extent: (160, 48, 0)\n            text: \"Hello, Goya\"\n            fill: #3267c8\n        }\n    }\n}\n";

fn usage() {
    println!(
        "goya-studio {}\n\nUsage:\n  goya-studio new <file.gscene>\n  goya-studio check <file.gscene>\n  goya-studio inspect <file.gscene>\n  goya-studio edit <file.gscene> [editor]\n  goya-studio preview <file.gscene> [--live] [scene-binary]\n  goya-studio serve <file.gscene> [port]\n",
        env!("CARGO_PKG_VERSION")
    );
}

const MAX_REQUEST_BYTES: usize = 8 * 1024;
const MAX_SCENE_BYTES: u64 = 128 * 1024;
const MAX_PREVIEW_NODES: usize = 512;

#[derive(Debug, PartialEq, Eq)]
struct HttpRequest {
    method: String,
    target: String,
}

fn parse_request(bytes: &[u8]) -> Result<HttpRequest, &'static str> {
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err("request too large");
    }
    let end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("incomplete request headers")?;
    let header = std::str::from_utf8(&bytes[..end]).map_err(|_| "invalid request headers")?;
    let mut lines = header.split("\r\n");
    let mut request = lines
        .next()
        .ok_or("missing request line")?
        .split_ascii_whitespace();
    let method = request.next().ok_or("missing method")?;
    let target = request.next().ok_or("missing target")?;
    let version = request.next().ok_or("missing HTTP version")?;
    if request.next().is_some() || !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return Err("invalid request line");
    }
    if lines.count() > 16 {
        return Err("too many headers");
    }
    if bytes[end + 4..]
        .iter()
        .any(|byte| !byte.is_ascii_whitespace())
    {
        return Err("request bodies are not supported");
    }
    Ok(HttpRequest {
        method: method.to_owned(),
        target: target.to_owned(),
    })
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn render_html(project: &SceneProject) -> String {
    let viewport = project.viewport();
    let mut html = format!("<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>body{{margin:0;background:#111;color:#eee;font:14px system-ui,sans-serif}}header{{padding:12px 18px}}main{{padding:18px;overflow:auto}}.stage{{position:relative;margin:auto;background:#20202a;box-shadow:0 4px 24px #0008}}.node{{position:absolute;box-sizing:border-box;overflow:hidden;white-space:pre-wrap;padding:4px;border:1px solid #ffffff30;border-radius:3px}}.button{{display:flex;align-items:center;justify-content:center;font:inherit}}</style></head><body><header>Goya Studio preview · {}</header><main><div class=\"stage\" style=\"width:{}px;height:{}px\">", escape_html(&project.scene().name), escape_html(&project.scene().name), viewport.width_points, viewport.height_points);
    for node in project.frame().nodes {
        let color = node
            .fill
            .map(|color| format!("#{:06x}", color & 0x00ff_ffff))
            .unwrap_or_else(|| "#353545".into());
        let label = node.text.as_deref().unwrap_or(&node.name);
        let role = if node.is_button { "button" } else { "group" };
        html.push_str(&format!("<div class=\"node{}\" role=\"{}\" aria-label=\"{}\" style=\"left:{}px;top:{}px;width:{}px;height:{}px;background:{};{}\">{}</div>",
            if node.is_button { " button" } else { "" }, role, escape_html(label), node.rect.x as f64 / 1000.0,
            node.rect.y as f64 / 1000.0, node.rect.width as f64 / 1000.0, node.rect.height as f64 / 1000.0,
            color, node.text_size.map(|size| format!("font-size:{}px;", size)).unwrap_or_default(), escape_html(label)));
    }
    html.push_str("</div></main></body></html>");
    html
}

fn route_request(request: &[u8], html: &str) -> (&'static str, &'static str, String) {
    match parse_request(request) {
        Err(_) => (
            "400 Bad Request",
            "text/plain; charset=utf-8",
            "Bad Request".to_owned(),
        ),
        Ok(req) if req.method != "GET" => (
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            "Method Not Allowed".to_owned(),
        ),
        Ok(req) if req.target != "/" => (
            "404 Not Found",
            "text/plain; charset=utf-8",
            "Not Found".to_owned(),
        ),
        Ok(_) => ("200 OK", "text/html; charset=utf-8", html.to_owned()),
    }
}

fn respond(stream: &mut TcpStream, request: &[u8], html: &str) -> std::io::Result<()> {
    let (status, content_type, body) = route_request(request, html);
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\n\r\n{}", body.len(), body)
}

fn serve(path: &Path, port: u16) -> Result<(), String> {
    let mut source = Vec::with_capacity(MAX_SCENE_BYTES as usize + 1);
    File::open(path)
        .map_err(|error| format!("cannot open scene: {error}"))?
        .take(MAX_SCENE_BYTES + 1)
        .read_to_end(&mut source)
        .map_err(|error| format!("cannot read scene: {error}"))?;
    if source.len() as u64 > MAX_SCENE_BYTES {
        return Err(format!(
            "scene exceeds the {} KiB preview limit",
            MAX_SCENE_BYTES / 1024
        ));
    }
    let source =
        String::from_utf8(source).map_err(|error| format!("scene is not UTF-8: {error}"))?;
    let scene = SceneProject::from_source(source, Viewport::default())
        .map_err(|error| error.to_string())?;
    if scene.frame().nodes.len() > MAX_PREVIEW_NODES {
        return Err(format!(
            "scene exceeds the {MAX_PREVIEW_NODES}-node preview limit"
        ));
    }
    let html = render_html(&scene);
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("cannot bind local preview: {error}"))?;
    println!(
        "Studio preview: http://{} (Ctrl-C to stop)",
        listener.local_addr().map_err(|error| error.to_string())?
    );
    for incoming in listener.incoming() {
        let Ok(mut stream) = incoming else { continue };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
        let mut request = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    request.extend_from_slice(&chunk[..count]);
                    if request.len() > MAX_REQUEST_BYTES
                        || request.windows(4).any(|window| window == b"\r\n\r\n")
                    {
                        break;
                    }
                }
            }
        }
        let _ = respond(&mut stream, &request, &html);
    }
    Ok(())
}

fn project(path: &Path) -> Result<SceneProject, String> {
    SceneProject::from_path(path, Viewport::default()).map_err(|error| error.to_string())
}

fn check(path: &Path) -> Result<(), String> {
    let project = project(path)?;
    println!(
        "valid: ontology={} scene_nodes={} ir_nodes={} viewport={}x{}pt",
        project.scene().name,
        project.frame().nodes.len(),
        project.module().nodes.len(),
        project.viewport().width_points,
        project.viewport().height_points
    );
    Ok(())
}

fn inspect(path: &Path) -> Result<(), String> {
    let project = project(path)?;
    println!("Scene: {}", project.scene().name);
    let frame = project.frame();
    for node in &frame.nodes {
        println!(
            "{} {} x={} y={} w={} h={} text={:?}",
            if node.is_button { "button" } else { "cell" },
            node.name,
            node.rect.x,
            node.rect.y,
            node.rect.width,
            node.rect.height,
            node.text
        );
    }
    Ok(())
}

fn edit(path: &Path, editor: Option<&str>) -> Result<(), String> {
    if !path.exists() {
        std::fs::write(path, TEMPLATE).map_err(|error| format!("cannot create scene: {error}"))?;
    }
    let editor = editor
        .map(str::to_owned)
        .or_else(|| std::env::var("VISUAL").ok())
        .or_else(|| std::env::var("EDITOR").ok())
        .ok_or_else(|| "set VISUAL/EDITOR or pass the editor executable".to_string())?;
    let status = Command::new(&editor)
        .arg(path)
        .status()
        .map_err(|error| format!("cannot launch editor '{editor}': {error}"))?;
    if !status.success() {
        return Err(format!("editor exited with {status}"));
    }
    check(path)
}

fn preview(path: &Path, live: bool, scene_binary: Option<&str>) -> Result<(), String> {
    // Validate before preview launch so malformed edits never replace the
    // user's last known-good running scene.
    check(path)?;
    if live {
        let binary = scene_binary.unwrap_or("goya-scene");
        let status = Command::new(binary)
            .arg("--file")
            .arg(path)
            .arg("--live")
            .status()
            .map_err(|error| format!("cannot launch scene preview '{binary}': {error}"))?;
        if !status.success() {
            return Err(format!("scene preview exited with {status}"));
        }
    } else {
        inspect(path)?;
    }
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        usage();
        return Ok(());
    };
    match command {
        "--help" | "-h" | "help" => usage(),
        "new" => {
            let path = args.get(1).ok_or_else(|| "new needs a path".to_string())?;
            let path = PathBuf::from(path);
            if path.exists() {
                return Err(format!("refusing to overwrite {}", path.display()));
            }
            std::fs::write(&path, TEMPLATE).map_err(|error| error.to_string())?;
            check(&path)?;
            println!("created {}", path.display());
        }
        "check" => {
            let path = args
                .get(1)
                .ok_or_else(|| "check needs a path".to_string())?;
            check(Path::new(path))?;
        }
        "inspect" => {
            let path = args
                .get(1)
                .ok_or_else(|| "inspect needs a path".to_string())?;
            inspect(Path::new(path))?;
        }
        "edit" => {
            let path = args.get(1).ok_or_else(|| "edit needs a path".to_string())?;
            edit(Path::new(path), args.get(2).map(String::as_str))?;
        }
        "preview" => {
            let path = args
                .get(1)
                .ok_or_else(|| "preview needs a path".to_string())?;
            let live = args.iter().any(|arg| arg == "--live");
            let binary = args
                .iter()
                .skip(2)
                .find(|arg| arg.as_str() != "--live")
                .map(String::as_str);
            preview(Path::new(path), live, binary)?;
        }
        "serve" => {
            let path = args
                .get(1)
                .ok_or_else(|| "serve needs a path".to_string())?;
            let port = args
                .get(2)
                .map(|value| {
                    value
                        .parse::<u16>()
                        .map_err(|_| "port must be an integer from 0 to 65535".to_string())
                })
                .transpose()?
                .unwrap_or(8765);
            serve(Path::new(path), port)?;
        }
        other => return Err(format!("unknown command: {other}")),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("goya-studio: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_prints_usage_without_error() {
        assert!(run(&[]).is_ok());
    }

    #[test]
    fn invalid_command_is_rejected() {
        assert!(run(&["publish".into()]).is_err());
    }

    #[test]
    fn request_parser_accepts_only_bounded_header_only_http_requests() {
        let parsed = parse_request(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        assert_eq!(
            parsed,
            HttpRequest {
                method: "GET".into(),
                target: "/".into()
            }
        );
        assert_eq!(
            parse_request(b"GET / HTTP/1.1\r\n\r\nbody"),
            Err("request bodies are not supported")
        );
        assert_eq!(
            parse_request(&vec![b'x'; MAX_REQUEST_BYTES + 1]),
            Err("request too large")
        );
    }

    #[test]
    fn preview_routes_and_escapes_scene_content() {
        let project = SceneProject::from_source("@ontology demo { @button ok { position: (0, 0, 0) extent: (80, 40, 0) text: \"<script>\" } }", Viewport::default()).unwrap();
        let html = render_html(&project);
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
        assert_eq!(route_request(b"GET / HTTP/1.1\r\n\r\n", &html).0, "200 OK");
        assert_eq!(
            route_request(b"GET /other HTTP/1.1\r\n\r\n", &html).0,
            "404 Not Found"
        );
        assert_eq!(
            route_request(b"POST / HTTP/1.1\r\n\r\n", &html).0,
            "405 Method Not Allowed"
        );
        assert_eq!(route_request(b"broken", &html).0, "400 Bad Request");
    }
}
