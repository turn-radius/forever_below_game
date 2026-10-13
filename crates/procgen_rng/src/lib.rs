//! Seeded randomness for the procedural generators.
//!
//! Every generator in the browser prototypes (Spline Armory, the Forever Below plate,
//! Florilegium, Armiger) uses `mulberry32`. This is a bit-exact port, so a seed in Rust
//! produces the same stream of numbers as the same seed in the prototype. That
//! is a convenience for comparing against the prototypes, not a requirement.

/// mulberry32: a small, fast 32-bit generator. Returns values in `[0, 1)`.
#[derive(Clone, Debug)]
pub struct Mulberry32 {
    state: u32,
}

impl Mulberry32 {
    /// Seed the generator. Matches `mulberry32(a)` in the prototypes, where `a` is
    /// coerced with `a|0` (so any u32 bit pattern is valid).
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// Next value in `[0, 1)`, identical to the JS:
    /// `a=a+0x6D2B79F5|0; t=imul(a^a>>>15,1|a); t=t+imul(t^t>>>7,61|t)^t; return ((t^t>>>14)>>>0)/4294967296`
    pub fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6D2B_79F5);
        let a = self.state;
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        ((t ^ (t >> 14)) as f64) / 4_294_967_296.0
    }

    /// Uniform pick from a slice. Matches `a[Math.floor(rnd()*a.length)]`.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        let i = (self.next_f64() * items.len() as f64).floor() as usize;
        &items[i.min(items.len() - 1)]
    }

    /// `rnd() < p`
    pub fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }
}

/// The Spline Armory randomizer seeds its stream with `(seed * 2654435761) >>> 0`:
/// a full-precision multiply, keeping the low 32 bits.
pub fn armory_stream_seed(seed: u32) -> u32 {
    ((seed as u64).wrapping_mul(2_654_435_761) & 0xFFFF_FFFF) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Streams recorded from the JS implementation in `fixtures/rng/mulberry32.json`.
    #[test]
    fn matches_prototype_streams() {
        let raw = include_str!("../../../fixtures/rng/mulberry32.json");
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        let streams = v["streams"].as_object().unwrap();
        assert!(!streams.is_empty());
        for (seed, values) in streams {
            let mut r = Mulberry32::new(seed.parse::<u64>().unwrap() as u32);
            for (i, want) in values.as_array().unwrap().iter().enumerate() {
                let got = r.next_f64();
                assert_eq!(got, want.as_f64().unwrap(), "seed {seed}, draw {i}");
            }
        }
    }

    #[test]
    fn armory_seed_transform() {
        // 1207 is the default plate seed in Spline Armory.
        assert_eq!(armory_stream_seed(1207), ((1207u64 * 2_654_435_761) % (1u64 << 32)) as u32);
    }
}
