//! Transactional scene hot-swap (Wave-2 live adaptation).
//!
//! Contract: a swap is atomic — the running scene is replaced only when the
//! new source parses; a failed parse leaves the current scene untouched and
//! the failure is reported. Callers retain their own interaction state
//! across swaps (state retention is the caller's job; the store guarantees
//! the scene replacement is transactional).

use crate::parser::{parse, ParseError};
use crate::scene::Scene;

/// A live scene that can be hot-swapped.
pub struct SceneStore {
    current: Scene,
    generation: u64,
}

impl SceneStore {
    /// Load the initial scene.
    pub fn new(source: &str) -> Result<Self, ParseError> {
        Ok(SceneStore {
            current: parse(source)?,
            generation: 1,
        })
    }

    /// Transactionally replace the scene. Returns the new generation on
    /// success; on parse failure returns the error and leaves the store
    /// completely untouched.
    pub fn swap(&mut self, source: &str) -> Result<u64, ParseError> {
        let next = parse(source)?;
        self.current = next;
        self.generation += 1;
        Ok(self.generation)
    }

    pub fn get(&self) -> &Scene {
        &self.current
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1: &str = "@ontology a { @button b { position: (0,0,0) extent: (10,10,0) } }";
    const V2: &str = "@ontology a { @button b2 { position: (0,0,0) extent: (20,10,0) } }";
    const BROKEN: &str = "@ontology a { @button b3 { position: (0,0,0) ";

    #[test]
    fn successful_swap_bumps_generation() {
        let mut store = SceneStore::new(V1).unwrap();
        assert_eq!(store.generation(), 1);
        assert!(store.get().find_node("b").is_some());
        let gen = store.swap(V2).unwrap();
        assert_eq!(gen, 2);
        assert!(store.get().find_node("b").is_none());
        assert!(store.get().find_node("b2").is_some());
    }

    #[test]
    fn hot_swap_accepts_input_additions() {
        const WITH_INPUT: &str = "@ontology a { @button b { position: (0,0,0) extent: (10,10,0) } @input composer { position: (0,20,0) extent: (30,10,0) text: \"Compose\" } }";
        let mut store = SceneStore::new(V1).unwrap();
        let gen = store.swap(WITH_INPUT).unwrap();
        assert_eq!(gen, 2);
        let composer = store.get().find_node("composer").unwrap();
        assert_eq!(composer.kind, crate::CellKind::Input);
        assert!(composer.is_input);
        assert_eq!(composer.text.as_deref(), Some("Compose"));
        // And an @input-only scene loads as the initial scene too.
        let store = SceneStore::new(WITH_INPUT).unwrap();
        assert!(store.get().find_node("composer").is_some());
    }

    #[test]
    fn hot_swap_accepts_wrap_edits() {
        const WRAPPED: &str = "@ontology a { @cell note { text: \"alpha beta gamma delta\" wrap: 10 } }";
        let mut store = SceneStore::new(V1).unwrap();
        let gen = store.swap(WRAPPED).unwrap();
        assert_eq!(gen, 2);
        let note = store.get().find_node("note").unwrap();
        assert_eq!(note.wrap, Some(10));
        // And back to an unwrapped scene.
        assert!(store.swap(V2).is_ok());
        assert!(store.get().find_node("note").is_none());
    }

    #[test]
    fn failed_swap_is_transactional() {
        let mut store = SceneStore::new(V1).unwrap();
        let gen_before = store.generation();
        assert!(store.swap(BROKEN).is_err());
        // Scene and generation completely untouched.
        assert_eq!(store.generation(), gen_before);
        assert!(store.get().find_node("b").is_some());
        // And the store still accepts a good swap afterwards.
        assert_eq!(store.swap(V2).unwrap(), 2);
    }
}
