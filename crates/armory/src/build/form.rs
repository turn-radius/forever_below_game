//! Body-segment forms that worn gear is built around, and the sheet geometry worn gear is
//! made of.
//!
//! The Armory draws worn items standalone, on an implied mannequin, as the prototype does.
//! The mannequin is a set of parametric forms whose dimensions are inputs: [`TorsoForm`] for
//! body armour and clothing, [`HandForm`] for gloves and gauntlets, [`LegForm`] for
//! footwear and greaves, [`LowerBodyForm`] (both legs and the hips) for trousers, and
//! [`HeadForm`] for hoods, coifs and hats. `fb_body` fits gear to a folk body by building the same
//! form from that body's dimensions and passing it to the same builders.
//!
//! Sheet geometry: [`shell`] gives a sampled surface real thickness (an outer face, an inner
//! face and a lip round every open edge), so cloth, leather and plate show an edge and
//! outline cleanly. [`patch`] samples and thickens a surface in one step, [`rolled_edge`]
//! rolls a plate's edge, [`limb_lames`] stacks overlapping plates down a limb, and
//! [`wound_band`] winds a strip of cloth up one.
//!
//! Angles round a form run as in [`crate::geom::lathe`]: from +X toward -Z, so 90 degrees is
//! the back and 270 degrees the front (+Z).

use std::f32::consts::{FRAC_PI_2, PI};

use crate::build::hafted::smoothstep;
use crate::geom::vec3::{self, V3};
use crate::geom::{CatmullRom, GridOptions, MeshBuilder, Shading, SweepOptions, stitch_grid, tube};

/// `sign(c) * |c|^e`: the superellipse term. Exponents below 1 square a section off.
pub fn signed_pow(c: f32, e: f32) -> f32 {
    c.signum() * c.abs().powf(e)
}

/// The angle (lathe convention) of the front of a form.
pub const FRONT: f32 = 1.5 * PI;
/// The angle (lathe convention) of the back of a form.
pub const BACK: f32 = FRAC_PI_2;

/// A point on a superelliptic section of half width `half_width` (along X) and half depth
/// `half_depth` (along Z) at `angle`. `exponent` 1 is an ellipse; below 1 squares it off.
/// `back` scales the depth behind the centre (a flatter back).
pub fn section_point(angle: f32, half_width: f32, half_depth: f32, exponent: f32, back: f32) -> [f32; 2] {
    let (s, c) = angle.sin_cos();
    let z = -half_depth * signed_pow(s, exponent);
    [half_width * signed_pow(c, exponent), if z < 0.0 { z * back } else { z }]
}

/// One cross-section of a form: a level and its half extents.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FormRing {
    pub y: f32,
    pub half_width: f32,
    pub half_depth: f32,
}

impl FormRing {
    pub const fn new(y: f32, half_width: f32, half_depth: f32) -> Self {
        FormRing { y, half_width, half_depth }
    }

    /// The same ring `dy` higher.
    pub const fn raised(self, dy: f32) -> Self {
        FormRing { y: self.y + dy, ..self }
    }

    fn scaled(self, k: f32) -> Self {
        FormRing { half_width: self.half_width * k, half_depth: self.half_depth * k, ..self }
    }
}

/// A swelling on one face of a form (a belly or a bust on the front, the seat behind):
/// `depth` deep at height `y`, falling off as a Gaussian `spread` above and below, and
/// round the form as `facing^sharpness`, where `facing` is 1 at the angle `toward` (lathe
/// convention) and 0 a quarter turn either side and beyond. The plate prototype's `tSurf`
/// swells the belly at sharpness 2 and the bust at 4, both toward the front.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bulge {
    pub y: f32,
    pub spread: f32,
    pub depth: f32,
    pub sharpness: i32,
    pub toward: f32,
}

impl Bulge {
    pub const NONE: Bulge = Bulge { y: 0.0, spread: 1.0, depth: 0.0, sharpness: 2, toward: FRONT };

    /// How far the form swells at `angle` (lathe convention) and height `y`.
    pub fn at(&self, angle: f32, y: f32) -> f32 {
        if self.depth <= 0.0 {
            return 0.0;
        }
        let facing = (angle - self.toward).cos().max(0.0);
        let d = (y - self.y) / self.spread.max(1e-3);
        self.depth * (-d * d).exp() * facing.powi(self.sharpness)
    }

    /// The same swelling `dy` higher.
    pub fn raised(self, dy: f32) -> Bulge {
        Bulge { y: self.y + dy, ..self }
    }
}

/// The trunk from neck to shins, as superelliptic sections on a spline. Below the hips the
/// form is the envelope of both legs together, which is what a skirt hangs over. The waist
/// is at y = 0; +Z is the front, +X the wearer's left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TorsoForm {
    pub neck: FormRing,
    pub shoulder_top: FormRing,
    pub shoulder: FormRing,
    pub chest: FormRing,
    pub waist: FormRing,
    pub hip: FormRing,
    pub thigh: FormRing,
    pub knee: FormRing,
    pub shin: FormRing,
    /// Superellipse exponent of every section (0.77: slightly boxy, as the prototype's `tSurf`).
    pub exponent: f32,
    /// Depth of the back relative to the front.
    pub back: f32,
    /// Swellings over the sections: a belly about the waist and a bust about the chest in
    /// front, the seat behind the hips. Garments drape over them.
    pub belly: Bulge,
    pub bust: Bulge,
    pub seat: Bulge,
}

impl TorsoForm {
    /// The prototype's garment mannequin (`buildGarment` rows), 1 unit = 10 cm.
    pub const MANNEQUIN: TorsoForm = TorsoForm {
        neck: FormRing::new(4.6, 0.6, 0.56),
        shoulder_top: FormRing::new(4.38, 1.2, 0.8),
        shoulder: FormRing::new(4.0, 1.98, 1.02),
        chest: FormRing::new(2.4, 1.78, 1.2),
        waist: FormRing::new(0.0, 1.47, 1.02),
        hip: FormRing::new(-0.9, 1.72, 1.16),
        thigh: FormRing::new(-2.4, 1.7, 1.15),
        knee: FormRing::new(-5.0, 1.74, 1.2),
        shin: FormRing::new(-9.0, 1.82, 1.26),
        exponent: 2.0 / 2.6,
        back: 0.9,
        belly: Bulge::NONE,
        bust: Bulge::NONE,
        seat: Bulge::NONE,
    };

    /// Every section from the shoulders down widened by `ease` (a garment's fit over the body).
    pub fn eased(&self, ease: f32) -> TorsoForm {
        TorsoForm {
            shoulder: self.shoulder.scaled(ease),
            chest: self.chest.scaled(ease),
            waist: self.waist.scaled(ease),
            hip: self.hip.scaled(ease),
            thigh: self.thigh.scaled(ease),
            knee: self.knee.scaled(ease),
            shin: self.shin.scaled(ease),
            ..*self
        }
    }

    /// Sections bottom to top.
    pub fn rings(&self) -> [FormRing; 9] {
        [self.shin, self.knee, self.thigh, self.hip, self.waist, self.chest, self.shoulder, self.shoulder_top, self.neck]
    }

    /// The form sampled densely for lookups by height.
    pub fn profile(&self) -> TorsoProfile {
        TorsoProfile::new(&self.rings(), self.exponent, self.back, vec![self.belly, self.bust, self.seat])
    }

    /// Where an arm leaves the body on side `side` (+1 left, -1 right), and the arm's
    /// direction hanging `arm_angle` radians out from the body.
    pub fn arm_root(&self, side: f32, arm_angle: f32) -> (V3, V3) {
        let root = [side * self.shoulder.half_width * 0.98, self.shoulder.y - 0.62, 0.0];
        let dir = vec3::normalize_or([side * arm_angle.sin(), -arm_angle.cos(), 0.04], vec3::neg(vec3::Y));
        (root, dir)
    }
}

/// A [`TorsoForm`] (or any rising run of sections) sampled for lookups.
#[derive(Clone, Debug, PartialEq)]
pub struct TorsoProfile {
    samples: Vec<FormRing>,
    pub exponent: f32,
    pub back: f32,
    pub bulges: Vec<Bulge>,
}

impl TorsoProfile {
    /// Sections `rings` (rising in y) on a centripetal Catmull-Rom spline, sampled densely.
    /// `fb_body` uses this for the skin of a trunk, which runs on past a garment's form to
    /// the crotch and up the neck.
    pub fn new(rings: &[FormRing], exponent: f32, back: f32, bulges: Vec<Bulge>) -> TorsoProfile {
        let curve = CatmullRom::centripetal(rings.iter().map(|r| [r.half_width, r.y, r.half_depth]).collect());
        let mut samples: Vec<FormRing> = curve.sample(48).into_iter().map(|p| FormRing::new(p[1], p[0].max(0.01), p[2].max(0.01))).collect();
        // Keep heights increasing so lookups can bisect.
        for i in 1..samples.len() {
            if samples[i].y <= samples[i - 1].y {
                samples[i].y = samples[i - 1].y + 1e-4;
            }
        }
        TorsoProfile { samples, exponent, back, bulges }
    }

    pub fn bottom(&self) -> f32 {
        self.samples[0].y
    }

    pub fn top(&self) -> f32 {
        self.samples[self.samples.len() - 1].y
    }

    /// Half width and half depth at height `y` (clamped to the form).
    pub fn at(&self, y: f32) -> (f32, f32) {
        let s = &self.samples;
        let i = s.partition_point(|r| r.y < y).clamp(1, s.len() - 1);
        let (a, b) = (s[i - 1], s[i]);
        let t = ((y - a.y) / (b.y - a.y)).clamp(0.0, 1.0);
        (a.half_width + (b.half_width - a.half_width) * t, a.half_depth + (b.half_depth - a.half_depth) * t)
    }

    /// The section's point at `angle` and height `y`, `inflate` further out, with its
    /// bulges.
    pub fn section(&self, angle: f32, y: f32, inflate: f32) -> [f32; 2] {
        let (hw, hd) = self.at(y);
        let swell: f32 = self.bulges.iter().map(|b| b.at(angle, y)).sum();
        section_point(angle, hw + inflate, hd + swell + inflate, self.exponent, self.back)
    }

    /// The surface point at `angle` and height `y`.
    pub fn point(&self, angle: f32, y: f32) -> V3 {
        let [x, z] = self.section(angle, y, 0.0);
        [x, y, z]
    }
}

/// How [`shell`] thickens a grid.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellOptions {
    /// The last column joins the first (a tube).
    pub wrap_around: bool,
    /// Distance from the outer face to the inner face, measured inward.
    pub thickness: f32,
    pub shading: Shading,
    /// Close the first and last rows with a lip.
    pub lip_rows: bool,
    /// Close the first and last columns with a lip (open, unwrapped sheets).
    pub lip_columns: bool,
}

impl ShellOptions {
    /// A tube of cloth or plate, open at both ends.
    pub fn tube(thickness: f32) -> Self {
        ShellOptions { wrap_around: true, thickness, shading: Shading::SMOOTH, lip_rows: true, lip_columns: false }
    }

    /// An open sheet with edges all round.
    pub fn sheet(thickness: f32) -> Self {
        ShellOptions { wrap_around: false, thickness, shading: Shading::SMOOTH, lip_rows: true, lip_columns: true }
    }
}

/// Per-point outward normals of a grid (`points[row][column]`, front facing `du x dv`).
pub fn grid_normals(points: &[Vec<V3>], wrap_around: bool) -> Vec<Vec<V3>> {
    let rows = points.len();
    let cols = points.first().map_or(0, Vec::len);
    let at = |r: usize, c: isize| -> V3 {
        let c = if wrap_around { c.rem_euclid(cols as isize) as usize } else { c.clamp(0, cols as isize - 1) as usize };
        points[r][c]
    };
    let raw: Vec<Vec<Option<V3>>> = (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| {
                    let du = vec3::sub(at(r, c as isize + 1), at(r, c as isize - 1));
                    let (r0, r1) = (r.saturating_sub(1), (r + 1).min(rows - 1));
                    let dv = vec3::sub(points[r1][c], points[r0][c]);
                    vec3::try_normalize(vec3::cross(du, dv))
                })
                .collect()
        })
        .collect();
    // Points where the grid has collapsed (a pole) borrow the nearest row's normal.
    (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| {
                    raw[r][c]
                        .or_else(|| (1..rows).find_map(|d| [r.checked_sub(d), Some(r + d)].into_iter().flatten().find_map(|k| raw.get(k).and_then(|row| row[c]))))
                        .unwrap_or(vec3::Y)
                })
                .collect()
        })
        .collect()
}

/// Gives a sampled surface thickness: the outer face as sampled, an inner face
/// `options.thickness` inward along the surface normal, and lips joining them round the
/// open edges. Front faces point outward everywhere, so the result is a closed solid for
/// lighting and the ink hull.
pub fn shell(points: &[Vec<V3>], options: &ShellOptions) -> MeshBuilder {
    let rows = points.len();
    if rows < 2 || points[0].len() < 2 {
        return MeshBuilder::new();
    }
    let normals = grid_normals(points, options.wrap_around);
    let inner: Vec<Vec<V3>> = points
        .iter()
        .zip(&normals)
        .map(|(row, ns)| row.iter().zip(ns).map(|(&p, &n)| vec3::sub(p, vec3::scale(n, options.thickness))).collect())
        .collect();
    let grid = |shading: Shading, wrap_around: bool| GridOptions { wrap_around, wrap_along: false, shading, row_v: None };
    let mut mesh = stitch_grid(points, &grid(options.shading, options.wrap_around));
    let mut back = stitch_grid(&inner, &grid(options.shading, options.wrap_around));
    back.flip();
    mesh.append(&back);
    if options.lip_rows {
        let last = rows - 1;
        mesh.append(&stitch_grid(&[inner[0].clone(), points[0].clone()], &grid(Shading::SMOOTH, options.wrap_around)));
        mesh.append(&stitch_grid(&[points[last].clone(), inner[last].clone()], &grid(Shading::SMOOTH, options.wrap_around)));
    }
    if options.lip_columns && !options.wrap_around {
        let last = points[0].len() - 1;
        let column = |a: &[Vec<V3>], b: &[Vec<V3>], c: usize| -> Vec<Vec<V3>> { (0..rows).map(|r| vec![a[r][c], b[r][c]]).collect() };
        mesh.append(&stitch_grid(&column(&inner, points, 0), &grid(Shading::SMOOTH, false)));
        mesh.append(&stitch_grid(&column(points, &inner, last), &grid(Shading::SMOOTH, false)));
    }
    mesh
}

/// Samples `f(u, v)` over `0..=1` squared (`nu` by `nv` cells; with `wrap_around`, u stops
/// short of 1) and thickens it with [`shell`]. Arrange `f` so `df/du x df/dv` faces outward.
pub fn patch(nu: usize, nv: usize, options: &ShellOptions, f: impl Fn(f32, f32) -> V3) -> MeshBuilder {
    let (nu, nv) = (nu.max(1), nv.max(1));
    let cols = if options.wrap_around { nu } else { nu + 1 };
    let points: Vec<Vec<V3>> =
        (0..=nv).map(|r| (0..cols).map(|c| f(c as f32 / nu as f32, r as f32 / nv as f32)).collect()).collect();
    shell(&points, options)
}

/// A rolled or wired edge: a round tube of `radius` along `points` (closed when `closed`).
pub fn rolled_edge(points: &[V3], radius: f32, closed: bool) -> MeshBuilder {
    if points.len() < 2 {
        return MeshBuilder::new();
    }
    let options = if closed { SweepOptions::closed_loop() } else { SweepOptions::default() };
    tube(points, 8, &options, |_| radius)
}

/// Where [`limb_lames`] stacks its plates: down a limb from `origin` along `axis`, centred on
/// the side facing `toward`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LameStack {
    pub origin: V3,
    /// Down the limb.
    pub axis: V3,
    /// The side of the limb the plates cover.
    pub toward: V3,
    pub count: u32,
    /// Radius of the first lame; each next one is `step` wider.
    pub radius: f32,
    pub step: f32,
    /// Height of each lame along the axis.
    pub height: f32,
    /// Fraction of each lame hidden under the one above.
    pub overlap: f32,
    /// Half the angle each lame wraps round the limb, in radians.
    pub half_angle: f32,
    pub thickness: f32,
    /// A domed cap over the top of the stack (a pauldron's shoulder cop).
    pub cap: bool,
}

/// Overlapping plates wrapping one side of a limb: each a partial band that widens a little
/// toward its lower edge, with a rolled lower edge, each lapping over the next (the
/// prototype's `limbLames`). Pauldrons, couters, cuisses and gauntlet cuffs.
pub fn limb_lames(stack: &LameStack) -> MeshBuilder {
    let axis = vec3::normalize_or(stack.axis, vec3::neg(vec3::Y));
    let side = vec3::normalize_or(vec3::sub(stack.toward, vec3::scale(axis, vec3::dot(stack.toward, axis))), vec3::any_perpendicular(axis));
    let across = vec3::cross(axis, side);
    // Angle a about the axis measured from `side`; u runs so faces point outward.
    let ring = |a: f32, r: f32| vec3::add(vec3::scale(side, r * a.cos()), vec3::scale(across, -r * a.sin()));
    let mut mesh = MeshBuilder::new();
    let options = ShellOptions { thickness: stack.thickness, ..ShellOptions::sheet(stack.thickness) };
    for k in 0..stack.count {
        let r = stack.radius + k as f32 * stack.step;
        let top = k as f32 * stack.height * (1.0 - stack.overlap);
        let h = stack.height;
        let lame = patch(28, 6, &options, |u, v| {
            let a = -stack.half_angle + 2.0 * stack.half_angle * u;
            // v runs from the lower edge up so faces point outward; flare toward the lower edge.
            let t = 1.0 - v;
            let rr = r * (1.0 + 0.06 * t * t);
            vec3::add(vec3::add(stack.origin, vec3::scale(axis, top + h * t)), ring(a, rr))
        });
        mesh.append(&lame);
        let edge: Vec<V3> = (0..=24)
            .map(|i| {
                let a = -stack.half_angle + 2.0 * stack.half_angle * i as f32 / 24.0;
                vec3::add(vec3::add(stack.origin, vec3::scale(axis, top + h)), ring(a, r * 1.06 + stack.thickness * 0.5))
            })
            .collect();
        mesh.append(&rolled_edge(&edge, stack.thickness * 1.1, false));
    }
    if stack.cap {
        let r = stack.radius * 0.98;
        let cap = patch(28, 10, &options, |u, v| {
            let a = -stack.half_angle + 2.0 * stack.half_angle * u;
            // v from the rim up over the dome.
            let polar = v * FRAC_PI_2 * 0.96;
            let rr = r * polar.cos();
            vec3::add(vec3::add(stack.origin, vec3::scale(axis, -r * 0.9 * polar.sin())), ring(a, rr))
        });
        mesh.append(&cap);
    }
    mesh
}

/// One finger or the thumb: where it leaves the hand, which way it points, how long and
/// thick it is, and how its length divides between the phalanges (base to tip).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Digit {
    pub base: V3,
    /// Direction with the digit straight, before curl.
    pub direction: V3,
    pub length: f32,
    pub radius: f32,
    /// Fraction of the length in each phalanx; a zero ends the digit early (the thumb).
    pub phalanges: [f32; 3],
    /// Which way the digit bends as it curls (toward the palm).
    pub bend: V3,
}

impl Digit {
    /// The joints from base to tip, each phalanx bent `curl` radians further toward `bend`.
    pub fn joints(&self, curl: f32) -> Vec<V3> {
        let mut d = vec3::normalize_or(self.direction, vec3::Y);
        let mut p = self.base;
        let mut out = vec![p];
        for &f in self.phalanges.iter().filter(|f| **f > 0.0) {
            p = vec3::add(p, vec3::scale(d, self.length * f));
            out.push(p);
            d = vec3::normalize_or(vec3::add(vec3::scale(d, curl.cos()), vec3::scale(self.bend, curl.sin())), d);
        }
        out
    }
}

/// A right hand, back of the hand toward +Z, fingers up +Y, wrist at y = 0, forearm down -Y.
/// The palm is a superelliptic slab; fingers and thumb are [`Digit`]s; the forearm is a
/// superelliptic tube widening toward the elbow. Mirror in X for a left hand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandForm {
    /// Wrist to knuckles.
    pub palm_length: f32,
    /// Half width at the wrist and across the knuckles.
    pub wrist_half_width: f32,
    pub knuckle_half_width: f32,
    /// Half thickness at the wrist and at the knuckles.
    pub wrist_half_depth: f32,
    pub knuckle_half_depth: f32,
    /// Index to little finger.
    pub fingers: [Digit; 4],
    pub thumb: Digit,
    /// Half width and half depth of the forearm at the wrist, and how much it widens per unit.
    pub forearm_half_width: f32,
    pub forearm_half_depth: f32,
    pub forearm_taper: f32,
}

impl HandForm {
    /// The prototype's glove hand (`buildHands`) scaled by `size`, fingers splayed by
    /// `spread` (0..=1).
    pub fn mannequin(size: f32, spread: f32) -> HandForm {
        let s = size;
        let hb = 0.95 * s;
        let bend = vec3::neg(vec3::Z);
        let finger = |x: f32, length: f32, radius: f32, splay: f32| {
            let a = splay * (0.4 + spread * 2.0);
            Digit {
                base: [x * s, hb - 0.06 * s, 0.0],
                direction: [a.sin(), a.cos(), 0.0],
                length: length * s,
                radius: radius * s,
                phalanges: [0.42, 0.32, 0.26],
                bend,
            }
        };
        HandForm {
            palm_length: hb,
            wrist_half_width: 0.36 * s,
            knuckle_half_width: 0.48 * s,
            wrist_half_depth: 0.2 * s,
            knuckle_half_depth: 0.14 * s,
            // Index finger on the thumb side (-X for a right hand seen from the back).
            fingers: [finger(-0.34, 0.74, 0.1, -0.1), finger(-0.115, 0.82, 0.104, -0.03), finger(0.115, 0.76, 0.1, 0.04), finger(0.34, 0.62, 0.09, 0.12)],
            thumb: Digit {
                base: [-0.36 * s, 0.33 * hb, 0.03 * s],
                direction: vec3::normalize_or([-0.62, 0.7, 0.32], vec3::Y),
                length: 0.6 * s,
                radius: 0.112 * s,
                phalanges: [0.55, 0.45, 0.0],
                bend: vec3::normalize_or([0.5, 0.0, -0.85], vec3::Z),
            },
            forearm_half_width: 0.36 * 1.12 * s,
            forearm_half_depth: 0.2 * 1.4 * s,
            forearm_taper: 0.05,
        }
    }

    /// Half width and half depth of the palm at height `y` (0 at the wrist).
    pub fn palm_at(&self, y: f32) -> (f32, f32) {
        let t = smoothstep(0.0, 0.55 * self.palm_length, y);
        let d = smoothstep(0.0, self.palm_length, y);
        (
            self.wrist_half_width + (self.knuckle_half_width - self.wrist_half_width) * t,
            self.wrist_half_depth + (self.knuckle_half_depth - self.wrist_half_depth) * d,
        )
    }

    /// A point on the palm at `angle` (lathe convention: 270 degrees is the back of the hand)
    /// and height `y`, `inflate` further out. Above the knuckles the palm rounds off into an
    /// end `cap` long (short over a glove's knuckles, long for a mitten's bag).
    pub fn palm_point(&self, angle: f32, y: f32, cap: f32, inflate: f32) -> V3 {
        let (mut w, mut d) = self.palm_at(y.min(self.palm_length));
        let (mut wi, mut di) = (inflate, inflate);
        if y > self.palm_length {
            let t = ((y - self.palm_length) / cap.max(1e-3)).clamp(0.0, 1.0);
            let k = (1.0 - t * t).max(0.0).sqrt();
            w *= k;
            d *= k;
            wi *= k;
            di *= k;
        }
        let [x, z] = section_point(angle, w + wi, d + di, 0.7, 1.0);
        [x, y, z]
    }

    /// A point on the forearm `down` below the wrist at `angle`, scaled by `flare`.
    pub fn forearm_point(&self, angle: f32, down: f32, flare: f32) -> V3 {
        let k = (1.0 + self.forearm_taper * down) * flare;
        let [x, z] = section_point(angle, self.forearm_half_width * k, self.forearm_half_depth * k, 2.0 / 2.4, 1.0);
        [x, -down, z]
    }
}

/// A right foot and lower leg in profile: toe toward +X, sole on y = 0, the outside of the
/// foot toward -Z, the shin rising from the ankle. The foot is a superelliptic section along
/// its length with a flat sole, rounded at the heel and the toe; the leg is a superelliptic
/// tube with a calf.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegForm {
    /// Heel to the ball's end of a round toe.
    pub foot_length: f32,
    /// Extra length of a pointed toe beyond the foot.
    pub toe_point: f32,
    /// How far a pointed toe curls up at its tip.
    pub toe_lift: f32,
    /// Half width across the ball of the foot and at the heel.
    pub ball_half_width: f32,
    pub heel_half_width: f32,
    /// Height of the instep and of the toe box.
    pub instep_height: f32,
    pub toe_height: f32,
    /// Where the leg rises from, along the foot.
    pub ankle_x: f32,
    /// Half width (toe to heel) and half depth (side to side) of the shin above the ankle.
    pub shin_half_width: f32,
    pub shin_half_depth: f32,
    /// How much the calf swells at its widest.
    pub calf: f32,
    /// Height of the knee above the sole.
    pub knee_height: f32,
}

impl LegForm {
    /// The prototype's boot form (`buildFeet`) scaled by `size`, `width` wide, with a pointed
    /// toe of `toe` (0..=1).
    pub fn mannequin(size: f32, width: f32, toe: f32) -> LegForm {
        let s = size;
        LegForm {
            foot_length: 2.6 * s,
            toe_point: toe * 1.8 * s,
            toe_lift: toe * 0.4 * s,
            ball_half_width: 0.46 * s * width,
            heel_half_width: 0.34 * s * width,
            instep_height: 0.95 * s,
            toe_height: 0.45 * s,
            ankle_x: 0.54 * s,
            shin_half_width: 0.5 * s,
            shin_half_depth: 0.35 * s * width,
            calf: 0.22,
            knee_height: 4.6 * s,
        }
    }

    pub fn pointed(&self) -> bool {
        self.toe_point > 0.09
    }

    /// Heel to the tip of the toe.
    pub fn toe_x(&self) -> f32 {
        self.foot_length + if self.pointed() { self.toe_point } else { 0.0 }
    }

    /// Half width of the foot at `x` along it, and how far its section is rounded off toward
    /// the heel or the toe (1 in the middle, 0 at the ends).
    pub fn half_width(&self, x: f32) -> (f32, f32) {
        let l = self.foot_length;
        let heel = {
            let t = (x / (0.13 * l)).clamp(0.0, 1.0);
            (1.0 - (1.0 - t) * (1.0 - t)).max(0.0).sqrt()
        };
        let mut w = self.heel_half_width + (self.ball_half_width - self.heel_half_width) * smoothstep(0.15 * l, 0.7 * l, x);
        let mut end = heel;
        if x > 0.72 * l {
            let t = ((x - 0.72 * l) / (self.toe_x() - 0.72 * l)).clamp(0.0, 1.0);
            if self.pointed() {
                w = self.ball_half_width * (1.0 - t.powf(0.75)) + 0.012 * self.foot_length / 2.6;
                end = end.min((1.0 - t.powf(0.75)).max(0.0).sqrt());
            } else {
                let k = (1.0 - t * t).max(0.0).sqrt();
                w = self.ball_half_width * k;
                end = end.min(k.sqrt());
            }
        }
        (w.max(0.0), end)
    }

    /// Height of the foot's top at `x`.
    pub fn height(&self, x: f32) -> f32 {
        let l = self.foot_length;
        let (inst, toe) = (self.instep_height, self.toe_height);
        let mut h = if x < 0.38 * l {
            inst * (0.63 + 0.37 * smoothstep(0.04 * l, 0.36 * l, x))
        } else {
            inst - (inst - toe) * smoothstep(0.38 * l, 0.78 * l, x)
        };
        if x > 0.78 * l {
            let t = ((x - 0.78 * l) / (self.toe_x() - 0.78 * l)).clamp(0.0, 1.0);
            h = toe - toe * if self.pointed() { 0.7 } else { 0.45 } * t;
        }
        h
    }

    /// How far a pointed toe has lifted at `x`.
    pub fn lift(&self, x: f32) -> f32 {
        let l = self.foot_length;
        if self.pointed() && x > 0.78 * l {
            self.toe_lift * ((x - 0.78 * l) / (self.toe_x() - 0.78 * l)).clamp(0.0, 1.0).powf(2.4)
        } else {
            0.0
        }
    }

    /// A point on the foot at `x` along it and `angle` round it (0 the outside, a quarter
    /// turn the top), `inflate` further out. Rows of increasing `x` with increasing `angle`
    /// face outward.
    pub fn foot_point(&self, angle: f32, x: f32, inflate: f32) -> V3 {
        let (s, c) = angle.sin_cos();
        let (w, end) = self.half_width(x);
        let h = self.height(x) + inflate;
        let w = w + inflate;
        let centre = 0.42 * h;
        let y = if s >= 0.0 { h * (0.42 + 0.58 * signed_pow(s, 0.8)) } else { 0.42 * h * (1.0 - (-s).powi(3)) } - inflate * 0.6;
        let y = centre + (y - centre) * end;
        [x, self.lift(x) + y, -w * end * signed_pow(c, 2.0 / 2.2)]
    }

    /// Half the foot's width (along Z) at `x` along it and `y` above the sole: the inverse of
    /// [`LegForm::foot_point`]'s section, 0 above the foot or past either end.
    pub fn foot_half_width_at(&self, x: f32, y: f32) -> f32 {
        if x < 0.0 || x > self.toe_x() {
            return 0.0;
        }
        let (w, end) = self.half_width(x);
        let h = self.height(x);
        let y = y - self.lift(x);
        if end <= 0.0 || y <= 0.0 {
            return 0.0;
        }
        // Undo the rounding toward the ends, which squeezes the section about its centre.
        let centre = 0.42 * h;
        let y = centre + (y - centre) / end;
        let s = if y >= centre {
            let t = (y / h - 0.42) / 0.58;
            if t >= 1.0 {
                return 0.0;
            }
            t.max(0.0).powf(1.0 / 0.8)
        } else {
            let t = 1.0 - y / centre;
            if t >= 1.0 {
                return 0.0;
            }
            -t.cbrt()
        };
        w * end * (1.0 - s * s).max(0.0).sqrt().powf(2.0 / 2.2)
    }

    /// A point on the leg `y` above the sole at `angle` (0 toward the toe, lathe convention),
    /// `inflate` further out. `height` is the shaft's top, for the calf's placement; `ridge`
    /// raises a crest down the shin (a greave).
    pub fn shin_point(&self, angle: f32, y: f32, height: f32, ridge: f32, inflate: f32) -> V3 {
        let t = (y / height.max(0.01)).clamp(0.0, 1.0);
        let tall = height > 2.0 * self.foot_length / 2.6;
        let calf = if tall { self.calf * (-((t - 0.55) / 0.22).powi(2)).exp() } else { 0.0 };
        let a = (angle + PI).rem_euclid(2.0 * PI) - PI;
        // Only a tall shaft flares at its top, over the calf; a short one closes on the ankle.
        let flare = if tall { 0.06 * smoothstep(0.85, 1.0, t) } else { 0.0 };
        let k = (1.0 + calf + flare) * (1.0 + ridge * (-(a / 0.32).powi(2)).exp());
        // The leg fills out by height, not by share of the shaft, and only above the ankle, so
        // a short shaft hugs the ankle instead of flaring like a bucket.
        let depth = self.shin_half_depth * (1.0 + 0.2 * smoothstep(0.2, 0.45, y / self.knee_height));
        let [x, z] = section_point(angle, self.shin_half_width * k + inflate, depth * k + inflate, 2.0 / 2.2, 1.0);
        // Above the heel the back of the leg (the Achilles tendon) stands forward of the heel's
        // back, easing out again toward the calf.
        let heel_top = self.instep_height;
        let achilles = 0.04 * self.foot_length
            * smoothstep(0.5 * heel_top, 0.9 * heel_top, y)
            * (1.0 - smoothstep(0.25 * self.knee_height, 0.45 * self.knee_height, y))
            * (-angle.cos()).max(0.0).powi(2);
        [self.ankle_x + x + achilles, y, z]
    }
}

/// Cubic (Catmull-Rom) interpolation of `(y, a, b)` knots by `y`, clamped at the ends.
/// Knots must rise in `y`.
fn knot_curve(knots: &[(f32, f32, f32)], y: f32) -> (f32, f32) {
    let n = knots.len();
    if y <= knots[0].0 {
        return (knots[0].1, knots[0].2);
    }
    if y >= knots[n - 1].0 {
        return (knots[n - 1].1, knots[n - 1].2);
    }
    let i = knots.partition_point(|k| k.0 <= y).clamp(1, n - 1) - 1;
    let (k0, k1) = (knots[i], knots[i + 1]);
    let h = k1.0 - k0.0;
    let t = (y - k0.0) / h;
    let slope = |j: usize, pick: fn(&(f32, f32, f32)) -> f32| -> f32 {
        let (a, b) = (knots[j.saturating_sub(1)], knots[(j + 1).min(n - 1)]);
        (pick(&b) - pick(&a)) / (b.0 - a.0)
    };
    let hermite = |pick: fn(&(f32, f32, f32)) -> f32| {
        let (p0, p1, m0, m1) = (pick(&k0), pick(&k1), slope(i, pick) * h, slope(i + 1, pick) * h);
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * p0 + (t3 - 2.0 * t2 + t) * m0 + (-2.0 * t3 + 3.0 * t2) * p1 + (t3 - t2) * m1
    };
    (hermite(|k| k.1), hermite(|k| k.2))
}

/// Both legs from the soles to the waist, for trousers, breeches and leggings: a [`LegForm`]
/// for the lower legs, thighs that meet at the crotch, and the [`TorsoForm`]'s hips and
/// waist above. Soles at y = 0, front +Z, the wearer's left +X (the torso's axes); the
/// torso's waist (its y = 0) sits `waist_height` above the soles.
///
/// Each leg is its own tube. Above the crotch each becomes one half of the torso's section,
/// flat on the midline where it meets the other, so a pair of trousers is two closed halves
/// seamed down the front and back as real ones are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LowerBodyForm {
    pub torso: TorsoForm,
    /// The lower leg: shin and calf girth and the knee's height.
    pub leg: LegForm,
    /// Height of the torso's waist above the soles.
    pub waist_height: f32,
    pub crotch_height: f32,
    /// Half width (side to side) and half depth of one thigh at the crotch.
    pub thigh_half_width: f32,
    pub thigh_half_depth: f32,
    /// Distance of each ankle from the midline.
    pub stance: f32,
}

impl LowerBodyForm {
    /// The garment mannequin's hips ([`TorsoForm::MANNEQUIN`]) on the boot mannequin's legs
    /// ([`LegForm::mannequin`]): a human about 17 units tall, legs straight and a little apart.
    pub fn mannequin() -> LowerBodyForm {
        LowerBodyForm {
            torso: TorsoForm::MANNEQUIN,
            leg: LegForm::mannequin(1.0, 1.0, 0.0),
            waist_height: 10.0,
            crotch_height: 8.0,
            thigh_half_width: 0.84,
            thigh_half_depth: 0.95,
            stance: 0.62,
        }
    }

    /// The torso's waist widened by `ease` (a garment's fit), legs unchanged.
    pub fn eased(&self, ease: f32) -> LowerBodyForm {
        LowerBodyForm { torso: self.torso.eased(ease), ..*self }
    }

    /// Height of the knee above the soles.
    pub fn knee_height(&self) -> f32 {
        self.leg.knee_height
    }

    /// Height of the hips' widest section above the soles.
    pub fn hip_height(&self) -> f32 {
        self.waist_height + self.torso.hip.y
    }

    /// The form sampled for lookups.
    pub fn profile(&self) -> LowerBodyProfile {
        let l = &self.leg;
        let k = l.knee_height;
        // Ankle, the narrow above it, calf, knee and crotch: (height, half width, half depth).
        let knots = vec![
            (0.0, l.shin_half_depth * 1.05, l.shin_half_width),
            (0.25 * k, l.shin_half_depth * 0.95, l.shin_half_width * 0.9),
            (0.62 * k, l.shin_half_depth * (1.0 + 0.7 * l.calf), l.shin_half_width * (1.0 + l.calf)),
            (k, l.shin_half_depth * 1.2, l.shin_half_width * 0.94),
            (self.crotch_height, self.thigh_half_width, self.thigh_half_depth),
        ];
        LowerBodyProfile { form: *self, torso: self.torso.profile(), knots }
    }
}

/// A [`LowerBodyForm`] sampled for lookups.
#[derive(Clone, Debug, PartialEq)]
pub struct LowerBodyProfile {
    pub form: LowerBodyForm,
    pub torso: TorsoProfile,
    knots: Vec<(f32, f32, f32)>,
}

impl LowerBodyProfile {
    /// Half width and half depth of one leg at height `y` (up to the crotch).
    pub fn leg_at(&self, y: f32) -> (f32, f32) {
        knot_curve(&self.knots, y.min(self.form.crotch_height))
    }

    /// Distance of a leg's axis from the midline at height `y`: the stance at the soles,
    /// the thigh's half width at the crotch, so the thighs just meet.
    pub fn axis_at(&self, y: f32) -> f32 {
        let f = &self.form;
        let t = (y / f.crotch_height).clamp(0.0, 1.0);
        f.stance + (f.thigh_half_width - f.stance) * t
    }

    /// How far a leg has become half of the torso at height `y`: 0 at the crotch, 1 at the hips.
    pub fn seat(&self, y: f32) -> f32 {
        smoothstep(self.form.crotch_height, self.form.hip_height(), y)
    }

    /// A point on leg `side` (+1 the wearer's left, -1 the right) at `angle` (lathe
    /// convention, as for [`TorsoProfile::point`]) and height `y`, `inflate` further out.
    /// Above the crotch the inner half lies on the midline (x = 0). Rows of increasing `y`
    /// with increasing `angle` face outward on both legs.
    pub fn point(&self, side: f32, angle: f32, y: f32, inflate: f32) -> V3 {
        let (s, c) = angle.sin_cos();
        let (hw, hd) = self.leg_at(y);
        let leg = [side * self.axis_at(y) + (hw + inflate) * signed_pow(c, 0.9), -(hd + inflate) * signed_pow(s, 0.9)];
        let b = self.seat(y);
        if b <= 0.0 {
            return [leg[0], y, leg[1]];
        }
        let [x, z] = self.half_section(side, angle, y, inflate);
        [leg[0] + (x - leg[0]) * b, y, leg[1] + (z - leg[1]) * b]
    }

    /// The torso's section at height `y` (above the soles) on leg `side`'s half: its own
    /// outline outward, the midline inward.
    fn half_section(&self, side: f32, angle: f32, y: f32, inflate: f32) -> [f32; 2] {
        let [x, z] = self.torso_xz(angle, y, inflate);
        if side * angle.cos() >= 0.0 { [x, z] } else { [0.0, z] }
    }

    fn torso_xz(&self, angle: f32, y: f32, inflate: f32) -> [f32; 2] {
        self.torso.section(angle, y - self.form.waist_height, inflate)
    }

    /// A point on the whole torso at `angle` and height `y` above the soles, `inflate`
    /// further out (a waistband round both halves).
    pub fn torso_point(&self, angle: f32, y: f32, inflate: f32) -> V3 {
        let [x, z] = self.torso_xz(angle, y, inflate);
        [x, y, z]
    }
}

/// A head on its neck: an ellipsoid with a narrower jaw, a chin, a flatter face and a
/// rounded back (the plate prototype's `headFn`), its centre at `centre`. Front +Z, up +Y,
/// the wearer's left +X, in the same space as the [`TorsoForm`] it sits on.
///
/// Points are placed by azimuth (0 the face, a quarter turn the wearer's left, half a turn
/// the back of the head) and polar angle (0 the crown, PI under the chin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadForm {
    pub centre: V3,
    /// Ear to ear, crown to chin, and face to back of the head, each from the centre.
    pub half_width: f32,
    pub half_height: f32,
    pub half_depth: f32,
    /// 0..=1: how much the jaw narrows below the cheekbones.
    pub jaw: f32,
    /// 0..=1: how far the chin juts.
    pub chin: f32,
    /// 0..=1: how flat the face is.
    pub flat: f32,
    pub neck_radius: f32,
    /// Where the neck meets the shoulders.
    pub neck_base: f32,
}

impl HeadForm {
    /// A human head (the plate prototype's `RACES.human`) scaled by `size`, set on
    /// [`TorsoForm::MANNEQUIN`]'s neck.
    pub fn mannequin(size: f32) -> HeadForm {
        let r = 1.05 * size;
        let torso = TorsoForm::MANNEQUIN;
        let half_height = 1.12 * r;
        let chin_y = torso.shoulder.y + 0.55;
        HeadForm {
            centre: [0.0, chin_y + half_height, 0.12],
            half_width: 0.86 * r,
            half_height,
            half_depth: r,
            jaw: 0.5,
            chin: 0.5,
            flat: 0.6,
            neck_radius: 0.42 * size,
            neck_base: torso.shoulder_top.y,
        }
    }

    /// The surface offset from the centre at `azimuth` and `polar`, before inflation.
    fn shape(&self, azimuth: f32, polar: f32) -> V3 {
        let (s, c) = polar.sin_cos();
        let (st, ct) = azimuth.sin_cos();
        let r = self.half_width.max(self.half_depth);
        let mut x = self.half_width * s * st;
        let y = self.half_height * c;
        let mut z = self.half_depth * s * ct;
        if c < 0.0 {
            let k = (-c).powf(1.4);
            x *= 1.0 - self.jaw * k * 0.38;
            if ct > 0.0 {
                z += self.chin * r * 0.12 * k * ct.powi(3);
            } else {
                z *= 1.0 - k * 0.15;
            }
        }
        if ct > 0.0 {
            z *= 1.0 - self.flat * 0.14 * ct.powi(4) * s;
        }
        if ct < 0.0 && c > -0.2 {
            z *= 1.0 + 0.07 * -ct;
        }
        [x, y, z]
    }

    /// A point on the head at `azimuth` and `polar`, `inflate` further out along the
    /// ellipsoid's normal. Rows of increasing `azimuth` with decreasing `polar` (going up)
    /// face outward.
    pub fn point(&self, azimuth: f32, polar: f32, inflate: f32) -> V3 {
        let p = self.shape(azimuth, polar);
        let n = self.normal_of(p);
        vec3::add(self.centre, vec3::add(p, vec3::scale(n, inflate)))
    }

    /// The outward normal at `azimuth` and `polar`.
    pub fn normal(&self, azimuth: f32, polar: f32) -> V3 {
        self.normal_of(self.shape(azimuth, polar))
    }

    fn normal_of(&self, p: V3) -> V3 {
        let g = [p[0] / self.half_width.powi(2), p[1] / self.half_height.powi(2), p[2] / self.half_depth.powi(2)];
        vec3::normalize_or(g, vec3::Y)
    }

    /// The polar angle of the brow line at `azimuth`, where a hat's band sits: above the
    /// eyebrows in front and lower behind, over the swell of the back of the head, so a hat
    /// tips back a little as hats are worn.
    pub fn brow_polar(azimuth: f32) -> f32 {
        let back = 0.5 - 0.5 * azimuth.cos();
        (0.38 + 0.05 * back) * PI
    }

    /// A point on the brow line at `azimuth`, `inflate` off the head.
    pub fn brow_point(&self, azimuth: f32, inflate: f32) -> V3 {
        self.point(azimuth, Self::brow_polar(azimuth), inflate)
    }

    /// The point at height `y` on the head at `azimuth`, `inflate` off it. Above the crown
    /// or below the chin it is the crown's or the chin's point.
    pub fn point_at_height(&self, azimuth: f32, y: f32, inflate: f32) -> V3 {
        let polar = ((y - self.centre[1]) / self.half_height).clamp(-1.0, 1.0).acos();
        self.point(azimuth, polar, inflate)
    }

    /// The top of the crown.
    pub fn crown_top(&self) -> f32 {
        self.centre[1] + self.half_height
    }

    /// The lathe-convention angle round a [`TorsoForm`] that faces the same way as `azimuth`.
    pub fn torso_angle(azimuth: f32) -> f32 {
        (azimuth - FRAC_PI_2).rem_euclid(2.0 * PI)
    }
}

/// A band of `width` wound `turns` times round a limb from height `y0` to `y1`, each turn
/// lapping over the one below (puttees, leg wraps, a wrapped grip on a body). `at(angle,
/// y, inflate)` is the limb's surface, angles in the lathe convention. Returns the band
/// and a cord along its lapped lower edge, which reads as the overlap.
pub fn wound_band(turns: f32, y0: f32, y1: f32, width: f32, thickness: f32, at: impl Fn(f32, f32, f32) -> V3) -> (MeshBuilder, MeshBuilder) {
    let turns = turns.max(1.0);
    let rise = |u: f32| y0 + width * 0.5 + (y1 - y0 - width) * u;
    let ribbon = patch((turns * 40.0) as usize, 2, &ShellOptions::sheet(thickness), |u, v| {
        let a = std::f32::consts::TAU * turns * u;
        at(a, rise(u) + (v - 0.5) * width, thickness * 0.5 + width * 0.12 * (1.0 - v))
    });
    let n = (turns * 48.0) as usize;
    let edge: Vec<V3> = (0..=n)
        .map(|i| {
            let u = i as f32 / n as f32;
            at(std::f32::consts::TAU * turns * u, rise(u) - width * 0.5, thickness + width * 0.12)
        })
        .collect();
    let cord = tube(&edge, 6, &SweepOptions::default(), |_| thickness * 0.8);
    (ribbon, cord)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;
    use std::f32::consts::TAU;

    #[test]
    fn the_head_sits_on_the_neck_and_faces_front() {
        let head = HeadForm::mannequin(1.0);
        let torso = TorsoForm::MANNEQUIN;
        let chin = head.point(0.0, PI, 0.0);
        assert!(chin[1] > torso.shoulder.y && chin[1] < torso.neck.y + 0.2, "chin {chin:?}");
        let face = head.point(0.0, FRAC_PI_2, 0.0);
        let back = head.point(PI, FRAC_PI_2, 0.0);
        assert!(face[2] > head.centre[2] && back[2] < head.centre[2]);
        for (az, polar) in [(0.3, 0.4), (2.0, 1.2), (4.0, 2.6)] {
            let n = head.normal(az, polar);
            let p = head.point(az, polar, 0.0);
            assert!(vec3::dot(n, vec3::sub(p, head.centre)) > 0.0, "normal faces out at {az} {polar}");
            let out = head.point(az, polar, 0.2);
            assert!((vec3::distance(out, p) - 0.2).abs() < 1e-4);
        }
        assert!((HeadForm::torso_angle(0.0) - FRONT).abs() < 1e-5);
    }

    #[test]
    fn the_legs_meet_on_the_midline_above_the_crotch() {
        let p = LowerBodyForm::mannequin().profile();
        let f = p.form;
        for side in [1.0f32, -1.0] {
            for i in 0..32 {
                let a = TAU * i as f32 / 32.0;
                let low = p.point(side, a, 3.0, 0.0);
                assert!(low[0] * side > 0.0, "a leg below the crotch stays on its side: {low:?}");
                let high = p.point(side, a, f.waist_height, 0.05);
                assert!(high[0] * side >= -1e-4, "half of the waist stays on its side: {high:?}");
            }
            // The waist round both halves matches the torso's own section.
            let out = p.point(side, if side > 0.0 { 0.0 } else { PI }, f.waist_height, 0.0);
            assert!((out[0].abs() - f.torso.waist.half_width).abs() < 0.05, "{out:?}");
        }
        let (hw, _) = p.leg_at(f.crotch_height);
        assert!((p.axis_at(f.crotch_height) - hw).abs() < 1e-4, "the thighs just meet");
    }

    #[test]
    fn mannequin_profile_matches_its_rings() {
        let p = TorsoForm::MANNEQUIN.profile();
        let (hw, hd) = p.at(0.0);
        assert!((hw - 1.47).abs() < 0.02 && (hd - 1.02).abs() < 0.02, "waist {hw} {hd}");
        assert!(p.bottom() < -8.9 && p.top() > 4.5);
        let front = p.point(FRONT, 2.4);
        assert!(front[2] > 1.1, "front of the chest faces +Z: {front:?}");
    }

    #[test]
    fn shells_are_drawable_and_face_out() {
        let ring = |u: f32, v: f32| {
            let a = TAU * u;
            [a.cos(), v * 2.0, -a.sin()]
        };
        let points: Vec<Vec<V3>> = (0..=6).map(|r| (0..16).map(|c| ring(c as f32 / 16.0, r as f32 / 6.0)).collect()).collect();
        for (row, ns) in points.iter().zip(grid_normals(&points, true)) {
            for (p, n) in row.iter().zip(ns) {
                assert!(vec3::dot(n, [p[0], 0.0, p[2]]) > 0.9, "normal {n:?} at {p:?}");
            }
        }
        let part = patch(16, 6, &ShellOptions::tube(0.05), ring).into_part("shell", OutlineMode::Welded);
        part.validate().unwrap();
        // Signed volume of a closed, outward-facing solid is positive.
        let volume: f32 = part
            .indices
            .chunks(3)
            .map(|t| {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| part.positions[i as usize]);
                vec3::dot(a, vec3::cross(b, c)) / 6.0
            })
            .sum();
        let expected = PI * (1.0 - 0.95f32 * 0.95) * 2.0;
        assert!((volume - expected).abs() < expected * 0.15, "volume {volume}, expected about {expected}");
    }

    #[test]
    fn lames_validate() {
        let stack = LameStack {
            origin: [0.0, 0.0, 0.0],
            axis: [0.0, -1.0, 0.0],
            toward: [1.0, 0.0, 0.0],
            count: 3,
            radius: 0.9,
            step: 0.05,
            height: 0.42,
            overlap: 0.2,
            half_angle: 1.9,
            thickness: 0.03,
            cap: true,
        };
        limb_lames(&stack).into_part("lames", OutlineMode::Welded).validate().unwrap();
    }
}
