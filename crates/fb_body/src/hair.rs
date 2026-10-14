//! Hair, brows and beards (the plate prototype's `buildHair`, `buildBeard` and the brows in
//! `buildHead`): lumpy offset surfaces of the head, styled by [`HairStyle`] and [`Beard`].
//!
//! Departures from the plate, all our own: every sheet has a little thickness so its edges
//! outline cleanly; the lumps are seeded value noise on top of the plate's ripples, so two
//! heads of the same style differ; long hair, a ponytail and a long beard are pushed clear of
//! the trunk and the upper arms (plus any clothing clearance the caller gives), not only the
//! trunk; and a lizard's frill is built with its head ([`crate::head`]), so
//! [`HairStyle::Crest`] adds nothing here.

use std::f32::consts::{PI, TAU};

use armory::build::form::{ShellOptions, patch, shell};
use armory::build::hafted::smoothstep;
use armory::geom::vec3::{self, V3};
use armory::geom::{CatmullRom, MeshBuilder, OutlineMode, Shading, SweepOptions, spike, torus, tube, uv_sphere, value_noise3};
use armory::{ItemMesh, MaterialKind, Rgb};

use crate::body::Body;
use crate::folk::{Beard, Folk, HairStyle};
use crate::head::Face;
use crate::limb::Side;

/// What [`Body::hair_mesh`] should leave room for.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HairFit {
    /// How far outside the bare skin long hair must hang: the thickness of everything worn
    /// over the shoulders (the plate's running `maxOff`).
    pub clearance: f32,
    /// A helmet or hat covers the crown: no cap, ridge or topknot (long hair and tails still
    /// hang below it).
    pub covered: bool,
}

/// How far a head's hair stands off the scalp (the plate's `hairVol`), for sizing a helmet
/// over it: halved under a cap.
pub fn hair_volume(style: HairStyle, head_radius: f32, covered: bool) -> f32 {
    let v = match style {
        HairStyle::Shaggy => 0.25,
        HairStyle::Long => 0.18,
        HairStyle::Bald | HairStyle::Crest => 0.0,
        _ => 0.1,
    };
    v * head_radius * if covered { 0.5 } else { 1.0 }
}

/// The plate's per-style cap: offset at the hairline, extra volume over the crown, lumpiness,
/// and how far down the back it reaches.
fn cap_style(style: HairStyle) -> (f32, f32, f32, f32) {
    match style {
        HairStyle::Shaggy => (0.09, 0.2, 0.09, 0.38),
        HairStyle::Long => (0.07, 0.13, 0.03, 0.31),
        HairStyle::Crop => (0.05, 0.04, 0.012, 0.31),
        _ => (0.05, 0.05, 0.012, 0.31),
    }
}

impl Body {
    /// How far this body's hair stands off its scalp ([`hair_volume`]).
    pub fn hair_volume(&self, covered: bool) -> f32 {
        hair_volume(self.spec.hair, self.face().r, covered)
    }

    /// Hair, brows and beard: `"hair"`, `"brows"`, `"beard"` and a ponytail's `"hair_tie"`.
    pub fn hair_mesh(&self, fit: HairFit) -> ItemMesh {
        let face = self.face();
        let mut hair = MeshBuilder::new();
        let mut tie = MeshBuilder::new();
        self.scalp_hair(&face, fit, &mut hair, &mut tie);
        let brows = self.brows(&face);
        let beard = self.beard(&face, fit);
        let colour = self.spec.hair_colour;
        let brow_colour = if self.folk() == Folk::Goblin { self.spec.skin.scaled(0.6) } else { colour };
        let mut parts = vec![];
        let mut add = |m: MeshBuilder, name: &'static str, material: MaterialKind, tint: Rgb| {
            if !m.is_empty() {
                parts.push(m.into_part(name, OutlineMode::Welded).with_material(material).with_tint(tint));
            }
        };
        add(hair, "hair", MaterialKind::Hair, colour);
        add(brows, "brows", MaterialKind::Hair, brow_colour);
        add(beard, "beard", MaterialKind::Hair, colour);
        add(tie, "hair_tie", MaterialKind::Leather, Rgb::hex(0x5a3a26));
        ItemMesh { parts }
    }

    /// Lumps: the plate's crossed ripples plus seeded value noise, `0..1` scaled by `lump`.
    fn lumps(&self, at: V3, th: f32, ph: f32, lump: f32) -> f32 {
        let ns = (self.spec.seed % 1000) as f32 * 0.01;
        let ripple = (th * 7.0 + ns).sin() * (ph * 9.0 + ns * 1.7).sin();
        let noise = value_noise3(vec3::scale(at, 2.6 / self.face().r), self.spec.seed) - 0.5;
        lump * (ripple * 0.6 + noise * 1.4)
    }

    /// The hairline's polar angle at azimuth `th` (`phiMax`): over the brow in front, down to
    /// the nape behind.
    fn hairline(&self, style: HairStyle, th: f32) -> f32 {
        let (_, _, _, back) = cap_style(style);
        let ns = (self.spec.seed % 1000) as f32 * 0.01;
        let b = (1.0 - th.cos()) / 2.0;
        let shag = if style == HairStyle::Shaggy { 0.035 * (th * 11.0 + ns).sin() } else { 0.0 };
        PI * (0.31 + back * b.powf(0.75) + shag)
    }

    fn scalp_hair(&self, face: &Face, fit: HairFit, hair: &mut MeshBuilder, tie: &mut MeshBuilder) {
        let style = self.spec.hair;
        let r = face.r;
        match style {
            HairStyle::Bald | HairStyle::Crest => return,
            HairStyle::Mohawk => {
                if fit.covered {
                    return;
                }
                for i in 0..9 {
                    let a = -0.22 + i as f32 * 0.105;
                    let (az, polar) = if a < 0.0 { (0.0, -a * PI) } else { (PI, a * PI) };
                    let root = face.point(az, polar.max(0.001), -0.04 * r);
                    let out = vec3::normalize_or(vec3::sub(root, face.head.centre), vec3::Y);
                    let dir = vec3::normalize_or(vec3::add(out, [0.0, 0.1, -0.35]), vec3::Y);
                    let length = (0.5 + 0.25 * (PI * i as f32 / 8.0).sin()) * r;
                    let mut s = spike(0.13 * r, length, 8, 0.25, Shading::SMOOTH);
                    s.place(root, dir);
                    hair.append(&s);
                }
                return;
            }
            _ => {}
        }
        let (base, vol, lump, _) = cap_style(style);
        if !fit.covered {
            // Rows from the hairline up to the crown, so faces point outward.
            let rows: Vec<Vec<V3>> = (0..=18)
                .map(|j| {
                    let v = 1.0 - j as f32 / 18.0;
                    (0..64)
                        .map(|i| {
                            let th = -PI + TAU * i as f32 / 64.0;
                            let ph = (self.hairline(style, th) * v).max(1e-3);
                            let at = face.point(th, ph, 0.0);
                            let mut off = (base + vol * (1.0 - v * 0.6)) * r + self.lumps(at, th, ph, lump) * r;
                            off *= 1.0 - 0.75 * smoothstep(0.8, 1.0, v);
                            off += 0.015 * r;
                            face.point(th, ph, off.max(0.008 * r))
                        })
                        .collect()
                })
                .collect();
            hair.append(&shell(&rows, &ShellOptions { wrap_around: true, ..ShellOptions::sheet(0.025 * r) }));
        }
        match style {
            HairStyle::Long => {
                hair.append(&self.long_hair(face, fit, base, vol));
            }
            HairStyle::Ponytail => {
                let (tail, band) = self.ponytail(face, fit);
                hair.append(&tail);
                tie.append(&band);
            }
            HairStyle::Topknot if !fit.covered => {
                let mut knot = uv_sphere(1.0, 16, 10);
                knot.scale([0.34 * r, 0.3 * r, 0.34 * r]).translate(face.point(PI * 0.85, 0.13 * PI, 0.22 * r));
                hair.append(&knot);
            }
            _ => {}
        }
    }

    /// Long hair: a curtain from the back and sides of the cap down past the shoulders,
    /// pushed clear of the trunk and arms.
    fn long_hair(&self, face: &Face, fit: HairFit, base: f32, vol: f32) -> MeshBuilder {
        let r = face.r;
        let ns = (self.spec.seed % 1000) as f32 * 0.01;
        let length = if self.folk() == Folk::Dwarf { 0.55 } else { 0.42 } * self.trunk_length + r * 0.6;
        let c = face.head.centre;
        let top = self.shoulder_y + 0.2 * self.trunk_length;
        patch(36, 18, &ShellOptions::sheet(0.03 * r), |u, v| {
            // Round the back from behind one ear to behind the other.
            let th = 1.3 + (TAU - 2.6) * (1.0 - u);
            let s = face.point(th, self.hairline(HairStyle::Long, th) * 0.93, (base + vol * 0.45) * r);
            let l = length * (1.0 - 0.07 * (th * 9.0 + ns).sin());
            let y = s[1] - l * v;
            let (dx, dz) = (s[0] - c[0], s[2] - c[2]);
            let rad0 = dx.hypot(dz).max(1e-3);
            let (ux, uz) = (dx / rad0, dz / rad0);
            let rad = rad0 * (1.0 + 0.1 * v);
            let p = [c[0] + ux * rad, y, c[2] + uz * rad];
            if y < top { self.clear_of_body(p, fit.clearance + 0.1 * r, smoothstep(top, self.shoulder_y - 0.05 * self.trunk_length, y)) } else { p }
        })
    }

    /// A tail tied at the back of the head, falling down the back.
    fn ponytail(&self, face: &Face, fit: HairFit) -> (MeshBuilder, MeshBuilder) {
        let r = face.r;
        let t = self.trunk_length;
        let s0 = face.point(PI, 0.55 * PI, 0.08 * r);
        let back = |y: f32, extra: f32| {
            let (p, _) = self.trunk_surface(armory::build::form::BACK, y, fit.clearance + extra);
            p
        };
        let path = CatmullRom::centripetal(vec![
            s0,
            vec3::add(s0, [0.0, -0.3 * r, -0.35 * r]),
            back(self.shoulder_y + 0.05 * t, 0.25),
            back(self.shoulder_y - 0.18 * t, 0.15),
        ])
        .sample_evenly(28);
        let tail = tube(&path, 12, &SweepOptions::default(), |u| r * (0.2 * (1.0 - u) + 0.06 + 0.05 * (PI * u).sin()));
        let mut band = torus(0.13 * r, 0.05 * r, 14, 6, Shading::SMOOTH);
        let along = vec3::normalize_or(vec3::sub(path[2], path[0]), vec3::neg(vec3::Y));
        band.place(vec3::lerp(path[0], path[2], 0.5), along);
        (tail, band)
    }

    /// Moves `p` out of the trunk and the upper arms until it is `clearance` clear of the
    /// skin, by `amount` (0 leaves it, 1 clears it fully).
    pub fn clear_of_body(&self, p: V3, clearance: f32, amount: f32) -> V3 {
        if amount <= 0.0 {
            return p;
        }
        let mut q = p;
        // Pushing off an arm can push into the trunk where they meet, so alternate a few times.
        for _ in 0..4 {
            // The trunk, along the horizontal ray from its axis through the point.
            let d = q[0].hypot(q[2]);
            let reach = self.trunk_reach([q[0], q[2]], q[1]) + clearance;
            if d < reach {
                q = if d > 1e-4 { [q[0] * reach / d, q[1], q[2] * reach / d] } else { [q[0], q[1], -reach] };
            }
            // The upper arms, out from the nearest point on each arm's line.
            for side in Side::BOTH {
                let arm = self.arm(side);
                let n = arm.path.len();
                let upto = ((arm.t_joints[0] + 0.5 * (arm.t_joints[1] - arm.t_joints[0])) * (n - 1) as f32) as usize;
                if let Some((i, a)) = arm.path[..upto.max(2)].iter().enumerate().min_by(|x, y| vec3::distance(*x.1, q).total_cmp(&vec3::distance(*y.1, q))) {
                    let reach = arm.radius_at(i as f32 / (n - 1) as f32) + clearance;
                    // Out along the ray from the body's axis, so hair behind the shoulder
                    // stays behind it instead of spreading round to the front.
                    let ray = vec3::normalize_or([q[0], 0.0, q[2]], vec3::neg(vec3::Z));
                    let mut steps = 0;
                    while vec3::distance(*a, q) < reach && steps < 80 {
                        q = vec3::add(q, vec3::scale(ray, 0.02 * self.scale));
                        steps += 1;
                    }
                }
            }
        }
        vec3::lerp(p, q, amount.min(1.0))
    }

    /// How far the trunk's skin reaches from its axis toward horizontal direction `dir` at
    /// height `y`.
    pub fn trunk_reach(&self, dir: [f32; 2], y: f32) -> f32 {
        let l = dir[0].hypot(dir[1]);
        if l < 1e-6 {
            return 0.0;
        }
        let (ux, uz) = (dir[0] / l, dir[1] / l);
        let y = y.clamp(self.trunk().bottom(), self.trunk().top());
        // The section point whose direction from the axis is closest to the ray's.
        let (len, cos) = (0..180)
            .map(|i| {
                let [x, z] = self.trunk().section(TAU * i as f32 / 180.0, y, 0.0);
                let len = x.hypot(z).max(1e-6);
                (len, (x * ux + z * uz) / len)
            })
            .fold((0.0, -2.0), |best, s| if s.1 > best.1 { s } else { best });
        len * cos.max(0.0)
    }

    /// Brows: a tapering stroke over each eye, tilted by the spec's brow. Lizardfolk have
    /// none.
    fn brows(&self, face: &Face) -> MeshBuilder {
        let mut out = MeshBuilder::new();
        if self.folk() == Folk::Lizardfolk {
            return out;
        }
        let r = face.r;
        let tilt = (self.spec.brow - 0.5) * 0.1 * PI;
        let width = match self.folk() {
            Folk::Dwarf => 0.085,
            Folk::Goblin => 0.035,
            _ => 0.055,
        };
        for sg in [1.0f32, -1.0] {
            let pts = CatmullRom::centripetal(vec![
                face.point(sg * 0.14, 0.43 * PI + tilt * 0.6, 0.03 * r),
                face.point(sg * 0.38, 0.415 * PI, 0.04 * r),
                face.point(sg * 0.62, 0.435 * PI - tilt * 0.4, 0.03 * r),
            ])
            .sample_evenly(14);
            out.append(&tube(&pts, 8, &SweepOptions::default(), |t| width * r * (1.0 - 0.4 * (t - 0.35).abs())));
        }
        out
    }

    /// A beard and moustache: a sheet over the jaw from ear to ear that hangs down by the
    /// style's length, a forked beard parting at its tip.
    fn beard(&self, face: &Face, fit: HairFit) -> MeshBuilder {
        let style = self.spec.beard;
        let mut out = MeshBuilder::new();
        if style == Beard::None || self.folk() == Folk::Lizardfolk {
            return out;
        }
        let r = face.r;
        let dwarf = if self.folk() == Folk::Dwarf { 1.45 } else { 1.0 };
        let length = match style {
            Beard::Stubble | Beard::None => 0.0,
            Beard::Short => 0.3,
            Beard::Full => 0.75,
            Beard::Long => 1.7,
            Beard::Forked => 1.5,
        } * r
            * dwarf;
        let thick = match style {
            Beard::Stubble => 0.02,
            Beard::Short => 0.07,
            _ => 0.13,
        };
        let hx = face.head.centre[0];
        let forked = style == Beard::Forked;
        let lump = if style == Beard::Stubble { 0.0 } else { 0.025 };
        let sheet = patch(32, 20, &ShellOptions::sheet(0.025 * r), |u, v| {
            let th = (u * 2.0 - 1.0) * 1.72;
            let ph0 = PI * (0.565 + 0.135 * (th * 0.85).cos().powi(2));
            let t = (v * 1.7).min(1.0);
            let ph = ph0 + (PI * 0.9 - ph0) * t;
            let at = face.point(th, ph, 0.0);
            let off = thick * (0.5 + (PI * (v * 1.15).min(1.0)).sin()) * r + self.lumps(at, th, ph, lump) * r;
            let mut p = face.point(th, ph, off.max(0.01 * r));
            let ex = (v - 0.42).max(0.0) / 0.58;
            if ex > 0.0 && length > 0.0 {
                // The sides by the ears stay short, so the beard hangs from the chin and jaw
                // and not over the shoulders.
                let hang = 1.0 - smoothstep(0.8, 1.72, th.abs());
                p[1] -= length * ex * hang;
                let k = 1.0 - 0.7 * ex * hang * if forked { 0.55 } else { 1.0 };
                p[0] = hx + (p[0] - hx) * k;
                p[2] += 0.18 * r * ex;
                if forked && th.abs() > 0.08 {
                    p[0] += (p[0] - hx).signum() * 0.14 * r * ex * ex;
                }
                // Hanging over the chest, it rests on it rather than sinking in.
                if p[1] < self.chin_y {
                    p = self.clear_of_body(p, fit.clearance + 0.04 * r, 1.0);
                }
            }
            p
        });
        out.append(&sheet);
        if style != Beard::Stubble {
            let drop = if matches!(style, Beard::Long | Beard::Forked) { 0.22 } else { 0.08 };
            let pts = CatmullRom::centripetal(vec![
                vec3::add(face.point(-0.46, 0.67 * PI, 0.05 * r), [0.0, -drop * r, 0.0]),
                face.point(-0.18, 0.645 * PI, 0.07 * r),
                face.point(0.0, 0.645 * PI, 0.08 * r),
                face.point(0.18, 0.645 * PI, 0.07 * r),
                vec3::add(face.point(0.46, 0.67 * PI, 0.05 * r), [0.0, -drop * r, 0.0]),
            ])
            .sample_evenly(22);
            out.append(&tube(&pts, 10, &SweepOptions::default(), |t| r * (0.05 + 0.04 * (PI * t).sin())));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::BodySpec;

    fn with(folk: Folk, seed: u32, hair: HairStyle, beard: Beard) -> Body {
        let mut spec = BodySpec::roll(folk, seed);
        spec.hair = hair;
        spec.beard = beard;
        Body::new(&spec)
    }

    #[test]
    fn every_listed_style_builds() {
        for folk in Folk::ALL {
            for &style in &folk.race().hairs {
                for beard in Beard::ALL {
                    let body = with(folk, 31, style, beard);
                    for covered in [false, true] {
                        let mesh = body.hair_mesh(HairFit { clearance: 0.1, covered });
                        for part in &mesh.parts {
                            part.validate().unwrap_or_else(|e| panic!("{folk:?} {style:?} {beard:?}: {e}"));
                        }
                        let has = |n: &str| mesh.parts.iter().any(|p| p.name == n);
                        let bare = matches!(style, HairStyle::Bald | HairStyle::Crest) || (covered && matches!(style, HairStyle::Mohawk | HairStyle::Crop | HairStyle::Shaggy | HairStyle::Topknot));
                        assert_eq!(has("hair"), !bare, "{folk:?} {style:?} covered {covered}");
                        assert_eq!(has("brows"), folk != Folk::Lizardfolk);
                        assert_eq!(has("beard"), beard != Beard::None && folk != Folk::Lizardfolk);
                    }
                }
            }
        }
        // And every style on any folk, listed or not.
        for style in HairStyle::ALL {
            for part in with(Folk::Human, 3, style, Beard::Forked).hair_mesh(HairFit::default()).parts {
                part.validate().unwrap();
            }
        }
    }

    #[test]
    fn long_hair_never_passes_inside_the_shoulders() {
        for folk in [Folk::Human, Folk::Dwarf, Folk::Gnome] {
            for seed in [1, 2, 1207] {
                for clearance in [0.0, 0.25] {
                    let body = with(folk, seed, HairStyle::Long, Beard::None);
                    let mesh = body.hair_mesh(HairFit { clearance, covered: false });
                    let hair = mesh.parts.iter().find(|p| p.name == "hair").unwrap();
                    let below = body.shoulder_y - 0.05 * body.trunk_length;
                    for p in hair.positions.iter().filter(|p| p[1] < below) {
                        let reach = body.trunk_reach([p[0], p[2]], p[1]);
                        assert!(p[0].hypot(p[2]) >= reach + clearance - 0.01, "{folk:?} {seed}: hair at {p:?} inside the trunk ({reach})");
                        for side in Side::BOTH {
                            let arm = body.arm(side);
                            let upto = (arm.t_joints[0] * (arm.path.len() - 1) as f32) as usize;
                            for (i, a) in arm.path[..upto].iter().enumerate() {
                                let rr = arm.radius_at(i as f32 / (arm.path.len() - 1) as f32);
                                assert!(vec3::distance(*a, *p) > rr * 0.98, "{folk:?}: hair at {p:?} inside the shoulder");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn hair_is_deterministic_and_seeded() {
        for style in [HairStyle::Shaggy, HairStyle::Long, HairStyle::Crop] {
            let a = with(Folk::Human, 8, style, Beard::Full).hair_mesh(HairFit::default());
            assert_eq!(a, with(Folk::Human, 8, style, Beard::Full).hair_mesh(HairFit::default()));
            assert_ne!(a, with(Folk::Human, 9, style, Beard::Full).hair_mesh(HairFit::default()));
        }
    }
}
