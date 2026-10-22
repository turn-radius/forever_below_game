//! Specs for what a hood, a coif or a cloak does round the face and over the shoulders: the
//! face stays open (nothing crosses the nose or mouth, a collar stops at the jaw) and the cape
//! lies on the shoulders and arms instead of standing out from them like a disc.

use armory::families::hood::Hoods;
use armory::geom::vec3::{self, V3};
use armory::sections::hood::HoodStyle;
use armory::Item;
use fb_items::Slot;
use std::f32::consts::PI;

use crate::body::Body;
use crate::gait::{Gait, IdleCycle};
use crate::head::Face;
use crate::rig::Rig;
use crate::skin::{PartSkin, SkinRegion, Skinner};
use crate::{BodySpec, Folk, fit_worn};

/// A named kit: what the reported screenshots wore, and the extremes round them.
struct Kit {
    name: &'static str,
    worn: Vec<(Slot, Item)>,
}

fn rolled(base: &str, seed: u32) -> Item {
    fb_items::roll_item(base, seed).unwrap_or_else(|| panic!("no base {base}")).shape
}

/// A hood of `style` rolled from `seed`, its face opening and ventail set.
fn hood_of(seed: u32, style: HoodStyle, opening: Option<f32>, ventail: Option<bool>) -> Item {
    let mut hood = Hoods::roll_as(seed, style);
    if let Some(o) = opening {
        hood.hood.opening = o;
    }
    if let Some(v) = ventail {
        hood.hood.ventail = v;
    }
    Item::Hood(hood)
}

/// The kits, rolled from `seed`: the reported ones (a mail coif over a shirt and over a hauberk, a
/// hood over a hauberk with a pack, a hood with a cloak on the shoulders), a mail coif forced to
/// have its ventail, hoods with the face opening at both extremes, and a short cloak (a mantle).
fn kits(seed: u32) -> Vec<Kit> {
    let base = |shirt: &str| vec![(Slot::Shirt, rolled(shirt, seed)), (Slot::Legs, rolled("Trousers", seed))];
    let with = |mut worn: Vec<(Slot, Item)>, more: Vec<(Slot, Item)>| {
        worn.extend(more);
        worn
    };
    let hauberk = || (Slot::Body, rolled("Mail", seed));
    let mut mantle = rolled("Cloak", seed);
    if let Item::Garment(g) = &mut mantle {
        g.cut.length = 0.0;
    }
    vec![
        Kit { name: "coif over a shirt", worn: with(base("Tunic"), vec![(Slot::Head, rolled("Mail coif", seed))]) },
        Kit { name: "coif over a hauberk", worn: with(base("Shirt"), vec![hauberk(), (Slot::Head, rolled("Mail coif", seed))]) },
        Kit { name: "hood over a hauberk with a pack", worn: with(base("Tunic"), vec![hauberk(), (Slot::Head, rolled("Hood", seed)), (Slot::Back, rolled("Pack", seed))]) },
        Kit { name: "hood with a cloak", worn: with(base("Shirt"), vec![(Slot::Head, rolled("Hood", seed)), (Slot::Cloak, rolled("Cloak", seed))]) },
        Kit { name: "coif with a ventail", worn: with(base("Shirt"), vec![hauberk(), (Slot::Head, hood_of(seed, HoodStyle::MailCoif, None, Some(true)))]) },
        Kit { name: "hood with the narrowest opening", worn: with(base("Tunic"), vec![hauberk(), (Slot::Head, hood_of(seed, HoodStyle::Hood, Some(0.0), None))]) },
        Kit { name: "hood with the widest opening", worn: with(base("Tunic"), vec![hauberk(), (Slot::Head, hood_of(seed, HoodStyle::Hood, Some(1.0), None))]) },
        Kit { name: "cowl with the narrowest opening", worn: with(base("Shirt"), vec![(Slot::Head, hood_of(seed, HoodStyle::Cowl, Some(0.0), None))]) },
        Kit { name: "hood with a mantle", worn: with(base("Shirt"), vec![(Slot::Head, rolled("Hood", seed)), (Slot::Cloak, mantle)]) },
    ]
}

/// The gear and bodies in a pose (the standing stance, the idle or a stride of the walk): every
/// worn part's vertices and triangles as the game's skin weights move them (a hood rides the
/// chest, and so does the head inside it).
struct Posed<'a> {
    layers: Vec<(Slot, &'a str, Vec<V3>, &'a [u32])>,
    skin: Vec<(&'a str, Vec<V3>, &'a [u32])>,
    /// The head's frame as the pose has it: its centre and its three axes.
    head: (V3, [V3; 3]),
}

fn poses(body: &Body) -> Vec<(String, Option<crate::rig::RigPose>)> {
    let rig = Rig::of(body);
    let gait = Gait::new(body, &rig);
    let mut out = vec![("standing".to_string(), None)];
    out.push(("idle".to_string(), Some(gait.idle(&IdleCycle::of(1207), 3.0))));
    // The two strides where the arms are farthest from the trunk's line (the legs pass and
    // are apart); the others are the same by symmetry.
    out.extend([0.0f32, 0.25].map(|p| (format!("walk {p}"), Some(gait.walk(p, gait.natural_speed())))));
    out
}

/// A kit fitted and weighted once, to be posed many times.
struct Fitted {
    layers: Vec<(Slot, &'static str, Vec<V3>, Vec<u32>, PartSkin)>,
    skin: Vec<(&'static str, Vec<V3>, Vec<u32>, PartSkin)>,
    rig: Rig,
    centre: V3,
    /// The head's centre and the points a unit along its axes, weighted as the hood and the head
    /// are (to the chest under a hood).
    refs: [(V3, PartSkin); 4],
}

fn fitted(body: &Body, worn: &[(Slot, Item)]) -> Fitted {
    let hooded = worn.iter().any(|(s, i)| *s == Slot::Head && matches!(i, Item::Hood(_)));
    let fit = fit_worn(body, worn);
    let skinner = Skinner::new(body);
    let mut layers = vec![];
    for layer in &fit.layers {
        for part in &layer.mesh.parts {
            let region = if hooded && layer.slot == Slot::Head { SkinRegion::Chest } else { SkinRegion::of_worn(layer.slot, part.name) };
            layers.push((layer.slot, part.name, part.positions.clone(), part.indices.clone(), skinner.part(part, region)));
        }
    }
    let skin = body.skin_mesh().parts.iter().map(|p| (p.name, p.positions.clone(), p.indices.clone(), skinner.part(p, SkinRegion::of_skin(p.name)))).collect();
    let c = body.head.centre;
    let region = if hooded { SkinRegion::Chest } else { SkinRegion::Head };
    let weighted = |p: V3| {
        let (joints, weights) = skinner.vertex(p, region);
        (p, PartSkin { joints: vec![joints], weights: vec![weights] })
    };
    let refs = [weighted(c), weighted(vec3::add(c, [1.0, 0.0, 0.0])), weighted(vec3::add(c, [0.0, 1.0, 0.0])), weighted(vec3::add(c, [0.0, 0.0, 1.0]))];
    Fitted { layers, skin, rig: Rig::of(body), centre: c, refs }
}

impl Fitted {
    fn pose(&self, pose: Option<&crate::rig::RigPose>) -> Posed<'_> {
        let frames = pose.map(|p| self.rig.skin_frames(p));
        let place = |points: &[V3], skin: &PartSkin| -> Vec<V3> {
            match &frames {
                Some(f) => points.iter().zip(skin.joints.iter().zip(&skin.weights)).map(|(&p, (&j, &w))| Rig::skin_point(f, p, j, w)).collect(),
                None => points.to_vec(),
            }
        };
        let r: Vec<V3> = self.refs.iter().map(|(p, skin)| place(&[*p], skin)[0]).collect();
        let axis = |k: usize| vec3::normalize_or(vec3::sub(r[k], r[0]), vec3::Y);
        Posed {
            layers: self.layers.iter().map(|(slot, name, points, indices, skin)| (*slot, *name, place(points, skin), &indices[..])).collect(),
            skin: self.skin.iter().map(|(name, points, indices, skin)| (*name, place(points, skin), &indices[..])).collect(),
            head: (r[0], [axis(1), axis(2), axis(3)]),
        }
    }

    /// `p` (as posed) in the head's own frame, in the head's rest coordinates: where it would be
    /// on a head standing still.
    fn in_head(&self, posed: &Posed, p: V3) -> V3 {
        let (c, [ax, ay, az]) = posed.head;
        let d = vec3::sub(p, c);
        vec3::add(self.centre, [vec3::dot(d, ax), vec3::dot(d, ay), vec3::dot(d, az)])
    }
}

/// How far a hood's sheet reaches over the face: the most any vertex of the `slot`'s items (but
/// the parts in `skip`) lies in front of the face, between the brow and the chin. `None` when the
/// face is clear.
///
/// The face is the strip down the middle of the head as wide as between the eyes' outer corners
/// (the sine of the eyes' azimuth, which each folk's face sets, times the head's half width; four
/// fifths of it, so the rim of a correctly narrow opening, which passes by the eyes, is not the
/// face), from the brow's line to a little above the chin's (the edge of a ventail or a hood's
/// rim at the chin is cloth at the chin, not over the mouth). A vertex lies over it when it is in
/// front of the head's centre by more than half the head's depth.
fn face_covered(body: &Body, dressed: &Fitted, posed: &Posed, slot: Slot, skip: &[&str]) -> Option<(f32, V3)> {
    let h = &body.head;
    let c = h.centre;
    let strip = 0.8 * Face::new(*h, body.folk()).eye_azimuth.sin() * h.half_width;
    let (brow_y, chin_y) = (c[1] + h.half_height * (0.43 * PI).cos(), c[1] - h.half_height * (0.8 * PI).cos().abs() + 0.05 * h.half_height);
    let mut worst: Option<(f32, V3)> = None;
    for (s, name, points, _) in &posed.layers {
        if *s != slot || skip.contains(name) {
            continue;
        }
        for p in points {
            let p = dressed.in_head(posed, *p);
            let q = vec3::sub(p, c);
            if q[0].abs() < strip && p[1] < brow_y && p[1] > chin_y && q[2] > 0.5 * h.half_depth {
                let depth = q[2] - 0.5 * h.half_depth;
                if worst.map_or(true, |w| depth > w.0) {
                    worst = Some((depth, p));
                }
            }
        }
    }
    worst
}

/// How far above the jaw the highest of the `slot`'s cloth stands in front of and beside the
/// face (negative: under it), in the head's own frame.
fn collar_above_jaw(body: &Body, dressed: &Fitted, posed: &Posed, slot: Slot) -> f32 {
    let h = &body.head;
    let jaw_y = h.centre[1] - h.half_height * (0.75 * PI).cos().abs();
    let mut top = f32::MIN;
    for (s, _, points, _) in &posed.layers {
        if *s != slot {
            continue;
        }
        for p in points {
            let p = dressed.in_head(posed, *p);
            if (p[0] - h.centre[0]).abs() < 0.9 * h.half_width && p[2] > h.centre[2] - 0.2 * h.half_depth {
                top = top.max(p[1]);
            }
        }
    }
    top - jaw_y
}

/// The closest point to `p` on triangle `abc` (Ericson, Real-Time Collision Detection).
fn closest_on_triangle(p: V3, a: V3, b: V3, c: V3) -> V3 {
    let (ab, ac, ap) = (vec3::sub(b, a), vec3::sub(c, a), vec3::sub(p, a));
    let (d1, d2) = (vec3::dot(ab, ap), vec3::dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = vec3::sub(p, b);
    let (d3, d4) = (vec3::dot(ab, bp), vec3::dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return vec3::add(a, vec3::scale(ab, d1 / (d1 - d3)));
    }
    let cp = vec3::sub(p, c);
    let (d5, d6) = (vec3::dot(ab, cp), vec3::dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return vec3::add(a, vec3::scale(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return vec3::add(b, vec3::scale(vec3::sub(c, b), w));
    }
    let denom = 1.0 / (va + vb + vc);
    vec3::add(a, vec3::add(vec3::scale(ab, vb * denom), vec3::scale(ac, vc * denom)))
}

/// How far a cape stands off what is under it, over the shoulders and arms: the farthest any
/// vertex of the `cape` slots' cloth, from 1.8 below the shoulder line up to a fifth of the wearer's scale above it and outside the collar round the neck (a hood's collar rides the chest with the head while the neck's skin turns with it, so the two part in a stride), lies from
/// the nearest point on a triangle of what is under it (the skin but the head's, and every layer
/// but the cape's and the pack's, which is worn over the cape and cannot hold it off; and
/// anything in `under_skip`: a cloak over a hood's cape is not what that lies on). A disc
/// standing out from the shoulders, a shelf or a bell reads as its distance from the shoulder or
/// arm below it, seen from any side. Returns the distance and the vertex.
fn cape_standoff(body: &Body, posed: &Posed, cape: &[Slot], under_skip: &[Slot]) -> (f32, V3) {
    let s = body.scale;
    let band = (body.shoulder_y - 1.8 * s, body.shoulder_y + 0.2 * s);
    const CELL: f32 = 0.8;
    let cell = |p: &V3| p.map(|v| (v / CELL).floor() as i32);
    let mut tris: Vec<[V3; 3]> = vec![];
    let mut grid: std::collections::HashMap<[i32; 3], Vec<usize>> = std::collections::HashMap::new();
    let parts = posed
        .skin
        .iter()
        .filter(|(name, _, _)| !matches!(*name, "head" | "skull" | "ears" | "eyes" | "face"))
        .map(|(_, pts, idx)| (pts, *idx))
        .chain(posed.layers.iter().filter(|(slot, name, _, _)| !cape.contains(slot) && !under_skip.contains(slot) && *slot != Slot::Back && *name != "collar").map(|(_, _, pts, idx)| (pts, *idx)));
    for (pts, idx) in parts {
        for t in idx.chunks(3) {
            let tri = [t[0], t[1], t[2]].map(|i| pts[i as usize]);
            // Only what is within reach of the cloth that is measured.
            if tri.iter().all(|v| v[1] < band.0 - 1.0) || tri.iter().all(|v| v[1] > band.1 + 1.0) {
                continue;
            }
            let id = tris.len();
            tris.push(tri);
            let lo = cell(&[0, 1, 2].map(|k| tri.iter().map(|v| v[k]).fold(f32::MAX, f32::min)));
            let hi = cell(&[0, 1, 2].map(|k| tri.iter().map(|v| v[k]).fold(f32::MIN, f32::max)));
            for x in lo[0]..=hi[0] {
                for y in lo[1]..=hi[1] {
                    for z in lo[2]..=hi[2] {
                        grid.entry([x, y, z]).or_default().push(id);
                    }
                }
            }
        }
    }
    // The distance to the nearest triangle, giving up once it is no more than `bound` (the
    // farthest found so far: a nearer vertex cannot be the worst).
    let nearest = |q: &V3, bound: f32| {
        let c = cell(q);
        let mut best = f32::MAX;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for &id in grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]).into_iter().flatten() {
                        let [a, b, cc] = tris[id];
                        best = best.min(vec3::distance(closest_on_triangle(*q, a, b, cc), *q));
                        if best <= bound {
                            return best;
                        }
                    }
                }
            }
        }
        best
    };
    let (lo, hi) = band;
    let mut worst = (0.0f32, [0.0; 3]);
    let cloth = posed.layers.iter().filter(|(slot, name, _, _)| cape.contains(slot) && !matches!(*name, "liripipe" | "ties" | "brooch" | "stone" | "trim")).flat_map(|(_, _, pts, _)| pts.iter());
    let (cx, cz, neck) = (body.head.centre[0], body.head.centre[2], 2.0 * body.head.half_width);
    // Not the collar round the neck, which rides the chest with the head while the neck's skin
    // turns with it: within twice the head's half width of its axis.
    for p in cloth.filter(|p| p[1] >= lo && p[1] <= hi && (p[0] - cx).hypot(p[2] - cz) > neck) {
        // Beyond the search (a cell) is far: count it as that far (past any limit).
        let d = nearest(p, worst.0).min(CELL + 0.5);
        if d > worst.0 {
            worst = (d, *p);
        }
    }
    worst
}

/// The numbers for one kit on one body, in one pose.
struct Reading {
    case: String,
    /// How far in front of the face any of the head slot's cloth lies, in the wearer's scale.
    face: f32,
    /// How far a cloak's collar stands above the jaw (negative: under it).
    collar: Option<f32>,
    /// How far the hood's cape, and a cloak's or mantle's, stand off what is under them, in the
    /// wearer's scale.
    standoff: f32,
    cloak_standoff: Option<f32>,
    walking: bool,
}

/// Reads the numbers for the kits on `folk`, `seed`: in the standing stance and, with `walking`,
/// the idle stance and two strides of the walk.
fn read(folk: Folk, seed: u32, kit_seed: u32, walking: bool) -> Vec<Reading> {
    let body = Body::new(&BodySpec::roll(folk, seed));
    let s = body.scale;
    let mut out = vec![];
    for kit in kits(kit_seed) {
        let style = kit.worn.iter().find_map(|(slot, i)| match (slot, i) {
            (Slot::Head, Item::Hood(h)) => Some(h.hood.style),
            _ => None,
        });
        let has_cloak = kit.worn.iter().any(|(slot, _)| *slot == Slot::Cloak);
        let dressed = fitted(&body, &kit.worn);
        for (pose_name, pose) in poses(&body).into_iter().filter(|(n, _)| walking || n == "standing") {
            let posed = dressed.pose(pose.as_ref());
            let case = format!("{folk:?} {seed} kit {kit_seed} {} ({style:?}) {pose_name}", kit.name);
            let face = face_covered(&body, &dressed, &posed, Slot::Head, &[]).map_or(0.0, |w| w.0) / s;
            let collar = has_cloak.then(|| collar_above_jaw(&body, &dressed, &posed, Slot::Cloak) / s);
            let (standoff, at) = cape_standoff(&body, &posed, &[Slot::Head], &[Slot::Cloak]);
            if std::env::var("FB_NUMBERS").is_ok() && standoff / s > 0.9 { eprintln!("WORST {case} at x {:.2} y {:.2} z {:.2} (shoulder y {:.2})", at[0], at[1], at[2], body.shoulder_y); }
            let cloak_standoff = has_cloak.then(|| cape_standoff(&body, &posed, &[Slot::Cloak], &[]).0 / s);
            out.push(Reading { case, face, collar, standoff: standoff / s, cloak_standoff, walking: pose.is_some() });
        }
    }
    out
}

/// What the specs hold the numbers to: nothing in front of the face and a cloak's collar under
/// the jaw (2 cm of the wearer's scale of slack for the cloth's thickness), and a cape within a
/// hand's width (a hand is 0.6 to 1.0 units: 1.0 of the wearer's scale) of what is under it.
const FACE_CLEAR: f32 = 0.0;
const COLLAR_SLACK: f32 = 0.02;
const CAPE_CLOSE: f32 = 1.0;
/// And in the walk, where arms and trunk move under a cape that rides the chest.
const CAPE_CLOSE_WALKING: f32 = 1.2;

fn hoods_keep_off_the_face_and_close_over_the_shoulders_check(folks: &[Folk], seeds: &[u32], kits: &[u32], walking: bool) -> usize {
    let mut failures = vec![];
    let mut read_count = 0;
    for &folk in folks {
        for &seed in seeds {
            for &kit in kits {
                for r in read(folk, seed, kit, walking) {
                    read_count += 1;
                    let limit = if r.walking { CAPE_CLOSE_WALKING } else { CAPE_CLOSE };
                    if r.face > FACE_CLEAR {
                        failures.push(format!("{}: cloth stands {:.2} in front of the face", r.case, r.face));
                    }
                    if let Some(collar) = r.collar.filter(|c| *c > COLLAR_SLACK) {
                        failures.push(format!("{}: the cloak's collar stands {collar:.2} above the jaw", r.case));
                    }
                    if r.standoff > limit {
                        failures.push(format!("{}: the cape stands {:.2} off what is under it", r.case, r.standoff));
                    }
                    if let Some(d) = r.cloak_standoff.filter(|d| *d > limit) {
                        failures.push(format!("{}: the cloak stands {d:.2} off what is under it", r.case));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    read_count
}

#[test]
#[ignore = "prints the numbers the sweep holds, for the folk, seeds and kits in FB_NUMBERS (Human,3,7) or all"]
fn print_hood_numbers() {
    let only = std::env::var("FB_NUMBERS").ok();
    let pick: Option<(Folk, u32, u32)> = only.map(|v| {
        let f: Vec<&str> = v.split(',').collect();
        (Folk::parse(&f[0].to_lowercase()).unwrap(), f[1].parse().unwrap(), f[2].parse().unwrap())
    });
    for folk in Folk::ALL {
        for seed in [1207u32, 40, 3] {
            for kit_seed in [5u32, 7] {
                if pick.is_some_and(|p| p != (folk, seed, kit_seed)) || (pick.is_none() && seed == 3) {
                    continue;
                }
                for r in read(folk, seed, kit_seed, true) {
                    eprintln!("{:80} face {:.2} collar {:?} standoff {:.2} cloak {:?}", r.case, r.face, r.collar.map(|c| (c * 100.0).round() / 100.0), r.standoff, r.cloak_standoff.map(|c| (c * 100.0).round() / 100.0));
                }
            }
        }
    }
}

#[test]
#[ignore = "slow sweep: body fitting test, every folk and seed, standing and walking; run before merging fitting changes"]
fn hoods_keep_off_the_face_and_close_over_the_shoulders() {
    let n = hoods_keep_off_the_face_and_close_over_the_shoulders_check(&Folk::ALL, &[1207, 40], &[7], true);
    assert!(n >= 6 * 2 * 9 * 4, "only {n} readings");
}

#[test]
fn hoods_keep_off_the_face_and_close_over_the_shoulders_smoke() {
    // The kits on a human and a gnome, standing and in a stride of the walk
    // (the sweep has the rest).
    let n = hoods_keep_off_the_face_and_close_over_the_shoulders_check(&[Folk::Human, Folk::Gnome], &[3], &[7], true);
    assert!(n >= 2 * 9 * 4, "only {n} readings");
}
