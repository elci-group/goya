//! Selector matching against the structural tree, using servo's
//! `selectors` crate (plan §11 decision 1). Selectors are compiled from
//! CasaIR selector text; matching walks the [`SceneTree`].

use crate::context::structural::{NodeId, SceneTree};
use cssparser::{Parser as CssParser, ParserInput, ToCss};
use selectors::{
    context::{MatchingContext, MatchingMode, QuirksMode},
    matching::matches_selector,
    parser::{
        NonTSPseudoClass as NonTSPseudoClassTrait, Parser as SelectorsParser,
        PseudoElement as PseudoElementTrait, Selector as CompiledSelector,
    },
    Element as SelElement, OpaqueElement,
};
use std::collections::HashMap;
use std::fmt;

/// The casa selector implementation: plain string names, no namespaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CasaSelectors;

macro_rules! css_string_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(pub String);

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                $name(value.to_string())
            }
        }

        impl ToCss for $name {
            fn to_css<W>(&self, dest: &mut W) -> fmt::Result
            where
                W: fmt::Write,
            {
                dest.write_str(&self.0)
            }
        }
    };
}

css_string_type!(CasaIdentifier);
css_string_type!(CasaLocalName);
css_string_type!(CasaAttrValue);
css_string_type!(CasaNamespace);

impl std::borrow::Borrow<str> for CasaLocalName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for CasaNamespace {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl selectors::parser::SelectorImpl for CasaSelectors {
    type ExtraMatchingData = ();
    type AttrValue = CasaAttrValue;
    type Identifier = CasaIdentifier;
    type LocalName = CasaLocalName;
    type NamespaceUrl = CasaNamespace;
    type NamespacePrefix = CasaIdentifier;
    type BorrowedNamespaceUrl = str;
    type BorrowedLocalName = str;
    type NonTSPseudoClass = PseudoClass;
    type PseudoElement = PseudoElement;
}

/// User-action pseudo-classes Casa tracks per element state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PseudoClass {
    Hover,
    Active,
    Focus,
    Checked,
    Disabled,
    /// Any other pseudo-class; never matches.
    Unsupported(String),
}

impl NonTSPseudoClassTrait for PseudoClass {
    type Impl = CasaSelectors;

    fn is_active_or_hover(&self) -> bool {
        matches!(self, PseudoClass::Hover | PseudoClass::Active)
    }

    fn is_user_action_state(&self) -> bool {
        matches!(
            self,
            PseudoClass::Hover | PseudoClass::Active | PseudoClass::Focus | PseudoClass::Checked
        )
    }
}

impl ToCss for PseudoClass {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        match self {
            PseudoClass::Hover => dest.write_str(":hover"),
            PseudoClass::Active => dest.write_str(":active"),
            PseudoClass::Focus => dest.write_str(":focus"),
            PseudoClass::Checked => dest.write_str(":checked"),
            PseudoClass::Disabled => dest.write_str(":disabled"),
            PseudoClass::Unsupported(name) => {
                dest.write_str(":")?;
                dest.write_str(name)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PseudoElement {
    Before,
    After,
    /// Any other pseudo-element; never matches.
    Unsupported(String),
}

impl PseudoElementTrait for PseudoElement {
    type Impl = CasaSelectors;
}

impl ToCss for PseudoElement {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        match self {
            PseudoElement::Before => dest.write_str("::before"),
            PseudoElement::After => dest.write_str("::after"),
            PseudoElement::Unsupported(name) => {
                dest.write_str("::")?;
                dest.write_str(name)
            }
        }
    }
}

/// The casa parser: accepts the user-action pseudo-classes and the two
/// content pseudo-elements; everything else is rejected (recorded by the
/// caller as an unmatched selector, never silently accepted).
pub struct CasaSelectorParser;

impl<'i> SelectorsParser<'i> for CasaSelectorParser {
    type Impl = CasaSelectors;
    type Error = selectors::parser::SelectorParseErrorKind<'i>;

    fn parse_non_ts_pseudo_class(
        &self,
        _location: cssparser::SourceLocation,
        name: cssparser::CowRcStr<'i>,
    ) -> Result<PseudoClass, selectors::parser::SelectorParseError<'i>> {
        match &*name {
            "hover" => Ok(PseudoClass::Hover),
            "active" => Ok(PseudoClass::Active),
            "focus" => Ok(PseudoClass::Focus),
            "checked" => Ok(PseudoClass::Checked),
            "disabled" => Ok(PseudoClass::Disabled),
            other => Ok(PseudoClass::Unsupported(other.to_string())),
        }
    }

    fn parse_pseudo_element(
        &self,
        _location: cssparser::SourceLocation,
        name: cssparser::CowRcStr<'i>,
    ) -> Result<PseudoElement, selectors::parser::SelectorParseError<'i>> {
        match &*name {
            "before" => Ok(PseudoElement::Before),
            "after" => Ok(PseudoElement::After),
            other => Ok(PseudoElement::Unsupported(other.to_string())),
        }
    }
}

/// A selector compiled from one comma-segment of a style rule's prelude.
#[derive(Debug, Clone)]
pub struct Compiled {
    /// The selector text it was compiled from.
    pub source: String,
    pub selector: CompiledSelector<CasaSelectors>,
    /// Packed specificity from the `selectors` crate (higher wins).
    pub specificity: u32,
}

/// Compile one selector. Err on malformed input or on selectors whose
/// pseudo-classes never match (recorded by the cascade as unmatched).
pub fn compile(text: &str) -> Result<Compiled, String> {
    let mut input = ParserInput::new(text);
    let mut parser = CssParser::new(&mut input);
    let selector = CompiledSelector::parse(&CasaSelectorParser, &mut parser)
        .map_err(|error| format!("{error:?}"))?;
    // A selector whose pseudo-class we support parsing but never match
    // (e.g. an unknown pseudo) is rejected outright: per CSS, a complex
    // selector containing an unknown pseudo-class invalidates the rule.
    if contains_unsupported(selector.iter_raw_match_order()) {
        return Err(format!("unsupported pseudo in selector: {text}"));
    }
    Ok(Compiled {
        source: text.to_string(),
        specificity: selector.specificity(),
        selector,
    })
}

fn contains_unsupported<'a>(
    mut components: impl Iterator<Item = &'a selectors::parser::Component<CasaSelectors>>,
) -> bool {
    components.any(|component| match component {
        selectors::parser::Component::NonTSPseudoClass(PseudoClass::Unsupported(_)) => true,
        selectors::parser::Component::PseudoElement(PseudoElement::Unsupported(_)) => true,
        // :not(), :is() and :where() hold nested selector slices.
        selectors::parser::Component::Negation(list)
        | selectors::parser::Component::Is(list)
        | selectors::parser::Component::Where(list) => list
            .iter()
            .any(|nested| contains_unsupported(nested.iter_raw_match_order())),
        _ => false,
    })
}

/// A borrowed view of one tree node implementing `selectors::Element`.
#[derive(Debug, Clone)]
pub struct ElementRef<'a> {
    tree: &'a SceneTree,
    node: NodeId,
}

impl<'a> ElementRef<'a> {
    pub fn new(tree: &'a SceneTree, node: NodeId) -> Self {
        ElementRef { tree, node }
    }

    pub fn node(&self) -> NodeId {
        self.node
    }
}

impl<'a> SelElement for ElementRef<'a> {
    type Impl = CasaSelectors;

    fn opaque(&self) -> OpaqueElement {
        OpaqueElement::new(&self.tree.nodes[self.node.0 as usize])
    }

    fn parent_element(&self) -> Option<Self> {
        self.tree
            .parent(self.node)
            .map(|parent| ElementRef::new(self.tree, parent))
    }

    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    fn is_pseudo_element(&self) -> bool {
        false
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        let parent = self.tree.parent(self.node)?;
        let siblings = &self.tree.get(parent).children;
        let index = siblings.iter().position(|c| *c == self.node)?;
        if index == 0 {
            None
        } else {
            Some(ElementRef::new(self.tree, siblings[index - 1]))
        }
    }

    fn next_sibling_element(&self) -> Option<Self> {
        let parent = self.tree.parent(self.node)?;
        let siblings = &self.tree.get(parent).children;
        let index = siblings.iter().position(|c| *c == self.node)?;
        siblings
            .get(index + 1)
            .map(|next| ElementRef::new(self.tree, *next))
    }

    fn is_html_element_in_html_document(&self) -> bool {
        true
    }

    fn has_local_name(&self, local_name: &str) -> bool {
        self.tree.get(self.node).name == local_name
    }

    fn has_namespace(&self, namespace: &str) -> bool {
        namespace.is_empty()
    }

    fn is_same_type(&self, other: &Self) -> bool {
        self.tree.get(self.node).name == self.tree.get(other.node).name
    }

    fn attr_matches(
        &self,
        ns: &selectors::attr::NamespaceConstraint<&CasaNamespace>,
        local_name: &CasaLocalName,
        operation: &selectors::attr::AttrSelectorOperation<&CasaAttrValue>,
    ) -> bool {
        if !matches!(
            ns,
            selectors::attr::NamespaceConstraint::Any
                | selectors::attr::NamespaceConstraint::Specific(_)
        ) {
            return false;
        }
        let node = self.tree.get(self.node);
        // `class` and `id` live in their dedicated fields; `class`
        // surfaces as the space-joined token list attribute selectors
        // (`~=`) operate on.
        let joined_class;
        let value = match local_name.0.as_str() {
            "id" => node.id.as_deref(),
            "class" => {
                joined_class = node.classes.join(" ");
                Some(joined_class.as_str())
            }
            name => node.attributes.get(name).map(|v| v.as_str()),
        };
        let Some(value) = value else {
            return false;
        };
        match operation {
            selectors::attr::AttrSelectorOperation::Exists => true,
            selectors::attr::AttrSelectorOperation::WithValue {
                operator,
                case_sensitivity,
                expected_value,
            } => operator.eval_str(value, &expected_value.0, *case_sensitivity),
        }
    }

    fn match_non_ts_pseudo_class<F>(
        &self,
        pc: &PseudoClass,
        _context: &mut MatchingContext<CasaSelectors>,
        _flags_setter: &mut F,
    ) -> bool
    where
        F: FnMut(&Self, selectors::matching::ElementSelectorFlags),
    {
        let name = match pc {
            PseudoClass::Hover => "hover",
            PseudoClass::Active => "active",
            PseudoClass::Focus => "focus",
            PseudoClass::Checked => "checked",
            PseudoClass::Disabled => "disabled",
            PseudoClass::Unsupported(_) => return false,
        };
        self.tree.get(self.node).state.iter().any(|s| s == name)
    }

    fn match_pseudo_element(
        &self,
        _pe: &PseudoElement,
        _context: &mut MatchingContext<CasaSelectors>,
    ) -> bool {
        // Content pseudo-elements resolve during emission, not matching.
        false
    }

    fn is_link(&self) -> bool {
        self.tree.get(self.node).name == "a"
    }

    fn is_html_slot_element(&self) -> bool {
        self.tree.get(self.node).name == "slot"
    }

    fn has_id(
        &self,
        id: &CasaIdentifier,
        case_sensitivity: selectors::attr::CaseSensitivity,
    ) -> bool {
        self.tree
            .get(self.node)
            .id
            .as_deref()
            .map(|own| case_sensitivity.eq(own.as_bytes(), id.0.as_bytes()))
            .unwrap_or(false)
    }

    fn has_class(
        &self,
        name: &CasaIdentifier,
        case_sensitivity: selectors::attr::CaseSensitivity,
    ) -> bool {
        self.tree
            .get(self.node)
            .classes
            .iter()
            .any(|class| case_sensitivity.eq(class.as_bytes(), name.0.as_bytes()))
    }

    fn imported_part(&self, _name: &CasaIdentifier) -> Option<CasaIdentifier> {
        None
    }

    fn is_part(&self, _name: &CasaIdentifier) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        self.tree.get(self.node).children.is_empty()
    }

    fn is_root(&self) -> bool {
        self.tree.get(self.node).parent == Some(NodeId(0))
    }
}

/// Match a compiled selector against an element, walking ancestors and
/// siblings as required by combinators.
pub fn matches(compiled: &Compiled, element: &ElementRef<'_>) -> bool {
    let mut context = MatchingContext::new(MatchingMode::Normal, None, None, QuirksMode::NoQuirks);
    matches_selector(
        &compiled.selector,
        0,
        None,
        element,
        &mut context,
        &mut |_, _| {},
    )
}

/// Cache of compiled selectors keyed by their text. Compilation failures
/// are cached too so a malformed selector is reported once.
#[derive(Debug, Default)]
pub struct SelectorCache {
    compiled: HashMap<String, Result<Compiled, String>>,
}

impl SelectorCache {
    pub fn get(&mut self, text: &str) -> Result<&Compiled, &str> {
        let entry = self
            .compiled
            .entry(text.to_string())
            .or_insert_with(|| compile(text));
        match entry {
            Ok(compiled) => Ok(compiled),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> SceneTree {
        let (tree, errors) = SceneTree::from_html(
            r##"<html><body>
                <div id="main" class="card wide">
                    <p class="lead">hi</p>
                    <p data-kind="primary">there</p>
                    <a href="#" class="btn">go</a>
                </div></body></html>"##,
        );
        assert!(errors.is_empty());
        tree
    }

    fn nodes_by_name<'a>(tree: &'a SceneTree, name: &str) -> Vec<ElementRef<'a>> {
        tree.walk()
            .into_iter()
            .filter(|node| tree.get(*node).name == name)
            .map(|node| ElementRef::new(tree, node))
            .collect()
    }

    #[test]
    fn compiles_and_reports_specificity_ordering() {
        let element = parse_ok(".card #main");
        let class_only = parse_ok(".card");
        let type_only = parse_ok("div");
        assert!(element.specificity > class_only.specificity);
        assert!(class_only.specificity > type_only.specificity);
    }

    fn parse_ok(text: &str) -> Compiled {
        compile(text).unwrap()
    }

    #[test]
    fn matches_type_class_id_and_attributes() {
        let tree = tree();
        let divs = nodes_by_name(&tree, "div");
        assert_eq!(divs.len(), 1);
        assert!(matches(&parse_ok("div"), &divs[0]));
        assert!(matches(&parse_ok(".card.wide"), &divs[0]));
        assert!(matches(&parse_ok("#main"), &divs[0]));
        assert!(!matches(&parse_ok(".card.narrow"), &divs[0]));
        assert!(!matches(&parse_ok("[data-kind]"), &divs[0]));
        let ps = nodes_by_name(&tree, "p");
        assert!(matches(&parse_ok("[data-kind=primary]"), &ps[1]));
        assert!(matches(&parse_ok("p[class~=lead]"), &ps[0]));
    }

    #[test]
    fn combinators_and_structural_pseudos() {
        let tree = tree();
        let ps = nodes_by_name(&tree, "p");
        let link = nodes_by_name(&tree, "a").pop().unwrap();
        assert!(matches(&parse_ok("div p"), &ps[0]));
        assert!(matches(&parse_ok("#main > p"), &ps[0]));
        // ps[1] is the second <p>: :first-child is ps[0].
        assert!(matches(&parse_ok("p:first-child"), &ps[0]));
        assert!(!matches(&parse_ok("p:first-child"), &ps[1]));
        assert!(!matches(&parse_ok("a:hover"), &link));
    }

    #[test]
    fn user_action_state_is_honoured() {
        let mut tree = tree();
        let link = tree
            .walk()
            .into_iter()
            .find(|node| tree.get(*node).name == "a")
            .unwrap();
        tree.set_state(link, "hover");
        let link_ref = ElementRef::new(&tree, link);
        assert!(matches(&parse_ok("a:hover"), &link_ref));
        assert!(!matches(&parse_ok("a:focus"), &link_ref));
    }

    #[test]
    fn unknown_pseudos_are_rejected_not_ignored() {
        assert!(compile("div:magic").is_err());
        assert!(compile("div::magic").is_err());
        assert!(compile("").is_err());
    }

    #[test]
    fn cache_reports_failures_once_per_text() {
        let mut cache = SelectorCache::default();
        assert!(cache.get("div").is_ok());
        assert!(cache.get("div:magic").is_err());
        assert!(cache.get("div:magic").is_err());
    }
}
