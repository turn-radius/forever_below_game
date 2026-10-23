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

/// How deep the side silhouette dips between its neighbours, in units of the body's scale:
/// how far a point of the profile lies behind the chord joining the points `reach` above
/// and below it (a crease, a ledge's underside, a step). The seat that carries on into the
/// thigh and the groin that narrows into it have none; the worst dip is taken over every
/// height from just above the waist to just below the crotch (`front`, `back`). Samples a
/// fine step apart, so where the grid falls does not matter.
fn crease(body: &Body, reach_scale: f32) -> (f32, f32) {
    let mesh = body.skin_mesh();
    let step = 0.04 * body.scale;
    let span = (reach_scale / 0.04).round() as usize;
    let (from, to) = (body.waist_y + 0.1 * body.trunk_length, body.hip_y - 0.14 * body.trunk_length - 0.1 * body.scale);
    let reaches: Vec<Reach> = heights(from, to, step).into_iter().filter_map(|y| reach(&mesh, &["trunk", "legs"], y)).collect();
    let (mut front, mut back) = (0.0f32, 0.0f32);
    for i in span..reaches.len().saturating_sub(span) {
        let (a, m, b) = (reaches[i - span], reaches[i], reaches[i + span]);
        front = front.max(0.5 * (a.front + b.front) - m.front);
        back = back.max(m.back - 0.5 * (a.back + b.back));
    }
    (front / body.scale, back / body.scale)
}

#[test]
fn the_front_has_no_crease_over_the_groin() {
    for body in bodies() {
        let (front, _) = crease(&body, 0.2);
        assert!(front < 0.02, "{:?} seed {}: the front profile dips {front} (of the body's scale) between waist and crotch", body.folk(), body.spec.seed);
    }
}

#[test]
fn the_seat_rounds_into_the_backs_of_the_thighs() {
    for body in bodies() {
        for reach in [0.1, 0.2] {
            let (_, back) = crease(&body, reach);
            assert!(back < 0.022, "{:?} seed {}: the back profile dips {back} (of the body's scale, over {reach}) between waist and thigh", body.folk(), body.spec.seed);
        }
        let (_, over) = proud(&body);
        assert!(over < 0.15 * body.scale, "{:?} seed {}: the seat overhangs the thighs by {over}", body.folk(), body.spec.seed);
    }
}
