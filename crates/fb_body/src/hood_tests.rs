//! Specs for what a hood, a coif or a cloak does round the face and over the shoulders: the
//! face stays open (nothing crosses the nose or mouth, a collar stops at the jaw) and the cape
//! lies on the shoulders and arms instead of standing out from them like a disc.

use armory::families::garment::Garment;
use armory::families::helm::{Helm, HelmStyle};
use armory::families::hood::Hoods;
use armory::geom::vec3::{self, V3};
use armory::sections::garment::GarmentStyle;
use armory::sections::hood::HoodStyle;
use armory::Item;
use fb_items::Slot;
use std::collections::HashMap;
use std::f32::consts::PI;

use crate::body::Body;
use crate::folk::{Beard, HairStyle};
use crate::gait::{Gait, IdleCycle};
use crate::head::Face;
use crate::rig::Rig;
use crate::skin::{PartSkin, SkinRegion, Skinner};
use crate::{BodySpec, Folk, fit_worn};

/// A named kit: what the reported screenshots wore, and the extremes round them.
struct Kit {
    name: &'static str,
    worn: Vec<(Slot, Item)>,
    /// Worn over long hair and a long beard.
    hairy: bool,
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

/// A hauberk with the biggest pauldrons it has (five lames: shoulders standing well up).
fn high_pauldrons(seed: u32) -> Item {
    let mut mail = Garment::roll_as(seed + 1, GarmentStyle::Hauberk);
    mail.plates.pauldrons = true;
    mail.plates.lames = 5;
    Item::Garment(mail)
}

/// The kits, rolled from `seed`: the reported ones (a mail coif over a shirt and over a hauberk, a
/// hood over a hauberk with a pack, a hood with a cloak on the shoulders), a mail coif forced to
/// have its ventail, hoods and cloth coifs with the face opening at both extremes, a cowl, a
/// mantle (a cloak with length 0), a helm with an aventail under a cloak, a cloak over very high
/// pauldrons, and hoods over long hair and a full beard.
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
    let mut helm = Helm::roll_as(seed, HelmStyle::Bascinet);
    helm.aventail.on = true;
    helm.aventail.length = 2.5;
    helm.aventail.flare = 1.0;
    let kit = |name, worn| Kit { name, worn, hairy: false };
    vec![
        kit("coif over a shirt", with(base("Tunic"), vec![(Slot::Head, rolled("Mail coif", seed))])),
        kit("coif over a hauberk", with(base("Shirt"), vec![hauberk(), (Slot::Head, rolled("Mail coif", seed))])),
        kit("hood over a hauberk with a pack", with(base("Tunic"), vec![hauberk(), (Slot::Head, rolled("Hood", seed)), (Slot::Back, rolled("Pack", seed))])),
        kit("hood with a cloak", with(base("Shirt"), vec![(Slot::Head, rolled("Hood", seed)), (Slot::Cloak, rolled("Cloak", seed))])),
        kit("coif with a ventail", with(base("Shirt"), vec![hauberk(), (Slot::Head, hood_of(seed, HoodStyle::MailCoif, None, Some(true)))])),
        kit("hood with the narrowest opening", with(base("Tunic"), vec![hauberk(), (Slot::Head, hood_of(seed, HoodStyle::Hood, Some(0.0), None))])),
        kit("hood with the widest opening", with(base("Tunic"), vec![hauberk(), (Slot::Head, hood_of(seed, HoodStyle::Hood, Some(1.0), None))])),
        kit("cowl with the narrowest opening", with(base("Shirt"), vec![(Slot::Head, hood_of(seed, HoodStyle::Cowl, Some(0.0), None))])),
        kit("hood with a mantle", with(base("Shirt"), vec![(Slot::Head, rolled("Hood", seed)), (Slot::Cloak, mantle)])),
        kit("cloth coif with the narrowest opening", with(base("Shirt"), vec![(Slot::Head, hood_of(seed, HoodStyle::Coif, Some(0.0), None))])),
        kit("cloth coif with the widest opening", with(base("Tunic"), vec![hauberk(), (Slot::Head, hood_of(seed, HoodStyle::Coif, Some(1.0), None))])),
        kit("helm with an aventail and a cloak", with(base("Shirt"), vec![(Slot::Head, Item::Helm(helm)), (Slot::Cloak, rolled("Cloak", seed))])),
        kit("cloak over very high pauldrons", with(base("Shirt"), vec![(Slot::Body, high_pauldrons(seed)), (Slot::Cloak, rolled("Cloak", seed))])),
        Kit { name: "hood over long hair and a full beard", worn: with(base("Tunic"), vec![hauberk(), (Slot::Head, hood_of(seed, HoodStyle::Hood, None, None))]), hairy: true },
        Kit { name: "coif over long hair and a full beard", worn: with(base("Shirt"), vec![(Slot::Head, hood_of(seed, HoodStyle::MailCoif, None, None))]), hairy: true },
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
    /// The hair and beard's points and triangles, at rest.
    hair: &'a Vec<V3>,
    hair_tris: &'a [u32],
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
    hair: Vec<V3>,
    hair_tris: Vec<u32>,
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
    let (mut hair, mut hair_tris) = (vec![], vec![]);
    for part in body.hair_mesh(fit.hair).parts {
        let at = hair.len() as u32;
        hair.extend(part.positions.iter().copied());
        hair_tris.extend(part.indices.iter().map(|i| i + at));
    }
    Fitted { layers, skin, rig: Rig::of(body), centre: c, refs, hair, hair_tris }
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
            hair: &self.hair,
            hair_tris: &self.hair_tris,
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
/// the parts in `skip`) lies in front of the face, between the brow and the chin, and how many
/// vertices of the slot lay in front of the head between those lines (what was looked at: a
/// check that looks at nothing passes). `None` when the face is clear.
///
/// The face is the strip down the middle of the head as wide as between the eyes' outer corners
/// (the sine of the eyes' azimuth, which each folk's face sets, times the head's half width; four
/// fifths of it, so the rim of a correctly narrow opening, which passes by the eyes, is not the
/// face), from the brow's line to a little above the chin's (the edge of a ventail or a hood's
/// rim at the chin is cloth at the chin, not over the mouth). A vertex lies over it when it is in
/// front of the head's centre by more than half the head's depth.
fn face_covered(body: &Body, dressed: &Fitted, posed: &Posed, slot: Slot, skip: &[&str]) -> (Option<(f32, V3)>, usize) {
    let h = &body.head;
    let c = h.centre;
    let strip = 0.8 * Face::new(*h, body.folk()).eye_azimuth.sin() * h.half_width;
    let (brow_y, chin_y) = (c[1] + h.half_height * (0.43 * PI).cos(), c[1] - h.half_height * (0.8 * PI).cos().abs() + 0.05 * h.half_height);
    let mut worst: Option<(f32, V3)> = None;
    let mut looked = 0;
    for (s, name, points, _) in &posed.layers {
        if *s != slot || skip.contains(name) {
            continue;
        }
        for p in points {
            let p = dressed.in_head(posed, *p);
            let q = vec3::sub(p, c);
            if p[1] < brow_y && p[1] > chin_y && q[2] > 0.0 && q[0].abs() < 2.0 * h.half_width {
                looked += 1;
            }
            if q[0].abs() < strip && p[1] < brow_y && p[1] > chin_y && q[2] > 0.5 * h.half_depth {
                let depth = q[2] - 0.5 * h.half_depth;
                if worst.map_or(true, |w| depth > w.0) {
                    worst = Some((depth, p));
                }
            }
        }
    }
    (worst, looked)
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

/// Triangles in a hash grid, for the distance from a point to the nearest of them.
struct Tris {
    tris: Vec<[V3; 3]>,
    grid: HashMap<[i32; 3], Vec<usize>>,
    /// The size of a cell: a point has the whole of its neighbours' cells searched, so a
    /// triangle farther than this may go unseen, and the distance is never reported as more.
    cell: f32,
}

impl Tris {
    /// The triangles of `parts` that reach into heights `band` (a cell either side of it).
    fn new<'a>(parts: impl Iterator<Item = (&'a Vec<V3>, &'a [u32])>, band: (f32, f32), cell: f32) -> Tris {
        let at = |p: &V3| p.map(|v| (v / cell).floor() as i32);
        let mut out = Tris { tris: vec![], grid: HashMap::new(), cell };
        for (pts, idx) in parts {
            for t in idx.chunks(3) {
                let tri = [t[0], t[1], t[2]].map(|i| pts[i as usize]);
                if tri.iter().all(|v| v[1] < band.0 - cell) || tri.iter().all(|v| v[1] > band.1 + cell) {
                    continue;
                }
                let id = out.tris.len();
                out.tris.push(tri);
                let lo = at(&[0, 1, 2].map(|k| tri.iter().map(|v| v[k]).fold(f32::MAX, f32::min)));
                let hi = at(&[0, 1, 2].map(|k| tri.iter().map(|v| v[k]).fold(f32::MIN, f32::max)));
                for x in lo[0]..=hi[0] {
                    for y in lo[1]..=hi[1] {
                        for z in lo[2]..=hi[2] {
                            out.grid.entry([x, y, z]).or_default().push(id);
                        }
                    }
                }
            }
        }
        out
    }

    /// The distance from `q` to the nearest triangle, no more than the cell; giving up once it
    /// is no more than `bound` (the farthest found so far: a nearer point cannot be the worst).
    fn nearest(&self, q: &V3, bound: f32) -> f32 {
        let c = q.map(|v| (v / self.cell).floor() as i32);
        let mut best = self.cell;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for &id in self.grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]).into_iter().flatten() {
                        let [a, b, cc] = self.tris[id];
                        best = best.min(vec3::distance(closest_on_triangle(*q, a, b, cc), *q));
                        if best <= bound {
                            return best;
                        }
                    }
                }
            }
        }
        best
    }
}

/// The cell of [`Tris`] for a body: past the walking limit of the wearer's scale and a margin,
/// so that a cape held as far off as the limit allows can be told from one that is not on any
/// body (a fixed cell stopped the distance at 1.3 units, under the limit of a tall wearer).
fn cell_for(body: &Body) -> f32 {
    (1.6 * body.scale).max(0.8)
}

/// What is under a cape: the skin but the head's, and every layer but the cape's and the pack's
/// (which is worn over the cape and cannot hold it off), and anything in `under_skip` (a cloak
/// over a hood's cape is not what that lies on).
fn under<'a>(posed: &'a Posed, cape: &[Slot], under_skip: &[Slot]) -> impl Iterator<Item = (&'a Vec<V3>, &'a [u32])> {
    let (cape, skip) = (cape.to_vec(), under_skip.to_vec());
    posed
        .skin
        .iter()
        .filter(|(name, _, _)| !matches!(*name, "head" | "skull" | "ears" | "eyes" | "face"))
        .map(|(_, pts, idx)| (pts, *idx))
        .chain(posed.layers.iter().filter(move |(slot, name, _, _)| !cape.contains(slot) && !skip.contains(slot) && *slot != Slot::Back && *name != "collar").map(|(_, _, pts, idx)| (pts, *idx)))
        // The hair and beard a hood's cape lies on.
        .chain(std::iter::once((posed.hair, posed.hair_tris)))
}

/// The heights a cape is measured over: from 1.8 of the wearer's scale below the shoulder line up
/// to the line (above it is the collar round the neck, which rides the chest with the head while
/// the neck's skin turns with it, so the two part in a stride).
fn cape_band(body: &Body) -> (f32, f32) {
    (body.shoulder_y - 1.8 * body.scale, body.shoulder_y)
}

/// How far a cape stands off what is under it, over the shoulders and arms: the farthest any
/// vertex of the `cape` slots' cloth, within [`cape_band`], lies from the nearest point on a triangle of what is under it ([`under`]). A disc standing out from the shoulders, a shelf
/// or a bell reads as its distance from the shoulder or arm below it, seen from any side. Returns
/// the distance, the vertex and how many vertices were measured.
fn cape_standoff(body: &Body, posed: &Posed, cape: &[Slot], under_skip: &[Slot]) -> (f32, V3, usize) {
    let band = cape_band(body);
    let tris = Tris::new(under(posed, cape, under_skip), band, cell_for(body));
    let mut worst = (0.0f32, [0.0; 3]);
    let mut measured = 0;
    let cloth = posed.layers.iter().filter(|(slot, name, _, _)| cape.contains(slot) && !matches!(*name, "liripipe" | "ties" | "brooch" | "stone" | "trim")).flat_map(|(_, _, pts, _)| pts.iter());
    // A cloak is open down the front, its edges rolled back and forward there: its stand-off is
    // the cloth over the shoulders and arms, the sides and back (not within 35 degrees of
    // straight ahead).
    let open_front = cape.contains(&Slot::Cloak);
    for p in cloth.filter(|p| p[1] >= band.0 && p[1] <= band.1 && !(open_front && p[0].atan2(p[2]).abs() < 0.6)) {
        measured += 1;
        let d = tris.nearest(p, worst.0);
        if d > worst.0 {
            worst = (d, *p);
        }
    }
    (worst.0, worst.1, measured)
}

/// How far the `inner` points (hair, a beard, an aventail) reach out past the `outer` cloth over
/// them, in the wearer's scale (but not at the front, open on a cloak): for each height (a quarter of a unit) and direction (twelve
/// degrees about the axis) where the cloth is, the most the points there stand beyond its
/// farthest vertex. Clipping reads as what is under the cloth showing through it. Returns the
/// most, and how many points were over cloth.
fn protrusion(body: &Body, inner: &[V3], outer: &[&V3]) -> (f32, usize) {
    let bin = |p: &V3| ((p[1] / 0.25).floor() as i32, ((p[0].atan2(p[2]) + PI) / (PI / 15.0)).floor() as i32);
    let r = |p: &V3| p[0].hypot(p[2]);
    let mut reach: HashMap<(i32, i32), f32> = HashMap::new();
    for p in outer {
        let e = reach.entry(bin(p)).or_insert(0.0);
        *e = e.max(r(p));
    }
    let (lo, hi) = cape_band(body);
    let (mut worst, mut counted) = (0.0f32, 0);
    // Not the front: a cloak is open down it and a hood's neck is a slit, and a beard or an
    // aventail there is seen, not clipped (the opening is 1.07 round at the cloak's hem).
    for p in inner.iter().filter(|p| p[1] >= lo && p[1] <= hi && p[0].atan2(p[2]).abs() > 1.2) {
        if let Some(&far) = reach.get(&bin(p)) {
            counted += 1;
            worst = worst.max(r(p) - far);
        }
    }
    (worst / body.scale, counted)
}

/// How far the `points` hang from everything under and round them, over the shoulders and
/// chest: the farthest any is from the nearest triangle of the skin and the worn layers.
fn floating(body: &Body, posed: &Posed, points: &[V3]) -> f32 {
    let band = cape_band(body);
    let all = posed.skin.iter().map(|(_, pts, idx)| (pts, *idx)).chain(posed.layers.iter().map(|(_, _, pts, idx)| (pts, *idx)));
    let tris = Tris::new(all, band, cell_for(body));
    let mut worst = 0.0f32;
    for p in points.iter().filter(|p| p[1] >= band.0 && p[1] <= band.1) {
        worst = worst.max(tris.nearest(p, worst));
    }
    worst / body.scale
}

/// The numbers for one kit on one body, in one pose. `None` for what the kit does not have.
struct Reading {
    case: String,
    /// How far in front of the face any of the head slot's cloth lies, in the wearer's scale, and
    /// how many of its vertices were looked at.
    face: Option<(f32, usize)>,
    /// How far a cloak's collar stands above the jaw (negative: under it).
    collar: Option<f32>,
    /// How far the hood's cape, and a cloak's or mantle's, stand off what is under them, in the
    /// wearer's scale, and how many vertices of each were measured.
    standoff: Option<(f32, usize)>,
    cloak_standoff: Option<(f32, usize)>,
    /// How far an aventail stands out past the cloak over it, or the hair and beard past a
    /// hood's cape, and how many points were over cloth; and how far hair hangs from what is under
    /// it.
    clip: Option<(f32, usize)>,
    floating: Option<f32>,
    walking: bool,
}

/// Reads the numbers for the kits on `folk`, `seed`: in the standing stance and, with `walking`,
/// the idle stance and two strides of the walk.
fn read(folk: Folk, seed: u32, kit_seed: u32, walking: bool) -> Vec<Reading> {
    let mut out = vec![];
    for kit in kits(kit_seed) {
        let mut spec = BodySpec::roll(folk, seed);
        if kit.hairy {
            spec.hair = HairStyle::Long;
            spec = spec.with_beard(Beard::Long);
        }
        let body = Body::new(&spec);
        let s = body.scale;
        let style = kit.worn.iter().find_map(|(slot, i)| match (slot, i) {
            (Slot::Head, Item::Hood(h)) => Some(h.hood.style),
            _ => None,
        });
        let has_cloak = kit.worn.iter().any(|(slot, _)| *slot == Slot::Cloak);
        let has_helm = kit.worn.iter().any(|(slot, i)| *slot == Slot::Head && matches!(i, Item::Helm(_)));
        let dressed = fitted(&body, &kit.worn);
        for (pose_name, pose) in poses(&body).into_iter().filter(|(n, _)| walking || n == "standing") {
            let posed = dressed.pose(pose.as_ref());
            let case = format!("{folk:?} {seed} kit {kit_seed} {} ({style:?}) {pose_name}", kit.name);
            let face = style.map(|_| {
                let (worst, looked) = face_covered(&body, &dressed, &posed, Slot::Head, &[]);
                (worst.map_or(0.0, |w| w.0) / s, looked)
            });
            let collar = has_cloak.then(|| collar_above_jaw(&body, &dressed, &posed, Slot::Cloak) / s);
            let standoff = style.filter(|st| st.has_cape()).map(|_| {
                let (d, _, n) = cape_standoff(&body, &posed, &[Slot::Head], &[Slot::Cloak]);
                (d / s, n)
            });
            let cloak_standoff = has_cloak.then(|| {
                let (d, _, n) = cape_standoff(&body, &posed, &[Slot::Cloak], &[]);
                (d / s, n)
            });
            // Only at rest: the hair and beard are not skinned here.
            let mut clip = None;
            let mut float = None;
            if pose.is_none() && has_helm {
                let aventail: Vec<V3> = posed.layers.iter().filter(|l| l.1 == "aventail").flat_map(|l| l.2.iter().copied()).collect();
                let cloak: Vec<&V3> = posed.layers.iter().filter(|l| l.0 == Slot::Cloak).flat_map(|l| l.2.iter()).collect();
                clip = Some(protrusion(&body, &aventail, &cloak));
            }
            if pose.is_none() && kit.hairy {
                let cape: Vec<&V3> = posed.layers.iter().filter(|l| l.0 == Slot::Head).flat_map(|l| l.2.iter()).collect();
                clip = Some(protrusion(&body, posed.hair, &cape));
                float = Some(floating(&body, &posed, posed.hair));
            }
            out.push(Reading { case, face, collar, standoff, cloak_standoff, clip, floating: float, walking: pose.is_some() });
        }
    }
    out
}

/// What the specs hold the numbers to: nothing in front of the face and a cloak's collar under
/// the jaw (5% of the wearer's scale of slack: the cloth's thickness above the collar's ring), and a cape within a
/// hand's width (a hand is 0.6 to 1.0 units: 1.0 of the wearer's scale) of what is under it.
const FACE_CLEAR: f32 = 0.0;
const COLLAR_SLACK: f32 = 0.05;
const CAPE_CLOSE: f32 = 1.0;
/// And in the walk, where arms and trunk move under a cape that rides the chest.
const CAPE_CLOSE_WALKING: f32 = 1.2;
/// A cloak hangs free over what is on the shoulders (a gnome's thick pauldrons and an aventail
/// under it, its front corners at the shoulder line), and is held to a hand's width more.
const CLOAK_EXTRA: f32 = 0.3;
/// What may show through the cloth over it: hair, a beard or an aventail beyond a cloth's
/// thickness (a tenth of the wearer's scale, a centimetre on a human; a gnome's hair under a mail
/// coif shows 0.08, the hair being laid on what is worn below the hood and the cape hung over it
/// and what is under it, not over the hair).
const CLIP: f32 = 0.1;
/// How far hair may hang from what is under it (the wearer's scale).
const FLOAT: f32 = 1.0;
/// The fewest vertices a check must have looked at to count: of the head slot in front of the
/// head, of a cape in its band, and of hair or an aventail under cloth. A check that looks at
/// nothing passes.
const MIN_FACE_LOOKED: usize = 50;
const MIN_CAPE_MEASURED: usize = 100;
const MIN_UNDER_CLOTH: usize = 20;

fn hoods_keep_off_the_face_and_close_over_the_shoulders_check(folks: &[Folk], seeds: &[u32], kits: &[u32], walking: bool) -> usize {
    let mut failures = vec![];
    let mut read_count = 0;
    for &folk in folks {
        for &seed in seeds {
            for &kit in kits {
                for r in read(folk, seed, kit, walking) {
                    read_count += 1;
                    let limit = if r.walking { CAPE_CLOSE_WALKING } else { CAPE_CLOSE };
                    if let Some((face, looked)) = r.face {
                        if face > FACE_CLEAR {
                            failures.push(format!("{}: cloth stands {face:.2} in front of the face", r.case));
                        }
                        if looked < MIN_FACE_LOOKED {
                            failures.push(format!("{}: only {looked} vertices of the head slot were looked at", r.case));
                        }
                    }
                    if let Some(collar) = r.collar.filter(|c| *c > COLLAR_SLACK) {
                        failures.push(format!("{}: the cloak's collar stands {collar:.2} above the jaw", r.case));
                    }
                    for (what, measured, extra) in [("cape", r.standoff, 0.0), ("cloak", r.cloak_standoff, CLOAK_EXTRA)] {
                        if let Some((d, n)) = measured {
                            if d > limit + extra {
                                failures.push(format!("{}: the {what} stands {d:.2} off what is under it", r.case));
                            }
                            if n < MIN_CAPE_MEASURED {
                                failures.push(format!("{}: only {n} vertices of the {what} were measured", r.case));
                            }
                        }
                    }
                    if let Some((c, n)) = r.clip {
                        if c > CLIP {
                            failures.push(format!("{}: {c:.2} shows through the cloth over it", r.case));
                        }
                        if n < MIN_UNDER_CLOTH {
                            failures.push(format!("{}: only {n} points were found under cloth", r.case));
                        }
                    }
                    if let Some(f) = r.floating.filter(|f| *f > FLOAT) {
                        failures.push(format!("{}: hair hangs {f:.2} from what is under it", r.case));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    read_count
}

#[test]
#[ignore = "prints the numbers the sweep holds, for the folk, seed and kit seed in FB_NUMBERS (Human,3,7) or all"]
fn print_hood_numbers() {
    let only = std::env::var("FB_NUMBERS").ok();
    let pick: Option<(Folk, u32, u32)> = only.map(|v| {
        let f: Vec<&str> = v.split(',').collect();
        (Folk::parse(&f[0].to_lowercase()).unwrap(), f[1].parse().unwrap(), f[2].parse().unwrap())
    });
    let r2 = |x: f32| (x * 100.0).round() / 100.0;
    for folk in Folk::fitted() {
        for seed in [1207u32, 40, 3] {
            for kit_seed in [5u32, 7] {
                if pick.is_some_and(|p| p != (folk, seed, kit_seed)) || (pick.is_none() && (seed == 3 || kit_seed == 5)) {
                    continue;
                }
                for r in read(folk, seed, kit_seed, true) {
                    eprintln!(
                        "{:84} face {:?} collar {:?} cape {:?} cloak {:?} clip {:?} floating {:?}",
                        r.case,
                        r.face.map(|f| (r2(f.0), f.1)),
                        r.collar.map(r2),
                        r.standoff.map(|f| (r2(f.0), f.1)),
                        r.cloak_standoff.map(|f| (r2(f.0), f.1)),
                        r.clip.map(|f| (r2(f.0), f.1)),
                        r.floating.map(r2)
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "slow sweep: body fitting test, every folk and seed, standing and walking; run before merging fitting changes"]
fn hoods_keep_off_the_face_and_close_over_the_shoulders() {
    let n = hoods_keep_off_the_face_and_close_over_the_shoulders_check(&Folk::fitted(), &[1207, 40], &[7], true);
    assert!(n >= 6 * 2 * 15 * 4, "only {n} readings");
}

#[test]
fn hoods_keep_off_the_face_and_close_over_the_shoulders_smoke() {
    // The kits on a human and a gnome, standing and in a stride of the walk (the sweep has the
    // rest).
    let n = hoods_keep_off_the_face_and_close_over_the_shoulders_check(&[Folk::Human, Folk::Gnome], &[3], &[7], true);
    assert!(n >= 2 * 15 * 4, "only {n} readings");
}

/// The hood over a hauberk with a pack, for the controls below, on a tall human (a scale past
/// the fixed 1.3 units the distance was once capped at) or on `folk`.
fn control_kit(folk: Folk, stature: f32) -> (Body, Fitted) {
    let mut spec = BodySpec::roll(folk, 3);
    spec.stature = stature;
    let body = Body::new(&spec);
    let kit = kits(7).into_iter().find(|k| k.name == "hood over a hauberk with a pack").unwrap();
    let dressed = fitted(&body, &kit.worn);
    (body, dressed)
}

#[test]
fn a_covered_face_fails_the_face_check() {
    for (folk, stature) in [(Folk::Human, 1.0), (Folk::Lizardfolk, 1.0), (Folk::Pigfolk, 1.0)] {
        let (body, dressed) = control_kit(folk, stature);
        let mut posed = dressed.pose(None);
        let (clear, looked) = face_covered(&body, &dressed, &posed, Slot::Head, &[]);
        assert!(clear.is_none() && looked >= MIN_FACE_LOOKED, "{folk:?}: the hood already covers the face ({clear:?}, {looked} looked at)");
        // A sheet of cloth drawn across the face, over the nose and mouth.
        let c = body.head.centre;
        let across: Vec<V3> = (0..5).flat_map(|i| (0..5).map(move |j| [c[0] + 0.1 * (i as f32 - 2.0), c[1] - 0.1 * body.head.half_height * j as f32, c[2] + 1.2 * body.head.half_depth])).collect();
        posed.layers.push((Slot::Head, "hood", across, &[]));
        let (covered, _) = face_covered(&body, &dressed, &posed, Slot::Head, &[]);
        assert!(covered.is_some_and(|c| c.0 / body.scale > FACE_CLEAR), "{folk:?}: a cloth over the face went unseen");
    }
}

#[test]
fn hair_through_the_cape_fails_the_clip_check() {
    let mut spec = BodySpec::roll(Folk::Human, 3);
    spec.hair = HairStyle::Long;
    let body = Body::new(&spec.with_beard(Beard::Long));
    let kit = kits(7).into_iter().find(|k| k.name == "hood over long hair and a full beard").unwrap();
    let dressed = fitted(&body, &kit.worn);
    let posed = dressed.pose(None);
    let cape: Vec<&V3> = posed.layers.iter().filter(|l| l.0 == Slot::Head).flat_map(|l| l.2.iter()).collect();
    let (rests, n) = protrusion(&body, posed.hair, &cape);
    assert!(rests <= CLIP && n >= MIN_UNDER_CLOTH, "the hair already shows through ({rests}, {n} under cloth)");
    // The same hair drawn out half as far again from the axis stands through the cape.
    let out: Vec<V3> = posed.hair.iter().map(|p| [p[0] * 1.5, p[1], p[2] * 1.5]).collect();
    assert!(protrusion(&body, &out, &cape).0 > CLIP);
}

#[test]
fn a_flared_cape_fails_the_stand_off_check_on_every_body() {
    for (folk, stature) in [(Folk::Human, 1.35), (Folk::Lizardfolk, 1.0), (Folk::Gnome, 1.0)] {
        let (body, dressed) = control_kit(folk, stature);
        let s = body.scale;
        let mut posed = dressed.pose(None);
        let (close, _, n) = cape_standoff(&body, &posed, &[Slot::Head], &[Slot::Cloak]);
        assert!(close / s <= CAPE_CLOSE && n >= MIN_CAPE_MEASURED, "{folk:?}: the cape is already off ({}, {n} measured)", close / s);
        // The cape held out from the shoulders like a disc, a good way past the limit: its
        // vertices pushed out from the axis, more the lower they hang.
        let band = cape_band(&body);
        for (slot, _, points, _) in posed.layers.iter_mut().filter(|l| l.0 == Slot::Head) {
            assert_eq!(*slot, Slot::Head);
            for p in points.iter_mut().filter(|p| p[1] < band.1) {
                let k = 1.0 + 2.2 * s * (band.1 - p[1]) / (band.1 - band.0 + 1.0) / 1.5;
                *p = [p[0] * k, p[1], p[2] * k];
            }
        }
        let (flared, _, _) = cape_standoff(&body, &posed, &[Slot::Head], &[Slot::Cloak]);
        assert!(flared / s > CAPE_CLOSE_WALKING + 0.3, "{folk:?} (scale {s:.2}): a flared cape stood only {:.2} off", flared / s);
    }
    // A tall human is past the scale where the old cap hid it.
    assert!(control_kit(Folk::Human, 1.35).0.scale > 1.083);
}
