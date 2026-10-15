//! Flames that move a little: every Armory flame (a torch's, a lantern's, a candle's: the
//! `flame` part and the `flame_core` inside it, moved as one) stretches, narrows and leans
//! about its base, each on its own phase, so a lit
//! thing never looks frozen. The light it gives already flickers in the toon shader
//! (`flame_flicker` in `toon.wesl`); this moves the flame itself. The amounts and rates are our
//! own, kept small: a flame in still air, not one in a draught.

use bevy::prelude::*;

/// On a flame part ([`super::ItemSpawner::spawn`]): where its base is, in its item's space,
/// which it stretches and leans about, and its own phase.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Flame {
    pub base: Vec3,
    pub phase: f32,
}

impl Flame {
    /// The flame of part `positions` (its item's space): its base the middle of its lowest
    /// points, its phase from where it stands, so two flames side by side move apart.
    pub fn of(positions: &[[f32; 3]]) -> Option<Flame> {
        let low = positions.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
        if !low.is_finite() {
            return None;
        }
        let height = positions.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max) - low;
        let lowest: Vec<Vec3> = positions.iter().map(|&p| Vec3::from_array(p)).filter(|p| p.y <= low + height * 0.1).collect();
        let base = lowest.iter().sum::<Vec3>() / lowest.len() as f32;
        let phase = (base.x * 12.9898 + base.z * 78.233).sin().fract().abs() * std::f32::consts::TAU;
        Some(Flame { base, phase })
    }

    /// The flame's pose `t` seconds on: up to [`STRETCH`] taller (and as much narrower, so it
    /// keeps its body) and leaning up to [`LEAN`] radians, from a few sines of unrelated rates
    /// so it never visibly repeats.
    pub fn pose(&self, t: f32) -> Transform {
        let p = self.phase;
        let wave = |rate: f32, offset: f32| (t * rate + p + offset).sin();
        let stretch = 1.0 + STRETCH * (0.6 * wave(7.3, 0.0) + 0.4 * wave(11.9, 1.7));
        let width = 1.0 / stretch.sqrt();
        let lean_x = LEAN * (0.7 * wave(3.1, 0.4) + 0.3 * wave(8.7, 2.9));
        let lean_z = LEAN * (0.7 * wave(2.6, 3.3) + 0.3 * wave(9.4, 0.8));
        let rotation = Quat::from_euler(EulerRot::XYZ, lean_x, 0.0, lean_z);
        let scale = Vec3::new(width, stretch, width);
        // Turned and scaled about the base: p -> base + R S (p - base).
        Transform { translation: self.base - rotation * (scale * self.base), rotation, scale }
    }
}

/// How much taller a flame grows at most, as a share of its height. Our own.
pub const STRETCH: f32 = 0.08;
/// How far a flame leans at most, radians. Our own.
pub const LEAN: f32 = 0.06;

/// Moves every flame to its pose this moment.
pub fn flicker_flames(time: Res<Time>, mut flames: Query<(&Flame, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (flame, mut transform) in &mut flames {
        *transform = flame.pose(t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cone() -> Vec<[f32; 3]> {
        let mut points: Vec<[f32; 3]> = (0..8).map(|i| {
            let a = i as f32 * std::f32::consts::TAU / 8.0;
            [2.0 + 0.3 * a.cos(), 5.0, 0.3 * a.sin()]
        }).collect();
        points.push([2.0, 6.5, 0.0]);
        points
    }

    #[test]
    fn a_flame_moves_about_its_base_and_only_a_little() {
        let flame = Flame::of(&cone()).unwrap();
        assert!(flame.base.abs_diff_eq(Vec3::new(2.0, 5.0, 0.0), 1e-4), "{}", flame.base);
        let tip = Vec3::new(2.0, 6.5, 0.0);
        let mut moved = 0.0f32;
        for i in 0..200 {
            let pose = flame.pose(i as f32 * 0.037);
            // The base stays put; the tip wanders, but only a little.
            assert!(pose.transform_point(flame.base).abs_diff_eq(flame.base, 1e-4));
            let d = pose.transform_point(tip).distance(tip);
            assert!(d < 1.5 * (STRETCH + LEAN) * 1.5 + 0.05, "{d}");
            moved = moved.max(d);
        }
        assert!(moved > 0.05, "it moves: {moved}");
    }

    #[test]
    fn flames_side_by_side_move_apart() {
        let a = Flame::of(&cone()).unwrap();
        let shifted: Vec<[f32; 3]> = cone().iter().map(|p| [p[0] + 3.0, p[1], p[2] + 1.0]).collect();
        let b = Flame::of(&shifted).unwrap();
        assert_ne!(a.phase, b.phase);
    }
}
