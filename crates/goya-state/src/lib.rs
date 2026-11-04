//! GY-STATE/1 — Revisioned state transaction engine.
//!
//! Contract: `begin` → stage writes → `commit(invariant)` or drop (rollback).
//! The committed snapshot is never mutated in place; a transaction stages a
//! copy and atomically swaps it in only when the invariant holds. Reads
//! outside a transaction always observe the last committed revision.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Contract identifier implemented by this crate.
pub const STATE_CONTRACT: &str = "GY-STATE/1";

/// Scalar application state value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Text(String),
    Bool(bool),
}

pub type Map = HashMap<String, Value>;

/// The transaction's invariant failed, or no transaction was open at commit
/// time; the store was rolled back to its last committed revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitRejected;

#[derive(Debug, Default)]
struct Inner {
    committed: Map,
    /// Staged snapshot of an open transaction, or `None`.
    scratch: Option<Map>,
}

/// Handle to the shared store. Clone to share across owners.
#[derive(Clone, Default)]
pub struct Store {
    inner: Arc<Mutex<Inner>>,
    revision: Arc<Mutex<u64>>,
}

impl Store {
    pub fn new() -> Self {
        Self::default()
    }

    /// Current committed revision counter.
    pub fn revision(&self) -> u64 {
        *self.revision.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Read a value: staged writes during an open transaction, otherwise the
    /// committed snapshot.
    pub fn get(&self, key: &str) -> Option<Value> {
        let guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        match &guard.scratch {
            Some(scratch) => scratch.get(key).cloned(),
            None => guard.committed.get(key).cloned(),
        }
    }

    /// Begin a transaction. Returns `None` if another transaction is open —
    /// single-writer is enforced structurally.
    pub fn begin(&self) -> Option<Transaction<'_>> {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if guard.scratch.is_some() {
            return None;
        }
        guard.scratch = Some(guard.committed.clone());
        drop(guard);
        Some(Transaction { store: self })
    }
}

/// Open transaction. Stage writes with `set`; `commit` validates an invariant
/// against the staged snapshot before publishing; dropping without `commit`
/// rolls back to the last committed revision.
pub struct Transaction<'a> {
    store: &'a Store,
}

impl Transaction<'_> {
    /// Stage a write (visible to `Store::get` on this store until rollback).
    pub fn set(&self, key: &str, value: Value) {
        let mut guard = self.store.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(scratch) = &mut guard.scratch {
            scratch.insert(key.to_string(), value);
        }
    }

    /// Publish staged writes if `invariant` holds for the staged snapshot.
    /// On failure (or if the transaction was invalidated) rolls back and
    /// returns `Err(CommitRejected)`.
    pub fn commit(self, invariant: impl Fn(&Map) -> bool) -> Result<(), CommitRejected> {
        let mut guard = self.store.inner.lock().unwrap_or_else(|p| p.into_inner());
        let result = match &guard.scratch {
            Some(scratch) if invariant(scratch) => {
                guard.committed = scratch.clone();
                Ok(())
            }
            _ => Err(CommitRejected),
        };
        guard.scratch = None; // publish or discard, then close the transaction
        drop(guard);
        if result.is_ok() {
            *self
                .store
                .revision
                .lock()
                .unwrap_or_else(|p| p.into_inner()) += 1;
        }
        result
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        let mut guard = self.store.inner.lock().unwrap_or_else(|p| p.into_inner());
        // Leaving scratch in place would wedge future `begin` calls; discarding
        // it restores reads to the committed snapshot (rollback).
        guard.scratch = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nonneg(m: &Map) -> bool {
        matches!(m.get("count"), Some(Value::Int(n)) if *n >= 0)
    }

    #[test]
    fn commit_publishes_and_bumps_revision() {
        let store = Store::new();
        let tx = store.begin().unwrap();
        tx.set("count", Value::Int(1));
        tx.commit(nonneg).unwrap();
        assert_eq!(store.get("count"), Some(Value::Int(1)));
        assert_eq!(store.revision(), 1);
    }

    #[test]
    fn failed_invariant_rolls_back() {
        let store = Store::new();
        {
            let tx = store.begin().unwrap();
            tx.set("count", Value::Int(-5));
            assert!(tx.commit(nonneg).is_err());
        }
        assert_eq!(store.get("count"), None);
        assert_eq!(store.revision(), 0);
    }

    #[test]
    fn drop_without_commit_rolls_back() {
        let store = Store::new();
        {
            let tx = store.begin().unwrap();
            tx.set("count", Value::Int(7));
        }
        assert_eq!(store.get("count"), None);
        // Store is usable again after rollback.
        let tx = store.begin().unwrap();
        tx.set("count", Value::Int(3));
        tx.commit(nonneg).unwrap();
        assert_eq!(store.get("count"), Some(Value::Int(3)));
    }

    #[test]
    fn single_writer_enforced() {
        let store = Store::new();
        let _tx = store.begin().unwrap();
        assert!(store.begin().is_none());
    }

    #[test]
    fn sequential_transactions_accumulate() {
        let store = Store::new();
        for i in 0..5 {
            let tx = store.begin().unwrap();
            tx.set("count", Value::Int(i));
            tx.commit(nonneg).unwrap();
        }
        assert_eq!(store.get("count"), Some(Value::Int(4)));
        assert_eq!(store.revision(), 5);
    }
}
