//! Cache of rendered equations keyed by source, mode and font size.

use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::sync::Arc;

use crate::MathError;
use crate::render::{RenderedMath, render_latex};

type CachedRender = Result<Arc<RenderedMath>, MathError>;

#[derive(Debug)]
struct Entry {
    source: Box<str>,
    display: bool,
    font_size_bits: u64,
    render: CachedRender,
    last_used: u64,
}

impl Entry {
    fn matches(&self, source: &str, display: bool, font_size_bits: u64) -> bool {
        self.display == display && self.font_size_bits == font_size_bits && *self.source == *source
    }
}

/// Remembers rendered equations, including failures, so re-rendering the
/// same equation is a hash lookup that allocates nothing. Past its capacity
/// it forgets the equations used least recently.
#[derive(Debug)]
pub struct MathCache {
    /// Entries by the hash of their key; a bucket holds the rare keys whose
    /// hashes collide.
    buckets: HashMap<u64, Vec<Entry>>,
    hasher: RandomState,
    len: usize,
    capacity: usize,
    clock: u64,
}

impl Default for MathCache {
    fn default() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }
}

impl MathCache {
    /// Equations a cache keeps before it forgets the least recently used.
    pub const DEFAULT_CAPACITY: usize = 2048;

    /// An empty cache holding up to [`Self::DEFAULT_CAPACITY`] equations.
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty cache holding up to `capacity` equations.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buckets: HashMap::new(),
            hasher: RandomState::new(),
            len: 0,
            capacity: capacity.max(1),
            clock: 0,
        }
    }

    fn key_hash(&self, source: &str, display: bool, font_size_bits: u64) -> u64 {
        self.hasher.hash_one((source, display, font_size_bits))
    }

    /// Returns the cached render of `source`, rendering it on a miss.
    pub fn render(&mut self, source: &str, display: bool, font_size: f64) -> CachedRender {
        let bits = font_size.to_bits();
        let hash = self.key_hash(source, display, bits);
        self.clock += 1;
        let clock = self.clock;
        let bucket = self.buckets.entry(hash).or_default();
        if let Some(entry) = bucket
            .iter_mut()
            .find(|entry| entry.matches(source, display, bits))
        {
            entry.last_used = clock;
            return entry.render.clone();
        }
        let render = render_latex(source, display, font_size).map(Arc::new);
        bucket.push(Entry {
            source: source.into(),
            display,
            font_size_bits: bits,
            render: render.clone(),
            last_used: clock,
        });
        self.len += 1;
        if self.len > self.capacity {
            self.forget_least_recent();
        }
        render
    }

    /// Drops the older half of the entries, so trimming costs little per
    /// render however full the cache runs.
    fn forget_least_recent(&mut self) {
        let mut ages: Vec<u64> = self
            .buckets
            .values()
            .flatten()
            .map(|entry| entry.last_used)
            .collect();
        let keep = (self.capacity / 2).max(1);
        let cutoff_index = ages.len() - keep;
        let (_, cutoff, _) = ages.select_nth_unstable(cutoff_index);
        let cutoff = *cutoff;
        self.buckets.retain(|_, bucket| {
            bucket.retain(|entry| entry.last_used >= cutoff);
            !bucket.is_empty()
        });
        self.len = self.buckets.values().map(Vec::len).sum();
    }

    /// Whether `source` has already been rendered with these settings.
    pub fn contains(&self, source: &str, display: bool, font_size: f64) -> bool {
        let bits = font_size.to_bits();
        self.buckets
            .get(&self.key_hash(source, display, bits))
            .is_some_and(|bucket| {
                bucket
                    .iter()
                    .any(|entry| entry.matches(source, display, bits))
            })
    }

    /// Number of cached equations.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Drops every cached render.
    pub fn clear(&mut self) {
        self.buckets.clear();
        self.len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_render_hits_the_cache() {
        let mut cache = MathCache::new();
        let first = cache.render(r"\sqrt{2}", false, 16.0).unwrap();
        let second = cache.render(r"\sqrt{2}", false, 16.0).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn key_includes_display_and_font_size() {
        let mut cache = MathCache::new();
        cache.render("x^2", false, 16.0).unwrap();
        cache.render("x^2", true, 16.0).unwrap();
        cache.render("x^2", false, 20.0).unwrap();
        assert_eq!(cache.len(), 3);
        assert!(cache.contains("x^2", true, 16.0));
        assert!(!cache.contains("x^2", true, 12.0));
    }

    #[test]
    fn failures_are_cached_too() {
        let mut cache = MathCache::new();
        let first = cache.render(r"\undefinedmacro", false, 16.0).unwrap_err();
        let second = cache.render(r"\undefinedmacro", false, 16.0).unwrap_err();
        assert_eq!(first, second);
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn a_full_cache_forgets_the_least_recently_used() {
        let mut cache = MathCache::with_capacity(4);
        for source in ["a", "b", "c", "d"] {
            cache.render(source, false, 16.0).unwrap();
        }
        cache.render("a", false, 16.0).unwrap();
        cache.render("b", false, 16.0).unwrap();
        cache.render("e", false, 16.0).unwrap();
        assert_eq!(cache.len(), 2);
        assert!(cache.contains("b", false, 16.0));
        assert!(cache.contains("e", false, 16.0));
        assert!(!cache.contains("c", false, 16.0));
        assert!(!cache.contains("a", false, 16.0));
    }

    #[test]
    fn hits_are_faster_than_misses() {
        let mut cache = MathCache::new();
        let source = r"\int_0^1 \frac{x^2}{1 + x^2} \, dx";
        let miss = std::time::Instant::now();
        cache.render(source, true, 16.0).unwrap();
        let miss = miss.elapsed();
        let hit = std::time::Instant::now();
        cache.render(source, true, 16.0).unwrap();
        let hit = hit.elapsed();
        assert!(hit * 10 < miss, "hit {hit:?} vs miss {miss:?}");
    }
}
