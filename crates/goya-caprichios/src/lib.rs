//! Deterministic construction of a small HTTPS GET capability.
pub mod accelerator_caprichio;
pub mod amd_apu_caprichio;
pub mod amd_apu_runtime;
pub mod amd_desktop_caprichio;
pub mod amd_desktop_runtime;
pub mod amd_epyc_caprichio;
pub mod amd_epyc_runtime;
pub mod amd_threadripper_caprichio;
pub mod amd_threadripper_runtime;
pub mod apple_pay_caprichio;
pub mod apple_runtime;
pub mod apple_watch_accelerator_models;
pub mod batching_caprichio;
pub mod bluetooth_access_caprichio;
pub mod bluetooth_caprichio;
pub mod bluetooth_continuity_caprichio;
pub mod bluetooth_control_caprichio;
pub mod bluetooth_facade;
pub mod bluetooth_guard_caprichio;
pub mod bluetooth_profile_caprichio;
pub mod bluetooth_runtime;
pub mod bluetooth_stream_caprichio;
pub mod catalog;
pub mod checksum_caprichio;
pub mod colour_cue_caprichio;
pub mod compression_caprichio;
pub mod compute_caprichio;
pub mod compute_runtime;
pub mod contract_validation;
pub mod csv_caprichio;
pub mod data_caprichio;
pub mod data_runtime;
pub mod deadline_caprichio;
pub mod exynos_accelerator_caprichio;
pub mod exynos_runtime;
pub mod face_id_caprichio;
pub mod filesystem_caprichio;
pub mod google_pay_caprichio;
pub mod http_caprichio;
pub mod input_validation_caprichio;
pub mod intel_desktop_caprichio;
pub mod intel_desktop_runtime;
pub mod intel_mobile_caprichio;
pub mod intel_mobile_runtime;
pub mod intel_xeon_caprichio;
pub mod intel_xeon_runtime;
pub mod ipad_accelerator_models;
pub mod iphone_accelerator_models;
pub mod ipod_touch_accelerator_models;
pub mod keychain_caprichio;
pub mod kirin_accelerator_caprichio;
pub mod kirin_runtime;
pub mod mediatek_accelerator_caprichio;
pub mod mediatek_runtime;
pub mod memory_caprichio;
pub mod memory_runtime;
pub mod metal_caprichio;
pub mod nvidia_accelerator_caprichio;
pub mod nvidia_runtime;
pub mod parallelism_caprichio;
pub mod priority_caprichio;
pub mod progress_caprichio;
pub mod registry;
pub mod retry_caprichio;
pub mod samsung_pay_caprichio;
pub mod snapdragon_accelerator_caprichio;
pub mod snapdragon_runtime;
pub mod state_persistence;
pub mod storage;
pub mod stream_caprichio;
pub mod tensor_accelerator_caprichio;
pub mod tensor_runtime;
pub mod touch_id_caprichio;
pub mod windows_hello_caprichio;

/// Exact policy entries a target compiler has implemented. A match includes
/// the feature, rule kind, and ordered option list; a feature-only match is
/// never enough to claim that an exception was resolved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformRuleResolution {
    pub feature: String,
    pub use_kind: String,
    pub options: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformRuleRef {
    pub platform: String,
    pub feature: String,
    pub use_kind: String,
    pub options: Vec<String>,
    pub range: SourceSpan,
}

/// A source-bound target exception that is not implemented by the compiler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnresolvedPlatformRule {
    pub platform: String,
    pub feature: String,
    pub range: SourceSpan,
}

/// Check all exceptions for `platform` against the compiler's explicit rule
/// inventory. Rules for other platforms are deliberately inert. Exact option
/// matching prevents a compiler from silently accepting a variant it does not
/// implement. The target compiler converts its admitted GSSH/4 exceptions to
/// these dependency-free records and calls this immediately after admission.
pub fn resolve_platform_exceptions(
    platform: &str,
    exceptions: impl IntoIterator<Item = PlatformRuleRef>,
    inventory: &[PlatformRuleResolution],
) -> Result<(), UnresolvedPlatformRule> {
    for rule in exceptions
        .into_iter()
        .filter(|rule| rule.platform == platform)
    {
        let resolved = inventory.iter().any(|entry| {
            entry.feature == rule.feature
                && entry.use_kind == rule.use_kind
                && entry.options == rule.options
        });
        if !resolved {
            return Err(UnresolvedPlatformRule {
                platform: rule.platform,
                feature: rule.feature,
                range: rule.range,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod platform_exception_tests {
    use super::*;

    const SOURCE: &str = "@ontology app { @persistence { cache_targets: null; } @platform ios { \"media\": only-use(\"native.player\"); } @cell title { id: \"title\"; text: \"Hello\"; } }";

    fn exception() -> PlatformRuleRef {
        let start = SOURCE.find("\"media\"").unwrap();
        let end = SOURCE[start..].find(';').unwrap() + start + 1;
        PlatformRuleRef {
            platform: "ios".into(),
            feature: "media".into(),
            use_kind: "only-use".into(),
            options: vec!["native.player".into()],
            range: SourceSpan { start, end },
        }
    }

    #[test]
    fn unrelated_platform_rules_are_inert() {
        assert!(resolve_platform_exceptions("browser", [exception()], &[]).is_ok());
    }

    #[test]
    fn matching_unregistered_rule_is_refused_at_its_source_range() {
        let error = resolve_platform_exceptions("ios", [exception()], &[]).unwrap_err();
        assert_eq!(
            &SOURCE[error.range.start..error.range.end],
            "\"media\": only-use(\"native.player\");"
        );
    }

    #[test]
    fn exact_registered_rule_is_accepted() {
        let inventory = [PlatformRuleResolution {
            feature: "media".into(),
            use_kind: "only-use".into(),
            options: vec!["native.player".into()],
        }];
        assert!(resolve_platform_exceptions("ios", [exception()], &inventory).is_ok());
        let mismatched = [PlatformRuleResolution {
            options: vec!["other.player".into()],
            ..inventory[0].clone()
        }];
        assert_eq!(
            resolve_platform_exceptions("ios", [exception()], &mismatched)
                .unwrap_err()
                .feature,
            "media"
        );
    }
}

use std::fmt;

pub const CONTRACT: &str = "goya.network.fetch.v1";
pub const RECIPE_VERSION: &str = "0.0.1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Browser,
    Node,
}
/// Runtime facilities required by both recipes, in addition to ES module support.
/// Hosts must report these from their runtime, rather than infer them from a target name.
/// The default is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub fetch: bool,
    pub abort_controller: bool,
    pub url: bool,
    pub uint8_array: bool,
    /// Both `setTimeout` and `clearTimeout` must be available.
    pub timers: bool,
}

impl RuntimeCapabilities {
    fn validate(self) -> Result<(), Diagnostic> {
        for (available, message) in [
            (self.fetch, "target runtime requires fetch"),
            (
                self.abort_controller,
                "target runtime requires AbortController",
            ),
            (self.url, "target runtime requires URL"),
            (self.uint8_array, "target runtime requires Uint8Array"),
            (
                self.timers,
                "target runtime requires setTimeout and clearTimeout",
            ),
        ] {
            if !available {
                return Err(Diagnostic(message));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkDeclaration {
    pub timeout_ms: u32,
    pub concurrency: u16,
    pub retries: u16,
    pub offline_queue: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkIr {
    timeout_ms: u32,
    concurrency: u16,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic(pub &'static str);
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Diagnostic {}

impl NetworkDeclaration {
    pub fn normalize(self) -> Result<NetworkIr, Diagnostic> {
        if self.timeout_ms == 0 || self.timeout_ms > i32::MAX as u32 {
            return Err(Diagnostic(
                "timeout must be between 1 and 2147483647 milliseconds",
            ));
        }
        if self.concurrency == 0 {
            return Err(Diagnostic("concurrency must be positive"));
        }
        if self.retries != 0 {
            return Err(Diagnostic("retries are not supported"));
        }
        if self.offline_queue {
            return Err(Diagnostic("offline queue is not supported"));
        }
        Ok(NetworkIr {
            timeout_ms: self.timeout_ms,
            concurrency: self.concurrency,
        })
    }
}

impl NetworkIr {
    /// Fail closed before emitting a recipe for a host with missing runtime facilities.
    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<Artifact, Diagnostic> {
        capabilities.validate()?;
        Ok(self.lower(target))
    }

    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\ntimeout_ms={}\nconcurrency={}\n",
            self.timeout_ms, self.concurrency
        )
    }
    /// Emit a recipe assuming all required runtime facilities are available.
    /// Use `lower_checked` when negotiating an unknown host environment.
    pub fn lower(&self, target: Target) -> Artifact {
        let recipe = match target {
            Target::Browser => "browser-fetch",
            Target::Node => "node-fetch",
        };
        let source = format!(
            r#"// {CONTRACT}; recipe={recipe}@{RECIPE_VERSION}
const limit = {concurrency};
const timeoutMs = {timeout};
let active = 0;
const pending = [];
function release() {{
  if (pending.length) pending.shift()();
  else active--;
}}
function acquire() {{
  if (active < limit) {{ active++; return Promise.resolve(); }}
  return new Promise(resolve => pending.push(resolve));
}}
export async function get(url) {{
  const parsed = new URL(url);
  if (parsed.protocol !== 'https:') throw new TypeError('HTTPS required');
  await acquire();
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {{
    const response = await fetch(parsed, {{ method: 'GET', redirect: 'error', signal: controller.signal }});
    const body = new Uint8Array(await response.arrayBuffer());
    return {{ status: response.status, body }};
  }} finally {{
    clearTimeout(timer);
    release();
  }}
}}
"#,
            concurrency = self.concurrency,
            timeout = self.timeout_ms
        );
        Artifact {
            contract: CONTRACT,
            recipe,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Artifact {
    pub contract: &'static str,
    pub recipe: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn declaration() -> NetworkDeclaration {
        NetworkDeclaration {
            timeout_ms: 5000,
            concurrency: 8,
            retries: 0,
            offline_queue: false,
        }
    }
    #[test]
    fn canonical_and_lowering_are_stable() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(
            ir.canonical(),
            "contract=goya.network.fetch.v1\ntimeout_ms=5000\nconcurrency=8\n"
        );
        for target in [Target::Browser, Target::Node] {
            assert_eq!(ir.lower(target), ir.lower(target));
            assert!(ir.lower(target).source.contains("redirect: 'error'"));
        }
        assert_ne!(
            ir.lower(Target::Browser).recipe,
            ir.lower(Target::Node).recipe
        );
    }
    #[test]
    fn unsupported_requirements_fail_closed() {
        let mut d = declaration();
        d.retries = 1;
        assert!(d.normalize().is_err());
        d = declaration();
        d.offline_queue = true;
        assert!(d.normalize().is_err());
        d = declaration();
        d.concurrency = 0;
        assert!(d.normalize().is_err());
        d = declaration();
        d.timeout_ms = 0;
        assert!(d.normalize().is_err());
        d.timeout_ms = u32::MAX;
        assert!(d.normalize().is_err());
    }

    #[test]
    fn runtime_negotiation_checks_every_required_facility_for_both_targets() {
        let ir = declaration().normalize().unwrap();
        let supported = RuntimeCapabilities {
            fetch: true,
            abort_controller: true,
            url: true,
            uint8_array: true,
            timers: true,
        };
        let missing = [
            (
                RuntimeCapabilities {
                    fetch: false,
                    ..supported
                },
                "target runtime requires fetch",
            ),
            (
                RuntimeCapabilities {
                    abort_controller: false,
                    ..supported
                },
                "target runtime requires AbortController",
            ),
            (
                RuntimeCapabilities {
                    url: false,
                    ..supported
                },
                "target runtime requires URL",
            ),
            (
                RuntimeCapabilities {
                    uint8_array: false,
                    ..supported
                },
                "target runtime requires Uint8Array",
            ),
            (
                RuntimeCapabilities {
                    timers: false,
                    ..supported
                },
                "target runtime requires setTimeout and clearTimeout",
            ),
        ];
        for target in [Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, supported).unwrap(),
                ir.lower(target)
            );
            for (capabilities, message) in missing {
                assert_eq!(
                    ir.lower_checked(target, capabilities),
                    Err(Diagnostic(message))
                );
            }
            assert_eq!(
                ir.lower_checked(target, RuntimeCapabilities::default()),
                Err(Diagnostic("target runtime requires fetch")),
            );
        }
    }
}
