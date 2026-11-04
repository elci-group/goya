//! GY-DGPM/1 — "Has equivalent work already been implemented or computed?"
//!
//! The sixth efficiency-hierarchy mechanism. DGPM keeps a registry of
//! deterministic pure kernels and memoises their results: identical input to
//! an identical kernel is computed exactly once and every subsequent call
//! returns the cached result bit-exactly. Reuse is only applied when the
//! cost model says it pays for itself.
//!
//! Core invariants:
//! - Determinism: same kernel + same input ⇒ same output, always (bit-exact).
//! - Content addressing: a kernel is identified by an FNV-1a fingerprint over
//!   its canonical byte description (name + version).
//! - Bounded memory: the memoisation cache is evictable LRU-style; eviction
//!   never changes results, only re-computation counts.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Contract identifier implemented by this crate.
pub const DGPM_CONTRACT: &str = "GY-DGPM/1";

/// FNV-1a over a byte slice (64-bit), the deterministic content hash used
/// for kernel fingerprints.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Content-addressed kernel identifier (FNV-1a over the canonical
/// description: name + version).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KernelId(pub u64);

/// A deterministic pure-function record. `execute` must be a pure function
/// of its input: same input ⇒ same output, always, with no side effects.
pub struct Kernel<K: Eq + Hash + Clone, R: Clone> {
    pub id: KernelId,
    pub fingerprint: u64,
    pub version: u32,
    pub execute: Arc<dyn Fn(&K) -> R + Send + Sync>,
}

impl<K: Eq + Hash + Clone, R: Clone> Clone for Kernel<K, R> {
    fn clone(&self) -> Self {
        Kernel {
            id: self.id,
            fingerprint: self.fingerprint,
            version: self.version,
            execute: Arc::clone(&self.execute),
        }
    }
}

impl<K: Eq + Hash + Clone, R: Clone> fmt::Debug for Kernel<K, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Kernel")
            .field("id", &self.id)
            .field("fingerprint", &self.fingerprint)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// Memoisation / reuse statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DgpmStats {
    /// Calls served from the cache.
    pub hits: u64,
    /// Calls that had to compute (first call per input, or post-eviction).
    pub misses: u64,
    /// Cache entries removed by LRU eviction or invalidation.
    pub evictions: u64,
}

/// Errors raised by the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DgpmError {
    /// No kernel registered under this id.
    UnknownKernel(KernelId),
}

impl fmt::Display for DgpmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DgpmError::UnknownKernel(id) => {
                write!(f, "no kernel registered with id {}", id.0)
            }
        }
    }
}

impl std::error::Error for DgpmError {}

/// One cached result plus its last-use tick for LRU ordering.
#[derive(Clone)]
struct CacheEntry<K: Eq + Hash + Clone, R: Clone> {
    input: K,
    value: R,
    last_used: u64,
}

/// Kernel store: registers deterministic kernels and memoises their results
/// per input. Deterministic: identical operations on identical stores yield
/// identical results regardless of call interleaving.
pub struct KernelStore<K: Eq + Hash + Clone, R: Clone> {
    kernels: HashMap<KernelId, Kernel<K, R>>,
    // Hashes select a bucket only; equality on the retained input decides a
    // hit. A 64-bit hash collision must never return another input's result.
    cache: HashMap<KernelId, HashMap<u64, Vec<CacheEntry<K, R>>>>,
    tick: u64,
    stats: DgpmStats,
}

impl<K: Eq + Hash + Clone, R: Clone> Default for KernelStore<K, R> {
    fn default() -> Self {
        KernelStore {
            kernels: HashMap::new(),
            cache: HashMap::new(),
            tick: 0,
            stats: DgpmStats::default(),
        }
    }
}

impl<K: Eq + Hash + Clone, R: Clone> KernelStore<K, R> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a deterministic kernel under `name` at `version`. The id is
    /// content-addressed: the same name+version always yields the same
    /// [`KernelId`], so re-registration is idempotent (it replaces the body).
    pub fn register(
        &mut self,
        name: &str,
        version: u32,
        execute: impl Fn(&K) -> R + Send + Sync + 'static,
    ) -> KernelId {
        let fingerprint = fingerprint_of(name, version);
        let id = KernelId(fingerprint);
        let kernel = Kernel {
            id,
            fingerprint,
            version,
            execute: Arc::new(execute),
        };
        self.kernels.insert(id, kernel);
        id
    }

    /// Call a registered kernel with memoisation. The first call for a given
    /// (kernel, input) pair computes and caches; later identical calls return
    /// the cached clone bit-exactly.
    pub fn call(&mut self, id: KernelId, input: &K) -> Result<R, DgpmError> {
        let kernel = self
            .kernels
            .get(&id)
            .ok_or(DgpmError::UnknownKernel(id))?
            .clone();
        let input_hash = hash_input(id, input);
        self.tick += 1;
        let tick = self.tick;

        let per_kernel = self.cache.entry(id).or_default();
        if let Some(entry) = per_kernel
            .get_mut(&input_hash)
            .and_then(|bucket| bucket.iter_mut().find(|entry| entry.input == *input))
        {
            entry.last_used = tick;
            self.stats.hits += 1;
            return Ok(entry.value.clone());
        }

        let value = (kernel.execute)(input);
        per_kernel.entry(input_hash).or_default().push(CacheEntry {
            input: input.clone(),
            value: value.clone(),
            last_used: tick,
        });
        self.stats.misses += 1;
        Ok(value)
    }

    /// LRU-evict cache entries until at most `n` entries remain across the
    /// whole store. Eviction never changes results, only re-computation.
    /// Returns the number of entries evicted.
    pub fn evict_lru(&mut self, n: usize) -> usize {
        let total: usize = self
            .cache
            .values()
            .flat_map(|m| m.values())
            .map(Vec::len)
            .sum();
        if total <= n {
            return 0;
        }
        // Collect (kernel id, input hash, last-use tick); evict oldest first.
        let mut all: Vec<(KernelId, u64, K, u64)> = self
            .cache
            .iter()
            .flat_map(|(kid, m)| {
                m.iter().flat_map(move |(hash, bucket)| {
                    bucket
                        .iter()
                        .map(move |entry| (*kid, *hash, entry.input.clone(), entry.last_used))
                })
            })
            .collect();
        all.sort_by_key(|(_, _, _, used)| *used);
        let mut to_evict = total - n;
        for (kid, hash, input, _) in all {
            if to_evict == 0 {
                break;
            }
            if let Some(per_kernel) = self.cache.get_mut(&kid) {
                if let Some(bucket) = per_kernel.get_mut(&hash) {
                    let old_len = bucket.len();
                    bucket.retain(|entry| entry.input != input);
                    let removed = old_len != bucket.len();
                    if bucket.is_empty() {
                        per_kernel.remove(&hash);
                    }
                    if removed {
                        self.stats.evictions += 1;
                        to_evict -= 1;
                    }
                }
            }
        }
        total
            - self
                .cache
                .values()
                .flat_map(|m| m.values())
                .map(Vec::len)
                .sum::<usize>()
    }

    /// Drop all memoised results for a kernel. The kernel stays registered;
    /// subsequent calls recompute. Returns the number of entries dropped.
    pub fn invalidate_kernel(&mut self, id: KernelId) -> usize {
        match self.cache.remove(&id) {
            Some(entries) => {
                let dropped = entries.len();
                self.stats.evictions += dropped as u64;
                dropped
            }
            None => 0,
        }
    }

    /// Whether a kernel is registered under this id.
    pub fn contains(&self, id: KernelId) -> bool {
        self.kernels.contains_key(&id)
    }

    /// Look up a registered kernel record.
    pub fn kernel(&self, id: KernelId) -> Option<&Kernel<K, R>> {
        self.kernels.get(&id)
    }

    /// Current hit/miss/eviction statistics.
    pub fn stats(&self) -> DgpmStats {
        self.stats
    }

    /// Number of memoised entries currently held.
    pub fn cached_entries(&self) -> usize {
        self.cache
            .values()
            .flat_map(|m| m.values())
            .map(Vec::len)
            .sum()
    }
}

/// Canonical byte description of a kernel: framed name + little-endian
/// version, with separators so concatenations cannot collide.
fn canonical_bytes(name: &str, version: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(name.len() + 8);
    bytes.extend_from_slice(b"DGPM/1\x00");
    bytes.extend_from_slice(name.as_bytes());
    bytes.push(0x00);
    bytes.extend_from_slice(&version.to_le_bytes());
    bytes
}

/// FNV-1a fingerprint over the canonical kernel description.
pub fn fingerprint_of(name: &str, version: u32) -> u64 {
    fnv1a(&canonical_bytes(name, version))
}

/// Deterministic hash of (kernel id, input) for memoisation keys.
/// `DefaultHasher::new()` is fixed-keyed, so cache behaviour does not depend
/// on the process run (unlike `RandomState`).
fn hash_input<K: Hash>(id: KernelId, input: &K) -> u64 {
    let mut h = DefaultHasher::new();
    id.0.hash(&mut h);
    input.hash(&mut h);
    h.finish()
}

/// Cost model (LCPE family): reuse pays when the total lookup cost across all
/// expected reuses plus the initial lookup is cheaper than recomputing every
/// time.
///
/// `lookup_cost * expected_reuses + lookup_cost < compute_cost * (expected_reuses + 1)`
pub fn reuse_worthwhile(compute_cost: f64, lookup_cost: f64, expected_reuses: u64) -> bool {
    let reuses = expected_reuses as f64;
    let cached_path = lookup_cost * reuses + lookup_cost;
    let recompute_path = compute_cost * (reuses + 1.0);
    cached_path < recompute_path
}

/// Integration hook (goya-change): two envelopes can share a compiled kernel
/// iff their mutable property masks, resolution tiles and spatial bounds are
/// equal. Persistent masks may differ (they do not affect the compiled
/// computation), as may object names, revisions and event rules.
pub fn envelope_compatible(
    a: goya_change::ChangeEnvelope,
    b: &goya_change::ChangeEnvelope,
) -> bool {
    a.mutable == b.mutable
        && a.resolution_tile == b.resolution_tile
        && a.spatial_bounds == b.spatial_bounds
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_change::{ChangeEnvelope, PixelRect, PropertyMask};

    fn envelope(object: &str, mutable: u32, tile: u32, bounds: PixelRect) -> ChangeEnvelope {
        ChangeEnvelope {
            object: object.to_string(),
            revision: 0,
            persistent: PropertyMask::default(),
            mutable: PropertyMask(mutable),
            event_rules: Vec::new(),
            spatial_bounds: bounds,
            resolution_tile: tile,
        }
    }

    fn cache_entry_exists<K: Eq + Hash + Clone, R: Clone>(
        store: &KernelStore<K, R>,
        id: KernelId,
        input_hash: u64,
    ) -> bool {
        store
            .cache
            .get(&id)
            .is_some_and(|m| m.contains_key(&input_hash))
    }

    #[test]
    fn memoisation_hit_and_miss_accounting() {
        let mut store = KernelStore::<(u64, u64), u64>::new();
        let id = store.register("dot", 1, |(a, b)| a.wrapping_mul(*b));

        assert_eq!(store.call(id, &(3, 4)), Ok(12));
        assert_eq!(store.call(id, &(3, 4)), Ok(12));
        assert_eq!(store.call(id, &(5, 6)), Ok(30));
        assert_eq!(store.call(id, &(3, 4)), Ok(12));
        let stats = store.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.evictions, 0);
    }

    #[test]
    fn bit_exact_determinism() {
        // Inputs must be Eq + Hash; determinism is asserted on the f64 output
        // at the bit level.
        let mut store = KernelStore::<u64, f64>::new();
        let id = store.register("scale", 1, |x| *x as f64 * 1.0e-3 + 0.1);
        let first = store.call(id, &7_250).expect("kernel registered");
        for _ in 0..32 {
            let again = store.call(id, &7_250).expect("kernel registered");
            assert_eq!(first.to_bits(), again.to_bits(), "cached value diverged");
        }
    }

    #[test]
    fn lru_eviction_respects_bound() {
        let mut store = KernelStore::<u64, u64>::new();
        let id = store.register("double", 1, |x| x * 2);
        for i in 0..10u64 {
            let _ = store.call(id, &i);
        }
        assert_eq!(store.cached_entries(), 10);
        let _ = store.call(id, &0); // refresh entry 0's recency

        let evicted = store.evict_lru(5);
        assert_eq!(evicted, 5);
        assert_eq!(store.cached_entries(), 5);
        assert_eq!(store.stats().evictions, 5);

        // Entry 0 was most recently used, so it must survive; entry 1 (least
        // recent) must have been evicted.
        let zero_hash = hash_input(id, &0u64);
        let one_hash = hash_input(id, &1u64);
        assert!(
            cache_entry_exists(&store, id, zero_hash),
            "MRU entry was evicted"
        );
        assert!(
            !cache_entry_exists(&store, id, one_hash),
            "LRU entry survived eviction"
        );

        // Bounding to the current size (or larger) evicts nothing.
        assert_eq!(store.evict_lru(5), 0);
        assert_eq!(store.evict_lru(100), 0);
    }

    #[test]
    fn reuse_worthwhile_boundaries() {
        // Lookup far cheaper than compute: worth it even for one reuse.
        assert!(reuse_worthwhile(1000.0, 1.0, 1));
        // Lookup costlier than compute: never worth it.
        assert!(!reuse_worthwhile(1.0, 10.0, 1));
        assert!(!reuse_worthwhile(1.0, 10.0, 1_000_000));
        // Equal costs: strict inequality means "not worthwhile".
        assert!(!reuse_worthwhile(2.0, 2.0, 5));
        // Zero reuse: cached path is one lookup, recompute path one compute.
        assert!(reuse_worthwhile(10.0, 1.0, 0));
        assert!(!reuse_worthwhile(1.0, 10.0, 0));
    }

    #[test]
    fn envelope_compatibility_matches_on_masks_tiles_bounds() {
        let bounds = PixelRect {
            x: 0,
            y: 0,
            width: 100,
            height: 50,
        };
        let a = envelope("button_a", PropertyMask::GEOMETRY, 8, bounds);
        // Equal except persistent mask and object name: compatible.
        let mut b = envelope("button_b", PropertyMask::GEOMETRY, 8, bounds);
        b.persistent = PropertyMask(PropertyMask::COLOUR);
        assert!(envelope_compatible(a.clone(), &b));

        // Different mutable mask: incompatible.
        let c = envelope("button_c", PropertyMask::COLOUR, 8, bounds);
        assert!(!envelope_compatible(a.clone(), &c));

        // Different resolution tile: incompatible.
        let d = envelope("button_d", PropertyMask::GEOMETRY, 16, bounds);
        assert!(!envelope_compatible(a.clone(), &d));

        // Different spatial bounds: incompatible.
        let e = envelope(
            "button_e",
            PropertyMask::GEOMETRY,
            8,
            PixelRect {
                x: 1,
                y: 0,
                width: 100,
                height: 50,
            },
        );
        assert!(!envelope_compatible(a, &e));
    }

    #[test]
    fn unknown_kernel_errors() {
        let mut store = KernelStore::<u64, u64>::new();
        let id = store.register("known", 1, |x| x + 1);
        let ghost = KernelId(u64::MAX);
        assert_eq!(store.call(ghost, &1), Err(DgpmError::UnknownKernel(ghost)));
        assert!(!store.contains(ghost));
        assert!(store.contains(id));
        assert_eq!(store.invalidate_kernel(ghost), 0);
    }

    #[test]
    fn two_stores_same_ops_same_results() {
        fn run() -> (Vec<u64>, DgpmStats) {
            let mut store = KernelStore::<u64, u64>::new();
            let id = store.register("fib", 1, |n| {
                let (mut a, mut b) = (0u64, 1u64);
                for _ in 0..*n {
                    let t = a.wrapping_add(b);
                    a = b;
                    b = t;
                }
                a
            });
            let mut out = Vec::new();
            for n in [5u64, 10, 5, 20, 10, 5] {
                out.push(store.call(id, &n).expect("kernel registered"));
            }
            (out, store.stats())
        }
        let (a, sa) = run();
        let (b, sb) = run();
        assert_eq!(a, b, "identical operations diverged between stores");
        assert_eq!(sa, sb);
        // 6 calls, 3 distinct inputs: 3 misses, 3 hits.
        assert_eq!(sa.misses, 3);
        assert_eq!(sa.hits, 3);
    }

    #[test]
    fn invalidate_forces_recompute_but_preserves_result() {
        let mut store = KernelStore::<u64, u64>::new();
        let id = store.register("sq", 1, |x| x.wrapping_mul(*x));
        let v1 = store.call(id, &9).expect("kernel registered");
        assert_eq!(store.stats().hits, 0);
        let _ = store.call(id, &9).expect("kernel registered");
        assert_eq!(store.stats().hits, 1);

        assert_eq!(store.invalidate_kernel(id), 1);
        assert_eq!(store.cached_entries(), 0);
        let v2 = store.call(id, &9).expect("kernel registered");
        assert_eq!(v1, v2);
        assert_eq!(store.stats().misses, 2);
        assert!(store.contains(id), "invalidate must not unregister");
    }

    #[test]
    fn content_addressing_is_stable_and_version_sensitive() {
        let id_a1 = fingerprint_of("render.tile", 3);
        let id_a1_again = fingerprint_of("render.tile", 3);
        let id_a2 = fingerprint_of("render.tile", 4);
        let id_b1 = fingerprint_of("render.tiles", 3);
        assert_eq!(id_a1, id_a1_again, "same name+version must give same id");
        assert_ne!(id_a1, id_a2, "version must change the id");
        assert_ne!(id_a1, id_b1, "name must change the id");
    }
}
