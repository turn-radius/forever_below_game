//! Geometry for a [`Socket`]: the hollow cone a head is seated on, its raised rings, and
//! the langets nailed down the haft below it.
//!
//! From the prototype's socket in `buildBladed()` (`reference/spline_armory_engine.js`),
//! rebuilt so the socket swallows the end of the haft the way a forged socket does, rather
//! than standing on top of it: the lower part of its length is a sleeve over the shaft,
//! with a rolled lip at the mouth, and the rest closes to a solid neck for the blade. Rings
//! are turned mouldings that follow its profile, and langets are strips laid on the haft's
//! surface (bending with it) and nailed through with domed rivets.

use std::f32::consts::{PI, TAU};

use super::hafted::{Haft, radius_on, turned};
use crate::geom::{Frame, LatheOptions, MeshBuilder, Shading, SweepOptions, lathe, spline_profile, sweep_frames, uv_sphere, vec3};

use crate::sections::socket::Socket;

/// Share of a socket's length that sleeves the haft; the rest stands above it.
pub const SLEEVE: f32 = 0.55;

/// A socket built in head space (y = 0 is the top of the haft, +Y continues it).
#[derive(Clone, Debug, PartialEq)]
pub struct SocketMesh {
    pub mesh: MeshBuilder,
    /// `[radius, y]` of its outer surface, mouth to tip.
    pub profile: Vec<[f32; 2]>,
    /// Height of the mouth (below the top of the haft).
    pub mouth: f32,
    /// Height of the tip, where a blade is seated.
    pub top: f32,
}

impl SocketMesh {
    /// The outer radius at height `y`.
    pub fn radius_at(&self, y: f32) -> f32 {
        radius_on(&self.profile, y)
    }
}

/// Builds `socket` over a haft whose radius at its top is `haft_radius`, closing to a neck
/// of radius `neck` under the blade. Faceted sockets (12 facets or fewer) keep their flats
/// clear of the round haft inside.
pub fn socket_mesh(socket: &Socket, haft_radius: f32, neck: f32) -> SocketMesh {
    let ls = socket.length.max(0.1);
    let (sides, shading) = turned(socket.facets);
    let clear = if shading.smooth_around { 1.0 } else { 1.0 / (PI / sides as f32).cos() };
    let rs = haft_radius * clear * 1.02;
    let sr = (haft_radius * socket.flare * clear).max(rs);
    let neck = neck.min(rs * 0.95);
    let sleeve = ls * SLEEVE;
    let control = [
        [rs * 1.1, 0.0],
        [rs * 1.12, ls * 0.025],
        [rs * 1.03, ls * 0.05],
        [sr, ls * 0.12],
        [sr * 0.97 + rs * 0.03, ls * 0.4],
        [(sr + rs) * 0.5, sleeve],
        [(rs + neck) * 0.5, sleeve + (ls - sleeve) * 0.5],
        [neck, ls * 0.96],
        [neck * 0.4, ls * 0.995],
        [0.0, ls],
    ];
    let profile: Vec<[f32; 2]> = spline_profile(&control, 56)
        .into_iter()
        // Never pinch in round the haft inside the sleeve.
        .map(|[r, y]| [if y <= sleeve + 0.02 { r.max(rs) } else { r }, y - sleeve])
        .collect();
    let mut mesh = lathe(&profile, &LatheOptions { facets: sides, shading, cap_start: true, cap_end: false, phase: 0.0 });
    let n = socket.rings;
    for k in 0..n {
        let y = ls * (0.06 + 0.3 * if n == 1 { 0.5 } else { k as f32 / (n - 1) as f32 }) - sleeve;
        let r = radius_on(&profile, y);
        let b = 0.022 + ls * 0.006;
        let ring = [[r * 0.98, -b], [r + b * 0.6, -b * 0.55], [r + b * 0.75, 0.0], [r + b * 0.6, b * 0.55], [r * 0.98, b]];
        let mut band = lathe(&spline_profile(&ring, 10), &LatheOptions { facets: sides, shading: Shading { smooth_around: shading.smooth_around, smooth_along: true }, ..Default::default() });
        band.translate([0.0, y, 0.0]);
        mesh.append(&band);
    }
    SocketMesh { mesh, profile, mouth: -sleeve, top: ls - sleeve }
}

/// `count` langets running `length` down the haft from `t_start` (where the socket's mouth
/// sits), laid on the haft's surface and each nailed with domed rivets. The first lies at
/// `phase` radians round the haft from its frame normal (head space +X); the rest are
/// evenly spaced.
pub fn langets_mesh(haft: &Haft, t_start: f32, count: u32, length: f32, width: f32, phase: f32) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    if count == 0 || length <= 0.0 {
        return out;
    }
    let thick = (width * 0.18).clamp(0.008, 0.02);
    let t_end = (t_start - length / haft.length()).max(0.0);
    let rows = ((length * 24.0).round() as usize).max(8);
    // A strap with chamfered corners: x across the haft's surface normal, y round it.
    let (h, w) = (thick * 0.5, width * 0.5);
    let section = [[-h, -w * 0.85], [-h * 0.2, -w], [h * 0.6, -w * 0.97], [h, -w * 0.7], [h, w * 0.7], [h * 0.6, w * 0.97], [-h * 0.2, w], [-h, w * 0.85]];
    let rivet = uv_sphere(width * 0.2, 10, 6);
    let rivets = ((length / 0.45).round() as usize).clamp(1, 5);
    for k in 0..count {
        let a = phase + TAU * k as f32 / count as f32;
        let frames: Vec<Frame> = (0..=rows)
            .map(|i| {
                let t = t_start + (t_end - t_start) * i as f32 / rows as f32;
                let f = haft.frame(t);
                let out_dir = vec3::add(vec3::scale(f.normal, a.cos()), vec3::scale(f.binormal, a.sin()));
                let tangent = vec3::neg(f.tangent);
                let origin = vec3::add(f.origin, vec3::scale(out_dir, haft.radius(t) + h * 0.8));
                Frame { origin, tangent, normal: out_dir, binormal: vec3::cross(tangent, out_dir) }
            })
            .collect();
        // Full width down most of its length, then a rounded point.
        out.append(&sweep_frames(&frames, &section, &SweepOptions::default(), |u| {
            if u < 0.8 { 1.0 } else { (1.0 - ((u - 0.8) / 0.2).powi(2)).max(0.0).sqrt().max(0.25) }
        }));
        for j in 0..rivets {
            let u = (j as f32 + 0.6) / (rivets as f32 + 0.2);
            let f = frames[((u * rows as f32).round() as usize).min(rows)];
            let mut head = rivet.clone();
            head.scale([1.0, 0.55, 1.0]).place(vec3::add(f.origin, vec3::scale(f.normal, h * 0.7)), f.normal);
            out.append(&head);
        }
    }
    out
}
