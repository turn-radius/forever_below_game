//! Paper notes: one sheet, from a scrap to a full page.
//!
//! The sheet is a thick grid laid along a path in the (x, y) plane and across it in z. Flat it
//! is a straight path lifted by a height field (cockle, curl, lifted corners). Folded once,
//! twice or into a letter it is a turtle path: panels joined by rounded creases (real
//! geometry), the free panels sprung a little open. Torn edges pull each row's sides and each
//! column's ends in by noise. Crumpled it is a displaced, faceted lump.
//!
//! Ink, blots, blood, foxing and scorch are thin patches laid on the top panel's outer face
//! through a [`Skin`], which maps panel coordinates to a point and a normal on that face.
//!
//! The note rests on y = 0, is centred over the origin and has its long side along X.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::build::parts::{Look, Parts};
use crate::geom::{GridOptions, MeshBuilder, Shading, V3, icosphere, stitch_grid, thick_sheet, value_noise3};
use crate::material::MaterialKind::{Bone, Cloth, Paint};
use crate::material::Rgb;

/// How the note is folded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteForm {
    Flat,
    /// Folded once, sprung a little open.
    Folded,
    /// Folded in thirds, sprung a little open.
    FoldedTwice,
    /// A folded letter closed with a wax seal.
    Sealed,
    /// Balled up.
    Crumpled,
}

impl NoteForm {
    pub const ALL: [NoteForm; 5] = [NoteForm::Flat, NoteForm::Folded, NoteForm::FoldedTwice, NoteForm::Sealed, NoteForm::Crumpled];
}

/// What the note builder needs. `length` and `size` are as the family scales them; `tint`
/// already carries foxing and grime.
#[derive(Clone, Copy, Debug)]
pub struct NoteSpec {
    pub form: NoteForm,
    /// The long side of the whole sheet.
    pub length: f32,
    /// Short side over long side.
    pub aspect: f32,
    pub thick: f32,
    pub cockle: f32,
    pub curl: f32,
    pub lift: f32,
    pub spring: f32,
    /// 0 straight, 1 torn all round.
    pub tear: f32,
    pub scrawl: u32,
    pub wild: f32,
    pub blot: f32,
    pub blood: f32,
    pub crush: f32,
    pub size: f32,
    pub tint: Rgb,
    pub ink: Rgb,
    /// How many foxing flecks, and how far the edges are scorched, 0..1.
    pub fox: u32,
    pub scorch: f32,
    pub seed: u32,
}

const BLOOD: Rgb = Rgb::hex(0x5a0f0c);
const FOX: Rgb = Rgb::hex(0x7a4a22);
const CHAR: Rgb = Rgb::hex(0x2a1d12);

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smooth(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Builds the note into `parts`. `seal` is a wax seal with its base at the origin and its
/// colour, placed on a sealed letter.
pub fn note(spec: &NoteSpec, seal: Option<(MeshBuilder, Rgb)>, parts: &mut Parts) {
    let mut rng = Mulberry32::new(spec.seed.wrapping_mul(0x9e37_79b9).wrapping_add(77));
    if spec.form == NoteForm::Crumpled {
        crumpled(spec, &mut rng, parts);
    } else {
        sheet_note(spec, seal, &mut rng, parts);
    }
    centre(parts);
}

/// Centre over the origin and put the longer side along X.
fn centre(parts: &mut Parts) {
    let (lo, hi) = parts.meshes_mut().filter_map(|m| m.bounds()).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), (a, b)| {
        ([lo[0].min(a[0]), lo[1].min(a[1]), lo[2].min(a[2])], [hi[0].max(b[0]), hi[1].max(b[1]), hi[2].max(b[2])])
    });
    let turn = hi[2] - lo[2] > hi[0] - lo[0];
    let c = [(lo[0] + hi[0]) * 0.5, (lo[2] + hi[2]) * 0.5];
    for m in parts.meshes_mut() {
        m.translate([-c[0], 0.0, -c[1]]);
        if turn {
            m.rotate_y(FRAC_PI_2);
        }
    }
}

// ───────────────────────── Path ─────────────────────────

/// The sheet's centre line in (x, y), by arc length.
struct Frame {
    pts: Vec<[f32; 2]>,
    cum: Vec<f32>,
    /// Arc-length range of each panel.
    panels: Vec<(f32, f32)>,
}

impl Frame {
    fn total(&self) -> f32 {
        *self.cum.last().unwrap_or(&0.0)
    }

    /// Point and heading at arc length `s`.
    fn at(&self, s: f32) -> ([f32; 2], f32) {
        let s = s.clamp(0.0, self.total());
        let i = self.cum.partition_point(|&c| c < s).clamp(1, self.pts.len() - 1);
        let (a, b) = (self.pts[i - 1], self.pts[i]);
        let u = ((s - self.cum[i - 1]) / (self.cum[i] - self.cum[i - 1]).max(1e-9)).clamp(0.0, 1.0);
        ([lerp(a[0], b[0], u), lerp(a[1], b[1], u)], (b[1] - a[1]).atan2(b[0] - a[0]))
    }
}

fn frame_of(pts: Vec<[f32; 2]>, panel_idx: Vec<(usize, usize)>) -> Frame {
    let cum: Vec<f32> = std::iter::once(0.0)
        .chain(pts.windows(2).scan(0.0, |acc, w| {
            *acc += ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
            Some(*acc)
        }))
        .collect();
    let panels = panel_idx.into_iter().map(|(a, b)| (cum[a], cum[b])).collect();
    Frame { pts, cum, panels }
}

/// A flat path, or `n` panels joined by creases of radius `rc`: the first fold turns left to
/// head back sprung `sp` open, the second turns right to head on, sprung a little less.
fn fold_path(n: usize, total: f32, rc: f32, sp: f32) -> Frame {
    if n <= 1 {
        return frame_of(vec![[0.0, 0.0], [total, 0.0]], vec![(0, 1)]);
    }
    let heads = [0.0, PI - sp, sp * 0.6];
    let folds = n - 1;
    let fold_len: f32 = (0..folds).map(|i| rc * (heads[i + 1] - heads[i]).abs()).sum();
    let p = ((total - fold_len) / n as f32).max(0.02);
    let (mut pos, mut th) = ([0.0f32, 0.0], 0.0f32);
    let mut pts = vec![pos];
    let mut panels = Vec::new();
    for i in 0..n {
        let start = pts.len() - 1;
        let steps = (p / 0.03).ceil().max(2.0) as usize;
        for _ in 0..steps {
            pos = [pos[0] + th.cos() * p / steps as f32, pos[1] + th.sin() * p / steps as f32];
            pts.push(pos);
        }
        panels.push((start, pts.len() - 1));
        if i < folds {
            let target = heads[i + 1];
            let steps = 12;
            let d = (target - th) / steps as f32;
            for k in 0..steps {
                let mid = th + d * (k as f32 + 0.5);
                pos = [pos[0] + mid.cos() * rc * d.abs(), pos[1] + mid.sin() * rc * d.abs()];
                pts.push(pos);
            }
            th = target;
        }
    }
    frame_of(pts, panels)
}

// ───────────────────────── Skin ─────────────────────────

/// The top panel's outer face: panel coordinates `(a, b)` along x and across z, from the
/// panel's centre, to a point on the face and its outward normal.
struct Skin<'a> {
    ha: f32,
    hb: f32,
    at: &'a dyn Fn(f32, f32) -> (V3, V3),
}

/// `mesh` turned to face along `out`.
fn faced(mut mesh: MeshBuilder, out: V3) -> MeshBuilder {
    let mut sum = 0.0;
    for t in mesh.indices.as_chunks::<3>().0 {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| mesh.positions[i as usize]);
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        sum += n[0] * out[0] + n[1] * out[1] + n[2] * out[2];
    }
    if sum < 0.0 {
        mesh.flip();
    }
    mesh
}

fn on(sk: &Skin, a: f32, b: f32, eps: f32) -> V3 {
    let (p, n) = (sk.at)(a, b);
    [p[0] + n[0] * eps, p[1] + n[1] * eps, p[2] + n[2] * eps]
}

fn strip_on(sk: &Skin, a0: f32, a1: f32, b: impl Fn(f32) -> f32, width: f32, segs: usize, eps: f32) -> MeshBuilder {
    let line = |dz: f32| -> Vec<V3> { (0..=segs).map(|i| { let a = lerp(a0, a1, i as f32 / segs as f32); on(sk, a, b(a) + dz, eps) }).collect() };
    let out = (sk.at)((a0 + a1) * 0.5, b((a0 + a1) * 0.5)).1;
    faced(stitch_grid(&[line(0.0), line(-width)], &GridOptions { shading: Shading::SMOOTH, ..Default::default() }), out)
}

/// An irregular patch about `(ca, cb)`, `rx` by `rz` big.
#[allow(clippy::too_many_arguments)]
fn patch(sk: &Skin, ca: f32, cb: f32, rx: f32, rz: f32, wobble: f32, phase: f32, eps: f32) -> MeshBuilder {
    const COLS: usize = 18;
    let rows: Vec<Vec<V3>> = [0.0f32, 0.4, 0.75, 1.0]
        .iter()
        .map(|&f| {
            (0..COLS)
                .map(|c| {
                    let a = TAU * c as f32 / COLS as f32;
                    let w = 1.0 + wobble * (0.25 * (3.0 * a + phase).sin() + 0.12 * (5.0 * a + 2.0 * phase).sin() + 0.08 * (9.0 * a - phase).sin());
                    on(sk, ca + rx * f * w * a.cos(), cb + rz * f * w * a.sin(), eps)
                })
                .collect()
        })
        .collect();
    let out = (sk.at)(ca, cb).1;
    faced(stitch_grid(&rows, &GridOptions { wrap_around: true, shading: Shading::SMOOTH, ..Default::default() }), out)
}

/// Scrawled lines of ink: few, uneven, drifting, some trailing off.
fn scrawl(sk: &Skin, spec: &NoteSpec, n: u32, margin: [f32; 2], bands: (f32, f32), rng: &mut Mulberry32) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    let (ha, size, wild) = (sk.ha, spec.size, spec.wild);
    let (b_lo, b_hi) = (bands.0.max(-sk.hb + margin[1]), bands.1.min(sk.hb - margin[1]));
    let span = b_hi - b_lo;
    let avail = ha - margin[0];
    if span < 0.1 || avail < 0.1 {
        return out;
    }
    let n = n.min((span / (0.075 * size)).floor() as u32);
    for j in 0..n {
        let b0 = b_hi - span * (j as f32 + 0.5 + rng.range_f32(-0.25, 0.25) * wild) / n as f32;
        let slope = rng.range_f32(-0.12, 0.12) * wild;
        let left = -avail + rng.range_f32(0.0, 0.3 * avail * wild);
        let right = avail - rng.range_f32(0.0, avail * (0.2 + 1.2 * wild)).min(avail * 1.5);
        let width = rng.range_f32(0.016, 0.026) * size;
        let mut a = left;
        while a < right - 0.06 {
            let len = rng.range_f32(0.08, 0.3).min(right - a);
            let phase = rng.range_f32(0.0, TAU);
            let jitter = rng.range_f32(-0.012, 0.012) * (0.3 + wild);
            let segs = (len / 0.05).ceil().max(2.0) as usize;
            let base = b0 + jitter;
            let start = a;
            out.append(&strip_on(sk, a, a + len, |x| (base + slope * (x - left) + (0.004 + 0.012 * wild) * ((x - start) * 11.0 + phase).sin()).clamp(-sk.hb + margin[1] + width, sk.hb - margin[1]), width, segs, 0.004));
            a += len + rng.range_f32(0.03, 0.09);
        }
    }
    out
}

// ───────────────────────── Sheets ─────────────────────────

fn inset(spec: &NoteSpec, w: f32, s: f32, side: f32) -> f32 {
    if spec.tear <= 0.0 {
        return 0.0;
    }
    let f = 3.0 / w.max(0.6);
    let n1 = value_noise3([s * 2.1 * f, side * 3.7, 0.5], spec.seed);
    let n2 = value_noise3([s * 5.3 * f, side * 3.7, 9.1], spec.seed ^ 0x55);
    spec.tear * w * (0.03 * n1 + 0.1 * ((n2 - 0.66) / 0.34).max(0.0))
}

fn end_inset(spec: &NoteSpec, total: f32, c: usize, end: f32) -> f32 {
    if spec.tear <= 0.0 {
        return 0.0;
    }
    let n1 = value_noise3([c as f32 * 0.9, end * 2.0, 0.7], spec.seed ^ 0x21);
    let n2 = value_noise3([c as f32 * 0.4, end * 2.0, 3.3], spec.seed ^ 0x77);
    (spec.tear * total * (0.02 * n1 + 0.09 * ((n2 - 0.6) / 0.4).max(0.0))).min(0.2 * total)
}

fn sheet_note(spec: &NoteSpec, seal: Option<(MeshBuilder, Rgb)>, rng: &mut Mulberry32, parts: &mut Parts) {
    const COLS: usize = 30;
    let (len, wid, t) = (spec.length, spec.length * spec.aspect, spec.thick);
    let n = match spec.form {
        NoteForm::Flat => 1,
        NoteForm::Folded => 2,
        _ => 3,
    };
    let sealed = spec.form == NoteForm::Sealed;
    let spring = if sealed { 0.02 } else { spec.spring };
    let rc = t * 1.3 + spring * 0.05;
    let frame = fold_path(n, len, rc, spring);
    let total = frame.total();
    let top = *frame.panels.last().unwrap_or(&(0.0, total));
    let ripple = |s: f32, z: f32| -> f32 {
        value_noise3([s * 1.6, z * 1.6, 0.3], spec.seed ^ 0x3d) - 0.5
    };
    let lift = |s: f32, z: f32| -> f32 {
        if sealed {
            return 0.0;
        }
        if n == 1 {
            let (u, v) = (2.0 * s / total - 1.0, 2.0 * z / wid);
            let cockle = 0.05 * spec.size * spec.cockle * ripple(s, z) * 2.0;
            let bow = spec.curl * 0.09 * wid * v * v;
            let corners = spec.lift * 0.16 * wid * (u * v).powi(2) * smooth(0.0, 0.2, (u * v).abs());
            cockle + bow + corners
        } else if s >= top.0 && s <= top.1 {
            let w = smooth(0.0, 0.12, s - top.0) * smooth(0.0, 0.12, top.1 - s);
            0.03 * spec.size * spec.cockle * (ripple(s, z) * 2.0).abs() * w
        } else {
            0.0
        }
    };
    let rows = ((total / 0.02).ceil() as usize).clamp(48, 300);
    let grid: Vec<Vec<V3>> = (0..rows)
        .map(|k| {
            let f = k as f32 / (rows - 1) as f32;
            (0..COLS)
                .map(|c| {
                    let cf = c as f32 / (COLS - 1) as f32;
                    let (a, b) = (end_inset(spec, total, c, 0.0), end_inset(spec, total, c, 1.0));
                    let s = a + (total - a - b) * f;
                    let (p, _) = frame.at(s);
                    let zlo = -wid * 0.5 + inset(spec, wid, s, 0.0);
                    let zhi = wid * 0.5 - inset(spec, wid, s, 1.0);
                    let z = lerp(zlo, zhi, cf);
                    [p[0], p[1] + lift(s, z), z]
                })
                .collect()
        })
        .collect();
    parts.add_look("sheet", thick_sheet(&grid, &GridOptions { shading: Shading::SMOOTH, ..Default::default() }, t), Look::of(Cloth).tinted(spec.tint));

    // The top panel's outer face.
    let left = (n - 1) % 2 == 0;
    let dir = if left { 1.0 } else { -1.0 };
    let smid = (top.0 + top.1) * 0.5;
    let at = |a: f32, b: f32| -> (V3, V3) {
        let s = smid + dir * a;
        let (p, h) = frame.at(s);
        let nl = [-h.sin(), h.cos()];
        let o = if left { nl } else { [-nl[0], -nl[1]] };
        let e = if left { 0.0 } else { t };
        ([p[0] + o[0] * e, p[1] + o[1] * e + lift(s, b), b], [o[0], o[1], 0.0])
    };
    let sk = Skin { ha: (top.1 - top.0) * 0.5, hb: wid * 0.5, at: &at };
    let margin = [0.07 * spec.size + spec.tear * 0.2 * (top.1 - top.0), 0.07 * spec.size + spec.tear * 0.27 * wid];
    let inner = [sk.ha - margin[0], sk.hb - margin[1]];

    if inner[0] > 0.03 && inner[1] > 0.03 {
        // Ink: a letter's address sits above its seal; a note's lines run down the page.
        let lines = if sealed { spec.scrawl.min(3) } else { spec.scrawl };
        let bands = if sealed { (inner[1] * 0.1, inner[1]) } else { (-inner[1], inner[1]) };
        let mut ink = scrawl(&sk, spec, lines, margin, bands, rng);
        let reach = |r: f32| r.min(inner[0].min(inner[1]) * 0.8);
        // Blots and blood sit where the page allows, clear of the seal.
        let place = |rng: &mut Mulberry32| -> (f32, f32) {
            let a = rng.range_f32(-inner[0], inner[0]) * 0.8;
            let b = if sealed { rng.range_f32(0.4, 0.9) * inner[1] } else { rng.range_f32(-inner[1], inner[1]) * 0.8 };
            (a, b)
        };
        if spec.blot > 0.0 {
            let r = reach((0.05 + 0.1 * spec.blot) * spec.size);
            let (a, b) = place(rng);
            ink.append(&patch(&sk, a, b, r, r * rng.range_f32(0.8, 1.2), 1.0, rng.range_f32(0.0, TAU), 0.0045));
            for _ in 0..(spec.blot * 4.0).round() as u32 {
                let (da, db) = (rng.range_f32(-2.2, 2.2) * r, rng.range_f32(-2.2, 2.2) * r);
                let d = rng.range_f32(0.12, 0.3) * r;
                ink.append(&patch(&sk, (a + da).clamp(-inner[0], inner[0]), (b + db).clamp(-inner[1], inner[1]), d, d, 0.6, 0.0, 0.0045));
            }
        }
        if !ink.is_empty() {
            parts.add_look("ink", ink, Look::of(Paint).tinted(spec.ink).without_outline());
        }
        if spec.blood > 0.0 {
            let mut blood = MeshBuilder::new();
            let r = reach((0.08 + 0.2 * spec.blood) * spec.size);
            let (a, b) = place(rng);
            blood.append(&patch(&sk, a, b, r * 1.1, r * rng.range_f32(0.6, 1.0), 1.0, rng.range_f32(0.0, TAU), 0.0055));
            for _ in 0..(2.0 + spec.blood * 4.0) as u32 {
                let (da, db) = (rng.range_f32(-1.8, 1.8) * r, rng.range_f32(-1.8, 1.8) * r);
                let d = rng.range_f32(0.06, 0.22) * r;
                blood.append(&patch(&sk, (a + da).clamp(-inner[0], inner[0]), (b + db).clamp(-inner[1], inner[1]), d, d * 1.4, 0.5, 0.0, 0.0055));
            }
            parts.add_look("blood", blood, Look::of(Paint).tinted(BLOOD).without_outline());
        }
        if spec.fox > 0 {
            let mut fox = MeshBuilder::new();
            for _ in 0..spec.fox {
                let (a, b) = (rng.range_f32(-inner[0], inner[0]), rng.range_f32(-inner[1], inner[1]));
                let (rx, rz) = (rng.range_f32(0.02, 0.07), rng.range_f32(0.02, 0.07));
                fox.append(&patch(&sk, a, b, rx, rz, 0.7, rng.range_f32(0.0, TAU), 0.0025));
            }
            parts.add_look("foxing", fox, Look::of(Cloth).tinted(FOX).without_outline());
        }
    }
    if spec.scorch > 0.0 {
        let mut out = MeshBuilder::new();
        let segs = ((sk.ha * 2.0 / 0.05).ceil() as usize).max(2);
        for side in [-1.0f32, 1.0] {
            let width = |a: f32| spec.scorch * (0.03 + 0.1 * value_noise3([a * 3.1, side * 5.0, 1.7], spec.seed ^ 0xb7)) * wid;
            let edge = |a: f32| side * (sk.hb - inset(spec, wid, smid + dir * a, if side < 0.0 { 0.0 } else { 1.0 }) - 0.004);
            let inward = |a: f32| edge(a) - side * width(a);
            let line = |f: &dyn Fn(f32) -> f32| -> Vec<V3> { (0..=segs).map(|i| { let a = lerp(-sk.ha, sk.ha, i as f32 / segs as f32); on(&sk, a, f(a), 0.003) }).collect() };
            let e = line(&edge);
            let i = line(&inward);
            out.append(&faced(stitch_grid(&[e, i], &GridOptions { shading: Shading::SMOOTH, ..Default::default() }), (sk.at)(0.0, 0.0).1));
        }
        parts.add_look("scorch", out, Look::of(Cloth).tinted(CHAR).without_outline());
    }

    if let (true, Some((mut mesh, color))) = (sealed, seal) {
        let k = ((0.22 * (top.1 - top.0).min(wid)) / 0.2).clamp(0.35, 1.0);
        mesh.scale_uniform(k);
        let (p, _) = (sk.at)(0.0, -0.1 * sk.hb);
        mesh.translate([p[0], p[1] - 0.01, p[2]]);
        parts.add_look("seal", mesh, Look::of(Bone).tinted(color));
    }
}

// ───────────────────────── Crumpled ─────────────────────────

fn crumpled(spec: &NoteSpec, rng: &mut Mulberry32, parts: &mut Parts) {
    let r0 = (0.19 * (spec.length * spec.length * spec.aspect).sqrt()).max(0.1);
    let seed = spec.seed;
    let crush = spec.crush;
    let radius = |d: V3| -> f32 {
        let n1 = value_noise3([d[0] * 1.7 + 4.0, d[1] * 1.7, d[2] * 1.7], seed ^ 0x11);
        let n2 = value_noise3([d[0] * 3.6, d[1] * 3.6 + 7.0, d[2] * 3.6], seed ^ 0x25);
        let n3 = value_noise3([d[0] * 7.5, d[1] * 7.5, d[2] * 7.5 + 3.0], seed ^ 0x39);
        let ridge = 1.0 - (2.0 * n2 - 1.0).abs();
        r0 * (0.95 + crush * (0.7 * (n1 - 0.5) + 0.35 * (ridge - 0.6) + 0.14 * (n3 - 0.5)))
    };
    let pos_of = |d: V3| -> V3 {
        let r = radius(d);
        [d[0] * r, d[1] * r * 0.85, d[2] * r]
    };
    let mut ball = icosphere(1.0, 3);
    for p in &mut ball.positions {
        let d = *p;
        *p = pos_of(d);
    }
    let ball = ball.flat_shaded();
    parts.add_look("sheet", ball, Look::of(Cloth).tinted(spec.tint));

    // Ink, blots and blood show on the top cap, lifted clear of the creases.
    let norm = |v: V3| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let at = |a: f32, b: f32| -> (V3, V3) {
        let d = norm([a / r0, 1.0, b / r0]);
        (pos_of(d), d)
    };
    let sk = Skin { ha: r0 * 0.7, hb: r0 * 0.7, at: &at };
    let eps = 0.012;
    let mut ink = scrawl(&sk, spec, spec.scrawl.min(3), [0.1 * r0, 0.1 * r0], (-sk.hb, sk.hb), rng);
    if spec.blot > 0.0 {
        let r = (0.05 + 0.1 * spec.blot) * spec.size;
        ink.append(&patch(&sk, rng.range_f32(-0.4, 0.4) * r0, rng.range_f32(-0.4, 0.4) * r0, r, r, 1.0, 0.0, eps));
    }
    if !ink.is_empty() {
        parts.add_look("ink", ink, Look::of(Paint).tinted(spec.ink).without_outline());
    }
    if spec.blood > 0.0 {
        let r = (0.08 + 0.15 * spec.blood) * spec.size;
        let mut blood = patch(&sk, rng.range_f32(-0.4, 0.4) * r0, rng.range_f32(-0.4, 0.4) * r0, r, r * 0.8, 1.0, 1.0, eps);
        blood.append(&patch(&sk, rng.range_f32(-0.5, 0.5) * r0, rng.range_f32(-0.5, 0.5) * r0, r * 0.25, r * 0.3, 0.5, 0.0, eps));
        parts.add_look("blood", blood, Look::of(Paint).tinted(BLOOD).without_outline());
    }
}
