//! Element construction: stable IDs, provenance from byte offsets, and the
//! semantics helpers shared by the JavaScript, HTML and CSS passes.

use goya_fair::{
    ElementKind, ElementProvenance, FairElement, Representability, RepresentabilityStatus,
    SemanticsValue,
};
use std::collections::BTreeMap;

pub type Semantics = BTreeMap<String, SemanticsValue>;

pub fn s(value: impl Into<String>) -> SemanticsValue {
    SemanticsValue::Str(value.into())
}

pub fn b(value: bool) -> SemanticsValue {
    SemanticsValue::Bool(value)
}

pub fn list<I: IntoIterator<Item = SemanticsValue>>(items: I) -> SemanticsValue {
    SemanticsValue::List(items.into_iter().collect())
}

/// The FAIR kind name, as serialised, used in element IDs.
pub fn kind_tag(kind: ElementKind) -> &'static str {
    match kind {
        ElementKind::Component => "component",
        ElementKind::Hook => "hook",
        ElementKind::EventHandler => "event_handler",
        ElementKind::IpcChannel => "ipc_channel",
        ElementKind::PreloadBridge => "preload_bridge",
        ElementKind::OsApiCall => "os_api_call",
        ElementKind::ChildProcess => "child_process",
        ElementKind::FilesystemOperation => "filesystem_operation",
        ElementKind::WindowLifecycle => "window_lifecycle",
        ElementKind::NativeModule => "native_module",
        ElementKind::DynamicModuleResolution => "dynamic_module_resolution",
        ElementKind::StateStore => "state_store",
        ElementKind::StyleRule => "style_rule",
        ElementKind::ModuleImport => "module_import",
        ElementKind::LogicUnit => "logic_unit",
        ElementKind::BridgeCall => "bridge_call",
        ElementKind::PackageManifest => "package_manifest",
        ElementKind::Unknown => "unknown",
    }
}

/// Where an element came from.
#[derive(Clone, Copy, Debug)]
pub enum Origin {
    /// An exact end-exclusive byte span.
    Span(u32, u32),
    /// A line only. `fallback` marks a line that is not the construct's start.
    Line { line: u64, fallback: bool },
}

/// How sure the extractor is about an element.
pub enum Status {
    Extracted,
    Undetermined {
        reason: &'static str,
        detail: String,
    },
}

/// Emits the elements of one material file. IDs are `<path>#<kind>.<nnnn>`,
/// where `n` counts elements of that kind in that file in emission order,
/// starting at 1.
pub struct Emitter {
    file: String,
    line_starts: Vec<usize>,
    counters: BTreeMap<&'static str, usize>,
    pub elements: Vec<FairElement>,
}

impl Emitter {
    pub fn new(file: &str, text: &str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Self {
            file: file.to_string(),
            line_starts,
            counters: BTreeMap::new(),
            elements: Vec::new(),
        }
    }

    pub fn file(&self) -> &str {
        &self.file
    }

    /// 1-based line of a byte offset.
    pub fn line_of(&self, offset: usize) -> u64 {
        match self.line_starts.binary_search(&offset) {
            Ok(i) => i as u64 + 1,
            Err(i) => i as u64,
        }
    }

    /// Emits one element and returns its index in `elements`.
    pub fn emit(
        &mut self,
        kind: ElementKind,
        origin: Origin,
        status: Status,
        semantics: Semantics,
    ) -> usize {
        let tag = kind_tag(kind);
        let n = self.counters.entry(tag).or_default();
        *n += 1;
        let id = format!("{}#{}.{:04}", self.file, tag, n);
        let provenance = match origin {
            Origin::Span(start, end) => ElementProvenance {
                file: self.file.clone(),
                line: self.line_of(start as usize),
                span_start: Some(u64::from(start)),
                span_end: Some(u64::from(end)),
                fallback_origin: false,
            },
            Origin::Line { line, fallback } => ElementProvenance {
                file: self.file.clone(),
                line: line.max(1),
                span_start: None,
                span_end: None,
                fallback_origin: fallback,
            },
        };
        let representability = match status {
            Status::Extracted => Representability {
                status: RepresentabilityStatus::Extracted,
                reason: None,
                detail: None,
            },
            Status::Undetermined { reason, detail } => Representability {
                status: RepresentabilityStatus::Undetermined,
                reason: Some(reason.to_string()),
                detail: Some(detail),
            },
        };
        self.elements.push(FairElement {
            id,
            kind,
            provenance,
            representability,
            semantics,
        });
        self.elements.len() - 1
    }

    /// Emits an `unknown` element: a construct outside the FAIR vocabulary.
    pub fn unknown(
        &mut self,
        observed: &str,
        origin: Origin,
        reason: &'static str,
        detail: impl Into<String>,
        mut semantics: Semantics,
    ) -> usize {
        semantics.insert("observed".into(), s(observed));
        self.emit(
            ElementKind::Unknown,
            origin,
            Status::Undetermined {
                reason,
                detail: detail.into(),
            },
            semantics,
        )
    }
}
