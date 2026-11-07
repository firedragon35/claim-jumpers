//! Randomness for the simulation.
//!
//! The browser calls `Math.random()` from everywhere. Here every random roll
//! goes through one [`Rng`] owned by the world, so a shift can be replayed
//! from a seed, and tests can pin every roll to a constant (the golden
//! traces were recorded with `Math.random` returning 0.5).

use std::f64::consts::TAU;

/// Source of uniform numbers in [0, 1).
#[derive(Clone, Debug)]
pub enum Rng {
    /// Mulberry32, the same generator the browser uses for daily seeds
    /// (src/js/10e-modes.js), so a seeded native shift is reproducible.
    Mulberry32(u32),
    /// Always returns this value. Used by tests to match the golden traces.
    Constant(f64),
}

impl Rng {
    pub fn seeded(seed: u32) -> Rng {
        Rng::Mulberry32(seed)
    }

    /// A seed from the clock, for normal play.
    pub fn from_time() -> Rng {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Rng::Mulberry32((nanos ^ (nanos >> 32)) as u32)
    }

    /// The next number in [0, 1): `Math.random()`.
    pub fn next(&mut self) -> f64 {
        match self {
            Rng::Constant(v) => *v,
            Rng::Mulberry32(state) => {
                // Mulberry32 with JavaScript's 32-bit integer semantics.
                *state = state.wrapping_add(0x6D2B_79F5);
                let mut t = *state;
                t = (t ^ (t >> 15)).wrapping_mul(t | 1);
                t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
                f64::from(t ^ (t >> 14)) / 4_294_967_296.0
            }
        }
    }

    /// Uniform in [a, b): `rand(a, b)`.
    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + self.next() * (b - a)
    }

    /// Uniform integer in [a, b], inclusive: `randInt(a, b)`.
    pub fn int(&mut self, a: i32, b: i32) -> i32 {
        self.range(f64::from(a), f64::from(b) + 1.0).floor() as i32
    }

    /// A random angle in [0, TAU).
    pub fn angle(&mut self) -> f64 {
        self.range(0.0, TAU)
    }

    /// True with probability `p`: `Math.random() < p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.next() < p
    }

    /// Index of a uniformly chosen element of a slice of length `len`.
    pub fn index(&mut self, len: usize) -> usize {
        ((self.next() * len as f64).floor() as usize).min(len.saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mulberry32_matches_browser_sequence() {
        // First outputs of mulberry32(1) in the browser:
        //   const r = mulberry32(1); [r(), r(), r()]
        let mut r = Rng::seeded(1);
        let got: Vec<f64> = (0..3).map(|_| r.next()).collect();
        let want = [0.6270739405881613, 0.002735721180215478, 0.5274470399599522];
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-15, "got {got:?}");
        }
    }

    #[test]
    fn constant_rolls_land_mid_range() {
        let mut r = Rng::Constant(0.5);
        assert_eq!(r.range(10.0, 20.0), 15.0);
        assert_eq!(r.int(1, 2), 2); // floor(1 + 0.5 * 2)
        assert!(!r.chance(0.28));
    }
}
