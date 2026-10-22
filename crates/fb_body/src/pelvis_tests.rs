//! The pelvis flows into the thighs and the seat: measured on the bare skin's trunk and legs,
//! sliced at heights from the waist to the crotch. From the front the trunk must not stand
//! proud of the thighs below the belly (a rounded mass that reads as a second rear), and from
//! the side the waist must meet the hips, and the seat the backs of the thighs, without a
//! ledge (a kink in the profile).

use super::*;

/// Where a body's skin reaches at a height: the widest |x|, the front z and the back z.
#[derive(Clone, Copy, Debug)]
struct Reach {
    x: f32,
    front: f32,
    back: f32,
}

/// The reach of the skin parts named `names` at height `y`, from where their triangle edges
/// cross it. `None` where none does.
fn reach(mesh: &armory::geom::mesh::ItemMesh, names: &[&str], y: f32) -> Option<Reach> {
    let mut r = Reach { x: 0.0, front: f32::MIN, back: f32::MAX };
    let mut any = false;
    for part in mesh.parts.iter().filter(|p| names.iter().any(|n| *n == p.name)) {
        for t in part.indices.chunks(3) {
            for k in 0..3 {
                let (p, q) = (part.positions[t[k] as usize], part.positions[t[(k + 1) % 3] as usize]);
                if (p[1] - y) * (q[1] - y) <= 0.0 && p[1] != q[1] {
                    let u = (y - p[1]) / (q[1] - p[1]);
                    r.x = r.x.max((p[0] + u * (q[0] - p[0])).abs());
                    let z = p[2] + u * (q[2] - p[2]);
                    r.front = r.front.max(z);
                    r.back = r.back.min(z);
                    any = true;
                }
            }
        }
    }
    any.then_some(r)
}

fn heights(from: f32, to: f32, step: f32) -> Vec<f32> {
    let n = ((from - to) / step).round().max(1.0) as usize;
    (0..=n).map(|i| from + (to - from) * i as f32 / n as f32).collect()
}

fn bodies() -> Vec<Body> {
    Folk::ALL.into_iter().flat_map(|f| [1u32, 1207, 40_401].map(move |s| Body::new(&BodySpec::roll(f, s)))).collect()
}

/// How far the trunk's skin stands out past the legs' at the front and the back, at the
/// worst height from the crotch up to the hips.
fn proud(body: &Body) -> (f32, f32) {
    let mesh = body.skin_mesh();
    let crotch = body.hip_y - 0.14 * body.trunk_length;
    let (mut front, mut back) = (f32::MIN, f32::MIN);
    for y in heights(body.hip_y, crotch + 0.05 * body.scale, 0.04 * body.scale) {
        if let (Some(t), Some(l)) = (reach(&mesh, &["trunk"], y), reach(&mesh, &["legs"], y)) {
            front = front.max(t.front - l.front);
            back = back.max(l.back - t.back);
        }
    }
    (front, back)
}

#[test]
fn the_pelvis_does_not_stand_proud_of_the_thighs_in_front() {
    for body in bodies() {
        let (front, _) = proud(&body);
        assert!(front < 0.08 * body.scale, "{:?} seed {}: the trunk stands {front} out past the thighs", body.folk(), body.spec.seed);
    }
}

#[test]
fn the_seat_does_not_overhang_the_backs_of_the_thighs() {
    for body in bodies() {
        let (_, back) = proud(&body);
        assert!(back < 0.15 * body.scale, "{:?} seed {}: the seat overhangs the thighs by {back}", body.folk(), body.spec.seed);
    }
}

/// The largest bend of the side profile (front and back) from just above the waist to the
/// crotch, as the change of slope between samples a fixed step apart.
fn side_kink(body: &Body) -> f32 {
    let mesh = body.skin_mesh();
    let step = 0.12 * body.scale;
    let (from, to) = (body.waist_y + 0.1 * body.trunk_length, body.hip_y - 0.14 * body.trunk_length - 0.1 * body.scale);
    let reaches: Vec<Reach> = heights(from, to, step).into_iter().filter_map(|y| reach(&mesh, &["trunk", "legs"], y)).collect();
    let mut worst = 0.0f32;
    for w in reaches.windows(3) {
        worst = worst.max((w[0].front - 2.0 * w[1].front + w[2].front).abs() / step);
        worst = worst.max((w[0].back - 2.0 * w[1].back + w[2].back).abs() / step);
    }
    worst
}

#[test]
fn the_side_profile_has_no_ledge_between_waist_and_thigh() {
    for body in bodies() {
        let kink = side_kink(&body);
        assert!(kink < 0.36, "{:?} seed {}: the side profile bends by {kink} from waist to crotch", body.folk(), body.spec.seed);
    }
}
