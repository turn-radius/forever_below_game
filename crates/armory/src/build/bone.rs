//! Bones: the pieces a skeleton is made of, built from the shared geometry like every item.
//! Long bones with knobbed ends, vertebrae, ribs, a shaped skull with eye sockets, teeth and a
//! separate jaw, a pelvis, hands and feet of small separate bones, and the cracks and pitting of
//! old age. Every builder is a pure function of its arguments and a seeded RNG, in whatever
//! frame its caller asks for (points are given, not implied), so `fb_creatures` can lay a
//! skeleton out on a body's joints and `fb_body` can fit gear over it.
//!
//! Frames: the skull, pelvis, hand and foot are built in the frame of their doc comment; long
//! bones, ribs and cracks are built between the points they are given. Faces point out, and a
//! left piece is the right one mirrored with `MeshBuilder::scale([-1.0, 1.0, 1.0])`.
//!
//! Everything here is our own: the Armory has no reference for a skeleton beyond the scenery's
//! scattered bones, which this replaces for the proportions of a whole body.

use std::f32::consts::{PI, TAU};

use procgen_rng::Mulberry32;

use crate::build::form::HandForm;
use crate::geom::{
    CatmullRom, GridOptions, Hollow, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, carve_hollows, ellipsoid, grid_normals, icosphere, lathe, spike, thick_sheet, tube, uv_sphere, value_noise3, vec3,
};
use crate::material::Rgb;

/// The dark inside an eye socket, a nose or a crack.
pub const SOCKET: Rgb = Rgb::hex(0x241e18);

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn lathe_bone(profile: &[[f32; 2]], facets: usize) -> MeshBuilder {
    lathe(profile, &LatheOptions { facets, shading: Shading::SMOOTH, cap_start: false, cap_end: false, phase: 0.0 })
}

/// How a long bone is shaped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LongBone {
    /// Radius of the knob at each end (`a`, `b`).
    pub ends: [f32; 2],
    /// Radius of the shaft, narrowest at its middle.
    pub shaft: f32,
    /// Radius of the two condyles that flank each end (0 for none): the lumps a joint's knob
    /// has either side of it.
    pub lobes: [f32; 2],
    /// Which way the condyles flank the ends; perpendicular to the bone.
    pub side: V3,
}

impl LongBone {
    /// A bone with knobs `ends`, no condyles and a shaft `shaft` across.
    pub fn plain(ends: [f32; 2], shaft: f32) -> LongBone {
        LongBone { ends, shaft, lobes: [0.0; 2], side: vec3::X }
    }

    /// The radius of the bone a share `t` of the way from `a` to `b`, for laying cracks on it.
    pub fn radius_at(&self, t: f32) -> f32 {
        let profile = self.profile();
        let t = t.clamp(0.0, 1.0);
        let i = profile.windows(2).position(|w| t <= w[1][1]).unwrap_or(profile.len() - 2);
        let (a, b) = (profile[i], profile[i + 1]);
        let s = ((t - a[1]) / (b[1] - a[1]).max(1e-6)).clamp(0.0, 1.0);
        a[0] + (b[0] - a[0]) * s
    }

    /// `[radius, t]` along the bone, a unit long.
    fn profile(&self) -> Vec<[f32; 2]> {
        let [r0, r1] = self.ends;
        let s = self.shaft;
        vec![
            [0.0, 0.0],
            [r0 * 0.62, 0.004],
            [r0 * 0.93, 0.022],
            [r0, 0.05],
            [r0 * 0.78 + s * 0.3, 0.1],
            [s * 1.18, 0.2],
            [s * 0.95, 0.38],
            [s * 0.88, 0.55],
            [s * 1.0, 0.75],
            [s * 1.12, 0.84],
            [r1 * 0.8 + s * 0.3, 0.9],
            [r1, 0.95],
            [r1 * 0.92, 0.978],
            [r1 * 0.62, 0.996],
            [0.0, 1.0],
        ]
    }
}

/// A long bone from `a` to `b`: a shaft that narrows toward its middle between two knobs, with
/// a condyle either side of each end when `lobes` asks for them.
pub fn long_bone(a: V3, b: V3, bone: &LongBone) -> MeshBuilder {
    let axis = vec3::sub(b, a);
    let length = vec3::length(axis).max(1e-4);
    let dir = vec3::scale(axis, 1.0 / length);
    let profile: Vec<[f32; 2]> = bone.profile().iter().map(|&[r, t]| [r, t * length]).collect();
    let mut mesh = lathe_bone(&profile, 12);
    for (end, (&lobe, &knob)) in bone.lobes.iter().zip(&bone.ends).enumerate() {
        if lobe <= 0.0 {
            continue;
        }
        let y = if end == 0 { 0.03 * length } else { 0.97 * length };
        for sign in [-1.0, 1.0] {
            let mut condyle = uv_sphere(lobe, 10, 7);
            condyle.scale([1.0, 1.0, 1.15]).translate([sign * (knob * 0.62 + lobe * 0.2), y, 0.0]);
            mesh.append(&condyle);
        }
    }
    // Local +X is the side the condyles flank, +Y the bone's axis.
    let forward = vec3::cross(bone.side, dir);
    mesh.place_basis(a, dir, forward);
    mesh
}

/// Pits an old bone's surface: each vertex moves along its normal by up to `amount`, by value
/// noise of its place, so the seams of a lathe stay joined.
pub fn weather(mesh: &mut MeshBuilder, seed: u32, amount: f32) {
    let normals = mesh.normals.clone();
    for (p, n) in mesh.positions.iter_mut().zip(&normals) {
        let a = value_noise3(vec3::scale(*p, 4.0), seed) - 0.5;
        let b = value_noise3(vec3::scale(*p, 11.0), seed ^ 0x51ed) - 0.5;
        *p = vec3::add(*p, vec3::scale(*n, amount * (a + 0.5 * b)));
    }
}

/// A thin dark crack along `path`, a tube `radius` across at its middle and tapering to a point
/// at both ends: lay the path a little inside the surface so it reads as a groove.
pub fn crack(path: &[V3], radius: f32) -> MeshBuilder {
    if path.len() < 2 {
        return MeshBuilder::new();
    }
    let smooth_path = CatmullRom::centripetal(path.to_vec()).sample_evenly(path.len() * 3);
    tube(&smooth_path, 5, &SweepOptions { cap_start: false, cap_end: false, ..Default::default() }, |t| radius * (1.0 - 0.7 * (2.0 * t - 1.0).abs()))
}

/// A crack that wanders over a surface given as `surface(u, v)`: from `start` in `(u, v)`,
/// heading `heading` radians in that plane, `steps` steps of `step` each, turning a little every
/// step.
pub fn crack_walk(rng: &mut Mulberry32, surface: &dyn Fn(f32, f32) -> V3, start: [f32; 2], heading: f32, steps: usize, step: f32, radius: f32) -> MeshBuilder {
    let (mut u, mut v, mut h) = (start[0], start[1], heading);
    let mut path = vec![surface(u, v)];
    for _ in 0..steps {
        h += rng.range_f32(-0.7, 0.7);
        u += h.cos() * step;
        v += h.sin() * step;
        path.push(surface(u, v));
    }
    crack(&path, radius)
}

/// A tooth: a rounded peg `width` across, `height` long and `depth` thick, its crown at the
/// origin and its root up the +Y axis (it hangs from an upper jaw; turn it over for a lower one).
pub fn tooth(width: f32, height: f32, depth: f32) -> MeshBuilder {
    let mut t = uv_sphere(1.0, 8, 6);
    t.scale([width * 0.5, height * 0.5, depth * 0.5]).translate([0.0, height * 0.5, 0.0]);
    t
}

/// A sheet with a real thickness whose front surface faces `outward`: the grid is turned over
/// if its own front faces the other way (a mirrored grid does).
fn facing_sheet(points: &[Vec<V3>], outward: V3, thickness: f32) -> MeshBuilder {
    let options = GridOptions { wrap_around: false, wrap_along: false, shading: Shading::SMOOTH, row_v: None };
    let normals = grid_normals(points, false, false);
    let (rows, cols) = (points.len(), points[0].len());
    if vec3::dot(normals[rows / 2][cols / 2], outward) >= 0.0 {
        thick_sheet(points, &options, thickness)
    } else {
        let turned: Vec<Vec<V3>> = points.iter().rev().cloned().collect();
        thick_sheet(&turned, &options, thickness)
    }
}

/// What a vertebra is like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertebra {
    /// Radius of the body (the drum in front).
    pub radius: f32,
    /// Height of the body.
    pub height: f32,
    /// Length of the spinous process, pointing back (-Z) and down.
    pub spine: f32,
    /// Length of each transverse process, pointing out to the sides.
    pub transverse: f32,
}

/// A vertebra centred on the origin, its axis up +Y and its back toward -Z: a waisted drum with
/// the bony arch behind it, a spinous process and two transverse processes.
pub fn vertebra(v: &Vertebra) -> MeshBuilder {
    let (r, h) = (v.radius, v.height);
    let profile = [[0.0, -0.5 * h], [r * 0.86, -0.5 * h], [r, -0.38 * h], [r * 0.84, 0.0], [r, 0.38 * h], [r * 0.86, 0.5 * h], [0.0, 0.5 * h]];
    let mut mesh = lathe_bone(&profile, 12);
    mesh.scale([1.18, 1.0, 0.92]);
    // The arch: a ring behind the drum, joined to it by two pedicles.
    let arch: Vec<V3> = (0..=10)
        .map(|k| {
            let a = PI * (0.12 + 0.76 * k as f32 / 10.0);
            [a.cos() * r * 0.9, 0.0, -r * 0.72 - a.sin() * r * 0.75]
        })
        .collect();
    mesh.append(&tube(&arch, 6, &SweepOptions::default(), |_| h * 0.2));
    let mut process = spike(h * 0.3, v.spine, 6, 0.3, Shading::SMOOTH);
    process.place([0.0, 0.0, -r * 1.45], vec3::normalize_or([0.0, -0.45, -1.0], vec3::neg(vec3::Z)));
    mesh.append(&process);
    for side in [-1.0, 1.0] {
        let mut wing = spike(h * 0.22, v.transverse, 6, 0.0, Shading::SMOOTH);
        wing.place([side * r * 0.9, 0.0, -r * 1.1], vec3::normalize_or([side, 0.1, -0.35], vec3::X));
        mesh.append(&wing);
    }
    mesh
}

/// A rib: a tube along `path`, `radius` across at the spine and thinning to its tip.
pub fn rib(path: &[V3], radius: f32) -> MeshBuilder {
    let smooth_path = CatmullRom::centripetal(path.to_vec()).sample_evenly(path.len() * 2);
    tube(&smooth_path, 7, &SweepOptions::default(), |t| radius * (1.0 - 0.45 * t) * (1.0 + 0.5 * (1.0 - (t * 9.0).min(1.0))))
}

/// The sternum: a manubrium, a body and a xiphoid, a flat blade `length` long standing with
/// its top at the origin, running down -Y, its front toward +Z.
pub fn sternum(length: f32, width: f32) -> MeshBuilder {
    let mut mesh = MeshBuilder::new();
    let mut manubrium = ellipsoid([width * 0.62, length * 0.1, width * 0.2]);
    manubrium.translate([0.0, -length * 0.1, 0.0]);
    let mut body = ellipsoid([width * 0.4, length * 0.3, width * 0.16]);
    body.translate([0.0, -length * 0.5, -width * 0.02]);
    let mut xiphoid = spike(width * 0.16, length * 0.14, 6, 0.0, Shading::SMOOTH);
    xiphoid.rotate_x(PI).translate([0.0, -length * 0.8, -width * 0.04]);
    mesh.append(&manubrium);
    mesh.append(&body);
    mesh.append(&xiphoid);
    mesh
}

/// A clavicle: an S-curved bar from the sternum `a` to the shoulder `b`, bowing forward near the
/// breastbone and back near the shoulder.
pub fn clavicle(a: V3, b: V3, radius: f32) -> MeshBuilder {
    let d = vec3::sub(b, a);
    let across = |t: f32, forward: f32, up: f32| vec3::add(vec3::add(a, vec3::scale(d, t)), [0.0, up, forward]);
    let reach = vec3::length(d);
    let path = CatmullRom::centripetal(vec![
        across(0.0, 0.0, 0.0),
        across(0.25, 0.1 * reach, 0.02 * reach),
        across(0.6, 0.05 * reach, 0.05 * reach),
        across(0.85, -0.05 * reach, 0.04 * reach),
        across(1.0, -0.06 * reach, 0.0),
    ])
    .sample_evenly(18);
    tube(&path, 8, &SweepOptions::default(), |t| radius * (1.25 - 0.45 * (2.0 * t - 1.0).abs()))
}

/// A shoulder blade: a flat triangular plate of height `size` lying on the back, its medial
/// border (the straight edge nearest the spine) running down through `border`, reaching out to
/// the shoulder `outward` (+1 the creature's left, -1 its right) about `size * 0.68` and forward
/// by `rise` as it goes round the ribs, with the ridge of the spine of the scapula across it.
pub fn scapula(border: V3, size: f32, outward: f32, rise: f32) -> MeshBuilder {
    let (rows, cols) = (8usize, 8usize);
    let surface = |u: f32, v: f32| -> V3 {
        // v: 0 the top of the medial border, 1 the lower tip. u: 0 on the border, 1 the lateral edge.
        let width = size * (0.62 * (1.0 - v).powf(0.9) + 0.06);
        [outward * u * width, size * (0.5 - v), rise * (u * width / (0.68 * size)).powf(1.6)]
    };
    let points: Vec<Vec<V3>> = (0..=rows).map(|r| (0..=cols).map(|c| surface(c as f32 / cols as f32, r as f32 / rows as f32)).collect()).collect();
    let mut blade = facing_sheet(&points, [0.0, 0.0, -1.0], size * 0.04);
    // The ridge across the back of the blade, running out to the acromion over the shoulder.
    let ridge: Vec<V3> = [0.0, 0.3, 0.6, 0.85, 1.0].iter().map(|&u| vec3::add(surface(u, 0.1 + 0.08 * u), [outward * 0.06 * u * size, 0.0, -0.07 * size * (1.0 - 0.6 * u)])).collect();
    let ridge = CatmullRom::centripetal(ridge).sample_evenly(14);
    blade.append(&tube(&ridge, 6, &SweepOptions::default(), |t| size * 0.035 * (1.0 + 0.8 * t)));
    blade.translate(border);
    blade
}

/// A ribcage: twelve pairs of ribs on the spine. The ribs sweep out and round to the front along
/// a half ellipse in each horizontal plane, drop as they go, and stop short of the breastbone
/// (the lower ones) or hang free (the lowest two).
#[derive(Clone, Debug, PartialEq)]
pub struct Ribcage {
    /// Where each rib leaves the spine (T1 to T12): the vertebra's centre.
    pub roots: Vec<V3>,
    /// Half width of the cage at each rib.
    pub widths: Vec<f32>,
    /// Half depth of the ellipse each rib follows (front to back), at each rib.
    pub depths: Vec<f32>,
    /// How far each rib drops between the spine and the front.
    pub drops: Vec<f32>,
    pub radius: f32,
}

/// Where a rib's ellipse starts, measured from the back (radians): out at the side of the
/// vertebra's transverse process.
const RIB_START: f32 = 0.28;
/// How far round its ellipse a rib reaches the breastbone (of a half turn).
const RIB_REACH: f32 = 0.93;

impl Ribcage {
    /// The rib `index` (0 the top) on `side` (+1 the creature's left), as a path from the spine
    /// to its tip.
    pub fn rib_path(&self, index: usize, side: f32) -> Vec<V3> {
        let n = self.roots.len();
        let root = self.roots[index];
        let (w, d, drop) = (self.widths[index], self.depths[index], self.drops[index]);
        // The upper seven reach the sternum; the next three stop at cartilage; the last two float.
        let reach = match index {
            0..=6 => RIB_REACH,
            7..=9 => 0.8 - 0.04 * (index - 7) as f32,
            _ => 0.42 - 0.04 * (index as f32 - (n as f32 - 2.0)).max(0.0),
        };
        let steps = 10;
        (0..=steps)
            .map(|k| {
                let u = k as f32 / steps as f32;
                let a = RIB_START + u * (reach * PI - RIB_START);
                [side * w * a.sin(), root[1] - drop * u.powf(1.3), root[2] - 0.1 + d * RIB_START.cos() - d * a.cos()]
            })
            .collect()
    }

    /// How many depths a full rib's front stands forward of its root less 0.1: the ellipse's
    /// depth for a front `z_front` is `(z_front - root_z + 0.1) / Ribcage::front_over_depth()`.
    pub fn front_over_depth() -> f32 {
        RIB_START.cos() - (RIB_REACH * PI).cos()
    }

    /// Every rib not `missing(index, side)`.
    pub fn mesh(&self, missing: &dyn Fn(usize, f32) -> bool) -> MeshBuilder {
        let mut mesh = MeshBuilder::new();
        for i in 0..self.roots.len() {
            for side in [-1.0, 1.0] {
                if !missing(i, side) {
                    mesh.append(&rib(&self.rib_path(i, side), self.radius));
                }
            }
        }
        mesh
    }
}

/// What a skull is like, in head space: origin at the middle of the head from crown to chin,
/// +Z the way the face looks, +Y up, +X the creature's left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkullSpec {
    /// Half the head's width, half its height from crown to chin, and half its depth.
    pub half_width: f32,
    pub half_height: f32,
    pub half_depth: f32,
    /// 0 a small, narrow chin to 1 a heavy square one.
    pub jaw: f32,
    /// How heavy the brow ridge is, 0..=1.
    pub brow: f32,
    /// Chance each tooth is gone.
    pub missing_teeth: f32,
}

/// A skull in head space, and the jaw hinge on it.
#[derive(Clone, Debug, PartialEq)]
pub struct Skull {
    pub bone: MeshBuilder,
    /// The dark of the eye sockets and the nose.
    pub sockets: MeshBuilder,
    pub teeth: MeshBuilder,
    /// The cracks laid on the cranium.
    pub cracks: MeshBuilder,
    /// The middle of the jaw's hinge (the two condyles), where the jaw turns.
    pub hinge: V3,
}

impl SkullSpec {
    /// The cranium and face as one surface: a unit sphere's point mapped to the skull's shape,
    /// a brain case above and a narrower face projecting below and in front.
    pub fn shape(&self, p: V3) -> V3 {
        let (w, hh, d) = (self.half_width, self.half_height, self.half_depth);
        let (yc, hc) = (0.4 * hh, 0.6 * hh);
        let (zc, dc) = (-0.06 * d, 0.94 * d);
        let mut q = [p[0] * w * 0.97, yc + p[1] * hc, zc + p[2] * dc];
        // The face drops from the brain case, narrows toward the teeth and juts forward.
        let fw = smooth((-p[1] - 0.12) / 0.5) * smooth(p[2] / 0.55);
        q[1] -= fw * 0.52 * hh;
        q[0] *= 1.0 - 0.3 * fw * smooth((-p[1] - 0.25) / 0.6) - 0.05 * fw;
        q[2] += 0.2 * d * fw;
        // The base of the skull is flatter behind than the dome.
        let back = smooth((-p[1] - 0.2) / 0.8) * smooth((0.25 - p[2]) / 0.9);
        q[1] += back * 0.14 * hc;
        q
    }

    /// The point of the shaped skull at longitude `u` and latitude `v` (radians), for laying
    /// cracks on it.
    pub fn surface(&self, u: f32, v: f32) -> V3 {
        let v = v.clamp(-1.4, 1.4);
        self.shape([v.cos() * u.sin(), v.sin(), v.cos() * u.cos()])
    }
}

/// A skull in head space: the cranium and face as one surface carved with two eye sockets and a
/// nose, a brow ridge, cheek arches, mastoid bumps and a row of upper teeth (some gone, by the
/// spec's chance). `cracks` of them are laid across the dome. The jaw is [`mandible`].
pub fn skull(spec: &SkullSpec, cracks: usize, rng: &mut Mulberry32) -> Skull {
    let (w, hh, d) = (spec.half_width, spec.half_height, spec.half_depth);
    let mut cranium = icosphere(1.0, 4);
    for p in &mut cranium.positions {
        *p = spec.shape(*p);
    }
    cranium.recompute_smooth_normals();

    // The surface point nearest `(x, y)` among the front of the face, and its normal.
    let front = |x: f32, y: f32| -> (V3, V3) {
        let mut best = (f32::MAX, cranium.positions[0], cranium.normals[0]);
        for (p, n) in cranium.positions.iter().zip(&cranium.normals) {
            if p[2] > 0.2 * d {
                let dist = (p[0] - x).powi(2) + (p[1] - y).powi(2);
                if dist < best.0 {
                    best = (dist, *p, *n);
                }
            }
        }
        (best.1, best.2)
    };
    let mut hollows = Vec::new();
    for side in [-1.0, 1.0] {
        let (p, n) = front(side * 0.43 * w, -0.02 * hh);
        let r = 0.27 * w;
        hollows.push(Hollow { center: vec3::add(p, vec3::scale(vec3::normalize_or(vec3::add(n, [0.0, 0.0, 1.0]), vec3::Z), r * 0.55)), radius: r });
    }
    let (p, n) = front(0.0, -0.36 * hh);
    let r = 0.17 * w;
    hollows.push(Hollow { center: vec3::add(p, vec3::scale(vec3::normalize_or(n, vec3::Z), r * 0.5)), radius: r });
    let (mut bone, sockets) = carve_hollows(&cranium, &hollows);

    // Brow ridge over the sockets, cheek arches under them, mastoid bumps behind the ears.
    let brow_y = 0.2 * hh;
    let brow: Vec<V3> = (0..=12)
        .map(|k| {
            let x = (k as f32 / 12.0 * 2.0 - 1.0) * 0.8 * w;
            let (p, _) = front(x, brow_y);
            [x, brow_y + 0.04 * hh * (1.0 - (x / w).powi(2)), p[2] + 0.01 * d]
        })
        .collect();
    bone.append(&tube(&brow, 8, &SweepOptions::default(), |t| (0.04 + 0.05 * spec.brow) * hh * (1.0 - 0.5 * (2.0 * t - 1.0).powi(4))));
    for side in [-1.0, 1.0] {
        let (cheek, _) = front(side * 0.48 * w, -0.3 * hh);
        let arch = CatmullRom::centripetal(vec![
            [side * 0.46 * w, cheek[1], cheek[2] - 0.02 * d],
            [side * 0.74 * w, -0.2 * hh, 0.4 * d],
            [side * 0.88 * w, -0.12 * hh, 0.0],
            [side * 0.86 * w, -0.1 * hh, -0.28 * d],
        ])
        .sample_evenly(14);
        bone.append(&tube(&arch, 7, &SweepOptions::default(), |t| 0.085 * hh * (1.0 - 0.3 * t)));
        // The cheekbone: a prominent knob under the socket's outer corner, where the arch starts, so the
        // face reads as cheeks and not as a mask.
        let (cb, cn) = front(side * 0.6 * w, -0.34 * hh);
        let mut cheekbone = ellipsoid([0.15 * w, 0.085 * hh, 0.1 * d]);
        cheekbone.rotate_z(side * 0.35).rotate_y(side * 0.7).translate(vec3::add(cb, vec3::scale(vec3::normalize_or(cn, vec3::Z), 0.02 * d)));
        bone.append(&cheekbone);
        let mut mastoid = spike(0.1 * w, 0.2 * hh, 7, -0.5, Shading::SMOOTH);
        mastoid.rotate_x(PI).translate([side * 0.8 * w, -0.05 * hh, -0.32 * d]);
        bone.append(&mastoid);
    }

    // The maxilla: the block of the upper jaw between the cheekbones, under the nose, that the upper
    // teeth set in, with the ridges of the canine roots over them.
    let (mp, mn) = front(0.0, -0.58 * hh);
    let mut maxilla = ellipsoid([0.36 * w, 0.17 * hh, 0.15 * d]);
    maxilla.translate(vec3::add(mp, vec3::scale(vec3::normalize_or(mn, vec3::Z), 0.02 * d)));
    bone.append(&maxilla);
    for side in [-1.0, 1.0] {
        let (rp, rn) = front(side * 0.2 * w, -0.54 * hh);
        let mut root = ellipsoid([0.05 * w, 0.08 * hh, 0.05 * d]);
        root.translate(vec3::add(rp, vec3::scale(vec3::normalize_or(rn, vec3::Z), 0.01 * d)));
        bone.append(&root);
    }

    // Upper teeth hang from the lowest edge of the face.
    let mut teeth = MeshBuilder::new();
    let count = 14usize;
    let floor = |x: f32, z: f32| -> f32 {
        cranium
            .positions
            .iter()
            .filter(|p| (p[0] - x).abs() < 0.12 * w && (p[2] - z).abs() < 0.2 * d && p[1] < -0.2 * hh)
            .map(|p| p[1])
            .fold(f32::MAX, f32::min)
    };
    for k in 0..count {
        if rng.chance(spec.missing_teeth as f64) {
            continue;
        }
        let a = (k as f32 / (count - 1) as f32 * 2.0 - 1.0) * 1.2;
        let (x, z) = (a.sin() * 0.3 * w, 0.6 * d + (a.cos() - 1.0) * 0.42 * d);
        let y = floor(x, z);
        if y == f32::MAX {
            continue;
        }
        let size = if (k as i32 - 6).abs() <= 1 { 1.1 } else { 0.9 };
        let mut t = tooth(0.17 * w * size, 0.2 * hh * size, 0.12 * w);
        t.rotate_x(PI).rotate_y(-a * 0.8);
        t.translate([x, y + 0.05 * hh, z]);
        teeth.append(&t);
    }

    // Cracks across the dome.
    let mut crack_mesh = MeshBuilder::new();
    for _ in 0..cracks {
        let start = [rng.range_f32(-1.2, 1.2), rng.range_f32(0.2, 0.9)];
        let heading = rng.range_f32(0.0, TAU);
        let surface = |u: f32, v: f32| {
            let p = spec.surface(u, v);
            let c = [0.0, 0.4 * hh, -0.06 * d];
            vec3::add(p, vec3::scale(vec3::normalize_or(vec3::sub(p, c), vec3::Y), 0.004 * hh))
        };
        let steps = rng.int(6, 12) as usize;
        crack_mesh.append(&crack_walk(rng, &surface, start, heading, steps, 0.15, 0.024 * hh));
    }

    let hinge = [0.0, -0.04 * hh, -0.2 * d];
    Skull { bone, sockets, teeth, cracks: crack_mesh, hinge }
}

/// A jaw about its hinge: the mandible as the bone and its lower teeth, both relative to the
/// hinge (the origin is where the jaw turns). The jaw's width, depth and chin come from the
/// same [`SkullSpec`] as the skull it fits.
pub fn mandible(spec: &SkullSpec, hinge: V3, rng: &mut Mulberry32) -> (MeshBuilder, MeshBuilder) {
    let (w, hh, d) = (spec.half_width, spec.half_height, spec.half_depth);
    let chin = 0.6 + 0.12 * spec.jaw;
    let body_y = -0.8 * hh;
    let points = vec![
        [-0.82 * w, hinge[1] + 0.12 * hh, hinge[2]],
        [-0.8 * w, hinge[1] - 0.25 * hh, hinge[2] - 0.04 * d],
        [-0.76 * w, body_y + 0.04 * hh, -0.08 * d],
        [-0.56 * w, body_y, 0.3 * d],
        [-0.28 * w, body_y - 0.02 * hh, 0.55 * d],
        [0.0, body_y - 0.03 * hh, chin * d],
        [0.28 * w, body_y - 0.02 * hh, 0.55 * d],
        [0.56 * w, body_y, 0.3 * d],
        [0.76 * w, body_y + 0.04 * hh, -0.08 * d],
        [0.8 * w, hinge[1] - 0.25 * hh, hinge[2] - 0.04 * d],
        [0.82 * w, hinge[1] + 0.12 * hh, hinge[2]],
    ];
    let path = CatmullRom::centripetal(points).sample_evenly(48);
    let mut bone = tube(&path, 9, &SweepOptions::default(), |t| {
        // Slender at the condyles, thick along the body of the jaw.
        let body = ((1.0 - (2.0 * t - 1.0).abs()) / 0.3).min(1.0);
        hh * (0.06 + 0.05 * body + 0.03 * spec.jaw * body)
    });
    // Chin, coronoid processes and condyle knobs.
    let mut chin_bulge = uv_sphere(0.09 * hh, 10, 8);
    chin_bulge.scale([1.8 + 1.4 * spec.jaw, 1.1 + 0.4 * spec.jaw, 1.0]).translate([0.0, body_y - 0.03 * hh, chin * d + 0.03 * d]);
    bone.append(&chin_bulge);
    for side in [-1.0, 1.0] {
        // The ramus: the broad plate that rises from the angle of the jaw to the condyle behind and
        // the coronoid process in front, with a notch between them.
        let rows = 6usize;
        let ramus: Vec<Vec<V3>> = (0..=rows)
            .map(|r| {
                let u = r as f32 / rows as f32;
                let y = body_y + 0.04 * hh + u * (hinge[1] + 0.1 * hh - body_y - 0.04 * hh);
                // Rear edge from the angle up to the condyle; front edge from the body up to the coronoid.
                let back = -0.1 * d + (hinge[2] - 0.02 * d + 0.1 * d) * u;
                let front = 0.3 * d + (hinge[2] + 0.17 * d - 0.3 * d) * u.powf(1.4);
                // The notch: the middle of the top row sinks.
                (0..=6)
                    .map(|c| {
                        let v = c as f32 / 6.0;
                        let dip = if u > 0.7 { 0.5 * hh * 0.25 * (u - 0.7) / 0.3 * (PI * v).sin() } else { 0.0 };
                        [side * (0.8 * w + 0.03 * w * (PI * v).sin()), y - dip, back + (front - back) * v]
                    })
                    .collect()
            })
            .collect();
        bone.append(&facing_sheet(&ramus, [side, 0.0, 0.0], 0.05 * hh));
        // The angle of the jaw, a rounded corner under the ear, bolder with a heavy jaw.
        let mut gonion = uv_sphere(0.1 * hh * (1.0 + 0.5 * spec.jaw), 10, 8);
        gonion.scale([0.8, 1.0, 1.0]).translate([side * 0.78 * w, body_y + 0.02 * hh, -0.1 * d]);
        bone.append(&gonion);
        let mut condyle = uv_sphere(0.07 * hh, 10, 8);
        condyle.scale([1.0, 0.9, 1.0]).translate([side * 0.82 * w, hinge[1] + 0.12 * hh, hinge[2]]);
        bone.append(&condyle);
        let mut coronoid = spike(0.05 * hh, 0.3 * hh, 7, 0.0, Shading::SMOOTH);
        coronoid.rotate_z(-side * 0.1).translate([side * 0.8 * w, hinge[1] - 0.2 * hh, hinge[2] + 0.12 * d]);
        bone.append(&coronoid);
    }
    // Lower teeth stand along the front of the body of the jaw.
    let mut teeth = MeshBuilder::new();
    let body: Vec<V3> = path.iter().copied().filter(|p| p[2] > 0.3 * d).collect();
    let count = 12usize;
    for k in 0..count {
        if rng.chance(spec.missing_teeth as f64) || body.len() < 2 {
            continue;
        }
        let f = k as f32 / (count - 1) as f32 * (body.len() - 1) as f32;
        let i = (f.floor() as usize).min(body.len() - 2);
        let p = vec3::lerp(body[i], body[i + 1], f - i as f32);
        let size = if (k as i32 - 5).abs() <= 1 { 0.95 } else { 0.85 };
        let mut t = tooth(0.15 * w * size, 0.14 * hh * size, 0.11 * w);
        t.translate([p[0], p[1] + 0.05 * hh, p[2]]);
        teeth.append(&t);
    }
    bone.translate(vec3::neg(hinge));
    teeth.translate(vec3::neg(hinge));
    (bone, teeth)
}

/// What a pelvis is like, in pelvis space: origin midway between the two hip sockets, +Y up, +Z
/// the way the creature faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pelvis {
    /// Half the distance between the hip sockets.
    pub hip_x: f32,
    /// Half the width across the tops of the hip blades.
    pub crest_x: f32,
    /// How high the crest stands over the hip sockets.
    pub crest_y: f32,
    /// Where the sacrum lies behind the sockets (negative) and the pubic joint before them.
    pub back_z: f32,
    pub front_z: f32,
    /// How far below the hip sockets the sitting bones and pubis reach.
    pub drop: f32,
    /// How thick the bone is, as a radius.
    pub thickness: f32,
}

/// A pelvis: two iliac wings fanning from the sockets up to curving crests that end in a spine
/// at the front and one at the back, the sacrum wedged between their backs (a spade of five
/// fused vertebrae that narrows to the coccyx), and in front the pubic bones, which meet at the
/// symphysis and leave the arch beneath it, looping with the sitting bones back round each socket.
pub fn pelvis(spec: &Pelvis) -> MeshBuilder {
    let mut mesh = MeshBuilder::new();
    let (hx, cx, cy, t) = (spec.hip_x, spec.crest_x, spec.crest_y, spec.thickness);
    for side in [-1.0f32, 1.0] {
        // The wing: rows climb from the socket to the crest, columns run front to back. The crest
        // runs from the front spine (which juts forward) over the iliac tubercle, high and wide,
        // to the back spine by the sacrum; the wing is broad at the top and narrow at the socket.
        let crest = CatmullRom::centripetal(vec![
            [side * cx * 0.9, cy * 0.7, spec.front_z * 1.08],
            [side * cx * 1.02, cy * 0.92, spec.front_z * 0.4],
            [side * cx * 1.08, cy * 1.02, spec.back_z * 0.15],
            [side * cx * 0.9, cy * 0.98, spec.back_z * 0.6],
            [side * hx * 0.62, cy * 0.8, spec.back_z * 1.0],
        ])
        .sample_evenly(17);
        let (front, back) = ([side * hx * 1.02, 0.1, spec.front_z * 0.5], [side * hx * 0.7, 0.15, spec.back_z * 0.55]);
        let last = (crest.len() - 1) as f32;
        let rows = 10usize;
        let points: Vec<Vec<V3>> = (0..=rows)
            .map(|r| {
                let u = r as f32 / rows as f32;
                crest
                    .iter()
                    .enumerate()
                    .map(|(c, top)| {
                        let v = c as f32 / last;
                        let bottom = vec3::lerp(front, back, v);
                        let mut p = vec3::lerp(bottom, *top, u);
                        // The outer face bulges, the blade's middle thinner toward the notch behind
                        // the socket; its front third turns forward and its back third in toward the sacrum.
                        let belly = (PI * u).sin() * (1.0 - (2.0 * v - 1.0).powi(2));
                        p[0] += side * 0.2 * belly;
                        p[2] += 0.18 * spec.front_z * u * (1.0 - v).powi(2) - 0.1 * spec.front_z.abs() * u * v * v;
                        p
                    })
                    .collect()
            })
            .collect();
        mesh.append(&facing_sheet(&points, [side, 0.0, 0.0], t * 1.15));
        // A thick, rounded crest, and the spine at its front end.
        mesh.append(&tube(&crest, 7, &SweepOptions::default(), |_| t * 1.2));
        let mut asis = uv_sphere(t * 1.7, 8, 6);
        asis.translate(crest[0]);
        mesh.append(&asis);
        // The pillar of the wing, from the socket up to the middle of the crest.
        let pillar = CatmullRom::centripetal(vec![[side * hx, 0.0, 0.0], [side * (hx + 0.35 * (cx - hx)), cy * 0.4, spec.front_z * 0.15], [side * cx * 0.98, cy * 0.9, spec.front_z * 0.1]]).sample_evenly(10);
        mesh.append(&tube(&pillar, 8, &SweepOptions::default(), |s| t * (2.2 - 0.9 * s)));

        // The socket: a thick ring the thigh's head turns in.
        let mut socket = uv_sphere(t * 2.0, 10, 8);
        socket.scale([0.8, 1.0, 1.0]).translate([side * hx * 1.04, 0.0, 0.0]);
        mesh.append(&socket);

        // The pubic bone runs from the socket forward and down to the symphysis; its lower branch
        // turns back out to the sitting bone, and the ischium rises from that to the socket's back.
        let symphysis = [side * 0.07, -spec.drop * 0.5, spec.front_z * 1.0];
        let tuber = [side * hx * 0.88, -spec.drop * 1.0, spec.back_z * 0.2];
        let upper = CatmullRom::centripetal(vec![[side * hx * 0.98, -0.1, spec.front_z * 0.35], [side * hx * 0.55, -spec.drop * 0.28, spec.front_z * 0.85], symphysis]).sample_evenly(14);
        mesh.append(&tube(&upper, 8, &SweepOptions::default(), |_| t * 1.1));
        let lower = CatmullRom::centripetal(vec![symphysis, [side * hx * 0.38, -spec.drop * 0.82, spec.front_z * 0.72], [side * hx * 0.7, -spec.drop * 1.0, spec.front_z * 0.2], tuber]).sample_evenly(16);
        mesh.append(&tube(&lower, 8, &SweepOptions::default(), |_| t * 1.0));
        let ischium = CatmullRom::centripetal(vec![[side * hx * 1.0, -0.1, spec.back_z * 0.3], [side * hx * 0.98, -spec.drop * 0.55, spec.back_z * 0.4], tuber]).sample_evenly(10);
        mesh.append(&tube(&ischium, 8, &SweepOptions::default(), |_| t * 1.25));
        let mut tuberosity = uv_sphere(t * 1.7, 8, 6);
        tuberosity.scale([0.9, 1.1, 1.2]).translate(tuber);
        mesh.append(&tuberosity);
    }
    // The pubic joint: a pad of cartilage's bone closing the two pubic bones in front.
    let mut joint = uv_sphere(t * 1.5, 8, 6);
    joint.scale([1.6, 1.3, 1.0]).translate([0.0, -spec.drop * 0.5, spec.front_z * 1.0]);
    mesh.append(&joint);
    // The sacrum: a spade of five fused vertebrae wedged between the backs of the wings, broad at
    // the top, curving forward as it narrows to the tip, with the coccyx hanging on.
    let (top, tip) = ([0.0, cy * 0.72, spec.back_z * 0.9], [0.0, -spec.drop * 0.25, spec.back_z * 0.35]);
    let spine = CatmullRom::centripetal(vec![top, [0.0, cy * 0.4, spec.back_z * 0.96], [0.0, cy * 0.08, spec.back_z * 0.78], tip]).sample_evenly(5);
    for (k, p) in spine.iter().enumerate() {
        let s = k as f32 / 4.0;
        let mut segment = ellipsoid([hx * (0.86 - 0.5 * s), cy * 0.14, t * (2.4 - 0.6 * s)]);
        segment.rotate_x(-0.35 + 0.5 * s).translate(*p);
        mesh.append(&segment);
    }
    // The wings of the sacrum reach out to the backs of the iliac wings.
    for side in [-1.0f32, 1.0] {
        let mut ala = ellipsoid([hx * 0.4, cy * 0.2, t * 2.0]);
        ala.rotate_z(side * 0.25).translate([side * hx * 0.62, cy * 0.66, spec.back_z * 0.94]);
        mesh.append(&ala);
    }
    let mut coccyx = spike(t * 1.3, spec.drop * 0.55, 7, 0.0, Shading::SMOOTH);
    coccyx.rotate_x(PI + 0.55).translate(vec3::add(tip, [0.0, 0.05, 0.0]));
    mesh.append(&coccyx);
    mesh
}

/// How a hand is built, in hand space: origin at the wrist, +Y toward the fingers, +Z the back of
/// the hand, the thumb toward -X (a right hand; mirror it for a left).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hand {
    /// From wrist to the tip of the middle finger.
    pub length: f32,
    /// Half the width across the knuckles.
    pub half_width: f32,
    /// How far the fingers curl, 0 straight to 1 closed.
    pub curl: f32,
    /// Which of the five digits (thumb first) have gone, as a bit mask.
    pub missing: u8,
}

/// A hand of separate bones: carpals, five metacarpals, and a phalanx at a time out to each
/// fingertip, curling together toward the palm (-Z).
pub fn hand(spec: &Hand, rng: &mut Mulberry32) -> MeshBuilder {
    let l = spec.length;
    let r = 0.027 * l;
    let mut mesh = MeshBuilder::new();
    // Carpals: a clump of small stones at the wrist.
    for k in 0..6 {
        let x = (k as f32 - 2.5) * 0.085 * l + rng.range_f32(-0.01, 0.01);
        let mut stone = uv_sphere(0.07 * l, 8, 6);
        stone.scale([1.0, 0.85, 0.7]).translate([x, 0.07 * l + 0.02 * l * (k % 2) as f32, 0.0]);
        mesh.append(&stone);
    }
    let bend = |k: usize, curl: f32| curl * [0.55, 0.9, 0.7][k.min(2)];
    // (x of the knuckle in half widths, length scale)
    let fingers = [(-0.72f32, 0.92f32), (-0.24, 1.0), (0.24, 0.93), (0.72, 0.74)];
    for (i, &(kx, scale)) in fingers.iter().enumerate() {
        let knuckle = [kx * spec.half_width, 0.5 * l, 0.0];
        let base = [kx * spec.half_width * 0.45, 0.13 * l, 0.0];
        mesh.append(&long_bone(base, knuckle, &LongBone::plain([r * 1.35, r * 1.5], r * 0.95)));
        if spec.missing & (1 << (i + 1)) != 0 {
            continue;
        }
        let mut at = knuckle;
        let mut angle = 0.0;
        for (k, share) in [0.5f32, 0.29, 0.21].into_iter().enumerate() {
            let len = 0.5 * l * share * 1.02 * scale;
            angle += bend(k, spec.curl + 0.28) * (0.9 + 0.2 * rng.range_f32(0.0, 1.0));
            let end = [at[0], at[1] + angle.cos() * len, at[2] - angle.sin() * len];
            let size = 1.0 - 0.12 * k as f32;
            mesh.append(&long_bone(at, end, &LongBone::plain([r * 1.3 * size, r * 1.15 * size], r * 0.8 * size)));
            at = end;
        }
    }
    // The thumb: a metacarpal that flares out and forward, then two phalanges.
    let base = [-0.28 * spec.half_width, 0.1 * l, -0.01 * l];
    let joint = [-1.25 * spec.half_width, 0.33 * l, -0.07 * l - 0.05 * l * spec.curl];
    mesh.append(&long_bone(base, joint, &LongBone::plain([r * 1.5, r * 1.5], r * 1.05)));
    if spec.missing & 1 == 0 {
        let mid = [joint[0] - 0.1 * l, joint[1] + 0.14 * l, joint[2] - 0.1 * l * (0.4 + spec.curl)];
        let tip = [mid[0] - 0.06 * l, mid[1] + 0.1 * l, mid[2] - 0.11 * l * (0.5 + spec.curl)];
        mesh.append(&long_bone(joint, mid, &LongBone::plain([r * 1.4, r * 1.25], r * 0.9)));
        mesh.append(&long_bone(mid, tip, &LongBone::plain([r * 1.2, r * 1.0], r * 0.75)));
    }
    mesh
}

/// A hand closed round a grip: the same carpals and metacarpals as [`hand`], and each finger and
/// the thumb laid along the joints of `form`'s digit curled `fingers[i]` and `thumb` radians per
/// joint ([`Digit::joints`], the curl `fb_body`'s gripping hand and glove take), so the bony
/// fingers lie where a gloved hand's would and a weapon in the fist sits in them. In hand space,
/// like [`hand`]; `spec.curl` is not used.
pub fn hand_gripping(spec: &Hand, form: &HandForm, fingers: [f32; 4], thumb: f32, rng: &mut Mulberry32) -> MeshBuilder {
    let l = spec.length;
    let r = 0.027 * l;
    let mut mesh = MeshBuilder::new();
    for k in 0..6 {
        let x = (k as f32 - 2.5) * 0.085 * l + rng.range_f32(-0.01, 0.01);
        let mut stone = uv_sphere(0.07 * l, 8, 6);
        stone.scale([1.0, 0.85, 0.7]).translate([x, 0.07 * l + 0.02 * l * (k % 2) as f32, 0.0]);
        mesh.append(&stone);
    }
    for (i, digit) in form.fingers.iter().enumerate() {
        let joints = digit.joints(fingers[i]);
        let base = [joints[0][0] * 0.45, 0.13 * l, 0.0];
        mesh.append(&long_bone(base, joints[0], &LongBone::plain([r * 1.35, r * 1.5], r * 0.95)));
        if spec.missing & (1 << (i + 1)) != 0 {
            continue;
        }
        for (k, pair) in joints.windows(2).enumerate() {
            let size = 1.0 - 0.12 * k as f32;
            mesh.append(&long_bone(pair[0], pair[1], &LongBone::plain([r * 1.3 * size, r * 1.15 * size], r * 0.8 * size)));
        }
    }
    let joints = form.thumb.joints(thumb);
    let base = [-0.28 * spec.half_width, 0.1 * l, -0.01 * l];
    mesh.append(&long_bone(base, joints[0], &LongBone::plain([r * 1.5, r * 1.5], r * 1.05)));
    if spec.missing & 1 == 0 {
        for (k, pair) in joints.windows(2).enumerate() {
            let size = 1.0 - 0.14 * k as f32;
            mesh.append(&long_bone(pair[0], pair[1], &LongBone::plain([r * 1.4 * size, r * 1.25 * size], r * 0.9 * size)));
        }
    }
    mesh
}

/// How a foot is built, in foot space: origin at the ankle, +Z toward the toes, the sole flat on
/// the plane `y = -ankle_height`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Foot {
    /// Ankle to the sole.
    pub ankle_height: f32,
    /// How far behind the ankle the heel reaches.
    pub heel: f32,
    /// Ankle to the tip of the longest toe.
    pub toe: f32,
    /// Half the width across the ball of the foot.
    pub half_width: f32,
}

/// A foot of separate bones: heel and ankle bones, five metatarsals fanning forward and a
/// phalanx at a time out to each toe, all resting on the sole's plane.
pub fn foot(spec: &Foot) -> MeshBuilder {
    let (ah, hw) = (spec.ankle_height, spec.half_width);
    let sole = -ah;
    let r = 0.055 * hw / 0.45;
    let mut mesh = MeshBuilder::new();
    let mut talus = uv_sphere(1.0, 12, 9);
    talus.scale([0.34 * hw / 0.45, 0.26, 0.36]).translate([0.0, -0.22, 0.1]);
    mesh.append(&talus);
    let mut heel = uv_sphere(1.0, 12, 9);
    heel.scale([0.26 * hw / 0.45, 0.3, 0.52]).translate([0.0, sole + 0.3, -spec.heel * 0.45]);
    mesh.append(&heel);
    let mut midfoot = uv_sphere(1.0, 12, 9);
    midfoot.scale([0.36 * hw / 0.45, 0.2, 0.3]).translate([0.0, sole + 0.34, 0.36]);
    mesh.append(&midfoot);
    let ball_z = 0.55 * spec.toe + 0.1;
    let toe_z = [1.0f32, 0.97, 0.9, 0.82, 0.74];
    for (k, &reach) in toe_z.iter().enumerate() {
        let x = (k as f32 - 2.0) * hw * 0.42;
        let start = [x * 0.7, sole + r * 3.0, 0.4];
        let ball = [x, sole + r * 1.25, ball_z + 0.04 * (k as f32 - 1.0).abs()];
        mesh.append(&long_bone(start, ball, &LongBone::plain([r * 1.4, r * 1.7], r * 1.1)));
        // Toes: two phalanges for the big toe, three for the rest.
        let tip = spec.toe * reach + 0.02;
        let len = (tip - ball[2]).max(0.1);
        let shares: &[f32] = if k == 0 { &[0.55, 0.45] } else { &[0.42, 0.33, 0.25] };
        let mut at = ball;
        for (j, share) in shares.iter().enumerate() {
            let end = [at[0], sole + r * 1.1, at[2] + len * share];
            let size = if k == 0 { 1.2 } else { 1.0 } * (1.0 - 0.1 * j as f32);
            mesh.append(&long_bone(at, end, &LongBone::plain([r * 1.2 * size, r * 1.1 * size], r * 0.85 * size)));
            at = end;
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    fn valid(mesh: MeshBuilder, name: &'static str) {
        assert!(!mesh.is_empty(), "{name} is empty");
        mesh.into_part(name, OutlineMode::Welded).validate().unwrap();
    }

    fn rng() -> Mulberry32 {
        Mulberry32::new(7)
    }

    #[test]
    fn a_long_bone_runs_between_its_points() {
        let (a, b) = ([1.0, 5.0, -2.0], [1.5, 1.0, -1.0]);
        let mesh = long_bone(a, b, &LongBone { ends: [0.2, 0.25], shaft: 0.1, lobes: [0.0, 0.12], side: vec3::X });
        valid(mesh.clone(), "bone");
        let along = vec3::normalize_or(vec3::sub(b, a), vec3::Y);
        let length = vec3::distance(a, b);
        for p in &mesh.positions {
            let t = vec3::dot(vec3::sub(*p, a), along);
            assert!(t > -0.01 * length && t < 1.01 * length, "{p:?} is off the bone ({t} of {length})");
            let off = vec3::length(vec3::sub(vec3::sub(*p, a), vec3::scale(along, t)));
            assert!(off < 0.45, "{p:?} stands {off} off the axis");
        }
        // Knobbed ends: wider there than in the middle.
        let plain = LongBone::plain([0.2, 0.2], 0.1);
        assert!(plain.radius_at(0.05) > 1.5 * plain.radius_at(0.5));
    }

    #[test]
    fn weathering_roughens_without_tearing() {
        let bone = LongBone::plain([0.2, 0.2], 0.1);
        let mut mesh = long_bone([0.0; 3], [0.0, 4.0, 0.0], &bone);
        let before = mesh.clone();
        weather(&mut mesh, 3, 0.03);
        assert_ne!(mesh.positions, before.positions);
        assert!(mesh.positions.iter().zip(&before.positions).all(|(a, b)| vec3::distance(*a, *b) <= 0.05));
        // Vertices sharing a place (a lathe's seam) still share it.
        for (i, p) in before.positions.iter().enumerate() {
            for (j, q) in before.positions.iter().enumerate().skip(i + 1).take(40) {
                if vec3::distance(*p, *q) < 1e-6 && vec3::distance(before.normals[i], before.normals[j]) < 1e-3 {
                    assert!(vec3::distance(mesh.positions[i], mesh.positions[j]) < 1e-5, "seam {i} {j} tore");
                }
            }
        }
        valid(mesh, "bone");
    }

    #[test]
    fn a_skull_has_sockets_teeth_and_a_jaw_that_fits() {
        let spec = SkullSpec { half_width: 0.9, half_height: 1.17, half_depth: 1.04, jaw: 0.5, brow: 0.5, missing_teeth: 0.0 };
        let skull = skull(&spec, 2, &mut rng());
        valid(skull.bone.clone(), "skull");
        valid(skull.sockets.clone(), "sockets");
        valid(skull.teeth.clone(), "teeth");
        valid(skull.cracks.clone(), "cracks");
        let (lo, hi) = skull.bone.bounds().unwrap();
        // The crown is at the top of the head, the face drops well below the eyes.
        assert!((hi[1] - spec.half_height).abs() < 0.12, "crown at {}", hi[1]);
        assert!(lo[1] < -0.45 * spec.half_height, "the face stops at {}", lo[1]);
        assert!(hi[0] < spec.half_width * 1.05 && lo[0] > -spec.half_width * 1.05);
        let (jaw, jaw_teeth) = mandible(&spec, skull.hinge, &mut rng());
        valid(jaw.clone(), "jaw");
        valid(jaw_teeth, "teeth");
        // The jaw's chin ends at the bottom of the head.
        let (jlo, _) = jaw.bounds().unwrap();
        assert!((jlo[1] + skull.hinge[1] + spec.half_height).abs() < 0.2 * spec.half_height, "chin at {}", jlo[1] + skull.hinge[1]);
        // Teeth can all be gone.
        let bare = super::skull(&SkullSpec { missing_teeth: 1.0, ..spec }, 0, &mut rng());
        assert!(bare.teeth.is_empty() && bare.cracks.is_empty());
    }

    #[test]
    fn ribs_are_missing_when_asked() {
        let cage = Ribcage {
            roots: (0..12).map(|k| [0.0, 14.0 - 0.28 * k as f32, -0.6]).collect(),
            widths: vec![1.2; 12],
            depths: vec![0.5; 12],
            drops: vec![0.8; 12],
            radius: 0.06,
        };
        let all = cage.mesh(&|_, _| false);
        let some = cage.mesh(&|i, side| i < 3 && side > 0.0);
        valid(all.clone(), "ribs");
        assert!(some.vertex_count() < all.vertex_count());
        assert!(cage.mesh(&|_, _| true).is_empty());
        // The sides are mirrors, and a full rib's front stands where its depth says.
        let (right, left) = (cage.rib_path(2, -1.0), cage.rib_path(2, 1.0));
        for (r, l) in right.iter().zip(&left) {
            assert!((r[0] + l[0]).abs() < 1e-5 && (r[1] - l[1]).abs() < 1e-5 && (r[2] - l[2]).abs() < 1e-5);
        }
        let front = right.last().unwrap()[2];
        assert!((front - (-0.6 - 0.1 + 0.5 * Ribcage::front_over_depth())).abs() < 1e-4, "{front}");
    }

    #[test]
    fn a_hand_without_fingers_has_fewer_bones() {
        let spec = Hand { length: 1.8, half_width: 0.42, curl: 0.2, missing: 0 };
        let whole = hand(&spec, &mut rng());
        let short = hand(&Hand { missing: 0b00110, ..spec }, &mut rng());
        valid(whole.clone(), "hand");
        valid(short.clone(), "hand");
        assert!(short.vertex_count() < whole.vertex_count());
        // It reaches about its length from the wrist.
        let (lo, hi) = whole.bounds().unwrap();
        assert!(lo[1] > -0.2 && hi[1] > 0.8 * spec.length && hi[1] < 1.2 * spec.length, "{lo:?} {hi:?}");
    }

    #[test]
    fn a_gripping_hand_lies_along_the_forms_curled_digits() {
        let form = HandForm::mannequin(1.0, 0.1);
        let spec = Hand { length: 1.77, half_width: form.knuckle_half_width, curl: 0.2, missing: 0 };
        let open = hand(&spec, &mut rng());
        let curls = [1.0, 1.0, 1.0, 1.0];
        let closed = hand_gripping(&spec, &form, curls, 0.75, &mut rng());
        valid(closed.clone(), "hand");
        // Closed, it is shorter than the open hand and reaches toward the palm (-Z), as its fingertips do.
        let (_, open_hi) = open.bounds().unwrap();
        let (closed_lo, closed_hi) = closed.bounds().unwrap();
        assert!(closed_hi[1] < open_hi[1] - 0.15, "{closed_hi:?} vs {open_hi:?}");
        assert!(closed_lo[2] < -0.5 * form.fingers[1].length * 0.5, "{closed_lo:?}");
        // Each fingertip bone ends at its digit's last joint, curled.
        for (i, digit) in form.fingers.iter().enumerate() {
            let tip = *digit.joints(curls[i]).last().unwrap();
            let near = closed.positions.iter().map(|p| vec3::distance(*p, tip)).fold(f32::MAX, f32::min);
            assert!(near < 0.12, "finger {i}: nothing within {near} of its tip");
        }
        // Missing fingers are missing here too.
        let short = hand_gripping(&Hand { missing: 0b00110, ..spec }, &form, curls, 0.75, &mut rng());
        assert!(short.vertex_count() < closed.vertex_count());
    }

    #[test]
    fn a_pelvis_has_wings_a_sacrum_and_an_arch_under_the_pubic_joint() {
        let spec = Pelvis { hip_x: 0.86, crest_x: 1.45, crest_y: 1.3, back_z: -0.6, front_z: 0.55, drop: 1.1, thickness: 0.09 };
        let pelvis = pelvis(&spec);
        let near = |p: V3, r: f32| pelvis.positions.iter().any(|q| vec3::distance(p, *q) < r);
        // The crests stand wide and high above the sockets, either side.
        for side in [-1.0f32, 1.0] {
            assert!(near([side * spec.crest_x * 1.05, spec.crest_y, spec.back_z * 0.1], 0.25), "no wing crest on {side}");
            // A sitting bone below and behind each socket.
            assert!(near([side * spec.hip_x * 0.88, -spec.drop, spec.back_z * 0.2], 0.2), "no sitting bone on {side}");
        }
        // The sacrum between the wings at the back, narrowing to a tip low down.
        assert!(near([0.0, spec.crest_y * 0.4, spec.back_z * 0.96], 0.3));
        assert!(near([0.0, -spec.drop * 0.25, spec.back_z * 0.35], 0.3));
        // The pubic joint in front, with the arch open beneath it: bone either side, none on the midline.
        assert!(near([0.0, -spec.drop * 0.5, spec.front_z], 0.2));
        assert!(near([spec.hip_x * 0.4, -spec.drop * 0.82, spec.front_z * 0.72], 0.2));
        assert!(!near([0.0, -spec.drop * 0.85, spec.front_z * 0.8], 0.1), "the arch is closed");
    }

    #[test]
    fn a_skull_has_cheekbones_and_a_jaw_with_rami() {
        let spec = SkullSpec { half_width: 0.9, half_height: 1.17, half_depth: 1.04, jaw: 1.0, brow: 0.5, missing_teeth: 0.0 };
        let narrow = SkullSpec { jaw: 0.0, ..spec };
        let skull = skull(&spec, 0, &mut rng());
        // Cheekbones stand out to either side of the face below the sockets.
        let (lo, hi) = skull.bone.bounds().unwrap();
        let cheek = |side: f32| skull.bone.positions.iter().filter(|p| p[2] > 0.3 * spec.half_depth && p[1] < -0.2 * spec.half_height && p[1] > -0.5 * spec.half_height).map(|p| p[0] * side).fold(f32::MIN, f32::max);
        assert!(cheek(1.0) > 0.55 * spec.half_width && cheek(-1.0) > 0.55 * spec.half_width, "{} {}", cheek(1.0), cheek(-1.0));
        assert!(hi[0] < 1.1 * spec.half_width && lo[0] > -1.1 * spec.half_width);
        // A heavy jaw is bolder than a slight one: broader across the angle, heavier at the chin.
        let hinge = skull.hinge;
        let (heavy, _) = mandible(&spec, hinge, &mut rng());
        let (slight, _) = mandible(&narrow, hinge, &mut rng());
        let volume = |m: &MeshBuilder| m.positions.len();
        assert!(volume(&heavy) >= volume(&slight));
        // The ramus rises from the angle to the hinge on each side: bone high above the body of the jaw at the sides.
        let (jlo, jhi) = heavy.bounds().unwrap();
        assert!(jhi[1] > 0.1 * spec.half_height, "the rami do not reach the hinge ({jhi:?} {jlo:?})");
        for side in [-1.0f32, 1.0] {
            let rising = heavy.positions.iter().filter(|p| p[0] * side > 0.7 * spec.half_width && p[1] > -0.25 * spec.half_height).count();
            assert!(rising > 30, "{side}: only {rising} vertices up the ramus");
        }
    }

    #[test]
    fn a_foot_rests_on_its_sole() {
        let spec = Foot { ankle_height: 0.83, heel: 0.53, toe: 2.0, half_width: 0.5 };
        let mesh = foot(&spec);
        valid(mesh.clone(), "foot");
        let (lo, hi) = mesh.bounds().unwrap();
        assert!((lo[1] + spec.ankle_height).abs() < 0.03, "sole at {}", lo[1]);
        assert!(hi[2] > 0.9 * spec.toe && lo[2] > -0.9 * spec.heel - 0.4);
    }

    #[test]
    fn a_pelvis_a_vertebra_and_a_shoulder_blade_are_closed_and_face_out() {
        let spec = Pelvis { hip_x: 0.86, crest_x: 1.45, crest_y: 1.3, back_z: -0.6, front_z: 0.55, drop: 1.1, thickness: 0.09 };
        let pelvis = pelvis(&spec);
        valid(pelvis.clone(), "pelvis");
        let (lo, hi) = pelvis.bounds().unwrap();
        assert!(hi[0] < 1.8 && lo[0] > -1.8 && hi[1] < 1.6 && lo[1] > -1.4, "{lo:?} {hi:?}");
        valid(vertebra(&Vertebra { radius: 0.2, height: 0.3, spine: 0.4, transverse: 0.3 }), "vertebra");
        for outward in [-1.0, 1.0] {
            let blade = scapula([outward * 0.6, 13.5, -0.7], 1.5, outward, 0.6);
            valid(blade.clone(), "scapula");
            // The surface that faces away from the body (-Z) is the outer one: the vertices
            // lowest in z have normals toward -Z.
            let (lo, _) = blade.bounds().unwrap();
            let outer: Vec<f32> = blade.positions.iter().zip(&blade.normals).filter(|(p, _)| p[2] < lo[2] + 0.06).map(|(_, n)| n[2]).collect();
            assert!(outer.iter().sum::<f32>() < 0.0, "{outward}: the blade's back faces in");
        }
    }
}
