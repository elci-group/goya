//! Fail-closed admission for `dockyard.linux.http/0.0.1`.

use crate::{BODY_LIMIT, CONTENT_TYPE, METHOD, PROFILE, SOURCE_LIMIT, STATUS};

/// Source diagnostic. `line` is 1-based when the fault belongs to one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub line: Option<u32>,
}

/// The only service shape this profile can compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub name: String,
    pub port: u16,
    pub path: String,
    pub body: String,
}

const OPERATIONAL: &[&str] = &[
    "secret",
    "secrets",
    "volume",
    "env",
    "capability",
    "capabilities",
    "kubernetes",
    "health",
    "healthcheck",
    "user",
    "from",
    "copy",
    "image",
    "cmd",
    "entrypoint",
    "platform",
    "root",
    "arg",
    "label",
    "expose",
];

const ORDER: [&str; 4] = ["service", "listen", "route", "respond"];

pub fn admit(source: &str) -> Result<Service, Diagnostic> {
    if source.len() > SOURCE_LIMIT {
        return Err(diagnostic("DK-E0001", "source exceeds 4096 bytes", None));
    }
    if source.bytes().any(|byte| byte == 0 || byte == b'\r') {
        return Err(diagnostic("DK-E0002", "source contains NUL or CR", None));
    }
    if source.starts_with('\u{feff}') {
        return Err(diagnostic(
            "DK-E0002",
            "source contains a leading BOM",
            None,
        ));
    }
    if let Some(marker) = [
        "@ontology",
        "@cell",
        "@button",
        "@input",
        "@interaction",
        "@gesture",
    ]
    .into_iter()
    .find(|marker| source.contains(marker))
    {
        return Err(diagnostic(
            "DK-E0100",
            "presentation constructs are rejected",
            line_of(source, marker),
        ));
    }
    let stripped = source.strip_suffix('\n').unwrap_or(source);
    if stripped.is_empty() || stripped.ends_with('\n') {
        return Err(diagnostic(
            "DK-E0003",
            format!("{PROFILE} requires exactly four lines"),
            None,
        ));
    }
    let lines: Vec<&str> = stripped.split('\n').collect();
    if lines.len() != 4 {
        return Err(diagnostic(
            "DK-E0003",
            format!("{PROFILE} requires exactly four lines"),
            None,
        ));
    }
    let mut service = Service {
        name: String::new(),
        port: 0,
        path: String::new(),
        body: String::new(),
    };
    for (index, line) in lines.iter().enumerate() {
        let line_no = (index + 1) as u32;
        let keyword = first_token(line);
        if OPERATIONAL.contains(&keyword) {
            return Err(diagnostic(
                "DK-E0101",
                "operational constructs are not part of this profile",
                Some(line_no),
            ));
        }
        if keyword != ORDER[index] {
            let code = if ORDER.contains(&keyword) {
                "DK-E0010"
            } else {
                "DK-E0005"
            };
            let message = if code == "DK-E0010" {
                "statements are out of order"
            } else {
                "unknown statement"
            };
            return Err(diagnostic(code, message, Some(line_no)));
        }
        match index {
            0 => service.name = parse_name(line, line_no)?,
            1 => service.port = parse_port(line, line_no)?,
            2 => service.path = parse_route(line, line_no)?,
            3 => service.body = parse_respond(line, line_no)?,
            _ => unreachable!("four lines"),
        }
    }
    Ok(service)
}

fn parse_name(line: &str, line_no: u32) -> Result<String, Diagnostic> {
    let mut parts = strict_split(line, line_no)?;
    if parts.len() != 2 || parts[0] != "service" {
        return Err(diagnostic(
            "DK-E0006",
            "service name is not admitted",
            Some(line_no),
        ));
    }
    let name = parts.remove(1);
    let bytes = name.as_bytes();
    let shaped = (1..=31).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && !name.ends_with('-')
        && !name.contains("--");
    if !shaped {
        return Err(diagnostic(
            "DK-E0006",
            "service name is not admitted",
            Some(line_no),
        ));
    }
    Ok(name.to_string())
}

fn parse_port(line: &str, line_no: u32) -> Result<u16, Diagnostic> {
    let parts = strict_split(line, line_no)?;
    if parts.len() != 2 || parts[0] != "listen" {
        return Err(diagnostic(
            "DK-E0007",
            "port is not admitted",
            Some(line_no),
        ));
    }
    let text = &parts[1];
    if text.len() > 1 && text.starts_with('0') {
        return Err(diagnostic(
            "DK-E0007",
            "port is not admitted",
            Some(line_no),
        ));
    }
    let port = text
        .parse::<u16>()
        .ok()
        .filter(|port| (1024..=65535).contains(port))
        .ok_or_else(|| diagnostic("DK-E0007", "port is not admitted", Some(line_no)))?;
    Ok(port)
}

fn parse_route(line: &str, line_no: u32) -> Result<String, Diagnostic> {
    let parts = strict_split(line, line_no)?;
    if parts.len() != 3 || parts[0] != "route" || parts[1] != METHOD || !valid_path(parts[2]) {
        return Err(diagnostic(
            "DK-E0008",
            "route is not admitted",
            Some(line_no),
        ));
    }
    Ok(parts[2].to_string())
}

fn parse_respond(line: &str, line_no: u32) -> Result<String, Diagnostic> {
    let prefix = format!("respond {STATUS} {CONTENT_TYPE} \"");
    let malformed =
        !line.starts_with(&prefix) || !line.ends_with('"') || line.len() < prefix.len() + 2;
    if malformed {
        return Err(diagnostic(
            "DK-E0009",
            "response is not admitted",
            Some(line_no),
        ));
    }
    let body = &line[prefix.len()..line.len() - 1];
    let bytes = body.as_bytes();
    let allowed = !bytes.is_empty()
        && bytes.len() <= BODY_LIMIT
        && bytes
            .iter()
            .all(|byte| (0x20..0x7f).contains(byte) && *byte != b'"' && *byte != b'\\');
    if !allowed {
        return Err(diagnostic(
            "DK-E0009",
            "response is not admitted",
            Some(line_no),
        ));
    }
    Ok(body.to_string())
}

fn valid_path(path: &str) -> bool {
    if path.len() > 128 || !path.starts_with('/') || path.contains("//") {
        return false;
    }
    if path == "/" {
        return true;
    }
    if path.ends_with('/') {
        return false;
    }
    path[1..].split('/').all(|segment| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && segment.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
            })
    })
}

fn strict_split(line: &str, line_no: u32) -> Result<Vec<&str>, Diagnostic> {
    if line.is_empty() || line.starts_with(' ') || line.ends_with(' ') || line.contains("  ") {
        return Err(diagnostic("DK-E0004", "malformed line", Some(line_no)));
    }
    Ok(line.split(' ').collect())
}

fn first_token(line: &str) -> &str {
    line.split(' ').next().unwrap_or("")
}

fn line_of(source: &str, needle: &str) -> Option<u32> {
    source.find(needle).map(|offset| {
        source[..offset]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count() as u32
            + 1
    })
}

fn diagnostic(code: &'static str, message: impl Into<String>, line: Option<u32>) -> Diagnostic {
    Diagnostic {
        code,
        message: message.into(),
        line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ping() -> &'static str {
        include_str!("../fixtures/ping.gservice")
    }

    #[test]
    fn fixture_is_admitted() {
        let service = admit(ping()).expect("fixture");
        assert_eq!(service.name, "ping");
        assert_eq!(service.port, 39171);
        assert_eq!(service.path, "/health");
        assert_eq!(service.body, "ok");
    }

    #[test]
    fn trailing_newline_is_admitted() {
        let source = ping().strip_suffix('\n').expect("newline").to_string();
        assert_eq!(admit(&source).expect("no trailing newline").name, "ping");
    }

    #[test]
    fn presentation_is_rejected() {
        let source = "@cell {\nid root\n}\n";
        let error = admit(source).expect_err("scene");
        assert_eq!(error.code, "DK-E0100");
        assert_eq!(error.line, Some(1));
    }

    #[test]
    fn secret_volume_and_kubernetes_are_rejected() {
        for line in ["secret TOKEN", "volume /data", "kubernetes deploy"] {
            let source =
                format!("service ping\n{line}\nroute GET /health\nrespond 200 text/plain \"ok\"\n");
            let error = admit(&source).expect_err(line);
            assert_eq!(error.code, "DK-E0101", "{line}");
        }
    }

    #[test]
    fn privileged_and_padded_ports_are_rejected() {
        for port in ["80", "1023", "01024", "65536", "39171 "] {
            let source = format!(
                "service ping\nlisten {port}\nroute GET /health\nrespond 200 text/plain \"ok\"\n"
            );
            let error = admit(&source).expect_err(port);
            assert!(
                error.code == "DK-E0007" || error.code == "DK-E0004",
                "{port} -> {}",
                error.code
            );
        }
    }

    #[test]
    fn only_get_text_plain_200_is_admitted() {
        let cases = [
            "route POST /health",
            "route GET /health?ready=1",
            "route GET /../health",
            "respond 201 text/plain \"ok\"",
            "respond 200 application/json \"ok\"",
            "respond 200 text/plain \"\"",
        ];
        for replacement in cases {
            let mut lines: Vec<_> = ping().lines().collect();
            if replacement.starts_with("route") {
                lines[2] = replacement;
            } else {
                lines[3] = replacement;
            }
            let source = format!("{}\n", lines.join("\n"));
            let error = admit(&source).expect_err(replacement);
            assert!(
                error.code == "DK-E0008" || error.code == "DK-E0009",
                "{replacement} -> {}",
                error.code
            );
        }
    }

    #[test]
    fn body_may_contain_spaces_and_not_controls() {
        let source =
            "service ping\nlisten 39171\nroute GET /health\nrespond 200 text/plain \"ready now\"\n";
        assert_eq!(admit(source).expect("spaces").body, "ready now");
        let source =
            "service ping\nlisten 39171\nroute GET /health\nrespond 200 text/plain \"bad\\nok\"\n";
        assert_eq!(admit(source).expect_err("slash").code, "DK-E0009");
    }

    #[test]
    fn edge_ports_are_admitted() {
        for port in [1024u16, 65535] {
            let source = format!(
                "service ping\nlisten {port}\nroute GET /health\nrespond 200 text/plain \"ok\"\n"
            );
            assert_eq!(admit(&source).expect("edge port").port, port);
        }
    }

    #[test]
    fn body_limit_is_512() {
        let ok = "a".repeat(512);
        let source = format!(
            "service ping\nlisten 39171\nroute GET /health\nrespond 200 text/plain \"{ok}\"\n"
        );
        assert_eq!(admit(&source).expect("512").body, ok);
        let over = "a".repeat(513);
        let source = format!(
            "service ping\nlisten 39171\nroute GET /health\nrespond 200 text/plain \"{over}\"\n"
        );
        assert_eq!(admit(&source).expect_err("513").code, "DK-E0009");
    }

    #[test]
    fn path_limit_is_128() {
        let ok = format!("/{}", "a".repeat(127));
        assert_eq!(ok.len(), 128);
        let source =
            format!("service ping\nlisten 39171\nroute GET {ok}\nrespond 200 text/plain \"ok\"\n");
        assert_eq!(admit(&source).expect("128").path, ok);
        let over = format!("/{}", "a".repeat(128));
        assert_eq!(over.len(), 129);
        let source = format!(
            "service ping\nlisten 39171\nroute GET {over}\nrespond 200 text/plain \"ok\"\n"
        );
        assert_eq!(admit(&source).expect_err("129").code, "DK-E0008");
    }

    #[test]
    fn service_name_limit_is_31() {
        let ok = "a".repeat(31);
        let source = format!(
            "service {ok}\nlisten 39171\nroute GET /health\nrespond 200 text/plain \"ok\"\n"
        );
        assert_eq!(admit(&source).expect("31").name, ok);
        let over = "a".repeat(32);
        let source = format!(
            "service {over}\nlisten 39171\nroute GET /health\nrespond 200 text/plain \"ok\"\n"
        );
        assert_eq!(admit(&source).expect_err("32").code, "DK-E0006");
    }
}
