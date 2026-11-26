//! Experimental interactive profiles: `maja.web.local.v2` over GSSH/3, with
//! GSSH/4 admission when `@platform` exceptions or `@persistence` targets
//! are authored (decisions D10, D11), and
//! `maja.web.local.v3` over GSSH/3-live (GSSH/4-live with GSSH/4
//! declarations, decision D12), which adds `text-from` display bindings
//! (decision D9).
//!
//! Every construct GSSH/3 admits is either carried into [`InteractiveWebIr`]
//! or refused with `MAJA-E0200` at that construct's exact source range.
//! The allow-list enumerates source entries, so an authored construct can
//! never disappear without an admission decision. Coverage mirrors Aragon's
//! (`docs/aragon/android-scene.md`) except that Maja also admits `scroll`
//! viewports (mirroring `Frame::scroll_limits`) and `durable` state.

use crate::Diagnostic;
use goya_lang::handoff::{canonical_property, Origin, SemanticHandoffV2};
use goya_lang::{CellKind, InteractionClass, InteractionValue, SceneCell, SourceRange};

/// Experimental interactive browser profile; not a production qualification.
pub const INTERACTIVE_PROFILE: &str = "maja.web.local.v2";
/// Experimental profile with `text-from` display bindings (GSSH/3-live).
pub const LIVE_PROFILE: &str = "maja.web.local.v3";
/// Largest scene the profile admits.
pub const MAX_CELLS: usize = 1024;
/// Largest UTF-8 text of one element after the reference wrap transform.
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
/// D1 limits checked at admission so the runtime never refuses its initial state.
const D1_MAX_TEXT_BYTES: usize = 64 * 1024;
const D1_MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebCellKind {
    Cell,
    Button,
    Input,
}

/// One authored element in depth-first preorder, the reference draw order.
#[derive(Debug, Clone, PartialEq)]
pub struct WebCell {
    pub id: String,
    /// Structural name; interaction triggers refer to buttons by it.
    pub name: String,
    pub kind: WebCellKind,
    /// Index of the parent in [`InteractiveWebIr::cells`].
    pub parent: Option<usize>,
    pub position_p: Option<[f64; 2]>,
    pub extent_p: Option<[f64; 2]>,
    /// 0xRRGGBB.
    pub fill: Option<u32>,
    /// Colour and `stroke-width-p`; admission requires both or neither.
    pub stroke: Option<(u32, f64)>,
    pub text_size_p: Option<f64>,
    /// Text after the reference wrap transform. On an input, its initial
    /// content (decision D3); inputs are never wrapped.
    pub text: Option<String>,
    /// A scroll viewport: its direct children may extend past it.
    pub scroll: bool,
    /// `text-from`: the state key whose value is shown after `text`.
    pub text_from: Option<String>,
    /// `@iden`: accessible name and description (decision D14).
    pub iden: Option<WebIden>,
    /// `present-when`. Ancestors conjoin at runtime.
    pub present: Option<WebCondition>,
    /// `fill-to` colour and `fill-period-s` seconds, together.
    pub fill_to: Option<(u32, f64)>,
    /// `@draw` of a declared `@graphic`.
    pub draw: Option<WebDraw>,
    /// `appear-s` seconds and the `appear-from-p` offset, as fractions of
    /// the parent. The offset is zero when the author omitted it.
    pub appear: Option<(f64, [f64; 2])>,
    pub range: SourceRange,
}

/// Accessible identity rendered natively: `label` becomes the element's
/// accessible name, `hint` its accessible description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebIden {
    pub label: String,
    pub hint: Option<String>,
}

/// A `present-when` or `@draw` condition. `at_least` is `at-least`; otherwise
/// the comparison is equality.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebCondition {
    pub key: String,
    pub at_least: bool,
    pub value: InteractionValue,
}

/// One `@draw` rule. The value is the dimension's authored label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebDrawRule {
    pub dimension: String,
    pub value: String,
    pub when: Option<WebCondition>,
}

/// `@draw`: the graphic name and its rules, in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebDraw {
    pub graphic: String,
    pub rules: Vec<WebDrawRule>,
}

/// Browser store for durable state (decision D11): `localStorage` unless the
/// scene names a defined database for the `browser` target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurableStore {
    LocalStorage,
    IndexedDb,
}

impl DurableStore {
    /// The caprichio adapter name, as written in `cache_targets`.
    pub fn adapter(self) -> &'static str {
        match self {
            Self::LocalStorage => "browser.local-storage.v1",
            Self::IndexedDb => "browser.indexeddb.v1",
        }
    }
}

/// A strictly admitted scene for the interactive profile.
#[derive(Debug, Clone)]
pub struct InteractiveWebIr {
    profile: &'static str,
    handoff: SemanticHandoffV2,
    source: String,
    cells: Vec<WebCell>,
    store: DurableStore,
    migration: goya_lang::Migration,
    warnings: Vec<Diagnostic>,
}

impl InteractiveWebIr {
    /// The Maja profile this scene was admitted under.
    pub fn profile(&self) -> &'static str {
        self.profile
    }
    pub fn handoff(&self) -> &SemanticHandoffV2 {
        &self.handoff
    }
    /// Exact source bytes of this revision.
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn cells(&self) -> &[WebCell] {
        &self.cells
    }
    /// Where durable keys are kept; meaningful only when some key is durable.
    pub fn durable_store(&self) -> DurableStore {
        self.store
    }
    /// What happens to a record in the other browser store when this
    /// scene's store has none (decision D13; default `Copy`).
    pub fn migration(&self) -> goya_lang::Migration {
        self.migration
    }
    /// Admission warnings: the scene is built, but the author should look.
    pub fn warnings(&self) -> &[Diagnostic] {
        &self.warnings
    }
    pub fn source_digest(&self) -> [u8; 32] {
        self.handoff.parsed().source_digest
    }
    /// Authored element ID of the structural name, when unique.
    pub fn cell_by_name(&self, name: &str) -> Option<&WebCell> {
        let mut found = self.cells.iter().filter(|cell| cell.name == name);
        match (found.next(), found.next()) {
            (Some(cell), None) => Some(cell),
            _ => None,
        }
    }
}

fn refuse(message: impl Into<String>, range: Option<SourceRange>) -> Diagnostic {
    Diagnostic {
        code: "MAJA-E0200",
        message: message.into(),
        range,
    }
}

/// Admit under the latest profile: GSSH/4, or GSSH/4-live when `live`
/// (decisions D12, D15). Earlier profiles are not tried.
fn admit_browser(source: &str, live: bool) -> Result<SemanticHandoffV2, Diagnostic> {
    use goya_lang::handoff::{admit_v4, admit_v4_live};
    let handoff = if live {
        admit_v4_live(source)
    } else {
        admit_v4(source)
    }
    .map_err(Diagnostic::from)?;
    let exceptions = handoff.platform_exceptions().iter().map(|rule| {
        let use_kind = match rule.use_kind {
            goya_lang::PlatformUse::OnlyUse => "only-use",
            goya_lang::PlatformUse::ChoiceUse => "choice-use",
            goya_lang::PlatformUse::Fallback => "fallback",
            goya_lang::PlatformUse::Fallbacks => "fallbacks",
            goya_lang::PlatformUse::MustUse => "must-use",
            goya_lang::PlatformUse::Invade => "invade",
            goya_lang::PlatformUse::Cede => "cede",
        };
        goya_caprichios::PlatformRuleRef {
            platform: rule.platform.clone(),
            feature: rule.feature.clone(),
            use_kind: use_kind.to_owned(),
            options: rule.options.clone(),
            range: goya_caprichios::SourceSpan {
                start: rule.range.start,
                end: rule.range.end,
            },
        }
    });
    goya_caprichios::resolve_platform_exceptions("browser", exceptions, &[]).map_err(|error| {
        Diagnostic {
            code: "GY-E1016",
            message: format!(
                "unresolved {} platform exception for '{}'",
                error.platform, error.feature
            ),
            range: Some(SourceRange {
                start: error.range.start,
                end: error.range.end,
            }),
        }
    })?;
    Ok(handoff)
}

/// Admit `source` under the interactive profile.
pub fn admit_local_interactive(
    source: &str,
    profile: &str,
) -> Result<InteractiveWebIr, Diagnostic> {
    let (profile, handoff) = match profile {
        INTERACTIVE_PROFILE => (INTERACTIVE_PROFILE, admit_browser(source, false)),
        LIVE_PROFILE => (LIVE_PROFILE, admit_browser(source, true)),
        _ => {
            return Err(Diagnostic {
                code: "MAJA-E0002",
                message: format!("unknown local profile: {profile}"),
                range: None,
            })
        }
    };
    let handoff = handoff?;
    let parsed = handoff.parsed();
    if parsed.entries.iter().any(|e| {
        e.source_digest != parsed.source_digest
            || e.range.start > e.range.end
            || e.range.end > source.len()
            || !source.is_char_boundary(e.range.start)
            || !source.is_char_boundary(e.range.end)
    }) {
        return Err(Diagnostic {
            code: "MAJA-E0004",
            message: "canonical source map does not match source bytes".into(),
            range: None,
        });
    }
    let whole = SourceRange {
        start: 0,
        end: source.len(),
    };
    let at = |origin: &Origin| handoff.origin(origin).or(Some(whole));

    // 0. Live constructs the web target does not implement (host state,
    // @link, image-from, remote-digit, drift) are refused, never dropped.
    // Maja paints @graphic/@draw, present-when, fill-to and appear-s.
    let maja_paints = |name: &str| {
        matches!(
            name,
            "graphic"
                | "draw"
                | "present_when"
                | "fill_to"
                | "fill_period_s"
                | "appear_s"
                | "appear_from_p"
        )
    };
    if let Some((what, range)) = goya_lang::extended_live_constructs(parsed, maja_paints).next() {
        return Err(refuse(
            format!("unsupported construct in {INTERACTIVE_PROFILE}: {what}"),
            Some(range),
        ));
    }

    // 1. Closed allow-list over every source entry.
    for entry in &parsed.entries {
        let allowed = match entry.kind.as_str() {
            "ontology" | "cell" | "button" | "input" | "interaction" | "state" | "state-key"
            | "binding" => true,
            // Browser rules were resolved by `admit_browser`; other
            // platforms' rules do not apply here (decision D10).
            "platform" => true,
            // Resolved to a browser store below (decision D11).
            "persistence" => true,
            // Rendered as the native accessible name and description (D14).
            "iden" => true,
            // Initial desktop state. Omission has no entry and is fullscreen (D16).
            "desktop" => true,
            // Live profile only: the parser refuses these on v2 (`GY-E1012`).
            "graphic" | "draw" => true,
            "property" => {
                let element = entry
                    .scene_path
                    .as_deref()
                    .and_then(|path| cell_at(&parsed.scene.root, path));
                let input = element.is_some_and(|cell| cell.kind == CellKind::Input);
                match canonical_property(&entry.name).as_str() {
                    "wrap" if input => return Err(refuse(
                        "`wrap` on @input: a single-line field cannot hold the wrapped line breaks",
                        Some(entry.range),
                    )),
                    "id" | "position_p" | "extent_p" | "fill" | "stroke" | "stroke_width_p"
                    | "text_size_p" | "wrap" => true,
                    // On @input, `text:` is the field's initial content
                    // (decision D3); line breaks are checked per element.
                    "text" => true,
                    // Admitted only by GSSH/3-live; checked per element below.
                    "text_from" | "present_when" | "fill_to" | "fill_period_s" | "appear_s"
                    | "appear_from_p" => true,
                    // Checked per element below.
                    "scroll" => true,
                    "text_anchor" => {
                        return Err(refuse(
                            "`text-anchor` is refused: the reference renderer ignores it",
                            Some(entry.range),
                        ))
                    }
                    _ => false,
                }
            }
            "gestures" => {
                return Err(refuse(
                    "clipboard gesture policies are not implemented for the web",
                    Some(entry.range),
                ))
            }
            _ => false,
        };
        if !allowed {
            return Err(refuse(
                format!(
                    "unsupported construct in {INTERACTIVE_PROFILE}: {} {}",
                    entry.kind, entry.name
                ),
                Some(entry.range),
            ));
        }
    }

    // 2. Elements, with text taken from the reference frame so the browser
    // never re-implements wrapping.
    let scene = &parsed.scene;
    let mut cells = Vec::new();
    collect(&scene.root, None, &mut cells);
    let frame = goya_lang::layout(scene, 1.0, 1.0);
    if frame.nodes.len() != cells.len() {
        return Err(refuse("reference frame does not match the scene", None));
    }
    let mut out = Vec::with_capacity(cells.len());
    for (index, (cell, parent)) in cells.into_iter().enumerate() {
        let id = cell.authored_id.clone().unwrap_or_default();
        let element = Origin::Element(id.clone());
        let property = |name: &str| Origin::Property {
            element: id.clone(),
            property: name.to_owned(),
        };
        if index == MAX_CELLS {
            return Err(refuse("more than 1024 elements in one scene", at(&element)));
        }
        if matches!(cell.kind, CellKind::Button | CellKind::Input) {
            if let Some(child) = cell.children.first() {
                return Err(refuse(
                    "elements nested inside a button or input",
                    at(&Origin::Element(
                        child.authored_id.clone().unwrap_or_default(),
                    )),
                ));
            }
        }
        if cell.stroke.is_some() != cell.stroke_width_p.is_some() {
            let (name, what) = if cell.stroke.is_some() {
                ("stroke", "a stroke colour without stroke-width-p")
            } else {
                ("stroke_width_p", "stroke-width-p without a stroke colour")
            };
            return Err(refuse(what, at(&property(name))));
        }
        if cell.scroll {
            if cell.kind != CellKind::Cell {
                return Err(refuse(
                    "`scroll` on a button or input (they cannot have children)",
                    at(&property("scroll")),
                ));
            }
            let mut ancestor = parent;
            while let Some(a) = ancestor {
                if out_scroll(&out, a) {
                    return Err(refuse(
                        "a scroll viewport inside another scroll viewport (the reference counts nested content in the outer limit; browsers do not)",
                        at(&property("scroll")),
                    ));
                }
                ancestor = out_parent(&out, a);
            }
            if frame.nodes.iter().filter(|n| n.name == cell.name).count() > 1 {
                return Err(refuse(
                    "a scroll viewport whose structural name is not unique (the reference keys scroll offsets by name)",
                    at(&property("scroll")),
                ));
            }
        }
        if cell.text_from.is_some() {
            let at_text_from = at(&property("text_from"));
            if cell.kind == CellKind::Input {
                return Err(refuse(
                    "`text-from` on @input (the field's text is what the user types)",
                    at_text_from,
                ));
            }
        }
        let text = frame.nodes[index].text.clone();
        if let Some(text) = &text {
            if text.len() > MAX_TEXT_BYTES {
                return Err(refuse(
                    "element text above 16 KiB of UTF-8",
                    at(&property("text")),
                ));
            }
            if text.contains(['\0', '\r']) {
                return Err(refuse(
                    "text contains a character the browser renders differently (U+0000 or carriage return)",
                    at(&property("text")),
                ));
            }
            if cell.kind == CellKind::Input && text.contains('\n') {
                return Err(refuse(
                    "initial content of an @input contains a line break (a single-line field strips it)",
                    at(&property("text")),
                ));
            }
        }
        let pair = |v: Option<goya_lang::Vec3>| v.map(|v| [v.x, v.y]);
        out.push(WebCell {
            id,
            name: cell.name.clone(),
            kind: match cell.kind {
                CellKind::Button => WebCellKind::Button,
                CellKind::Input => WebCellKind::Input,
                _ => WebCellKind::Cell,
            },
            parent,
            position_p: pair(cell.position_p),
            extent_p: pair(cell.extent_p),
            fill: cell.fill,
            stroke: cell.stroke.zip(cell.stroke_width_p),
            text_size_p: cell.text_size_p,
            text,
            scroll: cell.scroll,
            text_from: cell.text_from.clone(),
            iden: cell.iden.as_ref().map(|iden| WebIden {
                label: iden.label.clone(),
                hint: iden.hint.clone(),
            }),
            present: cell.present_when.as_ref().map(web_condition),
            fill_to: cell.fill_to.zip(cell.fill_period_s),
            draw: cell.draw.as_ref().map(|draw| WebDraw {
                graphic: draw.graphic.clone(),
                rules: draw
                    .rules
                    .iter()
                    .map(|rule| WebDrawRule {
                        dimension: rule.dimension.clone(),
                        value: rule.value.clone(),
                        when: rule.when.as_ref().map(web_condition),
                    })
                    .collect(),
            }),
            appear: cell.appear_s.map(|seconds| {
                let from = cell.appear_from_p.unwrap_or_default();
                (seconds, [from.x, from.y])
            }),
            range: at(&element).unwrap_or(whole),
        });
    }

    // 3. State and interactions.
    let mut snapshot_bytes = 8usize;
    for decl in handoff.state() {
        let origin = Origin::State(decl.key.clone());
        // Durable keys are admitted: `storage.js` persists them (see
        // docs/maja/interactive-profile.md, "Durable state").
        let value_bytes = match &decl.initial {
            InteractionValue::Int(_) => 8,
            InteractionValue::Bool(_) => 1,
            InteractionValue::Text(text) if text.len() > D1_MAX_TEXT_BYTES => {
                return Err(refuse(
                    "an initial text value above the 64 KiB D1 limit",
                    at(&origin),
                ))
            }
            InteractionValue::Text(text) => text.len(),
        };
        snapshot_bytes += 8 + decl.key.len() + value_bytes;
    }
    if snapshot_bytes > D1_MAX_SNAPSHOT_BYTES {
        return Err(refuse(
            "an initial state snapshot above the 1 MiB D1 limit",
            parsed.declarations.state.as_ref().map(|(_, range)| *range),
        ));
    }
    let (store, defaulted) = durable_store(&handoff, parsed)?;
    let migration = parsed
        .declarations
        .persistence_migration
        .as_ref()
        .map_or(goya_lang::Migration::Copy, |rule| {
            rule.for_platform("browser")
        });
    let mut warnings = Vec::new();
    // Decision D11: saving to a store the author did not choose is allowed
    // but reported, at the first durable key.
    if let Some(key) = handoff.durable_keys().next().filter(|_| defaulted) {
        warnings.push(Diagnostic {
            code: "MAJA-W0001",
            message: format!(
                "durable state is kept in the default browser store {} because the scene names none; declare `@persistence {{ cache_targets: {{ browser: \"...\"; }}; }}` (GSSH/4) to choose one",
                store.adapter()
            ),
            range: handoff.origin(&Origin::State(key.to_owned())),
        });
    }
    for interaction in &scene.interactions {
        if interaction.class == InteractionClass::D2 {
            return Err(refuse(
                "D2 interactions (no browser capability adapters)",
                at(&Origin::Interaction(
                    interaction.authored_id.clone().unwrap_or_default(),
                )),
            ));
        }
    }

    Ok(InteractiveWebIr {
        profile,
        handoff,
        source: source.to_owned(),
        cells: out,
        store,
        migration,
        warnings,
    })
}

/// Resolve the `browser` entry of `@persistence` (decision D11). Omitted
/// declarations and unlisted targets use `localStorage` (and report that it
/// was a default); a named adapter must be one Maja implements.
fn durable_store(
    handoff: &SemanticHandoffV2,
    parsed: &goya_lang::ParsedScene,
) -> Result<(DurableStore, bool), Diagnostic> {
    use goya_caprichios::state_persistence::{resolve, Declaration, Platform};
    let declaration = parsed
        .declarations
        .persistence_cache_targets
        .as_ref()
        .map(|targets| Declaration {
            cache_targets: targets.clone(),
        });
    let range = parsed
        .entries
        .iter()
        .find(|entry| entry.kind == "persistence")
        .map(|entry| entry.range);
    match resolve(declaration.as_ref(), Platform::Browser) {
        Ok(resolution) if resolution.adapter == "browser.indexeddb.v1" => {
            Ok((DurableStore::IndexedDb, resolution.defaulted))
        }
        Ok(resolution) => Ok((DurableStore::LocalStorage, resolution.defaulted)),
        // Durable state with `null` is refused by GSSH/4 itself; without
        // durable state, disabled persistence needs no store.
        Err(_) if handoff.persistence_disabled() => Ok((DurableStore::LocalStorage, false)),
        Err(_) => Err(refuse(
            format!(
                "unknown browser persistence adapter {:?} (Maja implements {})",
                handoff
                    .persistence_cache_targets()
                    .and_then(|targets| targets.get("browser"))
                    .map_or("", String::as_str),
                Platform::Browser.adapters().join(", ")
            ),
            range,
        )),
    }
}

fn out_scroll(cells: &[WebCell], index: usize) -> bool {
    cells[index].scroll
}

fn out_parent(cells: &[WebCell], index: usize) -> Option<usize> {
    cells[index].parent
}

fn web_condition(condition: &goya_lang::Condition) -> WebCondition {
    WebCondition {
        key: condition.key.clone(),
        at_least: condition.op == goya_lang::ConditionOp::AtLeast,
        value: condition.value.clone(),
    }
}

fn collect<'a>(
    cell: &'a SceneCell,
    parent: Option<usize>,
    out: &mut Vec<(&'a SceneCell, Option<usize>)>,
) {
    let index = out.len();
    out.push((cell, parent));
    for child in &cell.children {
        collect(child, Some(index), out);
    }
}

fn cell_at<'a>(root: &'a SceneCell, path: &[usize]) -> Option<&'a SceneCell> {
    path.iter()
        .try_fold(root, |cell, &index| cell.children.get(index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_lang::StateLifetime;

    const COUNTER: &str = include_str!("../../../fixtures/nevada/counter.gscene");

    #[test]
    fn browser_admits_the_latest_profile_and_refuses_scoped_rules() {
        assert_eq!(admit_browser(COUNTER, false).unwrap().contract(), "GSSH/4");

        let unrelated = COUNTER.replace(
            "@state {",
            "@platform ios { \"media\": only-use(\"native.player\"); } @state {",
        );
        assert_eq!(
            admit_browser(&unrelated, false).unwrap().contract(),
            "GSSH/4"
        );

        let matching = COUNTER.replace(
            "@state {",
            "@platform browser { \"media\": only-use(\"native.player\"); } @state {",
        );
        let error = admit_browser(&matching, false).unwrap_err();
        let range = error.range.unwrap();
        assert_eq!(error.code, "GY-E1016");
        assert_eq!(
            &matching[range.start..range.end],
            "\"media\": only-use(\"native.player\");"
        );
    }

    #[test]
    fn durable_store_defaults_to_local_storage_and_admits_a_declared_database() {
        const DURABLE: &str = include_str!("../../../tests/maja/fixtures/durable-counter.gscene");
        let declare = |block: &str| DURABLE.replacen("@state", &format!("{block} @state"), 1);
        let store = |source: &str| {
            let ir = admit_local_interactive(source, INTERACTIVE_PROFILE).unwrap();
            (ir.handoff().contract(), ir.durable_store())
        };
        let warned = |source: &str| {
            admit_local_interactive(source, INTERACTIVE_PROFILE)
                .unwrap()
                .warnings()
                .iter()
                .map(|w| {
                    let range = w.range.unwrap();
                    (w.code, source[range.start..range.end].to_owned())
                })
                .collect::<Vec<_>>()
        };
        // A default store for durable state is reported at the first durable
        // key, whether the declaration is missing or names other platforms.
        let default_warning = vec![("MAJA-W0001", "count: int = 0 durable;".to_owned())];
        assert_eq!(warned(DURABLE), default_warning);
        assert_eq!(
            warned(&declare(
                r#"@persistence { cache_targets: { android: "android.preferences.v1"; }; }"#
            )),
            default_warning
        );
        assert!(warned(&declare(
            r#"@persistence { cache_targets: { browser: "browser.local-storage.v1"; }; }"#
        ))
        .is_empty());
        assert!(warned(&declare(
            r#"@persistence { cache_targets: { browser: "browser.indexeddb.v1"; }; }"#
        ))
        .is_empty());
        // Nothing is saved, so nothing is reported.
        assert!(warned(COUNTER).is_empty());
        // No declaration: GSSH/3 and the default local store.
        assert_eq!(store(DURABLE), ("GSSH/4", DurableStore::LocalStorage));
        assert_eq!(
            store(&declare(
                r#"@persistence { cache_targets: { browser: "browser.indexeddb.v1"; }; }"#
            )),
            ("GSSH/4", DurableStore::IndexedDb)
        );
        assert_eq!(
            store(&declare(
                r#"@persistence { cache_targets: { browser: "browser.local-storage.v1"; }; }"#
            )),
            ("GSSH/4", DurableStore::LocalStorage)
        );
        // Another platform's adapter leaves the browser on its default.
        assert_eq!(
            store(&declare(
                r#"@persistence { cache_targets: { android: "android.preferences.v1"; }; }"#
            )),
            ("GSSH/4", DurableStore::LocalStorage)
        );
        let unknown =
            declare(r#"@persistence { cache_targets: { browser: "browser.opfs.v1"; }; }"#);
        let error = admit_local_interactive(&unknown, INTERACTIVE_PROFILE).unwrap_err();
        let range = error.range.unwrap();
        assert_eq!(error.code, "MAJA-E0200");
        assert!(
            error.message.contains("browser.opfs.v1"),
            "{}",
            error.message
        );
        assert!(unknown[range.start..range.end].starts_with("@persistence"));
        let disabled = declare("@persistence { cache_targets: null; }");
        assert_eq!(
            admit_local_interactive(&disabled, INTERACTIVE_PROFILE)
                .unwrap_err()
                .code,
            "GY-E1014"
        );
        // The live profile moves to GSSH/4-live for the declaration (D12).
        let live =
            declare(r#"@persistence { cache_targets: { browser: "browser.indexeddb.v1"; }; }"#);
        let ir = admit_local_interactive(&live, LIVE_PROFILE).unwrap();
        assert_eq!(
            (ir.handoff().contract(), ir.durable_store()),
            ("GSSH/4-live", DurableStore::IndexedDb)
        );
        assert_eq!(
            admit_local_interactive(DURABLE, LIVE_PROFILE)
                .unwrap()
                .handoff()
                .contract(),
            "GSSH/4-live"
        );
    }

    #[test]
    fn iden_is_carried_and_required_on_every_input() {
        let ir = admit_local_interactive(COUNTER, INTERACTIVE_PROFILE).unwrap();
        let amount = ir
            .cells()
            .iter()
            .find(|c| c.kind == WebCellKind::Input)
            .unwrap();
        assert_eq!(
            amount.iden,
            Some(WebIden {
                label: "Amount".into(),
                hint: Some("A whole number to add to the count".into())
            })
        );
        let start = COUNTER.find("@iden").unwrap();
        let end = start + COUNTER[start..].find('}').unwrap() + 1;
        let bare = format!("{}{}", &COUNTER[..start], &COUNTER[end..]);
        for profile in [INTERACTIVE_PROFILE, LIVE_PROFILE] {
            assert_eq!(
                admit_local_interactive(&bare, profile).unwrap_err().code,
                "GY-E1017"
            );
        }
    }

    #[test]
    fn other_platforms_rules_are_admitted_and_inert() {
        let unrelated = COUNTER.replace(
            "@state {",
            "@platform ios { \"media\": only-use(\"native.player\"); } @state {",
        );
        let ir = admit_local_interactive(&unrelated, INTERACTIVE_PROFILE).unwrap();
        assert_eq!(ir.handoff().contract(), "GSSH/4");
    }

    fn refused(source: &str) -> (String, &str) {
        let error = admit_local_interactive(source, INTERACTIVE_PROFILE).unwrap_err();
        let range = error.range.expect("refusals carry a range");
        (error.code.to_owned(), &source[range.start..range.end])
    }

    #[test]
    fn counter_is_admitted_with_elements_in_draw_order() {
        let ir = admit_local_interactive(COUNTER, INTERACTIVE_PROFILE).unwrap();
        let ids: Vec<_> = ir.cells().iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            ["window.root", "window.title", "window.amount", "window.add"]
        );
        assert_eq!(ir.cells()[2].kind, WebCellKind::Input);
        assert_eq!(ir.cells()[2].parent, Some(0));
        assert_eq!(ir.cells()[2].stroke, Some((0x333333, 0.05)));
        assert_eq!(ir.cells()[3].kind, WebCellKind::Button);
        assert_eq!(ir.cells()[1].text.as_deref(), Some("Counter"));
        assert_eq!(ir.cell_by_name("add").unwrap().id, "window.add");
        assert_eq!(ir.handoff().contract(), "GSSH/4");
        assert_eq!(ir.source(), COUNTER);
    }

    #[test]
    fn every_unsupported_construct_is_refused_at_its_own_range() {
        let cases = [
            (
                COUNTER.replace("stroke: #333;", "stroke: #333; text: \"5 6\"; wrap: 1;"),
                "wrap: 1",
            ),
            (
                COUNTER.replace("stroke: #333;", "stroke: #333; text: \"5\n6\";"),
                "text: \"5\n6\"",
            ),
            (
                COUNTER.replace("text: \"Add\";", "text: \"Add\"; scroll: true;"),
                "scroll: true",
            ),
            (
                COUNTER
                    .replace("id: \"window.root\";", "id: \"window.root\"; scroll: true;")
                    .replace("text: \"Counter\";", "text: \"Counter\"; scroll: true;"),
                "scroll: true",
            ),
            (
                COUNTER.replace(
                    "text: \"Counter\";",
                    "text: \"Counter\"; text-anchor: middle;",
                ),
                "text-anchor: middle",
            ),
            (
                COUNTER.replace("stroke: #333; stroke_width_p: 0.05;", "stroke: #333;"),
                "stroke: #333",
            ),
            (
                COUNTER.replace(
                    "text: \"Add\";",
                    "text: \"Add\"; @cell inner { id: \"window.inner\"; }",
                ),
                "@cell inner { id: \"window.inner\"; }",
            ),
            (
                COUNTER.replace("text: \"Counter\";", "text: \"Coun\rter\";"),
                "text: \"Coun\rter\"",
            ),
        ];
        for (source, expected) in &cases {
            let (code, text) = refused(source);
            assert_eq!(code, "MAJA-E0200", "{expected}");
            assert!(
                text.starts_with(expected),
                "expected range {expected:?}, got {text:?}"
            );
        }
    }

    #[test]
    fn scroll_viewports_are_admitted_when_unambiguous() {
        let source = COUNTER.replace("text: \"Counter\";", "text: \"Counter\"; scroll: true;");
        let ir = admit_local_interactive(&source, INTERACTIVE_PROFILE).unwrap();
        assert!(ir.cells()[1].scroll);
        assert!(!ir.cells()[0].scroll);
        let duplicate = r#"@ontology d { @cell root { id: "r"; @cell a { id: "a1"; scroll: true; } @cell b { id: "b"; @cell a { id: "a2"; } } } }"#;
        let (code, text) = refused(duplicate);
        assert_eq!(code, "MAJA-E0200");
        assert!(text.starts_with("scroll: true"), "{text:?}");
    }

    #[test]
    fn admission_errors_and_unknown_profiles_fail_closed() {
        let absolute = COUNTER.replace("text_size_p: 0.6", "text_size: 12");
        let error = admit_local_interactive(&absolute, INTERACTIVE_PROFILE).unwrap_err();
        assert_eq!(error.code, "GY-E1009");
        assert_eq!(
            admit_local_interactive(COUNTER, "maja.web.local.v9")
                .unwrap_err()
                .code,
            "MAJA-E0002"
        );
        let d2 = COUNTER.replace("class: D1;", "class: D2;");
        assert!(admit_local_interactive(&d2, INTERACTIVE_PROFILE).is_err());
    }

    const DISPLAY: &str = include_str!("../../../tests/maja/fixtures/display-counter.gscene");

    #[test]
    fn text_from_needs_the_live_profile() {
        let ir = admit_local_interactive(DISPLAY, LIVE_PROFILE).unwrap();
        assert_eq!(ir.profile(), LIVE_PROFILE);
        assert_eq!(ir.handoff().contract(), "GSSH/4-live");
        let total = ir.cells().iter().find(|c| c.id == "total").unwrap();
        assert_eq!(total.text.as_deref(), Some("Count: "));
        assert_eq!(total.text_from.as_deref(), Some("count"));
        let raise = ir.cells().iter().find(|c| c.id == "raise").unwrap();
        assert_eq!(raise.kind, WebCellKind::Button);
        assert_eq!(raise.text_from.as_deref(), Some("flag"));
        // v2 is GSSH/3 and refuses the binding at its own range.
        let error = admit_local_interactive(DISPLAY, INTERACTIVE_PROFILE).unwrap_err();
        assert_eq!(error.code, "GY-E1012");
        let range = error.range.unwrap();
        assert!(DISPLAY[range.start..range.end].starts_with("text-from: state.count"));
        // v3 still admits every v2 scene.
        assert_eq!(
            admit_local_interactive(COUNTER, LIVE_PROFILE)
                .unwrap()
                .handoff()
                .contract(),
            "GSSH/4-live"
        );
    }

    #[test]
    fn text_from_is_refused_on_inputs_and_admitted_with_wrap() {
        let source = DISPLAY.replace("id: \"name\";", "id: \"name\"; text-from: state.label;");
        let error = admit_local_interactive(&source, LIVE_PROFILE).unwrap_err();
        // The validation happens at GSSH/3-live parse time in goya-lang.
        assert_eq!(error.code, "GY-E1013");
        let range = error.range.unwrap();
        assert!(source[range.start..range.end].starts_with("text-from: state.label"));
        // D9: `wrap` applies to the authored text; the whole text wraps
        // at the element's edge as Arachnid renders it.
        let wrapped = DISPLAY.replace(
            "text-from: state.count;",
            "text-from: state.count; wrap: 4;",
        );
        let ir = admit_local_interactive(&wrapped, LIVE_PROFILE).unwrap();
        let total = ir.cells().iter().find(|c| c.id == "total").unwrap();
        // "Count: " splits to ["Count:"] (6 graphemes). Width 4 hard-splits to "Coun\nt:".
        assert_eq!(total.text.as_deref(), Some("Coun\nt:"));
        assert_eq!(total.text_from.as_deref(), Some("count"));
    }

    #[test]
    fn input_text_is_initial_content() {
        let source = COUNTER.replace("stroke: #333;", "stroke: #333; text: \"5\";");
        let ir = admit_local_interactive(&source, INTERACTIVE_PROFILE).unwrap();
        assert_eq!(ir.cells()[2].kind, WebCellKind::Input);
        assert_eq!(ir.cells()[2].text.as_deref(), Some("5"));
    }

    #[test]
    fn durable_state_is_admitted() {
        let source = COUNTER.replace("= 0 session", "= 0 durable");
        let ir = admit_local_interactive(&source, INTERACTIVE_PROFILE).unwrap();
        assert_eq!(ir.handoff().durable_keys().collect::<Vec<_>>(), ["count"]);
        assert!(ir
            .handoff()
            .state()
            .iter()
            .all(|d| d.lifetime == StateLifetime::Durable));
    }
}
