//! A coat's markings (the race row's `coat`): socks on the lower legs and pied patches on the
//! trunk, laid on the skin as thin parts of their own so a pose carries them with the skin they
//! lie on. A blaze is part of the head ([`crate::head`]).

use std::f32::consts::TAU;

use armory::geom::vec3::{self, V3};
use armory::geom::{GridOptions, MeshBuilder, OutlineMode, SweepOptions, stitch_grid, tube, tube_varying};
use armory::{MaterialKind, Part, Rgb};
use procgen_rng::Mulberry32;

use crate::body::Body;
use crate::limb::Side;
use crate::parts::Marking;

/// The white of a blaze, socks and patches.
pub const WHITE: Rgb = Rgb::hex(0xd8d0bf);
/// How far a marking stands off the skin it lies on, in units: a thin film, so cloth cut to
/// the skin still clears it.
const FILM: f32 = 0.012;

impl Body {
    /// The marking parts of a coated race: `"socks"` (on the lower legs, the skin region of a
    /// leg) and `"markings"` (pied patches on the trunk). Empty for any other.
    pub(crate) fn markings(&self) -> Vec<Part> {
        if !self.race().coat {
            return vec![];
        }
        let mut out = vec![];
        let mut add = |m: MeshBuilder, name: &'static str| {
            if !m.is_empty() {
                out.push(m.into_part(name, OutlineMode::Welded).with_material(MaterialKind::Skin).with_tint(WHITE).without_outline());
            }
        };
        match self.traits().marking {
            Marking::Socks => add(self.socks(), "socks"),
            Marking::Pied => add(self.patches(), "markings"),
            Marking::Blaze | Marking::None => {}
        }
        out
    }

    /// White over each leg from just above the hock to the foot's top.
    fn socks(&self) -> MeshBuilder {
        let mut out = MeshBuilder::new();
        for side in Side::BOTH {
            let leg = self.leg(side);
            let tk = leg.t_joints[1];
            let t0 = tk + 0.55 * (1.0 - tk);
            let n = leg.path.len() - 1;
            let i0 = ((t0 * n as f32) as usize).min(n - 1);
            let sub = &leg.path[i0..];
            let t_of = |t: f32| t0 + (1.0 - t0) * t;
            out.append(&tube_varying(sub, 28, &SweepOptions { up: Some(vec3::Z), ..SweepOptions::default() }, |t, a| self.leg_radius(leg, t_of(t), a) + FILM));
        }
        out
    }

    /// Large irregular patches over the trunk, each a film lying on the skin itself (a grid in
    /// the skin's own angle and height, so it follows the curve).
    fn patches(&self) -> MeshBuilder {
        let mut rng = Mulberry32::new(self.spec.seed ^ 0x9a7c_4e5d);
        let mut out = MeshBuilder::new();
        let (y0, y1) = (self.hip_y + 0.05 * self.trunk_length, self.shoulder_y - 0.05 * self.trunk_length);
        let across = 0.5 * (self.chest.half_width + self.chest.half_depth);
        for _ in 0..9 {
            let angle = rng.range_f32(0.0, TAU);
            let y = rng.range_f32(y0, y1);
            let r = rng.range_f32(0.5, 1.1) * self.girth;
            // An irregular outline: a few low harmonics of the patch's radius.
            let phases = [rng.range_f32(0.0, TAU), rng.range_f32(0.0, TAU), rng.range_f32(0.0, TAU)];
            let edge = |phi: f32| 1.0 + 0.22 * (2.0 * phi + phases[0]).sin() + 0.14 * (3.0 * phi + phases[1]).sin() + 0.08 * (5.0 * phi + phases[2]).sin();
            let (cols, rings) = (28usize, 5usize);
            let rows: Vec<Vec<V3>> = (0..=rings)
                .map(|k| {
                    let rho = 0.2 + 0.8 * k as f32 / rings as f32;
                    (0..cols)
                        .map(|c| {
                            let phi = TAU * c as f32 / cols as f32;
                            let reach = r * rho * edge(phi);
                            let (a, h) = (angle + reach * phi.cos() / across, (y + reach * phi.sin()).clamp(y0, y1));
                            self.trunk_surface(a, h, FILM).0
                        })
                        .collect()
                })
                .collect();
            let mut patch = stitch_grid(&rows, &GridOptions { wrap_around: true, ..Default::default() });
            let (_, n) = self.trunk_surface(angle, y, FILM);
            patch.push_fan_cap(&rows[0], n);
            out.append(&patch);
        }
        out
    }
}

impl Body {
    /// The colour of the fur on the legs, for a race whose row has `fur`.
    pub fn fur_colour(&self) -> Option<Rgb> {
        let row = self.race().fur.as_ref()?;
        let i = ((self.traits().fur * row.colours.len() as f32) as usize).min(row.colours.len() - 1);
        Some(row.colours[i].0)
    }

    /// Shaggy locks over each thigh and shank, and a feather down the back of each pastern: a
    /// race's `fur`. Each lock roots in the skin and hangs down along the leg, a little out
    /// from it, longer at the back and the outside; one part (`"leg_fur"`) skinned to the legs.
    pub(crate) fn leg_fur(&self) -> MeshBuilder {
        let Some(row) = self.race().fur.as_ref() else { return MeshBuilder::new() };
        let mut rng = Mulberry32::new(self.spec.seed ^ 0x5f3a_91c7);
        let mut out = MeshBuilder::new();
        let s = self.scale * row.shag;
        for side in Side::BOTH {
            let leg = self.leg(side);
            let sg = side.sign();
            let [th, tk] = leg.t_joints;
            let rows = 26usize;
            for r in 0..rows {
                let t = (0.8 * th) + (0.985 - 0.8 * th) * r as f32 / (rows - 1) as f32;
                let c = leg.point_at(t);
                let along = leg.tangent_at(t);
                let front = vec3::normalize_or(vec3::sub(vec3::Z, vec3::scale(along, along[2])), vec3::Z);
                let across = vec3::normalize_or(vec3::cross(along, front), vec3::X);
                // Fuller and longer toward the hock and down the back; sparse over the knee.
                let below = (t - tk) / (1.0 - tk);
                let base_len = if t < tk { 0.35 + 0.25 * ((t - th) / (tk - th)).clamp(0.0, 1.0) } else { 0.6 + 0.3 * below };
                let count = 11;
                for k in 0..count {
                    let a = TAU * (k as f32 + rng.range_f32(-0.4, 0.4)) / count as f32;
                    // a = 0 is the front; the back (PI) is shaggiest.
                    let back = 0.5 * (1.0 - a.cos());
                    let outside = (a.sin() * sg).max(0.0);
                    if a.cos() > 0.6 && t > tk - 0.05 && t < tk + 0.1 {
                        continue; // a bare knee
                    }
                    let dir = vec3::normalize_or(vec3::add(vec3::scale(front, a.cos()), vec3::scale(across, a.sin())), front);
                    let rad = self.leg_radius(leg, t, a);
                    let root = vec3::add(c, vec3::scale(dir, rad - 0.04 * s));
                    let len = base_len * s * rng.range_f32(0.7, 1.25) * (0.55 + 0.6 * back + 0.25 * outside);
                    let flare = rng.range_f32(0.35, 0.8);
                    let wave = rng.range_f32(-0.12, 0.12) * s;
                    let path: Vec<V3> = (0..=4)
                        .map(|i| {
                            let u = i as f32 / 4.0;
                            let hang = vec3::scale(along, len * u);
                            let out_k = flare * len * u * (1.2 - u) * 0.8;
                            let sideways = vec3::scale(across, wave * u * u);
                            vec3::add(vec3::add(root, hang), vec3::add(vec3::scale(dir, out_k), sideways))
                        })
                        .collect();
                    out.append(&tube(&path, 4, &SweepOptions::default(), |u| (0.1 * s) * (1.0 - 0.9 * u) + 0.008));
                }
            }
            // A feather down the back of the pastern from the hock.
            if let Some(foot) = self.hooved_foot(side) {
                let hock = leg.joints[2];
                for k in 0..9 {
                    let a = TAU * (k as f32 + rng.range_f32(-0.3, 0.3)) / 9.0;
                    let at = vec3::add(hock, [0.3 * leg.radius * a.sin(), -0.1 * s, -0.5 * leg.radius * (0.4 + 0.6 * (0.5 - 0.5 * a.cos()))]);
                    let to = vec3::lerp(hock, foot.coronet, rng.range_f32(0.55, 0.9));
                    let mid = vec3::add(vec3::lerp(at, to, 0.5), [0.0, 0.0, -0.1 * s]);
                    out.append(&tube(&[at, mid, to], 4, &SweepOptions::default(), |u| 0.08 * s * (1.0 - 0.85 * u) + 0.008));
                }
            }
        }
        out
    }
}
