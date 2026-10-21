//! Scenery: what lies about a level that nobody carries off. Heaps of rubble, scattered
//! bones and stone pillars, built from the shared geometry like every item, so the level
//! never draws a one-off shape of its own (`CLAUDE.md`: "The armory crate makes the shape of
//! every item").
//!
//! Scenery is a set of builders rather than an Armory [`crate::Family`]: nothing here is ever
//! an item, so it has no state to roll, no noun and no place in the turntable or the loot
//! tables. Each builder is a pure function of a seed (and, for a pillar, the ceiling it holds
//! up), in item space: +Y up from the floor it rests on, centred on the origin, within
//! [`Scenery::radius`] of it across the floor. A whole pillar reaches past the ceiling height
//! it is given, so its top beds into the rock's relief.

use std::f32::consts::{PI, TAU};

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::build::furniture;
use crate::build::parts::Parts;
use crate::build::roll::rolled_sheet;
use crate::geom::{CatmullRom, ItemMesh, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, cuboid, icosphere, lathe, tube, value_noise3, vec3};
use crate::geom::{Hollow, carve_hollows};
use crate::material::{MaterialKind, Rgb};

/// What a piece of scenery is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scenery {
    /// A heap of broken stone fallen from the walls and ceiling.
    Rubble,
    /// Someone who did not get out: a skull and a scatter of long bones and ribs.
    Bones,
    /// A dressed stone column, whole and bedded in the ceiling, or broken off short.
    Pillar,
    /// A bedroll unrolled on the floor, human length (about 17 units): a canvas mat under a
    /// blanket, with a rolled pillow at the head. Lies along item X, head toward -X.
    Bedroll,
    /// A timber bunk bed, human length (19 by 8 units), on four posts: a straw tick, a blanket
    /// and a pillow, one tier or two (about 15 units high). Long side along X, back to -Z.
    Bunk,
    /// A heavy stone or carved timber chair on a stepped dais (14 by 11 units), a tall back
    /// (20 to 26 units), armrests and finials. Back to -Z, front toward +Z.
    Throne,
    /// A stone altar 10 units high and 12 wide by 6 deep, with a cloth runner, candles, and a
    /// bowl or an idol. Back to -Z.
    Altar,
    /// A tall timber bookcase, 16 wide, 5 deep and 20 to 24 high, shelves of books. Back to -Z.
    Bookshelf,
    /// A plank table 8 units high and 14 by 7, with benches and clutter. Back to -Z.
    Table,
    /// A timber weapon rack 14 wide and 4 deep, holding spears, axes, halberds and maces.
    /// Back to -Z.
    WeaponRack,
}

impl Scenery {
    pub const ALL: [Scenery; 10] = [
        Scenery::Rubble,
        Scenery::Bones,
        Scenery::Pillar,
        Scenery::Bedroll,
        Scenery::Bunk,
        Scenery::Throne,
        Scenery::Altar,
        Scenery::Bookshelf,
        Scenery::Table,
        Scenery::WeaponRack,
    ];

    /// The rooms' furniture, stood with its back to a wall or a room's edge.
    pub const FURNITURE: [Scenery; 6] = [Scenery::Bunk, Scenery::Throne, Scenery::Altar, Scenery::Bookshelf, Scenery::Table, Scenery::WeaponRack];

    /// Half the width (along X) and half the depth (along Z) of the floor a piece of furniture
    /// covers, centred on the origin with its back at -Z: nothing of any seed reaches past it.
    /// None for the round and irregular kinds, which have only a [`Scenery::radius`].
    pub fn footprint(self) -> Option<(f32, f32)> {
        match self {
            Scenery::Bunk => Some((9.5, 4.0)),
            Scenery::Throne => Some((7.0, 5.5)),
            Scenery::Altar => Some((6.4, 3.4)),
            Scenery::Bookshelf => Some((8.0, 2.5)),
            Scenery::Table => Some((7.0, 5.5)),
            Scenery::WeaponRack => Some((7.0, 2.0)),
            Scenery::Rubble | Scenery::Bones | Scenery::Pillar | Scenery::Bedroll => None,
        }
    }

    /// The furthest any part of it reaches from its centre across the floor, in world units,
    /// whatever its seed. A pillar's square plinth is this far to a corner.
    pub fn radius(self) -> f32 {
        match self {
            Scenery::Rubble => 4.6,
            Scenery::Bones => 3.6,
            Scenery::Pillar => 4.0,
            Scenery::Bedroll => 9.1,
            // The corner of each piece's footprint, rounded up.
            Scenery::Bunk => 10.4,
            Scenery::Throne => 9.0,
            Scenery::Altar => 7.3,
            Scenery::Bookshelf => 8.4,
            Scenery::Table => 9.0,
            Scenery::WeaponRack => 7.3,
        }
    }

    /// The scenery's mesh for `seed`, under a ceiling `ceiling` units up.
    pub fn build(self, seed: u32, ceiling: f32) -> ItemMesh {
        match self {
            Scenery::Rubble => rubble(seed),
            Scenery::Bones => bones(seed),
            Scenery::Pillar => pillar(seed, ceiling),
            Scenery::Bedroll => bedroll(seed),
            Scenery::Bunk => furniture::bunk(seed),
            Scenery::Throne => furniture::throne(seed),
            Scenery::Altar => furniture::altar(seed),
            Scenery::Bookshelf => furniture::bookshelf(seed),
            Scenery::Table => furniture::table(seed),
            Scenery::WeaponRack => furniture::weapon_rack(seed),
        }
    }
}

/// The level's stone, matching the walls the rubble fell from (`WALL_STONE` in the game).
const ROCK: Rgb = Rgb::hex(0x7a7b75);
/// Dressed stone, a shade paler than the raw rock, for pillars.
const DRESSED: Rgb = Rgb::hex(0x8d8b82);
/// Old bone, yellowed.
const BONE: Rgb = Rgb::hex(0xd6ccb0);
/// The dark inside an eye socket.
const SOCKET: Rgb = Rgb::hex(0x241e18);

/// A broken lump of stone about `size` across: an icosphere roughened by value noise and
/// squashed a little, faceted as chipped stone is.
pub fn stone_chunk(size: f32, rng: &mut Mulberry32) -> MeshBuilder {
    let mut chunk = icosphere(0.5, 1);
    let noise_seed = (rng.next_f64() * 4_294_967_296.0) as u32;
    let roughness = rng.range_f32(0.25, 0.45);
    for p in &mut chunk.positions {
        let k = 1.0 + roughness * (value_noise3(vec3::scale(*p, 3.0), noise_seed) - 0.5) * 2.0;
        *p = vec3::scale(*p, k);
    }
    chunk.scale([size * rng.range_f32(0.85, 1.2), size * rng.range_f32(0.5, 0.8), size * rng.range_f32(0.85, 1.2)]);
    chunk.rotate_y(rng.range_f32(0.0, TAU)).rotate_x(rng.range_f32(-0.3, 0.3));
    chunk.flat_shaded()
}

/// One layer of a heap of rubble: how many chunks, their size, how far out from the middle
/// they lie, and how high their middles sit, in world units or, for a layer on the floor, in
/// multiples of each chunk's size.
struct HeapLayer {
    count: (u32, u32),
    size: (f32, f32),
    out: (f32, f32),
    height: (f32, f32),
    on_floor: bool,
}

/// A heap of rubble: big stones low in the middle, smaller ones about the edges and on top,
/// and now and then a flat slab of fallen ceiling.
pub fn rubble(seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0x7ab0_1e5c);
    let reach = Scenery::Rubble.radius();
    let (mut light, mut dark) = (MeshBuilder::new(), MeshBuilder::new());
    // Built up as a heap settles: a core of big blocks on the floor, smaller ones lodged on top
    // of them, a few chips on the crown, and a scatter that rolled down round the foot.
    let layers = [
        HeapLayer { count: (3, 4), size: (1.9, 2.5), out: (0.3, 1.3), height: (0.25, 0.3), on_floor: true },
        HeapLayer { count: (3, 5), size: (1.2, 1.7), out: (0.2, 1.4), height: (1.5, 1.9), on_floor: false },
        HeapLayer { count: (1, 3), size: (0.7, 1.1), out: (0.0, 0.8), height: (2.4, 2.7), on_floor: false },
        HeapLayer { count: (6, 10), size: (0.45, 1.0), out: (1.9, reach), height: (0.25, 0.3), on_floor: true },
    ];
    for layer in layers {
        for _ in 0..rng.int(layer.count.0, layer.count.1) {
            let size = rng.range_f32(layer.size.0, layer.size.1);
            // A chunk reaches about 0.7 of its size from its middle (half, roughened and
            // stretched), so it stays inside the heap's radius.
            let r = rng.range_f32(layer.out.0, layer.out.1).min(reach - size * 0.75);
            let a = rng.range_f32(0.0, TAU);
            let mut chunk = stone_chunk(size, &mut rng);
            let y = rng.range_f32(layer.height.0, layer.height.1) * if layer.on_floor { size } else { 1.0 };
            chunk.translate([a.cos() * r, y, a.sin() * r]);
            if rng.chance(0.4) { dark.append(&chunk) } else { light.append(&chunk) };
        }
    }
    if rng.chance(0.5) {
        let (w, d) = (rng.range_f32(1.6, 2.4), rng.range_f32(1.1, 1.6));
        let mut slab = cuboid([w, rng.range_f32(0.3, 0.5), d]);
        let a = rng.range_f32(0.0, TAU);
        slab.rotate_z(rng.range_f32(0.15, 0.4)).rotate_y(a);
        let r = reach - w.hypot(d) * 0.55;
        slab.translate([a.cos() * r * 0.6, 0.6, a.sin() * r * 0.6]);
        light.append(&slab.flat_shaded());
    }
    let mut parts = Parts::new();
    parts.add_tinted("rubble", light, MaterialKind::Stone, ROCK);
    parts.add_tinted("rubble_dark", dark, MaterialKind::Stone, ROCK.scaled(0.82));
    parts.rest_on_ground();
    parts.finish()
}

/// A long bone `length` units long lying along X, centred on the origin: a shaft swelling to
/// knobbed ends, turned on a lathe.
fn long_bone(length: f32, rng: &mut Mulberry32) -> MeshBuilder {
    let (head, shaft) = (length * rng.range_f32(0.065, 0.08), length * rng.range_f32(0.03, 0.038));
    let l = length;
    let profile = [
        [0.0, 0.0],
        [head * 0.7, l * 0.005],
        [head, l * 0.04],
        [head * 0.85, l * 0.09],
        [shaft * 1.15, l * 0.18],
        [shaft, l * 0.5],
        [shaft * 1.1, l * 0.82],
        [head * 0.8, l * 0.92],
        [head * 1.05, l * 0.965],
        [head * 0.7, l * 0.995],
        [0.0, l],
    ];
    let mut bone = lathe(&profile, &LatheOptions { facets: 12, shading: Shading::SMOOTH, cap_start: false, cap_end: false, phase: 0.0 });
    bone.translate([0.0, -l * 0.5, 0.0]).rotate_z(PI * 0.5);
    bone
}

/// A rib: a thin curved bone lying on the floor, `span` across its arc.
fn rib(span: f32, rng: &mut Mulberry32) -> MeshBuilder {
    let bend = rng.range_f32(0.35, 0.5) * span;
    let path = CatmullRom::centripetal(vec![[-span * 0.5, 0.0, 0.0], [-span * 0.2, 0.0, bend * 0.8], [span * 0.2, 0.0, bend], [span * 0.5, 0.0, bend * 0.6]]).sample(10);
    tube(&path, 8, &SweepOptions::default(), |t| 0.09 * (1.0 - 0.4 * t))
}

/// A skull about two units long, facing +Z: a cranium with its eye sockets and nose pressed
/// in, and the jaw beneath.
fn skull(rng: &mut Mulberry32) -> (MeshBuilder, MeshBuilder) {
    let mut cranium = icosphere(1.0, 3);
    cranium.scale([0.68, 0.74, 0.9]);
    // A narrower face under the brow.
    for p in &mut cranium.positions {
        if p[2] > 0.2 && p[1] < 0.1 {
            let k = 1.0 - 0.25 * ((p[2] - 0.2) / 0.7).min(1.0) * ((0.1 - p[1]) / 0.8).min(1.0);
            p[0] *= k;
        }
    }
    cranium.recompute_smooth_normals();
    let hollows = [
        Hollow::bite([0.0, 0.0, 0.0], [0.38, 0.0, 1.0], 0.85, 0.2),
        Hollow::bite([0.0, 0.0, 0.0], [-0.38, 0.0, 1.0], 0.85, 0.2),
        Hollow::bite([0.0, 0.0, 0.0], [0.0, -0.32, 1.0], 0.85, 0.12),
    ];
    let (mut bone, mut sockets) = carve_hollows(&cranium, &hollows);
    let jaw_path = CatmullRom::centripetal(vec![[-0.45, -0.45, 0.05], [-0.36, -0.68, 0.5], [0.0, -0.74, 0.74], [0.36, -0.68, 0.5], [0.45, -0.45, 0.05]]).sample(14);
    bone.append(&tube(&jaw_path, 8, &SweepOptions::default(), |_| 0.1));
    // Lying as it fell: on its side, or tipped forward, or upright.
    let (tip, roll) = (rng.range_f32(-0.4, 0.3), if rng.chance(0.4) { rng.range_f32(1.1, 1.5) } else { rng.range_f32(-0.2, 0.2) });
    for mesh in [&mut bone, &mut sockets] {
        mesh.rotate_x(tip).rotate_z(roll);
    }
    (bone, sockets)
}

/// The bones of someone long dead: a skull, a few long bones and ribs scattered about.
pub fn bones(seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0xb0e5_5ca7);
    let reach = Scenery::Bones.radius();
    let mut bone = MeshBuilder::new();
    let place = |mut mesh: MeshBuilder, extent: f32, rng: &mut Mulberry32| {
        let r = rng.range_f32(0.0, 1.0).sqrt() * (reach - extent).max(0.0);
        let a = rng.range_f32(0.0, TAU);
        mesh.rotate_y(rng.range_f32(0.0, TAU));
        let low = mesh.bounds().map_or(0.0, |(lo, _)| lo[1]);
        mesh.translate([a.cos() * r, -low, a.sin() * r]);
        mesh
    };
    let (skull_bone, sockets) = skull(&mut rng);
    let at_r = rng.range_f32(0.0, 1.0).sqrt() * (reach - 1.1);
    let (a, turn) = (rng.range_f32(0.0, TAU), rng.range_f32(0.0, TAU));
    let low = skull_bone.bounds().map_or(0.0, |(lo, _)| lo[1]);
    let mut sockets = sockets;
    let mut skull_bone = skull_bone;
    for mesh in [&mut skull_bone, &mut sockets] {
        mesh.rotate_y(turn).translate([a.cos() * at_r, -low, a.sin() * at_r]);
    }
    bone.append(&skull_bone);
    for _ in 0..rng.int(2, 4) {
        let length = rng.range_f32(2.6, 4.4);
        let piece = place(long_bone(length, &mut rng), length * 0.5 + 0.1, &mut rng);
        bone.append(&piece);
    }
    for _ in 0..rng.int(1, 4) {
        let span = rng.range_f32(1.4, 2.2);
        let piece = place(rib(span, &mut rng), span * 0.6, &mut rng);
        bone.append(&piece);
    }
    let mut parts = Parts::new();
    parts.add_tinted("bones", bone, MaterialKind::Bone, BONE.scaled(rng.range_f32(0.9, 1.0)));
    parts.add_look("sockets", sockets, crate::build::parts::Look::of(MaterialKind::Bone).tinted(SOCKET).without_outline());
    parts.finish()
}

/// A dressed stone column on a stepped square plinth: whole, with a capital bedded in the
/// ceiling `ceiling` units up, or broken off short with its ragged top and a few chips of it
/// lying on the plinth.
pub fn pillar(seed: u32, ceiling: f32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0x9111_a7e5);
    let half = Scenery::Pillar.radius() * std::f32::consts::FRAC_1_SQRT_2 - 0.05;
    let mut plinth = cuboid([half * 2.0, 1.2, half * 2.0]);
    plinth.translate([0.0, 0.6, 0.0]);
    let step = half * 0.84;
    let mut upper = cuboid([step * 2.0, 0.8, step * 2.0]);
    upper.translate([0.0, 1.6, 0.0]);
    plinth.append(&upper);

    let radius = rng.range_f32(1.6, 1.9).min(step * 0.78);
    let foot = 2.0;
    let broken = rng.chance(0.45);
    let top = if broken { rng.range_f32(8.0, 18.0) } else { ceiling + 1.2 };
    let capital = if broken { top } else { ceiling - 1.6 };
    // The shaft with a gentle entasis, on a torus of moulding, faceted round as dressed stone.
    let mut profile: Vec<[f32; 2]> = vec![[radius * 1.22, foot], [radius * 1.25, foot + 0.3], [radius * 1.08, foot + 0.65], [radius, foot + 0.8]];
    let rows = 12;
    for i in 1..=rows {
        let t = i as f32 / rows as f32;
        let y = foot + 0.8 + (capital - foot - 0.8) * t;
        profile.push([radius * (1.0 + 0.04 * (PI * t * 0.8).sin() - 0.06 * t), y]);
    }
    if broken {
        profile.push([radius * 0.6, top + 0.15]);
        profile.push([0.0, top + 0.25]);
    }
    let flutes = 2 * rng.int(8, 10) as usize;
    let mut shaft = lathe(&profile, &LatheOptions { facets: flutes, shading: Shading::FACETED, cap_start: false, cap_end: false, phase: 0.0 });
    if broken {
        // The break: everything near the top drops by a ragged amount, so the column ends in a
        // jagged edge rather than a cut.
        let noise_seed = seed.wrapping_mul(0x2c1b_3c6d);
        let band = 1.4;
        for p in &mut shaft.positions {
            let into = (p[1] - (top - band)) / band;
            if into > 0.0 {
                let n = value_noise3([p[0] * 0.9, 0.0, p[2] * 0.9], noise_seed);
                p[1] -= into.min(1.0) * n * 1.6;
            }
        }
        shaft.recompute_smooth_normals();
    }
    let mut parts = Parts::new();
    parts.add_tinted("plinth", plinth, MaterialKind::Stone, DRESSED.scaled(0.9));
    parts.add_tinted("shaft", shaft, MaterialKind::Stone, DRESSED);
    if broken {
        let mut chips = MeshBuilder::new();
        for _ in 0..rng.int(2, 4) {
            let size = rng.range_f32(0.6, 1.1);
            let mut chip = stone_chunk(size, &mut rng);
            let a = rng.range_f32(0.0, TAU);
            let r = rng.range_f32(radius * 1.35, step - size * 0.5).max(radius * 1.3);
            let (x, z) = (a.cos() * r, a.sin() * r);
            // On the upper step while the chip's middle is over it.
            chip.translate([x.clamp(-step, step), 2.0 + size * 0.2, z.clamp(-step, step)]);
            chips.append(&chip);
        }
        parts.add_tinted("chips", chips, MaterialKind::Stone, DRESSED.scaled(0.95));
    } else {
        let echinus = [[radius * 0.98, capital], [radius * 1.25, capital + 0.6], [radius * 1.5, capital + 1.0]];
        let mut cap = lathe(&echinus, &LatheOptions { facets: flutes, shading: Shading::SMOOTH, cap_start: false, cap_end: false, phase: 0.0 });
        let abacus_half = (radius * 1.6).min(half);
        let mut abacus = cuboid([abacus_half * 2.0, top - capital - 1.0, abacus_half * 2.0]);
        abacus.translate([0.0, (capital + 1.0 + top) * 0.5, 0.0]);
        cap.append(&abacus);
        parts.add_tinted("capital", cap, MaterialKind::Stone, DRESSED);
    }
    parts.finish()
}

/// Dyes a blanket may be, dull as a delver's gear is.
pub(crate) const BLANKET_DYES: [u32; 6] = [0x9a3d33, 0x3d6b8f, 0x4f7f45, 0xb58a35, 0x7a4a7a, 0x8a5a3a];

/// A bedroll laid out flat, human length: a canvas mat about 17 units long and 6 wide, a
/// blanket over the foot two thirds of it with a ruck, and a small rolled pillow across the
/// head. It is scaled by its seed from a short folk's length to a tall one's (`s` 0.85 to 1,
/// so it never reaches past [`Scenery::Bedroll`]'s floor), and lies along item X, head toward
/// -X, under 1.2 units high.
pub fn bedroll(seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0xbed0_11ed);
    let dye = Rgb::hex(*rng.pick(&BLANKET_DYES)).scaled(rng.range_f32(0.9, 1.1));
    let s = rng.range_f32(0.85, 1.0);
    let (len, wide) = (17.0 * s, 6.0 * s);
    let mut mat = cuboid([len, 0.3, wide]);
    mat.translate([0.0, 0.15, 0.0]);
    // The blanket covers the foot two thirds, its head edge folded back in a band.
    let cover = len * 0.66;
    let foot = len * 0.5;
    let mut blanket = cuboid([cover, 0.45, wide * 0.94]);
    blanket.rotate_y(rng.range_f32(-0.03, 0.03)).translate([foot - cover * 0.5 - 0.1 * s, 0.52, 0.0]);
    let mut fold = cuboid([1.3 * s, 0.55, wide * 0.94]);
    fold.rotate_y(rng.range_f32(-0.03, 0.03)).translate([foot - cover + 0.55 * s, 0.57, 0.0]);
    // A ruck across the blanket.
    let mut ruck = cuboid([2.6 * s, 0.25, 1.0 * s]);
    ruck.rotate_y(rng.range_f32(-0.5, 0.5)).translate([rng.range_f32(0.0, 3.5) * s, 0.9, rng.range_f32(-1.0, 1.0) * s]);
    blanket.append(&ruck);
    // The pillow lies across the head of the mat.
    let r = 0.45 * s;
    let (mut pillow, mut ends) = rolled_sheet([0.0, 0.0, 0.0], r, 3.6 * s, 0.015, 3.0);
    for m in [&mut pillow, &mut ends] {
        m.rotate_y(PI * 0.5).translate([-foot + 1.6 * s, 0.3 + r, 0.0]);
    }
    let mut parts = Parts::new();
    parts.add_tinted("mat", mat, MaterialKind::Cloth, Rgb::hex(0x6e6248).scaled(rng.range_f32(0.85, 1.0)));
    parts.add_tinted("blanket", blanket, MaterialKind::Cloth, dye);
    parts.add_tinted("blanket_fold", fold, MaterialKind::Cloth, dye.scaled(0.8));
    parts.add_tinted("pillow", pillow, MaterialKind::Cloth, Rgb::hex(0xb8ab88));
    parts.add_tinted("pillow_end", ends, MaterialKind::Cloth, Rgb::hex(0x8f8466));
    parts.finish()
}

/// How far a mesh reaches from the origin across the floor (XZ).
pub fn floor_reach(mesh: &ItemMesh) -> f32 {
    mesh.parts.iter().flat_map(|p| &p.positions).map(|p: &V3| p[0].hypot(p[2])).fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CEILING: f32 = 30.0;

    #[test]
    fn scenery_is_deterministic_and_drawable() {
        for kind in Scenery::ALL {
            for seed in [0, 1, 1207, u32::MAX] {
                let mesh = kind.build(seed, CEILING);
                assert_eq!(mesh, kind.build(seed, CEILING), "{kind:?} {seed}");
                assert!(!mesh.parts.is_empty());
                for part in &mesh.parts {
                    part.validate().unwrap_or_else(|e| panic!("{kind:?} {seed}: {e}"));
                }
            }
            assert_ne!(kind.build(1, CEILING), kind.build(2, CEILING), "{kind:?}");
        }
    }

    #[test]
    fn scenery_rests_on_the_floor_within_its_radius() {
        for kind in Scenery::ALL {
            for seed in 0..64 {
                let mesh = kind.build(seed, CEILING);
                let (lo, hi) = mesh.bounds().unwrap();
                assert!(lo[1].abs() < 0.05, "{kind:?} {seed}: lowest point at {}", lo[1]);
                let reach = floor_reach(&mesh);
                assert!(reach <= kind.radius(), "{kind:?} {seed}: reaches {reach} > {}", kind.radius());
                match kind {
                    Scenery::Rubble => assert!(hi[1] < 4.5, "{seed}: a heap {} high", hi[1]),
                    Scenery::Bones => assert!(hi[1] < 2.0, "{seed}: bones {} high", hi[1]),
                    Scenery::Pillar => assert!(hi[1] > 6.0 && hi[1] < CEILING + 2.0, "{seed}: a pillar {} high", hi[1]),
                    Scenery::Bedroll => assert!(hi[1] < 1.3, "{seed}: a bedroll {} high", hi[1]),
                    Scenery::Bunk => assert!(hi[1] > 5.0 && hi[1] < 16.0, "{seed}: a bunk {} high", hi[1]),
                    Scenery::Throne => assert!(hi[1] > 20.0 && hi[1] < 26.5, "{seed}: a throne {} high", hi[1]),
                    Scenery::Altar => assert!(hi[1] > 10.0 && hi[1] < 14.0, "{seed}: an altar {} high", hi[1]),
                    Scenery::Bookshelf => assert!(hi[1] > 20.0 && hi[1] < 24.0, "{seed}: a bookshelf {} high", hi[1]),
                    Scenery::Table => assert!(hi[1] > 8.0 && hi[1] < 11.0, "{seed}: a table {} high", hi[1]),
                    Scenery::WeaponRack => assert!(hi[1] > 14.0 && hi[1] < 27.0, "{seed}: a rack {} high", hi[1]),
                }
            }
        }
    }

    #[test]
    fn furniture_stays_in_its_footprint_with_its_back_to_minus_z() {
        for kind in Scenery::FURNITURE {
            let (hw, hd) = kind.footprint().unwrap();
            assert!(hw.hypot(hd) <= kind.radius(), "{kind:?}: corner {} past its radius", hw.hypot(hd));
            for seed in 0..64 {
                let mesh = kind.build(seed, CEILING);
                let (lo, hi) = mesh.bounds().unwrap();
                assert!(lo[0] >= -hw - 1e-3 && hi[0] <= hw + 1e-3, "{kind:?} {seed}: x {lo:?} {hi:?}");
                let culprit = mesh.parts.iter().find(|p| p.positions.iter().any(|v| v[2] < -hd - 1e-3 || v[2] > hd + 1e-3)).map(|p| p.name.clone());
                assert!(lo[2] >= -hd - 1e-3 && hi[2] <= hd + 1e-3, "{kind:?} {seed}: z {lo:?} {hi:?} in {culprit:?}");
                if !matches!(kind, Scenery::Table | Scenery::Bunk) {
                    assert!(lo[2] < -hd + 0.3, "{kind:?} {seed}: back at {} not against -Z", lo[2]);
                }
                assert!(hi[1] < CEILING, "{kind:?} {seed}: {} reaches the ceiling", hi[1]);
            }
        }
    }

    #[test]
    fn furniture_varies_with_its_seed() {
        for kind in Scenery::FURNITURE {
            let distinct: std::collections::HashSet<usize> = (0..32).map(|s| kind.build(s, CEILING).parts.iter().map(|p| p.positions.len()).sum()).collect();
            assert!(distinct.len() > 8, "{kind:?}: {} distinct builds in 32", distinct.len());
        }
        let tall = (0..64).filter(|&s| bunk_height(s) > 12.0).count();
        assert!((10..54).contains(&tall), "{tall} of 64 bunks are two-tier");
    }

    fn bunk_height(seed: u32) -> f32 {
        Scenery::Bunk.build(seed, CEILING).bounds().unwrap().1[1]
    }

    #[test]
    fn some_pillars_hold_up_the_ceiling_and_some_are_broken() {
        let heights: Vec<f32> = (0..32).map(|s| pillar(s, CEILING).bounds().unwrap().1[1]).collect();
        assert!(heights.iter().any(|&h| h > CEILING), "{heights:?}");
        assert!(heights.iter().any(|&h| h < 20.0), "{heights:?}");
    }

    #[test]
    fn a_skull_has_its_sockets() {
        let mesh = bones(1207);
        assert!(mesh.parts.iter().any(|p| p.name == "sockets" && !p.indices.is_empty()));
    }
}
