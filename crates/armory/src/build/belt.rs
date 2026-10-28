//! A belt worn as its own item, on a [`TorsoForm`]: a strap buckled round the hips, its free
//! end through a keeper (or hanging down the front), and what hangs from it: studs or bar
//! mounts, sword frogs, pouch loops and D-rings. Built from [`crate::build::strap`] in the
//! form's space (waist at y = 0, front +Z), so `fb_body` can buckle it round a folk body.
//!
//! The plate prototype draws a belt as a band offset from the body with a torus for the
//! buckle and a squashed ellipsoid for its pouch; here the strap has its section, the buckle
//! its frame and tongue, and the pouch is the Armory's own belt pouch (see
//! [`belt_pouch_mount`]).

use std::f32::consts::{PI, TAU};

use crate::build::form::{FRONT, TorsoForm, TorsoProfile};
use crate::build::parts::{Look, Parts};
use crate::build::strap::{buckle, d_ring, rivets, strap};
use crate::geom::vec3::{self, V3};
use crate::geom::{CatmullRom, Mat3, MeshBuilder, Shading, cuboid, cylinder};
use crate::material::MaterialKind;
use crate::sections::belt::{Belt, BeltFittings, BeltStyle};

/// Strap thickness.
const THICKNESS: f32 = 0.05;
/// Clearance between the body and the belt's back.
const CLEARANCE: f32 = 0.02;

/// Where a belt lies on a form: its height round the body and the body's surface there.
#[derive(Clone, Debug)]
pub struct BeltLine {
    pub profile: TorsoProfile,
    /// Height of the belt's centre at the front.
    pub y: f32,
    pub slant: f32,
    pub width: f32,
}

impl BeltLine {
    pub fn new(form: &TorsoForm, belt: &Belt, fittings: &BeltFittings) -> BeltLine {
        let form = form.eased(fittings.girth);
        BeltLine { profile: form.profile(), y: form.waist.y - 0.25, slant: fittings.slant, width: belt.width }
    }

    /// Height of the belt's centre at `angle` (lathe convention): lowest on the left hip.
    pub fn height(&self, angle: f32) -> f32 {
        self.y - self.slant * (0.5 + 0.5 * angle.cos())
    }

    /// The body's surface at `angle` and height `y`, `off` out, with its outward normal.
    pub fn surface(&self, angle: f32, y: f32, off: f32) -> (V3, V3) {
        let p = self.profile.point(angle, y);
        let da = vec3::sub(self.profile.point(angle + 1e-3, y), self.profile.point(angle - 1e-3, y));
        let dy = vec3::sub(self.profile.point(angle, y + 1e-3), self.profile.point(angle, y - 1e-3));
        let n = vec3::normalize_or(vec3::cross(da, dy), [p[0], 0.0, p[2]]);
        (vec3::add(p, vec3::scale(n, off)), n)
    }

    /// The belt's back at `angle`, `along` across its width (-0.5 lower edge to 0.5 upper),
    /// `off` out from it, and the outward normal there.
    pub fn at(&self, angle: f32, along: f32, off: f32) -> (V3, V3) {
        self.surface(angle, self.height(angle) + along * self.width, CLEARANCE + off)
    }

    /// The direction round the belt at `angle` (increasing angle).
    pub fn tangent(&self, angle: f32) -> V3 {
        vec3::normalize_or(vec3::sub(self.at(angle + 1e-3, 0.0, 0.0).0, self.at(angle - 1e-3, 0.0, 0.0).0), vec3::X)
    }

    /// A small strap looped round the belt at `angle`, `width` wide, standing `slack` proud
    /// of its face (a keeper, a pouch loop, a frog's top).
    pub fn loop_round(&self, angle: f32, width: f32, slack: f32) -> MeshBuilder {
        let (p, n) = self.at(angle, 0.0, 0.0);
        let up = vec3::normalize_or(vec3::cross(n, self.tangent(angle)), vec3::Y);
        let up = if up[1] < 0.0 { vec3::neg(up) } else { up };
        let h = self.width * 0.5 + 0.03;
        let out = THICKNESS + 0.02 + slack;
        let corners = [(-0.012, h), (out, h * 1.02), (out + slack * 0.3, 0.0), (out, -h * 1.02), (-0.012, -h), (-0.02, 0.0)];
        let control: Vec<V3> = corners.iter().map(|&(a, b)| vec3::add(p, vec3::add(vec3::scale(n, a), vec3::scale(up, b)))).collect();
        let path = CatmullRom::centripetal(control).with_closed(true).sample(8);
        let centre = vec3::add(p, vec3::scale(n, out * 0.5));
        let faces: Vec<V3> = path.iter().map(|&q| vec3::sub(q, centre)).collect();
        strap(&path, &faces, width, 0.03, true)
    }
}

/// Angles (lathe convention) of the frogs: left hip, then right hip, both a little forward.
fn frog_angle(i: u32) -> f32 {
    if i == 0 { TAU - 0.55 } else { PI + 0.55 }
}

/// Where a pouch hangs: behind the right hip.
const POUCH_ANGLE: f32 = PI - 0.45;

/// Where a small sack hangs: behind the left hip.
const SACK_ANGLE: f32 = 0.95;

/// The middle of the potion pouch's three loops: between the buckle and the right frog.
const POTION_ANGLE: f32 = FRONT - 0.55;
/// How far round the belt the pouch's loops stand apart.
const POTION_STEP: f32 = 0.14;

/// Every part of a belt on `form`: `"belt"`, `"tail"`, `"buckle"`, `"keeper"`, `"tip"`,
/// `"studs"`, `"frogs"`, `"loops"` and `"rings"`, as the state calls for them.
pub fn belt_parts(form: &TorsoForm, belt: &Belt, fittings: &BeltFittings) -> Parts {
    let mut parts = Parts::new();
    let line = BeltLine::new(form, belt, fittings);
    let w = belt.width;
    let leather = Look::of(belt.material);
    let metal = Look::of(belt.buckle);

    // The strap round the body.
    let n = 160;
    let (path, faces): (Vec<V3>, Vec<V3>) = (0..n).map(|i| line.at(TAU * i as f32 / n as f32, 0.0, 0.0)).unzip();
    parts.add_look("belt", strap(&path, &faces, w, THICKNESS, true), leather);

    // The buckle at the front, the free end through it toward the left hip and, if long
    // enough, turned down to hang.
    let (front, fn_) = line.at(FRONT, 0.0, THICKNESS);
    let mut b = buckle(w, (w * 0.08).max(0.018));
    b.place_basis(front, line.tangent(FRONT), fn_);
    parts.add_look("buckle", b, metal);

    let r = line.at(FRONT, 0.0, 0.0).0[2].abs().max(0.5);
    // A short end lies along the belt; a long one runs a little way and hangs.
    let (along, hang) = if fittings.tail > 0.9 { (0.45, fittings.tail - 0.45) } else { (fittings.tail, 0.0) };
    let a_end = FRONT + along / r;
    let mut tail_path: Vec<V3> = (0..=10).map(|i| line.at(FRONT + 0.02 + (a_end - FRONT - 0.02) * i as f32 / 10.0, 0.0, THICKNESS).0).collect();
    let mut tail_faces: Vec<V3> = (0..=10).map(|i| line.at(FRONT + 0.02 + (a_end - FRONT - 0.02) * i as f32 / 10.0, 0.0, THICKNESS).1).collect();
    if hang > 0.0 {
        // Over the belt's lower edge and down the front.
        let (edge, en) = line.at(a_end + 0.02, -0.5, THICKNESS * 1.6);
        let top = line.height(a_end) - w * 0.5;
        let control: Vec<V3> = [vec3::add(edge, [0.0, w * 0.2, 0.0]), edge]
            .into_iter()
            .chain((1..=8).map(|k| line.surface(a_end + 0.03, top - hang * k as f32 / 8.0, CLEARANCE + THICKNESS * 1.6 + 0.02).0))
            .collect();
        let drop = CatmullRom::centripetal(control).sample_evenly(24);
        tail_faces.extend(drop.iter().skip(1).map(|_| en));
        tail_path.extend(drop.into_iter().skip(1));
    }
    let tail = strap(&tail_path, &tail_faces, w * 0.96, THICKNESS * 0.9, false);
    parts.add_look("tail", tail, leather);
    if fittings.tip {
        let k = tail_path.len() - 1;
        let dir = vec3::normalize_or(vec3::sub(tail_path[k], tail_path[k - 1]), vec3::neg(vec3::Y));
        let face = tail_faces[k];
        let length = 0.1 + 0.25 * w;
        let mut end = cuboid([w * 1.02, length, THICKNESS * 1.8]);
        end.place_basis(vec3::add(vec3::sub(tail_path[k], vec3::scale(dir, length * 0.5)), vec3::scale(face, THICKNESS * 0.45)), dir, face);
        parts.add_look("tip", end, metal);
    }
    if fittings.keeper {
        parts.add_look("keeper", line.loop_round(FRONT + (along * 0.6) / r, 0.1, THICKNESS), leather);
    }

    // Studs, or a girdle's bar mounts, evenly round the belt clear of the buckle and frogs.
    if fittings.studs > 0 {
        let skip = |a: f32| {
            let near = |b: f32| ((a - b + PI).rem_euclid(TAU) - PI).abs() < 0.3;
            near(FRONT) || near(FRONT + 0.35) || (0..fittings.frogs).any(|i| near(frog_angle(i))) || fittings.pouch && near(POUCH_ANGLE) || fittings.potion_pouch && near(POTION_ANGLE) || fittings.sack && near(SACK_ANGLE)
        };
        let angles: Vec<f32> =
            (0..fittings.studs).map(|i| FRONT + 0.4 + (TAU - 0.8) * (i as f32 + 0.5) / fittings.studs as f32).filter(|&a| !skip(a)).collect();
        let spots: Vec<(V3, V3)> = angles.iter().map(|&a| line.at(a, 0.0, THICKNESS)).collect();
        if fittings.style == BeltStyle::Girdle {
            let mut mounts = MeshBuilder::new();
            for (&a, &(p, n)) in angles.iter().zip(&spots) {
                let mut bar = cuboid([0.07, w * 0.8, 0.03]);
                let up = vec3::normalize_or(vec3::cross(n, line.tangent(a)), vec3::Y);
                bar.place_basis(vec3::add(p, vec3::scale(n, 0.012)), if up[1] < 0.0 { vec3::neg(up) } else { up }, n);
                mounts.append(&bar);
            }
            parts.add_look("studs", mounts, metal);
        } else {
            parts.add_look("studs", rivets(&spots, w * 0.17), metal);
        }
    }

    // Frogs: a tab looped over the belt, hanging down the hip to a sleeve for the scabbard.
    if fittings.frogs > 0 {
        let mut frogs = MeshBuilder::new();
        for i in 0..fittings.frogs {
            let a = frog_angle(i);
            frogs.append(&line.loop_round(a, 0.22, 0.02));
            let top = line.height(a) - w * 0.5;
            let drop = 0.75;
            let (tab, tab_faces): (Vec<V3>, Vec<V3>) =
                (0..=10).map(|k| line.surface(a, top + 0.05 - drop * k as f32 / 10.0, CLEARANCE + THICKNESS + 0.03)).unzip();
            frogs.append(&strap(&tab, &tab_faces, 0.2, 0.04, false));
            let (bottom, n) = line.surface(a, top - drop + 0.12, CLEARANCE + THICKNESS + 0.25);
            let t = line.tangent(a);
            let axis = vec3::normalize_or(vec3::add(vec3::Y, vec3::scale(t, if i == 0 { -0.45 } else { 0.45 })), vec3::Y);
            let u = vec3::normalize_or(vec3::sub(n, vec3::scale(axis, vec3::dot(n, axis))), n);
            let v = vec3::cross(axis, u);
            let sleeve: Vec<V3> = (0..32)
                .map(|k| {
                    let th = TAU * k as f32 / 32.0;
                    vec3::add(bottom, vec3::add(vec3::scale(u, 0.2 * th.cos()), vec3::scale(v, 0.14 * th.sin())))
                })
                .collect();
            let sleeve_faces: Vec<V3> = sleeve.iter().map(|&q| vec3::sub(q, bottom)).collect();
            frogs.append(&strap(&sleeve, &sleeve_faces, 0.18, 0.04, true));
        }
        parts.add_look("frogs", frogs, leather);
    }

    // The potion pouch: three leather sleeves hung from the belt's lower edge, a vial standing
    // in each with its cork showing. Decorative: what is carried in it is the inventory's.
    if fittings.potion_pouch {
        let mut sleeves = MeshBuilder::new();
        let mut corks = MeshBuilder::new();
        for k in 0..3 {
            let a = POTION_ANGLE + (k as f32 - 1.0) * POTION_STEP;
            sleeves.append(&line.loop_round(a, 0.12, 0.015));
            let top = line.height(a) - w * 0.5;
            let (tab, tab_faces): (Vec<V3>, Vec<V3>) =
                (0..=6).map(|j| line.surface(a, top + 0.05 - 0.4 * j as f32 / 6.0, CLEARANCE + THICKNESS + 0.025)).unzip();
            sleeves.append(&strap(&tab, &tab_faces, 0.12, 0.03, false));
            // The sleeve itself, standing off the tab so a vial fits behind its face.
            let (centre, n) = line.surface(a, top - 0.3, CLEARANCE + THICKNESS + 0.11);
            let t = line.tangent(a);
            let ring: Vec<V3> = (0..16)
                .map(|j| {
                    let th = TAU * j as f32 / 16.0;
                    vec3::add(centre, vec3::add(vec3::scale(n, 0.085 * th.cos()), vec3::scale(t, 0.085 * th.sin())))
                })
                .collect();
            let ring_faces: Vec<V3> = ring.iter().map(|&q| vec3::sub(q, centre)).collect();
            sleeves.append(&strap(&ring, &ring_faces, 0.2, 0.025, true));
            let mut cork = cylinder(0.05, 0.16, 8, Shading::SMOOTH);
            cork.translate(vec3::add(centre, [0.0, 0.1, 0.0]));
            corks.append(&cork);
        }
        parts.add_look("potion_pouch", sleeves, leather);
        parts.add_look("vials", corks, Look::of(MaterialKind::Wood));
    }

    // Pouch loops at the back, and D-rings on tabs at the sides.
    if fittings.loops > 0 {
        let mut loops = MeshBuilder::new();
        for i in 0..fittings.loops {
            let a = PI / 2.0 + (i as f32 - (fittings.loops - 1) as f32 * 0.5) * 0.42;
            loops.append(&line.loop_round(a, 0.16, 0.06));
        }
        parts.add_look("loops", loops, leather);
    }
    if fittings.rings > 0 {
        let mut rings = MeshBuilder::new();
        let mut tabs = MeshBuilder::new();
        for i in 0..fittings.rings {
            let a = [0.2, PI - 0.15, 0.75][i as usize % 3];
            tabs.append(&line.loop_round(a, 0.1, 0.0));
            let (p, n) = line.at(a, -0.5, THICKNESS + 0.03);
            let mut ring = d_ring(0.13, 0.02);
            ring.transform(&Mat3::from_cols(line.tangent(a), vec3::neg(vec3::Y), n)).translate(vec3::add(p, [0.0, -0.02, 0.0]));
            rings.append(&ring);
        }
        parts.add_look("loops", tabs, leather);
        parts.add_look("rings", rings, metal);
    }
    parts
}

/// Where a pouch hangs on the belt: the point on the belt's face behind the right hip and
/// the outward direction there. Set a pouch's back on the point, facing the direction, its
/// belt loops straddling the belt.
pub fn belt_pouch_mount(form: &TorsoForm, belt: &Belt, fittings: &BeltFittings) -> (V3, V3) {
    let line = BeltLine::new(form, belt, fittings);
    let (p, n) = line.at(POUCH_ANGLE, 0.0, THICKNESS);
    (p, vec3::normalize_or([n[0], 0.0, n[2]], n))
}

/// Where a small sack hangs on the belt: the point on the belt's face behind the left hip and the
/// outward direction there, like [`belt_pouch_mount`].
pub fn belt_sack_mount(form: &TorsoForm, belt: &Belt, fittings: &BeltFittings) -> (V3, V3) {
    let line = BeltLine::new(form, belt, fittings);
    let (p, n) = line.at(SACK_ANGLE, 0.0, THICKNESS);
    (p, vec3::normalize_or([n[0], 0.0, n[2]], n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    #[test]
    fn the_belt_lies_round_the_waist() {
        let form = TorsoForm::MANNEQUIN;
        let belt = Belt::default();
        let line = BeltLine::new(&form, &belt, &BeltFittings::default());
        for i in 0..16 {
            let a = TAU * i as f32 / 16.0;
            let (p, n) = line.at(a, 0.0, 0.0);
            let body = line.profile.point(a, p[1]);
            assert!(vec3::dot(vec3::sub(p, body), n) > 0.0, "the belt stands off the body at {a}");
            assert!((p[1] - (form.waist.y - 0.25)).abs() < 0.01);
        }
        line.loop_round(FRONT, 0.1, 0.05).into_part("loop", OutlineMode::Welded).validate().unwrap();
    }
}
