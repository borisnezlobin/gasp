//! A small SplitMix64 generator, so the corpus never changes because a crate changed its algorithm.

/// Deterministic pseudo-random number generator.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Creates a generator from a seed.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Creates an independent generator for one numbered stream of a seed.
    pub fn stream(seed: u64, stream: u64) -> Self {
        let mut rng = Self::new(seed ^ stream.wrapping_mul(0xD1B5_4A32_D192_ED03));
        rng.next_u64();
        rng
    }

    /// Returns the next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Returns a value in `0..n`. `n` must be positive.
    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "below(0) has no values to pick from");
        (self.next_u64() % n as u64) as usize
    }

    /// Returns a value in `low..=high`.
    pub fn range(&mut self, low: usize, high: usize) -> usize {
        low + self.below(high - low + 1)
    }

    /// Returns a float in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Returns true with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    /// Picks one item of a non-empty slice.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// Shuffles a slice in place (Fisher-Yates).
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }

    /// Picks an index with probability proportional to its weight.
    pub fn weighted(&mut self, weights: &[u32]) -> usize {
        let total: u32 = weights.iter().sum();
        let mut roll = self.below(total as usize) as u32;
        for (index, weight) in weights.iter().enumerate() {
            if roll < *weight {
                return index;
            }
            roll -= weight;
        }
        weights.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_same_sequence() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn streams_differ() {
        assert_ne!(Rng::stream(1, 1).next_u64(), Rng::stream(1, 2).next_u64());
    }

    #[test]
    fn range_stays_in_bounds() {
        let mut rng = Rng::new(3);
        for _ in 0..1000 {
            let value = rng.range(4, 9);
            assert!((4..=9).contains(&value));
        }
    }

    #[test]
    fn weighted_never_picks_zero_weight() {
        let mut rng = Rng::new(11);
        for _ in 0..500 {
            assert_ne!(rng.weighted(&[3, 0, 5]), 1);
        }
    }

    #[test]
    fn shuffle_keeps_items() {
        let mut rng = Rng::new(5);
        let mut items: Vec<u32> = (0..20).collect();
        rng.shuffle(&mut items);
        items.sort_unstable();
        assert_eq!(items, (0..20).collect::<Vec<_>>());
    }
}
