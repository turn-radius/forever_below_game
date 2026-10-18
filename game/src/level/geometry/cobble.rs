//! Loose paving: cobblestones and the rough flagstones of cave floors. Both are stones of
//! every size packed into a cell by dart throwing, so no two rows line up: cobbles are
//! domes of different shapes, heights and tilts in several tones, thinning to nothing where
//! the floor is worn, damp or by a wall; flagstones are large flat polygons fitted roughly
//! with wide joints, crazy-paving style. Each stone is a closed patch over the floor shell,
//! which shows between them as the joints.
//!
//! Laid per cell from the cell's own seed, and no stone crosses a cell's edge, so a stretch of
//! the level draws exactly the stones the whole level would.

use armory::{
    MaterialKind, Rgb,
    build::parts::{Look, Parts},
    geom::{MeshBuilder, V3, value_noise3, vec3},
};
use fb_level::CELL;
use procgen_rng::Mulberry32;

/// The tones stones come in: the floor's own grey, a paler, a darker and a bluer grey, and two
/// browner ones. Each tone is a part of its own.
pub const TONES: [(&str, Rgb); 6] = [
    ("stones_mid", Rgb::hex(0x6e6c66)),
    ("stones_pale", Rgb::hex(0x85837c)),
    ("stones_dark", Rgb::hex(0x4f4d49)),
    ("stones_slate", Rgb::hex(0x5d626a)),
    ("stones_brown", Rgb::hex(0x70604b)),
    ("stones_umber", Rgb::hex(0x56473a)),
];
const COBBLE_TONES: [f32; 6] = [0.34, 0.18, 0.17, 0.13, 0.12, 0.06];
const PAVING_TONES: [f32; 6] = [0.4, 0.28, 0.12, 0.2, 0.0, 0.0];

/// The stones laid so far, one mesh per tone.
#[derive(Default)]
pub struct Stones {
    tones: [MeshBuilder; 6],
}

impl Stones {
    pub fn is_empty(&self) -> bool {
        self.tones.iter().all(|t| t.vertex_count() == 0)
    }

    /// Adds each tone that has stones as a part, in `look` of its colour.
    pub fn add_parts(self, parts: &mut Parts, look: impl Fn(Rgb) -> Look) {
        for (mesh, (name, tint)) in self.tones.into_iter().zip(TONES) {
            if mesh.vertex_count() > 0 {
                parts.add_look(name, mesh, look(tint));
            }
        }
    }
}

/// How a kind of stone is made and packed. Sizes are half widths in world units.
struct Kind {
    /// `(attempts, smallest, largest)` radius per class, big stones first so they pack well.
    classes: &'static [(usize, f32, f32)],
    /// The share of stones that are long, and how long: ratio of the long side to the short.
    long_share: f32,
    long: (f32, f32),
    plain: (f32, f32),
    /// How squarish the outline is (2 is round) and how far it wanders.
    squareness: (f32, f32),
    wander: f32,
    /// Height over the floor, and the share that sit sunk, with their height.
    raise: (f32, f32),
    sunk_share: f32,
    sunk: (f32, f32),
    /// The share with a flat worn top, and how far a stone may lean.
    flat_share: f32,
    lean: f32,
    /// The profile of a round stone and of a flat one: `(reach, height)` of two rings from the
    /// crown out; the foot ring follows.
    round: [(f32, f32); 2],
    flat: [(f32, f32); 2],
    /// The gap between stones, tight at 0 on the looseness field and wide at 1.
    gap: (f32, f32),
    /// Most triangles a cell may take.
    budget: usize,
    tones: &'static [f32; 6],
    /// Whether wear thins the stones out.
    wears: bool,
}

const COBBLE: Kind = Kind {
    classes: &[(20, 1.9, 2.25), (220, 1.15, 1.75), (420, 0.8, 1.1)],
    long_share: 0.3,
    long: (1.3, 1.65),
    plain: (1.0, 1.25),
    squareness: (2.1, 3.2),
    wander: 0.12,
    raise: (0.22, 0.7),
    sunk_share: 0.15,
    sunk: (0.06, 0.18),
    flat_share: 0.3,
    lean: 0.07,
    round: [(0.55, 0.9), (0.88, 0.5)],
    flat: [(0.6, 1.0), (0.9, 0.82)],
    gap: (0.1, 0.75),
    budget: 1500,
    tones: &COBBLE_TONES,
    wears: true,
};

const PAVING: Kind = Kind {
    classes: &[(30, 2.4, 3.4), (260, 1.5, 2.3)],
    long_share: 0.4,
    long: (1.2, 1.55),
    plain: (1.0, 1.2),
    squareness: (2.0, 2.4),
    wander: 0.22,
    raise: (0.14, 0.3),
    sunk_share: 0.12,
    sunk: (0.03, 0.09),
    flat_share: 1.0,
    lean: 0.025,
    round: [(0.6, 1.0), (0.94, 0.92)],
    flat: [(0.6, 1.0), (0.94, 0.92)],
    gap: (0.55, 1.5),
    budget: 1200,
    tones: &PAVING_TONES,
    wears: false,
};

/// Appends the cobbles over cell `(x, y)`, and returns each one's half sizes. `base` is the
/// floor's height at a world `(x, z)`; `wear` is how worn the floor is there, 0 to 1 (high near
/// walls and where it is damp).
pub fn cobbles(seed: u32, x: i32, y: i32, base: &impl Fn(f32, f32) -> f32, wear: &impl Fn(f32, f32) -> f32, out: &mut Stones) -> Vec<[f32; 2]> {
    lay(&COBBLE, seed ^ 0xc0b8_1e57, x, y, base, wear, out)
}

/// Appends rough flagstones, laid on the relief, over cell `(x, y)`.
pub fn paving(seed: u32, x: i32, y: i32, base: &impl Fn(f32, f32) -> f32, out: &mut Stones) -> Vec<[f32; 2]> {
    lay(&PAVING, seed ^ 0x9a71_6000, x, y, base, &|_, _| 0.0, out)
}

/// One stone before it is meshed.
struct Stone {
    centre: [f32; 2],
    half: [f32; 2],
    /// Cosine and sine of the stone's turn.
    turn: [f32; 2],
    squareness: f32,
    raise: f32,
    rings: [(f32, f32); 2],
    lean: [f32; 2],
    wobble: Vec<f32>,
    tone: usize,
}

impl Stone {
    /// How far the stone reaches from its centre toward world direction `(dx, dz)` (unit).
    fn reach(&self, dx: f32, dz: f32) -> f32 {
        // Into the stone's own axes.
        let (u, v) = (dx * self.turn[0] + dz * self.turn[1], -dx * self.turn[1] + dz * self.turn[0]);
        let e = (u / self.half[0]).powi(2) + (v / self.half[1]).powi(2);
        1.08 * self.wobble.iter().copied().fold(1.0, f32::max) / e.sqrt()
    }

    /// The point on ring `ring` (0 is the crown) at side `k`.
    fn point(&self, ring: usize, k: usize, base: &impl Fn(f32, f32) -> f32) -> V3 {
        let sides = self.wobble.len();
        if ring == 0 {
            return [self.centre[0], base(self.centre[0], self.centre[1]) + self.raise, self.centre[1]];
        }
        let (reach, height) = if ring == 3 { (1.0, 0.0) } else { self.rings[ring - 1] };
        let angle = k as f32 / sides as f32 * std::f32::consts::TAU;
        let (c, s) = (angle.cos(), angle.sin());
        let p = self.squareness;
        let fit = (c.abs().powf(p) + s.abs().powf(p)).powf(-1.0 / p) * reach * self.wobble[k];
        let (lx, lz) = (c * fit * self.half[0], s * fit * self.half[1]);
        let (px, pz) = (self.centre[0] + lx * self.turn[0] - lz * self.turn[1], self.centre[1] + lx * self.turn[1] + lz * self.turn[0]);
        // The foot sinks under the floor so it never floats; the rest leans.
        let rise = if ring == 3 { -SINK } else { self.raise * height + height * (self.lean[0] * (px - self.centre[0]) + self.lean[1] * (pz - self.centre[1])) };
        [px, base(px, pz) + rise, pz]
    }

    fn lay(&self, base: &impl Fn(f32, f32) -> f32, mesh: &mut MeshBuilder) {
        let sides = self.wobble.len();
        let mut points = vec![self.point(0, 0, base)];
        for ring in 1..=3 {
            points.extend((0..sides).map(|k| self.point(ring, k, base)));
        }
        let at = |ring: usize, k: usize| if ring == 0 { 0 } else { 1 + (ring - 1) * sides + k % sides };
        let mut tris = Vec::new();
        for k in 0..sides {
            tris.push([0, at(1, k + 1), at(1, k)]);
            for ring in 1..3 {
                let (a, b, c, d) = (at(ring, k), at(ring, k + 1), at(ring + 1, k + 1), at(ring + 1, k));
                tris.push([a, c, d]);
                tris.push([a, b, c]);
            }
        }
        // Smooth normals: each vertex takes the area-weighted normals of its triangles.
        let mut normals = vec![[0.0f32; 3]; points.len()];
        for t in &tris {
            let n = vec3::cross(vec3::sub(points[t[1]], points[t[0]]), vec3::sub(points[t[2]], points[t[0]]));
            for &i in t {
                normals[i] = vec3::add(normals[i], n);
            }
        }
        let first = mesh.vertex_count() as u32;
        for (p, n) in points.iter().zip(&normals) {
            mesh.push_vertex(*p, vec3::normalize_or(*n, [0.0, 1.0, 0.0]), [p[0] / CELL, p[2] / CELL]);
        }
        for t in &tris {
            // Wound counter-clockwise seen from above, so the dome faces up.
            mesh.push_triangle(first + t[0] as u32, first + t[1] as u32, first + t[2] as u32);
        }
    }

    fn triangles(&self) -> usize {
        self.wobble.len() * 5
    }
}

/// How far a stone's foot sinks under the floor.
const SINK: f32 = 0.25;
/// How far from the cell's edge a stone stays, so a joint runs along every edge.
const EDGE: f32 = 0.3;

/// Packs `kind`'s stones into cell `(x, y)` by dart throwing and meshes them.
fn lay(kind: &Kind, seed: u32, x: i32, y: i32, base: &impl Fn(f32, f32) -> f32, wear: &impl Fn(f32, f32) -> f32, out: &mut Stones) -> Vec<[f32; 2]> {
    let cell_seed = seed ^ (x as u32).wrapping_mul(0x9e37_79b1) ^ (y as u32).wrapping_mul(0x85eb_ca6b);
    let mut rng = Mulberry32::new(cell_seed);
    let origin = [x as f32 * CELL, y as f32 * CELL];
    // Low frequency fields over the world, so a patch of floor is tight, loose or bare as a whole.
    let field = |px: f32, pz: f32, scale: f32, salt: u32| value_noise3([px / scale, 0.37 * salt as f32, pz / scale], seed ^ salt.wrapping_mul(0x2545_f491));
    let mut placed: Vec<Stone> = Vec::new();
    let mut triangles = 0;
    for &(attempts, r_lo, r_hi) in kind.classes {
        for _ in 0..attempts {
            // Each attempt draws these numbers whether or not it is kept. The per-side wobble is drawn
            // after the early rejections, which depend only on the cell, so the layout is still a
            // pure function of the seed.
            let d: [f32; 14] = std::array::from_fn(|_| rng.range_f32(0.0, 1.0));
            let r = r_lo + (r_hi - r_lo) * d[0];
            let ratio = if d[1] < kind.long_share { kind.long.0 + (kind.long.1 - kind.long.0) * d[2] } else { kind.plain.0 + (kind.plain.1 - kind.plain.0) * d[2] };
            let half = [r * ratio.sqrt(), r / ratio.sqrt()];
            let turn_angle = d[3] * std::f32::consts::PI;
            let ext = half[0].max(half[1]) * (1.0 + kind.wander) * 1.1;
            if ext * 2.0 + EDGE * 2.0 > CELL {
                continue;
            }
            let span = CELL - 2.0 * (EDGE + ext);
            let centre = [origin[0] + EDGE + ext + span * d[4], origin[1] + EDGE + ext + span * d[5]];
            // Worn floor, and floor that is damp or by a wall, is bare in patches.
            let worn = if kind.wears { (wear(centre[0], centre[1]) * 0.8 + (1.0 - field(centre[0], centre[1], 11.0, 1)) * 0.3).clamp(0.0, 1.0) } else { 0.0 };
            if kind.wears && field(centre[0], centre[1], 4.0, 2) < worn * 0.9 - 0.1 {
                continue;
            }
            let loose = field(centre[0], centre[1], 9.0, 3);
            let gap = kind.gap.0 + (kind.gap.1 - kind.gap.0) * loose;
            let squareness = kind.squareness.0 + (kind.squareness.1 - kind.squareness.0) * d[6];
            let flat = d[7] < kind.flat_share;
            let sunk = d[8] < kind.sunk_share;
            let raise = if sunk { kind.sunk.0 + (kind.sunk.1 - kind.sunk.0) * d[9] } else { kind.raise.0 + (kind.raise.1 - kind.raise.0) * d[9] };
            let sides = if r < 1.05 { 6 } else if r < 1.8 { 7 } else { 9 };
            let lean_angle = d[10] * std::f32::consts::TAU;
            let lean_size = kind.lean * d[11] * if sunk { 2.0 } else { 1.0 };
            let wobble: Vec<f32> = (0..sides).map(|_| 1.0 + kind.wander * (rng.range_f32(0.0, 2.0) - 1.0)).collect();
            let mut tone = 0;
            let mut pick = d[12] * kind.tones.iter().sum::<f32>();
            for (i, w) in kind.tones.iter().enumerate() {
                if pick < *w {
                    tone = i;
                    break;
                }
                pick -= w;
            }
            let stone = Stone {
                centre,
                half,
                turn: [turn_angle.cos(), turn_angle.sin()],
                squareness,
                raise,
                rings: if flat { kind.flat } else { kind.round },
                lean: [lean_angle.cos() * lean_size, lean_angle.sin() * lean_size],
                wobble,
                tone,
            };
            if triangles + stone.triangles() > kind.budget {
                continue;
            }
            // Clear of every stone already down, by the gap along the line between them.
            let clear = placed.iter().all(|o| {
                let (dx, dz) = (o.centre[0] - stone.centre[0], o.centre[1] - stone.centre[1]);
                let dist = (dx * dx + dz * dz).sqrt();
                if dist < 1e-3 {
                    return false;
                }
                let (ux, uz) = (dx / dist, dz / dist);
                dist > stone.reach(ux, uz) + o.reach(-ux, -uz) + gap
            });
            if !clear {
                continue;
            }
            triangles += stone.triangles();
            placed.push(stone);
        }
    }
    for stone in &placed {
        stone.lay(base, &mut out.tones[stone.tone]);
    }
    placed.iter().map(|s| s.half).collect()
}

/// The look of every tone's part: dressed stone without hulls, as the rest of the floor.
pub fn stone_look(tint: Rgb) -> Look {
    Look::of(MaterialKind::Stone).tinted(tint).without_outline()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(stones: &Stones) -> usize {
        stones.tones.iter().map(|t| t.indices.len() / 3).sum()
    }

    fn cobbled(seed: u32, x: i32, y: i32, wear: f32) -> Stones {
        let mut out = Stones::default();
        cobbles(seed, x, y, &|_, _| 0.0, &|_, _| wear, &mut out);
        out
    }

    #[test]
    fn a_cobbled_cell_is_deterministic_and_within_budget() {
        let (a, b) = (cobbled(7, 3, 4, 0.0), cobbled(7, 3, 4, 0.0));
        for (p, q) in a.tones.iter().zip(&b.tones) {
            assert_eq!(p.positions, q.positions);
        }
        assert!((600..=COBBLE.budget).contains(&count(&a)), "{} triangles", count(&a));
        // Several tones appear over a few cells.
        let used = (0..6).filter(|&i| (0..6).any(|c| cobbled(7, c, 1, 0.0).tones[i].vertex_count() > 0)).count();
        assert!(used >= 5, "{used} tones");
    }

    #[test]
    fn worn_floors_have_fewer_stones() {
        let fresh: usize = (0..8).map(|x| count(&cobbled(3, x, 2, 0.0))).sum();
        let worn: usize = (0..8).map(|x| count(&cobbled(3, x, 2, 1.0))).sum();
        assert!(worn * 10 < fresh * 8, "{worn} worn against {fresh}");
    }

    #[test]
    fn stones_vary_widely_in_size() {
        let mut out = Stones::default();
        let halves: Vec<[f32; 2]> = (0..4).flat_map(|c| cobbles(11, c, 0, &|_, _| 0.0, &|_, _| 0.0, &mut out)).collect();
        let widths: Vec<f32> = halves.iter().map(|h| h[0].min(h[1]) * 2.0).collect();
        let (lo, hi) = widths.iter().fold((f32::MAX, f32::MIN), |(l, h), &w| (l.min(w), h.max(w)));
        assert!(lo < 2.2 && hi > 3.6, "widths {lo} to {hi}");
    }

    #[test]
    fn stones_stay_in_their_cell_face_up_and_follow_the_floor() {
        let slope = |x: f32, _: f32| 0.01 * x;
        let mut out = Stones::default();
        cobbles(9, 2, 1, &slope, &|_, _| 0.0, &mut out);
        paving(9, 2, 1, &slope, &mut out);
        assert!(!out.is_empty());
        for mesh in &out.tones {
            for (p, n) in mesh.positions.iter().zip(&mesh.normals) {
                assert!(p[0] > 2.0 * CELL && p[0] < 3.0 * CELL && p[2] > CELL && p[2] < 2.0 * CELL, "{p:?} leaves its cell");
                assert!(n[1] > 0.0, "normal {n:?} faces down");
                assert!(p[1] >= slope(p[0], p[2]) - SINK - 1e-4 && p[1] <= slope(p[0], p[2]) + 1.2, "{p:?} floats or sinks");
            }
        }
    }

    #[test]
    fn paving_is_a_few_large_slabs() {
        let mut out = Stones::default();
        let slabs = paving(5, 0, 0, &|_, _| 0.0, &mut out).len();
        assert!((6..=45).contains(&slabs), "{slabs} slabs");
    }
}
