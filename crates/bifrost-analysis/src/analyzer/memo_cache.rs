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
//! * A **FIFO cap**: an insert that would take a shard over its share of the
//!   budget evicts the shard's oldest entries, one at a time, until the insert
//!   fits. Insertion order is a `VecDeque<K>` maintained at insert only, so a
//!   hit still records nothing. This is not LRU and makes no pretence of being
//!   LRU; it is a memory bound with an eviction order cheap enough to keep.
//!
//!   The first draft of this cap dropped the **whole shard** on overflow, on
//!   the theory that everything it drops is recomputable and a crude flush was
//!   therefore acceptable. **It is not, and the measurement said so plainly.**
//!   On the rustc-tree answering cell a 16 MiB cache over 35,370 files holds a
//!   few thousand entries, so a shard-wide flush destroys the working set on
//!   every overflow and the walk recomputes it: moka fell from 38.05 % of the
//!   window to 0.03 % exactly as intended, and `path` + `ProjectFile` rose
//!   6.27 % -> 23.33 % taking wall from 781 s to 1,299 s. Evicting one entry at
//!   a time is what moka was doing that actually mattered.
//!
//! Use [`build_flight_cache`] instead when a cache genuinely needs concurrent
//! single-flight (`get_with`): those callers depend on exactly one thread
//! running the init closure per key, which a plain map does not provide.

use std::borrow::Borrow;
use std::collections::VecDeque;
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
    /// Entries evicted by the cap so far, for its regression pin.
    evictions: AtomicU64,
    hasher: FxBuildHasher,
}

struct Shard<K, V> {
    /// Each entry keeps the weight it was inserted with, so replacing a key
    /// corrects the running total instead of double-counting it.
    entries: HashMap<K, (u32, V)>,
    /// Keys in insertion order, one per live entry: a rewrite updates the
    /// entry in place and does not re-queue the key, so this stays the same
    /// length as `entries` and eviction never has to skip a stale name.
    order: VecDeque<K>,
    weight: u64,
}

impl<K, V> Default for Shard<K, V> {
    fn default() -> Self {
        Self {
            entries: HashMap::default(),
            order: VecDeque::new(),
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

    /// Memoize `value` under `key`, evicting the shard's oldest entries first
    /// when the insert would take it over budget.
    pub(crate) fn insert(&self, key: K, value: V)
    where
        K: Clone,
    {
        // Outside the lock: the weigher walks the value, and a panic inside it
        // must not poison a shard.
        let weight = u64::from((self.inner.weigher)(&key, &value));
        let mut discarded: Vec<(u32, V)> = Vec::new();
        {
            let mut shard = self
                .shard_of(&key)
                .write()
                .expect("memo shard lock is never held across a fallible step");
            // Rewriting a key trades its old weight for the new one, so the
            // budget question is about the total this insert leaves behind,
            // not about the total plus the new value.
            let rewrite = match shard.entries.get(&key) {
                Some((displaced, _)) => {
                    shard.weight -= u64::from(*displaced);
                    true
                }
                None => false,
            };
            while shard.weight + weight > self.inner.shard_budget_bytes {
                let Some(oldest) = shard.order.pop_front() else {
                    // The shard is empty and this one value is still over
                    // budget. Admit it anyway: refusing would turn a cache
                    // into a permanent miss for that key.
                    break;
                };
                let evicted = shard
                    .entries
                    .remove(&oldest)
                    .expect("the order queue holds exactly the live keys");
                shard.weight -= u64::from(evicted.0);
                discarded.push(evicted);
            }
            self.inner
                .evictions
                .fetch_add(discarded.len() as u64, Ordering::Relaxed);
            if !rewrite {
                shard.order.push_back(key.clone());
            }
            if let Some(previous) = shard.entries.insert(key, (weight as u32, value)) {
                discarded.push(previous);
            }
            shard.weight += weight;
        }
        // Deallocating what the cap dropped, or the value a key just replaced,
        // is the caller's own time and does not belong inside the lock.
        drop(discarded);
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

    /// How many entries the cap has evicted.
    #[cfg(test)]
    pub(crate) fn evictions(&self) -> u64 {
        self.inner.evictions.load(Ordering::Relaxed)
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
            evictions: AtomicU64::new(0),
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
        // evict itself if each rewrite were charged again.
        let cache = build_weighted_cache::<String, Arc<String>>(10 * SHARD_COUNT as u64, ten_bytes);
        for round in 0..11 {
            cache.insert("alpha".to_string(), Arc::new(round.to_string()));
        }
        assert_eq!(cache.evictions(), 0);
        assert_eq!(cache.len(), 1);
        assert_eq!(
            cache.get("alpha").as_deref().map(String::as_str),
            Some("10")
        );
    }

    #[test]
    fn the_cap_evicts_the_oldest_entry_and_keeps_the_rest() {
        // 100 bytes per shard, 10 bytes an entry: each shard holds ten, and the
        // eleventh evicts the first -- not the other nine. That distinction is
        // the whole point of the policy; a shard-wide flush destroys the
        // working set and the walk recomputes it.
        let cache =
            build_weighted_cache::<String, Arc<String>>(100 * SHARD_COUNT as u64, ten_bytes);
        // Keys that share one shard, found by construction rather than by luck.
        let mut same_shard: Vec<String> = Vec::new();
        let mut candidate = 0;
        while same_shard.len() < 12 {
            let key = format!("key-{candidate}");
            if std::ptr::eq(cache.shard_of(key.as_str()), cache.shard_of("key-0")) {
                same_shard.push(key);
            }
            candidate += 1;
        }
        for key in &same_shard {
            cache.insert(key.clone(), Arc::new(key.clone()));
        }
        assert_eq!(
            cache.evictions(),
            2,
            "twelve 10-byte entries in a 100-byte shard evict exactly the two oldest"
        );
        assert_eq!(cache.get(same_shard[0].as_str()), None, "the oldest went");
        assert_eq!(cache.get(same_shard[1].as_str()), None, "and the next");
        for key in &same_shard[2..] {
            assert_eq!(
                cache.get(key.as_str()).as_deref().map(String::as_str),
                Some(key.as_str()),
                "everything younger than the evicted pair survives"
            );
        }
        assert_eq!(cache.len(), 10);
    }

    #[test]
    fn the_cap_bounds_a_flood_of_distinct_keys() {
        let cache = build_weighted_cache::<String, Arc<String>>(10 * SHARD_COUNT as u64, ten_bytes);
        for key in 0..4_000 {
            cache.insert(format!("key-{key}"), Arc::new(key.to_string()));
        }
        assert!(
            cache.evictions() > 0,
            "a 10-byte-per-shard budget must evict under 4,000 inserts"
        );
        assert!(
            cache.len() <= SHARD_COUNT,
            "entries retained: {}",
            cache.len()
        );
        cache.insert("fresh".to_string(), Arc::new("value".to_string()));
        assert_eq!(
            cache.get("fresh").as_deref().map(String::as_str),
            Some("value")
        );
    }

    #[test]
    fn an_oversized_value_is_admitted_rather_than_permanently_missed() {
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
