//! Geometry for the head end of a mace or staff: the collar, the head body, flanges,
//! spikes, prongs and the finial, from the [`crate::sections::head`] sections.
//!
//! Everything here is built in head space: +Y continues the shaft, y = 0 is the top of the
//! shaft, and the shaft's top radius is the neck. A family sets the finished meshes onto
//! the haft with [`MeshBuilder::place`].
//!
//! From the prototype's `build()` in `reference/spline_armory_engine.js` ("Head assembly",
//! "Flanges", "Spikes", "Finial", "Prongs") and its `distribute()`, rebuilt at higher
//! fidelity: profiles are centripetal splines, flanges are wedges (thick at the root,
//! thinner toward the edge) with true holes when pierced, and spikes seat on the head's
//! actual surface, faceted or round, along its normal.

use procgen_rng::Mulberry32;

use super::hafted::{radius_on, turned};
use super::parts::PartList;
use crate::geom::polygon::P2;
use crate::geom::{
    CatmullRom, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, distance_to_polygon, extrude_tapered, lathe,
    point_in_polygon, spline_profile, tube, uv_sphere, vec3,
};
use crate::sections::head::{Collar, Finial, FinialStyle, FlangeShape, Flanges, Head, HeadStyle, Hemisphere, Prongs, SpikePattern, Spikes};

/// The unit direction at `angle` round +Y, turning from +X toward -Z as
/// [`crate::geom::lathe`] and [`MeshBuilder::rotate_y`] do.
pub fn radial(angle: f32) -> V3 {
    [angle.cos(), 0.0, -angle.sin()]
}

/// The outer surface of a turned head, for seating flanges, spikes and prongs on it.
#[derive(Clone, Debug, PartialEq)]
pub struct HeadSurface {
    /// `[radius, y]` in head space, bottom to top (a cup's profile turns back down inside).
    pub profile: Vec<[f32; 2]>,
    pub base: f32,
    pub top: f32,
    /// The shaft's radius under the head.
    pub neck: f32,
    /// Sides of a faceted head, `None` for a round one.
    pub sides: Option<usize>,
}

impl HeadSurface {
    /// A head with no body (a staff's bare top): the surface is the shaft end.
    pub fn bare(y: f32, neck: f32) -> Self {
        HeadSurface { profile: Vec::new(), base: y, top: y, neck, sides: None }
    }

    pub fn length(&self) -> f32 {
        self.top - self.base
    }

    /// Radius at the vertices of the outer surface at height `y`; the neck outside the head.
    pub fn radius_at(&self, y: f32) -> f32 {
        let r = radius_on(&self.profile, y);
        if r > 0.0 { r } else { self.neck }
    }

    /// The widest the head gets.
    pub fn max_radius(&self) -> f32 {
        self.profile.iter().map(|p| p[0]).fold(self.neck, f32::max)
    }

    /// The point on the surface at `angle` and height `y`, and the outward normal there.
    /// On a faceted head the point lies on the flat face and the normal is the face's.
    pub fn surface(&self, angle: f32, y: f32) -> (V3, V3) {
        let r = self.radius_at(y);
        let e = (self.length() * 0.01).max(1e-3);
        let slope = (self.radius_at(y + e) - self.radius_at(y - e)) / (2.0 * e);
        let (r, facing, slope) = match self.sides {
            Some(n) => {
                let step = std::f32::consts::TAU / n as f32;
                let off = angle.rem_euclid(step) - step * 0.5;
                let apothem = (step * 0.5).cos();
                (r * apothem / off.cos(), angle - off, slope * apothem)
            }
            None => (r, angle, slope),
        };
        let point = vec3::add(vec3::scale(radial(angle), r), [0.0, y, 0.0]);
        let normal = vec3::normalize_or(vec3::add(radial(facing), [0.0, -slope, 0.0]), radial(facing));
        (point, normal)
    }
}

/// Raised bands where the head meets the shaft, starting at `start_y` over a neck of
/// radius `neck`. The lowest band flares down into a socket over the shaft. Returns the
/// mesh and the height the head starts at.
pub fn collar_mesh(collar: &Collar, neck: f32, start_y: f32) -> (MeshBuilder, f32) {
    let mut out = MeshBuilder::new();
    let (sides, shading) = turned(collar.facets);
    // A faceted collar's flats must still clear the round shaft inside it.
    let neck = neck / (std::f32::consts::PI / sides as f32).cos();
    let rc = neck * collar.radius;
    let h = collar.height;
    let mut y = start_y;
    for band in 0..collar.bands.max(1) {
        let mut control = vec![[neck, 0.0], [rc * 0.96, h * 0.18], [rc, h * 0.5], [rc * 0.96, h * 0.82], [neck, h]];
        if band == 0 {
            let flare = h * 0.7;
            control.splice(0..1, [[neck * 1.01, -flare], [neck * 1.08, -flare * 0.55], [rc * 0.9, h * 0.1]]);
        }
        let profile: Vec<[f32; 2]> = spline_profile(&control, 12).into_iter().map(|[r, py]| [r, py + y]).collect();
        out.append(&lathe(&profile, &LatheOptions { facets: sides, shading, ..Default::default() }));
        y += h * 0.92;
    }
    (out, y)
}

/// The turned head body from `base_y` up, over a neck of radius `neck`. A head with style
/// [`HeadStyle::None`] has no body: the mesh is empty and the surface is bare.
pub fn head_mesh(head: &Head, base_y: f32, neck: f32) -> (MeshBuilder, HeadSurface) {
    let (lh, rh, b) = (head.length, head.radius, head.belly);
    let control: Vec<[f32; 2]> = match head.style {
        HeadStyle::None => return (MeshBuilder::new(), HeadSurface::bare(base_y, neck)),
        HeadStyle::Orb => vec![[neck, 0.0], [rh * 0.62, lh * b * 0.3], [rh, lh * b], [rh * 0.72, lh * (b + (1.0 - b) * 0.6)], [rh * 0.26, lh * (b + (1.0 - b) * 0.93)], [0.0, lh]],
        HeadStyle::Pear => vec![[neck, 0.0], [rh * 0.35, lh * b * 0.45], [rh * 0.95, lh * b], [rh, lh * (b + (1.0 - b) * 0.3)], [rh * 0.55, lh * (b + (1.0 - b) * 0.8)], [0.0, lh]],
        HeadStyle::Drum => vec![[neck, 0.0], [rh * 0.98, lh * 0.1], [rh * (0.88 + 0.24 * (b - 0.5)), lh * b], [rh, lh * 0.9], [rh * 0.35, lh * 0.995], [0.0, lh]],
        HeadStyle::Onion => vec![[neck, 0.0], [rh * 0.8, lh * b * 0.4], [rh, lh * b], [rh * 0.5, lh * (b + (1.0 - b) * 0.55)], [rh * 0.12, lh * (b + (1.0 - b) * 0.85)], [0.0, lh]],
        HeadStyle::Cup => vec![
            [neck, 0.0],
            [rh * 0.35, lh * 0.3],
            [rh * 0.75, lh * b + lh * 0.1],
            [rh, lh * 0.92],
            [rh * 1.02, lh],
            [rh * 0.86, lh * 0.97],
            [rh * 0.5, lh * 0.86],
            [0.0, lh * 0.8],
        ],
    };
    let profile: Vec<[f32; 2]> = spline_profile(&control, 48).into_iter().map(|[r, y]| [r, y + base_y]).collect();
    let (sides, shading) = turned(head.facets);
    let mesh = lathe(&profile, &LatheOptions { facets: sides, shading, ..Default::default() });
    let faceted = (head.facets <= 12).then_some(sides);
    (mesh, HeadSurface { profile, base: base_y, top: base_y + lh, neck, sides: faceted })
}

/// A flange's outer edge as `(height, depth)` fractions, from the prototype's `prof` table.
fn flange_profile(shape: FlangeShape) -> &'static [[f32; 2]] {
    match shape {
        FlangeShape::Blade => &[[0.0, 0.0], [0.1, 0.55], [0.4, 1.0], [0.75, 0.85], [0.97, 0.25], [1.0, 0.0]],
        FlangeShape::Crescent => &[[0.0, 0.0], [-0.12, 1.0], [0.5, 0.45], [1.12, 1.0], [1.0, 0.0]],
        FlangeShape::Stepped => &[[0.0, 0.0], [0.0, 0.5], [0.2, 0.5], [0.35, 1.0], [0.65, 1.0], [0.8, 0.5], [1.0, 0.5], [1.0, 0.0]],
        FlangeShape::Leaf => &[[0.0, 0.0], [0.22, 0.5], [0.58, 1.0], [0.9, 0.42], [1.0, 0.0]],
        FlangeShape::Fan => &[[0.0, 0.0], [0.12, 0.9], [0.5, 1.0], [0.88, 0.9], [1.0, 0.0]],
    }
}

/// Drops consecutive points closer than `eps` (and a last point equal to the first).
fn dedupe(points: Vec<P2>, eps: f32) -> Vec<P2> {
    let mut out: Vec<P2> = Vec::with_capacity(points.len());
    for p in points {
        if out.last().is_none_or(|q| (p[0] - q[0]).hypot(p[1] - q[1]) > eps) {
            out.push(p);
        }
    }
    while out.len() > 3 && out.first().zip(out.last()).is_some_and(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]) <= eps) {
        out.pop();
    }
    out
}

/// Radial flanges round the head, their inner edge buried in it and following its profile.
pub fn flanges_mesh(flanges: &Flanges, surface: &HeadSurface) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    let lh = surface.length();
    if lh <= 0.0 || flanges.count == 0 {
        return out;
    }
    let hz = flanges.height * lh;
    let ya = surface.base + (lh - hz) * 0.5 + flanges.bias * lh;
    let r_min = (0..=20).map(|i| surface.radius_at(ya + hz * i as f32 / 20.0)).fold(f32::MAX, f32::min);
    let r_in = (r_min * 0.65).max(surface.neck * 0.6);
    let base = |y: f32| (surface.radius_at(ya + y) - r_in).max(0.0);
    let depth = flanges.depth;

    let mut shape: Vec<P2> = flange_profile(flanges.shape).to_vec();
    if flanges.notch > 0.0 {
        if flanges.shape == FlangeShape::Stepped {
            for p in &mut shape {
                if p[0] > 0.3 && p[0] < 0.7 {
                    p[1] *= 1.0 - flanges.notch * 0.8;
                }
            }
        } else {
            shape.retain(|p| (p[0] - 0.5).abs() > 0.14);
            shape.push([0.5, (1.0 - flanges.notch) * 0.85]);
            shape.sort_by(|a, b| a[0].total_cmp(&b[0]));
        }
    }
    let raw: Vec<V3> = shape.iter().map(|&[y, f]| [base(y * hz) + depth * f, y * hz, 0.0]).collect();
    let edge: Vec<P2> = if flanges.shape == FlangeShape::Stepped {
        let mut pts = Vec::new();
        for w in raw.windows(2) {
            for q in 0..8 {
                let p = vec3::lerp(w[0], w[1], q as f32 / 8.0);
                pts.push([p[0], p[1]]);
            }
        }
        let last = raw[raw.len() - 1];
        pts.push([last[0], last[1]]);
        pts
    } else {
        CatmullRom::centripetal(raw.clone()).sample(16).into_iter().map(|p| [p[0], p[1]]).collect()
    };
    // The outer edge sits on the head surface; below the surface it carries on in to the root.
    let edge: Vec<P2> = edge.into_iter().map(|[x, y]| [x.max(base(y)), y]).collect();
    let (y0, y1) = (edge[0][1], edge[edge.len() - 1][1]);
    let mut outline = vec![[0.0, y0]];
    outline.extend(edge);
    outline.push([0.0, y1]);
    let outline = dedupe(outline, 1e-4);

    let mut holes = Vec::new();
    if flanges.pierce {
        let yc = hz * 0.5;
        let center = [base(yc) + depth * 0.42, yc];
        let room = if point_in_polygon(center, &outline) { distance_to_polygon(center, &outline) } else { 0.0 };
        let hr = (depth * 0.24).min(hz * 0.1).min(room / 1.4);
        // Only a window that clears the head and leaves metal round it.
        if hr > depth * 0.05 && center[0] - hr > base(yc) + hr * 0.3 {
            holes.push((0..20).rev().map(|k| {
                let a = std::f32::consts::TAU * k as f32 / 20.0;
                [center[0] + hr * a.cos(), center[1] + hr * a.sin()]
            }).collect::<Vec<P2>>());
        }
    }

    let th = flanges.thickness;
    let mut blade = extrude_tapered(&outline, &holes, th, th * 0.45);
    blade.translate([0.0, -hz * 0.5, 0.0]).rotate_x(flanges.lean.to_radians()).translate([r_in, ya + hz * 0.5, 0.0]);
    for i in 0..flanges.count {
        let mut f = blade.clone();
        f.rotate_y(flanges.phase.to_radians() + std::f32::consts::TAU * i as f32 / flanges.count as f32);
        out.append(&f);
    }
    out
}

/// Where one ornament goes in a radial pattern: `angle` round the axis (radians), `offset`
/// from the centre plane (-1..=1, jittered), and `rank`, the unjittered offset that sizes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadialSlot {
    pub angle: f32,
    pub offset: f32,
    pub rank: f32,
}

/// Places `count` ornaments per ring over `rings` rings about a centre plane (the
/// prototype's `distribute`): aligned, staggered, spiral or golden-angle, on both halves
/// or one. Spikes use it; studs, rivets and bosses can too.
#[allow(clippy::too_many_arguments)]
pub fn distribute(
    count: u32,
    rings: u32,
    pattern: SpikePattern,
    phase_deg: f32,
    twist_deg: f32,
    hemisphere: Hemisphere,
    jitter: f32,
    rng: &mut Mulberry32,
) -> Vec<RadialSlot> {
    use std::f32::consts::{PI, TAU};
    let c = count.max(1);
    let rn = rings.max(1);
    let ph = phase_deg.to_radians();
    let tw = twist_deg.to_radians();
    let step = TAU / c as f32;
    let mut slots: Vec<(f32, f32)> = Vec::new();
    match pattern {
        SpikePattern::Spiral | SpikePattern::GoldenAngle => {
            let n = c * rn;
            for k in 0..n {
                let kf = k as f32;
                let golden = pattern == SpikePattern::GoldenAngle;
                let off = if golden { 1.0 - 2.0 * (kf + 0.5) / n as f32 } else if n == 1 { 0.0 } else { 1.0 - 2.0 * kf / (n - 1) as f32 };
                let angle = if golden { ph + kf * 2.399_963 } else { ph + kf * step + kf * step / rn as f32 + off * tw };
                slots.push((angle, off));
            }
        }
        SpikePattern::Aligned | SpikePattern::Staggered => {
            for i in 0..rn {
                let off = if rn == 1 { 0.0 } else { 1.0 - 2.0 * i as f32 / (rn - 1) as f32 };
                let stagger = if pattern == SpikePattern::Staggered && i % 2 == 1 { PI / c as f32 } else { 0.0 };
                for j in 0..c {
                    slots.push((ph + j as f32 * step + i as f32 * tw + stagger, off));
                }
            }
        }
    }
    slots
        .into_iter()
        .map(|(angle, off)| {
            let off = match hemisphere {
                Hemisphere::Both => off,
                Hemisphere::Upper => (off + 1.0) * 0.5,
                Hemisphere::Lower => -(off + 1.0) * 0.5,
            };
            let ja = (rng.next_f64() as f32 - 0.5) * jitter * step;
            let jo = (rng.next_f64() as f32 - 0.5) * jitter * 0.25;
            RadialSlot { angle: angle + ja, offset: off + jo, rank: off }
        })
        .collect()
}

/// One spike, base on y = 0, point up (the prototype's spike lathe, smoothed).
pub fn spike_mesh(base: f32, length: f32, sides: u32, concave: f32) -> MeshBuilder {
    let (b, l, cv) = (base, length, concave);
    let control = [[b, 0.0], [b * 0.95, l * 0.1], [b * (0.55 - 0.25 * cv), l * (0.45 + 0.1 * cv)], [b * (0.2 - 0.1 * cv), l * 0.85], [0.0, l]];
    let (sides, shading) = if sides <= 10 { (sides.max(3) as usize, Shading::FACETED) } else { (sides.max(16) as usize, Shading::SMOOTH) };
    lathe(&spline_profile(&control, 12), &LatheOptions { facets: sides, shading, cap_start: true, cap_end: false, phase: 0.0 })
}

/// Spikes in rings over the head, each seated a little into the surface along its normal
/// and shrinking away from the centre plane by `falloff`. `seed` drives the jitter.
pub fn spikes_mesh(spikes: &Spikes, surface: &HeadSurface, seed: u32) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    let lh = surface.length();
    if lh <= 0.0 {
        return out;
    }
    let mut rng = Mulberry32::new(seed.wrapping_mul(31).wrapping_add(7));
    let slots = distribute(spikes.count, spikes.rings, spikes.pattern, spikes.phase, spikes.twist, spikes.hemisphere, spikes.jitter, &mut rng);
    let one = spike_mesh(spikes.base, spikes.length, spikes.sides, spikes.concave);
    let tilt = spikes.tilt.to_radians();
    for slot in slots {
        let v = (0.5 + spikes.bias + slot.offset * spikes.spread * 0.5).clamp(0.03, 0.97);
        let (point, normal) = surface.surface(slot.angle, surface.base + v * lh);
        let dir = vec3::normalize_or(vec3::add(vec3::scale(normal, tilt.cos()), vec3::scale(vec3::Y, tilt.sin())), normal);
        let scale = (1.0 - spikes.falloff * slot.rank.abs()).max(0.15);
        let mut spike = one.clone();
        spike.scale_uniform(scale).place(vec3::sub(point, vec3::scale(dir, spikes.base * 0.35 * scale)), dir);
        out.append(&spike);
    }
    out
}

/// The ornament on top of the head, its foot sunk `sink` below `top`. Empty for
/// [`FinialStyle::None`].
pub fn finial_mesh(finial: &Finial, top: f32, sink: f32) -> MeshBuilder {
    let (fr, fl) = (finial.radius, finial.length);
    let control: &[[f32; 2]] = match finial.style {
        FinialStyle::None => return MeshBuilder::new(),
        FinialStyle::Spike => &[[fr, 0.0], [fr * 0.9, fl * 0.2], [fr * 0.35, fl * 0.7], [0.0, fl]],
        FinialStyle::Knob => &[[fr * 0.6, 0.0], [fr, fl * 0.3], [fr * 1.1, fl * 0.6], [fr * 0.6, fl * 0.9], [0.0, fl]],
        FinialStyle::Spire => &[[fr, 0.0], [fr * 1.35, fl * 0.1], [fr * 0.6, fl * 0.25], [fr * 0.4, fl * 0.6], [0.0, fl]],
    };
    let (sides, shading) = turned(finial.sides);
    let mut mesh = lathe(&spline_profile(control, 20), &LatheOptions { facets: sides, shading, ..Default::default() });
    mesh.translate([0.0, top - sink, 0.0]);
    mesh
}

/// The space the prongs close round: their shared profile as `[radius, y]` from root to tip.
#[derive(Clone, Debug, PartialEq)]
pub struct Cradle {
    pub profile: Vec<[f32; 2]>,
    /// Prong thickness at the root.
    pub thickness: f32,
}

impl Cradle {
    /// Height of the tip.
    pub fn tip(&self) -> f32 {
        self.profile.last().map_or(0.0, |p| p[1])
    }

    /// The girdle height for a stone of girdle radius `radius`: where the prongs, closing in
    /// above their widest point, come to touch it. `None` if they never close that far.
    pub fn seat(&self, radius: f32) -> Option<f32> {
        let widest = self.profile.iter().enumerate().fold(0, |best, (i, p)| if p[0] > self.profile[best][0] { i } else { best });
        self.profile[widest..].iter().find(|p| p[0] - self.thickness * 0.5 <= radius).map(|p| p[1])
    }
}

/// Curling arms rising from the head, tapering to a knob. `cup` roots them on a cup head's
/// rim. Returns the mesh and the [`Cradle`] they form.
pub fn prongs_mesh(prongs: &Prongs, surface: &HeadSurface, cup: bool) -> (MeshBuilder, Cradle) {
    let mut out = MeshBuilder::new();
    let lh = surface.length();
    let y0 = if cup { surface.top - lh * 0.06 } else if lh > 0.0 { surface.base + lh * 0.8 } else { surface.top };
    let r0 = (if lh > 0.0 { surface.radius_at(y0) } else { 0.0 }).max(surface.neck * 0.8) * prongs.base;
    let (h, s, cu) = (prongs.height, prongs.spread, prongs.curl);
    let tw = prongs.twist * std::f32::consts::TAU;
    let tip_r = ((r0 + s) * (1.0 - cu * 1.05)).max(0.02);
    let control = [
        [r0 * 0.9, 0.0, 0.0],
        [r0 + s * 0.5, h * 0.22, 0.25],
        [r0 + s, h * 0.55, 0.5],
        [r0 + s * (1.0 - cu * 0.6), h * 0.85, 0.78],
        [tip_r, h * (1.0 - 0.12 * cu), 1.0],
    ];
    let profile_curve = CatmullRom::centripetal(control.iter().map(|&[r, y, _]| [r, y0 + y, 0.0]).collect()).sample(12);
    let cradle = Cradle { profile: profile_curve.iter().map(|p| [p[0], p[1]]).collect(), thickness: prongs.thickness };
    let knob = uv_sphere(prongs.thickness * 0.55, 12, 8);
    for i in 0..prongs.count {
        let a0 = prongs.phase.to_radians() + std::f32::consts::TAU * i as f32 / prongs.count as f32;
        let points: Vec<V3> = control
            .iter()
            .map(|&[r, y, q]| vec3::add(vec3::scale(radial(a0 + tw * q), r), [0.0, y0 + y, 0.0]))
            .collect();
        let path = CatmullRom::centripetal(points).sample(12);
        out.append(&tube(&path, 10, &SweepOptions::default(), |t| prongs.thickness * (1.0 - 0.6 * t)));
        let mut k = knob.clone();
        k.translate(path[path.len() - 1]);
        out.append(&k);
    }
    (out, cradle)
}

/// The sections a turned head assembly is built from (maces, staves).
#[derive(Clone, Copy, Debug)]
pub struct HeadSections<'a> {
    pub collar: &'a Collar,
    pub head: &'a Head,
    pub flanges: &'a Flanges,
    pub spikes: &'a Spikes,
    pub prongs: &'a Prongs,
    pub finial: &'a Finial,
}

/// A head assembly built upright in head space (y = 0 is the top of the shaft), with what a
/// family needs to set a stone in it.
#[derive(Clone, Debug)]
pub struct HeadAssembly {
    /// `collar`, `head`, `flanges`, `spikes`, `finial` and `prongs`, as enabled.
    pub parts: PartList,
    pub surface: HeadSurface,
    /// The prongs' cradle, when there are prongs.
    pub cradle: Option<Cradle>,
    /// The top of the head, or of the finial when there is one.
    pub crown: f32,
}

/// Builds the collar, head body, flanges, spikes, finial and prongs over a shaft whose top
/// radius is `neck`, in head space. The family seats any stone (in
/// [`HeadAssembly::cradle`] or on [`HeadAssembly::crown`]) and then places
/// [`HeadAssembly::parts`] on the haft. `seed` drives spike jitter.
pub fn head_assembly(s: HeadSections<'_>, neck: f32, seed: u32) -> HeadAssembly {
    let mut parts = PartList::new();
    let mut y = -0.03;
    if s.collar.on {
        let (collar, top) = collar_mesh(s.collar, neck, y);
        parts.add("collar", collar, s.collar.material);
        y = top;
    }
    let neck_head = if s.collar.on { neck * s.collar.radius.min(1.3) } else { neck };
    let (body, surface) = head_mesh(s.head, y, neck_head);
    let surface = HeadSurface { neck, ..surface };
    parts.add("head", body, s.head.material);
    if s.flanges.on {
        parts.add("flanges", flanges_mesh(s.flanges, &surface), s.flanges.material);
    }
    if s.spikes.on {
        parts.add("spikes", spikes_mesh(s.spikes, &surface, seed), s.spikes.material);
    }
    let sink = 0.04f32.min(surface.length() * 0.05);
    parts.add("finial", finial_mesh(s.finial, surface.top, sink), s.finial.material);
    let crown = if s.finial.style == FinialStyle::None { surface.top } else { surface.top + s.finial.length - sink };
    let cradle = s.prongs.on.then(|| {
        let (mesh, cradle) = prongs_mesh(s.prongs, &surface, s.head.style == HeadStyle::Cup);
        parts.add("prongs", mesh, s.prongs.material);
        cradle
    });
    HeadAssembly { parts, surface, cradle, crown }
}
