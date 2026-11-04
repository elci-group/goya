//! GY-CLIPBOARD/1 — Dual-mode clipboard: verbatim and Deep Copy.
//!
//! `Ctrl+C` copies exactly the selected visible text. `Ctrl+Shift+C` (or a
//! remapped shortcut — never stolen from embedded terminals, GY-CHR-001 §6)
//! assembles a bounded, redacted, provenance-bearing bundle. Writes are
//! gated by capability checks and output-size limits.

use goya_provenance::{
    build_bundle, BundleError, DeepCopyBundle, DeepCopyLimits, DiagnosticRecord, SelectionRange,
    SourceReference,
};

/// Contract identifier implemented by this crate.
pub const CLIPBOARD_CONTRACT: &str = "GY-CLIPBOARD/1";

/// MIME type of the structured Goya representation offered alongside
/// `text/plain` on Linux data-transfer mechanisms.
pub const GOYA_MIME: &str = "application/x-goya-provenance";

/// Identity of the actor requesting a copy (capability enforcement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrincipalId(pub u64);

/// Capabilities relevant to clipboard access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClipboardCapability {
    /// May read the current selection and copy it verbatim.
    Verbatim,
    /// May attach diagnostics, traces and source references (Deep Copy).
    DeepContext,
}

/// A policy decision function: which capabilities a principal holds.
pub trait CapabilityPolicy {
    fn permits(&self, principal: PrincipalId, capability: ClipboardCapability) -> bool;
}

/// Denies everything — used by tests and as a safe default.
pub struct DenyAll;
impl CapabilityPolicy for DenyAll {
    fn permits(&self, _: PrincipalId, _: ClipboardCapability) -> bool {
        false
    }
}

/// Which projection of the selection to place on the clipboard.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CopyMode {
    /// Exact visible text only.
    Verbatim,
    /// Selected text plus bounded, redacted supporting context.
    Deep {
        max_bytes: usize,
        include_traces: bool,
        include_logs: bool,
    },
}

/// A copy request as issued by the input layer.
#[derive(Debug, Clone, PartialEq)]
pub struct CopyRequest {
    pub selection: SelectionRange,
    pub mode: CopyMode,
    pub principal: PrincipalId,
}

/// Clipboard failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardError {
    /// Principal lacks the capability for the requested mode.
    CapabilityDenied,
    /// Bundle assembly refused (selection too large or consent denied).
    Bundle(BundleError),
    /// Rendered payload exceeded the output-size limit.
    PayloadTooLarge,
}

impl From<BundleError> for ClipboardError {
    fn from(e: BundleError) -> Self {
        ClipboardError::Bundle(e)
    }
}

/// One entry placed on the clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardEntry {
    pub plain_text: String,
    /// Present when the copy was Deep mode.
    pub goya_payload: Option<String>,
}

/// Platform clipboard abstraction. The Linux compositor data-transfer
/// implementation (wayland data-device) lands with GY-WAYLAND-001; tests and
/// the runtime use the in-memory provider below.
pub trait ClipboardProvider {
    /// Validate, assemble and write the payload.
    fn copy(
        &self,
        request: CopyRequest,
        source_text: &str,
        source: &SourceReference,
        diagnostics: &[DiagnosticRecord],
    ) -> Result<(), ClipboardError>;

    /// Last written entry, if the provider supports inspection.
    fn last_entry(&self) -> Option<ClipboardEntry>;
}

/// Capability-gated, in-memory provider used for verification and tests.
pub struct MemoryClipboard<'a> {
    policy: &'a dyn CapabilityPolicy,
    entry: std::cell::RefCell<Option<ClipboardEntry>>,
}

impl<'a> MemoryClipboard<'a> {
    pub fn new(policy: &'a dyn CapabilityPolicy) -> Self {
        MemoryClipboard {
            policy,
            entry: std::cell::RefCell::new(None),
        }
    }
}

impl ClipboardProvider for MemoryClipboard<'_> {
    fn copy(
        &self,
        request: CopyRequest,
        source_text: &str,
        source: &SourceReference,
        diagnostics: &[DiagnosticRecord],
    ) -> Result<(), ClipboardError> {
        let selected = request.selection.slice(source_text);
        match request.mode {
            CopyMode::Verbatim => {
                if !self
                    .policy
                    .permits(request.principal, ClipboardCapability::Verbatim)
                {
                    return Err(ClipboardError::CapabilityDenied);
                }
                *self.entry.borrow_mut() = Some(ClipboardEntry {
                    plain_text: selected.to_string(),
                    goya_payload: None,
                });
                Ok(())
            }
            CopyMode::Deep {
                max_bytes,
                include_traces,
                include_logs,
            } => {
                if !self
                    .policy
                    .permits(request.principal, ClipboardCapability::DeepContext)
                {
                    return Err(ClipboardError::CapabilityDenied);
                }
                let limits = DeepCopyLimits {
                    max_bytes,
                    include_traces,
                    include_logs,
                };
                let bundle = build_bundle(selected, source.clone(), diagnostics.to_vec(), &limits)?;
                let rendered = bundle.to_string();
                if rendered.len() > max_bytes {
                    return Err(ClipboardError::PayloadTooLarge);
                }
                *self.entry.borrow_mut() = Some(ClipboardEntry {
                    plain_text: rendered,
                    goya_payload: Some(render_bundle_structured(&bundle)),
                });
                Ok(())
            }
        }
    }

    fn last_entry(&self) -> Option<ClipboardEntry> {
        self.entry.borrow().clone()
    }
}

/// Structured `application/x-goya-provenance` rendering (line-based
/// key:value, stable order).
fn render_bundle_structured(bundle: &DeepCopyBundle) -> String {
    let mut s = String::from("goya-provenance/1\n");
    s.push_str(&format!("text: {}\n", bundle.text));
    s.push_str(&format!("source: {}\n", bundle.source.object_id));
    s.push_str(&format!("component: {}\n", bundle.source.component));
    s.push_str(&format!(
        "state-revision: {}\n",
        bundle.source.state_revision
    ));
    s.push_str(&format!(
        "module-version: {}\n",
        bundle.source.module_version
    ));
    for d in &bundle.diagnostics {
        s.push_str(&format!(
            "diagnostic: {} | {} | {} | {} | {}\n",
            d.event, d.component, d.reason, d.trace_id.0, d.timestamp
        ));
    }
    for r in &bundle.redactions {
        s.push_str(&format!("redaction: {} ({})\n", r.kind, r.reason));
    }
    s
}

/// Grants every capability to a fixed set of principals (test helper).
pub struct AllowList(pub Vec<PrincipalId>);
impl CapabilityPolicy for AllowList {
    fn permits(&self, principal: PrincipalId, _: ClipboardCapability) -> bool {
        self.0.contains(&principal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "Module compilation failed: incompatible state schema.";
    const SELECT: SelectionRange = SelectionRange { start: 0, end: 45 };

    fn source() -> SourceReference {
        SourceReference {
            object_id: "node-1".into(),
            component: "goya-module".into(),
            state_revision: 3,
            module_version: "0.1.0".into(),
        }
    }

    #[test]
    fn verbatim_copies_exact_visible_text() {
        let policy = AllowList(vec![PrincipalId(1)]);
        let provider = MemoryClipboard::new(&policy);
        provider
            .copy(
                CopyRequest {
                    selection: SELECT,
                    mode: CopyMode::Verbatim,
                    principal: PrincipalId(1),
                },
                TEXT,
                &source(),
                &[],
            )
            .unwrap();
        assert_eq!(
            provider.last_entry().unwrap().plain_text,
            "Module compilation failed: incompatible state"
        );
        assert!(provider.last_entry().unwrap().goya_payload.is_none());
    }

    #[test]
    fn deep_copy_includes_context_and_structured_payload() {
        let policy = AllowList(vec![PrincipalId(1)]);
        let provider = MemoryClipboard::new(&policy);
        let diags = vec![DiagnosticRecord {
            event: "module.swap.rejected".into(),
            component: "goya-module".into(),
            reason: "state_schema_mismatch".into(),
            trace_id: goya_provenance::TraceId("gy-0042".into()),
            timestamp: "2026-11-04T02:00:00Z".into(),
        }];
        provider
            .copy(
                CopyRequest {
                    selection: SELECT,
                    mode: CopyMode::Deep {
                        max_bytes: 8192,
                        include_traces: true,
                        include_logs: true,
                    },
                    principal: PrincipalId(1),
                },
                TEXT,
                &source(),
                &diags,
            )
            .unwrap();
        let entry = provider.last_entry().unwrap();
        assert!(entry.plain_text.contains("GOYA DIAGNOSTIC CONTEXT"));
        let payload = entry.goya_payload.unwrap();
        assert!(payload.starts_with("goya-provenance/1"));
        assert!(payload.contains("diagnostic: module.swap.rejected"));
    }

    #[test]
    fn capability_denied_without_grant() {
        let policy = DenyAll;
        let provider = MemoryClipboard::new(&policy);
        let err = provider
            .copy(
                CopyRequest {
                    selection: SELECT,
                    mode: CopyMode::Verbatim,
                    principal: PrincipalId(9),
                },
                TEXT,
                &source(),
                &[],
            )
            .unwrap_err();
        assert_eq!(err, ClipboardError::CapabilityDenied);
    }

    #[test]
    fn deep_requires_distinct_capability() {
        struct VerbatimOnly;
        impl CapabilityPolicy for VerbatimOnly {
            fn permits(&self, _: PrincipalId, cap: ClipboardCapability) -> bool {
                cap == ClipboardCapability::Verbatim
            }
        }
        let policy = VerbatimOnly;
        let provider = MemoryClipboard::new(&policy);
        let err = provider
            .copy(
                CopyRequest {
                    selection: SELECT,
                    mode: CopyMode::Deep {
                        max_bytes: 8192,
                        include_traces: false,
                        include_logs: false,
                    },
                    principal: PrincipalId(1),
                },
                TEXT,
                &source(),
                &[],
            )
            .unwrap_err();
        assert_eq!(err, ClipboardError::CapabilityDenied);
    }
}
