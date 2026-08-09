//! Bounded memo maps for the analyzer's generation-lifetime query caches.
//!
//! Every per-language `*MemoCaches` bucket holds the same kind of data: values
//! derived from one analyzer instance, keyed by a file, a code unit or a
//! module name, and thrown away wholesale when `update`/`update_all` builds the
//! next analyzer. The analyzer instance IS the generation, so an entry only has
//! to survive until the next update, and within that generation nearly nothing
//! is ever evicted.
//!
//! Those caches used to be `moka::sync::Cache`. Moka buys eviction quality --
//! a TinyLFU admission sketch, LRU deques, a housekeeper and epoch-based
//! reclamation -- and charges for it on **every probe**, not on every eviction.
//! That is the wrong trade for data whose useful life ends at the next
//! generation: the m14 re-profile
//! (`.agents/docs/graph-churn-profile-2026-08.md` and its Stage-1 follow-up)
//! measured moka at **32.5 % of the answering window**, split 20.2 % lookup and
//! 11.9 % LRU/sketch bookkeeping, with `crossbeam-epoch` riding along -- for a
//! Rust scan whose caches mostly never evict.
//!
//! [`WeightedCache`] is what that data actually needs:
//!
//! * A sharded `RwLock<HashMap>`. A hit is one read-lock acquire, one Fx hash
//!   and one `Arc` clone. No read recording, no admission sketch, no
//!   housekeeper, no epoch pinning.
//! * The **same weigher arithmetic**, run once per insert (never per hit), so
//!   the byte budgets the callers already chose keep their meaning.
//! * A **crude cap**: when a shard's inserted weight would exceed its share of
//!   the budget, the whole shard is dropped and refills. This is not LRU and
//!   makes no pretence of being LRU -- it is a memory bound, and the values it
//!   drops are all recomputable from the analyzer that owns them. The tradeoff
//!   is deliberate: a policy good enough to keep is a policy that costs
//!   something per hit, and per-hit cost is the thing being removed.
//!
//! Use [`build_flight_cache`] instead when a cache genuinely needs concurrent
//! single-flight (`get_with`): those callers depend on exactly one thread
//! running the init closure per key, which a plain map does not provide.

use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use rustc_hash::FxBuildHasher;

use crate::hash::HashMap;

/// A moka cache whose keys are hashed with Fx rather than SipHash.
///
/// Reserved for the caches that need `get_with`'s single-flight guarantee.
/// Everything else belongs in a [`WeightedCache`].
pub(crate) type FlightCache<K, V> = moka::sync::Cache<K, V, FxBuildHasher>;

/// Shard count. A power of two so the index is a mask, and larger than the
/// core count of the machines Bifrost runs a fan-out scan on so that a
/// rayon-wide probe storm spreads across cache lines instead of queueing on
/// one lock word.
const SHARD_COUNT: usize = 64;

/// A bounded, generation-lifetime memo map with no per-hit bookkeeping.
///
/// Cloning shares the entries, exactly as `moka::sync::Cache` does: the
/// per-language bucket is held behind one `Arc` and every analyzer clone reads
/// the same map.
pub(crate) struct WeightedCache<K, V> {
    inner: Arc<Inner<K, V>>,
}

struct Inner<K, V> {
    shards: Box<[RwLock<Shard<K, V>>]>,
    /// One shard's share of the caller's byte budget.
    shard_budget_bytes: u64,
    weigher: Box<dyn Fn(&K, &V) -> u32 + Send + Sync>,
    /// Shard flushes so far, for the cap's regression pin.
    flushes: AtomicU64,
    hasher: FxBuildHasher,
}

struct Shard<K, V> {
    /// Each entry keeps the weight it was inserted with, so replacing a key
    /// corrects the running total instead of double-counting it.
    entries: HashMap<K, (u32, V)>,
    weight: u64,
}

impl<K, V> Default for Shard<K, V> {
    fn default() -> Self {
        Self {
            entries: HashMap::default(),
            weight: 0,
        }
    }
}

impl<K, V> Clone for WeightedCache<K, V> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<K, V> WeightedCache<K, V>
where
    K: Eq + Hash,
{
    fn shard_of<Q>(&self, key: &Q) -> &RwLock<Shard<K, V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        // Bits 32..38 of the Fx hash. The map inside the shard mixes the same
        // hash again, and uses the low bits for its bucket index and the top
        // seven for its control byte, so this window overlaps neither.
        let index = (self.inner.hasher.hash_one(key) >> 32) as usize % SHARD_COUNT;
        &self.inner.shards[index]
    }

    /// The memoized value, or `None`.
    ///
    /// This is the whole hot path: hash, read-lock one shard, clone one `Arc`.
    pub(crate) fn get<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
        V: Clone,
    {
        let shard = self
            .shard_of(key)
            .read()
            .expect("memo shard lock is never held across a fallible step");
        shard.entries.get(key).map(|(_, value)| value.clone())
    }

    /// Memoize `value` under `key`, flushing the shard first when the insert
    /// would take it over budget.
    pub(crate) fn insert(&self, key: K, value: V) {
        // Outside the lock: the weigher walks the value, and a panic inside it
        // must not poison a shard.
        let weight = u64::from((self.inner.weigher)(&key, &value));
        let (flushed, replaced) = {
            let mut shard = self
                .shard_of(&key)
                .write()
                .expect("memo shard lock is never held across a fallible step");
            // Rewriting a key trades its old weight for the new one, so the
            // budget question is about the total this insert leaves behind,
            // not about the total plus the new value.
            let displaced = shard
                .entries
                .get(&key)
                .map_or(0, |(weight, _)| u64::from(*weight));
            let flushed = if shard.weight - displaced + weight > self.inner.shard_budget_bytes {
                self.inner.flushes.fetch_add(1, Ordering::Relaxed);
                shard.weight = 0;
                Some(std::mem::take(&mut shard.entries))
            } else {
                shard.weight -= displaced;
                None
            };
            let replaced = shard.entries.insert(key, (weight as u32, value));
            shard.weight += weight;
            (flushed, replaced)
        };
        // Deallocating a flushed shard, or the value a key just replaced, is
        // the caller's own time and does not belong inside the lock.
        drop(flushed);
        drop(replaced);
    }

    /// Entries currently memoized, across all shards.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.inner
            .shards
            .iter()
            .map(|shard| {
                shard
                    .read()
                    .expect("memo shard lock is never held across a fallible step")
                    .entries
                    .len()
            })
            .sum()
    }

    /// How many times a shard was dropped for being over budget.
    #[cfg(test)]
    pub(crate) fn flushes(&self) -> u64 {
        self.inner.flushes.load(Ordering::Relaxed)
    }
}

/// A bounded memo map holding `budget_bytes` of `weigher`-measured values.
///
/// Signature-compatible with the `moka` builder it replaces, so the byte
/// budgets and weighers the callers already chose carry over unchanged.
pub(crate) fn build_weighted_cache<K, V>(
    budget_bytes: u64,
    weigher: impl Fn(&K, &V) -> u32 + Send + Sync + 'static,
) -> WeightedCache<K, V>
where
    K: Eq + Hash + Send + Sync + 'static,
    V: Send + Sync + 'static,
{
    let shards = (0..SHARD_COUNT)
        .map(|_| RwLock::new(Shard::default()))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    WeightedCache {
        inner: Arc::new(Inner {
            shards,
            shard_budget_bytes: (budget_bytes / SHARD_COUNT as u64).max(1),
            weigher: Box::new(weigher),
            flushes: AtomicU64::new(0),
            hasher: FxBuildHasher,
        }),
    }
}

/// A moka cache, for the callers that need `get_with`'s single-flight.
///
/// The one thing kept from moka here is the guarantee a plain map cannot give:
/// when many rayon workers ask the same cold key at once, exactly one runs the
/// initializer. The hasher is Fx because the key is hashed on every operation
/// and these keys are repository-local, never attacker-controlled.
pub(crate) fn build_flight_cache<K, V>(
    budget_bytes: u64,
    weigher: impl Fn(&K, &V) -> u32 + Send + Sync + 'static,
) -> FlightCache<K, V>
where
    K: Clone + Eq + Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    moka::sync::Cache::builder()
        .max_capacity(budget_bytes.max(1))
        .weigher(weigher)
        .build_with_hasher(FxBuildHasher)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ten_bytes<K>(_key: &K, _value: &Arc<String>) -> u32 {
        10
    }

    #[test]
    fn memoizes_and_reads_back_through_a_borrowed_key() {
        let cache = build_weighted_cache::<String, Arc<String>>(1 << 20, ten_bytes);
        cache.insert("alpha".to_string(), Arc::new("one".to_string()));
        assert_eq!(
            cache.get("alpha").as_deref().map(String::as_str),
            Some("one")
        );
        assert_eq!(cache.get("beta"), None);
    }

    #[test]
    fn replacing_a_key_does_not_double_count_its_weight() {
        // One shard's budget is 10 bytes, so a key rewritten eleven times would
        // flush if each rewrite were charged again.
        let cache = build_weighted_cache::<String, Arc<String>>(10 * SHARD_COUNT as u64, ten_bytes);
        for round in 0..11 {
            cache.insert("alpha".to_string(), Arc::new(round.to_string()));
        }
        assert_eq!(cache.flushes(), 0);
        assert_eq!(cache.len(), 1);
        assert_eq!(
            cache.get("alpha").as_deref().map(String::as_str),
            Some("10")
        );
    }

    #[test]
    fn the_cap_flushes_the_shard_and_the_cache_keeps_answering() {
        // Every key lands in one shard's 10-byte budget only if it hashes
        // there, so drive enough distinct keys that some shard must overflow.
        let cache = build_weighted_cache::<String, Arc<String>>(10 * SHARD_COUNT as u64, ten_bytes);
        for key in 0..4_000 {
            cache.insert(format!("key-{key}"), Arc::new(key.to_string()));
        }
        assert!(
            cache.flushes() > 0,
            "a 10-byte-per-shard budget must flush under 4,000 inserts"
        );
        // The bound holds: at most one entry over budget per shard survives.
        assert!(
            cache.len() <= 2 * SHARD_COUNT,
            "entries retained: {}",
            cache.len()
        );
        // A flushed cache is still a working cache.
        cache.insert("fresh".to_string(), Arc::new("value".to_string()));
        assert_eq!(
            cache.get("fresh").as_deref().map(String::as_str),
            Some("value")
        );
    }

    #[test]
    fn an_oversized_value_is_still_readable_after_its_own_flush() {
        let cache = build_weighted_cache::<String, Arc<String>>(
            SHARD_COUNT as u64,
            |_key: &String, _value: &Arc<String>| 1_000,
        );
        cache.insert("alpha".to_string(), Arc::new("one".to_string()));
        assert_eq!(
            cache.get("alpha").as_deref().map(String::as_str),
            Some("one")
        );
    }

    #[test]
    fn clones_share_one_map() {
        let cache = build_weighted_cache::<String, Arc<String>>(1 << 20, ten_bytes);
        let clone = cache.clone();
        cache.insert("alpha".to_string(), Arc::new("one".to_string()));
        assert_eq!(
            clone.get("alpha").as_deref().map(String::as_str),
            Some("one")
        );
    }
}
