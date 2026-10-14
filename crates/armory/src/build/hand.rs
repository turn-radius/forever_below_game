//! Gloves, gauntlets, mittens and bracers on a [`HandForm`]: a right hand, back toward +Z,
//! fingers up, wrist at y = 0, the cuff running down the forearm. Built in the form's
//! space so `fb_body` can pass a folk hand.
//!
//! Departures from the prototype's `buildHands`: fingers and thumb are single smooth tubes
//! with domed tips (not tubes plus spheres); finger plates are partial shells over the back
//! of each phalanx with rolled edges (not full cylinders), and the tip plate is domed; every
//! sheet has thickness.

use std::f32::consts::TAU;

use crate::build::form::{BACK, Digit, FRONT, HandForm, LameStack, ShellOptions, limb_lames, patch, rolled_edge};
use crate::build::garment::buckle_mesh;
use crate::build::parts::{Look, Parts};
use crate::fabric::{PatternKind, SurfacePattern};
use crate::geom::vec3::{self, V3};
use crate::geom::{CatmullRom, GridOptions, MeshBuilder, Shading, SweepOptions, spike, stitch_grid, tube};
use crate::material::{MaterialKind, Rgb};
use crate::sections::hand::{Hand, HandStyle};
use crate::sections::hem::Hem;

/// A digit as one smooth tube from inside the palm to a domed tip, `inflate` thicker.
pub fn digit_mesh(digit: &Digit, curl: f32, inflate: f32) -> MeshBuilder {
    let joints = digit.joints(curl);
    let n = joints.len();
    let first = vec3::normalize_or(vec3::sub(joints[1], joints[0]), vec3::Y);
    let last = vec3::normalize_or(vec3::sub(joints[n - 1], joints[n - 2]), vec3::Y);
    let r = digit.radius + inflate;
    let tip_r = r * 0.82;
    let mut control = vec![vec3::sub(joints[0], vec3::scale(first, r * 1.2))];
    control.extend_from_slice(&joints);
    control.push(vec3::add(joints[n - 1], vec3::scale(last, tip_r)));
    let path = CatmullRom::centripetal(control).sample_evenly(12 * n + 8);
    let length: f32 = path.windows(2).map(|w| vec3::distance(w[0], w[1])).sum();
    let dome = (tip_r / length.max(1e-3)).min(0.5);
    let knuckles = (n - 2) as f32;
    tube(&path, 14, &SweepOptions { cap_end: false, ..SweepOptions::default() }, |t| {
        let taper = r * (1.0 - 0.18 * t) * (1.0 + 0.06 * (t * (knuckles + 1.0) * TAU).cos().max(0.0));
        let end = ((t - (1.0 - dome)) / dome).clamp(0.0, 1.0);
        taper * (1.0 - end * end).max(0.0).sqrt()
    })
}

/// The palm (and a mitten's bag when `cap` is long), from just inside the cuff to its
/// rounded end, as a closed surface.
pub fn palm_mesh(form: &HandForm, cap: f32, inflate: f32) -> MeshBuilder {
    let end = form.palm_length + cap;
    let rows = 34;
    let points: Vec<Vec<V3>> = (0..=rows)
        .map(|r| {
            let y = -0.05 + (end + 0.05) * r as f32 / rows as f32;
            (0..56).map(|c| form.palm_point(BACK + TAU * c as f32 / 56.0, y, cap, inflate)).collect()
        })
        .collect();
    stitch_grid(&points, &GridOptions { wrap_around: true, ..Default::default() })
}

/// The cuff's radius multiplier at `f` (0 at the wrist, 1 at the open end).
fn cuff_flare(hand: &Hand, f: f32) -> f32 {
    let flare = hand.flare * if hand.style == HandStyle::Hourglass { 1.4 } else { 1.0 };
    let k = 1.0 + flare * f.powf(1.6) * 0.9;
    if hand.style == HandStyle::Hourglass {
        // A bulge over the wrist bones, then the waist, then the bell.
        k * (1.0 + 0.25 * (1.0 - (f * 4.0).min(1.0)).powi(2))
    } else {
        k
    }
}

/// A point on the cuff a fraction `f` from the wrist to the open end, `inflate` out.
fn cuff_point(form: &HandForm, hand: &Hand, angle: f32, f: f32, inflate: f32) -> V3 {
    let length = hand.cuff * hand.size;
    let down = -0.08 * hand.size + (length + 0.08 * hand.size) * f;
    let p = form.forearm_point(angle, down, cuff_flare(hand, f));
    let r = p[0].hypot(p[2]).max(1e-4);
    [p[0] * (1.0 + inflate / r), p[1], p[2] * (1.0 + inflate / r)]
}

/// The cuff from `f0` to `f1` of its length, `inflate` out, as a thickened tube.
fn cuff_band(form: &HandForm, hand: &Hand, f0: f32, f1: f32, rows: usize, inflate: f32, thickness: f32) -> MeshBuilder {
    // Rows run from the open end up toward the wrist so faces point outward.
    patch(56, rows, &ShellOptions::tube(thickness), |u, v| cuff_point(form, hand, BACK + TAU * u, f1 + (f0 - f1) * v, inflate))
}

/// A strap round the cuff at `f`, with its buckle on the back of the forearm.
fn strap(form: &HandForm, hand: &Hand, f: f32, width: f32) -> (MeshBuilder, MeshBuilder) {
    let length = hand.cuff * hand.size;
    let df = width / length.max(0.1) * 0.5;
    let band = cuff_band(form, hand, f - df, f + df, 2, 0.03 * hand.size, 0.03);
    let at = cuff_point(form, hand, FRONT, f, 0.07 * hand.size);
    (band, buckle_mesh(width, at))
}

/// Every part of a glove on `form`: `"hand"`, `"fingers"`, `"thumb"`, `"cuff"`, `"plates"`,
/// `"gadlings"`, `"edges"`, `"straps"`, `"buckles"` and `"trim"`, as the style calls for them.
pub fn hand_parts(form: &HandForm, hand: &Hand, hem: &Hem) -> Parts {
    let mut parts = Parts::new();
    let s = hand.size;
    let style = hand.style;
    let glove = Look::of(hand.material);
    let plate = Look::of(hand.plate);
    let mail = Look::patterned(hand.plate, SurfacePattern { kind: PatternKind::Mail, color: Rgb::hex(0x232323), scale: 9.0 / s });
    let trim = (hem.on && hem.hemline).then(|| Look::cloth(&hem.fabric));
    let cuff_look = match style {
        HandStyle::Gauntlet | HandStyle::Hourglass => plate,
        HandStyle::Mitten => mail,
        HandStyle::Glove | HandStyle::Bracer => glove,
    };
    let cuff_thickness = if style.is_plated() { 0.04 } else { 0.03 } * s;

    // Cuff.
    parts.add_look("cuff", cuff_band(form, hand, 0.0, 1.0, 24, 0.0, cuff_thickness), cuff_look);
    if style != HandStyle::Glove {
        let rim: Vec<V3> = (0..64).map(|i| cuff_point(form, hand, BACK + TAU * i as f32 / 64.0, 1.0, 0.0)).collect();
        parts.add_look("edges", rolled_edge(&rim, (0.03 * s).max(0.025), true), cuff_look);
    }
    if let Some(trim) = trim {
        let length = hand.cuff * s;
        let f0 = (1.0 - hem.width * 0.6 / length.max(0.1)).max(0.0);
        parts.add_look("trim", cuff_band(form, hand, f0, 1.0, 3, 0.01 * s, cuff_thickness + 0.02), trim);
    }
    if style == HandStyle::Bracer {
        for f in [0.2, 0.5, 0.8] {
            let (band, buckle) = strap(form, hand, f, 0.14 * s);
            parts.add_look("straps", band, Look::of(MaterialKind::Leather));
            parts.add_look("buckles", buckle, Look::of(MaterialKind::Iron));
        }
        return parts;
    }

    // The hand inside.
    let mitten = style.is_mitten();
    let cap = if mitten { 0.8 * s } else { 0.12 * s };
    let curl = hand.curl * 0.55;
    let hand_look = if style == HandStyle::Mitten { mail } else { glove };
    parts.add_look("hand", palm_mesh(form, cap, 0.0), hand_look);
    if !mitten {
        for finger in &form.fingers {
            parts.add_look("fingers", digit_mesh(finger, curl, 0.0), glove);
        }
    }
    parts.add_look("thumb", digit_mesh(&form.thumb, curl, 0.0), hand_look);

    if !style.is_plated() {
        return parts;
    }

    // Plates over each finger and the thumb: a lame per phalanx, the tip one domed. Each lame
    // sits close over the back of its phalanx and laps a little past the joints, so the plates
    // read as one articulated row per finger: wider or looser lames run into the next finger's
    // and leave the glove showing at every knuckle.
    let digit_plates = |digit: &Digit, count: usize| -> MeshBuilder {
        let joints = digit.joints(curl);
        let mut mesh = MeshBuilder::new();
        let thickness = 0.02 * s;
        for i in 0..count.min(joints.len() - 1) {
            let (a, b) = (joints[i], joints[i + 1]);
            let len = vec3::distance(a, b);
            let axis = vec3::sub(b, a);
            let tip = i + 2 == joints.len();
            let stack = LameStack {
                origin: if tip { vec3::add(b, vec3::scale(vec3::normalize_or(axis, vec3::Y), digit.radius * 0.5)) } else { vec3::sub(a, vec3::scale(axis, 0.06)) },
                axis: if tip { vec3::neg(axis) } else { axis },
                toward: vec3::Z,
                count: 1,
                radius: digit.radius * 1.06 + thickness,
                step: 0.0,
                height: len * if tip { 1.0 } else { 1.02 },
                overlap: 0.0,
                half_angle: 1.15,
                thickness,
                cap: tip,
            };
            mesh.append(&limb_lames(&stack));
        }
        mesh
    };
    if style == HandStyle::Gauntlet {
        for finger in &form.fingers {
            parts.add_look("plates", digit_plates(finger, 3), plate);
        }
    }
    parts.add_look("plates", digit_plates(&form.thumb, 2), plate);

    // Lames over the back of the hand, each lapping over the one below.
    let hb = form.palm_length;
    let sheet = ShellOptions::sheet(0.025 * s);
    let back_plate = |y0: f32, y1: f32, inflate: f32, half: f32, cap: f32| -> MeshBuilder {
        let mut mesh = patch(30, 4, &sheet, |u, v| form.palm_point(FRONT - half + 2.0 * half * u, y0 + (y1 - y0) * v, cap, inflate));
        let edge: Vec<V3> = (0..=24).map(|i| form.palm_point(FRONT - half + 2.0 * half * i as f32 / 24.0, y1, cap, inflate + 0.012 * s)).collect();
        mesh.append(&rolled_edge(&edge, 0.018 * s, false));
        mesh
    };
    let lames = hand.lames.max(1);
    let top = hb * 0.8;
    let lh = top / lames as f32;
    for i in 0..lames {
        let y0 = i as f32 * lh - 0.02 * s;
        parts.add_look("plates", back_plate(y0, y0 + lh * 1.3, (0.04 + 0.014 * i as f32) * s, 1.9, cap), plate);
    }
    parts.add_look("plates", back_plate(top, hb * 1.02, 0.09 * s, 1.95, cap), plate);
    if style == HandStyle::Hourglass {
        // Lames over the mitten's fingers, each lapping toward the knuckles.
        let n = 3;
        let mlen = cap;
        for i in 0..n {
            let y0 = hb + mlen * i as f32 / n as f32 - 0.02 * s;
            let y1 = (hb + mlen * (i + 1) as f32 / n as f32 + 0.05 * s).min(hb + mlen * 0.97);
            parts.add_look("plates", back_plate(y0, y1, (0.05 + 0.012 * (n - i) as f32) * s, 2.2, cap), plate);
        }
    }
    if hand.gadlings && style == HandStyle::Gauntlet {
        let mut spikes = MeshBuilder::new();
        for finger in &form.fingers {
            let mut sp = spike(0.06 * s, 0.16 * s, 4, 0.3, Shading::FLAT);
            let base = form.palm_point(FRONT, hb * 0.92, cap, 0.09 * s);
            sp.align_y_to(vec3::normalize_or([0.0, 0.15, 1.0], vec3::Z)).translate([finger.base[0], base[1], base[2] - 0.01 * s]);
            spikes.append(&sp);
        }
        parts.add_look("gadlings", spikes, plate);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    #[test]
    fn fingers_are_closed_tubes_that_reach_their_tips() {
        let form = HandForm::mannequin(1.0, 0.3);
        for finger in &form.fingers {
            let mesh = digit_mesh(finger, 0.0, 0.0);
            let tip = *finger.joints(0.0).last().unwrap();
            let reach = mesh.positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
            assert!(reach > tip[1] && reach < tip[1] + finger.radius * 1.5, "reach {reach} tip {}", tip[1]);
            mesh.into_part("finger", OutlineMode::Welded).validate().unwrap();
        }
    }

    #[test]
    fn curl_bends_fingers_toward_the_palm() {
        let form = HandForm::mannequin(1.0, 0.3);
        let tip = |curl: f32| *form.fingers[1].joints(curl).last().unwrap();
        assert!(tip(0.5)[2] < tip(0.0)[2] - 0.2);
    }
}
