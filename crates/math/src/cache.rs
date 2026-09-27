//! Cache of rendered equations keyed by source, mode and font size.

use std::collections::HashMap;
use std::sync::Arc;

use crate::MathError;
use crate::render::{RenderedMath, render_latex};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    source: String,
    display: bool,
    font_size_bits: u64,
}

impl CacheKey {
    fn new(source: &str, display: bool, font_size: f64) -> Self {
        Self {
            source: source.to_owned(),
            display,
            font_size_bits: font_size.to_bits(),
        }
    }
}

type CachedRender = Result<Arc<RenderedMath>, MathError>;

/// Remembers rendered equations, including failures, so re-rendering the
/// same equation is a hash lookup.
#[derive(Debug, Default)]
pub struct MathCache {
    entries: HashMap<CacheKey, CachedRender>,
}

impl MathCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the cached render of `source`, rendering it on a miss.
    pub fn render(&mut self, source: &str, display: bool, font_size: f64) -> CachedRender {
        let key = CacheKey::new(source, display, font_size);
        self.entries
            .entry(key)
            .or_insert_with(|| render_latex(source, display, font_size).map(Arc::new))
            .clone()
    }

    /// Whether `source` has already been rendered with these settings.
    pub fn contains(&self, source: &str, display: bool, font_size: f64) -> bool {
        self.entries
            .contains_key(&CacheKey::new(source, display, font_size))
    }

    /// Number of cached equations.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Drops every cached render.
    pub fn clear(&mut self) {
        self.entries.clear();
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
