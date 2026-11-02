//! The curves a lasting glow follows: it blooms in past its size and settles, and sputters when
//! it dies. Divine Light's `glow` (`game/src/casting.rs`) and an effect's light share them.

/// An ease that overshoots and settles back, for `risen` (0 to 1) of the way through the
/// bloom: 0 at the start, a little over 1 near the end, 1 once risen.
pub fn bloom(risen: f32) -> f32 {
    let risen = risen.clamp(0.0, 1.0);
    let (c1, c3) = (1.70158, 2.70158);
    (1.0 + c3 * (risen - 1.0).powi(3) + c1 * (risen - 1.0).powi(2)).max(0.0)
}

/// How much of its brightness a glow keeps at real time `t` with `left` of its time (1 full,
/// 0 out): it flutters harder the nearer it is to out.
pub fn sputter(left: f32, t: f32) -> f32 {
    let dying = 1.0 - left.clamp(0.0, 1.0);
    let flutter = (0.5 + 0.5 * (t * 17.0 + 3.0 * (t * 5.3).sin()).sin()) * (0.5 + 0.5 * (t * 29.0 + 1.3).sin());
    1.0 - 0.7 * dying.powf(1.5) * flutter
}

/// A glow's brightness `age` seconds in, as a share of its fullest: it blooms over `bloom_in`
/// seconds (none at 0), has `left` of its time, and sputters at real time `t`.
pub fn brightness(age: f32, bloom_in: f32, left: f32, t: f32) -> f32 {
    let risen = if bloom_in > 0.0 { age / bloom_in } else { 1.0 };
    bloom(risen) * left.clamp(0.0, 1.0) * sputter(left, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_blooms_past_full_and_settles() {
        assert_eq!(bloom(0.0), 0.0);
        assert!(bloom(0.7) > 1.0);
        assert_eq!(bloom(1.0), 1.0);
        assert_eq!(bloom(3.0), 1.0);
    }

    #[test]
    fn it_flutters_only_when_dying() {
        assert_eq!(sputter(1.0, 3.3), 1.0);
        assert!((0.3..=1.0).contains(&sputter(0.1, 3.3)));
    }
}
