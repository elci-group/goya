//! Style pooling for the appearance transformation stage
//! (directive GOYA-DIS-CASA-001).

use std::collections::BTreeMap;

/// A deduplicating intern pool for appearance payloads. Payloads are
/// keyed by a caller-chosen canonical key (`K`); equal keys share one
/// pooled entry, so a scene's styles can be stored once and referenced
/// by pooled index.
///
/// Determinism: the lookup index is a [`BTreeMap`], so iteration order
/// is the key order, and `pool()` lists entries in first-intern order —
/// both stable across runs regardless of hashing.
#[derive(Debug, Clone, PartialEq)]
pub struct StylePool<K, V> {
    /// Canonical key → pooled index.
    index: BTreeMap<K, usize>,
    /// The key of each pooled entry, parallel to `entries`.
    keys: Vec<K>,
    entries: Vec<V>,
}

impl<K: Ord + Clone, V> StylePool<K, V> {
    pub fn new() -> StylePool<K, V> {
        StylePool {
            index: BTreeMap::new(),
            keys: Vec::new(),
            entries: Vec::new(),
        }
    }

    /// Intern `value` under `key`, returning its pooled index. The first
    /// value interned under a key wins; later values with the same key
    /// return the existing index unchanged.
    pub fn intern(&mut self, key: K, value: V) -> usize {
        if let Some(&existing) = self.index.get(&key) {
            return existing;
        }
        let index = self.entries.len();
        self.index.insert(key.clone(), index);
        self.keys.push(key);
        self.entries.push(value);
        index
    }

    /// The pooled entry at `index`, if in range.
    pub fn get(&self, index: usize) -> Option<&V> {
        self.entries.get(index)
    }

    /// The canonical key of the pooled entry at `index`, if in range.
    pub fn key(&self, index: usize) -> Option<&K> {
        self.keys.get(index)
    }

    /// The index `key` is pooled under, if it has been interned.
    pub fn lookup(&self, key: &K) -> Option<usize> {
        self.index.get(key).copied()
    }

    /// All pooled entries in first-intern order (index i = entry i).
    pub fn pool(&self) -> &[V] {
        &self.entries
    }

    /// All (key, index) pairs in canonical key order.
    pub fn index(&self) -> impl Iterator<Item = (&K, usize)> + '_ {
        self.index.iter().map(|(key, index)| (key, *index))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<K: Ord + Clone, V> Default for StylePool<K, V> {
    fn default() -> StylePool<K, V> {
        StylePool::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_keys_share_one_entry() {
        let mut pool = StylePool::new();
        let first = pool.intern("solid-black", "2px solid black");
        let second = pool.intern("solid-black", "3px solid white");
        assert_eq!(first, second);
        // The first value interned under a key wins.
        assert_eq!(pool.get(first), Some(&"2px solid black"));
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn distinct_keys_append_in_order() {
        let mut pool = StylePool::new();
        assert_eq!(pool.intern("a", 1), 0);
        assert_eq!(pool.intern("b", 2), 1);
        assert_eq!(pool.intern("a", 3), 0);
        assert_eq!(pool.pool(), &[1, 2]);
        assert_eq!(pool.lookup(&"b"), Some(1));
        assert_eq!(pool.lookup(&"c"), None);
    }

    #[test]
    fn keys_round_trip() {
        let mut pool = StylePool::new();
        pool.intern("key", 7);
        assert_eq!(pool.key(0), Some(&"key"));
        assert_eq!(pool.key(1), None);
        assert_eq!(pool.get(0), Some(&7));
        assert_eq!(pool.get(1), None);
    }

    #[test]
    fn canonical_key_order_is_sorted() {
        let mut pool = StylePool::new();
        pool.intern("c", 3);
        pool.intern("a", 1);
        pool.intern("b", 2);
        let keys: Vec<&str> = pool.index().map(|(key, _)| *key).collect();
        assert_eq!(keys, ["a", "b", "c"]);
        // Pool order stays insertion order, independent of key order.
        assert_eq!(pool.pool(), &[3, 1, 2]);
    }

    #[test]
    fn empty_pool_is_empty() {
        let pool: StylePool<String, u32> = StylePool::default();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
        assert_eq!(pool.pool(), &[] as &[u32]);
    }

    #[test]
    fn tuple_keys_dedup_structured_payloads() {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
        struct Entry {
            style: String,
            width: u32,
        }
        let mut pool = StylePool::new();
        let key = ("solid", 2u32);
        let first = pool.intern(
            key,
            Entry {
                style: "solid".into(),
                width: 2,
            },
        );
        let second = pool.intern(
            ("solid", 2u32),
            Entry {
                style: "solid".into(),
                width: 2,
            },
        );
        assert_eq!(first, second);
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.get(first).unwrap().width, 2);
    }
}
