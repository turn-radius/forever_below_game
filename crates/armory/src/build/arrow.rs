//! Arrows and crossbow bolts from the [`crate::sections::ranged::Arrow`] section: shaft,
//! nock, head and fletching, built along +Y with the nock end at the origin, for the family
//! to lay on its string.
//!
//! From the prototype's `buildArrow` in `reference/spline_armory_engine.js` (which the
//! prototype defines but never draws), rebuilt with a barrelled shaft, a leaf head with a
//! real diamond section, a midrib on the broadhead, and vanes set at the shaft's surface.

use crate::geom::{Frame, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, extrude, lathe, spline_profile, sweep_scaled, vec3};
use crate::sections::ranged::{Arrow, ArrowHead, FletchCut};

/// An arrow's pieces, one per material.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArrowMeshes {
    pub shaft: MeshBuilder,
    /// A turned nock on the end of the shaft (bows). Empty for a bolt.
    pub nock: MeshBuilder,
    pub head: MeshBuilder,
    pub fletching: MeshBuilder,
}

impl ArrowMeshes {
    /// Every piece, in a fixed order, for moving them together.
    fn all_mut(&mut self) -> [&mut MeshBuilder; 4] {
        [&mut self.shaft, &mut self.nock, &mut self.head, &mut self.fletching]
    }

    /// Turns the arrow `roll` radians about its own axis (which way the vanes face), then
    /// sets its nock end on `nock`, pointing along `direction`.
    pub fn placed(mut self, nock: V3, direction: V3, roll: f32) -> ArrowMeshes {
        for m in self.all_mut() {
            m.rotate_y(roll).place(nock, direction);
        }
        self
    }
}

/// Where the arrow's point reaches from its nock end.
pub fn arrow_reach(arrow: &Arrow) -> f32 {
    arrow.length - 0.02 + arrow.head_length
}

/// Builds `arrow` along +Y from its nock end at the origin. `nocked` adds a turned nock for
/// a bowstring; a crossbow bolt's butt is left square.
pub fn arrow_meshes(arrow: &Arrow, nocked: bool) -> ArrowMeshes {
    let r = arrow.radius;
    let len = arrow.length;
    // Barrelled: fullest a third of the way along, slimmer at both ends.
    let shaft_profile = spline_profile(&[[r * 0.86, 0.0], [r, len * 0.35], [r * 0.97, len * 0.7], [r * 0.85, len]], 16);
    let shaft = lathe(&shaft_profile, &LatheOptions { facets: 12, shading: Shading::SMOOTH, ..Default::default() });

    let nock = if nocked {
        let control = [[0.0, -0.06], [r * 1.05, -0.062], [r * 1.32, -0.03], [r * 1.3, 0.04], [r * 0.92, 0.09]];
        lathe(&spline_profile(&control, 12), &LatheOptions { facets: 12, shading: Shading::SMOOTH, ..Default::default() })
    } else {
        MeshBuilder::new()
    };

    let mut head = head_mesh(arrow);
    head.translate([0.0, len - 0.02, 0.0]);

    ArrowMeshes { shaft, nock, head, fletching: fletching_mesh(arrow) }
}

/// The head along +Y from its socket at the origin.
fn head_mesh(arrow: &Arrow) -> MeshBuilder {
    let (r, hl, hw) = (arrow.radius, arrow.head_length, arrow.head_width);
    let socket = || {
        let control = [[r * 1.12, -0.04], [r * 1.25, hl * 0.12], [r * 1.05, hl * 0.25], [r * 0.6, hl * 0.4]];
        lathe(&spline_profile(&control, 10), &LatheOptions { facets: 12, shading: Shading::SMOOTH, ..Default::default() })
    };
    match arrow.head {
        ArrowHead::Bodkin => {
            let profile = [[r * 1.15, -0.03], [(r * 1.3).max(hw * 0.5), hl * 0.18], [hw * 0.38, hl * 0.5], [hw * 0.12, hl * 0.85], [0.0, hl]];
            lathe(&profile, &LatheOptions { facets: 4, shading: Shading::FACETED, phase: std::f32::consts::FRAC_PI_4, ..Default::default() })
        }
        ArrowHead::Blunt => {
            let control = [[r * 1.1, -0.02], [hw * 0.5, hl * 0.3], [hw * 0.55, hl * 0.62], [hw * 0.32, hl * 0.92], [0.0, hl]];
            lathe(&spline_profile(&control, 16), &LatheOptions { facets: 16, shading: Shading::SMOOTH, ..Default::default() })
        }
        ArrowHead::Broadhead => {
            let outline = [
                [0.0, hl],
                [hw * 0.5, hl * 0.22],
                [hw * 0.48, -hl * 0.1],
                [r * 1.4, hl * 0.12],
                [-r * 1.4, hl * 0.12],
                [-hw * 0.48, -hl * 0.1],
                [-hw * 0.5, hl * 0.22],
            ];
            let mut out = extrude(&outline, &[], 0.012);
            // A midrib down the blade.
            let rib = [[r * 0.75, 0.0], [r * 0.55, hl * 0.6], [0.0, hl * 0.96]];
            out.append(&lathe(&rib, &LatheOptions { facets: 4, shading: Shading::FACETED, ..Default::default() }));
            out.append(&socket());
            out
        }
        ArrowHead::Leaf => {
            let rows = 24;
            let frames: Vec<Frame> = (0..=rows)
                .map(|i| Frame { origin: [0.0, hl * i as f32 / rows as f32, 0.0], tangent: vec3::Y, normal: vec3::X, binormal: vec3::neg(vec3::Z) })
                .collect();
            let diamond = [[1.0, 0.0], [0.0, 1.0], [-1.0, 0.0], [0.0, -1.0]];
            let options = SweepOptions { shading: Shading::FACETED, ..Default::default() };
            let mut out = sweep_scaled(&frames, &diamond, &options, |f| {
                let w = hw * 0.5 * (std::f32::consts::PI * f.powf(0.75)).sin().max(0.0).powf(0.9) + r * 1.1 * (1.0 - f).powi(4);
                [w, w * 0.2 + 0.004 * (1.0 - f)]
            });
            out.append(&socket());
            out
        }
    }
}

/// The vanes, evenly round the shaft near the nock end. Two vanes lie flat, three or more
/// put the first (the cock feather) square to the string.
fn fletching_mesh(arrow: &Arrow) -> MeshBuilder {
    let (fl, fh) = (arrow.fletch_length, arrow.fletch_height);
    let outline: Vec<[f32; 2]> = match arrow.fletch_cut {
        FletchCut::Shield => vec![[0.0, 0.0], [fh * 0.75, 0.0], [fh, fl * 0.2], [fh, fl * 0.55], [fh * 0.62, fl * 0.75], [fh * 0.35, fl * 0.95], [0.0, fl]],
        FletchCut::Parabolic => {
            let mut v = vec![[0.0, 0.0]];
            v.extend((1..12).map(|k| {
                let f = k as f32 / 12.0;
                [fh * (1.0 - (2.0 * f - 0.35).powi(2) / 2.8).clamp(0.05, 1.0) * (1.0 - f).powf(0.35), fl * f]
            }));
            v.push([0.0, fl]);
            v
        }
        FletchCut::Triangle => vec![[0.0, 0.0], [fh, 0.0], [fh * 0.5, fl * 0.5], [0.0, fl]],
    };
    let mut vane = extrude(&outline, &[], 0.008);
    vane.translate([arrow.radius * 0.85, 0.12, 0.0]);
    let n = arrow.fletches.max(2);
    let mut out = MeshBuilder::new();
    for k in 0..n {
        let mut v = vane.clone();
        let offset = if n == 2 { 0.0 } else { std::f32::consts::FRAC_PI_2 };
        v.rotate_y(std::f32::consts::TAU * k as f32 / n as f32 + offset);
        out.append(&v);
    }
    out
}
