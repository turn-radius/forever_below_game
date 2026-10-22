//! Specs for what a hood, a coif or a cloak does round the face and over the shoulders: the
//! face stays open (nothing crosses the nose or mouth, a collar stops at the jaw) and the cape
//! lies on the shoulders and arms instead of standing out from them like a disc.

use armory::geom::vec3::{self, V3};
use armory::Item;
use fb_items::Slot;
use std::f32::consts::PI;

use crate::body::Body;
use crate::gait::{Gait, IdleCycle};
use crate::rig::Rig;
use crate::skin::{PartSkin, SkinRegion, Skinner};
use crate::{BodySpec, Folk, fit_worn};

/// A named kit: what the reported screenshots wore.
struct Kit {
    pub name: &'static str,
    pub worn: Vec<(Slot, Item)>,
}

fn rolled(base: &str, seed: u32) -> Item {
    fb_items::roll_item(base, seed).unwrap_or_else(|| panic!("no base {base}")).shape
}

/// The reported kits, rolled from `seed`: a mail coif over a shirt and over a hauberk, a hood
/// over a hauberk with a pack, and a hood with a cloak on the shoulders.
fn kits(seed: u32) -> Vec<Kit> {
    let base = |shirt: &str| vec![(Slot::Shirt, rolled(shirt, seed)), (Slot::Legs, rolled("Trousers", seed))];
    let mut coif_shirt = base("Tunic");
    coif_shirt.push((Slot::Head, rolled("Mail coif", seed)));
    let mut coif_hauberk = base("Shirt");
    coif_hauberk.push((Slot::Body, rolled("Mail", seed)));
    coif_hauberk.push((Slot::Head, rolled("Mail coif", seed)));
    let mut hood_hauberk = base("Tunic");
    hood_hauberk.push((Slot::Body, rolled("Mail", seed)));
    hood_hauberk.push((Slot::Head, rolled("Hood", seed)));
    hood_hauberk.push((Slot::Back, rolled("Pack", seed)));
    let mut hood_cloak = base("Shirt");
    hood_cloak.push((Slot::Head, rolled("Hood", seed)));
    hood_cloak.push((Slot::Cloak, rolled("Cloak", seed)));
    vec![
        Kit { name: "coif over a shirt", worn: coif_shirt },
        Kit { name: "coif over a hauberk", worn: coif_hauberk },
        Kit { name: "hood over a hauberk with a pack", worn: hood_hauberk },
        Kit { name: "hood with a cloak", worn: hood_cloak },
    ]
}

/// The bodies and gear in `pose` (the idle stance or a stride of the walk): every worn part's
/// vertices, as the game's skin weights move them (a hood rides the chest as the head does).
struct Posed {
    layers: Vec<(Slot, &'static str, Vec<V3>)>,
    skin: Vec<(&'static str, Vec<V3>)>,
}

fn poses(body: &Body) -> Vec<(String, Option<crate::rig::RigPose>)> {
    let rig = Rig::of(body);
    let gait = Gait::new(body, &rig);
    let mut out = vec![("standing".to_string(), None)];
    out.push(("idle".to_string(), Some(gait.idle(&IdleCycle::of(1207), 3.0))));
    out.extend([0.0f32, 0.25, 0.5, 0.75].map(|p| (format!("walk {p}"), Some(gait.walk(p, gait.natural_speed())))));
    out
}

/// A kit fitted and weighted once, to be posed many times.
struct Fitted {
    layers: Vec<(Slot, &'static str, Vec<V3>, PartSkin)>,
    skin: Vec<(&'static str, Vec<V3>, PartSkin)>,
    rig: Rig,
}

fn fitted(body: &Body, worn: &[(Slot, Item)]) -> Fitted {
    let hooded = worn.iter().any(|(s, i)| *s == Slot::Head && matches!(i, Item::Hood(_)));
    let fit = fit_worn(body, worn);
    let skinner = Skinner::new(body);
    let mut layers = vec![];
    for layer in &fit.layers {
        for part in &layer.mesh.parts {
            let region = if hooded && layer.slot == Slot::Head { SkinRegion::Chest } else { SkinRegion::of_worn(layer.slot, part.name) };
            layers.push((layer.slot, part.name, part.positions.clone(), skinner.part(part, region)));
        }
    }
    let skin = body.skin_mesh().parts.iter().map(|p| (p.name, p.positions.clone(), skinner.part(p, SkinRegion::of_skin(p.name)))).collect();
    Fitted { layers, skin, rig: Rig::of(body) }
}

impl Fitted {
    fn pose(&self, pose: Option<&crate::rig::RigPose>) -> Posed {
        let frames = pose.map(|p| self.rig.skin_frames(p));
        let place = |points: &Vec<V3>, skin: &PartSkin| match &frames {
            Some(f) => points.iter().zip(skin.joints.iter().zip(&skin.weights)).map(|(&p, (&j, &w))| Rig::skin_point(f, p, j, w)).collect(),
            None => points.clone(),
        };
        Posed {
            layers: self.layers.iter().map(|(slot, name, points, skin)| (*slot, *name, place(points, skin))).collect(),
            skin: self.skin.iter().map(|(name, points, skin)| (*name, place(points, skin))).collect(),
        }
    }
}

/// How far a hood's sheet reaches over the face: the most any vertex of the head or
/// shoulders slot's item lies in front of the face, between brow and chin, within the middle
/// of its width. `None` when the face is clear.
///
/// The face is the strip down the middle of the head, `0.38` of its half width either side,
/// from the brow's line to the chin's, and a vertex lies over it when it is in front of the
/// head's centre by more than half the head's depth.
fn face_covered(body: &Body, posed: &Posed, slot: Slot, skip: &[&str]) -> Option<(f32, V3)> {
    let h = &body.head;
    // The head's centre as the pose has it: the cloth rides the chest with the head.
    let c = h.centre;
    let (brow_y, chin_y) = (c[1] + h.half_height * (0.43 * PI).cos(), c[1] - h.half_height * (0.8 * PI).cos().abs());
    let mut worst: Option<(f32, V3)> = None;
    for (s, name, points) in &posed.layers {
        if *s != slot || skip.contains(name) {
            continue;
        }
        for p in points {
            let q = vec3::sub(*p, c);
            // Over the head's depth in front of its centre, the face is where its surface is:
            // anything in the strip beyond a third of the depth is over it.
            if q[0].abs() < 0.38 * h.half_width && p[1] < brow_y && p[1] > chin_y && q[2] > 0.5 * h.half_depth {
                let depth = q[2] - 0.5 * h.half_depth;
                if worst.map_or(true, |w| depth > w.0) {
                    worst = Some((depth, *p));
                }
            }
        }
    }
    worst
}

/// The highest a collar (of the items in `slot`) stands in front of and beside the face, over
/// the jaw's height: how far above the jaw it reaches, in the head's own width.
fn collar_above_jaw(body: &Body, posed: &Posed, slot: Slot) -> f32 {
    let h = &body.head;
    let jaw_y = h.centre[1] - h.half_height * (0.75 * PI).cos().abs();
    let mut top = f32::MIN;
    for (s, _, points) in &posed.layers {
        if *s != slot {
            continue;
        }
        for p in points {
            if p[0].abs() < 0.9 * h.half_width && p[2] > h.centre[2] - 0.2 * h.half_depth {
                top = top.max(p[1]);
            }
        }
    }
    top - jaw_y
}

/// How far a cape stands off what is under it, over the shoulders and arms: the farthest any
/// vertex of the `cape` slots' cloth, from 1.8 below the shoulder line up to the chin, lies
/// from the nearest vertex of what is under it (the skin but the head's, and every layer but
/// the cape's, the pack and anything in `under_skip`: a cloak over a hood's cape is not what
/// it lies on). A disc standing out from the shoulders, a shelf or a bell reads as its
/// distance from the shoulder or arm below it, seen from any side. Returns the distance and
/// the vertex.
fn cape_standoff(body: &Body, posed: &Posed, cape: &[Slot], under_skip: &[Slot]) -> (f32, V3) {
    let s = body.scale;
    const CELL: f32 = 0.4;
    let cell = |p: &V3| p.map(|v| (v / CELL).floor() as i32);
    let mut grid: std::collections::HashMap<[i32; 3], Vec<V3>> = std::collections::HashMap::new();
    let under = posed
        .skin
        .iter()
        .filter(|(name, _)| !matches!(*name, "head" | "skull" | "ears" | "eyes" | "face"))
        .flat_map(|(_, pts)| pts.iter())
        .chain(posed.layers.iter().filter(|(slot, name, _)| !cape.contains(slot) && !under_skip.contains(slot) && *name != "collar").flat_map(|(_, _, pts)| pts.iter()));
    for p in under {
        grid.entry(cell(p)).or_default().push(*p);
    }
    let nearest = |q: &V3| {
        let c = cell(q);
        let mut best = f32::MAX;
        for dx in -2..=2 {
            for dy in -2..=2 {
                for dz in -2..=2 {
                    for p in grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]).into_iter().flatten() {
                        best = best.min(vec3::distance(*p, *q));
                    }
                }
            }
        }
        best
    };
    let (lo, hi) = (body.shoulder_y - 1.8 * s, body.head.centre[1] - body.head.half_height);
    let mut worst = (0.0f32, [0.0; 3]);
    let cloth = posed.layers.iter().filter(|(slot, name, _)| cape.contains(slot) && !matches!(*name, "liripipe" | "ties")).flat_map(|(_, _, pts)| pts.iter());
    for p in cloth.filter(|p| p[1] >= lo && p[1] <= hi).step_by(5) {
        // Beyond the search (2 cells) is far: count it as that far.
        let d = nearest(p).min(2.0 * CELL + 1.0);
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
    /// How far the cape stands off what is under it, in the wearer's scale.
    standoff: f32,
    /// What it is held to: the swing of the arms and the twist of the trunk in a stride carry
    /// what is under the cape (skinned to the spine and arms) off it, the cape riding the chest.
    limit: f32,
}

/// Reads the numbers for the reported kits on `folk`, `seed`: the face in the standing pose
/// (a hood rides the chest with the head, so it keeps its place on the face in the walk), the
/// cape's stand-off in the standing pose and, with `walking`, the idle stance and four strides
/// of the walk.
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
        let rest = dressed.pose(None);
        let face = face_covered(&body, &rest, Slot::Head, &[]).map_or(0.0, |w| w.0) / s;
        let collar = has_cloak.then(|| collar_above_jaw(&body, &rest, Slot::Cloak) / s);
        for (pose_name, pose) in poses(&body).into_iter().filter(|(n, _)| walking || n == "standing") {
            let moved = pose.as_ref().map(|p| dressed.pose(Some(p)));
            let posed = moved.as_ref().unwrap_or(&rest);
            let case = format!("{folk:?} {seed} kit {kit_seed} {} ({style:?}) {pose_name}", kit.name);
            let (standoff, _) = cape_standoff(&body, posed, &[Slot::Head], &[Slot::Cloak, Slot::Back]);
            out.push(Reading { case, face, collar, standoff: standoff / s, limit: if pose.is_some() { CAPE_CLOSE_WALKING } else { CAPE_CLOSE } });
        }
    }
    out
}

/// What the specs hold the numbers to: nothing in front of the face and a cloak's collar under
/// the jaw (2 cm of the wearer's scale of slack for the cloth's thickness), and a cape within
/// a hand's width (1.0 of the wearer's scale: a hand is 0.6 to 1.0 units) of what is under it.
const FACE_CLEAR: f32 = 0.0;
const COLLAR_SLACK: f32 = 0.02;
const CAPE_CLOSE: f32 = 1.0;
/// And in the walk, where arms and trunk move under a cape that rides the chest.
const CAPE_CLOSE_WALKING: f32 = 1.5;

fn hoods_keep_off_the_face_and_close_over_the_shoulders_check(folks: &[Folk], seeds: &[u32], kits: &[u32], walking: bool) -> usize {
    let mut failures = vec![];
    let mut read_count = 0;
    for &folk in folks {
        for &seed in seeds {
            for &kit in kits {
                for r in read(folk, seed, kit, walking) {
                    read_count += 1;
                    if r.face > FACE_CLEAR {
                        failures.push(format!("{}: cloth stands {:.2} in front of the face", r.case, r.face));
                    }
                    if let Some(collar) = r.collar.filter(|c| *c > COLLAR_SLACK) {
                        failures.push(format!("{}: the cloak's collar stands {collar:.2} above the jaw", r.case));
                    }
                    if r.standoff > r.limit {
                        failures.push(format!("{}: the cape stands {:.2} off what is under it", r.case, r.standoff));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    read_count
}

#[test]
#[ignore = "slow sweep: body fitting test, every folk and seed, standing and walking; run before merging fitting changes"]
fn hoods_keep_off_the_face_and_close_over_the_shoulders() {
    let n = hoods_keep_off_the_face_and_close_over_the_shoulders_check(&Folk::ALL, &[1207, 40], &[5, 7], true);
    assert!(n >= 6 * 2 * 2 * 4 * 6, "only {n} readings");
}

#[test]
fn hoods_keep_off_the_face_and_close_over_the_shoulders_smoke() {
    // The reported kits on a human, a dwarf, a gnome and a pigfolk, standing and in a stride of the walk
    // (the sweep has the rest).
    let n = hoods_keep_off_the_face_and_close_over_the_shoulders_check(&[Folk::Human, Folk::Dwarf, Folk::Gnome, Folk::Pigfolk], &[3], &[7], true);
    assert!(n >= 4 * 4 * 6, "only {n} readings");
}
