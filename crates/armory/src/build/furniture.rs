//! Room furniture, built as scenery ([`super::scenery::Scenery`]): bunks, thrones, altars,
//! bookshelves, tables and weapon racks, the things that say what a room is for.
//!
//! Every builder is a pure function of a seed, in item space like the rest of the scenery:
//! +Y up from the floor it stands on, centred on the origin across the floor, its back along
//! -Z and its front facing +Z, so a piece set with its back to a wall needs no turning. The
//! floor each covers is a rectangle ([`super::scenery::Scenery::footprint`]); the radius every
//! seed stays inside is the distance to that rectangle's corner.
//!
//! Timber and stone are chamfered and rounded the way [`soft_box`] does it, and cloth is a
//! thick soft slab rather than a flat card, so the lantern finds an edge on everything.
//! Colours vary per seed from short lists of the dull dyes and woods a delver finds.

use std::f32::consts::{FRAC_PI_2, PI};

use procgen_rng::Mulberry32;

use crate::build::fitting::rounded_rect;
use crate::build::parts::Parts;
use crate::build::scenery::BLANKET_DYES;
use crate::geom::ItemMesh;
use crate::geom::{LatheOptions, MeshBuilder, Shading, SlabOptions, V3, bevelled_slab, cuboid, cylinder, icosphere, lathe, spike};
use crate::material::{MaterialKind, Rgb};

/// Timbers: pale pine, dark oak, honey ash, near-black bog oak.
const TIMBERS: [u32; 4] = [0x7c5a38, 0x5d4129, 0x8e7048, 0x4a3524];
/// Straw in a tick, gone grey with use.
const STRAW: Rgb = Rgb::hex(0xb39c5e);
/// Dressed stones: granite, marble, basalt, sandstone.
const STONES: [u32; 4] = [0x8d8b82, 0xb9b5aa, 0x484850, 0xa89475];
/// Pale beeswax.
const WAX: Rgb = Rgb::hex(0xe4d8b4);
/// Page edges.
const PAPER: Rgb = Rgb::hex(0xd9cdaa);
/// Warm brass for dishes, cups and bowls.
const BRASS: Rgb = Rgb::hex(0xa88a46);
/// Gilt for bands and bosses.
const GILT: Rgb = Rgb::hex(0xc9a24a);

/// A box from `lo` to `hi` with its edges softened: the faces that look along Z get a bevel
/// of `bevel` round their rims, and the outline corners (seen from the front) are rounded by
/// `round`. A bevel under 0.02 gives a plain box.
pub fn soft_box(lo: V3, hi: V3, round: f32, bevel: f32) -> MeshBuilder {
    let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let mid = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
    let least = size[0].min(size[1]).min(size[2]);
    let bevel = bevel.min(least * 0.4);
    let mut mesh = if bevel < 0.02 {
        cuboid(size)
    } else {
        let (hw, hh) = (size[0] * 0.5, size[1] * 0.5);
        let outline = if round > bevel * 1.2 {
            rounded_rect(0.0, 0.0, size[0], size[1], round.min(hw.min(hh) - 0.01))
        } else {
            vec![[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]]
        };
        bevelled_slab(&outline, &[], &SlabOptions { depth: size[2], bevel, bevel_segments: if bevel > 0.15 { 2 } else { 1 }, crease: 40f32.to_radians() })
    };
    mesh.translate(mid);
    mesh
}

/// `mesh` moved by `by`.
fn moved(mut mesh: MeshBuilder, by: V3) -> MeshBuilder {
    mesh.translate(by);
    mesh
}

/// A vessel or figure turned on a lathe from `profile` (radius, height), `facets` round.
fn turned(profile: &[[f32; 2]], facets: usize) -> MeshBuilder {
    lathe(profile, &LatheOptions { facets, shading: Shading::SMOOTH, cap_start: false, cap_end: false, phase: 0.0 })
}

fn timber(rng: &mut Mulberry32) -> Rgb {
    Rgb::hex(*rng.pick(&TIMBERS)).scaled(rng.range_f32(0.92, 1.08))
}

fn stone(rng: &mut Mulberry32) -> Rgb {
    Rgb::hex(*rng.pick(&STONES)).scaled(rng.range_f32(0.94, 1.06))
}

fn dye(rng: &mut Mulberry32) -> Rgb {
    Rgb::hex(*rng.pick(&BLANKET_DYES)).scaled(rng.range_f32(0.75, 0.95))
}

/// A bunk: a timber bed frame on four posts, human length (19 by 8 units), with a straw tick,
/// a blanket with a folded head edge, a ruck and sometimes a patch, and a pillow at the head
/// (toward -X). Its long side lies along X with its back to -Z. Some are two-tier: the posts
/// run up past a second bed 7.6 units over the first, to about 15 units in all.
pub fn bunk(seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0xb0b0_5e11);
    let wood = timber(&mut rng);
    let blanket_dye = dye(&mut rng);
    let tiers = if rng.chance(0.4) { 2 } else { 1 };
    let (hx, hz, post, pitch) = (9.5f32, 4.0f32, 0.7f32, 7.6f32);
    let mut frame = MeshBuilder::new();
    let (mut tick, mut blanket, mut folds, mut patch, mut pillows) = (MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new());
    let bed_y = |k: u32| 2.6 + k as f32 * pitch;
    let top_mattress = bed_y(tiers - 1) + 2.3;
    for sx in [-1.0f32, 1.0] {
        // The head posts stand taller than the foot's.
        let tall = if sx < 0.0 { top_mattress + 2.2 } else { top_mattress + 0.6 };
        for sz in [-1.0f32, 1.0] {
            let (x0, z0) = (sx * (hx - post * 0.5), sz * (hz - post * 0.5));
            frame.append(&soft_box([x0 - post * 0.5, 0.0, z0 - post * 0.5], [x0 + post * 0.5, tall, z0 + post * 0.5], 0.0, 0.1));
            frame.append(&moved(icosphere(0.34, 1), [x0, tall, z0]));
        }
    }
    for k in 0..tiers {
        let y = bed_y(k);
        for sz in [-1.0f32, 1.0] {
            let z = sz * (hz - 0.35);
            frame.append(&soft_box([-hx + 0.2, y, z - 0.25], [hx - 0.2, y + 1.0, z + 0.25], 0.0, 0.1));
        }
        for sx in [-1.0f32, 1.0] {
            let x = sx * (hx - 0.35);
            frame.append(&soft_box([x - 0.25, y, -hz + 0.7], [x + 0.25, y + 1.0, hz - 0.7], 0.0, 0.1));
        }
        // Slats under the tick.
        for i in 0..5 {
            let x = -7.2 + i as f32 * 3.6;
            frame.append(&soft_box([x - 0.5, y + 0.55, -hz + 0.7], [x + 0.5, y + 0.85, hz - 0.7], 0.0, 0.05));
        }
        // The head board rises above the mattress, the foot board a little.
        frame.append(&soft_box([-hx + 0.1, y + 1.0, -hz + 0.7], [-hx + 0.6, y + 3.8, hz - 0.7], 0.15, 0.1));
        frame.append(&soft_box([hx - 0.6, y + 1.0, -hz + 0.7], [hx - 0.1, y + 2.6, hz - 0.7], 0.15, 0.1));

        let mt = y + 2.3;
        tick.append(&soft_box([-hx + 0.9, y + 0.9, -3.0], [hx - 0.9, mt, 3.0], 0.5, 0.3));
        if k + 1 == tiers || rng.chance(0.75) {
            let x0 = rng.range_f32(-3.0, 0.5);
            blanket.append(&soft_box([x0, mt - 0.05, -3.2], [hx - 0.75, mt + 0.45, 3.2], 0.25, 0.2));
            for sz in [-1.0f32, 1.0] {
                let z = sz * 3.15;
                blanket.append(&soft_box([x0 + 0.2, mt - 1.1, z - 0.17], [hx - 0.95, mt + 0.2, z + 0.17], 0.1, 0.08));
            }
            folds.append(&soft_box([x0, mt + 0.4, -3.1], [x0 + 1.5, mt + 0.66, 3.1], 0.12, 0.12));
            let rx = rng.range_f32(x0 + 2.5, hx - 4.0);
            let mut ruck = soft_box([-1.3, 0.0, -0.5], [1.3, 0.32, 0.5], 0.15, 0.14);
            ruck.rotate_y(rng.range_f32(-0.5, 0.5)).translate([rx, mt + 0.4, rng.range_f32(-1.5, 1.5)]);
            blanket.append(&ruck);
            if rng.chance(0.5) {
                let (px, pz) = (rng.range_f32(x0 + 2.0, hx - 3.5), rng.range_f32(-1.8, 1.8));
                patch.append(&soft_box([px - 0.7, mt + 0.44, pz - 0.6], [px + 0.7, mt + 0.5, pz + 0.6], 0.0, 0.02));
            }
        }
        if rng.chance(0.85) {
            let pz = rng.range_f32(-0.5, 0.5);
            pillows.append(&soft_box([-hx + 1.0, mt - 0.1, pz - 1.8], [-hx + 4.1, mt + 0.95, pz + 1.8], 0.45, 0.4));
        }
    }
    let mut parts = Parts::new();
    parts.add_tinted("frame", frame, MaterialKind::Wood, wood);
    parts.add_tinted("tick", tick, MaterialKind::Cloth, STRAW.scaled(rng.range_f32(0.8, 1.0)));
    if !blanket.is_empty() {
        parts.add_tinted("blanket", blanket, MaterialKind::Cloth, blanket_dye);
        parts.add_tinted("blanket_fold", folds, MaterialKind::Cloth, blanket_dye.scaled(1.25));
    }
    if !patch.is_empty() {
        parts.add_tinted("patch", patch, MaterialKind::Cloth, dye(&mut rng));
    }
    if !pillows.is_empty() {
        parts.add_tinted("pillow", pillows, MaterialKind::Cloth, Rgb::hex(0xb8ab88).scaled(rng.range_f32(0.85, 1.0)));
    }
    parts.finish()
}

/// How the back of a throne is shaped.
#[derive(Clone, Copy)]
enum BackShape {
    Flat,
    Pointed,
    Fanned,
}

/// A throne on a low stepped dais: a heavy chair of stone or carved timber with a tall back
/// (20.4 to 24.2 units over the floor, 26 with its finial), armrests, sometimes a cushion, a
/// gilt boss on the back and a runner down the dais steps. The dais is 14 by 11 units, its
/// back at -Z, the chair a little forward of it.
pub fn throne(seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0x7410_0e5e);
    let in_stone = rng.chance(0.5);
    let body_colour = if in_stone { stone(&mut rng) } else { timber(&mut rng) };
    let body_material = if in_stone { MaterialKind::Stone } else { MaterialKind::Wood };
    let shape = *rng.pick(&[BackShape::Flat, BackShape::Pointed, BackShape::Fanned]);
    let height = rng.range_f32(20.4, 24.2);
    let finial = rng.int(0, 2);
    let step_colour = Rgb::hex(*rng.pick(&STONES)).scaled(0.9);

    let mut dais = soft_box([-7.0, 0.0, -5.5], [7.0, 0.9, 5.5], 0.0, 0.18);
    dais.append(&soft_box([-5.8, 0.9, -5.0], [5.8, 1.8, 4.0], 0.0, 0.16));

    let mut chair = soft_box([-2.9, 1.8, -2.9], [2.9, 4.2, 1.6], 0.0, 0.15);
    let inset = soft_box([-2.2, 2.2, 1.58], [2.2, 3.8, 1.75], 0.1, 0.05);
    // The back: an outline extruded and chamfered, with a raised panel on its face.
    let y0 = 4.2;
    let half = 3.4;
    let mut outline: Vec<[f32; 2]> = vec![[-half, y0], [half, y0]];
    match shape {
        BackShape::Flat => outline.extend([[half, height], [-half, height]]),
        BackShape::Pointed => outline.extend([[half, height - 5.0], [0.0, height], [-half, height - 5.0]]),
        BackShape::Fanned => {
            // Flares out from the shoulders into a scallop.
            let y1 = height - 4.8;
            outline.extend([[half, y1 - 2.0], [4.4, y1 - 0.5]]);
            for i in 0..=20 {
                let a = PI * i as f32 / 20.0;
                outline.push([4.9 * a.cos(), y1 + 4.8 * a.sin()]);
            }
            outline.extend([[-4.4, y1 - 0.5], [-half, y1 - 2.0]]);
        }
    }
    let mut back = bevelled_slab(&outline, &[], &SlabOptions { depth: 0.9, bevel: 0.16, bevel_segments: 1, crease: 40f32.to_radians() });
    back.translate([0.0, 0.0, -3.0]);
    chair.append(&back);
    let mid = (y0 + height) * 0.5;
    let face: Vec<[f32; 2]> = outline.iter().map(|&[x, y]| [x * 0.72, mid + (y - mid) * 0.8]).collect();
    let mut panel = bevelled_slab(&face, &[], &SlabOptions { depth: 0.3, bevel: 0.08, bevel_segments: 1, crease: 40f32.to_radians() });
    panel.translate([0.0, 0.0, -2.55]);
    // Armrests, each on a front post and ending in a knob.
    for (x0, x1) in [(-4.0f32, -2.9f32), (2.9, 4.0)] {
        chair.append(&soft_box([x0, 6.5, -2.9], [x1, 7.1, 1.8], 0.0, 0.12));
        chair.append(&soft_box([x0 + 0.1, 4.2, 0.9], [x1 - 0.1, 6.5, 1.7], 0.0, 0.1));
        chair.append(&moved(icosphere(0.55, 2), [(x0 + x1) * 0.5, 6.8, 1.85]));
    }
    let mut finials = MeshBuilder::new();
    let tops: &[[f32; 2]] = match shape {
        BackShape::Flat => &[[-3.0, 0.0], [3.0, 0.0]],
        BackShape::Pointed | BackShape::Fanned => &[[0.0, 0.0]],
    };
    for &[x, _] in tops {
        let base = [x, height - 0.05, -3.0];
        match finial {
            1 => {
                finials.append(&moved(cylinder(0.24, 0.5, 8, Shading::SMOOTH), base));
                finials.append(&moved(icosphere(0.55, 2), [base[0], base[1] + 1.0, base[2]]));
            }
            2 => {
                finials.append(&moved(spike(0.4, 1.8, 8, 0.0, Shading::SMOOTH), base));
            }
            _ => {}
        }
    }
    let boss = moved(icosphere(0.5, 2), [0.0, mid, -2.3]);
    let cushion = rng.chance(0.65).then(|| soft_box([-2.7, 4.2, -2.6], [2.7, 5.0, 1.4], 0.35, 0.3));
    let carpet = rng.chance(0.55).then(|| soft_box([-1.8, 0.9, 4.0], [1.8, 1.0, 5.4], 0.0, 0.03));

    let cloth = dye(&mut rng);
    let mut parts = Parts::new();
    parts.add_tinted("dais", dais, MaterialKind::Stone, step_colour);
    parts.add_tinted("throne", chair, body_material, body_colour);
    parts.add_tinted("throne_panel", inset, body_material, body_colour.scaled(0.78));
    parts.add_tinted("back_panel", panel, body_material, body_colour.scaled(0.86));
    if !finials.is_empty() {
        parts.add_tinted("finials", finials, body_material, body_colour.scaled(1.1));
    }
    parts.add_tinted("boss", boss, MaterialKind::Gold, GILT);
    if let Some(c) = cushion {
        parts.add_tinted("cushion", c, MaterialKind::Cloth, cloth);
    }
    if let Some(c) = carpet {
        parts.add_tinted("carpet", c, MaterialKind::Cloth, cloth.scaled(0.9));
    }
    parts.finish()
}

/// What stands on an altar's top.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Centrepiece {
    Bowl,
    Idol,
    Both,
}

/// An altar: a stone block (or two stone piers under a slab) on a plinth, 10 units high and
/// 12 wide by 6 deep over its step, with a cloth runner hanging over the ends, two to four
/// candles in brass dishes, and a bowl, an idol or both. Its back is at -Z.
pub fn altar(seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0xa17a_5e11);
    let rock = stone(&mut rng);
    let piers = rng.chance(0.4);
    let mut body = soft_box([-6.4, 0.0, -3.4], [6.4, 0.9, 3.4], 0.0, 0.2);
    let mut panels = MeshBuilder::new();
    if piers {
        for (a, b) in [(-5.6f32, -3.6f32), (3.6, 5.6)] {
            body.append(&soft_box([a, 0.9, -2.4], [b, 8.1, 2.4], 0.0, 0.14));
        }
        body.append(&soft_box([-4.0, 5.9, -2.0], [4.0, 8.1, 2.0], 0.0, 0.1));
    } else {
        body.append(&soft_box([-5.4, 0.9, -2.4], [5.4, 8.1, 2.4], 0.0, 0.18));
        // Three recessed panels on the front.
        for i in 0..3 {
            let x = -3.4 + i as f32 * 3.4;
            panels.append(&soft_box([x - 1.3, 2.0, 2.38], [x + 1.3, 7.0, 2.55], 0.12, 0.05));
        }
    }
    body.append(&soft_box([-6.2, 8.1, -3.2], [6.2, 10.0, 3.2], 0.0, 0.25));

    let top = 10.0;
    let runner_dye = dye(&mut rng);
    let mut runner = soft_box([-5.5, top, -1.25], [5.5, top + 0.14, 1.25], 0.0, 0.05);
    for (a, b) in [(-6.0f32, -5.5f32), (5.5, 6.0)] {
        runner.append(&soft_box([a, top - 3.2, -1.25], [b, top + 0.14, 1.25], 0.0, 0.05));
    }
    let (mut dishes, mut wax) = (MeshBuilder::new(), MeshBuilder::new());
    let candles = rng.int(2, 4) as usize;
    let slots: &[f32] = match candles {
        2 => &[-4.6, 4.6],
        3 => &[-4.6, 2.4, 4.6],
        _ => &[-4.6, -2.4, 2.4, 4.6],
    };
    for &slot in slots {
        let x = slot + rng.range_f32(-0.4, 0.4);
        let z = rng.range_f32(-1.9, 0.0);
        let h = rng.range_f32(0.9, 2.2);
        dishes.append(&moved(turned(&[[0.0, 0.0], [0.55, 0.0], [0.6, 0.12], [0.3, 0.2], [0.0, 0.2]], 12), [x, top + 0.14, z]));
        wax.append(&moved(cylinder(0.22, h, 10, Shading::SMOOTH), [x, top + 0.34, z]));
    }
    let piece = *rng.pick(&[Centrepiece::Bowl, Centrepiece::Idol, Centrepiece::Both]);
    let (bowl_x, idol_x) = if piece == Centrepiece::Both { (-1.4, 1.3) } else { (0.0, 0.0) };
    let (mut vessel, mut offering, mut idol) = (MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new());
    if piece != Centrepiece::Idol {
        let p = [[0.0, 0.0], [0.7, 0.0], [1.35, 0.7], [1.5, 0.95], [1.3, 0.95], [1.2, 0.8], [0.6, 0.2], [0.0, 0.2]];
        vessel.append(&moved(turned(&p, 20), [bowl_x, top + 0.14, -0.3]));
        offering.append(&moved(cylinder(1.0, 0.03, 16, Shading::SMOOTH), [bowl_x, top + 0.14 + 0.7, -0.3]));
    }
    if piece != Centrepiece::Bowl {
        let p = [[0.0, 0.0], [1.1, 0.0], [1.1, 0.25], [0.7, 0.35], [0.78, 1.2], [0.98, 1.9], [0.6, 2.3], [0.3, 2.45], [0.55, 2.75], [0.5, 3.1], [0.25, 3.3], [0.0, 3.35]];
        idol.append(&moved(turned(&p, 14), [idol_x, top + 0.14, -0.3]));
    }
    let idol_material = *rng.pick(&[MaterialKind::Stone, MaterialKind::Bone, MaterialKind::Gold, MaterialKind::Wood]);
    let idol_tint = match idol_material {
        MaterialKind::Gold => GILT,
        MaterialKind::Bone => Rgb::hex(0xd6ccb0),
        MaterialKind::Wood => timber(&mut rng),
        _ => rock.scaled(0.8),
    };

    let mut parts = Parts::new();
    parts.add_tinted("altar", body, MaterialKind::Stone, rock);
    if !panels.is_empty() {
        parts.add_tinted("altar_panels", panels, MaterialKind::Stone, rock.scaled(0.82));
    }
    parts.add_tinted("runner", runner, MaterialKind::Cloth, runner_dye);
    parts.add_tinted("dishes", dishes, MaterialKind::Brass, BRASS);
    parts.add_tinted("candles", wax, MaterialKind::Bone, WAX);
    if !vessel.is_empty() {
        parts.add_tinted("bowl", vessel, MaterialKind::Brass, BRASS);
        parts.add_tinted("offering", offering, MaterialKind::Paint, Rgb::hex(0x5a1814));
    }
    if !idol.is_empty() {
        parts.add_tinted("idol", idol, idol_material, idol_tint);
    }
    parts.finish()
}

/// Leathers for the books on a shelf: oxblood, blue, green, brown, tan and plum.
const SPINES: [u32; 6] = [0x6b2d2a, 0x2f4a5c, 0x3e5a3a, 0x5a4028, 0x7d6a3f, 0x4a3350];
const SPINE_PARTS: [&str; 6] = ["books_oxblood", "books_blue", "books_green", "books_brown", "books_tan", "books_plum"];

/// A tall timber bookcase, 16 wide and 5 deep, 20.4 to 23.8 high, on a plinth under a stepped
/// top, with three to five shelves of books: spines facing out in varied heights and widths,
/// runs of one set, gaps, a book leaning on its neighbour, and some lying flat in stacks. Its
/// back is against -Z.
pub fn bookshelf(seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0xb00c_5be1);
    let wood = timber(&mut rng);
    let (hx, hz) = (8.0f32, 2.5f32);
    let h = rng.range_f32(20.4, 23.8);
    let (side, board, base, crown, back) = (0.7f32, 0.5f32, 1.1f32, 0.8f32, 0.3f32);
    let mut case = soft_box([-hx, 0.0, -hz], [-hx + side, h - crown, hz], 0.0, 0.14);
    case.append(&soft_box([hx - side, 0.0, -hz], [hx, h - crown, hz], 0.0, 0.14));
    case.append(&soft_box([-hx, 0.0, -hz + back], [hx, base, hz], 0.0, 0.16));
    case.append(&soft_box([-hx, h - crown, -hz], [hx, h - 0.25, hz], 0.0, 0.16));
    case.append(&soft_box([-hx + 0.5, h - 0.25, -hz + 0.3], [hx - 0.5, h, hz - 0.4], 0.0, 0.1));
    let backing = soft_box([-hx + side - 0.1, base, -hz], [hx - side + 0.1, h - crown, -hz + back], 0.0, 0.0);

    let bays = rng.int(3, 5) as usize;
    let bay_h = (h - base - crown - (bays - 1) as f32 * board) / bays as f32;
    let (x_lo, x_hi) = (-hx + side, hx - side);
    let mut spines: Vec<MeshBuilder> = (0..SPINES.len()).map(|_| MeshBuilder::new()).collect();
    let (mut pages, mut bands) = (MeshBuilder::new(), MeshBuilder::new());
    let mut set = rng.int(0, 5) as usize;
    for bay in 0..bays {
        let y0 = base + bay as f32 * (bay_h + board);
        if bay > 0 {
            case.append(&soft_box([x_lo - 0.05, y0 - board, -hz + back], [x_hi + 0.05, y0, hz - 0.15], 0.0, 0.1));
        }
        let mut start = x_lo + 0.1;
        let mut end = x_hi - 0.1;
        let sparse = rng.chance(0.2);
        if sparse {
            end = start + (end - start) * rng.range_f32(0.3, 0.6);
        }
        // Books lying flat in a stack at the far end of the run (past it, when sparse).
        if rng.chance(0.3) {
            let w = rng.range_f32(3.2, 4.4);
            let x = if sparse { end + 0.2 } else { end - w };
            let mut y = y0;
            for _ in 0..rng.int(2, 3) {
                let t = rng.range_f32(0.45, 0.7);
                if y + t > y0 + bay_h - 0.4 {
                    break;
                }
                let cover = rng.int(0, 5) as usize;
                let x = x + rng.range_f32(-0.12, 0.12);
                let (zf, d) = (1.7 - rng.range_f32(0.0, 0.3), 3.0);
                spines[cover].append(&soft_box([x, y, zf - d], [x + w, y + 0.1, zf], 0.0, 0.03));
                spines[cover].append(&soft_box([x, y + t - 0.1, zf - d], [x + w, y + t, zf], 0.0, 0.03));
                spines[cover].append(&soft_box([x, y, zf - 0.12], [x + w, y + t, zf], 0.0, 0.04));
                pages.append(&soft_box([x + 0.08, y + 0.1, zf - d + 0.05], [x + w - 0.08, y + t - 0.1, zf - 0.12], 0.0, 0.0));
                y += t;
            }
            if sparse {
                // The stack stands past the books, so they end where it begins.
                end = end.min(x - 0.2).max(start);
            } else {
                end -= w + 0.25;
            }
        }
        if sparse {
            start = start.min(end);
        }
        let mut x = start;
        let mut last_h = 0.0f32;
        loop {
            if rng.chance(0.07) {
                x += rng.range_f32(0.4, 1.4);
            }
            if rng.chance(0.18) {
                set = rng.int(0, 5) as usize;
            }
            let w = rng.range_f32(0.35, 0.85);
            if x + w > end {
                break;
            }
            let hb = rng.range_f32(0.6, 0.97) * (bay_h - 0.35);
            let zf = 1.75 - rng.range_f32(0.0, 0.35);
            let d = rng.range_f32(2.6, 3.4);
            spines[set].append(&soft_box([x, y0, zf - d], [x + w, y0 + hb, zf], 0.0, 0.05));
            if w > 0.5 && rng.chance(0.5) {
                bands.append(&soft_box([x - 0.01, y0 + hb * 0.78, zf - 0.01], [x + w + 0.01, y0 + hb * 0.78 + 0.12, zf + 0.04], 0.0, 0.0));
                if rng.chance(0.5) {
                    bands.append(&soft_box([x - 0.01, y0 + hb * 0.2, zf - 0.01], [x + w + 0.01, y0 + hb * 0.2 + 0.1, zf + 0.04], 0.0, 0.0));
                }
            }
            last_h = hb;
            x += w + 0.02;
        }
        // One leaning on the last of the run, when there is room for it.
        let (w, theta) = (rng.range_f32(0.5, 0.8), rng.range_f32(0.2, 0.4));
        let hb = last_h * 0.9;
        let gap = hb * theta.sin();
        let lean = rng.chance(0.6);
        if lean && hb > 1.5 && x + gap + w + 0.1 < end + 0.15 {
            let (zf, d) = (1.6, 2.8);
            let mut book = soft_box([0.0, 0.0, zf - d], [w, hb, zf], 0.0, 0.05);
            book.rotate_z(theta).translate([x + gap, y0, 0.0]);
            spines[set].append(&book);
        }
    }
    let mut parts = Parts::new();
    parts.add_tinted("case", case, MaterialKind::Wood, wood);
    parts.add_tinted("case_back", backing, MaterialKind::Wood, wood.scaled(0.7));
    for (i, mesh) in spines.into_iter().enumerate() {
        if !mesh.is_empty() {
            parts.add_tinted(SPINE_PARTS[i], mesh, MaterialKind::Leather, Rgb::hex(SPINES[i]).scaled(rng.range_f32(0.9, 1.1)));
        }
    }
    if !pages.is_empty() {
        parts.add_tinted("pages", pages, MaterialKind::Cloth, PAPER);
    }
    if !bands.is_empty() {
        parts.add_tinted("spine_bands", bands, MaterialKind::Gold, Rgb::hex(0xb98f3c));
    }
    parts.finish()
}

/// A table, 8 units high and 14 by 7: planks on two trestle ends or on four legs, a bench in
/// front, behind or both (the footprint then reaches 5.5 either side of the middle), and
/// clutter from the seed: cups, plates, a jug, a loaf, candles, sometimes a cloth.
pub fn table(seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0x7ab1_e5e1);
    let wood = timber(&mut rng);
    let trestle = rng.chance(0.55);
    let mut frame = MeshBuilder::new();
    let mut top = MeshBuilder::new();
    let tz = 3.5f32;
    // Four planks, with hair gaps.
    let planks = 4;
    for i in 0..planks {
        let z0 = -tz + i as f32 * (2.0 * tz / planks as f32);
        let z1 = z0 + 2.0 * tz / planks as f32 - 0.05;
        top.append(&soft_box([-7.0, 7.4, z0], [7.0, 8.0, z1], 0.0, 0.1));
    }
    if trestle {
        let mut outline: Vec<[f32; 2]> = vec![[-3.0, 0.0], [-1.9, 0.0]];
        for i in 0..=12 {
            let a = PI - PI * i as f32 / 12.0;
            outline.push([1.9 * a.cos(), 4.2 + 1.9 * a.sin()]);
        }
        outline.extend([[1.9, 0.0], [3.0, 0.0], [2.5, 7.4], [-2.5, 7.4]]);
        for s in [-1.0f32, 1.0] {
            let mut end = bevelled_slab(&outline, &[], &SlabOptions { depth: 0.7, bevel: 0.18, bevel_segments: 1, crease: 40f32.to_radians() });
            end.rotate_y(FRAC_PI_2).translate([s * 4.6, 0.0, 0.0]);
            frame.append(&end);
        }
        frame.append(&soft_box([-4.6, 2.0, -0.3], [4.6, 2.6, 0.3], 0.0, 0.1));
    } else {
        for sx in [-1.0f32, 1.0] {
            for sz in [-1.0f32, 1.0] {
                let (x, z) = (sx * 6.2, sz * 2.7);
                frame.append(&soft_box([x - 0.4, 0.0, z - 0.4], [x + 0.4, 7.4, z + 0.4], 0.0, 0.08));
            }
            frame.append(&soft_box([sx * 6.2 - 0.25, 6.2, -2.7], [sx * 6.2 + 0.25, 7.4, 2.7], 0.0, 0.06));
        }
        for sz in [-1.0f32, 1.0] {
            frame.append(&soft_box([-6.2, 6.2, sz * 2.7 - 0.25], [6.2, 7.4, sz * 2.7 + 0.25], 0.0, 0.06));
        }
    }
    // Benches: none, one in front, one behind, or both.
    let benches = match rng.int(0, 5) {
        0 => (false, false),
        1..=3 => (true, false),
        4 => (false, true),
        _ => (true, true),
    };
    for (present, s) in [(benches.0, 1.0f32), (benches.1, -1.0)] {
        if !present {
            continue;
        }
        let z = s * 4.6;
        frame.append(&soft_box([-6.0, 3.9, z - 0.9], [6.0, 4.4, z + 0.9], 0.0, 0.1));
        for e in [-1.0f32, 1.0] {
            frame.append(&soft_box([e * 5.2 - 0.25, 0.0, z - 0.8], [e * 5.2 + 0.25, 3.9, z + 0.8], 0.0, 0.08));
        }
    }
    // Clutter on the planks.
    let ty = 8.0;
    let (mut cups, mut plates, mut candles, mut jugs, mut loaves, mut cloth) = (MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new());
    let cloth_dye = dye(&mut rng);
    let has_cloth = rng.chance(0.3);
    if has_cloth {
        cloth.append(&soft_box([-3.5, ty, -2.4], [3.5, ty + 0.08, 2.4], 0.0, 0.03));
    }
    let mut slots = [-5.0f32, -3.0, -1.0, 1.0, 3.0, 5.0];
    for i in (1..slots.len()).rev() {
        slots.swap(i, rng.int(0, i as u32) as usize);
    }
    let floor = ty + if has_cloth { 0.08 } else { 0.0 };
    let count = rng.int(3, 5) as usize;
    for &slot in slots.iter().take(count) {
        let (x, z) = (slot + rng.range_f32(-0.35, 0.35), rng.range_f32(-1.9, 1.9));
        match rng.int(0, 4) {
            0 => {
                cups.append(&moved(turned(&[[0.0, 0.0], [0.38, 0.0], [0.5, 0.8], [0.44, 0.8], [0.33, 0.1], [0.0, 0.1]], 12), [x, floor, z]));
            }
            1 => {
                plates.append(&moved(turned(&[[0.0, 0.0], [0.7, 0.0], [1.0, 0.18], [0.9, 0.18], [0.62, 0.06], [0.0, 0.06]], 16), [x, floor, z]));
            }
            2 => {
                candles.append(&moved(turned(&[[0.0, 0.0], [0.5, 0.0], [0.55, 0.1], [0.25, 0.18], [0.0, 0.18]], 12), [x, floor, z]));
                candles.append(&moved(cylinder(0.2, rng.range_f32(0.8, 1.6), 10, Shading::SMOOTH), [x, floor + 0.3, z]));
            }
            3 => {
                jugs.append(&moved(turned(&[[0.0, 0.0], [0.55, 0.0], [0.75, 0.6], [0.62, 1.3], [0.38, 1.5], [0.42, 1.8], [0.0, 1.8]], 12), [x, floor, z]));
            }
            _ => {
                let mut loaf = icosphere(1.0, 2);
                loaf.scale([0.95, 0.42, 0.55]).rotate_y(rng.range_f32(0.0, PI));
                loaves.append(&moved(loaf, [x, floor + 0.3, z]));
            }
        }
    }
    let mut parts = Parts::new();
    parts.add_tinted("table", top, MaterialKind::Wood, wood.scaled(1.05));
    parts.add_tinted("table_frame", frame, MaterialKind::Wood, wood.scaled(0.9));
    if !cups.is_empty() {
        parts.add_tinted("cups", cups, MaterialKind::Brass, BRASS);
    }
    if !plates.is_empty() {
        parts.add_tinted("plates", plates, MaterialKind::Silver, Rgb::hex(0x9a9a98));
    }
    if !candles.is_empty() {
        parts.add_tinted("candle", candles, MaterialKind::Bone, WAX);
    }
    if !jugs.is_empty() {
        parts.add_tinted("jug", jugs, MaterialKind::Stone, Rgb::hex(0x9a5b3c));
    }
    if !loaves.is_empty() {
        parts.add_tinted("bread", loaves, MaterialKind::Food, Rgb::hex(0xb98a4e));
    }
    if !cloth.is_empty() {
        parts.add_tinted("cloth", cloth, MaterialKind::Cloth, cloth_dye);
    }
    parts.finish()
}

/// A weapon standing on its butt: the haft and the iron head, as meshes with the butt at the
/// origin and the weapon up the Y axis, an axe's edge facing +Z. `kind` is a spear, an axe, a
/// halberd or a mace.
fn hafted(kind: u32, rng: &mut Mulberry32) -> (MeshBuilder, MeshBuilder) {
    let len = match kind {
        0 => rng.range_f32(20.0, 22.0),
        1 => rng.range_f32(16.0, 19.0),
        2 => rng.range_f32(20.0, 22.0),
        _ => rng.range_f32(14.0, 17.0),
    };
    let mut haft = cylinder(0.23, len, 8, Shading::SMOOTH);
    // A swell at the butt, so the haft does not read as a turned dowel.
    haft.append(&cylinder(0.3, 0.6, 8, Shading::SMOOTH));
    let leaf = |length: f32, wide: f32| {
        let mut leaf = turned(&[[0.0, 0.0], [wide * 0.8, length * 0.28], [wide, length * 0.5], [wide * 0.5, length * 0.78], [0.0, length]], 8);
        leaf.scale([1.0, 1.0, 0.28]);
        leaf.recompute_smooth_normals();
        leaf
    };
    let axe_head = |size: f32| {
        let outline = [[0.15, -0.9 * size], [1.9 * size, -1.9 * size], [2.05 * size, 0.0], [1.9 * size, 0.7 * size], [0.15, 0.25 * size]];
        let mut head = bevelled_slab(&outline, &[], &SlabOptions { depth: 0.3, bevel: 0.07, bevel_segments: 1, crease: 40f32.to_radians() });
        head.rotate_y(-FRAC_PI_2);
        head
    };
    let mut iron = MeshBuilder::new();
    match kind {
        0 => {
            iron.append(&moved(cylinder(0.3, 1.0, 8, Shading::SMOOTH), [0.0, len - 0.2, 0.0]));
            iron.append(&moved(leaf(2.6, 0.62), [0.0, len + 0.7, 0.0]));
        }
        1 => {
            iron.append(&moved(axe_head(1.0), [0.0, len - 0.7, 0.0]));
        }
        2 => {
            iron.append(&moved(cylinder(0.3, 1.0, 8, Shading::SMOOTH), [0.0, len - 0.2, 0.0]));
            iron.append(&moved(leaf(1.9, 0.4), [0.0, len + 0.7, 0.0]));
            iron.append(&moved(axe_head(0.85), [0.0, len - 1.0, 0.0]));
        }
        _ => {
            iron.append(&moved(cylinder(0.34, 0.7, 8, Shading::SMOOTH), [0.0, len - 0.3, 0.0]));
            iron.append(&moved(icosphere(0.85, 2), [0.0, len + 0.6, 0.0]));
            for (about_z, angle) in [(true, FRAC_PI_2), (true, -FRAC_PI_2), (false, FRAC_PI_2), (false, -FRAC_PI_2)] {
                let mut flange = spike(0.24, 1.25, 6, 0.0, Shading::SMOOTH);
                if about_z {
                    flange.rotate_z(angle);
                } else {
                    flange.rotate_x(angle);
                }
                flange.translate([0.0, len + 0.6, 0.0]);
                iron.append(&flange);
            }
        }
    }
    (haft, iron)
}

/// A weapon rack standing on two foot boards against the wall, 14 wide and 4 deep: two posts
/// joined by a low rail, and a mid and a top rail doubled to clamp the hafts between them. It
/// holds four to six spears, axes, halberds and maces, 14 to 26 units tall, their edges
/// turned to the front. Its back is at -Z.
pub fn weapon_rack(seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0x3ac4_0001);
    let wood = timber(&mut rng);
    let mut frame = MeshBuilder::new();
    for (lo, hi) in [(-6.9f32, -5.9f32), (5.9, 6.9)] {
        frame.append(&soft_box([lo, 0.0, -2.0], [hi, 0.8, 2.0], 0.0, 0.14));
        frame.append(&soft_box([lo, 0.8, -2.0], [hi, 14.4, -1.1], 0.0, 0.12));
        frame.append(&moved(icosphere(0.44, 1), [(lo + hi) * 0.5, 14.5, -1.55]));
        // The stubs that carry the front rails.
        for y in [8.0f32, 12.6] {
            frame.append(&soft_box([lo, y, -1.1], [hi, y + 1.0, 0.2], 0.0, 0.08));
        }
    }
    for y in [8.0f32, 12.6] {
        frame.append(&soft_box([-6.9, y, -2.0], [6.9, y + 1.0, -1.1], 0.0, 0.1));
        frame.append(&soft_box([-6.9, y, -0.3], [6.9, y + 1.0, 0.2], 0.0, 0.1));
    }
    frame.append(&soft_box([-6.9, 2.0, -2.0], [6.9, 2.8, -1.1], 0.0, 0.1));
    let n = rng.int(4, 6) as usize;
    let (mut hafts, mut heads) = (MeshBuilder::new(), MeshBuilder::new());
    for i in 0..n {
        if i > 0 && i + 1 < n && rng.chance(0.12) {
            continue;
        }
        let x = -5.2 + i as f32 * 10.4 / (n - 1) as f32;
        let (haft, head) = hafted(rng.int(0, 3), &mut rng);
        let at = [x, 0.8, -0.7];
        hafts.append(&moved(haft, at));
        heads.append(&moved(head, at));
    }
    let mut parts = Parts::new();
    parts.add_tinted("rack", frame, MaterialKind::Wood, wood);
    parts.add_tinted("hafts", hafts, MaterialKind::Wood, wood.scaled(1.25));
    parts.add_tinted("heads", heads, MaterialKind::Iron, Rgb::hex(0x8a8d90));
    parts.finish()
}
