//! Select one GSSH handoff and refuse what that handoff leaves unresolved.
//!
//! The resolver does not parse a private dialect. An explicit contract, or the
//! source's `// profile:` comment, selects the handoff. Without either, the
//! strictest handoff that admits the source is used.

use goya_lang::handoff::{self, SemanticHandoffV2};
use goya_lang::{InputBinding, ParsedScene, Scene, StateDecl, StrictParseError};

use crate::diagnostics::Diagnostic;

/// Semantic contract implemented by [`goya_lang::handoff`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticContract {
    Gssh1,
    Gssh2,
    Gssh3,
    Gssh3Live,
    Gssh4,
    Gssh4Live,
}

impl SemanticContract {
    pub fn label(self) -> &'static str {
        match self {
            Self::Gssh1 => handoff::CONTRACT,
            Self::Gssh2 => handoff::CONTRACT_V2,
            Self::Gssh3 => handoff::CONTRACT_V3,
            Self::Gssh3Live => handoff::CONTRACT_V3_LIVE,
            Self::Gssh4 => handoff::CONTRACT_V4,
            Self::Gssh4Live => handoff::CONTRACT_V4_LIVE,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "gssh1" | "GSSH/1" => Some(Self::Gssh1),
            "gssh2" | "GSSH/2" => Some(Self::Gssh2),
            "gssh3" | "GSSH/3" => Some(Self::Gssh3),
            "gssh3-live" | "GSSH/3-live" => Some(Self::Gssh3Live),
            "gssh4" | "GSSH/4" => Some(Self::Gssh4),
            "gssh4-live" | "GSSH/4-live" => Some(Self::Gssh4Live),
            _ => None,
        }
    }
}

const AUTO: [SemanticContract; 6] = [
    SemanticContract::Gssh4Live,
    SemanticContract::Gssh4,
    SemanticContract::Gssh3Live,
    SemanticContract::Gssh3,
    SemanticContract::Gssh2,
    SemanticContract::Gssh1,
];

pub(crate) struct Admitted {
    pub contract: &'static str,
    pub scene: Scene,
    pub digest: [u8; 32],
    pub state: Vec<StateDecl>,
    pub bindings: Vec<InputBinding>,
    pub state_uses: Vec<goya_lang::KeyUse>,
    pub input_keys: Vec<goya_lang::KeyUse>,
}

enum Header {
    Absent,
    One(SemanticContract),
    Rejected(String),
}

pub(crate) fn admit(
    source: &str,
    requested: Option<SemanticContract>,
    target: Option<crate::Target>,
) -> Result<Admitted, Diagnostic> {
    match (profile_header(source), requested) {
        (Header::Rejected(text), _) => Err(Diagnostic::new(
            "GY-RESOLVER-PROFILE",
            format!("profile comment `{text}` is not a single GSSH contract"),
        )),
        (Header::One(declared), Some(asked)) if asked != declared => Err(Diagnostic::new(
            "GY-RESOLVER-PROFILE",
            format!(
                "source declares {} but the resolver was asked for {}",
                declared.label(),
                asked.label()
            ),
        )),
        (Header::One(declared), _) => admit_one(declared, source, target),
        (Header::Absent, None) => admit_auto(source, target),
        (Header::Absent, Some(asked)) => admit_one(asked, source, target),
    }
}

fn admit_one(
    contract: SemanticContract,
    source: &str,
    target: Option<crate::Target>,
) -> Result<Admitted, Diagnostic> {
    match parse_only(contract, source) {
        Ok(parsed) => finish(contract, parsed, target),
        Err(error) => Err(strict_diagnostic(&error)),
    }
}

fn admit_auto(source: &str, target: Option<crate::Target>) -> Result<Admitted, Diagnostic> {
    let mut failures = Vec::new();
    for contract in AUTO {
        match parse_only(contract, source) {
            Ok(parsed) => return finish(contract, parsed, target),
            Err(error) => failures.push(error),
        }
    }
    Err(choose(&failures))
}

fn parse_only(contract: SemanticContract, source: &str) -> Result<ParsedScene, StrictParseError> {
    match contract {
        SemanticContract::Gssh1 => Ok(handoff::admit(source)?.parsed.clone()),
        SemanticContract::Gssh2 => parsed_v2(handoff::admit_v2(source)?),
        SemanticContract::Gssh3 => parsed_v2(handoff::admit_v3(source)?),
        SemanticContract::Gssh3Live => parsed_v2(handoff::admit_v3_live(source)?),
        SemanticContract::Gssh4 => parsed_v2(handoff::admit_v4(source)?),
        SemanticContract::Gssh4Live => parsed_v2(handoff::admit_v4_live(source)?),
    }
}

fn parsed_v2(handoff: SemanticHandoffV2) -> Result<ParsedScene, StrictParseError> {
    Ok(handoff.parsed().clone())
}

fn finish(
    contract: SemanticContract,
    parsed: ParsedScene,
    target: Option<crate::Target>,
) -> Result<Admitted, Diagnostic> {
    let offending = match target {
        None => parsed.declarations.platform_exceptions.first(),
        Some(selected) => parsed
            .declarations
            .platform_exceptions
            .iter()
            .find(|rule| rule.platform == selected.id()),
    };
    if let Some(rule) = offending {
        let message = match target {
            None => {
                let count = parsed.declarations.platform_exceptions.len();
                format!(
                    "{count} unresolved platform exception(s); the first is {} feature {}",
                    rule.platform, rule.feature
                )
            }
            Some(selected) => format!(
                "unresolved {} platform exception for '{}'",
                selected.id(),
                rule.feature
            ),
        };
        return Err(Diagnostic::new("GY-RESOLVER-PLATFORM", message).with_range(rule.range));
    }
    let state = parsed
        .declarations
        .state
        .as_ref()
        .map(|(decls, _)| decls.clone())
        .unwrap_or_default();
    let bindings = parsed.declarations.bindings.clone();
    let state_uses = parsed.declarations.state_uses.clone();
    let input_keys = parsed.declarations.input_keys.clone();
    Ok(Admitted {
        contract: contract.label(),
        scene: parsed.scene,
        digest: parsed.source_digest,
        state,
        bindings,
        state_uses,
        input_keys,
    })
}

fn profile_header(source: &str) -> Header {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("//") else {
            return Header::Absent;
        };
        let rest = rest.trim();
        let Some(value) = rest.strip_prefix("profile:") else {
            continue;
        };
        let value = value.trim();
        return match SemanticContract::parse(value) {
            Some(contract) => Header::One(contract),
            None => Header::Rejected(value.to_owned()),
        };
    }
    Header::Absent
}

fn is_profile_gate(code: &str) -> bool {
    matches!(
        code,
        "GY-E1006" | "GY-E1009" | "GY-E1010" | "GY-E1012" | "GY-E1015"
    )
}

fn choose(failures: &[StrictParseError]) -> Diagnostic {
    let chosen = failures
        .iter()
        .rev()
        .find(|error| !is_profile_gate(error.code))
        .or_else(|| failures.first());
    match chosen {
        Some(error) => strict_diagnostic(error),
        None => Diagnostic::new("GY-RESOLVER-ADMIT", "admission produced no diagnostic"),
    }
}

fn strict_diagnostic(error: &StrictParseError) -> Diagnostic {
    Diagnostic::new(error.code, error.to_string()).with_range(error.range)
}
