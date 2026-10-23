//! Wings: a bat's, a membrane stretched on an arm and fingers, that any race turns on by giving
//! its row in `data/races.json` a `wings` field ([`crate::folk::WingRow`]).
//!
//! - [`Wing`]: one wing's bones laid out in body space for an openness `u` (0 folded against the
//!   back, 1 spread): root, elbow, wrist and a finger tip each, and the line along the back the
//!   membrane is fixed to. Bone directions move between a folded and an open set of angles, so the
//!   same seed gives one wing at every openness.
//! - [`Wing::bones`] and [`Wing::membrane`]: the meshes. The membrane is panels fanned from the wrist
//!   between the fingers (scalloped along the trailing edge, sagging between them), and a sail
//!   from the last finger to the body. A seed may tear it.
//! - [`Wing::skin`]: the weights, from the rig's wing joints (a root on the back and a wrist
//!   for each wing); the membrane near the body follows the trunk.
//!
//! A wing that is not wide open is kept behind the back ([`Body::back_floor`]) with room to
//! spare, so it never goes through the body, and `lift` stands the whole wing further out, over
//! gear on the back (see `docs/body.md`).
//!
//! Space is the body's: 1 unit = 10 cm, soles on y = 0, face +Z, the wearer's left +X. Plain
//! Rust; a pure function of the body and its seed.

use std::f32::consts::{PI, TAU};

use armory::build::form::BACK;
use armory::build::hafted::smoothstep;
use armory::geom::vec3::{self, V3};
use armory::geom::{MeshBuilder, Shading, SweepOptions, spike, tube};
use armory::{ItemMesh, MaterialKind, Rgb};
use armory::geom::OutlineMode;

use crate::body::Body;
use crate::limb::Side;
use crate::parts::{mix, shade};

/// Bone lengths as shares of the wing's reach: upper arm, forearm, then the fingers.
const HUMERUS: f32 = 0.2;
const FOREARM: f32 = 0.27;
const FINGERS: [f32; 4] = [0.64, 0.62, 0.52, 0.4];

/// A bone's heading: how far above level and how far toward the back from straight out.
type Heading = (f32, f32);

const HUMERUS_FOLDED: Heading = (1.35, 1.0);
const HUMERUS_OPEN: Heading = (0.7, 0.35);
const FOREARM_FOLDED: Heading = (0.9, 1.0);
const FOREARM_OPEN: Heading = (0.4, 0.5);
const FINGERS_FOLDED: [Heading; 4] = [(-1.5, 0.0), (-1.52, 0.3), (-1.54, 0.6), (-1.56, 0.9)];
const FINGERS_OPEN: [Heading; 4] = [(0.18, 0.5), (-0.25, 0.62), (-0.62, 0.74), (-0.98, 0.86)];

fn heading(sg: f32, (theta, psi): Heading) -> V3 {
    [sg * theta.cos() * psi.cos(), theta.sin(), -theta.cos() * psi.sin()]
}

fn between(a: Heading, b: Heading, u: f32) -> Heading {
    (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u)
}

/// One wing, laid out in body space.
#[derive(Clone, Debug)]
pub struct Wing {
    pub side: Side,
    /// Where the wing joins the back, on the skin, and where its bones start (standing out by
    /// the lift).
    pub root_skin: V3,
    pub root: V3,
    pub elbow: V3,
    pub wrist: V3,
    /// The ends of the fingers, leading finger first.
    pub tips: Vec<V3>,
    /// The line along the back the sail is fixed to, from the root down to the hip.
    pub edge: Vec<V3>,
    /// How thick the arm bone is at the root.
    pub bone_radius: f32,
    /// How open it is, 0..=1.
    pub open: f32,
    /// How the membrane tears: (panel, how far along its trailing edge) of up to two tears.
    tears: Vec<(usize, f32)>,
    /// How far the membrane sags between the fingers, and the lift.
    sag: f32,
    lift: f32,
    /// How far behind the skin the folded wing is kept.
    clearance: f32,
    /// The back's reach behind it at a height ([`Body::back_floor`]) sampled up the body.
    floor: Vec<(f32, f32)>,
}

impl Body {
    /// The wing of `side`, if the race has wings, standing `lift` out from the back (over gear),
    /// at the openness its seed rolled.
    pub fn wing(&self, side: Side, lift: f32) -> Option<Wing> {
        let row = self.race().wings.as_ref()?;
        let tr = self.traits().wings;
        let open = row.open[0] + (row.open[1] - row.open[0]) * tr.open;
        Some(self.wing_at(side, lift, open))
    }

    /// The same wing at openness `open`.
    pub fn wing_at(&self, side: Side, lift: f32, open: f32) -> Wing {
        let row = self.race().wings.as_ref().expect("a race with wings");
        let tr = self.traits().wings;
        let sg = side.sign();
        let s = self.scale;
        let t = self.trunk_length;
        let reach = (row.span[0] + (row.span[1] - row.span[0]) * tr.span) * self.height * 0.5;
        // The root: between the shoulder blades, a little off the spine.
        let y_root = self.shoulder_y - 0.17 * t;
        let x_root = sg * 0.3 * self.shoulder_half_width;
        let floor: Vec<(f32, f32)> = (0..=48).map(|k| {
            let y = self.height * k as f32 / 48.0;
            (y, self.back_floor(y))
        }).collect();
        let long_hair = matches!(self.spec.hair, crate::folk::HairStyle::Long | crate::folk::HairStyle::Ponytail);
        let clearance = (0.12 + if long_hair { 0.5 } else { 0.0 }) * s;
        let root_skin = [x_root, y_root, self.back_z(x_root, y_root)];
        let root = vec3::add(root_skin, [0.0, 0.0, -lift - 0.25 * s]);
        let wing = Wing {
            side,
            root_skin,
            root,
            elbow: root,
            wrist: root,
            tips: vec![],
            edge: vec![],
            bone_radius: 0.45 * self.arm(side).radius,
            open,
            tears: Self::tears(row, &tr),
            sag: 0.35 * s.sqrt(),
            lift,
            clearance,
            floor,
        };
        let mut wing = wing;
        let u = smoothstep(0.0, 1.0, open);
        let at = |from: V3, h: Heading, len: f32| vec3::add(from, vec3::scale(heading(sg, h), len * reach));
        wing.elbow = at(root, between(HUMERUS_FOLDED, HUMERUS_OPEN, u), HUMERUS);
        wing.wrist = at(wing.elbow, between(FOREARM_FOLDED, FOREARM_OPEN, u), FOREARM);
        for i in 0..row.fingers.clamp(3, 4) {
            let tip = at(wing.wrist, between(FINGERS_FOLDED[i], FINGERS_OPEN[i], u), FINGERS[i]);
            wing.tips.push(tip);
        }
        // The sail's edge runs down the back from the root to the hip, a little in from the side.
        let y_hip = self.hip_y + 0.12 * t;
        let x_hip = sg * 0.5 * self.hip.half_width;
        wing.edge = (0..=6)
            .map(|k| {
                let f = k as f32 / 6.0;
                let (x, y) = (x_root + (x_hip - x_root) * f, y_root + (y_hip - y_root) * f);
                [x, y, self.back_z(x, y) - lift - 0.25 * s]
            })
            .collect();
        wing.elbow = wing.settle(wing.elbow);
        wing.wrist = wing.settle(wing.wrist);
        let tips = std::mem::take(&mut wing.tips);
        wing.tips = tips.into_iter().map(|tip| wing.settle(tip)).collect();
        wing
    }

    fn tears(row: &crate::folk::WingRow, tr: &crate::parts::WingRoll) -> Vec<(usize, f32)> {
        if tr.tear_roll >= row.tear {
            return vec![];
        }
        let panels = row.fingers.clamp(3, 4);
        let pick = |roll: f32| ((roll * panels as f32) as usize).min(panels - 1);
        let mut out = vec![(pick(tr.tear_panel), tr.tear_at)];
        if tr.tear_roll < 0.5 * row.tear {
            out.push((pick(tr.tear2_panel), tr.tear2_at));
        }
        out
    }

    /// The root and the wrist of `side`'s wing at rest, the rig's wing joints.
    pub fn wing_joints(&self, side: Side) -> Option<[V3; 2]> {
        let w = self.wing(side, 0.0)?;
        Some([w.root_skin, w.wrist])
    }

    /// The back's surface behind the body at lateral `x` and height `y`: the trunk's, found by
    /// the section nearest that `x`.
    pub fn back_z(&self, x: f32, y: f32) -> f32 {
        let y = y.clamp(self.hip_y - 0.14 * self.trunk_length, self.shoulder_y + 0.1 * self.trunk_length);
        (0..=36)
            .map(|k| {
                let a = BACK + (k as f32 / 36.0 - 0.5) * 1.8;
                self.trunk().section(a, y, 0.0)
            })
            .min_by(|a, b| (a[0] - x).abs().total_cmp(&(b[0] - x).abs()))
            .map_or(-1.0, |p| p[1])
    }

    /// How far back anything of the body reaches at height `y`: the trunk's seat and back,
    /// the legs (hocks and thighs) below the hips and the head above.
    pub fn back_floor(&self, y: f32) -> f32 {
        let yc = y.clamp(self.hip_y - 0.14 * self.trunk_length, self.shoulder_y + 0.1 * self.trunk_length);
        let mut z = (0..=24)
            .map(|k| self.trunk().section(BACK + (k as f32 / 24.0 - 0.5) * 1.8, yc, 0.0)[1])
            .fold(0.0f32, f32::min);
        for side in Side::BOTH {
            let leg = self.leg(side);
            for (i, p) in leg.path.iter().enumerate() {
                if (p[1] - y).abs() < 0.6 * self.scale {
                    z = z.min(p[2] - leg.radius_at(i as f32 / (leg.path.len() - 1) as f32));
                }
            }
            if let Some(foot) = self.hooved_foot(side) {
                if y < foot.coronet[1] + 0.6 * self.scale {
                    z = z.min(foot.heel - 0.3 * self.scale);
                }
            }
        }
        if y > self.chin_y && y < self.height {
            z = z.min(self.head.centre[2] - self.head.half_depth);
        }
        z
    }

    /// The parts of both wings: `"wing_bones"` and `"wing_membrane"`, standing `lift` out from
    /// the back.
    pub fn wing_parts(&self, lift: f32) -> Vec<armory::Part> {
        let Some(row) = self.race().wings.as_ref() else { return vec![] };
        let tr = self.traits().wings;
        let skin = self.spec.skin;
        let bone_tint = shade(skin, -0.12 - 0.2 * tr.bone);
        let membrane_tint = if row.membrane.is_empty() {
            shade(mix(skin, Rgb::hex(0x2a1210), 0.3), -0.1)
        } else {
            let i = ((tr.tone * row.membrane.len() as f32) as usize).min(row.membrane.len() - 1);
            row.membrane[i].0
        };
        let (mut bones, mut membrane) = (MeshBuilder::new(), MeshBuilder::new());
        for side in Side::BOTH {
            let Some(wing) = self.wing(side, lift) else { continue };
            bones.append(&wing.bones());
            membrane.append(&wing.membrane());
        }
        let mut parts = vec![];
        if !bones.is_empty() {
            parts.push(bones.into_part("wing_bones", OutlineMode::Welded).with_material(MaterialKind::Skin).with_tint(bone_tint));
        }
        if !membrane.is_empty() {
            parts.push(membrane.into_part("wing_membrane", OutlineMode::Welded).with_material(MaterialKind::Skin).with_tint(membrane_tint).without_outline());
        }
        parts
    }

    /// The bare skin with the wings standing `lift` out from the back (a folk without wings is
    /// [`Body::skin_mesh`] as it was).
    pub fn skin_mesh_lifted(&self, lift: f32) -> ItemMesh {
        let mut mesh = self.skin_mesh();
        if lift != 0.0 {
            mesh.parts.retain(|p| !p.name.starts_with("wing_"));
            mesh.parts.extend(self.wing_parts(lift));
        }
        mesh
    }
}

impl Wing {
    fn sg(&self) -> f32 {
        self.side.sign()
    }

    /// A point of the wing pushed behind the back where the wing is folded.
    fn settle(&self, p: V3) -> V3 {
        self.settle_with(p, self.open)
    }

    fn settle_with(&self, p: V3, open: f32) -> V3 {
        let y = p[1].clamp(0.0, self.floor.last().map_or(0.0, |f| f.0));
        let k = (y / self.floor.last().map_or(1.0, |f| f.0).max(1e-3) * (self.floor.len() - 1) as f32).clamp(0.0, (self.floor.len() - 1) as f32);
        let (i, f) = ((k.floor() as usize).min(self.floor.len() - 2), k - k.floor());
        let z_floor = self.floor[i].1 * (1.0 - f) + self.floor[i + 1].1 * f - self.clearance - self.lift;
        let hold = 1.0 - smoothstep(0.5, 0.95, open);
        let z = if p[2] > z_floor { p[2] + (z_floor - p[2]) * hold } else { p[2] };
        [p[0], p[1], z]
    }

    /// The arm and the fingers, with the knuckles, the thumb claw and the stalk joining the
    /// root to the back.
    pub fn bones(&self) -> MeshBuilder {
        let r = self.bone_radius;
        let mut out = MeshBuilder::new();
        let chain = |a: V3, b: V3, bow: V3, n: usize| -> Vec<V3> {
            (0..=n)
                .map(|k| {
                    let f = k as f32 / n as f32;
                    let p = vec3::add(vec3::lerp(a, b, f), vec3::scale(bow, (PI * f).sin()));
                    self.settle(p)
                })
                .collect()
        };
        let up = [0.0, 1.0, 0.0];
        // The stalk through whatever is on the back, then the arm.
        out.append(&tube(&[self.root_skin, self.root], 10, &SweepOptions::default(), |_| 1.25 * r));
        out.append(&tube(&chain(self.root, self.elbow, vec3::scale(up, 0.0), 6), 12, &SweepOptions::default(), |t| r * (1.2 - 0.25 * t)));
        out.append(&tube(&chain(self.elbow, self.wrist, vec3::scale(up, 0.0), 8), 12, &SweepOptions::default(), |t| r * (0.95 - 0.3 * t)));
        for (center, k) in [(self.root, 1.35), (self.elbow, 1.2), (self.wrist, 1.35)] {
            let mut ball = armory::geom::uv_sphere(1.0, 12, 8);
            ball.scale_uniform(r * k).translate(center);
            out.append(&ball);
        }
        for (i, &tip) in self.tips.iter().enumerate() {
            let bow = vec3::scale(heading(self.sg(), (0.6, 1.2)), 0.04 * vec3::distance(self.wrist, tip));
            let f0 = 0.7 - 0.08 * i as f32;
            out.append(&tube(&chain(self.wrist, tip, bow, 9), 8, &SweepOptions::default(), move |t| r * f0 * (1.0 - 0.7 * t) + 0.015));
            // A hooked claw at the end of each finger.
            let dir = vec3::normalize_or(vec3::sub(tip, self.wrist), vec3::neg(vec3::Y));
            let mut claw = spike(0.6 * r * f0, 1.7 * r, 6, 0.0, Shading::SMOOTH);
            claw.place(tip, dir);
            out.append(&claw);
        }
        // The thumb: a claw standing from the wrist, leading the wing.
        let lead = vec3::normalize_or(vec3::sub(self.wrist, self.elbow), vec3::Y);
        let mut thumb = spike(0.8 * r, 3.2 * r, 8, 0.0, Shading::SMOOTH);
        thumb.place(self.wrist, vec3::normalize_or(vec3::add(lead, [0.0, 0.5, -0.3]), vec3::Y));
        out.append(&thumb);
        out
    }

    /// The membrane: one panel between each pair of fingers, a sail from the last finger to
    /// the back, and a strip between the arm and the first finger.
    pub fn membrane(&self) -> MeshBuilder {
        let mut out = MeshBuilder::new();
        let n = self.tips.len();
        // The membrane sags between fingers more when the wing is folded.
        let slack = self.sag * (1.0 - 0.6 * self.open);
        for i in 0..n - 1 {
            let (a, b) = (self.tips[i], self.tips[i + 1]);
            let edge = |t: f32| vec3::lerp(a, b, t);
            let tear = self.tears.iter().find(|(p, _)| *p == i).map(|&(_, at)| at);
            out.append(&self.panel(self.wrist, &edge, 0.17, slack, tear, 14, 18));
        }
        // The sail: from the last finger's tip along the trailing edge to the hip, then up the
        // back to the root.
        let last = self.tips[n - 1];
        let mut line = vec![last];
        line.extend(self.edge.iter().rev().copied());
        let lengths: Vec<f32> = std::iter::once(0.0).chain(line.windows(2).scan(0.0, |acc, w| { *acc += vec3::distance(w[0], w[1]); Some(*acc) })).collect();
        let total = lengths.last().copied().unwrap_or(1.0).max(1e-4);
        let along = |t: f32| -> V3 {
            let d = t * total;
            let i = lengths.windows(2).position(|w| d <= w[1]).unwrap_or(line.len() - 2);
            let f = ((d - lengths[i]) / (lengths[i + 1] - lengths[i]).max(1e-6)).clamp(0.0, 1.0);
            vec3::lerp(line[i], line[i + 1], f)
        };
        let tear = self.tears.iter().find(|(p, _)| *p == n - 1).map(|&(_, at)| at);
        // The scalloped edge hangs between the tip and the hip only; the part up the back is fixed.
        let edge_len = vec3::distance(last, self.edge[self.edge.len() - 1]);
        let free = (edge_len / total).clamp(0.05, 1.0);
        let sail_edge = |t: f32| along(t);
        out.append(&self.panel_free(self.wrist, &sail_edge, 0.16, free, slack, tear, 16, 24));
        // The leading edge: the membrane between the upper arm, the forearm and the root-wrist line.
        let lead = |t: f32| vec3::lerp(self.root, self.elbow, t);
        let mut front = self.panel_free(self.wrist, &lead, 0.0, 0.0, 0.3 * slack, None, 4, 8);
        front.recompute_smooth_normals();
        out.append(&front);
        out.make_double_sided();
        out
    }

    fn panel(&self, hub: V3, edge: &dyn Fn(f32) -> V3, scallop: f32, slack: f32, tear: Option<f32>, rows: usize, cols: usize) -> MeshBuilder {
        self.panel_free(hub, edge, scallop, 1.0, slack, tear, rows, cols)
    }

    /// A panel fanned from `hub` to the outer `edge`. The edge's scallop (`scallop` of the
    /// length, deepest mid-way) applies over the first `free` of it; `slack` is how far the
    /// membrane sags out of its plane; `tear` is where a tear opens along the edge.
    #[allow(clippy::too_many_arguments)]
    fn panel_free(&self, hub: V3, edge: &dyn Fn(f32) -> V3, scallop: f32, free: f32, slack: f32, tear: Option<f32>, rows: usize, cols: usize) -> MeshBuilder {
        let sg = self.sg();
        let mut grid: Vec<Vec<V3>> = vec![];
        let (e0, e1) = (edge(0.0), edge(1.0));
        let plane = vec3::cross(vec3::sub(e0, hub), vec3::sub(e1, hub));
        // Out of the body: toward the back and the side.
        let mut sag_dir = vec3::normalize_or(plane, [0.0, 0.0, -1.0]);
        if vec3::dot(sag_dir, [0.4 * sg, 0.0, -1.0]) < 0.0 {
            sag_dir = vec3::neg(sag_dir);
        }
        for i in 0..=rows {
            let s = i as f32 / rows as f32;
            let mut row = vec![];
            for j in 0..=cols {
                let t = j as f32 / cols as f32;
                let in_free = if free > 0.0 { (t / free).min(1.0) } else { 0.0 };
                let cup = scallop * (PI * in_free).sin().max(0.0).powf(0.8) * if t < free { 1.0 } else { 0.0 };
                let far = vec3::sub(edge(t), hub);
                let reach = 1.0 - cup * smoothstep(0.35, 1.0, s);
                let bulge = slack * (PI * t).sin() * s * (1.0 - 0.3 * s);
                let p = vec3::add(vec3::add(hub, vec3::scale(far, s * reach)), vec3::scale(sag_dir, bulge));
                row.push(self.settle(p));
            }
            grid.push(row);
        }
        let mut m = MeshBuilder::new();
        let cols_n = cols + 1;
        let mut ids = vec![];
        for row in &grid {
            for &p in row {
                ids.push(m.push_vertex(p, [0.0, 0.0, 1.0], [0.0, 0.0]));
            }
        }
        for i in 0..rows {
            for j in 0..cols {
                let (s, t) = ((i as f32 + 0.5) / rows as f32, (j as f32 + 0.5) / cols as f32);
                if let Some(at) = tear {
                    // A tear opens from the trailing edge, wider at the edge.
                    let depth = 0.45;
                    if s > depth && (t - at).abs() < 0.07 * (s - depth) / (1.0 - depth) + 0.012 {
                        continue;
                    }
                }
                let (a, b, c, d) = (ids[i * cols_n + j], ids[i * cols_n + j + 1], ids[(i + 1) * cols_n + j + 1], ids[(i + 1) * cols_n + j]);
                m.push_triangle(a, c, b).push_triangle(a, d, c);
            }
        }
        m.recompute_smooth_normals();
        m
    }

    /// The weights of a point of the wing or its membrane: the bones' joints along the bone it
    /// is nearest, and the trunk's near the line it is fixed to.
    pub fn skin(&self) -> WingSkin {
        let mut bones = vec![(self.root, self.elbow, 0.0, 0.0), (self.elbow, self.wrist, 0.3, 1.0)];
        for &tip in &self.tips {
            bones.push((self.wrist, tip, 1.0, 1.0));
        }
        WingSkin { bones, edge: self.edge.clone() }
    }
}

/// A wing's weights ([`Wing::skin`]).
#[derive(Clone, Debug)]
pub struct WingSkin {
    /// Bones as `(from, to, share on the wrist joint at each end)`.
    bones: Vec<(V3, V3, f32, f32)>,
    edge: Vec<V3>,
}

impl WingSkin {
    /// `(share on the root joint, on the wrist joint, on the trunk)` of the point `p`, summing to 1.
    pub fn weights(&self, p: V3) -> (f32, f32, f32) {
        let nearest = |segs: &mut dyn Iterator<Item = (V3, V3, f32, f32)>| -> (f32, f32) {
            let mut best = (f32::MAX, 0.0);
            for (a, b, w0, w1) in segs {
                let ab = vec3::sub(b, a);
                let f = (vec3::dot(vec3::sub(p, a), ab) / vec3::length_squared(ab).max(1e-9)).clamp(0.0, 1.0);
                let d = vec3::distance(p, vec3::add(a, vec3::scale(ab, f)));
                if d < best.0 {
                    best = (d, w0 + (w1 - w0) * f);
                }
            }
            best
        };
        let (d_bone, wrist) = nearest(&mut self.bones.iter().copied());
        let (d_body, _) = nearest(&mut self.edge.windows(2).map(|w| (w[0], w[1], 0.0, 0.0)));
        let t = d_body / (d_body + d_bone + 1e-4);
        let wing = smoothstep(0.1, 0.75, t);
        (wing * (1.0 - wrist), wing * wrist, 1.0 - wing)
    }
}

/// How the wings move: the root joint swings the whole wing out and the wrist opens the
/// fingers, each by an angle given by `spread` (0 rest, 1 as far as a walk takes them) and `flap`.
pub fn wing_rotation(side: Side, spread: f32, flap: f32) -> [crate::rig::Quat; 2] {
    use crate::rig::Quat;
    let sg = side.sign();
    let root = Quat::z(sg * 0.2 * spread).mul(Quat::y(-sg * 0.1 * spread)).mul(Quat::x(0.05 * flap));
    let wrist = Quat::z(sg * 0.26 * spread).mul(Quat::x(0.12 * flap));
    let _ = TAU;
    [root, wrist]
}
