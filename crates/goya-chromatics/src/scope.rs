//! Chromatic Context Manager: nested interaction scopes (§6).
//!
//! Only the innermost scope is active. Hidden, suspended or covered scopes
//! hold no executable control; the registry refuses their presses. Leaving
//! a scope returns the parent's context key, which the caller re-resolves
//! (the engine's history then preserves the parent's slots).
//!
//! A scope may declare that it *inherits* its parent's candidate pool
//! (§6: "explicitly declared inheritance policies"): its own candidates
//! are offered first, and the parent's fill the remaining pool without
//! repeating a candidate id. Without the declaration, a scope replaces its
//! parent's pool, as Show Details replaces Television Shows.

use crate::model::Candidate;
use std::fmt;

/// Scope levels, broadest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScopeKind {
    Platform,
    Application,
    Profile,
    Scene,
    View,
    Component,
    Modal,
}

/// Declared inheritance policy of one scope (§6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Inheritance {
    /// The scope resolves its own candidates only.
    #[default]
    None,
    /// The scope's pool is its own candidates plus its parent's effective
    /// pool; its own candidates win an id conflict.
    Parent,
}

/// A rejected scope transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeError {
    /// A broader scope cannot open inside a narrower one (a modal may open
    /// anywhere, and views may nest).
    Nesting {
        outer: ScopeKind,
        inner: ScopeKind,
    },
    EmptyName,
    /// Names are joined with `/`, so they may not contain it.
    Separator(String),
    NothingToClose,
}

impl fmt::Display for ScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ScopeError {}

#[derive(Debug, Clone, Default)]
pub struct Scopes {
    stack: Vec<(ScopeKind, String, Inheritance)>,
}

impl Scopes {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a scope that resolves its own candidates only; returns the new
    /// active context key.
    pub fn enter(&mut self, kind: ScopeKind, name: &str) -> Result<String, ScopeError> {
        self.enter_with(kind, name, Inheritance::None)
    }

    /// Enter a scope that inherits its parent's effective pool (§6).
    pub fn enter_inheriting(&mut self, kind: ScopeKind, name: &str) -> Result<String, ScopeError> {
        self.enter_with(kind, name, Inheritance::Parent)
    }

    fn enter_with(
        &mut self,
        kind: ScopeKind,
        name: &str,
        inheritance: Inheritance,
    ) -> Result<String, ScopeError> {
        if name.is_empty() {
            return Err(ScopeError::EmptyName);
        }
        if name.contains('/') {
            return Err(ScopeError::Separator(name.to_string()));
        }
        if let Some((outer, _, _)) = self.stack.last() {
            let ok = kind == ScopeKind::Modal
                || kind > *outer
                || (kind == *outer && matches!(kind, ScopeKind::View | ScopeKind::Component));
            if !ok {
                return Err(ScopeError::Nesting {
                    outer: *outer,
                    inner: kind,
                });
            }
        }
        self.stack.push((kind, name.to_string(), inheritance));
        Ok(self.context().unwrap_or_default())
    }

    /// Leave the innermost scope; returns the parent context, if any.
    pub fn leave(&mut self) -> Result<Option<String>, ScopeError> {
        self.stack.pop().ok_or(ScopeError::NothingToClose)?;
        Ok(self.context())
    }

    /// Active context key: every scope name from the outside in, joined by
    /// `/`. It contains the profile scope, which isolates profiles.
    pub fn context(&self) -> Option<String> {
        if self.stack.is_empty() {
            return None;
        }
        Some(
            self.stack
                .iter()
                .map(|(_, n, _)| n.as_str())
                .collect::<Vec<_>>()
                .join("/"),
        )
    }

    /// The active scope's declared inheritance policy.
    pub fn inheritance(&self) -> Inheritance {
        self.stack
            .last()
            .map(|(_, _, i)| *i)
            .unwrap_or(Inheritance::None)
    }

    pub fn active_kind(&self) -> Option<ScopeKind> {
        self.stack.last().map(|(k, _, _)| *k)
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }
}

/// Effective pool of one scope, given its declared policy: with
/// [`Inheritance::None`] the pool is the scope's own candidates; with
/// [`Inheritance::Parent`] its own candidates come first and the parent's
/// effective pool fills the rest without repeating a candidate id (§6).
///
/// Nested declarations fold outward in: the parent's effective pool is
/// composed first, then the child composes onto it, so an inheriting modal
/// sees the whole chain beneath it. Allocation re-sorts the pool, so the
/// composition order matters only for id conflicts, where the inner scope
/// wins.
pub fn compose(own: Vec<Candidate>, inherited: &[Candidate]) -> Vec<Candidate> {
    let mut out = own;
    for c in inherited {
        if !out.iter().any(|e| e.id == c.id) {
            out.push(c.clone());
        }
    }
    out
}
