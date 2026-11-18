//! Deterministic process-local runtimes for the SSO identity caprichios.
//!
//! [`SsoFlow`] models the client side of an OAuth2 authorization-code flow
//! with optional PKCE (S256): it validates the caller-supplied identifiers,
//! scopes, and state, builds the authorization URL, and validates the
//! provider redirect. [`sha256`] and [`Pkce`] implement the S256 challenge
//! without external dependencies. None of these types open a network
//! connection, contact an authorization server, or exchange an access
//! token; hosts report their real network and browser facilities through
//! the caprichio runtime capabilities instead.

use std::fmt;

pub const MAX_CLIENT_ID_BYTES: usize = 128;
pub const MAX_REDIRECT_URI_BYTES: usize = 256;
pub const MIN_STATE_BYTES: usize = 16;
pub const MAX_STATE_BYTES: usize = 64;
pub const MAX_SCOPES: usize = 16;
pub const MAX_SCOPE_BYTES: usize = 64;
pub const MIN_VERIFIER_BYTES: usize = 43;
pub const MAX_VERIFIER_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SsoError {
    BadClientId,
    BadRedirectUri,
    BadEndpoint,
    BadState,
    BadScope,
    TooManyScopes,
    DuplicateScope,
    BadVerifier,
    StateMismatch,
    RedirectMismatch,
    MissingCode,
    FlowCompleted,
}

impl fmt::Display for SsoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadClientId => "client identifier must be a non-empty URL-safe name",
            Self::BadRedirectUri => "redirect URI must be https:// or a custom scheme",
            Self::BadEndpoint => "provider endpoints must use https://",
            Self::BadState => "state must be 16-64 URL-safe bytes",
            Self::BadScope => "scope must be 1-64 URL-safe bytes",
            Self::TooManyScopes => "scope count exceeds the supported bound",
            Self::DuplicateScope => "scopes must not repeat",
            Self::BadVerifier => "PKCE verifier must be 43-128 unreserved bytes",
            Self::StateMismatch => "redirect state does not match the request state",
            Self::RedirectMismatch => "redirect URI does not match the registered redirect",
            Self::MissingCode => "redirect query carries no authorization code",
            Self::FlowCompleted => "authorization code was already consumed",
        })
    }
}

impl std::error::Error for SsoError {}

fn url_safe(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/' | b':' | b'?' | b'=' | b'%' | b'+'))
        && !value.bytes().any(|b| matches!(b, b'&' | b' ' | b'#'))
}

fn is_endpoint(value: &str) -> bool {
    value.starts_with("https://") && url_safe(value)
}

/// Validate an OAuth2 redirect URI: `https://` or a non-`http` custom scheme
/// for native-app loops. Plain `http://` is rejected.
fn is_redirect_uri(value: &str) -> bool {
    if value.len() > MAX_REDIRECT_URI_BYTES {
        return false;
    }
    if value.starts_with("https://") {
        return url_safe(value);
    }
    // Custom scheme, e.g. `com.example.app:/oauth/callback`.
    let scheme = match value.split_once(':') {
        Some((scheme, _)) => scheme,
        None => return false,
    };
    !scheme.is_empty()
        && scheme != "http"
        && scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'+')
        && url_safe(value)
}

/// Minimal SHA-256 (FIPS 180-4) used for the PKCE S256 challenge. Only
/// tested against known vectors; not intended as a general-purpose cipher.
pub fn sha256(message: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Padding: message, 0x80, zeros, 64-bit big-endian bit length.
    let bit_len = (message.len() as u64) * 8;
    let mut padded = message.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for block in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (index, value) in [a, b, c, d, e, f, g, hh].into_iter().enumerate() {
            h[index] = h[index].wrapping_add(value);
        }
    }

    let mut digest = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        digest[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

fn base64url_nopad(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let packed = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(packed >> 18) as usize & 63] as char);
        out.push(ALPHABET[(packed >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(packed >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[packed as usize & 63] as char);
        }
    }
    out
}

/// A validated PKCE pair: the caller keeps the verifier secret and the flow
/// carries the S256 challenge in the authorization URL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pkce {
    verifier: String,
    challenge: String,
}

impl Pkce {
    pub fn from_verifier(verifier: &str) -> Result<Self, SsoError> {
        let in_range = (MIN_VERIFIER_BYTES..=MAX_VERIFIER_BYTES).contains(&verifier.len());
        if !in_range
            || !verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
        {
            return Err(SsoError::BadVerifier);
        }
        Ok(Self {
            verifier: verifier.to_owned(),
            challenge: base64url_nopad(&sha256(verifier.as_bytes())),
        })
    }

    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    pub fn challenge(&self) -> &str {
        &self.challenge
    }
}

/// Validated, ordered, duplicate-free scope set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScopeSet {
    scopes: Vec<String>,
}

impl ScopeSet {
    pub fn new(scopes: &[&str]) -> Result<Self, SsoError> {
        if scopes.is_empty() || scopes.len() > MAX_SCOPES {
            return Err(SsoError::TooManyScopes);
        }
        let mut validated = Vec::with_capacity(scopes.len());
        for scope in scopes {
            if scope.len() > MAX_SCOPE_BYTES
                || !scope
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b':' | b'/'))
            {
                return Err(SsoError::BadScope);
            }
            if validated.iter().any(|existing| existing == scope) {
                return Err(SsoError::DuplicateScope);
            }
            validated.push((*scope).to_owned());
        }
        Ok(Self { scopes: validated })
    }

    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }

    fn query_value(&self) -> String {
        self.scopes.join(" ")
    }
}

/// A validated, single-use authorization-code flow. `authorization_url`
/// builds the provider URL deterministically; `redirect_matches` validates
/// the provider callback and returns the authorization code. This type never
/// opens a network connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SsoFlow {
    authorize_endpoint: String,
    token_endpoint: String,
    client_id: String,
    redirect_uri: String,
    state: String,
    scopes: ScopeSet,
    pkce: Option<Pkce>,
    completed: bool,
}

impl SsoFlow {
    pub fn new(
        authorize_endpoint: &str,
        token_endpoint: &str,
        client_id: &str,
        redirect_uri: &str,
        state: &str,
        scopes: &[&str],
        pkce: Option<&Pkce>,
    ) -> Result<Self, SsoError> {
        if !is_endpoint(authorize_endpoint) || !is_endpoint(token_endpoint) {
            return Err(SsoError::BadEndpoint);
        }
        if client_id.is_empty()
            || client_id.len() > MAX_CLIENT_ID_BYTES
            || !client_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'))
        {
            return Err(SsoError::BadClientId);
        }
        if !is_redirect_uri(redirect_uri) {
            return Err(SsoError::BadRedirectUri);
        }
        if !(MIN_STATE_BYTES..=MAX_STATE_BYTES).contains(&state.len())
            || !state
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            return Err(SsoError::BadState);
        }
        let scopes = ScopeSet::new(scopes)?;
        Ok(Self {
            authorize_endpoint: authorize_endpoint.to_owned(),
            token_endpoint: token_endpoint.to_owned(),
            client_id: client_id.to_owned(),
            redirect_uri: redirect_uri.to_owned(),
            state: state.to_owned(),
            scopes,
            pkce: pkce.cloned(),
            completed: false,
        })
    }

    pub fn authorize_endpoint(&self) -> &str {
        &self.authorize_endpoint
    }

    pub fn token_endpoint(&self) -> &str {
        &self.token_endpoint
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    pub fn state(&self) -> &str {
        &self.state
    }

    pub fn scopes(&self) -> &[String] {
        self.scopes.scopes()
    }

    pub fn pkce_challenge(&self) -> Option<&str> {
        self.pkce.as_ref().map(|pkce| pkce.challenge())
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }

    /// The deterministic authorization URL. Scopes are space-separated as
    /// required by OAuth2; the flow validates that no component carries
    /// characters that would break the query.
    pub fn authorization_url(&self) -> String {
        let mut url = format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
            self.authorize_endpoint,
            self.client_id,
            self.redirect_uri,
            self.scopes.query_value(),
            self.state
        );
        if let Some(pkce) = &self.pkce {
            url.push_str(&format!(
                "&code_challenge={}&code_challenge_method=S256",
                pkce.challenge
            ));
        }
        url
    }

    /// Validate a provider redirect: the redirect URI must match exactly, the
    /// state must match, and a non-empty code must be present. The returned
    /// code is what a caller would exchange at the token endpoint.
    pub fn redirect_matches(&mut self, redirect: &str) -> Result<String, SsoError> {
        if self.completed {
            return Err(SsoError::FlowCompleted);
        }
        let (uri, query) = redirect
            .split_once('?')
            .ok_or(SsoError::MissingCode)?;
        if uri != self.redirect_uri {
            return Err(SsoError::RedirectMismatch);
        }
        let mut code = None;
        for param in query.split('&') {
            match param.split_once('=') {
                Some(("state", value)) if value == self.state => {}
                Some(("state", _)) => return Err(SsoError::StateMismatch),
                Some(("code", value)) if !value.is_empty() => code = Some(value.to_owned()),
                Some(("code", _)) => return Err(SsoError::MissingCode),
                _ => {}
            }
        }
        let code = code.ok_or(SsoError::MissingCode)?;
        self.completed = true;
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENDPOINT: &str = "https://provider.example/authorize";
    const TOKEN: &str = "https://provider.example/token";
    const REDIRECT: &str = "https://app.example/callback";
    const STATE: &str = "state-1234567890ab";

    fn flow(pkce: Option<&Pkce>) -> SsoFlow {
        SsoFlow::new(
            ENDPOINT,
            TOKEN,
            "client.example",
            REDIRECT,
            STATE,
            &["openid", "profile"],
            pkce,
        )
        .unwrap()
    }

    #[test]
    fn sha256_matches_known_vectors() {
        let empty = sha256(b"");
        assert_eq!(
            empty.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let abc = sha256(b"abc");
        assert_eq!(
            abc.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // Longer than one block (verified against a reference SHA-256).
        let long = sha256(&[b'a'; 1000]);
        assert_eq!(
            long.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
        );
    }

    #[test]
    fn pkce_challenge_is_s256_of_verifier() {
        // RFC 7636 appendix B vector.
        let pkce =
            Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk").unwrap();
        assert_eq!(
            pkce.challenge(),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(
            Pkce::from_verifier(&"a".repeat(42)).err(),
            Some(SsoError::BadVerifier)
        );
        assert_eq!(
            Pkce::from_verifier(&"a".repeat(129)).err(),
            Some(SsoError::BadVerifier)
        );
        assert!(Pkce::from_verifier(&"a".repeat(43)).is_ok());
        assert!(Pkce::from_verifier(&"a".repeat(128)).is_ok());
        assert_eq!(
            Pkce::from_verifier("bad verifier with spaces").err(),
            Some(SsoError::BadVerifier)
        );
    }

    #[test]
    fn flow_builds_deterministic_authorization_url_and_consumes_redirect() {
        let pkce = Pkce::from_verifier(&"v".repeat(64)).unwrap();
        let flow = flow(Some(&pkce));
        let expected = format!(
            "{ENDPOINT}?response_type=code&client_id=client.example&redirect_uri={REDIRECT}&scope=openid%20profile&state={STATE}&code_challenge={}&code_challenge_method=S256",
            pkce.challenge()
        );
        // Scope separator is a literal space per OAuth2, not percent-encoded
        // by this builder.
        let url = flow.authorization_url();
        assert!(url.starts_with(&format!(
            "{ENDPOINT}?response_type=code&client_id=client.example&redirect_uri={REDIRECT}&scope=openid profile&state={STATE}"
        )));
        assert!(url.ends_with(&format!(
            "&code_challenge={}&code_challenge_method=S256",
            pkce.challenge()
        )));
        assert_eq!(url, expected.replace("%20", " "));

        let mut consumed = flow;
        let redirect = format!("{REDIRECT}?code=abc123&state={STATE}");
        assert_eq!(consumed.redirect_matches(&redirect).unwrap(), "abc123");
        assert!(consumed.is_completed());
        assert_eq!(
            consumed.redirect_matches(&redirect),
            Err(SsoError::FlowCompleted)
        );
    }

    #[test]
    fn redirect_validation_rejects_mismatched_state_and_missing_code() {
        let mut flow = flow(None);
        assert_eq!(
            flow.redirect_matches(&format!("{REDIRECT}?code=x&state=wrong")),
            Err(SsoError::StateMismatch)
        );
        assert_eq!(
            flow.redirect_matches(&format!("{REDIRECT}?state={STATE}")),
            Err(SsoError::MissingCode)
        );
        assert_eq!(
            flow.redirect_matches("https://other.example/callback?code=x"),
            Err(SsoError::RedirectMismatch)
        );
        // Host mismatch still fails through query parsing; exact-URI equality
        // is asserted by the code not being found in a matching-shape query.
        assert_eq!(
            flow.redirect_matches(&format!("{REDIRECT}?code=&state={STATE}")),
            Err(SsoError::MissingCode)
        );
    }

    #[test]
    fn flow_validation_rejects_bad_components() {
        assert_eq!(
            SsoFlow::new("http://insecure.example/auth", TOKEN, "c", REDIRECT, STATE, &["openid"], None),
            Err(SsoError::BadEndpoint)
        );
        assert_eq!(
            SsoFlow::new(ENDPOINT, "https://t.example", "", REDIRECT, STATE, &["openid"], None),
            Err(SsoError::BadClientId)
        );
        assert_eq!(
            SsoFlow::new(ENDPOINT, TOKEN, "c", "http://app.example/cb", STATE, &["openid"], None),
            Err(SsoError::BadRedirectUri)
        );
        assert_eq!(
            SsoFlow::new(ENDPOINT, TOKEN, "c", REDIRECT, "short", &["openid"], None),
            Err(SsoError::BadState)
        );
        assert_eq!(
            SsoFlow::new(ENDPOINT, TOKEN, "c", REDIRECT, STATE, &[], None),
            Err(SsoError::TooManyScopes)
        );
        assert_eq!(
            SsoFlow::new(ENDPOINT, TOKEN, "c", REDIRECT, STATE, &["openid", "openid"], None),
            Err(SsoError::DuplicateScope)
        );
        // Custom scheme redirects are accepted for native loops.
        assert!(SsoFlow::new(
            ENDPOINT,
            TOKEN,
            "c",
            "com.example.app:/oauth/callback",
            STATE,
            &["openid"],
            None
        )
        .is_ok());
    }
}
