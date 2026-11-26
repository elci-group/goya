//! `bloom.link/0`: Aragon's Android capability adapter for a Bloom node
//! (`bloomd`). Contract `aragon.bloomlink/0.0.1`.
//!
//! A GSSH/4-live scene declares the link with the existing `@link` grammar
//! (the same construct Arachnid lowers for `goya.lanlink/0`). It names the
//! `host` keys the link writes and addresses the link with D2 `network`
//! requests whose first word is the link's name:
//!
//! ```text
//! @state { link: int = 0 host; detail: text = "" host; paused: bool = false host; said: text = "" session; }
//! @link bloom { id: "pocket.link"; protocol: "bloom.link/0"; endpoint: "https://bloom-node.local:7420";
//!               status: state.link; detail: state.detail; field draining: state.paused; }
//! @interaction pause { ... class: D2; effects { network; }
//!     requests { r: effect network capability "network" request "bloom pause" result text; }
//!     state { write said: text; } input { } set said = effect.r; }
//! ```
//!
//! | Request | Effect on `bloomd` |
//! | --- | --- |
//! | `NAME refresh` | `GET /bloom/v1/nodes` plus the endpoints the bound fields need |
//! | `NAME pause` | `POST /bloom/v1/nodes/drain` `{"enabled":true}`, then refresh |
//! | `NAME resume` | checks the node ID is the one shown, `POST …/drain` `{"enabled":false}`, then refresh |
//! | `NAME credentials` | the platform's token dialog (never scene state) |
//! | `NAME forget` | deletes the stored token, CA and Keystore key |
//!
//! Every request returns text. Requests, roles, fields, the protocol and the
//! endpoint are checked here at compile time and refused at their exact
//! source range; nothing in the scene is protocol code. The Kotlin runtime
//! (`GoyaLink.kt`, `GoyaLinkModel.kt`) is compiler output, identical for every
//! app apart from the `GoyaLinkConfig` table.
//!
//! **Security.** No credential can be written in the scene: the endpoint
//! refuses user info, queries and fragments, and the operator token is
//! entered in a platform dialog, sealed with an Android Keystore AES-GCM key
//! bound to this link's name and endpoint, and stored outside the D8 record.
//! Non-loopback endpoints must be HTTPS with normal certificate and host
//! name verification; an operator CA entered on the device replaces the
//! system trust anchors for this link (a pin), the way `bloom-node-app
//! --ca-pem` trusts a LAN CA.

use crate::android_scene::{AndroidSceneError, AndroidSceneReason};
use crate::kotlin::{KotlinFile, KotlinWriter, SourceMapEntry};
use goya_lang::handoff::{Origin, SemanticHandoffV2};
use goya_lang::{
    EffectValueType, InteractionClass, InteractionExpr, InteractionType, InteractionValue,
    LinkDecl, ParsedScene, SourceRange, StateLifetime, Token, TokenKind, TypedKey,
};

/// The protocol a `@link` names to be lowered by this adapter.
pub const BLOOM_LINK_PROTOCOL: &str = "bloom.link/0";
/// Version of Aragon's lowering of [`BLOOM_LINK_PROTOCOL`].
pub const BLOOM_LINK_CONTRACT: &str = "aragon.bloomlink/0.0.1";

/// Roles `bloom.link/0` writes. `status` is an int (see the runtime's
/// status values); `detail` and `error` are text.
pub const BLOOM_LINK_ROLES: &[&str] = &["status", "detail", "error"];

/// Fields `bloom.link/0` reports, with the type the bound `host` key must have.
pub const BLOOM_LINK_FIELDS: &[(&str, InteractionType)] = &[
    ("node_id", InteractionType::Text),
    ("node_name", InteractionType::Text),
    ("node_state", InteractionType::Text),
    ("draining", InteractionType::Bool),
    ("runner_ok", InteractionType::Bool),
    ("active_jobs", InteractionType::Int),
    ("free_slots", InteractionType::Int),
    ("uptime_s", InteractionType::Int),
    ("cpu_percent", InteractionType::Int),
    ("memory_free_mib", InteractionType::Int),
    ("models", InteractionType::Text),
    ("model_count", InteractionType::Int),
    ("peers", InteractionType::Int),
    ("nearby", InteractionType::Int),
];

/// The request verbs (second word of a request).
pub const BLOOM_LINK_REQUESTS: &[&str] = &["refresh", "pause", "resume", "credentials", "forget"];

/// Prefix of the D1 runtime inputs that carry D2 effect results. Authored
/// input names are identifiers, so they never contain the dot.
pub(crate) const EFFECT_INPUT_PREFIX: &str = "effect.";

/// What the Android project needs to host the link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidLink {
    /// `bloom.link/0`.
    pub protocol: String,
    /// The link's name (first word of its requests).
    pub name: String,
    /// Validated base URL, without a trailing slash.
    pub endpoint: String,
    /// The endpoint is plain HTTP on loopback (`127.0.0.1` or `localhost`).
    pub cleartext_loopback: bool,
}

impl AndroidLink {
    /// `res/xml/goya_network_security.xml`: cleartext is refused everywhere
    /// except, for a loopback HTTP endpoint, on loopback; trust anchors are
    /// the system's (an operator CA is applied by the runtime itself).
    pub fn network_security_config(&self) -> String {
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- Generated by Aragon for ");
        out.push_str(BLOOM_LINK_PROTOCOL);
        out.push_str(": HTTPS only, system trust anchors; an operator CA entered on the device is applied by GoyaLink for this link alone. -->\n<network-security-config>\n    <base-config cleartextTrafficPermitted=\"false\">\n        <trust-anchors>\n            <certificates src=\"system\"/>\n        </trust-anchors>\n    </base-config>\n");
        if self.cleartext_loopback {
            out.push_str("    <domain-config cleartextTrafficPermitted=\"true\">\n        <domain includeSubdomains=\"false\">127.0.0.1</domain>\n        <domain includeSubdomains=\"false\">localhost</domain>\n    </domain-config>\n");
        }
        out.push_str("</network-security-config>\n");
        out
    }
}

/// The checked link and the D2 actions addressed to it.
pub(crate) struct LinkPlan {
    pub link: AndroidLink,
    roles: Vec<(String, String)>,
    fields: Vec<(String, String)>,
    /// Action ID and its (request name, verb) pairs, in declared order.
    requests: Vec<(String, Vec<(String, String)>)>,
    /// Host keys some interaction reads.
    pub host_reads: Vec<String>,
}

fn refuse(message: String, range: SourceRange, origin: Option<Origin>) -> AndroidSceneError {
    AndroidSceneError {
        reason: AndroidSceneReason::Link(message),
        origin,
        range,
    }
}

/// Tokens whose bytes lie inside `range`.
fn tokens_in(tokens: &[Token], range: SourceRange) -> &[Token] {
    let start = tokens.partition_point(|t| t.byte_start < range.start);
    let end = tokens.partition_point(|t| t.byte_end <= range.end);
    &tokens[start..end.max(start)]
}

/// Range of a top-level member of a `@link` body whose leading words are
/// `head` (for example `["protocol"]`, `["field", "models"]`, `["slot",
/// "main"]`). A `key: value` member ends with its value; a slot with its
/// closing brace. `value_only` returns just the value token's range.
fn member_range(tokens: &[Token], link: SourceRange, head: &[&str], value_only: bool) -> Option<SourceRange> {
    let body = tokens_in(tokens, link);
    let mut depth = 0usize;
    for (i, t) in body.iter().enumerate() {
        match t.kind {
            TokenKind::LBrace => depth += 1,
            TokenKind::RBrace => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth != 1 || t.kind != TokenKind::Ident {
            continue;
        }
        let matches = head
            .iter()
            .enumerate()
            .all(|(k, word)| body.get(i + k).is_some_and(|x| x.kind == TokenKind::Ident && x.text == *word));
        if !matches {
            continue;
        }
        let after = i + head.len();
        let end = match body.get(after).map(|x| x.kind) {
            Some(TokenKind::Colon) => match body.get(after + 1) {
                Some(v) if v.kind == TokenKind::Str => {
                    if value_only {
                        return Some(SourceRange { start: v.byte_start, end: v.byte_end });
                    }
                    v.byte_end
                }
                // `state . key`
                Some(_) => body.get(after + 3).map_or(t.byte_end, |k| k.byte_end),
                None => t.byte_end,
            },
            Some(TokenKind::LBrace) => {
                let mut d = 0usize;
                let mut close = t.byte_end;
                for x in &body[after..] {
                    match x.kind {
                        TokenKind::LBrace => d += 1,
                        TokenKind::RBrace => {
                            d -= 1;
                            if d == 0 {
                                close = x.byte_end;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                close
            }
            _ => body[after - 1].byte_end,
        };
        return Some(SourceRange { start: t.byte_start, end });
    }
    None
}

/// Source ranges of the `request "..."` literals of one interaction, in
/// order; `None` for a request whose literal cannot be located exactly.
fn request_ranges(tokens: &[Token], interaction: SourceRange, texts: &[&str]) -> Vec<Option<SourceRange>> {
    let body = tokens_in(tokens, interaction);
    let found: Vec<&Token> = body
        .windows(2)
        .filter(|w| w[0].kind == TokenKind::Ident && w[0].text == "request" && w[1].kind == TokenKind::Str)
        .map(|w| &w[1])
        .collect();
    texts
        .iter()
        .enumerate()
        .map(|(k, text)| {
            found
                .get(k)
                .filter(|t| found.len() == texts.len() && t.text == *text)
                .map(|t| SourceRange { start: t.byte_start, end: t.byte_end })
        })
        .collect()
}

/// A validated endpoint: HTTPS to any DNS name or IP literal, or HTTP on
/// loopback only. No user info, query or fragment; a path is allowed (a
/// reverse-proxy prefix) but not `.` or `..` segments.
pub(crate) fn parse_endpoint(endpoint: &str) -> Result<(String, bool), &'static str> {
    if endpoint.len() > 2048 {
        return Err("the endpoint is longer than 2048 bytes");
    }
    let (https, rest) = if let Some(rest) = endpoint.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = endpoint.strip_prefix("http://") {
        (false, rest)
    } else {
        return Err("the endpoint must start with https:// (or http:// on loopback)");
    };
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    if authority.contains('@') {
        return Err("the endpoint must not carry user info; credentials never belong in the scene");
    }
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (addr, after) = v6.split_once(']').ok_or("an IPv6 host needs its closing bracket")?;
        if addr.is_empty() || !addr.bytes().all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.') {
            return Err("the IPv6 host is not a literal address");
        }
        match after {
            "" => (authority, None),
            p => (&authority[..authority.len() - p.len()], Some(p.strip_prefix(':').ok_or("expected :port after the IPv6 host")?)),
        }
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        }
    };
    if let Some(port) = port {
        if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) || port.starts_with('0') || port.parse::<u32>().map_or(true, |p| p == 0 || p > 65535) {
            return Err("the endpoint port must be 1 to 65535");
        }
    }
    if !host.starts_with('[') {
        let labels_ok = !host.is_empty()
            && host.len() <= 253
            && host.split('.').all(|l| {
                !l.is_empty()
                    && l.len() <= 63
                    && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    && !l.starts_with('-')
                    && !l.ends_with('-')
            });
        if !labels_ok {
            return Err("the endpoint host must be a DNS name or an IP literal");
        }
    }
    let loopback = matches!(host, "127.0.0.1" | "localhost");
    if !https && !loopback {
        return Err("plain HTTP is allowed only on loopback (127.0.0.1 or localhost); use https:// with a certificate the phone can verify");
    }
    let path = path.trim_end_matches('/');
    if !path.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'.' | b'_' | b'~'))
        || path.split('/').skip(1).any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err("the endpoint path may hold only letters, digits and - . _ ~ segments, with no query, fragment, empty, . or .. segments");
    }
    let normalized = format!("{}://{}{}", if https { "https" } else { "http" }, authority, path);
    Ok((normalized, !https))
}

fn type_name(t: InteractionType) -> &'static str {
    match t {
        InteractionType::Int => "int",
        InteractionType::Text => "text",
        InteractionType::Bool => "bool",
    }
}

/// Check the scene's link and every D2 request. `Ok(None)` when the scene
/// has neither. Every refusal carries the exact range of its construct.
pub(crate) fn check(handoff: &SemanticHandoffV2, source: &str) -> Result<Option<LinkPlan>, AndroidSceneError> {
    let parsed = handoff.parsed();
    let whole = SourceRange { start: 0, end: source.len() };
    // The source was admitted, so it lexes.
    let tokens = goya_lang::lex(source).unwrap_or_default();
    let links = &parsed.declarations.links;
    if let Some(second) = links.get(1) {
        return Err(refuse(format!("a second @link ({}); Aragon lowers one link per scene", second.name), second.range, None));
    }
    let link: Option<&LinkDecl> = links.first();
    let host_decls: Vec<_> = handoff.state().iter().filter(|d| d.lifetime == StateLifetime::Host).collect();
    if link.is_none() {
        if let Some(d) = host_decls.first() {
            return Err(AndroidSceneError {
                reason: AndroidSceneReason::Unsupported("host state without a @link that writes it"),
                origin: Some(Origin::State(d.key.clone())),
                range: handoff.origin(&Origin::State(d.key.clone())).unwrap_or(d.range),
            });
        }
    }
    let mut plan = None;
    if let Some(l) = link {
        let at = |head: &[&str], value_only: bool| member_range(&tokens, l.range, head, value_only).unwrap_or(l.range);
        if l.protocol != BLOOM_LINK_PROTOCOL {
            return Err(refuse(
                format!("link {} speaks {}; Aragon lowers only {BLOOM_LINK_PROTOCOL}", l.name, l.protocol),
                at(&["protocol"], true),
                None,
            ));
        }
        let (endpoint, cleartext_loopback) = parse_endpoint(&l.endpoint)
            .map_err(|why| refuse(format!("link {}: {why}", l.name), at(&["endpoint"], true), None))?;
        for (role, _) in &l.roles {
            if !BLOOM_LINK_ROLES.contains(&role.as_str()) {
                return Err(refuse(
                    format!("role {role} of link {}; {BLOOM_LINK_PROTOCOL} writes only status, detail and error", l.name),
                    at(&[role.as_str()], false),
                    None,
                ));
            }
        }
        if let Some(slot) = l.slots.first() {
            return Err(refuse(
                format!("image slot {} of link {}; {BLOOM_LINK_PROTOCOL} has no images", slot.name, l.name),
                at(&["slot", slot.name.as_str()], false),
                None,
            ));
        }
        if let Some(act) = l.acts.first() {
            return Err(refuse(
                format!("act {act} of link {}; {BLOOM_LINK_PROTOCOL} requests are fixed (refresh, pause, resume, credentials, forget)", l.name),
                at(&["act", act.as_str()], false),
                None,
            ));
        }
        for (field, key) in &l.fields {
            let range = at(&["field", field.as_str()], false);
            let Some((_, ty)) = BLOOM_LINK_FIELDS.iter().find(|(f, _)| f == field) else {
                let known: Vec<_> = BLOOM_LINK_FIELDS.iter().map(|(f, _)| *f).collect();
                return Err(refuse(
                    format!("field {field} of link {} is not a {BLOOM_LINK_PROTOCOL} field ({})", l.name, known.join(", ")),
                    range,
                    None,
                ));
            };
            let declared = handoff.state().iter().find(|d| d.key == *key).map(|d| d.value_type);
            if declared != Some(*ty) {
                return Err(refuse(
                    format!("field {field} of link {} is {}, but state.{key} is not", l.name, type_name(*ty)),
                    range,
                    None,
                ));
            }
        }
        plan = Some(LinkPlan {
            link: AndroidLink {
                protocol: BLOOM_LINK_PROTOCOL.to_owned(),
                name: l.name.clone(),
                endpoint,
                cleartext_loopback,
            },
            roles: l.roles.clone(),
            fields: l.fields.clone(),
            requests: Vec::new(),
            host_reads: Vec::new(),
        });
    }
    for interaction in &parsed.scene.interactions {
        let id = interaction.authored_id.clone().unwrap_or_default();
        let origin = Origin::Interaction(id.clone());
        let declared = handoff.origin(&origin).unwrap_or(whole);
        if interaction.class != InteractionClass::D2 {
            continue;
        }
        let texts: Vec<&str> = interaction.requests.iter().map(|r| r.request_utf8.as_str()).collect();
        let ranges = request_ranges(&tokens, declared, &texts);
        let mut verbs = Vec::new();
        for (r, range) in interaction.requests.iter().zip(ranges) {
            let range = range.unwrap_or(declared);
            let words: Vec<&str> = r.request_utf8.split(' ').collect();
            let Some(plan) = plan.as_ref().filter(|p| words.first() == Some(&p.link.name.as_str())) else {
                return Err(refuse(
                    format!(
                        "request \"{}\" in {} is addressed to no @link; on Android D2 requests are served only by a {BLOOM_LINK_PROTOCOL} link named by the first word",
                        r.request_utf8, interaction.name
                    ),
                    range,
                    None,
                ));
            };
            if r.effect_kind.as_str() != "network" || r.capability != "network" || r.result_type != EffectValueType::Text {
                return Err(refuse(
                    format!("requests to link {} are network effects with a text result ({})", plan.link.name, interaction.name),
                    range,
                    None,
                ));
            }
            match words.as_slice() {
                [_, verb] if BLOOM_LINK_REQUESTS.contains(verb) => verbs.push((r.name.clone(), (*verb).to_owned())),
                _ => {
                    return Err(refuse(
                        format!(
                            "request \"{}\" in {} is not a {BLOOM_LINK_PROTOCOL} request ({} followed by one of {})",
                            r.request_utf8,
                            interaction.name,
                            plan.link.name,
                            BLOOM_LINK_REQUESTS.join(", ")
                        ),
                        range,
                        None,
                    ))
                }
            }
        }
        if let Some(plan) = plan.as_mut() {
            plan.requests.push((id, verbs));
        }
    }
    if let Some(plan) = plan.as_mut() {
        let mut reads: Vec<String> = parsed
            .scene
            .interactions
            .iter()
            .flat_map(|i| &i.state_reads)
            .filter(|k| host_decls.iter().any(|d| d.key == k.name))
            .map(|k| k.name.clone())
            .collect();
        reads.sort();
        reads.dedup();
        plan.host_reads = reads;
    }
    Ok(plan)
}

/// The scene as the canonical D1 runtime sees it: each D2 interaction
/// becomes a D1 transition whose effect results are the inputs
/// `effect.<request>` (text). The runtime then checks them like any input
/// (type, UTF-8, the 64 KiB limit) and applies the authored assignments
/// with D1's checked, atomic semantics.
pub(crate) fn lower_d2_for_runtime(parsed: &ParsedScene) -> ParsedScene {
    fn rewrite(e: &InteractionExpr) -> InteractionExpr {
        match e {
            InteractionExpr::Effect(name) => InteractionExpr::Input(format!("{EFFECT_INPUT_PREFIX}{name}")),
            InteractionExpr::Add(a, b) => InteractionExpr::Add(Box::new(rewrite(a)), Box::new(rewrite(b))),
            other => other.clone(),
        }
    }
    let mut lowered = parsed.clone();
    for i in &mut lowered.scene.interactions {
        if i.class != InteractionClass::D2 {
            continue;
        }
        for r in &i.requests {
            let value_type = match r.result_type {
                EffectValueType::Int => InteractionType::Int,
                EffectValueType::Text => InteractionType::Text,
                EffectValueType::Bool => InteractionType::Bool,
            };
            i.inputs.push(TypedKey { name: format!("{EFFECT_INPUT_PREFIX}{}", r.name), value_type });
        }
        for a in &mut i.assignments {
            a.value = rewrite(&a.value);
        }
        i.class = InteractionClass::D1;
        i.requests.clear();
        i.declared_effects.clear();
    }
    lowered
}

fn kotlin_value(v: &InteractionValue) -> String {
    match v {
        InteractionValue::Int(i) if *i == i64::MIN => "Long.MIN_VALUE".to_owned(),
        InteractionValue::Int(i) => format!("{i}L"),
        InteractionValue::Bool(b) => b.to_string(),
        InteractionValue::Text(t) => crate::kotlin::kotlin_string(t),
    }
}

/// `GoyaLinkModel.kt` (pure Kotlin) and `GoyaLink.kt` (the Android runtime
/// with the per-app `GoyaLinkConfig`), with source-map entries for the
/// configuration.
pub(crate) fn emit(plan: &LinkPlan, handoff: &SemanticHandoffV2, package: &str) -> Vec<(KotlinFile, Vec<SourceMapEntry>)> {
    let model = KotlinFile {
        name: "GoyaLinkModel.kt".to_owned(),
        text: format!("package {package}\n{}", MODEL_KT.replace("__CONTRACT__", BLOOM_LINK_CONTRACT).replace("__PROTOCOL__", BLOOM_LINK_PROTOCOL)),
    };
    let mut w = KotlinWriter::new("GoyaLink.kt");
    w.code("package ");
    for (k, segment) in package.split('.').enumerate() {
        if k > 0 {
            w.code(".");
        }
        w.ident(segment);
    }
    w.code("\n");
    w.code(LINK_IMPORTS);
    w.code("\n/** Generated by Aragon (aragon.bloomlink/0.0.1) from the scene's @link: the only part of the link runtime that varies per app. Holds no credentials. */\nobject GoyaLinkConfig {\n    const val NAME: String = ")
        .string(&plan.link.name)
        .code("\n    const val ENDPOINT: String = ")
        .string(&plan.link.endpoint)
        .code("\n    /** Role -> host key. */\n    val ROLES: Map<String, String> = mapOf(");
    for (k, (role, key)) in plan.roles.iter().enumerate() {
        if k > 0 {
            w.code(", ");
        }
        w.mapped(Origin::State(key.clone()), |w| {
            w.string(role).code(" to ").string(key);
        });
    }
    w.code(")\n    /** Field -> host key. */\n    val FIELDS: Map<String, String> = mapOf(");
    for (k, (field, key)) in plan.fields.iter().enumerate() {
        if k > 0 {
            w.code(", ");
        }
        w.mapped(Origin::State(key.clone()), |w| {
            w.string(field).code(" to ").string(key);
        });
    }
    w.code(")\n    /** Every host key with its authored initial value, which also fixes its type. */\n    val HOST: Map<String, Any> = linkedMapOf(");
    let mut first = true;
    for d in handoff.state().iter().filter(|d| d.lifetime == StateLifetime::Host) {
        if !first {
            w.code(", ");
        }
        first = false;
        w.mapped(Origin::State(d.key.clone()), |w| {
            w.string(&d.key).code(" to ").generated(&kotlin_value(&d.initial));
        });
    }
    w.code(")\n    /** D2 action ID -> (request name, verb) pairs, in declared order. */\n    val REQUESTS: Map<String, Array<Array<String>>> = mapOf(");
    for (k, (action, verbs)) in plan.requests.iter().enumerate() {
        if k > 0 {
            w.code(", ");
        }
        w.mapped(Origin::Interaction(action.clone()), |w| {
            w.string(action).code(" to arrayOf(");
            for (n, (name, verb)) in verbs.iter().enumerate() {
                if n > 0 {
                    w.code(", ");
                }
                w.code("arrayOf(").string(name).code(", ").string(verb).code(")");
            }
            w.code(")");
        });
    }
    w.code(")\n}\n");
    w.code(LINK_KT);
    let (link, entries) = w.finish(handoff);
    vec![(model, Vec::new()), (link, entries)]
}

const LINK_IMPORTS: &str = "
import android.app.AlertDialog
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.text.InputType
import android.util.Base64
import android.view.View
import android.view.WindowManager
import android.view.inputmethod.EditorInfo
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.security.GeneralSecurityException
import java.security.KeyStore
import java.security.MessageDigest
import java.security.cert.CertificateException
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.util.concurrent.Executors
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import javax.net.ssl.HttpsURLConnection
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLException
import javax.net.ssl.SSLSocketFactory
import javax.net.ssl.TrustManagerFactory
";

/// The Android half of the runtime: host state, HTTP over verified TLS,
/// the Keystore-sealed token, the credentials dialog, polling and D2.
const LINK_KT: &str = r##"
/** `host` state (generated by Aragon): written only by GoyaLink, on the main thread. */
object GoyaHost {
    private val values = LinkedHashMap<String, Any>(GoyaLinkConfig.HOST)

    /** The view to refresh after host writes; set while a scene view is attached. */
    var listener: (() -> Unit)? = null

    fun snapshot(): Map<String, Any> = LinkedHashMap(values)

    /** Write one key. A key that is not host state, or a value of another type, is refused. */
    fun set(key: String, value: Any): Boolean {
        val initial = GoyaLinkConfig.HOST[key] ?: return false
        if (initial.javaClass != value.javaClass || values[key] == value) return false
        values[key] = value
        return true
    }

    fun reset(key: String): Boolean = GoyaLinkConfig.HOST[key]?.let { set(key, it) } ?: false

    fun changed() { listener?.invoke() }
}

/**
 * bloom.link/0 for Android (generated by Aragon, aragon.bloomlink/0.0.1).
 * Compiler output, identical for every app apart from GoyaLinkConfig.
 *
 * Network calls run on one background thread; every state change happens on
 * the main thread. A failure never throws out of this object: it becomes the
 * status, detail and error roles, and a failed D2 request is reported while
 * scene state stays as it was.
 */
object GoyaLink {
    private const val PREFS = "goya.link.v1"
    private const val KEY_ALIAS = "goya.link.v1.token"
    private const val POLL_MS = 5000L
    private const val MAX_BACKOFF_MS = 30000L
    private val main = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadExecutor { r -> Thread(r, "goya-link").apply { isDaemon = true } }
    private val where = GoyaLinkModel.where(GoyaLinkConfig.ENDPOINT)
    private val poller = Runnable { poll() }
    private var app: Context? = null
    private var attached: View? = null
    private var polling = false
    private var inFlight = false
    private var delayMs = POLL_MS
    private var dialog: AlertDialog? = null
    private var trustPem: String? = null
    private var trustFactory: SSLSocketFactory? = null

    /** Node ID shown after the last successful refresh; `resume` acts only on it. */
    private var shownNodeId: String? = null

    /** A D2 activation is running; the scene refuses further activations (D12). */
    var busy = false
        private set

    private class LinkFailure(val status: Long, message: String) : Exception(message)
    private class Outcome(val status: Long, val error: String, val fields: Map<String, Any>?, val nodeId: String?)

    fun attach(view: View, refresh: () -> Unit) {
        app = view.context.applicationContext
        attached = view
        GoyaHost.listener = refresh
        if (!polling) {
            polling = true
            main.post(poller)
        }
    }

    fun detach(view: View) {
        if (attached !== view) return
        attached = null
        GoyaHost.listener = null
        polling = false
        main.removeCallbacks(poller)
        // A dialog cannot outlive its activity; closing it cancels its request.
        dialog?.dismiss()
    }

    private fun poke() {
        delayMs = POLL_MS
        main.removeCallbacks(poller)
        if (polling) main.post(poller)
    }

    private fun poll() {
        val ctx = app ?: return
        if (!polling) return
        if (inFlight || busy) { main.postDelayed(poller, delayMs); return }
        inFlight = true
        if (GoyaLinkConfig.ROLES["status"]?.let { GoyaHost.snapshot()[it] } == GoyaLinkModel.IDLE) apply(Outcome(GoyaLinkModel.CONNECTING, "", null, null))
        worker.execute {
            val o = guarded { fetch(ctx) }
            main.post {
                inFlight = false
                apply(o)
                if (polling) { main.removeCallbacks(poller); main.postDelayed(poller, delayMs) }
            }
        }
    }

    /** Run a D2 activation's requests in declared order. */
    fun perform(view: View, requests: Array<Array<String>>, fail: (String) -> Unit, done: (Map<String, String>) -> Unit) {
        if (busy) { fail("another action is still running"); return }
        busy = true
        val ctx = view.context
        app = ctx.applicationContext
        val results = LinkedHashMap<String, String>()
        fun step(k: Int) {
            if (k == requests.size) { busy = false; done(results); return }
            val finish: (String?, String?) -> Unit = { value, error ->
                if (value == null) { busy = false; fail(error ?: "the request failed") }
                else { results[requests[k][0]] = value; step(k + 1) }
            }
            when (val verb = requests[k][1]) {
                "credentials" -> credentials(ctx, finish)
                else -> request(ctx.applicationContext, verb, finish)
            }
        }
        step(0)
    }

    private fun request(ctx: Context, verb: String, finish: (String?, String?) -> Unit) {
        val expected = shownNodeId
        worker.execute {
            var answer: String? = null
            val o = guarded {
                when (verb) {
                    "refresh" -> { val r = fetch(ctx); if (r.fields != null) answer = "Status refreshed"; r }
                    "pause" -> { drain(ctx, true); answer = "New inference paused"; fetch(ctx) }
                    "resume" -> {
                        if (expected == null) throw LinkFailure(GoyaLinkModel.ERROR, "refresh first: resume acts only on the node whose status is shown")
                        val id = GoyaLinkModel.nodeId(call(ctx, "GET", "/bloom/v1/nodes", null))
                        if (id != expected) throw LinkFailure(GoyaLinkModel.ERROR, "$where now reports node " + GoyaLinkModel.clean(id) + ", not the node shown; refresh before resuming")
                        drain(ctx, false); answer = "Sharing resumed"; fetch(ctx)
                    }
                    "forget" -> { forget(ctx); answer = "Credentials removed"; Outcome(GoyaLinkModel.IDLE, "", null, null) }
                    else -> throw LinkFailure(GoyaLinkModel.ERROR, "unknown request $verb")
                }
            }
            main.post {
                if (verb == "forget") { for (k in GoyaLinkConfig.FIELDS.values) GoyaHost.reset(k); shownNodeId = null }
                apply(o)
                finish(answer, o.error)
                poke()
            }
        }
    }

    /** Never throws: every failure becomes an outcome. */
    private fun guarded(body: () -> Outcome): Outcome = try {
        body()
    } catch (e: LinkFailure) {
        Outcome(e.status, e.message ?: "failed", null, null)
    } catch (e: SSLException) {
        Outcome(GoyaLinkModel.UNTRUSTED, "TLS to $where failed (certificate not trusted, or TLS refused): " + GoyaLinkModel.clean(e.message ?: e.javaClass.simpleName), null, null)
    } catch (e: IOException) {
        Outcome(GoyaLinkModel.UNREACHABLE, "$where cannot be reached: " + GoyaLinkModel.clean(e.message ?: e.javaClass.simpleName), null, null)
    } catch (e: GoyaLinkModel.BadResponse) {
        Outcome(GoyaLinkModel.ERROR, "unexpected answer from $where: " + GoyaLinkModel.clean(e.message ?: ""), null, null)
    } catch (e: Exception) {
        Outcome(GoyaLinkModel.ERROR, "link failed: " + e.javaClass.simpleName, null, null)
    }

    private fun apply(o: Outcome) {
        var changed = false
        fun put(role: String, v: Any) { GoyaLinkConfig.ROLES[role]?.let { if (GoyaHost.set(it, v)) changed = true } }
        if (o.fields != null) delayMs = POLL_MS else if (o.status != GoyaLinkModel.IDLE && o.status != GoyaLinkModel.CONNECTING) delayMs = minOf(delayMs * 2, MAX_BACKOFF_MS)
        put("status", o.status)
        put("error", o.error)
        put("detail", GoyaLinkModel.detail(where, o.status, delayMs / 1000))
        o.fields?.let { f -> for ((name, v) in f) GoyaLinkConfig.FIELDS[name]?.let { if (GoyaHost.set(it, v)) changed = true } }
        if (o.nodeId != null) shownNodeId = o.nodeId
        if (changed) GoyaHost.changed()
    }

    private fun fetch(ctx: Context): Outcome {
        val wanted = GoyaLinkConfig.FIELDS.keys
        val bodies = HashMap<String, Any?>()
        for (path in GoyaLinkModel.paths(wanted)) bodies[path] = call(ctx, "GET", path, null)
        val nodes = bodies["/bloom/v1/nodes"]
        return Outcome(GoyaLinkModel.ONLINE, "", GoyaLinkModel.fields(wanted, bodies), GoyaLinkModel.nodeId(nodes))
    }

    private fun drain(ctx: Context, enabled: Boolean) {
        val answer = call(ctx, "POST", "/bloom/v1/nodes/drain", if (enabled) "{\"enabled\":true}" else "{\"enabled\":false}")
        if (GoyaLinkModel.at(answer, "draining") != enabled) throw GoyaLinkModel.BadResponse("/bloom/v1/nodes/drain did not confirm draining=$enabled")
    }

    /** One HTTP exchange on the worker thread: HTTPS (or loopback HTTP), no redirects, bounded body. */
    private fun call(ctx: Context, method: String, path: String, body: String?): Any? {
        val url = URL(GoyaLinkConfig.ENDPOINT + path)
        val loopback = url.protocol == "http" && (url.host == "127.0.0.1" || url.host == "localhost")
        if (url.protocol != "https" && !loopback) throw LinkFailure(GoyaLinkModel.ERROR, "refused: the endpoint is neither HTTPS nor loopback")
        val c = url.openConnection() as HttpURLConnection
        try {
            if (c is HttpsURLConnection) trust(ctx)?.let { c.sslSocketFactory = it }
            c.instanceFollowRedirects = false
            c.useCaches = false
            c.connectTimeout = 3000
            c.readTimeout = 5000
            c.requestMethod = method
            c.setRequestProperty("Accept", "application/json")
            token(ctx)?.let { c.setRequestProperty("Authorization", "Bearer $it") }
            if (body != null) {
                c.doOutput = true
                c.setRequestProperty("Content-Type", "application/json")
                c.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
            }
            val code = c.responseCode
            if (code !in 200..299) throw LinkFailure(GoyaLinkModel.httpStatus(code), GoyaLinkModel.httpProblem(path, code))
            val out = ByteArrayOutputStream()
            val buf = ByteArray(8192)
            c.inputStream.use { s ->
                while (true) {
                    val n = s.read(buf)
                    if (n < 0) break
                    if (out.size() + n > GoyaLinkModel.MAX_BODY_BYTES) throw GoyaLinkModel.BadResponse("$path answered with more than 1 MiB")
                    out.write(buf, 0, n)
                }
            }
            return GoyaLinkModel.parse(String(out.toByteArray(), Charsets.UTF_8))
        } finally {
            c.disconnect()
        }
    }

    private fun prefs(ctx: Context) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    /** The operator CA replaces the system anchors for this link; host names are still verified. */
    private fun trust(ctx: Context): SSLSocketFactory? {
        val pem = prefs(ctx).getString("ca", null) ?: return null
        if (pem == trustPem) return trustFactory
        val ks = KeyStore.getInstance(KeyStore.getDefaultType())
        ks.load(null, null)
        try {
            certificates(pem).forEachIndexed { i, cert -> ks.setCertificateEntry("ca$i", cert) }
        } catch (e: CertificateException) {
            throw LinkFailure(GoyaLinkModel.UNTRUSTED, "the stored CA certificate is invalid; enter it again with Credentials")
        }
        val tmf = TrustManagerFactory.getInstance(TrustManagerFactory.getDefaultAlgorithm())
        tmf.init(ks)
        val ssl = SSLContext.getInstance("TLS")
        ssl.init(null, tmf.trustManagers, null)
        trustPem = pem
        trustFactory = ssl.socketFactory
        return trustFactory
    }

    private fun certificates(pem: String): List<X509Certificate> {
        val all = CertificateFactory.getInstance("X.509").generateCertificates(ByteArrayInputStream(pem.toByteArray(Charsets.US_ASCII)))
        val certs = all.mapNotNull { it as? X509Certificate }
        if (certs.isEmpty()) throw CertificateException("no certificate")
        return certs
    }

    /** Associated data: a sealed token opens only for this link name and endpoint. */
    private fun aad(): ByteArray = ("goya.link.v1\u0000" + GoyaLinkConfig.NAME + "\u0000" + GoyaLinkConfig.ENDPOINT).toByteArray(Charsets.UTF_8)

    private fun secretKey(create: Boolean): SecretKey? {
        val ks = KeyStore.getInstance("AndroidKeyStore")
        ks.load(null)
        (ks.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        if (!create) return null
        val g = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        g.init(
            KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return g.generateKey()
    }

    private fun seal(token: String): String {
        val c = Cipher.getInstance("AES/GCM/NoPadding")
        c.init(Cipher.ENCRYPT_MODE, secretKey(true))
        c.updateAAD(aad())
        val sealed = c.doFinal(token.toByteArray(Charsets.UTF_8))
        return Base64.encodeToString(c.iv, Base64.NO_WRAP) + ":" + Base64.encodeToString(sealed, Base64.NO_WRAP)
    }

    /** The operator token, opened for one request; never logged, shown or kept in scene state. */
    private fun token(ctx: Context): String? {
        val stored = prefs(ctx).getString("token", null) ?: return null
        val unreadable = LinkFailure(GoyaLinkModel.UNAUTHORIZED, "the stored token cannot be opened on this device; enter it again with Credentials")
        val parts = stored.split(':')
        if (parts.size != 2) throw unreadable
        try {
            val key = secretKey(false) ?: throw unreadable
            val c = Cipher.getInstance("AES/GCM/NoPadding")
            c.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, Base64.decode(parts[0], Base64.NO_WRAP)))
            c.updateAAD(aad())
            return String(c.doFinal(Base64.decode(parts[1], Base64.NO_WRAP)), Charsets.UTF_8)
        } catch (e: GeneralSecurityException) {
            throw unreadable
        } catch (e: IllegalArgumentException) {
            throw unreadable
        }
    }

    private fun forget(ctx: Context) {
        prefs(ctx).edit().clear().commit()
        val ks = KeyStore.getInstance("AndroidKeyStore")
        ks.load(null)
        if (ks.containsAlias(KEY_ALIAS)) ks.deleteEntry(KEY_ALIAS)
        trustPem = null
        trustFactory = null
    }

    private fun fingerprint(cert: X509Certificate): String =
        MessageDigest.getInstance("SHA-256").digest(cert.encoded).take(8).joinToString(":") { String.format("%02X", it.toInt() and 0xff) }

    /**
     * The only way a token enters the app: a platform dialog that is not part
     * of the scene. It is kept out of screenshots and the recents thumbnail
     * (FLAG_SECURE), out of autofill and keyboard learning, and the field is
     * cleared once the token is sealed.
     */
    private fun credentials(ctx: Context, finish: (String?, String?) -> Unit) {
        val store = ctx.applicationContext
        val pad = (16 * ctx.resources.displayMetrics.density).toInt()
        val box = LinearLayout(ctx).apply { orientation = LinearLayout.VERTICAL; setPadding(pad, pad / 2, pad, 0) }
        box.addView(TextView(ctx).apply { text = "Operator token for $where. It is sealed with an Android Keystore key and never shown again. Leave it empty to keep the stored token." })
        val token = EditText(ctx).apply {
            hint = "Bearer token"
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or 0x1000000 // IME_FLAG_NO_PERSONALIZED_LEARNING
            if (Build.VERSION.SDK_INT >= 26) importantForAutofill = View.IMPORTANT_FOR_AUTOFILL_NO
        }
        val ca = EditText(ctx).apply {
            hint = "Optional: LAN CA certificate (PEM); replaces system trust for this link"
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            minLines = 3
            maxLines = 6
            setText(prefs(store).getString("ca", ""))
        }
        box.addView(token)
        box.addView(ca)
        var answered = false
        val d = AlertDialog.Builder(ctx)
            .setTitle("Connect to bloomd")
            .setView(box)
            .setPositiveButton("Save", null)
            .setNegativeButton("Cancel", null)
            .create()
        d.window?.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
        d.setOnDismissListener {
            dialog = null
            token.setText("")
            if (!answered) { answered = true; finish(null, "credentials entry cancelled") }
        }
        d.setOnShowListener {
            d.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
                val t = token.text.toString()
                val pem = ca.text.toString().trim()
                GoyaLinkModel.tokenProblem(t)?.let { token.error = it; return@setOnClickListener }
                var print: String? = null
                if (pem.isNotEmpty()) {
                    if (pem.length > 65536) { ca.error = "Above 64 KiB"; return@setOnClickListener }
                    try { print = fingerprint(certificates(pem)[0]) } catch (e: Exception) { ca.error = "Not a PEM certificate"; return@setOnClickListener }
                }
                try {
                    val edit = prefs(store).edit()
                    if (t.isNotEmpty()) edit.putString("token", seal(t))
                    if (pem.isEmpty()) edit.remove("ca") else edit.putString("ca", pem)
                    if (!edit.commit()) throw IOException("storage refused the write")
                } catch (e: Exception) {
                    token.error = "Could not store: " + e.javaClass.simpleName
                    return@setOnClickListener
                }
                trustPem = null
                trustFactory = null
                answered = true
                d.dismiss()
                finish(if (print == null) "Credentials saved" else "Credentials saved; CA SHA-256 $print", null)
                poke()
            }
        }
        dialog = d
        d.show()
    }
}
"##;

/// Pure Kotlin (no Android imports): the response model, so it runs on a
/// plain JVM in the conformance test.
pub(crate) const MODEL_KT: &str = r##"
/** Generated by Aragon (__CONTRACT__): the __PROTOCOL__ response model. Pure Kotlin, no Android imports. */
object GoyaLinkModel {
    const val PROTOCOL: String = "__PROTOCOL__"
    const val MAX_BODY_BYTES: Int = 1048576
    const val MAX_TEXT: Int = 256

    // Values of the `status` role.
    const val IDLE: Long = 0L
    const val CONNECTING: Long = 1L
    const val ONLINE: Long = 2L
    const val UNREACHABLE: Long = 3L
    const val UNAUTHORIZED: Long = 4L
    const val UNTRUSTED: Long = 5L
    const val ERROR: Long = 6L

    class BadResponse(message: String) : Exception(message)

    /** host[:port] of an endpoint, for messages. */
    fun where(endpoint: String): String = endpoint.substringAfter("://").substringBefore('/')

    fun detail(where: String, status: Long, retryS: Long): String = where + " · " + when (status) {
        IDLE -> "not contacted yet"
        CONNECTING -> "connecting"
        ONLINE -> "online"
        UNREACHABLE -> "unreachable, retrying in $retryS s"
        UNAUTHORIZED -> "token refused; use Credentials"
        UNTRUSTED -> "certificate not trusted"
        else -> "error"
    }

    fun httpStatus(code: Int): Long = if (code == 401 || code == 403) UNAUTHORIZED else ERROR

    fun httpProblem(path: String, code: Int): String = when {
        code == 401 || code == 403 -> "bloomd refused $path (HTTP $code): set the operator token with Credentials"
        code in 300..399 -> "bloomd redirected $path (HTTP $code); redirects are refused"
        else -> "bloomd answered $path with HTTP $code"
    }

    /** Null when the token is acceptable (empty keeps the stored one). */
    fun tokenProblem(token: String): String? = when {
        token.length > 4096 -> "Longer than 4096 characters"
        token.any { it < '!' || it > '~' } -> "Only printable ASCII without spaces"
        else -> null
    }

    /** Printable text of at most MAX_TEXT characters; control characters become '?'. */
    fun clean(s: String): String {
        val b = StringBuilder()
        for (c in s) {
            if (b.length >= MAX_TEXT) break
            b.append(if (c < ' ' || c == '\u007f') '?' else c)
        }
        if (b.isNotEmpty() && Character.isHighSurrogate(b[b.length - 1])) b.setLength(b.length - 1)
        return b.toString()
    }

    /** The endpoints a set of bound fields needs; /bloom/v1/nodes always. */
    fun paths(wanted: Set<String>): List<String> {
        val out = arrayListOf("/bloom/v1/nodes")
        if ("cpu_percent" in wanted || "memory_free_mib" in wanted) out.add("/bloom/v1/resources")
        if ("models" in wanted || "model_count" in wanted) out.add("/v1/models")
        if ("peers" in wanted) out.add("/bloom/v1/swarm")
        if ("nearby" in wanted) out.add("/bloom/v1/discovery")
        return out
    }

    fun at(v: Any?, vararg path: Any): Any? {
        var cur = v
        for (p in path) cur = when (p) {
            is String -> (cur as? Map<*, *>)?.get(p)
            is Int -> (cur as? List<*>)?.getOrNull(p)
            else -> null
        }
        return cur
    }

    private fun need(v: Any?, what: String): Any = v ?: throw BadResponse("no $what")
    private fun text(v: Any?, what: String): String = clean(need(v, what) as? String ?: throw BadResponse("$what is not text"))
    private fun bool(v: Any?, what: String): Boolean = need(v, what) as? Boolean ?: throw BadResponse("$what is not true or false")
    private fun whole(v: Any?, what: String): Long {
        val d = need(v, what) as? Double ?: throw BadResponse("$what is not a number")
        if (!(d >= 0.0 && d <= 9.0E15)) throw BadResponse("$what is out of range")
        return Math.floor(d).toLong()
    }
    private fun count(v: Any?, what: String): Long = (need(v, what) as? List<*> ?: throw BadResponse("$what is not a list")).size.toLong()

    fun nodeId(nodes: Any?): String = text(at(nodes, "nodes", 0, "descriptor", "id"), "node id in /bloom/v1/nodes")

    /** Summary of a node, worded as bloom-node-app words it. */
    fun nodeState(draining: Boolean, runnerOk: Boolean): String = when {
        draining -> "Paused; not accepting new work"
        !runnerOk -> "Online; model runner is unavailable"
        else -> "Online; ready to accept work"
    }

    /** Field values from the parsed bodies of [paths]. */
    fun fields(wanted: Set<String>, bodies: Map<String, Any?>): Map<String, Any> {
        val node = at(bodies["/bloom/v1/nodes"], "nodes", 0)
        val res = bodies["/bloom/v1/resources"]
        val models = bodies["/v1/models"]
        val out = LinkedHashMap<String, Any>()
        for (f in wanted) out[f] = when (f) {
            "node_id" -> text(at(node, "descriptor", "id"), "node id")
            "node_name" -> text(at(node, "descriptor", "name"), "node name")
            "draining" -> bool(at(node, "status", "draining"), "status.draining")
            "runner_ok" -> bool(at(node, "status", "harness_healthy"), "status.harness_healthy")
            "node_state" -> nodeState(bool(at(node, "status", "draining"), "status.draining"), bool(at(node, "status", "harness_healthy"), "status.harness_healthy"))
            "active_jobs" -> whole(at(node, "status", "active_jobs"), "status.active_jobs")
            "free_slots" -> whole(at(node, "scheduling", "available_slots"), "scheduling.available_slots")
            "uptime_s" -> whole(at(node, "status", "uptime_seconds"), "status.uptime_seconds")
            "cpu_percent" -> whole((need(at(res, "cpu", "utilisation_percent"), "cpu.utilisation_percent") as? Double ?: throw BadResponse("cpu.utilisation_percent is not a number")) + 0.5, "cpu.utilisation_percent")
            "memory_free_mib" -> whole(at(res, "memory", "available_bytes"), "memory.available_bytes") / 1048576L
            "models" -> modelNames(at(models, "data"))
            "model_count" -> count(at(models, "data"), "/v1/models data")
            "peers" -> count(at(bodies["/bloom/v1/swarm"], "peers"), "swarm peers")
            "nearby" -> count(at(bodies["/bloom/v1/discovery"], "candidates"), "discovery candidates")
            else -> throw BadResponse("unknown field $f")
        }
        return out
    }

    private fun modelNames(data: Any?): String {
        val list = need(data, "/v1/models data") as? List<*> ?: throw BadResponse("/v1/models data is not a list")
        if (list.isEmpty()) return "none"
        val names = list.take(3).map { m -> clean(text(at(m, "id"), "model id").take(40)) }
        return names.joinToString(", ") + if (list.size > 3) " +" + (list.size - 3) + " more" else ""
    }

    /** A bounded JSON reader (objects, arrays, strings, numbers, literals; depth <= 32). */
    fun parse(text: String): Any? {
        val p = Json(text)
        val v = p.value(0)
        p.ws()
        if (p.i != text.length) throw BadResponse("trailing data after JSON")
        return v
    }

    private class Json(val s: String) {
        var i = 0

        fun ws() { while (i < s.length && (s[i] == ' ' || s[i] == '\n' || s[i] == '\r' || s[i] == '\t')) i++ }

        fun expect(c: Char) { if (i >= s.length || s[i] != c) throw BadResponse("invalid JSON at $i"); i++ }

        fun value(depth: Int): Any? {
            if (depth > 32) throw BadResponse("JSON nested too deeply")
            ws()
            if (i >= s.length) throw BadResponse("truncated JSON")
            return when (s[i]) {
                '{' -> obj(depth)
                '[' -> arr(depth)
                '"' -> str()
                't' -> word("true", true)
                'f' -> word("false", false)
                'n' -> word("null", null)
                else -> num()
            }
        }

        fun word(w: String, v: Any?): Any? {
            if (!s.startsWith(w, i)) throw BadResponse("invalid JSON at $i")
            i += w.length
            return v
        }

        fun obj(depth: Int): Map<String, Any?> {
            val m = LinkedHashMap<String, Any?>()
            expect('{')
            ws()
            if (i < s.length && s[i] == '}') { i++; return m }
            while (true) {
                ws()
                val k = str()
                ws()
                expect(':')
                m[k] = value(depth + 1)
                ws()
                if (i < s.length && s[i] == ',') { i++; continue }
                expect('}')
                return m
            }
        }

        fun arr(depth: Int): List<Any?> {
            val a = ArrayList<Any?>()
            expect('[')
            ws()
            if (i < s.length && s[i] == ']') { i++; return a }
            while (true) {
                a.add(value(depth + 1))
                ws()
                if (i < s.length && s[i] == ',') { i++; continue }
                expect(']')
                return a
            }
        }

        fun str(): String {
            expect('"')
            val b = StringBuilder()
            while (true) {
                if (i >= s.length) throw BadResponse("unterminated JSON string")
                val c = s[i++]
                when {
                    c == '"' -> return b.toString()
                    c == '\\' -> {
                        if (i >= s.length) throw BadResponse("unterminated JSON escape")
                        when (val e = s[i++]) {
                            '"', '\\', '/' -> b.append(e)
                            'b' -> b.append('\b')
                            'f' -> b.append('\u000c')
                            'n' -> b.append('\n')
                            'r' -> b.append('\r')
                            't' -> b.append('\t')
                            'u' -> {
                                if (i + 4 > s.length) throw BadResponse("short \\u escape")
                                val h = s.substring(i, i + 4)
                                if (!h.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }) throw BadResponse("invalid \\u escape")
                                b.append(h.toInt(16).toChar())
                                i += 4
                            }
                            else -> throw BadResponse("invalid JSON escape")
                        }
                    }
                    c < ' ' -> throw BadResponse("control character in a JSON string")
                    else -> b.append(c)
                }
            }
        }

        fun num(): Double {
            val start = i
            if (i < s.length && s[i] == '-') i++
            while (i < s.length && (s[i] in '0'..'9' || s[i] == '.' || s[i] == 'e' || s[i] == 'E' || s[i] == '+' || s[i] == '-')) i++
            if (i == start) throw BadResponse("invalid JSON at $i")
            return s.substring(start, i).toDoubleOrNull() ?: throw BadResponse("invalid JSON number")
        }
    }
}
"##;

#[cfg(test)]
mod tests {
    use super::parse_endpoint;

    #[test]
    fn endpoints_are_https_or_loopback_http_without_credentials() {
        assert_eq!(parse_endpoint("https://bloom-node.local:7420"), Ok(("https://bloom-node.local:7420".into(), false)));
        assert_eq!(parse_endpoint("https://192.168.1.20:7420/"), Ok(("https://192.168.1.20:7420".into(), false)));
        assert_eq!(parse_endpoint("https://[fd00::2]:7420/bloom"), Ok(("https://[fd00::2]:7420/bloom".into(), false)));
        assert_eq!(parse_endpoint("http://127.0.0.1:7420"), Ok(("http://127.0.0.1:7420".into(), true)));
        assert_eq!(parse_endpoint("http://localhost:7420"), Ok(("http://localhost:7420".into(), true)));
        for bad in [
            "http://192.168.1.20:7420",
            "http://bloom-node.local:7420",
            "https://user:secret@bloom-node.local:7420",
            "https://bloom-node.local:7420/?token=abc",
            "https://bloom-node.local:7420/#x",
            "https://bloom-node.local:0",
            "https://bloom-node.local:70000",
            "https://bloom-node.local:07420",
            "https://bloom_node.local",
            "https://bloom-node.local/a/../b",
            "https://",
            "ftp://bloom-node.local",
            "https://[fd00::2",
        ] {
            assert!(parse_endpoint(bad).is_err(), "{bad}");
        }
    }
}
