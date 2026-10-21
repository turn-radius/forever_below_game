//! Claim doors and their switches as plain mesh data (#568).
//!
//! A door is built to the opening its doorway really has: [`Opening`] reads the width, the
//! height of the straight sides and the curve of the head from the doorway's [`DoorStyle`]
//! (a timber frame is square, the round, pointed and rock-cut arches are not), and every kind of
//! leaf fills that outline with a small clearance, so none floats off the frame or sinks into it.
//! A swinging leaf sits inside the reveal, a little back from the inward face, on a pin that
//! stands [`HINGE_SET`] off the hinge-side jamb; iron pintles let into the jamb (the leaf's
//! `fixed` part) hold the pin, so the hinges are on the frame.
//!
//! - **Wood** ([`DoorKind::Wood`]): vertical boards cut to the head, held by iron straps that
//!   wrap the pin, riveted through, with a ring pull and a lock plate on each face.
//! - **Steel** ([`DoorKind::Steel`]): one riveted plate cut to the head, heavier and thicker,
//!   with flat iron bands, stiffening ribs, heavy strap hinges, a lock box and a pull bar.
//! - **Portcullis** ([`DoorKind::Portcullis`]): a grate of upright bars with pointed feet and
//!   cross rails, cut to the head, that slides up in iron guides into the lintel instead of
//!   swinging. Iron in the stone halls; timber beams shod with iron in the rough rock.
//!
//! Switches: a slim lever in a short slot that throws up and down ([`lever_plate`],
//! [`lever_handle`]), or a chain and ring hanging from a small housing, pulled down and let
//! spring back ([`chain_housing`], [`chain_pull`]). Both are small, and made in their own space:
//! the origin on the wall (the plate's back a little in it), +Z out of the wall, +Y up.
//!
//! All the numbers are our own.

use std::f32::consts::{FRAC_PI_2, PI};

use armory::{
    ItemMesh, MaterialKind, Rgb,
    build::{
        fitting::{drop_handle, lock_plate},
        parts::{Look, Parts},
        plank::plank,
    },
    geom::{MeshBuilder, P2, Shading, V3, cylinder, extrude, frustum, icosphere, torus},
};
use fb_level::{
    DoorKind, DoorStyle, LevelStyle, SwitchKind,
    door::{CLEAR, HINGE_SET, LEAF_Z},
};
use procgen_rng::Mulberry32;

use super::arch::{Arch, prism};
use super::{DOOR_HEIGHT, DOOR_WIDTH};

/// The sill plank of a timber frame, which a leaf clears.
const TIMBER_SILL: f32 = 0.5;
/// The portcullis rises this far past its own crown, so its feet clear the opening.
const GATE_OVERLIFT: f32 = 0.8;
/// The steel of a steel door's plate, which reads against the iron of its bands.
const STEEL_PLATE: Rgb = Rgb::hex(0x6f7882);

/// The opening a doorway leaves, in the doorway's space (`x` across, `y` up from the floor).
#[derive(Clone, Copy, Debug)]
pub struct Opening {
    arch: Option<Arch>,
    pub half_width: f32,
    /// The clear height at the middle.
    pub crown: f32,
    /// The height of the floor in the opening: a timber frame has a sill.
    pub sill: f32,
}

impl Opening {
    pub fn of(style: DoorStyle) -> Self {
        match style {
            DoorStyle::Timber => Opening { arch: None, half_width: fb_level::door::half_width(style), crown: DOOR_HEIGHT, sill: TIMBER_SILL },
            _ => {
                let arch = Arch::new(style);
                Opening { arch: Some(arch), half_width: arch.hw, crown: arch.height(0.0, arch.r).unwrap_or(arch.spring + arch.r), sill: 0.0 }
            }
        }
    }

    /// The height up to which the sides run straight.
    pub fn straight(&self) -> f32 {
        self.arch.map_or(self.crown, |a| a.spring)
    }

    /// The clear height of the opening over `x`.
    pub fn top_at(&self, x: f32) -> f32 {
        let x = x.clamp(-self.half_width + 1e-3, self.half_width - 1e-3);
        self.arch.map_or(self.crown, |a| a.height(x, a.r).unwrap_or(0.0))
    }

    /// Half the opening's width at height `y`, `0` above the crown.
    pub fn half_width_at(&self, y: f32) -> f32 {
        match self.arch {
            None => if y <= self.crown { self.half_width } else { 0.0 },
            Some(a) if y <= a.spring => a.hw,
            Some(_) if y >= self.crown => 0.0,
            Some(a) => ((a.hw - a.r) + (a.r * a.r - (y - a.spring).powi(2)).max(0.0).sqrt()).clamp(0.0, a.hw),
        }
    }

    /// The outline of the opening from `a` to `b` across and `bottom` up, its head `inset`
    /// under the frame's, sampled `steps` times: convex, counter-clockwise.
    pub fn outline(&self, a: f32, b: f32, bottom: f32, inset: f32, steps: usize) -> Vec<P2> {
        let mut xs: Vec<f32> = (0..=steps).map(|k| a + (b - a) * k as f32 / steps as f32).collect();
        if self.arch.is_some() && a < 0.0 && b > 0.0 && !xs.contains(&0.0) {
            xs.push(0.0);
            xs.sort_by(f32::total_cmp);
        }
        let mut out = vec![[a, bottom], [b, bottom]];
        out.extend(xs.iter().rev().map(|&x| [x, (self.top_at(x) - inset).max(bottom + 0.5)]));
        out
    }
}

/// What a door is made of and how it is put together: the numbers the tweak view moves.
#[derive(Clone, Debug, PartialEq)]
pub struct DoorSpec {
    pub kind: DoorKind,
    pub style: DoorStyle,
    pub seed: u32,
    /// Wood: the boards across the leaf and the iron straps over them.
    pub boards: u32,
    pub straps: u32,
    /// Wood and steel: how thick the leaf is.
    pub thickness: f32,
    /// Steel: flat bands across the plate, stiffening ribs up it, and the gap between rivets.
    pub bands: u32,
    pub ribs: u32,
    pub rivet_gap: f32,
    /// Portcullis: upright bars, cross rails, the bar's radius (half its side if timber), and
    /// whether the grate is timber.
    pub bars: u32,
    pub rails: u32,
    pub bar: f32,
    pub timber: bool,
}

impl DoorSpec {
    /// The door of `kind` at a doorway of `style` in a level of `level_style`, its small
    /// variations rolled from `seed`.
    pub fn rolled(kind: DoorKind, style: DoorStyle, level_style: LevelStyle, seed: u32) -> Self {
        let mut rng = Mulberry32::new(seed ^ 0x5bec_d007);
        let timber = level_style == LevelStyle::RoughRock;
        DoorSpec {
            kind,
            style,
            seed,
            boards: 5 + (rng.range_f32(0.0, 1.0) * 3.0) as u32,
            straps: 2 + (rng.range_f32(0.0, 1.0) * 2.0) as u32,
            thickness: if kind == DoorKind::Steel { 1.1 } else { 1.0 },
            bands: 3 + (rng.range_f32(0.0, 1.0) * 2.0) as u32,
            ribs: (rng.range_f32(0.0, 1.0) * 2.0) as u32,
            rivet_gap: 1.4 + rng.range_f32(0.0, 1.0) * 0.5,
            bars: 6 + (rng.range_f32(0.0, 1.0) * 3.0) as u32,
            rails: 4 + (rng.range_f32(0.0, 1.0) * 2.0) as u32,
            bar: if timber { 0.42 } else { 0.28 },
            timber,
        }
    }
}

/// A claim door's leaf, in the space it moves in.
#[derive(Clone, Debug, PartialEq)]
pub struct ClaimLeaf {
    /// The moving part. A swinging leaf is in hinge space (the pin up +Y, the leaf along +X, its
    /// faces looking along ±Z); a portcullis is in doorway space.
    pub leaf: ItemMesh,
    /// What stays put, in doorway space: the pintles, or the guides.
    pub fixed: ItemMesh,
    /// Where the pin stands in doorway space (the origin for a portcullis).
    pub hinge: V3,
    /// How far the leaf rises when open: `0` for a swinging leaf.
    pub lift: f32,
    /// How tall a grate is (`0` for a swinging leaf).
    pub height: f32,
}

impl ClaimLeaf {
    pub fn rises(&self) -> bool {
        self.lift > 0.0
    }

    /// The height scale a grate telescopes to as it is fully raised, so that its top stands no
    /// higher than `cap` (the ceiling over the doorway) and nothing of it pokes out of the stone.
    pub fn squash_for(&self, cap: f32) -> f32 {
        if self.rises() { ((cap - self.lift) / self.height.max(1.0)).clamp(0.08, 1.0) } else { 1.0 }
    }
}

/// The leaf of `spec`, built to its doorway's opening.
pub fn claim_leaf(spec: &DoorSpec) -> ClaimLeaf {
    let opening = Opening::of(spec.style);
    match spec.kind {
        DoorKind::Wood => swinging(spec, &opening, wood),
        DoorKind::Steel => swinging(spec, &opening, steel),
        DoorKind::Portcullis => portcullis(spec, &opening),
    }
}

/// The span a swinging leaf covers across the opening: from the pin to the latch-side clearance.
fn span(opening: &Opening) -> (f32, f32) {
    // The same span as `fb_level::door::leaf_span`, which keeps folk out of the swing.
    (-opening.half_width + HINGE_SET, opening.half_width - CLEAR)
}

/// Heights `n` straps or bands are hung at, from near the floor to just under where the sides
/// stop running straight.
fn tiers(opening: &Opening, n: u32) -> Vec<f32> {
    let (lo, hi) = (opening.sill + 3.0, opening.straight() - 2.2);
    let n = n.max(2);
    (0..n).map(|i| lo + (hi - lo) * i as f32 / (n - 1) as f32).collect()
}

/// A swinging leaf, built in doorway space by `make` and moved into hinge space.
fn swinging(spec: &DoorSpec, opening: &Opening, make: fn(&DoorSpec, &Opening, &mut Parts)) -> ClaimLeaf {
    let (x0, _) = span(opening);
    let mut parts = Parts::new();
    make(spec, opening, &mut parts);
    for mesh in parts.meshes_mut() {
        mesh.translate([-x0, 0.0, 0.0]);
    }
    // The pintles on the hinge-side jamb, one at the height of each strap or band.
    let heights = tiers(opening, if spec.kind == DoorKind::Wood { spec.straps } else { spec.bands });
    let mut pintles = MeshBuilder::new();
    for y in heights {
        let reach = HINGE_SET + 0.15;
        let mut block = plank(reach, 1.5, 0.9, 0.12);
        block.translate([-opening.half_width - 0.15 + reach * 0.5, y, LEAF_Z]);
        pintles.append(&block);
        let mut eye = cylinder(0.42, 2.0, 10, Shading::BANDED);
        eye.translate([x0, y - 1.0, LEAF_Z]);
        pintles.append(&eye);
    }
    let mut fixed = Parts::new();
    fixed.add("pintles", pintles, MaterialKind::Iron);
    ClaimLeaf { leaf: parts.finish(), fixed: fixed.finish(), hinge: [x0, 0.0, LEAF_Z], lift: 0.0, height: 0.0 }
}

/// The ring pull and lock plate on each face of a leaf `thickness` thick, at `x`, in `iron`.
fn fittings(iron: &mut MeshBuilder, x: f32, y: f32, thickness: f32, proud: f32) {
    for face in [-1.0f32, 1.0] {
        let z = face * (thickness * 0.5 + proud);
        let forward = [0.0, 0.0, face];
        let mut ring = drop_handle(2.4, 0.3);
        ring.place_basis([x, y, z], [0.0, 1.0, 0.0], forward);
        iron.append(&ring);
        let (mut plate, mut keyhole) = lock_plate(1.8, 2.6, 0.18);
        plate.place_basis([x, y + 2.6, z], [0.0, 1.0, 0.0], forward);
        keyhole.place_basis([x, y + 2.6, z], [0.0, 1.0, 0.0], forward);
        iron.append(&plate);
        iron.append(&keyhole);
    }
}

/// A rivet head on a face at `(x, y)`, `z` out.
fn rivet(radius: f32, at: V3) -> MeshBuilder {
    let mut head = icosphere(radius, 1);
    head.scale([1.0, 1.0, 0.6]).translate(at);
    head
}

/// Planks and iron straps, cut to the head of the opening.
fn wood(spec: &DoorSpec, opening: &Opening, parts: &mut Parts) {
    let (x0, x1) = span(opening);
    let t = spec.thickness;
    let mut rng = Mulberry32::new(spec.seed ^ 0x1eaf_d00b);
    let n = spec.boards.max(2);
    let gap = 0.14;
    let width = (x1 - x0 - gap * (n - 1) as f32) / n as f32;
    let bottom = opening.sill + 0.1;
    let mut boards = MeshBuilder::new();
    for i in 0..n {
        let a = x0 + i as f32 * (width + gap);
        // Boards end a little short of one another's height, as sawn by hand, never over the head.
        let inset = CLEAR + rng.range_f32(0.0, 0.25);
        let b_bottom = bottom + rng.range_f32(0.0, 0.2);
        boards.append(&prism(&opening.outline(a, a + width, b_bottom, inset, 1), t, 0.12));
    }
    // The hinge stile is a round post turned on the pin, so it never meets the jamb as it swings.
    let post_top = opening.top_at(x0 - t * 0.5) - 0.3 - bottom;
    let mut post = cylinder(t * 0.5, post_top, 12, Shading::BANDED);
    post.translate([x0, bottom, 0.0]);
    boards.append(&post);
    parts.add("boards", boards, MaterialKind::Wood);

    let mut iron = MeshBuilder::new();
    let band = 0.2;
    for y in tiers(opening, spec.straps) {
        // A strap runs from the pin as far as the head allows, never past it.
        let end = (opening.half_width_at(y + 0.7) - 0.45).min(x1 - 0.3);
        let len = end - x0;
        if len < 3.0 {
            continue;
        }
        for face in [-1.0f32, 1.0] {
            let z = face * (t * 0.5 + band * 0.5);
            let mut strap = plank(len, 1.4, band, 0.05);
            strap.translate([x0 + len * 0.5, y, z]);
            iron.append(&strap);
            for k in 0..n {
                let x = x0 + width * 0.5 + k as f32 * (width + gap);
                if x < end - 0.4 {
                    iron.append(&rivet(0.22, [x, y, z + face * band * 0.5]));
                }
            }
        }
        let mut knuckle = cylinder(0.5, 1.8, 12, Shading::BANDED);
        knuckle.translate([x0, y - 0.9, 0.0]);
        iron.append(&knuckle);
    }
    let pin_top = tiers(opening, spec.straps).last().copied().unwrap_or(opening.straight()) + 1.0;
    let mut pin = cylinder(0.2, pin_top - bottom, 8, Shading::SMOOTH);
    pin.translate([x0, bottom, 0.0]);
    iron.append(&pin);
    fittings(&mut iron, x1 - 2.4, opening.straight() * 0.42, t, band);
    parts.add("ironwork", iron, MaterialKind::Iron);
}

/// A riveted plate, heavier than a wooden leaf, cut to the head of the opening.
fn steel(spec: &DoorSpec, opening: &Opening, parts: &mut Parts) {
    let (x0, x1) = span(opening);
    let t = spec.thickness;
    let bottom = opening.sill + 0.1;
    let mut plate = prism(&opening.outline(x0, x1, bottom, CLEAR, 18), t, 0.2);
    // The hinge edge is a round post turned on the pin, so it never meets the jamb as it swings.
    let mut post = cylinder(t * 0.5, opening.top_at(x0 - t * 0.5) - 0.3 - bottom, 12, Shading::BANDED);
    post.translate([x0, bottom, 0.0]);
    plate.append(&post);
    parts.add_look("plate", plate, Look::of(MaterialKind::Iron).tinted(STEEL_PLATE));

    let mut iron = MeshBuilder::new();
    let proud = 0.22;
    let heights = tiers(opening, spec.bands);
    let reach_of = |y: f32| (opening.half_width_at(y + 0.75) - 0.5).min(x1 - 0.3);
    for face in [-1.0f32, 1.0] {
        let z = face * (t * 0.5 + proud * 0.5);
        let rivet_z = z + face * proud * 0.5;
        for &y in &heights {
            let end = reach_of(y);
            let len = end - x0 - 0.45;
            if len < 3.0 {
                continue;
            }
            let mut band = plank(len, 1.2, proud, 0.06);
            band.translate([x0 + 0.45 + len * 0.5, y, z]);
            iron.append(&band);
            let mut x = x0 + 1.2;
            while x < end - 0.4 {
                iron.append(&rivet(0.24, [x, y, rivet_z]));
                x += spec.rivet_gap.max(0.6);
            }
        }
        // Ribs up the plate between the lowest band and the highest, clear of the head.
        let (lo, hi) = (heights[heights.len() - 2], *heights.last().unwrap());
        for k in 0..spec.ribs {
            let x = x0 + 2.5 + (x1 - x0 - 5.0) * (k + 1) as f32 / (spec.ribs + 1) as f32;
            let top = hi.min(opening.top_at(x) - 1.2);
            if top - lo < 3.0 {
                continue;
            }
            let mut rib = plank(top - lo, 0.9, proud, 0.06);
            rib.rotate_z(FRAC_PI_2).translate([x, (lo + top) * 0.5, z]);
            iron.append(&rib);
            let mut y = lo + 1.0;
            while y < top - 0.4 {
                iron.append(&rivet(0.22, [x, y, rivet_z]));
                y += spec.rivet_gap.max(0.6) * 1.6;
            }
        }
        // The lock box and a pull bar on the latch side.
        let (bx, by) = (x1 - 2.4, opening.straight() * 0.45);
        let mut lock_box = plank(3.4, 4.2, 0.5, 0.1);
        lock_box.translate([bx, by, face * (t * 0.5 + 0.25)]);
        iron.append(&lock_box);
        let forward = [0.0, 0.0, face];
        let (mut keyplate, mut keyhole) = lock_plate(1.5, 2.2, 0.15);
        keyplate.place_basis([bx, by + 0.4, face * (t * 0.5 + 0.5)], [0.0, 1.0, 0.0], forward);
        keyhole.place_basis([bx, by + 0.4, face * (t * 0.5 + 0.5)], [0.0, 1.0, 0.0], forward);
        iron.append(&keyplate);
        iron.append(&keyhole);
        let pull_x = x1 - 1.0;
        let mut bar = cylinder(0.22, 4.0, 10, Shading::SMOOTH);
        bar.translate([pull_x, by - 2.5, face * (t * 0.5 + 0.95)]);
        iron.append(&bar);
        for dy in [0.5, 3.5] {
            let mut stay = cylinder(0.18, 0.95, 8, Shading::SMOOTH);
            stay.place_basis([pull_x, by - 2.5 + dy, face * (t * 0.5)], [0.0, 0.0, face], [0.0, 1.0, 0.0]);
            iron.append(&stay);
        }
    }
    // Heavy strap hinges: a wide strap and a thick knuckle at each band.
    for &y in &heights {
        for face in [-1.0f32, 1.0] {
            let mut strap = plank(4.2, 1.7, 0.2, 0.05);
            strap.translate([x0 + 2.1, y, face * (t * 0.5 + 0.1)]);
            iron.append(&strap);
        }
        let mut knuckle = cylinder(0.5, 2.0, 12, Shading::BANDED);
        knuckle.translate([x0, y - 1.0, 0.0]);
        iron.append(&knuckle);
    }
    let mut pin = cylinder(0.22, heights.last().copied().unwrap_or(10.0) + 1.0 - bottom, 8, Shading::SMOOTH);
    pin.translate([x0, bottom, 0.0]);
    iron.append(&pin);
    parts.add("ironwork", iron, MaterialKind::Iron);
}

/// A grate that slides up in guides, cut to the head of the opening.
fn portcullis(spec: &DoorSpec, opening: &Opening) -> ClaimLeaf {
    let hw = opening.half_width;
    let inner = hw - 0.75;
    let r = spec.bar;
    let tip = opening.sill + 0.05;
    let foot = 1.5;
    let n = spec.bars.max(3);
    let mut bars = MeshBuilder::new();
    let mut shoes = MeshBuilder::new();
    let mut tops = Vec::new();
    let rail_r = r * 0.8;
    // The head rail follows the opening's curve, its top a little under the arch; each upright
    // stops in it.
    let mut head: Vec<(f32, f32)> = Vec::new();
    for i in 0..n {
        let x = -inner + 2.0 * inner * i as f32 / (n - 1) as f32;
        let rail_y = opening.top_at(x.abs() + r * 1.5) - 0.55 - rail_r;
        head.push((x, rail_y));
        let top = rail_y;
        tops.push(top);
        let body = top - (tip + foot);
        let mut bar = if spec.timber {
            let mut beam = plank(body, r * 2.0, r * 2.0, 0.08);
            beam.rotate_z(FRAC_PI_2).translate([x, tip + foot + body * 0.5, -0.2]);
            beam
        } else {
            let mut rod = cylinder(r, body, 8, Shading::SMOOTH);
            rod.translate([x, tip + foot, -0.2]);
            rod
        };
        bars.append(&bar);
        bar = frustum(0.03, if spec.timber { r * 1.5 } else { r }, foot, 8, Shading::SMOOTH);
        bar.translate([x, tip, -0.2]);
        // The iron-shod points of a timber grate are iron; a bar's own point is its own metal.
        if spec.timber { shoes.append(&bar) } else { bars.append(&bar) };
    }
    let mut rails = MeshBuilder::new();
    let mut studs = MeshBuilder::new();
    for pair in head.windows(2) {
        let ([x0, y0], [x1, y1]) = ([pair[0].0, pair[0].1], [pair[1].0, pair[1].1]);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len = dx.hypot(dy);
        let seg = if spec.timber {
            let mut beam = plank(len + rail_r, rail_r * 2.0, rail_r * 2.0, 0.08);
            beam.rotate_z(dy.atan2(dx)).translate([(x0 + x1) * 0.5, (y0 + y1) * 0.5, 0.2]);
            beam
        } else {
            let mut rod = armory::geom::tube(&[[x0, y0, 0.2], [x1, y1, 0.2]], 8, &armory::geom::SweepOptions::default(), |_| rail_r);
            rod.translate([0.0, 0.0, 0.0]);
            rod
        };
        rails.append(&seg);
    }
    let (low, high) = (tip + 3.0, (opening.straight() + (opening.crown - opening.straight()) * 0.55).min(opening.top_at(0.0) - 5.0));
    let rails_n = spec.rails.max(2);
    for j in 0..rails_n {
        let y = low + (high - low) * j as f32 / (rails_n - 1) as f32;
        // Where the sides run straight a rail runs on into the guide's channel; under the curve it
        // stops short of the arch.
        let reach = if y + rail_r + 1.3 <= opening.straight() { hw - 0.2 } else { opening.half_width_at(y + rail_r + 1.3) - 0.5 };
        if reach < 2.0 {
            continue;
        }
        let rail = if spec.timber {
            let mut beam = plank(reach * 2.0, rail_r * 2.0, rail_r * 2.0, 0.08);
            beam.translate([0.0, y, 0.2]);
            beam
        } else {
            let mut rod = cylinder(rail_r, reach * 2.0, 8, Shading::SMOOTH);
            rod.rotate_z(FRAC_PI_2).translate([reach, y, 0.2]);
            rod
        };
        rails.append(&rail);
        // A stud where each upright crosses the rail.
        for i in 0..n {
            let x = -inner + 2.0 * inner * i as f32 / (n - 1) as f32;
            if x.abs() <= reach && y < tops[i as usize] - 0.3 {
                let mut stud = icosphere(r * 1.25, 1);
                stud.translate([x, y, 0.0]);
                studs.append(&stud);
            }
        }
    }
    let mut parts = Parts::new();
    let (post, cross) = if spec.timber { (MaterialKind::Wood, MaterialKind::Wood) } else { (MaterialKind::Iron, MaterialKind::Iron) };
    parts.add("uprights", bars, post);
    parts.add("rails", rails, cross);
    parts.add("studs", studs, MaterialKind::Iron);
    parts.add("shoes", shoes, MaterialKind::Iron);

    // Guides let into the jambs, and a clasp over the head of each.
    let mut guides = MeshBuilder::new();
    let top = opening.straight().min(opening.crown) - 0.4;
    for side in [-1.0f32, 1.0] {
        let mut guide = plank(top - opening.sill, 0.42, 1.1, 0.06);
        guide.rotate_z(FRAC_PI_2).translate([side * (hw - 0.11), (top + opening.sill) * 0.5, 0.0]);
        guides.append(&guide);
    }
    let mut fixed = Parts::new();
    fixed.add("guides", guides, MaterialKind::Iron);
    ClaimLeaf { leaf: parts.finish(), fixed: fixed.finish(), hinge: [0.0; 3], lift: opening.crown + GATE_OVERLIFT, height: opening.crown }
}

/// The turn of a swinging leaf about its pin when it stands `swing` open (0 shut, 1 open): a
/// heavy door gathers way and settles, and a steel one is slower to start.
pub fn open_angle(progress: f32) -> f32 {
    super::entrance_swing(1.0 - progress)
}

// ---------------------------------------------------------------------------------------------
// Switches
// ---------------------------------------------------------------------------------------------

/// The lever's plate: 1.4 wide and 3 tall.
pub const LEVER_PLATE: [f32; 2] = [1.4, 3.0];
/// How far into the wall the plate's back sits, and how far its face stands out.
const PLATE_BACK: f32 = -0.5;
const PLATE_FACE: f32 = 0.35;
/// Where the lever pivots, along the switch's Z.
pub const LEVER_PIVOT: f32 = -0.1;
/// The handle's length from the pivot to the tip of its knob.
pub const LEVER_REACH: f32 = 3.0;
/// How far the handle swings up or down from level, radians.
pub const LEVER_ARC: f32 = 38.0 * PI / 180.0;
/// How far a mounted switch stands above the floor: the lever at hand height, a housing
/// higher up so its chain brings the ring to hand height.
pub const LEVER_HEIGHT: f32 = 11.0;
pub const CHAIN_HEIGHT: f32 = 14.5;
/// How far the ring is pulled down before it springs back.
pub const CHAIN_DROP: f32 = 1.6;
/// The chain housing: width, height and how far it stands out, and the length of chain to the
/// ring's top.
pub const HOUSING: [f32; 3] = [1.3, 1.9, 1.4];
const CHAIN_LENGTH: f32 = 3.4;
const RING_RADIUS: f32 = 0.75;

/// Where a switch of `kind` hangs above the floor.
pub fn switch_height(kind: SwitchKind) -> f32 {
    match kind {
        SwitchKind::Lever => LEVER_HEIGHT,
        SwitchKind::Chain => CHAIN_HEIGHT,
    }
}

/// The part of the wall a switch is aimed at, as a centre offset from its origin and half sizes
/// across and up (what its plate and what hangs from it cover).
pub fn switch_aim(kind: SwitchKind) -> ([f32; 2], [f32; 2]) {
    match kind {
        SwitchKind::Lever => ([0.0, 0.0], [LEVER_PLATE[0] * 0.5, LEVER_ARC.sin() * LEVER_REACH + 0.6]),
        SwitchKind::Chain => {
            let below = HOUSING[1] * 0.5 + CHAIN_LENGTH + RING_RADIUS * 2.0 + CHAIN_DROP;
            ([0.0, -(below - HOUSING[1] * 0.5) * 0.5], [RING_RADIUS + 0.3, (below + HOUSING[1] * 0.5) * 0.5])
        }
    }
}

/// How far the switch's origin stands off the wall's surface, so its back is in the wall
/// whether the wall is bare rock or stands proud in dressed blocks.
pub fn switch_proud(level_style: LevelStyle) -> f32 {
    match level_style {
        LevelStyle::RoughRock => 0.0,
        LevelStyle::StoneHalls => 0.5,
    }
}

fn dark() -> Look {
    Look::of(MaterialKind::Iron).tinted(Rgb::hex(0x16140f))
}

/// The lever's plate: a small iron plate let into the wall, with a short vertical slot the
/// handle comes out of and a rivet at each corner.
pub fn lever_plate(seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0x1e7e_4b01);
    let [w, h] = LEVER_PLATE;
    let slot = vec![[-0.25, -1.1], [0.25, -1.1], [0.25, 1.1], [-0.25, 1.1]];
    let mut plate = extrude(&[[-w / 2.0, -h / 2.0], [w / 2.0, -h / 2.0], [w / 2.0, h / 2.0], [-w / 2.0, h / 2.0]], &[slot.clone()], PLATE_FACE - PLATE_BACK);
    plate.translate([0.0, 0.0, (PLATE_FACE + PLATE_BACK) * 0.5]);
    let mut iron = MeshBuilder::new();
    iron.append(&plate);
    for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        iron.append(&rivet(0.13, [sx * (w * 0.5 - 0.25), sy * (h * 0.5 - 0.25), PLATE_FACE + rng.range_f32(0.0, 0.02)]));
    }
    // The pivot's bar runs across the slot, behind the handle.
    let mut bar = cylinder(0.14, 0.5, 8, Shading::SMOOTH);
    bar.rotate_z(FRAC_PI_2).translate([0.25, 0.0, LEVER_PIVOT - 0.15]);
    iron.append(&bar);
    let mut back = extrude(&slot, &[], 0.1);
    back.translate([0.0, 0.0, PLATE_BACK + 0.15]);
    let mut parts = Parts::new();
    parts.add("plate", iron, MaterialKind::Iron);
    parts.add_look("slot", back, dark());
    parts.finish()
}

/// The lever's handle in its pivot's space: an arm out along +Z ending in a knob. Turned about X
/// ([`lever_swing`]) the knob goes up or down.
pub fn lever_handle() -> ItemMesh {
    let mut iron = MeshBuilder::new();
    let mut arm = cylinder(0.16, LEVER_REACH - 0.4, 10, Shading::SMOOTH);
    arm.rotate_x(FRAC_PI_2).translate([0.0, 0.0, -0.2]);
    iron.append(&arm);
    let mut knob = icosphere(0.42, 2);
    knob.translate([0.0, 0.0, LEVER_REACH - 0.4]);
    iron.append(&knob);
    let mut hub = cylinder(0.3, 0.4, 10, Shading::BANDED);
    hub.rotate_z(FRAC_PI_2).translate([0.2, 0.0, 0.0]);
    iron.append(&hub);
    let mut parts = Parts::new();
    parts.add("handle", iron, MaterialKind::Iron);
    parts.finish()
}

/// The handle's turn about X, radians, with the lever `pull` of the way thrown: up (arm raised)
/// at 0, down at 1, eased as a stiff lever is.
pub fn lever_swing(pull: f32) -> f32 {
    let t = pull.clamp(0.0, 1.0);
    let eased = t * t * (3.0 - 2.0 * t);
    LEVER_ARC * (2.0 * eased - 1.0)
}

/// The chain's housing: a small iron box on a flange with the chain coming out of its foot.
pub fn chain_housing(seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0xc4a1_4b07);
    let [w, h, d] = HOUSING;
    let mut iron = MeshBuilder::new();
    let mut flange = plank(w + 0.3, h + 0.3, 0.3, 0.06);
    flange.translate([0.0, 0.0, PLATE_BACK + 0.15]);
    iron.append(&flange);
    let mut body = plank(w, h, d + PLATE_BACK, 0.12);
    body.translate([0.0, 0.0, PLATE_BACK + 0.3 + (d - 0.3) * 0.5]);
    iron.append(&body);
    for sy in [-1.0f32, 1.0] {
        iron.append(&rivet(0.12, [0.0, sy * (h * 0.5 + 0.05), PLATE_BACK + 0.32 + rng.range_f32(0.0, 0.02)]));
    }
    // The eye the chain hangs through.
    let mut eye = torus(0.28, 0.08, 10, 6, Shading::SMOOTH);
    eye.translate([0.0, -h * 0.5, chain_z()]);
    iron.append(&eye);
    let mut parts = Parts::new();
    parts.add("housing", iron, MaterialKind::Iron);
    parts.finish()
}

/// Where the chain hangs, along the switch's Z.
pub fn chain_z() -> f32 {
    PLATE_BACK + 0.3 + (HOUSING[2] - 0.3) * 0.5
}

/// Where the chain leaves the housing, in the switch's space.
pub fn chain_exit() -> V3 {
    [0.0, -HOUSING[1] * 0.5, chain_z()]
}

/// The chain and ring in their own space, origin where the chain leaves the housing: links up
/// inside the housing (hidden), links hanging down, and a ring at the foot to pull.
pub fn chain_pull(seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0x7c1a_90e3);
    let mut iron = MeshBuilder::new();
    let pitch = 0.5;
    let mut y = HOUSING[1] - 0.2;
    let mut k = 0;
    while y > -CHAIN_LENGTH {
        // A link is a stretched ring; alternate links turn edge-on.
        let mut link = torus(0.3, 0.075, 10, 6, Shading::SMOOTH);
        link.rotate_x(FRAC_PI_2).scale([0.75, 1.35, 1.0]);
        if k % 2 == 1 {
            link.rotate_y(FRAC_PI_2);
        }
        link.translate([0.0, y, 0.0]);
        iron.append(&link);
        y -= pitch;
        k += 1;
    }
    let mut ring = torus(RING_RADIUS, 0.14 + rng.range_f32(0.0, 0.02), 16, 8, Shading::SMOOTH);
    ring.rotate_x(FRAC_PI_2).translate([0.0, -CHAIN_LENGTH - RING_RADIUS + 0.2, 0.0]);
    iron.append(&ring);
    let mut parts = Parts::new();
    parts.add("chain", iron, MaterialKind::Iron);
    parts.finish()
}

/// How far down the ring is pulled while the door stands `swing` of the way open (0 or 1: at
/// rest): it is drawn down as the door moves and springs back as the door settles.
pub fn chain_dip(swing: f32) -> f32 {
    let s = swing.clamp(0.0, 1.0);
    CHAIN_DROP * (PI * s).sin()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLES: [DoorStyle; 4] = [DoorStyle::Timber, DoorStyle::RoundArch, DoorStyle::PointedArch, DoorStyle::RockCut];

    fn spec(kind: DoorKind, style: DoorStyle, seed: u32) -> DoorSpec {
        DoorSpec::rolled(kind, style, if seed % 2 == 0 { LevelStyle::RoughRock } else { LevelStyle::StoneHalls }, seed)
    }

    fn points(mesh: &ItemMesh) -> impl Iterator<Item = V3> + '_ {
        mesh.parts.iter().flat_map(|p| p.positions.iter().copied())
    }

    #[test]
    fn the_opening_matches_the_frame_it_describes() {
        let timber = Opening::of(DoorStyle::Timber);
        assert_eq!((timber.half_width, timber.crown), (DOOR_WIDTH * 0.5, DOOR_HEIGHT));
        for style in STYLES {
            let o = Opening::of(style);
            assert!((o.half_width - DOOR_WIDTH * 0.5).abs() < 1e-4);
            assert!(o.crown >= DOOR_HEIGHT - 1e-3, "{style:?}: the head is lower than a timber door's");
            // The head only falls away from the middle, and the sides are straight up to the spring.
            assert!((o.top_at(0.0) - o.crown).abs() < 1e-3);
            assert!(o.top_at(3.0) >= o.top_at(5.0) - 1e-3);
            assert!((o.half_width_at(o.straight() - 0.1) - o.half_width).abs() < 1e-4);
            assert!(o.half_width_at(o.crown + 0.1) == 0.0 && (style == DoorStyle::Timber || o.half_width_at(o.crown - 0.2) < o.half_width));
        }
        // A pointed head is higher than a round one.
        assert!(Opening::of(DoorStyle::PointedArch).crown > Opening::of(DoorStyle::RoundArch).crown + 2.0);
    }

    #[test]
    fn every_kinds_leaf_fits_every_opening() {
        for style in STYLES {
            let o = Opening::of(style);
            for kind in DoorKind::ALL {
                for seed in 0..12 {
                    let s = spec(kind, style, seed);
                    let built = claim_leaf(&s);
                    for part in built.leaf.parts.iter().chain(&built.fixed.parts) {
                        part.validate().unwrap_or_else(|e| panic!("{kind:?} {style:?}: {e}"));
                        assert!(part.positions.iter().all(|p| p.iter().all(|c| c.is_finite())));
                    }
                    for p in points(&built.leaf) {
                        // Into doorway space.
                        let (x, y, z) = (p[0] + built.hinge[0], p[1], p[2] + built.hinge[2]);
                        assert!(x.abs() <= o.half_width + 1e-3, "{kind:?} {style:?} seed {seed}: x {x} outside the jambs");
                        assert!(y >= o.sill - 1e-3, "{kind:?} {style:?}: y {y} under the sill");
                        assert!(y <= o.top_at(x) + 0.02, "{kind:?} {style:?} seed {seed}: ({x}, {y}) over the head {}", o.top_at(x));
                        // Within the thinnest reveal (a timber frame's), with the leaf set back from its face.
                        assert!(z.abs() <= super::super::FRAME_DEPTH * 0.5 + 0.7, "{kind:?} {style:?}: z {z} out of the reveal");
                    }
                    // The pintles and guides are let into the jambs (a hair), and no more.
                    for p in points(&built.fixed) {
                        assert!(p[0].abs() <= o.half_width + 0.16, "{kind:?} {style:?}: fixed part at x {}", p[0]);
                        assert!(p[1] <= o.crown + 1e-3 && p[1] >= o.sill - 1e-3);
                    }
                }
            }
        }
    }

    #[test]
    fn a_swinging_leaf_fills_the_opening_it_hangs_in() {
        for style in STYLES {
            let o = Opening::of(style);
            for kind in [DoorKind::Wood, DoorKind::Steel] {
                let built = claim_leaf(&spec(kind, style, 3));
                assert!(!built.rises());
                let xs: Vec<f32> = points(&built.leaf).map(|p| p[0] + built.hinge[0]).collect();
                let (lo, hi) = xs.iter().fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
                // The pin is set off the hinge jamb, and the latch edge clears its own by a hair.
                assert!(lo > -o.half_width + HINGE_SET - 0.7 && lo < -o.half_width + HINGE_SET + 0.1, "{kind:?} {style:?}: lo {lo}");
                assert!(hi > o.half_width - 0.5 && hi <= o.half_width, "{kind:?} {style:?}: hi {hi}");
                let top = points(&built.leaf).map(|p| p[1]).fold(f32::MIN, f32::max);
                assert!(top > o.straight() - 0.5, "{kind:?} {style:?}: the leaf is only {top} high");
                // The hinge stands inside the opening, on the frame side.
                assert!(built.hinge[0] < -o.half_width + 1.5 && built.hinge[0] > -o.half_width);
            }
        }
    }

    #[test]
    fn a_swung_leaf_stays_clear_of_the_jamb() {
        // Turning about the pin, no point of the leaf passes into the hinge-side jamb.
        for style in STYLES {
            let o = Opening::of(style);
            for kind in [DoorKind::Wood, DoorKind::Steel] {
                let built = claim_leaf(&spec(kind, style, 5));
                for step in 0..=10 {
                    let angle = open_angle(step as f32 / 10.0);
                    let (s, c) = angle.sin_cos();
                    for p in points(&built.leaf) {
                        let x = built.hinge[0] + p[0] * c + p[2] * s;
                        let z = built.hinge[2] - p[0] * s + p[2] * c;
                        // Only what is still within the frame's depth can meet the jamb.
                        if z.abs() < 2.1 {
                            assert!(x >= -o.half_width - 0.02, "{kind:?} {style:?}: swung to x {x} from {p:?} at {angle}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_portcullis_rises_clear_of_the_opening_and_has_feet_on_the_sill() {
        for style in STYLES {
            let o = Opening::of(style);
            for timber in [false, true] {
                let mut s = spec(DoorKind::Portcullis, style, 7);
                s.timber = timber;
                s.bar = if timber { 0.42 } else { 0.28 };
                let built = claim_leaf(&s);
                assert!(built.rises());
                let low = points(&built.leaf).map(|p| p[1]).fold(f32::MAX, f32::min);
                assert!(low >= o.sill && low < o.sill + 0.2, "{style:?}: the feet are at {low}");
                assert!(low + built.lift > o.crown, "{style:?}: raised, its feet at {} still hang in the opening", low + built.lift);
                // The grate is a grate: bars with gaps between, not a plate.
                let uprights = built.leaf.parts.iter().find(|p| p.name == "uprights").unwrap();
                let xs: std::collections::BTreeSet<i32> = uprights.positions.iter().map(|p| (p[0] * 4.0).round() as i32).collect();
                assert!(xs.len() > 12);
                assert_eq!(built.hinge, [0.0; 3]);
            }
        }
    }

    #[test]
    fn a_portcullis_follows_the_arch_and_no_bar_ends_outside_it() {
        for style in [DoorStyle::RoundArch, DoorStyle::PointedArch, DoorStyle::RockCut] {
            let o = Opening::of(style);
            for timber in [false, true] {
                for seed in 0..6 {
                    let mut s = spec(DoorKind::Portcullis, style, seed);
                    s.timber = timber;
                    s.bar = if timber { 0.42 } else { 0.28 };
                    let built = claim_leaf(&s);
                    let uprights = built.leaf.parts.iter().find(|p| p.name == "uprights").unwrap();
                    let rails = built.leaf.parts.iter().find(|p| p.name == "rails").unwrap();
                    // Every point of every bar, and of the head rail, lies inside the profile with room to spare.
                    for p in uprights.positions.iter().chain(&rails.positions) {
                        assert!(p[1] <= o.top_at(p[0]) - 0.3, "{style:?} timber {timber}: ({}, {}) over the head {}", p[0], p[1], o.top_at(p[0]));
                    }
                    // Each upright's top is held by the head rail: it reaches the rail's height there.
                    let xs: std::collections::BTreeSet<i32> = uprights.positions.iter().map(|p| (p[0] * 100.0).round() as i32).collect();
                    assert!(xs.len() > 6);
                    let rail_top = |x: f32| rails.positions.iter().filter(|p| (p[0] - x).abs() < 1.6).map(|p| p[1]).fold(f32::MIN, f32::max);
                    for p in uprights.positions.iter().filter(|p| p[1] > o.straight()) {
                        assert!(p[1] <= rail_top(p[0]) + 0.1, "{style:?}: a bar ends at {} over its rail {}", p[1], rail_top(p[0]));
                    }
                    // The rail rises toward the crown, as the arch does.
                    assert!(rail_top(0.0) > rail_top(5.0) + 0.4, "{style:?} timber {timber} seed {seed}: the head rail is flat {} {}", rail_top(0.0), rail_top(5.0));
                }
            }
        }
    }

    #[test]
    fn a_raised_grate_telescopes_to_stay_under_the_ceiling_and_clears_the_opening() {
        for style in STYLES {
            let o = Opening::of(style);
            let built = claim_leaf(&spec(DoorKind::Portcullis, style, 2));
            for cap in [28.0f32, 31.0, 34.0, 40.0] {
                let squash = built.squash_for(cap);
                assert!((0.08..=1.0).contains(&squash));
                // Fully raised: the top is no higher than the cap (unless the floor of the squash was hit),
                // and the feet are above the opening.
                let top = built.lift + built.height * squash;
                assert!(top <= cap + 1e-3 || squash <= 0.0801, "{style:?} cap {cap}: top {top}");
                assert!(built.lift > o.crown, "the feet rise past the head of the opening");
            }
            // A swinging leaf is never squashed.
            assert_eq!(claim_leaf(&spec(DoorKind::Wood, style, 2)).squash_for(30.0), 1.0);
        }
    }

    #[test]
    fn the_grates_rails_run_into_the_guides() {
        let o = Opening::of(DoorStyle::Timber);
        let built = claim_leaf(&spec(DoorKind::Portcullis, DoorStyle::Timber, 4));
        let rails = built.leaf.parts.iter().find(|p| p.name == "rails").unwrap();
        let reach = rails.positions.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
        let guide_inner = built.fixed.parts.iter().flat_map(|p| p.positions.iter()).filter(|p| p[0] > 0.0).map(|p| p[0]).fold(f32::MAX, f32::min);
        assert!(reach > guide_inner, "the rails stop {} short of the guides' channel", guide_inner - reach);
        assert!(reach <= o.half_width, "and no further than the jamb");
    }

    #[test]
    fn the_doors_are_deterministic_and_the_kinds_differ() {
        for kind in DoorKind::ALL {
            assert_eq!(claim_leaf(&spec(kind, DoorStyle::RoundArch, 9)), claim_leaf(&spec(kind, DoorStyle::RoundArch, 9)));
        }
        let weight = |kind| claim_leaf(&spec(kind, DoorStyle::Timber, 9)).leaf.parts.iter().map(|p| p.positions.len()).sum::<usize>();
        assert!(weight(DoorKind::Steel) != weight(DoorKind::Wood) && weight(DoorKind::Portcullis) != weight(DoorKind::Wood));
        // A steel door is a thicker leaf than a wooden one.
        assert!(spec(DoorKind::Steel, DoorStyle::Timber, 1).thickness > spec(DoorKind::Wood, DoorStyle::Timber, 1).thickness);
    }

    #[test]
    fn switches_are_small_and_built_on_the_wall() {
        // The lever's plate is no larger than 1.5 by 3, and stands out of the wall by under a unit.
        let plate = lever_plate(3);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in points(&plate) {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        assert!(hi[0] - lo[0] <= 1.5 && hi[1] - lo[1] <= 3.0 + 1e-3, "{lo:?} {hi:?}");
        assert!(lo[2] >= PLATE_BACK - 1e-3 && hi[2] <= 0.5, "{lo:?} {hi:?}");
        // The housing too, at the same footprint.
        let housing = chain_housing(3);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in points(&housing) {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        assert!(hi[0] - lo[0] <= 1.7 && hi[1] - lo[1] <= 2.3, "{lo:?} {hi:?}");
        for mesh in [&plate, &housing, &lever_handle(), &chain_pull(3)] {
            for part in &mesh.parts {
                part.validate().unwrap();
            }
        }
        // Hanging at hand height, the ring reaches down to where a hand can take it.
        let ring_bottom = CHAIN_HEIGHT + chain_exit()[1] - CHAIN_LENGTH - RING_RADIUS * 2.0 - CHAIN_DROP;
        assert!((6.0..12.0).contains(&ring_bottom), "{ring_bottom}");
    }

    #[test]
    fn the_lever_throws_up_and_down_and_the_chain_springs_back() {
        assert!((lever_swing(0.0) + LEVER_ARC).abs() < 1e-5 && (lever_swing(1.0) - LEVER_ARC).abs() < 1e-5);
        assert!(lever_swing(0.5).abs() < 1e-5);
        // Turned about X, the arm (along +Z) rises at pull 0 and falls at pull 1.
        let up = glam_rotate_x(lever_swing(0.0));
        let down = glam_rotate_x(lever_swing(1.0));
        assert!(up > 0.0 && down < 0.0);
        assert!(chain_dip(0.0).abs() < 1e-6 && chain_dip(1.0).abs() < 1e-4);
        assert!(chain_dip(0.5) > CHAIN_DROP - 1e-4);
    }

    /// The height of a unit arm along +Z after a turn of `angle` about X.
    fn glam_rotate_x(angle: f32) -> f32 {
        -angle.sin()
    }
}
