//! Interned strings and stable component identities for one admitted scene.
//!
//! Component ids are the depth-first index of the scene tree. They are stable
//! for one source revision and are not a persistent database identity.

use std::collections::BTreeMap;

use goya_lang::CellKind;

#[derive(Debug, Clone, Default)]
pub(crate) struct Interner {
    strings: Vec<String>,
    map: BTreeMap<String, u32>,
}

impl Interner {
    pub(crate) fn intern(&mut self, value: &str) -> u32 {
        if let Some(id) = self.map.get(value) {
            return *id;
        }
        let id = self.strings.len() as u32;
        self.strings.push(value.to_owned());
        self.map.insert(value.to_owned(), id);
        id
    }

    pub(crate) fn get(&self, id: u32) -> &str {
        &self.strings[id as usize]
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StoredComponent {
    pub id: u32,
    pub name: u32,
    pub semantic_id: Option<u32>,
    pub kind: CellKind,
}

pub(crate) fn index_components(
    root: &goya_lang::SceneCell,
    intern: &mut Interner,
) -> Vec<StoredComponent> {
    let mut out = Vec::new();
    walk(root, intern, &mut out);
    out
}

fn walk(cell: &goya_lang::SceneCell, intern: &mut Interner, out: &mut Vec<StoredComponent>) {
    let id = out.len() as u32;
    out.push(StoredComponent {
        id,
        name: intern.intern(&cell.name),
        semantic_id: cell.authored_id.as_deref().map(|name| intern.intern(name)),
        kind: cell.kind,
    });
    for child in &cell.children {
        walk(child, intern, out);
    }
}
