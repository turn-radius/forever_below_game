//! Flames that move a little: every Armory flame (a torch's, a lantern's, a candle's: the
//! `flame` part and the `flame_core` inside it, moved as one) stretches, narrows and leans
//! about its base, each on its own phase, so a lit
//! thing never looks frozen. A flame is built along its item's own up, but fire rises: each
//! flame is turned about its base to stand upright however its item is held or lies
//! ([`FlameUp`]), so a leaning torch's flame climbs rather than reaching out along the handle.
//! A torch's flame also leans forward along its frame's +Z (`armory::build::light`), so it is
//! then yawed to lean the way the item faces, or along [`FlameFacing`] (the way the delver
//! looks) for what is held. The flame is see-through at its base and solid at its tip
//! ([`flame_alpha`]). The light it gives already flickers in the toon shader
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

    /// The flame's pose `t` seconds on, standing along `up` (its item's space): up to
    /// [`STRETCH`] taller (and as much narrower, so it keeps its body) and leaning up to
    /// [`LEAN`] radians, from a few sines of unrelated rates so it never visibly repeats.
    ///
    /// The flame frame's +Z is the way a leaning flame bends. Turned upright it is yawed about
    /// `up` to lie along `facing` (horizontal part only); without a `facing` it keeps the way
    /// its item faces.
    pub fn pose(&self, t: f32, up: Vec3, facing: Option<Vec3>) -> Transform {
        let p = self.phase;
        let wave = |rate: f32, offset: f32| (t * rate + p + offset).sin();
        let stretch = 1.0 + STRETCH * (0.6 * wave(7.3, 0.0) + 0.4 * wave(11.9, 1.7));
        let width = 1.0 / stretch.sqrt();
        let lean_x = LEAN * (0.7 * wave(3.1, 0.4) + 0.3 * wave(8.7, 2.9));
        let lean_z = LEAN * (0.7 * wave(2.6, 3.3) + 0.3 * wave(9.4, 0.8));
        let up = up.normalize_or(Vec3::Y);
        let upright = Quat::from_rotation_arc(Vec3::Y, up);
        let upright = yaw_towards(up, upright * Vec3::Z, facing).unwrap_or(Quat::IDENTITY) * upright;
        let rotation = upright * Quat::from_euler(EulerRot::XYZ, lean_x, 0.0, lean_z);
        let scale = Vec3::new(width, stretch, width);
        // Turned and scaled about the base: p -> base + R S (p - base).
        Transform { translation: self.base - rotation * (scale * self.base), rotation, scale }
    }
}

/// The turn about `up` that carries `forward`'s horizontal part onto `facing`'s, or `None`
/// when either has none (looking straight up or down) or there is nothing to face.
pub fn yaw_towards(up: Vec3, forward: Vec3, facing: Option<Vec3>) -> Option<Quat> {
    let flat = |v: Vec3| {
        let v = v - up * v.dot(up);
        (v.length() > 1e-4).then(|| v.normalize())
    };
    let (from, to) = (flat(forward)?, flat(facing?)?);
    Some(Quat::from_axis_angle(up, up.dot(from.cross(to)).atan2(from.dot(to))))
}

/// Share of a held flame's lean that points outward, toward the screen edge its hand is on;
/// the rest points forward. Straight ahead the lean is foreshortened to nothing in first
/// person, so a held flame leans forward and out. Our own tuning.
pub const HELD_OUTWARD: f32 = 0.45;

/// The horizontal way a held flame leans, in the world: `ahead` blended with `right` (both
/// horizontal, unit), toward the right for the main hand (`outward` 1) and the left for the
/// off hand (`outward` -1).
pub fn held_facing(ahead: Vec3, right: Vec3, outward: f32) -> Vec3 {
    ((1.0 - HELD_OUTWARD) * ahead + HELD_OUTWARD * outward * right).normalize_or(ahead)
}

/// Which of a flame's two meshes a part is: the outer veil, or the bright core inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlameLayer {
    Veil,
    Core,
}

/// A flame's opacity `t` of the way up its layer (0 at the base, 1 at the tip). The veil is
/// a thin skin, mostly see-through (0.15 at the base, 0.3 half way up) and only building
/// toward its tip; the core inside it is what shows, from 0.35 at its base up to 0.9 at its
/// top. The burning head shows through the lower flame. Our own.
pub fn flame_alpha(layer: FlameLayer, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match layer {
        FlameLayer::Veil if t < 0.5 => 0.15 + 0.3 * t,
        FlameLayer::Veil => 0.3 + 0.4 * (t - 0.5),
        FlameLayer::Core => 0.35 + 0.55 * t,
    }
}

/// A flame's colour (display sRGB) `t` of the way up its layer: the core from a pale
/// yellow-white at its base through yellow, the veil warming from yellow up through orange to a
/// deep red-orange at its tip. The shader reddens the veil's edges further (`toon.wesl`). Our
/// own palette.
pub fn flame_colour(layer: FlameLayer, t: f32) -> [f32; 3] {
    const VEIL: [(f32, [f32; 3]); 4] = [(0.0, [1.0, 0.78, 0.30]), (0.35, [1.0, 0.55, 0.12]), (0.7, [0.95, 0.33, 0.07]), (1.0, [0.72, 0.14, 0.04])];
    const CORE: [(f32, [f32; 3]); 3] = [(0.0, [1.0, 0.97, 0.78]), (0.5, [1.0, 0.88, 0.45]), (1.0, [1.0, 0.72, 0.25])];
    let stops: &[(f32, [f32; 3])] = if layer == FlameLayer::Core { &CORE } else { &VEIL };
    let t = t.clamp(0.0, 1.0);
    let upper = stops.iter().position(|s| s.0 >= t).unwrap_or(stops.len() - 1).max(1);
    let (a, b) = (stops[upper - 1], stops[upper]);
    let k = ((t - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
    std::array::from_fn(|i| a.1[i] + (b.1[i] - a.1[i]) * k)
}

/// The vertex colours of a flame part: [`flame_colour`] and [`flame_alpha`] by height along
/// the part, as the linear RGBA a mesh carries. The material is white so these are the
/// flame's colours.
pub fn flame_vertex_colours(positions: &[[f32; 3]], layer: FlameLayer) -> Vec<[f32; 4]> {
    let low = positions.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let high = positions.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
    let span = (high - low).max(1e-6);
    positions
        .iter()
        .map(|p| {
            let t = (p[1] - low) / span;
            let [r, g, b] = flame_colour(layer, t);
            let linear = Color::srgb(r, g, b).to_linear();
            [linear.red, linear.green, linear.blue, flame_alpha(layer, t)]
        })
        .collect()
}

/// How much taller a flame grows at most, as a share of its height. Our own.
pub const STRETCH: f32 = 0.08;
/// How far a flame leans at most, radians. Our own.
pub const LEAN: f32 = 0.06;

/// On an item's root: the way its flames rise, in the space its transform is in (the world,
/// or a stage). Without it, up is +Y. Held things in first person are drawn on the view
/// model's stage, where the world's up is wherever the eye's look puts it
/// (`crate::carry::upright_held_flames`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct FlameUp(pub Vec3);

impl Default for FlameUp {
    fn default() -> Self {
        FlameUp(Vec3::Y)
    }
}

/// On an item's root: the horizontal way its flames lean, in the space its transform is in,
/// for what is held: the way the delver looks. Without it a flame leans the way its item
/// faces (its +Z, flattened).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct FlameFacing(pub Vec3);

/// Moves every flame to its pose this moment, upright and leaning the way it faces.
pub fn flicker_flames(
    time: Res<Time>,
    mut flames: Query<(&Flame, &ChildOf, &mut Transform)>,
    items: Query<(&GlobalTransform, Option<&FlameUp>, Option<&FlameFacing>)>,
) {
    let t = time.elapsed_secs();
    for (flame, parent, mut transform) in &mut flames {
        let (up, facing) = match items.get(parent.parent()) {
            Ok((at, up, facing)) => (upright_in(at, up.copied().unwrap_or_default()), facing.map(|f| item_space(at, f.0))),
            Err(_) => (Vec3::Y, None),
        };
        *transform = flame.pose(t, up, facing);
    }
}

/// `direction` (in the space `item`'s transform is in) carried into the item's own space.
pub fn item_space(item: &GlobalTransform, direction: Vec3) -> Vec3 {
    let (_, rotation, _) = item.to_scale_rotation_translation();
    rotation.inverse() * direction
}

/// `up` (in the space `item`'s transform is in) carried into the item's own space.
pub fn upright_in(item: &GlobalTransform, up: FlameUp) -> Vec3 {
    item_space(item, up.0).normalize_or(Vec3::Y)
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
            let pose = flame.pose(i as f32 * 0.037, Vec3::Y, None);
            // The base stays put; the tip wanders, but only a little.
            assert!(pose.transform_point(flame.base).abs_diff_eq(flame.base, 1e-4));
            let d = pose.transform_point(tip).distance(tip);
            assert!(d < 1.5 * (STRETCH + LEAN) * 1.5 + 0.05, "{d}");
            moved = moved.max(d);
        }
        assert!(moved > 0.05, "it moves: {moved}");
    }

    #[test]
    fn a_flame_rises_however_its_item_lies() {
        let flame = Flame::of(&cone()).unwrap();
        let tip = Vec3::new(2.0, 6.5, 0.0);
        // A torch laid on its side, its own up pointing east.
        let lying = GlobalTransform::from(Transform::from_rotation(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)));
        let up = upright_in(&lying, FlameUp::default());
        let pose = flame.pose(0.0, up, None);
        let (base_world, tip_world) = (lying.transform_point(pose.transform_point(flame.base)), lying.transform_point(pose.transform_point(tip)));
        assert!(pose.transform_point(flame.base).abs_diff_eq(flame.base, 1e-4), "the base stays on the wick");
        let rise = (tip_world - base_world).normalize();
        assert!(rise.y > 0.95, "the tip is above the base: {rise}");
        // Upright already, it stands as built.
        assert!(upright_in(&GlobalTransform::IDENTITY, FlameUp::default()).abs_diff_eq(Vec3::Y, 1e-6));
    }

    #[test]
    fn flames_side_by_side_move_apart() {
        let a = Flame::of(&cone()).unwrap();
        let shifted: Vec<[f32; 3]> = cone().iter().map(|p| [p[0] + 3.0, p[1], p[2] + 1.0]).collect();
        let b = Flame::of(&shifted).unwrap();
        assert_ne!(a.phase, b.phase);
    }

    #[test]
    fn the_flicker_never_moves_the_base() {
        let flame = Flame::of(&cone()).unwrap();
        for facing in [None, Some(Vec3::X), Some(Vec3::new(-1.0, 0.3, 2.0))] {
            for up in [Vec3::Y, Vec3::new(1.0, 1.0, 0.0), Vec3::NEG_X] {
                for i in 0..100 {
                    let pose = flame.pose(i as f32 * 0.11, up, facing);
                    assert!(pose.transform_point(flame.base).abs_diff_eq(flame.base, 1e-4));
                }
            }
        }
    }

    #[test]
    fn a_held_flame_yaws_to_face_forward() {
        let flame = Flame::of(&cone()).unwrap();
        // A flame leans along its frame's +Z. Held, it should lean along the way the delver looks.
        for facing in [Vec3::NEG_Z, Vec3::X, Vec3::NEG_X, Vec3::new(1.0, 0.4, -1.0)] {
            let pose = flame.pose(0.0, Vec3::Y, Some(facing));
            // Still upright, and its lean (+Z) lies along the horizontal of `facing`.
            assert!((pose.rotation * Vec3::Y).y > 0.99);
            let lean = pose.rotation * Vec3::Z;
            let flat = Vec3::new(facing.x, 0.0, facing.z).normalize();
            assert!(lean.dot(flat) > 0.99, "lean {lean} facing {flat}");
        }
        // Without a facing the flame keeps the way its item faces.
        let pose = flame.pose(0.0, Vec3::Y, None);
        assert!((pose.rotation * Vec3::Z).dot(Vec3::Z) > 0.99);
        // Looking straight down gives nothing to face.
        assert!(yaw_towards(Vec3::Y, Vec3::Z, Some(Vec3::NEG_Y)).is_none());
    }

    #[test]
    fn a_held_flame_leans_forward_and_out_by_hand() {
        // World forward is -Z, right is +X.
        let main = held_facing(Vec3::NEG_Z, Vec3::X, 1.0);
        let off = held_facing(Vec3::NEG_Z, Vec3::X, -1.0);
        assert!(main.x > 0.3 && main.z < -0.5 && (main.length() - 1.0).abs() < 1e-5, "{main}");
        assert!(off.x < -0.3 && off.z < -0.5, "{off}");
        assert!((main.x + off.x).abs() < 1e-6);
        // The flame yaws to it: leaning right and ahead for the main hand, left and ahead for the off hand.
        let flame = Flame::of(&cone()).unwrap();
        for (facing, side) in [(main, 1.0), (off, -1.0)] {
            let lean = flame.pose(0.0, Vec3::Y, Some(facing)).rotation * Vec3::Z;
            assert!(lean.x * side > 0.3 && lean.z < -0.5, "{lean}");
        }
    }

    #[test]
    fn a_tilted_stage_still_leans_a_flame_forward() {
        let flame = Flame::of(&cone()).unwrap();
        // The view model's stage is the eye's space: up is tilted by the eye's pitch.
        let pitch = Quat::from_rotation_x(0.6);
        let pose = flame.pose(0.0, pitch.inverse() * Vec3::Y, Some(pitch.inverse() * Vec3::NEG_Z));
        // Seen from the world the lean points along world forward.
        let world = pitch * (pose.rotation * Vec3::Z);
        assert!(world.dot(Vec3::NEG_Z) > 0.99, "{world}");
    }

    #[test]
    fn a_flame_is_see_through_at_its_base_and_builds_toward_the_core() {
        for layer in [FlameLayer::Veil, FlameLayer::Core] {
            assert!(flame_alpha(layer, 0.0) < flame_alpha(layer, 0.5) && flame_alpha(layer, 0.5) < flame_alpha(layer, 1.0), "{layer:?}");
        }
        // The veil is mostly see-through over its lower half; the core is what builds up.
        for i in 0..=10 {
            let alpha = flame_alpha(FlameLayer::Veil, i as f32 * 0.05);
            assert!((0.15..=0.3 + 1e-6).contains(&alpha), "{alpha}");
        }
        assert!((flame_alpha(FlameLayer::Veil, 0.0) - 0.15).abs() < 1e-6);
        assert!(flame_alpha(FlameLayer::Core, 1.0) > 3.0 * flame_alpha(FlameLayer::Veil, 0.0));
        let points = cone();
        for (c, p) in flame_vertex_colours(&points, FlameLayer::Veil).iter().zip(&points) {
            let want = if p[1] == 5.0 { 0.15 } else { 0.5 };
            assert!((c[3] - want).abs() < 1e-5, "{c:?}");
        }
    }

    #[test]
    fn the_colour_runs_from_a_pale_core_to_a_deep_red_tip() {
        let luma = |c: [f32; 3]| 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
        let core_base = flame_colour(FlameLayer::Core, 0.0);
        let veil_tip = flame_colour(FlameLayer::Veil, 1.0);
        // The core is pale (nearly white) and brightest; the tip is deep red.
        assert!(core_base[2] > 0.7 && luma(core_base) > luma(veil_tip) + 0.5, "{core_base:?} {veil_tip:?}");
        assert!(veil_tip[0] > veil_tip[1] * 3.0 && veil_tip[2] < 0.1, "{veil_tip:?}");
        // The veil warms steadily: green (and so yellow) drains out going up.
        let greens: Vec<f32> = (0..=10).map(|i| flame_colour(FlameLayer::Veil, i as f32 / 10.0)[1]).collect();
        assert!(greens.windows(2).all(|w| w[1] <= w[0]), "{greens:?}");
        // Yellow low, orange mid, red high.
        assert!(flame_colour(FlameLayer::Veil, 0.0)[1] > 0.7 && (0.4..0.65).contains(&flame_colour(FlameLayer::Veil, 0.35)[1]));
        // The core cools to yellow-orange upward and is paler than the veil at any height.
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            assert!(luma(flame_colour(FlameLayer::Core, t)) > luma(flame_colour(FlameLayer::Veil, t)), "{t}");
        }
    }
}
