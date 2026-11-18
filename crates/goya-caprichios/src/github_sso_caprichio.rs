//! A bounded github SSO (OAuth2 authorization-code) capability.
//!
//! The negotiated artifact binds to this crate's process-local SSO flow
//! runtime. It does not open a browser, contact GitHub endpoints, exchange
//! tokens, or prove that a browser is available. Hosts report their real
//! network and user-agent facilities through [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.identity.sso.github.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const AUTHORIZE_ENDPOINT: &str = "https://github.com/login/oauth/authorize";
pub const TOKEN_ENDPOINT: &str = "https://github.com/login/oauth/access_token";
/// Scopes this recipe allows; anything else is rejected at normalization.
pub const ALLOWED_SCOPES: &[&str] = &["read:user", "user:email", "read:org", "repo"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GithubSsoDeclaration {
    /// OAuth2 client identifier issued by the provider console.
    pub client_id: String,
    /// Redirect URI: `https://` or a custom native-app scheme.
    pub redirect_uri: String,
    /// Anti-forgery state, 16-64 URL-safe bytes, caller-generated.
    pub state: String,
    /// Requested scopes; must all be in `ALLOWED_SCOPES` and non-empty.
    pub scopes: Vec<String>,
    /// `Some(verifier)` enables PKCE S256; `None` disables it.
    pub pkce_verifier: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GithubSsoIr {
    client_id: String,
    redirect_uri: String,
    state: String,
    scopes: Vec<String>,
    pkce_verifier: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Browser, Node, CrossPlatform, Android, Ios,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub https_client: bool,
    /// Host can present an external user agent and receive the redirect.
    pub external_user_agent: bool,
    /// Host can persist tokens out of this process's lifetime.
    pub secure_storage: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    BadClientId,
    BadRedirectUri,
    BadState,
    UnknownScope,
    EmptyScopes,
    TooManyScopes,
    BadVerifier,
    UnsupportedTarget,
    MissingHttpsClient,
    MissingExternalUserAgent,
    MissingSecureStorage,
    MissingUtf8Strings,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadClientId => "client identifier must be a non-empty URL-safe name",
            Self::BadRedirectUri => "redirect URI must be https:// or a custom scheme",
            Self::BadState => "state must be 16-64 URL-safe bytes",
            Self::UnknownScope => {
                "requested scope is not in the GitHub allowlist for this recipe"
            }
            Self::EmptyScopes => "at least one scope must be requested",
            Self::TooManyScopes => "scope count exceeds the supported bound",
            Self::BadVerifier => "PKCE verifier must be 43-128 unreserved bytes",
            Self::UnsupportedTarget => "GitHub SSO supports browser and CLI (Node) targets",
            Self::MissingHttpsClient => "target runtime must provide an HTTPS client",
            Self::MissingExternalUserAgent => {
                "target must present an external user agent for authorization"
            }
            Self::MissingSecureStorage => {
                "target must provide storage outside this process lifetime"
            }
            Self::MissingUtf8Strings => "target runtime must provide UTF-8 strings",
        })
    }
}

impl std::error::Error for Diagnostic {}

/// Negotiated design and source text. The artifact is a factory definition;
/// successful construction is not proof that a browser was opened or that an
/// authorization code was issued.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GithubSsoArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl GithubSsoDeclaration {
    pub fn normalize(self) -> Result<GithubSsoIr, Diagnostic> {
        if self.client_id.is_empty()
            || self.client_id.len() > crate::sso_runtime::MAX_CLIENT_ID_BYTES
            || !self
                .client_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'))
        {
            return Err(Diagnostic::BadClientId);
        }
        let bytes = self.redirect_uri.as_bytes();
        if bytes.len() > crate::sso_runtime::MAX_REDIRECT_URI_BYTES
            || !(self.redirect_uri.starts_with("https://")
                || self
                    .redirect_uri
                    .split_once(':')
                    .is_some_and(|(scheme, _)| {
                        scheme != "http"
                            && !scheme.is_empty()
                            && scheme
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
                    }))
            || self.redirect_uri.bytes().any(|b| matches!(b, b'&' | b' ' | b'#'))
        {
            return Err(Diagnostic::BadRedirectUri);
        }
        if !(crate::sso_runtime::MIN_STATE_BYTES..=crate::sso_runtime::MAX_STATE_BYTES)
            .contains(&self.state.len())
            || !self
                .state
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            return Err(Diagnostic::BadState);
        }
        if self.scopes.is_empty() {
            return Err(Diagnostic::EmptyScopes);
        }
        if self.scopes.len() > crate::sso_runtime::MAX_SCOPES {
            return Err(Diagnostic::TooManyScopes);
        }
        for scope in &self.scopes {
            if !ALLOWED_SCOPES.contains(&scope.as_str()) {
                return Err(Diagnostic::UnknownScope);
            }
        }
        if let Some(verifier) = &self.pkce_verifier {
            let in_range = (crate::sso_runtime::MIN_VERIFIER_BYTES
                ..=crate::sso_runtime::MAX_VERIFIER_BYTES)
                .contains(&verifier.len());
            if !in_range
                || !verifier
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
            {
                return Err(Diagnostic::BadVerifier);
            }
        }
        Ok(GithubSsoIr {
            client_id: self.client_id,
            redirect_uri: self.redirect_uri,
            state: self.state,
            scopes: self.scopes,
            pkce_verifier: self.pkce_verifier,
        })
    }
}

impl GithubSsoIr {
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
        &self.scopes
    }

    pub fn uses_pkce(&self) -> bool {
        self.pkce_verifier.is_some()
    }

    fn target_label(target: Target) -> &'static str {
        match target {
            Target::Browser => "browser",
            Target::Node => "node",
            Target::CrossPlatform => "cross_platform",
            Target::Android => "android",
            Target::Ios => "ios",
        }
    }

    pub fn canonical_for(&self, target: Target) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget={}\nprovider=github\nauthorize={AUTHORIZE_ENDPOINT}\ntoken={TOKEN_ENDPOINT}\nclient_id={}\nredirect={}\nscopes={}\npkce={}\ntoken_exchange=none\n",
            Self::target_label(target),
            self.client_id,
            self.redirect_uri,
            self.scopes.join(" "),
            if self.pkce_verifier.is_some() {
                "s256"
            } else {
                "none"
            }
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<GithubSsoArtifact, Diagnostic> {
        if !matches!(
            target,
            Target::Browser | Target::Node
        ) {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.https_client {
            return Err(Diagnostic::MissingHttpsClient);
        }
        if !capabilities.external_user_agent {
            return Err(Diagnostic::MissingExternalUserAgent);
        }
        if !capabilities.secure_storage {
            return Err(Diagnostic::MissingSecureStorage);
        }
        if !capabilities.utf8_strings {
            return Err(Diagnostic::MissingUtf8Strings);
        }
        let scope_literals = self
            .scopes
            .iter()
            .map(|scope| format!("{scope:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        let pkce_lines = match &self.pkce_verifier {
            Some(verifier) => format!(
                "    let pkce = crate::sso_runtime::Pkce::from_verifier({verifier:?})?;\n"
            ),
            None => String::new(),
        };
        let pkce_arg = if self.pkce_verifier.is_some() {
            "Some(&pkce)"
        } else {
            "None"
        };
        Ok(GithubSsoArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for(target),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; github SSO authorization-code flow, no network connection is opened here.\npub fn make_sso_flow() -> Result<crate::sso_runtime::SsoFlow, crate::sso_runtime::SsoError> {{\n{pkce_lines}    crate::sso_runtime::SsoFlow::new(\n        {AUTHORIZE_ENDPOINT:?},\n        {TOKEN_ENDPOINT:?},\n        {:?},\n        {:?},\n        {:?},\n        &[{scope_literals}],\n        {pkce_arg},\n    )\n}}\n",
                self.client_id, self.redirect_uri, self.state
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BROWSER: RuntimeCapabilities = RuntimeCapabilities {
        https_client: true,
        external_user_agent: true,
        secure_storage: true,
        utf8_strings: true,
    };

    fn declaration() -> GithubSsoDeclaration {
        GithubSsoDeclaration {
            client_id: "goya-client.example".into(),
            redirect_uri: "https://app.example/oauth/callback".into(),
            state: "state-1234567890ab".into(),
            scopes: vec!["read:user".to_owned(), "user:email".to_owned()],
            pkce_verifier: Some("v".repeat(64)),
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_factory() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(ir.client_id(), "goya-client.example");
        assert_eq!(ir.scopes().to_vec(), ["read:user".to_owned(), "user:email".to_owned()]);
        assert!(ir.uses_pkce());
        let first = ir.lower_checked(Target::Browser, BROWSER).unwrap();
        let second = ir.lower_checked(Target::Browser, BROWSER).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=browser\n"));
        assert!(first.canonical_ir.contains("provider=github\n"));
        assert!(first
            .canonical_ir
            .contains("authorize=https://github.com/login/oauth/authorize\n"));
        assert!(first.canonical_ir.contains("scopes=read:user user:email\n"));
        assert!(first.canonical_ir.contains("pkce=s256\n"));
        assert!(first.canonical_ir.contains("token_exchange=none\n"));
        assert!(first.source.contains("make_sso_flow"));
        assert!(first
            .source
            .contains("https://github.com/login/oauth/authorize"));
        assert!(first.source.contains("Pkce::from_verifier"));
    }

    #[test]
    fn pkce_free_declaration_lowers_without_challenge() {
        let mut plain = declaration();
        plain.pkce_verifier = None;
        let ir = plain.normalize().unwrap();
        assert!(!ir.uses_pkce());
        let artifact = ir.lower_checked(Target::Browser, BROWSER).unwrap();
        assert!(artifact.canonical_ir.contains("pkce=none\n"));
        assert!(artifact.canonical_ir.contains("target=browser\n"));
        assert!(!artifact.source.contains("Pkce::from_verifier"));
        assert!(artifact.source.contains("        None,\n"));
    }

    #[test]
    fn rejects_bad_client_redirect_state_and_scopes() {
        assert_eq!(
            GithubSsoDeclaration {
                client_id: String::new(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::BadClientId)
        );
        assert_eq!(
            GithubSsoDeclaration {
                client_id: "bad client!".into(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::BadClientId)
        );
        assert_eq!(
            GithubSsoDeclaration {
                redirect_uri: "http://app.example/cb".into(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::BadRedirectUri)
        );
        assert_eq!(
            GithubSsoDeclaration {
                redirect_uri: "not a uri".into(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::BadRedirectUri)
        );
        assert_eq!(
            GithubSsoDeclaration {
                state: "short".into(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::BadState)
        );
        assert_eq!(
            GithubSsoDeclaration {
                scopes: vec!["delete:packages".into()],
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::UnknownScope)
        );
        assert_eq!(
            GithubSsoDeclaration {
                scopes: Vec::new(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::EmptyScopes)
        );
        assert_eq!(
            GithubSsoDeclaration {
                scopes: vec!["read:user".to_owned(); crate::sso_runtime::MAX_SCOPES + 1],
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::TooManyScopes)
        );
        assert_eq!(
            GithubSsoDeclaration {
                pkce_verifier: Some("short".into()),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::BadVerifier)
        );
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let ir = declaration().normalize().unwrap();
        for target in [Target::CrossPlatform, Target::Android, Target::Ios] {
            assert_eq!(
                ir.lower_checked(target, BROWSER),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    https_client: false,
                    ..BROWSER
                },
                Diagnostic::MissingHttpsClient,
            ),
            (
                RuntimeCapabilities {
                    external_user_agent: false,
                    ..BROWSER
                },
                Diagnostic::MissingExternalUserAgent,
            ),
            (
                RuntimeCapabilities {
                    secure_storage: false,
                    ..BROWSER
                },
                Diagnostic::MissingSecureStorage,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..BROWSER
                },
                Diagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(ir.lower_checked(Target::Browser, missing), Err(expected));
        }
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_sso_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration()
            .normalize()
            .unwrap()
            .lower_checked(Target::Browser, BROWSER)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/sso_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod sso_runtime;\n{}\nfn main() {{\n    let mut flow = make_sso_flow().expect(\"factory should construct flow\");\n    assert_eq!(flow.client_id(), \"goya-client.example\");\n    assert!(flow.authorization_url().starts_with(\"https://github.com/login/oauth/authorize\"));\n    assert!(flow.pkce_challenge().is_some());\n    let redirect = \"https://app.example/oauth/callback?code=abc123&state=state-1234567890ab\";\n    assert_eq!(flow.redirect_matches(redirect).unwrap(), \"abc123\");\n    assert!(flow.is_completed());\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-github-sso-factory-{}-{nonce}",
            std::process::id()
        ));
        let mut rustc = Command::new("rustc")
            .args(["--edition=2021", "-o"])
            .arg(&output_path)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("rustc must be available to compile generated source");
        rustc
            .stdin
            .take()
            .expect("rustc stdin")
            .write_all(harness.as_bytes())
            .expect("write generated factory harness");
        let compile = rustc.wait_with_output().expect("wait for rustc");
        assert!(
            compile.status.success(),
            "generated factory failed to compile:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let execution = Command::new(&output_path)
            .output()
            .expect("run generated GitHub SSO factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
