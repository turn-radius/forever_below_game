//! A coat's markings (the race row's `coat`): socks on the lower legs and pied patches on the
//! trunk, laid on the skin as thin parts of their own so a pose carries them with the skin they
//! lie on. A blaze is part of the head ([`crate::head`]).

use std::f32::consts::TAU;

use armory::geom::vec3::{self, V3};
use armory::geom::{GridOptions, MeshBuilder, OutlineMode, SweepOptions, stitch_grid, tube_varying};
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
