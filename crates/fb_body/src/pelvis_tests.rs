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
    for part in mesh.parts.iter().filter(|p| names.is_empty() || names.iter().any(|n| *n == p.name)) {
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
    crease_of(body, &body.skin_mesh(), &["trunk", "legs"], reach_scale)
}

/// [`crease`] of any mesh (the parts named `names`, or all of them where there are none).
fn crease_of(body: &Body, mesh: &armory::geom::mesh::ItemMesh, names: &[&str], reach_scale: f32) -> (f32, f32) {
    let step = 0.04 * body.scale;
    let span = (reach_scale / 0.04).round() as usize;
    let (from, to) = (body.waist_y + 0.1 * body.trunk_length, body.hip_y - 0.14 * body.trunk_length - 0.1 * body.scale);
    let reaches: Vec<Reach> = heights(from, to, step).into_iter().filter_map(|y| reach(mesh, names, y)).collect();
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

/// Gear of `base` fitted on `body` in `slot`, its outer surface.
fn worn_mesh(body: &Body, slot: fb_items::Slot, base: &str, seed: u32) -> armory::geom::mesh::ItemMesh {
    let item = fb_items::roll_item(base, seed).unwrap_or_else(|| panic!("no base {base}")).shape;
    fit_worn(body, &[(slot, item)]).layer(slot).expect("worn").mesh.clone()
}

/// How much further the cloth stands out from the skin, front and back, at the crotch than at
/// the hips (in the body's scale): cloth that follows the pelvis stands off by about the same
/// everywhere there, while cloth built on a deeper pelvis hangs out over the groin and seat as a
/// pouch.
fn pouch(body: &Body, cloth: &armory::geom::mesh::ItemMesh) -> Option<(f32, f32)> {
    let skin = body.skin_mesh();
    let off = |y: f32| {
        let (c, k) = (reach(cloth, &[], y)?, reach(&skin, &["trunk", "legs"], y)?);
        Some((c.front - k.front, k.back - c.back))
    };
    let crotch = body.hip_y - 0.14 * body.trunk_length;
    let (hip, low) = (off(body.hip_y)?, off(crotch + 0.1 * body.scale)?);
    Some(((low.0 - hip.0) / body.scale, (low.1 - hip.1) / body.scale))
}

/// How far the cloth's front-view half width stands out from the skin's at height `y`.
fn stand_x(body: &Body, cloth: &armory::geom::mesh::ItemMesh, y: f32) -> Option<f32> {
    Some(reach(cloth, &[], y)?.x - reach(&body.skin_mesh(), &["trunk", "legs"], y)?.x)
}

/// How deep the cloth's front-view outline dips between its neighbours (as [`crease`] does the side
/// profile) from the hips to the middle of the thigh, in the body's scale: a pinch under a lobe.
fn outline_dip(body: &Body, cloth: &armory::geom::mesh::ItemMesh, reach_scale: f32) -> f32 {
    let step = 0.04 * body.scale;
    let span = (reach_scale / 0.04).round() as usize;
    let (from, to) = (body.hip_y + 0.1 * body.trunk_length, body.hip_y - 1.8 * body.scale);
    let xs: Vec<f32> = heights(from, to, step).into_iter().filter_map(|y| reach(cloth, &[], y).map(|r| r.x)).collect();
    let mut dip = 0.0f32;
    for i in span..xs.len().saturating_sub(span) {
        dip = dip.max(0.5 * (xs[i - span] + xs[i + span]) - xs[i]);
    }
    dip / body.scale
}

/// Bodies the cloth tests run over: every folk, two seeds.
fn dressed_bodies() -> impl Iterator<Item = Body> {
    bodies().into_iter().filter(|b| b.spec.seed == 1207 || b.spec.seed == 1)
}

#[test]
fn trousers_hang_over_the_pelvis_without_a_pouch() {
    for body in dressed_bodies() {
        let trousers = worn_mesh(&body, fb_items::Slot::Legs, "Trousers", body.spec.seed);
        let (front, back) = pouch(&body, &trousers).expect("trousers over the crotch");
        assert!(front < 0.14, "{:?} seed {}: trousers stand out {front} (of the body's scale) further over the groin than at the hips", body.folk(), body.spec.seed);
        assert!(back < 0.13, "{:?} seed {}: trousers stand out {back} further behind at the crotch than at the hips", body.folk(), body.spec.seed);
    }
}

#[test]
fn a_tunic_or_robe_hangs_over_the_pelvis_without_a_pouch() {
    for body in dressed_bodies() {
        for (slot, base, bound) in [(fb_items::Slot::Shirt, "Tunic", 0.25), (fb_items::Slot::Body, "Robe", 0.3)] {
            let cloth = worn_mesh(&body, slot, base, body.spec.seed);
            // A tunic that ends above the crotch has no pouch to measure.
            let Some((front, _)) = pouch(&body, &cloth) else { continue };
            assert!(front < bound, "{:?} seed {}: a {base} stands out {front} (of the body's scale) further over the groin than at the hips", body.folk(), body.spec.seed);
        }
    }
}

#[test]
fn trousers_narrow_from_the_hips_into_the_thighs() {
    for body in dressed_bodies() {
        let cloth = worn_mesh(&body, fb_items::Slot::Legs, "Trousers", body.spec.seed);
        // The cloth stands off the skin evenly (its own ease), not more over the hips than
        // above them: no balloon.
        let reference = stand_x(&body, &cloth, body.hip_y + 0.3 * body.trunk_length).expect("cloth at the waist");
        let crotch = body.hip_y - 0.14 * body.trunk_length;
        let worst = heights(body.hip_y + 0.1 * body.trunk_length, crotch, 0.1 * body.scale).into_iter().filter_map(|y| stand_x(&body, &cloth, y)).fold(f32::MIN, f32::max);
        let growth = (worst - reference) / body.scale;
        assert!(growth < 0.02, "{:?} seed {}: trousers stand out {growth} (of the body's scale) further past the skin over the hips than at the waist", body.folk(), body.spec.seed);
        // And the outline from the hips to the thighs has no pinch after a lobe.
        for reach_scale in [0.2, 0.4] {
            let dip = outline_dip(&body, &cloth, reach_scale);
            assert!(dip < 0.04, "{:?} seed {}: the trousers' outline pinches by {dip} (over {reach_scale}) between the hips and the thighs", body.folk(), body.spec.seed);
        }
    }
}

#[test]
fn stacked_gear_over_the_hips_does_not_stand_further_out_than_one_layer_would() {
    for body in bodies().into_iter().filter(|b| b.spec.seed == 1207) {
        let item = |base: &str| fb_items::roll_item(base, 1207).unwrap_or_else(|| panic!("no base {base}")).shape;
        let stack = fit_worn(&body, &[(fb_items::Slot::Legs, item("Trousers")), (fb_items::Slot::Shirt, item("Tunic")), (fb_items::Slot::Belt, item("Belt"))]);
        let alone = worn_mesh(&body, fb_items::Slot::Legs, "Trousers", 1207);
        let (top, under) = (reach(&stack.layer(fb_items::Slot::Shirt).expect("tunic").mesh, &[], body.hip_y).expect("tunic"), reach(&alone, &[], body.hip_y).expect("trousers"));
        // A tunic over trousers stands off them by about its own ease, as it does over skin.
        for (name, over) in [("sideways", top.x - under.x), ("in front", top.front - under.front), ("behind", under.back - top.back)] {
            assert!(over < 0.4 * body.scale, "{:?}: a tunic over trousers stands {over} out {name} past them", body.folk());
        }
    }
}
