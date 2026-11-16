//! Browser Fetch construction for a deliberately narrow HTTPS request.
//!
//! This is a prototype recipe, not a browser conformance claim. Browser CORS
//! rules expose only safelisted response headers and headers named by the
//! server's `Access-Control-Expose-Headers`; forbidden request headers remain
//! browser-controlled. Redirects fail, credentials are omitted, and Fetch
//! resolves for HTTP error statuses so callers can inspect status and visible
//! response headers.

use std::fmt;

pub const CONTRACT: &str = "goya.network.http.browser_fetch_cors_limited.v1";
pub const RECIPE_VERSION: &str = "0.0.1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Method {
    Get,
    Post,
}

impl Method {
    fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RedirectPolicy {
    Error,
    Follow,
    Manual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialPolicy {
    Omit,
    SameOrigin,
    Include,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpDeclaration {
    pub method: Method,
    /// Absolute HTTPS URL. User information and whitespace are rejected.
    pub target: String,
    pub redirect: RedirectPolicy,
    pub credentials: CredentialPolicy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpIr {
    method: Method,
    target: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Browser,
    Node,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub fetch: bool,
    pub url: bool,
    /// `Headers.forEach` must be available. Exposed headers remain CORS-limited.
    pub response_headers: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    InvalidHttpsTarget,
    UnsupportedRedirectPolicy,
    UnsupportedCredentialPolicy,
    UnsupportedTarget,
    MissingFetch,
    MissingUrl,
    MissingResponseHeaders,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidHttpsTarget => {
                "target must be an absolute HTTPS URL without user information or whitespace"
            }
            Self::UnsupportedRedirectPolicy => "browser recipe supports only redirect:error",
            Self::UnsupportedCredentialPolicy => "browser recipe supports only credentials:omit",
            Self::UnsupportedTarget => "HTTP recipe supports only the browser Fetch target",
            Self::MissingFetch => "target runtime must provide Fetch",
            Self::MissingUrl => "target runtime must provide URL",
            Self::MissingResponseHeaders => {
                "target runtime must provide Fetch response Headers.forEach"
            }
        })
    }
}

impl std::error::Error for Diagnostic {}

/// Generated browser source and its normalized contract. `header_visibility`
/// states the browser limitation explicitly; it does not assert full header
/// visibility or implementation of every `network.http` catalog policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub header_visibility: &'static str,
    pub source: String,
}

impl HttpDeclaration {
    pub fn normalize(self) -> Result<HttpIr, Diagnostic> {
        if !is_acceptable_https_target(&self.target) {
            return Err(Diagnostic::InvalidHttpsTarget);
        }
        if self.redirect != RedirectPolicy::Error {
            return Err(Diagnostic::UnsupportedRedirectPolicy);
        }
        if self.credentials != CredentialPolicy::Omit {
            return Err(Diagnostic::UnsupportedCredentialPolicy);
        }
        Ok(HttpIr {
            method: self.method,
            target: self.target,
        })
    }
}

fn is_acceptable_https_target(target: &str) -> bool {
    if !target.starts_with("https://")
        || !target.is_ascii()
        || target.bytes().any(|byte| {
            byte.is_ascii_whitespace() || byte.is_ascii_control() || byte == b'\\' || byte == b'#'
        })
    {
        return false;
    }
    let rest = &target[8..];
    if !valid_percent_escapes(rest) {
        return false;
    }
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    valid_authority(authority)
}

fn valid_authority(authority: &str) -> bool {
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    if let Some(bracketed) = authority.strip_prefix('[') {
        let Some((address, remainder)) = bracketed.split_once(']') else {
            return false;
        };
        if address.parse::<std::net::Ipv6Addr>().is_err() {
            return false;
        }
        return remainder.is_empty()
            || remainder.strip_prefix(':').map(valid_port).unwrap_or(false);
    }

    let mut parts = authority.split(':');
    let host = parts.next().unwrap_or_default();
    let port = parts.next();
    if parts.next().is_some() || !valid_dns_host(host) {
        return false;
    }
    match port {
        Some(port) => valid_port(port),
        None => true,
    }
}

fn valid_dns_host(host: &str) -> bool {
    if !host.is_empty()
        && host
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        // WHATWG URL parsing treats numeric dotted hosts as IPv4 addresses;
        // require the strict four-octet form to avoid hidden reinterpretation.
        return valid_strict_ipv4(host);
    }
    !host.is_empty()
        && host.len() <= 253
        && !host.rsplit('.').next().is_some_and(whatwg_ipv4_number_like)
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn valid_strict_ipv4(host: &str) -> bool {
    let octets: Vec<_> = host.split('.').collect();
    octets.len() == 4
        && octets.iter().all(|octet| {
            !octet.is_empty()
                && octet.len() <= 3
                && (*octet == "0" || !octet.starts_with('0'))
                && octet.bytes().all(|byte| byte.is_ascii_digit())
                && octet.parse::<u8>().is_ok()
        })
}

/// Reject domain endings that WHATWG URL parsing may reinterpret as IPv4.
/// The prefix checks cover decimal, legacy-octal and `0x` hexadecimal forms.
fn whatwg_ipv4_number_like(label: &str) -> bool {
    if label.bytes().all(|byte| byte.is_ascii_digit()) {
        return true;
    }
    if let Some(hex) = label
        .strip_prefix("0x")
        .or_else(|| label.strip_prefix("0X"))
    {
        return hex.is_empty() || hex.bytes().all(|byte| byte.is_ascii_hexdigit());
    }
    label.len() > 1
        && label.starts_with('0')
        && label.bytes().all(|byte| matches!(byte, b'0'..=b'7'))
}

fn valid_port(port: &str) -> bool {
    !port.is_empty()
        && port.bytes().all(|byte| byte.is_ascii_digit())
        && port.parse::<u16>().map(|value| value != 0).unwrap_or(false)
}

fn valid_percent_escapes(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return false;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    true
}

fn js_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{2028}' => escaped.push_str("\\u2028"),
            '\u{2029}' => escaped.push_str("\\u2029"),
            c if c.is_control() => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            c => escaped.push(c),
        }
    }
    escaped.push('"');
    escaped
}

fn hex(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        use fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

impl HttpIr {
    pub fn method(&self) -> Method {
        self.method
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=browser_fetch\nmethod={}\nurl_utf8_hex={}\nredirect=error\ncredentials=omit\nresponse_status=observable\nresponse_headers=cors_exposed_only\n",
            self.method.as_str(),
            hex(&self.target)
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<HttpArtifact, Diagnostic> {
        if target != Target::Browser {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.fetch {
            return Err(Diagnostic::MissingFetch);
        }
        if !capabilities.url {
            return Err(Diagnostic::MissingUrl);
        }
        if !capabilities.response_headers {
            return Err(Diagnostic::MissingResponseHeaders);
        }
        let source = format!(
            r#"// {CONTRACT}; browser Fetch prototype recipe={RECIPE_VERSION}
// Response headers are only those exposed by Fetch/CORS; forbidden request headers are browser-controlled.
// Redirects fail, credentials are omitted, and non-2xx HTTP statuses are returned normally.
export async function request() {{
  const target = new URL({url});
  if (target.protocol !== 'https:') throw new TypeError('HTTPS required');
  const response = await fetch(target, {{
    method: '{method}',
    mode: 'cors',
    redirect: 'error',
    credentials: 'omit'
  }});
  const headers = [];
  response.headers.forEach((value, name) => headers.push([name, value]));
  return {{ status: response.status, headers }};
}}
"#,
            url = js_string(&self.target),
            method = self.method.as_str()
        );
        Ok(HttpArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            header_visibility: "Only browser-visible CORS safelisted or explicitly exposed headers; values may be normalized by Fetch.",
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BROWSER: RuntimeCapabilities = RuntimeCapabilities {
        fetch: true,
        url: true,
        response_headers: true,
    };

    fn sample_declaration() -> HttpDeclaration {
        HttpDeclaration {
            method: Method::Get,
            target: "https://api.example.test/v1/items?q=one".to_owned(),
            redirect: RedirectPolicy::Error,
            credentials: CredentialPolicy::Omit,
        }
    }

    #[test]
    fn normalization_and_lowering_are_deterministic_and_observe_status_and_headers() {
        let ir = sample_declaration().normalize().unwrap();
        assert_eq!(ir.method(), Method::Get);
        assert_eq!(ir.target(), "https://api.example.test/v1/items?q=one");
        let artifact = ir.lower_checked(Target::Browser, BROWSER).unwrap();
        assert_eq!(
            artifact,
            ir.lower_checked(Target::Browser, BROWSER).unwrap()
        );
        assert_eq!(artifact.contract, CONTRACT);
        assert!(artifact
            .canonical_ir
            .contains("response_status=observable\n"));
        assert!(artifact
            .canonical_ir
            .contains("response_headers=cors_exposed_only\n"));
        assert!(artifact
            .source
            .contains("return { status: response.status, headers }"));
        assert!(artifact.source.contains("response.headers.forEach"));
        assert!(artifact.source.contains("redirect: 'error'"));
        assert!(artifact.source.contains("credentials: 'omit'"));
        assert!(artifact.source.contains("method: 'GET'"));
        assert!(artifact.header_visibility.contains("CORS"));
    }

    #[test]
    fn method_is_explicit_and_post_is_supported() {
        let mut declaration = sample_declaration();
        declaration.method = Method::Post;
        let artifact = declaration
            .normalize()
            .unwrap()
            .lower_checked(Target::Browser, BROWSER)
            .unwrap();
        assert!(artifact.canonical_ir.contains("method=POST\n"));
        assert!(artifact.source.contains("method: 'POST'"));
    }

    #[test]
    fn rejects_invalid_targets_redirects_and_credentials() {
        for target in [
            "http://api.example.test/",
            "https:///missing-host",
            "https://user:secret@example.test/",
            "https://example.test/a b",
            "https://example.test:bad/",
            "https://example.test:65536/",
            "https://example..test/",
            "https://[not-ipv6]/",
            "https://[2001:db8::1]:0/",
            "https://999.999.999.999/",
            "https://127.1/",
            "https://0x7f000001/",
            "https://example.1/",
            "https://example.0x1/",
            "https://example.0177/",
            "https://example.test\\@evil.test/",
            "https://example.test\\path",
            "https://example.test/%xy",
            "https://example.test/#fragment",
        ] {
            let mut declaration = sample_declaration();
            declaration.target = target.to_owned();
            assert_eq!(
                declaration.normalize(),
                Err(Diagnostic::InvalidHttpsTarget),
                "{target}"
            );
        }
        let mut declaration = sample_declaration();
        declaration.redirect = RedirectPolicy::Follow;
        assert_eq!(
            declaration.normalize(),
            Err(Diagnostic::UnsupportedRedirectPolicy)
        );
        let mut declaration = sample_declaration();
        declaration.credentials = CredentialPolicy::Include;
        assert_eq!(
            declaration.normalize(),
            Err(Diagnostic::UnsupportedCredentialPolicy)
        );
    }

    #[test]
    fn accepts_strict_ip_literals_and_valid_ports() {
        for target in [
            "https://192.0.2.10:443/resource",
            "https://[2001:db8::1]:8443/resource",
            "https://0.0.0.0/resource",
        ] {
            let mut declaration = sample_declaration();
            declaration.target = target.to_owned();
            assert!(declaration.normalize().is_ok(), "{target}");
        }
    }

    #[test]
    fn negotiation_fails_closed_for_targets_and_missing_capabilities() {
        let ir = sample_declaration().normalize().unwrap();
        assert_eq!(
            ir.lower_checked(Target::Node, BROWSER),
            Err(Diagnostic::UnsupportedTarget)
        );
        assert_eq!(
            ir.lower_checked(Target::Browser, RuntimeCapabilities::default()),
            Err(Diagnostic::MissingFetch)
        );
        assert_eq!(
            ir.lower_checked(
                Target::Browser,
                RuntimeCapabilities {
                    fetch: true,
                    url: false,
                    response_headers: true,
                }
            ),
            Err(Diagnostic::MissingUrl)
        );
        assert_eq!(
            ir.lower_checked(
                Target::Browser,
                RuntimeCapabilities {
                    fetch: true,
                    url: true,
                    response_headers: false,
                }
            ),
            Err(Diagnostic::MissingResponseHeaders)
        );
    }

    /// Construction-only smoke benchmark; it does not measure browser Fetch.
    #[test]
    #[ignore]
    fn benchmark_http_recipe_construction() {
        let ir = sample_declaration().normalize().unwrap();
        let start = std::time::Instant::now();
        for _ in 0..100_000 {
            std::hint::black_box(ir.lower_checked(Target::Browser, BROWSER).unwrap());
        }
        println!("100,000 HTTP recipe constructions in {:?}", start.elapsed());
    }
}
