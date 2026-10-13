//! Chains, cords, thongs and bead strings along any path: necklaces, a charm's chain stub,
//! and (for later families) bag straps, lantern chains and belts.
//!
//! Every builder takes a path of points in item space and whether it closes on itself, so
//! a caller can lay a chain along a necklace curve, a strap along a shoulder, or a short
//! straight stub. From the prototype's `linkGeo`, `twistedCord` and the chain styles in
//! `buildAmulet` (`reference/spline_armory_engine.js`), rebuilt so that cable links
//! interlock at their true pitch instead of floating apart, and twisted strands close
//! cleanly round a loop.

use std::f32::consts::{PI, TAU};

use crate::geom::polygon::P2;
use crate::geom::{
    CatmullRom, LatheOptions, Mat3, MeshBuilder, Shading, SweepOptions, V3, lathe, polyline_length, resample_by_length,
    rotation_minimizing_frames, spline_profile, sweep, torus, tube, vec3,
};

/// One oval chain link, its long axis along +Y, lying in the XY plane, centred on the
/// origin. `length` is end to end over the wire; `wire` is the wire's radius.
pub fn chain_link(length: f32, wire: f32) -> MeshBuilder {
    let (a, b) = link_axes(length, wire);
    // A slightly squared oval, the shape a link takes when drawn round a mandrel.
    let n = 20;
    let path: Vec<V3> = (0..n)
        .map(|i| {
            let t = TAU * i as f32 / n as f32;
            let (s, c) = t.sin_cos();
            [b * c.signum() * c.abs().powf(0.8), a * s.signum() * s.abs().powf(0.8), 0.0]
        })
        .collect();
    tube(&path, 6, &SweepOptions { up: Some(vec3::Z), ..SweepOptions::closed_loop() }, |_| wire)
}

/// Half-length and half-width of a link's centre line.
fn link_axes(length: f32, wire: f32) -> (f32, f32) {
    ((length * 0.5 - wire).max(wire * 1.4), (length * 0.32 - wire).max(wire * 1.3))
}

/// Centre-to-centre distance of interlocking links: each link's end wire rests inside the
/// next link's end.
pub fn link_pitch(length: f32, wire: f32) -> f32 {
    let (a, _) = link_axes(length, wire);
    (2.0 * a - 2.0 * wire).max(wire * 2.0)
}

/// A cable chain along `path`: links of `length` in wire of radius `wire`, each turned a
/// quarter turn from the last so they interlock. A closed path gets an even count so the
/// last link meets the first. Links stop at `max_links`.
pub fn cable_chain(path: &[V3], closed: bool, length: f32, wire: f32, max_links: usize) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    if path.len() < 2 {
        return out;
    }
    let pitch = link_pitch(length, wire);
    let total = polyline_length(path, closed);
    let mut n = if closed { (total / pitch).round() as usize } else { (total / pitch).floor() as usize + 1 };
    n = n.clamp(2, max_links.max(2));
    if closed && n % 2 == 1 {
        n += 1;
    }
    let centres = resample_by_length(path, n, closed);
    let frames = rotation_minimizing_frames(&centres, None, closed);
    let link = chain_link(length, wire);
    for (i, f) in frames.iter().enumerate() {
        let (x, z) = if i % 2 == 0 { (f.normal, vec3::neg(f.binormal)) } else { (f.binormal, f.normal) };
        let mut m = link.clone();
        m.transform(&Mat3::from_cols(x, f.tangent, z)).translate(f.origin);
        out.append(&m);
    }
    out
}

/// How strands lie in a twisted cord or a twisted ring band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrandLay {
    pub strands: usize,
    /// Distance from the path to each strand's centre.
    pub lay: f32,
    /// Radius of each strand.
    pub wire: f32,
    /// Full turns of the lay along the whole path. Whole numbers close cleanly on a loop.
    pub turns: f32,
    /// Stretch of the lay along the frame binormal (1 for a round cord; a twisted ring
    /// band widens it across the finger).
    pub aspect: f32,
    /// Sides of each strand's tube.
    pub sides: usize,
    /// Where the first frame's normal points (the lay's `cos` axis); `aspect` stretches the
    /// other. `None` picks a stable perpendicular.
    pub up: Option<V3>,
}

/// Strands wound round `path`, each a smooth tube.
pub fn twisted_strands(path: &[V3], closed: bool, lay: &StrandLay) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    if path.len() < 2 || lay.strands == 0 {
        return out;
    }
    let total = polyline_length(path, closed);
    // Enough samples per turn that each strand reads as a smooth helix.
    let count = ((lay.turns * 18.0) as usize).max((total / (lay.wire * 1.5)).ceil() as usize).clamp(24, 4000);
    let pts = resample_by_length(path, count, closed);
    let frames = rotation_minimizing_frames(&pts, lay.up, closed);
    let steps = if closed { count } else { count - 1 };
    let options = if closed { SweepOptions::closed_loop() } else { SweepOptions::default() };
    for k in 0..lay.strands {
        let strand: Vec<V3> = frames
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let ph = lay.turns * TAU * i as f32 / steps as f32 + k as f32 * TAU / lay.strands as f32;
                f.place([ph.cos() * lay.lay, ph.sin() * lay.lay * lay.aspect])
            })
            .collect();
        out.append(&tube(&strand, lay.sides, &options, |_| lay.wire));
    }
    out
}

/// A twisted cord of overall radius `radius` laid from `strands` strands, with a lay of
/// about one turn per seven radii.
pub fn twisted_cord(path: &[V3], closed: bool, radius: f32, strands: usize) -> MeshBuilder {
    let strands = strands.max(2);
    let (lay, wire) = if strands == 2 { (radius * 0.45, radius * 0.58) } else { (radius * 0.5, radius * 0.52) };
    let turns = (polyline_length(path, closed) / (radius * 7.0)).round().max(1.0);
    twisted_strands(path, closed, &StrandLay { strands, lay, wire, turns, aspect: 1.0, sides: 6, up: None })
}

/// A leather thong: a cut strip with softened corners, a little wider than thick.
pub fn thong(path: &[V3], closed: bool, radius: f32) -> MeshBuilder {
    let section: Vec<P2> = (0..12)
        .map(|i| {
            let t = TAU * i as f32 / 12.0 + PI / 12.0;
            let (s, c) = t.sin_cos();
            // A superellipse between a circle and a rectangle.
            [radius * c.signum() * c.abs().powf(0.6), radius * 0.7 * s.signum() * s.abs().powf(0.6)]
        })
        .collect();
    let count = ((polyline_length(path, closed) / (radius * 1.5)).ceil() as usize).clamp(16, 1200);
    let pts = resample_by_length(path, count, closed);
    let options = if closed { SweepOptions::closed_loop() } else { SweepOptions::default() };
    sweep(&pts, &section, &options, |_| 1.0)
}

/// A plain round cord or wire along `path`.
pub fn cord(path: &[V3], closed: bool, radius: f32, sides: usize) -> MeshBuilder {
    let count = ((polyline_length(path, closed) / (radius * 2.0)).ceil() as usize).clamp(16, 1200);
    let pts = resample_by_length(path, count, closed);
    let options = if closed { SweepOptions::closed_loop() } else { SweepOptions::default() };
    tube(&pts, sides, &options, |_| radius)
}

/// Positions and directions every `spacing` along `path`, skipping `margin` (a fraction of
/// the length) at each end of an open path.
pub fn stations(path: &[V3], closed: bool, spacing: f32, margin: f32) -> Vec<(V3, V3)> {
    let total = polyline_length(path, closed);
    let n = ((total / spacing.max(1e-6)).floor() as usize).max(1);
    let pts = resample_by_length(path, if closed { n } else { n + 1 }, closed);
    let frames = rotation_minimizing_frames(&pts, None, closed);
    frames
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            let t = *i as f32 / n as f32;
            closed || (t >= margin && t <= 1.0 - margin)
        })
        .map(|(_, f)| (f.origin, f.tangent))
        .collect()
}

/// A round bead of `radius`, drilled along +Y (flattened where the cord enters).
pub fn bead(radius: f32, facets: usize) -> MeshBuilder {
    let r = radius;
    let profile = spline_profile(&[[r * 0.3, -r * 0.82], [r * 0.8, -r * 0.6], [r, 0.0], [r * 0.8, r * 0.6], [r * 0.3, r * 0.82]], 14);
    lathe(&profile, &LatheOptions { facets, shading: Shading::SMOOTH, ..Default::default() })
}

/// A jump ring (a split ring of wire) of centre-line `radius`, lying in the XY plane, its
/// axis along +Z.
pub fn jump_ring(radius: f32, wire: f32) -> MeshBuilder {
    let mut ring = torus(radius, wire, 28, 8, Shading::SMOOTH);
    ring.rotate_x(PI / 2.0);
    ring
}

/// The closed curve of a necklace hanging from a bail at `bail`: it rises `drop` above it
/// to the back of the neck, a third as wide and a little deeper than wide, running along
/// +X through the bail. About 120 points.
pub fn necklace_path(bail: V3, drop: f32) -> Vec<V3> {
    let (h, w, d) = (drop, drop * 0.34, drop * 0.38);
    let half = [[0.3 * w, 0.1 * h, -0.02 * d], [0.82 * w, 0.42 * h, -0.18 * d], [w, 0.72 * h, -0.5 * d], [0.6 * w, 0.95 * h, -0.88 * d]];
    let mut control: Vec<V3> = vec![[0.0, 0.0, 0.0]];
    control.extend(half.iter().copied());
    control.push([0.0, h, -d]);
    control.extend(half.iter().rev().map(|&[x, y, z]| [-x, y, z]));
    let control = control.into_iter().map(|p| vec3::add(p, bail)).collect();
    CatmullRom::centripetal(control).with_closed(true).sample(12)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_cable_chain_has_an_even_count_of_interlocking_links() {
        let path = necklace_path([0.0, 0.0, 0.0], 1.5);
        let link = chain_link(0.05, 0.005);
        let chain = cable_chain(&path, true, 0.05, 0.005, 1000);
        let links = chain.vertex_count() / link.vertex_count();
        assert_eq!(chain.vertex_count() % link.vertex_count(), 0);
        assert_eq!(links % 2, 0);
        let pitch = polyline_length(&path, true) / links as f32;
        assert!((pitch - link_pitch(0.05, 0.005)).abs() < link_pitch(0.05, 0.005) * 0.1);
    }

    #[test]
    fn necklace_closes_and_rises_by_its_drop() {
        let path = necklace_path([0.0, 0.1, 0.0], 2.0);
        let (lo, hi) = path.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
        assert!((lo - 0.1).abs() < 0.02 && (hi - 2.1).abs() < 0.05, "{lo}..{hi}");
    }

    #[test]
    fn twisted_cord_stays_within_its_radius() {
        let path: Vec<V3> = (0..=20).map(|i| [0.0, i as f32 * 0.1, 0.0]).collect();
        let cord = twisted_cord(&path, false, 0.05, 3);
        assert!(cord.positions.iter().all(|p| p[0].hypot(p[2]) < 0.05 * 1.05));
    }
}
