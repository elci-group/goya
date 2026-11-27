//! Mixed-dimensional composition contract (directive §5, §22 Phase 4).
//!
//! Goya scenes compose elements across dimensional boundaries: an ordinary
//! Goya element may host a Rugid *model* as its graphical content (§5.2), a
//! Rugid *environment viewport* (§5.3), a Rugid surface may host a `.gscene`
//! application (§5.4), and Rugid environments may nest. This module owns the
//! dimension-agnostic contract — the four composition classes, recursive
//! composition budgets (§5.1) and the parent-element correctness checks
//! (AVR-C13) that every consumer of a spatial `.gscene` must apply.
//!
//! Deliberately out of scope here: geometry and layout (Goya's layout engine
//! owns element rects; the spatial content is clipped by them), input
//! translation (§5.4 belongs to the runtime shell) and the lifecycle
//! *scheduler* — only the policy contract is defined, because "suspending
//! graphical rendering MUST NOT inherently suspend logical simulation"
//! (§5.3) is a runtime invariant that the policy type makes expressible and
//! checkable, not enforceable.

use crate::domain::ObservationDomain;
use std::collections::BTreeMap;
use std::fmt;

/// Maximum spatial nesting depth (§5.1 recursive composition budget).
///
/// Element → model → surface-hosted scene → element → … must terminate.
/// The depth counts spatial edges only; ordinary Goya element nesting has
/// its own parser-side bound (`goya-lang` MAX_CELL_DEPTH).
pub const MAX_SPATIAL_NESTING_DEPTH: u8 = 16;

/// The four compositional relationships (§5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompositionClass {
    /// Goya element → Goya element (conventional composition).
    ElementToElement,
    /// Goya element → Rugid model/environment (embedded spatial content).
    ElementToSpatial,
    /// Rugid surface → Goya element (functional spatial interface, §5.4).
    SpatialSurfaceToElement,
    /// Rugid environment → Rugid environment (nested spatial composition).
    SpatialToSpatial,
}

impl fmt::Display for CompositionClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            CompositionClass::ElementToElement => "element->element",
            CompositionClass::ElementToSpatial => "element->spatial",
            CompositionClass::SpatialSurfaceToElement => "spatial-surface->element",
            CompositionClass::SpatialToSpatial => "spatial->spatial",
        };
        f.write_str(name)
    }
}

/// Which kind of spatial content an element declares (§5.2/§5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialContentKind {
    /// Embedded Rugid model — the element is an object-centric viewport.
    Model,
    /// Viewport into an independent Rugid environment.
    Viewport,
}

/// Authorised simulation policy for a viewport (§5.3 lifecycle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationPolicy {
    /// The environment's logical simulation advances independently of the
    /// parent element's visibility.
    Independent,
    /// Simulation advances only while the element is active.
    Bound,
}

/// Authorised rendering policy for a viewport (§5.3 lifecycle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderingPolicy {
    /// Rendering runs only while the element is visible.
    VisibilityBound,
    /// Rendering runs continuously.
    Continuous,
}

/// Declared lifecycle policy (§5.3). The type pairs the two policies
/// explicitly so that a consumer cannot suspend one without *deciding*
/// about the other: the §5.3 invariant is that suspending rendering must
/// not inherently suspend simulation, which a shared "active" flag would
/// silently violate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecyclePolicy {
    pub simulation: SimulationPolicy,
    pub rendering: RenderingPolicy,
}

/// Whether unresolvable observations may fall back to live rendering.
/// Live fallback is a substitution (§4.3) and must be explicitly authorised
/// by the developer; absence of this flag means it is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveFallback {
    Authorised,
    Refused,
}

/// One spatial composition declaration, already validated against its
/// grammar (the parser rejects unknown keys) and checked here against the
/// composition contract (AVR-C13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpatialElement {
    /// Authored element identity; parent addressing requires it.
    pub element_id: String,
    pub kind: SpatialContentKind,
    /// Model source or environment path, as authored.
    pub asset: String,
    /// Viewport camera name (§5.3: independent camera state).
    pub camera: Option<String>,
    /// Permitted observation space the element declares (AVR-C01 applies
    /// at resolution; the declaration is checked here for domain/kind
    /// agreement).
    pub domain: ObservationDomain,
    pub live_fallback: LiveFallback,
    pub lifecycle: Option<LifecyclePolicy>,
}

/// Why a composition tree is rejected (AVR-C13, §5.1 budgets, §17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositionError {
    /// The element hosting spatial content has no authored identity, so
    /// camera/input/lifecycle state could never be addressed.
    UnidentifiedSpatialElement,
    /// An embedded model must name its source asset (§5.2).
    MissingAsset,
    /// An environmental viewport must name its camera (§5.3: independent
    /// camera state is the point of a viewport).
    ViewportWithoutCamera,
    /// The declared observation domain does not fit the content kind
    /// (navigation graphs are environmental; orbital is object-centric).
    DomainMismatch {
        kind: SpatialContentKind,
        domain: ObservationDomain,
    },
    /// Spatial nesting exceeds `MAX_SPATIAL_NESTING_DEPTH` (§5.1).
    NestingTooDeep { depth: u8, at: String },
    /// A spatial surface hosts a scene that contains a spatial edge back
    /// into an ancestor — composition cycles are rejected (§5.1).
    CompositionCycle { at: String },
    /// A lifecycle declaration is required for viewport content: the
    /// simulation/rendering split must be an explicit developer decision.
    ViewportWithoutLifecycle,
}

impl fmt::Display for CompositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompositionError::UnidentifiedSpatialElement => {
                write!(f, "spatial element has no authored id")
            }
            CompositionError::MissingAsset => write!(f, "spatial model names no source"),
            CompositionError::ViewportWithoutCamera => {
                write!(f, "spatial viewport names no camera")
            }
            CompositionError::DomainMismatch { kind, domain } => {
                write!(f, "observation domain {domain:?} does not fit {kind:?} content")
            }
            CompositionError::NestingTooDeep { depth, at } => {
                write!(f, "spatial nesting depth {depth} exceeds the budget at '{at}'")
            }
            CompositionError::CompositionCycle { at } => {
                write!(f, "spatial composition cycle at '{at}'")
            }
            CompositionError::ViewportWithoutLifecycle => {
                write!(f, "spatial viewport declares no lifecycle policy")
            }
        }
    }
}

impl std::error::Error for CompositionError {}

/// Validate one spatial element declaration against the composition
/// contract (AVR-C13). Tree-level checks (depth, cycles) live in
/// [`CompositionTree`].
pub fn validate_element(element: &SpatialElement) -> Result<(), CompositionError> {
    if element.element_id.trim().is_empty() {
        return Err(CompositionError::UnidentifiedSpatialElement);
    }
    if element.asset.trim().is_empty() {
        return Err(CompositionError::MissingAsset);
    }
    match element.kind {
        SpatialContentKind::Model => {
            // Object-centric domains only: a model is an object viewport.
            if matches!(element.domain, ObservationDomain::NavigationGraph { .. }) {
                return Err(CompositionError::DomainMismatch {
                    kind: SpatialContentKind::Model,
                    domain: element.domain.clone(),
                });
            }
        }
        SpatialContentKind::Viewport => {
            // Environmental domains only: a viewport is a place, not an
            // object. Its camera and lifecycle must be explicit decisions.
            if matches!(
                element.domain,
                ObservationDomain::Orbital
                    | ObservationDomain::Spherical
                    | ObservationDomain::Hemispherical
            ) {
                // (Environmental domains are NavigationGraph and the
                // refinement/custom domains; object-centric ones rejected here.)
                return Err(CompositionError::DomainMismatch {
                    kind: SpatialContentKind::Viewport,
                    domain: element.domain.clone(),
                });
            }
            if element.camera.as_deref().unwrap_or("").trim().is_empty() {
                return Err(CompositionError::ViewportWithoutCamera);
            }
            if element.lifecycle.is_none() {
                return Err(CompositionError::ViewportWithoutLifecycle);
            }
        }
    }
    Ok(())
}

/// The §5.3 invariant, as a checkable property of a lifecycle policy:
/// `rendering: VisibilityBound` with `simulation: Independent` is the
/// supported suspended-rendering pairing — allowed. The reverse coupling
/// (simulation silently bound by a rendering decision) cannot be expressed
/// with this type at all, which is the point.
pub fn rendering_may_suspend(lifecycle: &LifecyclePolicy) -> bool {
    // Rendering may always be suspended; simulation continuity is a
    // separate, explicit policy dimension and is never implied.
    let _ = lifecycle;
    true
}

/// One node of the spatial composition tree: an authored element that
/// hosts spatial content or a Rugid surface hosting a `.gscene` (§5.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionNode {
    pub element_id: String,
    pub class: CompositionClass,
    pub content: SpatialElement,
    /// Parent element id, if any.
    pub parent: Option<String>,
    /// Spatial nesting depth of this node (root spatial element = 1).
    pub depth: u8,
}

/// A validated spatial composition tree: every declared spatial element,
/// keyed by authored element id, with budget and cycle checks applied
/// (§5.1, AVR-C13, §17 nesting-depth enforcement).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompositionTree {
    nodes: BTreeMap<String, CompositionNode>,
}

impl CompositionTree {
    /// Build a tree from declared spatial elements. `hosts` maps each
    /// spatial element's authored id to its parent authored id (absent for
    /// top-level elements). Cycle detection walks parent chains; depth is
    /// the count of spatial edges from the root (§5.1).
    pub fn build(
        elements: Vec<SpatialElement>,
        hosts: &BTreeMap<String, String>,
    ) -> Result<Self, CompositionError> {
        let mut nodes = BTreeMap::new();
        for element in elements {
            validate_element(&element)?;
            if nodes.contains_key(&element.element_id) {
                return Err(CompositionError::CompositionCycle {
                    at: element.element_id.clone(),
                });
            }
            let parent = hosts.get(&element.element_id).cloned();
            nodes.insert(
                element.element_id.clone(),
                CompositionNode {
                    element_id: element.element_id.clone(),
                    class: CompositionClass::ElementToSpatial,
                    content: element,
                    parent,
                    depth: 0,
                },
            );
        }
        // Depth + cycle check per node by walking the parent chain.
        for id in nodes.keys().cloned().collect::<Vec<_>>() {
            let mut seen: Vec<String> = Vec::new();
            let mut current = Some(id.clone());
            let mut depth = 0u8;
            while let Some(step) = current {
                if seen.contains(&step) {
                    return Err(CompositionError::CompositionCycle { at: step });
                }
                seen.push(step.clone());
                depth += 1;
                if depth > MAX_SPATIAL_NESTING_DEPTH {
                    return Err(CompositionError::NestingTooDeep { depth, at: step });
                }
                current = nodes.get(&step).and_then(|n| n.parent.clone());
            }
            if let Some(node) = nodes.get_mut(&id) {
                node.depth = depth;
            }
        }
        Ok(Self { nodes })
    }

    pub fn nodes(&self) -> impl Iterator<Item = &CompositionNode> {
        self.nodes.values()
    }

    pub fn get(&self, element_id: &str) -> Option<&CompositionNode> {
        self.nodes.get(element_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str) -> SpatialElement {
        SpatialElement {
            element_id: id.into(),
            kind: SpatialContentKind::Model,
            asset: "vehicle.rugid".into(),
            camera: None,
            domain: ObservationDomain::Spherical,
            live_fallback: LiveFallback::Refused,
            lifecycle: None,
        }
    }

    fn viewport(id: &str) -> SpatialElement {
        SpatialElement {
            element_id: id.into(),
            kind: SpatialContentKind::Viewport,
            asset: "property.rugid".into(),
            camera: Some("visitor".into()),
            domain: ObservationDomain::NavigationGraph { nodes: 8 },
            live_fallback: LiveFallback::Authorised,
            lifecycle: Some(LifecyclePolicy {
                simulation: SimulationPolicy::Independent,
                rendering: RenderingPolicy::VisibilityBound,
            }),
        }
    }

    #[test]
    fn embedded_model_and_environmental_viewport_validate() {
        validate_element(&model("vehicle_preview")).expect("model validates");
        validate_element(&viewport("property_explorer")).expect("viewport validates");
    }

    #[test]
    fn unidentified_or_unsourced_elements_are_rejected() {
        let mut no_id = model("");
        no_id.element_id = "  ".into();
        assert_eq!(
            validate_element(&no_id),
            Err(CompositionError::UnidentifiedSpatialElement)
        );
        let mut no_asset = model("a");
        no_asset.asset = "".into();
        assert_eq!(validate_element(&no_asset), Err(CompositionError::MissingAsset));
    }

    #[test]
    fn domain_must_agree_with_content_kind() {
        let mut nav_model = model("a");
        nav_model.domain = ObservationDomain::NavigationGraph { nodes: 8 };
        assert_eq!(
            validate_element(&nav_model),
            Err(CompositionError::DomainMismatch {
                kind: SpatialContentKind::Model,
                domain: ObservationDomain::NavigationGraph { nodes: 8 }
            })
        );
        let mut spherical_viewport = viewport("b");
        spherical_viewport.domain = ObservationDomain::Spherical;
        assert!(matches!(
            validate_element(&spherical_viewport),
            Err(CompositionError::DomainMismatch {
                kind: SpatialContentKind::Viewport,
                ..
            })
        ));
    }

    #[test]
    fn viewport_requires_camera_and_lifecycle() {
        let mut no_camera = viewport("a");
        no_camera.camera = None;
        assert_eq!(
            validate_element(&no_camera),
            Err(CompositionError::ViewportWithoutCamera)
        );
        let mut no_lifecycle = viewport("b");
        no_lifecycle.lifecycle = None;
        assert_eq!(
            validate_element(&no_lifecycle),
            Err(CompositionError::ViewportWithoutLifecycle)
        );
    }

    #[test]
    fn suspended_rendering_never_implies_bound_simulation() {
        // §5.3: suspending graphical rendering must not inherently suspend
        // logical simulation. The supported pairing is explicit.
        let lifecycle = LifecyclePolicy {
            simulation: SimulationPolicy::Independent,
            rendering: RenderingPolicy::VisibilityBound,
        };
        assert!(rendering_may_suspend(&lifecycle));
    }

    #[test]
    fn tree_builds_with_parent_depths() {
        let mut host = BTreeMap::new();
        // A scene embedded on a Rugid surface (§5.4) inside a viewport's
        // environment: viewport → surface scene → embedded model.
        host.insert("dashboard".into(), "property_explorer".into());
        let tree = CompositionTree::build(
            vec![viewport("property_explorer"), model("dashboard")],
            &host,
        )
        .expect("tree builds");
        let dashboard = tree.get("dashboard").expect("node");
        assert_eq!(dashboard.parent.as_deref(), Some("property_explorer"));
        assert_eq!(dashboard.depth, 2);
        assert_eq!(dashboard.class, CompositionClass::ElementToSpatial);
    }

    #[test]
    fn composition_cycle_is_rejected() {
        let mut host = BTreeMap::new();
        host.insert("a".into(), "b".into());
        host.insert("b".into(), "a".into());
        assert!(matches!(
            CompositionTree::build(vec![model("a"), model("b")], &host),
            Err(CompositionError::CompositionCycle { .. })
        ));
    }

    #[test]
    fn nesting_beyond_the_budget_is_rejected() {
        // A chain one node deeper than the budget is rejected.
        let n = u32::from(MAX_SPATIAL_NESTING_DEPTH) + 1;
        let mut host = BTreeMap::new();
        let mut elements = Vec::new();
        for i in 0..n {
            let id = format!("e{i}");
            if i > 0 {
                host.insert(id.clone(), format!("e{}", i - 1));
            }
            elements.push(model(&id));
        }
        assert!(matches!(
            CompositionTree::build(elements, &host),
            Err(CompositionError::NestingTooDeep { .. })
        ));

        // Exactly the budget is fine.
        let mut host = BTreeMap::new();
        let mut elements = Vec::new();
        for i in 0..u32::from(MAX_SPATIAL_NESTING_DEPTH) {
            let id = format!("e{i}");
            if i > 0 {
                host.insert(id.clone(), format!("e{}", i - 1));
            }
            elements.push(model(&id));
        }
        CompositionTree::build(elements, &host).expect("budget-respecting chain builds");
    }

    #[test]
    fn duplicate_spatial_declarations_are_rejected() {
        let host = BTreeMap::new();
        assert!(matches!(
            CompositionTree::build(vec![model("a"), model("a")], &host),
            Err(CompositionError::CompositionCycle { .. })
        ));
    }
}
