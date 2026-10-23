//! Heads and faces (the plate prototype's `headFn` and `buildHead`): the skull on a
//! [`HeadForm`], eyes, a nose or snout, a mouth, ears, and each folk's own features (a
//! lizard's snout, teeth and frill, a pig's snout disc and tusks, goblin fangs).
//!
//! Departures from the plate, all our own: the skin carries a little relief (a brow ridge,
//! eye sockets the eyes sit in, cheekbones); eyes have an upper lid, so they read as eyes at
//! doll size and keep an expression close up; ears are cupped sheets with a rolled rim
//! rather than flat ellipsoids and extruded blades; a lizard's snout has a row of teeth
//! along its jaw line, and its frill is spines joined by a membrane instead of bare cones.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use armory::build::form::{HeadForm, ShellOptions, patch, rolled_edge};
use armory::geom::vec3::{self, V3};
use armory::geom::{CatmullRom, GridOptions, LatheOptions, MeshBuilder, OutlineMode, Shading, SweepOptions, lathe, spike, stitch_grid, tube, uv_sphere};
use armory::{ItemMesh, MaterialKind, Part, Rgb};
use procgen_rng::Mulberry32;

use crate::body::{Body, lighten};
use crate::folk::{Ears, EyeKind, Folk, Nose};

const EYE_WHITE: Rgb = Rgb::hex(0xefe8dc);
const PUPIL: Rgb = Rgb::hex(0x120c0a);
const MOUTH: Rgb = Rgb::hex(0x2a1712);
const IVORY: Rgb = Rgb::hex(0xefe6cf);

fn bump(x: f32, width: f32) -> f32 {
    (-(x / width).powi(2)).exp()
}

/// A head's skin with its relief, for placing features on.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    pub head: HeadForm,
    pub folk: Folk,
    /// The head's radius (its larger half extent across).
    pub r: f32,
    /// Azimuth and polar angle of the eyes' centres (left eye; the right mirrors).
    pub eye_azimuth: f32,
    pub eye_polar: f32,
}

impl Face {
    pub fn new(head: HeadForm, folk: Folk) -> Face {
        let (eye_azimuth, eye_polar) = match folk {
            Folk::Lizardfolk => (0.95, 0.43 * PI),
            Folk::Pigfolk => (0.34, 0.5 * PI),
            Folk::Minotaur => (1.12, 0.46 * PI),
            // A one-eyed face keeps a nominal spread: it is how wide a hood's opening and
            // the brow's stroke run, though the one eye sits on the middle, a little high.
            Folk::Cyclops => (0.4, 0.43 * PI),
            _ => (0.4, 0.5 * PI),
        };
        Face { head, folk, r: head.half_width.max(head.half_depth), eye_azimuth, eye_polar }
    }

    /// Whether the face has one eye, centred ([`EyeKind::One`]).
    pub fn one_eyed(&self) -> bool {
        self.folk.race().eyes == EyeKind::One
    }

    /// Relief over the head form, in units of the head's radius.
    fn relief(&self, az: f32, polar: f32) -> f32 {
        let a = az.abs();
        let front = bump(az, 0.75);
        let one = self.one_eyed();
        let brow = if self.folk == Folk::Lizardfolk { 0.0 } else if one { 0.17 } else if self.folk == Folk::Dwarf { 0.09 } else { 0.065 };
        // One heavy brow runs unbroken over the single eye, a little above it.
        let ridge_at = if one { self.eye_polar - 0.14 * PI } else { 0.43 * PI };
        let ridge = brow * bump(polar - ridge_at, if one { 0.055 * PI } else { 0.05 * PI }) * if one { bump(az, 0.95) } else { front };
        let socket = if one {
            -0.1 * bump(polar - self.eye_polar, 0.1 * PI) * bump(a, 0.45)
        } else {
            -0.06 * bump(polar - self.eye_polar, 0.055 * PI) * bump(a - self.eye_azimuth, 0.17)
        };
        let cheek = 0.03 * bump(polar - 0.575 * PI, 0.06 * PI) * bump(a - 0.75, 0.25);
        // The mouth's muzzle over the teeth and the point of the chin stand out of the flat
        // face, so the profile has a mouth and a chin and not only a nose.
        let flat = self.head.flat;
        let muzzle = 0.12 * flat * bump(polar - 0.69 * PI, 0.07 * PI) * bump(az, 0.45);
        let chin = 0.16 * self.head.chin * bump(polar - 0.8 * PI, 0.05 * PI) * bump(az, 0.32);
        ridge + socket + cheek + muzzle + chin
    }

    /// The skin at `az` (0 the face, a quarter turn the wearer's left) and `polar` (0 the
    /// crown), `off` further out.
    pub fn point(&self, az: f32, polar: f32, off: f32) -> V3 {
        self.head.point(az, polar, off + self.r * self.relief(az, polar))
    }

    pub fn normal(&self, az: f32, polar: f32) -> V3 {
        self.head.normal(az, polar)
    }

    /// Where an eye's centre sits, on side `sg` (+1 the wearer's left).
    pub fn eye(&self, sg: f32) -> V3 {
        if self.one_eyed() {
            return self.point(0.0, self.eye_polar, -0.01 * self.r);
        }
        self.point(sg * self.eye_azimuth, self.eye_polar, -0.01 * self.r)
    }

    /// The skull's skin as a closed surface.
    pub fn skull(&self) -> MeshBuilder {
        let (cols, rows) = (64, 44);
        // Rows climb from under the chin to the crown; columns run round from the back.
        let points: Vec<Vec<V3>> = (0..=rows)
            .map(|j| {
                let polar = PI * (1.0 - j as f32 / rows as f32);
                (0..cols).map(|i| self.point(-PI + TAU * i as f32 / cols as f32, polar, 0.0)).collect()
            })
            .collect();
        stitch_grid(&points, &GridOptions { wrap_around: true, ..Default::default() })
    }
}

/// Seeded nudges to a face beyond the spec's jaw, brow and nose: ear size, eye spacing,
/// mouth width.
struct Quirks {
    ear: f32,
    eyes: f32,
    mouth: f32,
    /// How far a one-eyed face's lid droops, 0..=1.
    droop: f32,
}

impl Quirks {
    fn roll(seed: u32) -> Quirks {
        let mut rng = Mulberry32::new(seed ^ 0x5eed_f00d);
        Quirks { ear: rng.range_f32(0.88, 1.12), eyes: rng.range_f32(0.94, 1.06), mouth: rng.range_f32(0.9, 1.1), droop: rng.range_f32(0.1, 0.9) }
    }
}

/// How a head's features sit under headgear (from [`crate::worn::fit_worn`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeadFit {
    /// The ears lie folded back flat against the head (under a hood, or flop ears under a hat).
    pub fold_ears: bool,
    /// A lizard's frill lies folded back flat along the crown (under any headgear).
    pub fold_frill: bool,
}

/// How far folded features stand off the head, in units of its radius: a folded ear's sheet
/// and rim, or the ridge of a folded frill. Headgear over them is sized up by this much.
const FOLDED_EARS: f32 = 0.13;
const FOLDED_FRILL: f32 = 0.2;

/// Collects a head's parts by look.
#[derive(Default)]
struct Pieces {
    skin: MeshBuilder,
    /// Skin a shade off (a pig's snout disc and ears).
    shade: MeshBuilder,
    whites: MeshBuilder,
    irises: MeshBuilder,
    pupils: MeshBuilder,
    mouth: MeshBuilder,
    ivory: MeshBuilder,
    frill: MeshBuilder,
    ears: MeshBuilder,
    horn: MeshBuilder,
    horn_tip: MeshBuilder,
    ear_in: MeshBuilder,
    mark: MeshBuilder,
    ring: MeshBuilder,
}

impl Body {
    /// The head's skin and relief, for placing features.
    pub fn face(&self) -> Face {
        let mut face = Face::new(self.head, self.folk());
        let q = Quirks::roll(self.spec.seed);
        face.eye_azimuth *= q.eyes;
        face
    }

    /// The head and face, features free: `"head"` (skin), `"head_shade"`, `"ears"`,
    /// `"eye_whites"`, `"irises"`, `"pupils"`, `"mouth"`, `"teeth"` (tusks, fangs, a lizard's
    /// teeth) and a lizard's `"frill"`, in body space.
    pub fn head_mesh(&self) -> ItemMesh {
        self.head_mesh_fitted(HeadFit::default())
    }

    /// How far this head's features stand off it when folded as `fit` folds them, for sizing
    /// headgear over them.
    pub fn feature_volume(&self, fit: HeadFit) -> f32 {
        let r = self.face().r;
        let ears = if fit.fold_ears && self.race().ears != Ears::None { FOLDED_EARS } else { 0.0 };
        let frill = if fit.fold_frill && self.folk() == Folk::Lizardfolk { FOLDED_FRILL } else { 0.0 };
        ears.max(frill) * r
    }

    /// [`Body::head_mesh`] with its features folded under headgear as `fit` says.
    pub fn head_mesh_fitted(&self, fit: HeadFit) -> ItemMesh {
        let face = self.face();
        let q = Quirks::roll(self.spec.seed);
        let race = self.race();
        let spec = &self.spec;
        let mut p = Pieces::default();
        p.skin.append(&face.skull());
        eyes(&face, race.eyes, q.droop, spec.woman, &mut p);
        let nv = 0.85 + spec.nose * 0.35;
        nose(&face, race.nose, nv, &mut p);
        if let (Some(row), Nose::Muzzle) = (&race.muzzle, race.nose) {
            muzzle(&face, row, &self.traits(), &mut p);
            if race.coat && self.traits().marking == crate::parts::Marking::Blaze {
                blaze(&face, &mut p);
            }
        }
        if !matches!(race.nose, Nose::Snout | Nose::Muzzle) {
            mouth(&face, (spec.brow - 0.5) * 0.06, q.mouth, spec.woman, &mut p);
        }
        for sg in [1.0f32, -1.0] {
            ear(&face, race.ears, sg, self.ear_size(q.ear), fit.fold_ears, &mut p);
        }
        self.horns_into(&face, &mut p);
        if self.folk() == Folk::Lizardfolk {
            if fit.fold_frill { folded_frill(&face, &mut p) } else { frill(&face, &mut p) }
        }
        let skin = spec.skin;
        let mut parts: Vec<Part> = vec![];
        let mut add = |m: MeshBuilder, name: &'static str, material: Option<MaterialKind>, tint: Rgb, outline: bool| {
            if m.is_empty() {
                return;
            }
            let mut part = m.into_part(name, OutlineMode::Welded).with_tint(tint);
            part.material = material;
            if !outline {
                part = part.without_outline();
            }
            parts.push(part);
        };
        add(p.skin, "head", Some(MaterialKind::Skin), skin, true);
        let shade = if self.folk() == Folk::Pigfolk { lighten(skin, 0.08) } else { skin.scaled(0.92) };
        add(p.shade, "head_shade", Some(MaterialKind::Skin), shade, true);
        add(p.ears, "ears", Some(MaterialKind::Skin), if race.ears == Ears::Flop { shade } else { skin }, true);
        add(p.ear_in, "ear_hollows", Some(MaterialKind::Skin), crate::parts::mix(skin, Rgb::hex(0xd9b0a0), 0.55), false);
        let tr = self.traits();
        let white = match race.eyes {
            EyeKind::Bovine => crate::parts::mix(Rgb::hex(0xdca02e), Rgb::hex(0xc4281a), tr.eyes),
            EyeKind::Yellow => Rgb::hex(0xe3c83a),
            EyeKind::Amber => Rgb::hex(0xd8962a),
            EyeKind::Small => Rgb::hex(0x160e0b),
            EyeKind::Human | EyeKind::One => EYE_WHITE,
        };
        add(p.whites, "eye_whites", None, white, !matches!(race.eyes, EyeKind::Human | EyeKind::One));
        add(p.irises, "irises", None, spec.eyes, false);
        add(p.pupils, "pupils", None, PUPIL, false);
        add(p.mouth, "mouth", None, MOUTH, false);
        add(p.ivory, "teeth", Some(MaterialKind::Bone), IVORY, true);
        add(p.frill, "frill", Some(MaterialKind::Skin), spec.hair_colour, true);
        add(p.horn, "horns", Some(MaterialKind::Bone), Rgb::hex(0xcfc4a6), true);
        add(p.horn_tip, "horn_tips", Some(MaterialKind::Bone), Rgb::hex(0x2b241d), true);
        add(p.mark, "markings", Some(MaterialKind::Skin), crate::coat::WHITE, false);
        if let Some(metal) = tr.ring.filter(|_| race.muzzle.is_some()) {
            let (material, tint) = crate::parts::Traits::ring_colour(metal);
            add(p.ring, "ring", Some(material), tint, true);
        }
        ItemMesh { parts }
    }
}

/// The features that stand out of a head under headgear ([`Body::standing_features`]), each
/// as a cloud of points over its surface in body space.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StandingFeatures {
    /// Each standing ear (not folded).
    pub ears: Vec<Vec<V3>>,
    /// Each horn (with its boss), which comes out through headgear.
    pub horns: Vec<Vec<V3>>,
    /// A lizard's or a bull's face: its snout with its jaw and teeth, its eyes and the ridges over them.
    pub face: Option<Vec<V3>>,
}

impl Body {
    /// The features that stand out of the head under headgear fitted as `fit` (each as its
    /// vertices and the middles of its triangles and their edges), so headgear can be cut
    /// open where they pass through it (our own; the plate draws features through
    /// headgear).
    pub fn standing_features(&self, fit: HeadFit) -> StandingFeatures {
        let face = self.face();
        let race = self.race();
        let q = Quirks::roll(self.spec.seed);
        let mut out = StandingFeatures::default();
        if !fit.fold_ears {
            for sg in [1.0f32, -1.0] {
                let mut p = Pieces::default();
                ear(&face, race.ears, sg, self.ear_size(q.ear), false, &mut p);
                if !p.ears.is_empty() {
                    out.ears.push(surface_points(&p.ears));
                }
            }
        }
        for sg in [1.0f32, -1.0] {
            let mut p = Pieces::default();
            self.horn_into(&face, sg, &mut p);
            let tip = p.horn_tip.clone();
            p.horn.append(&tip);
            if !p.horn.is_empty() {
                out.horns.push(surface_points(&p.horn));
            }
        }
        if let (Some(row), Nose::Muzzle) = (&race.muzzle, race.nose) {
            let mut p = Pieces::default();
            muzzle(&face, row, &self.traits(), &mut p);
            eyes(&face, race.eyes, q.droop, self.spec.woman, &mut p);
            for m in [p.shade, p.mouth, p.whites, p.pupils] {
                p.skin.append(&m);
            }
            out.face = Some(surface_points(&p.skin));
        }
        if self.folk() == Folk::Lizardfolk {
            let mut p = Pieces::default();
            snout(&face, &mut p);
            eyes(&face, race.eyes, q.droop, self.spec.woman, &mut p);
            for m in [p.mouth, p.ivory, p.whites, p.pupils] {
                p.skin.append(&m);
            }
            out.face = Some(surface_points(&p.skin));
        }
        out
    }
}

/// A mesh's vertices and the middles of its triangles and of their edges.
fn surface_points(m: &MeshBuilder) -> Vec<V3> {
    let mut out = m.positions.clone();
    for t in m.indices.chunks(3) {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| m.positions[i as usize]);
        out.push(vec3::scale(vec3::add(vec3::add(a, b), c), 1.0 / 3.0));
        out.extend([(a, b), (b, c), (c, a)].map(|(x, y)| vec3::scale(vec3::add(x, y), 0.5)));
    }
    out
}

/// An ellipsoid of half extents `size`, set at `at` with its +Y toward `up` and +Z toward
/// `facing`.
fn blob(size: V3, at: V3, up: V3, facing: V3, segments: usize) -> MeshBuilder {
    let mut m = uv_sphere(1.0, segments, (segments * 2 / 3).max(6));
    m.scale(size).place_basis(at, up, facing);
    m
}

fn eyes(face: &Face, kind: EyeKind, droop: f32, woman: bool, p: &mut Pieces) {
    if kind == EyeKind::One {
        one_eye(face, droop, p);
        return;
    }
    // A woman's eyes are drawn a tenth larger.
    let r = face.r * if woman { 1.1 } else { 1.0 };
    for sg in [1.0f32, -1.0] {
        let at = face.eye(sg);
        let n = face.normal(sg * face.eye_azimuth, face.eye_polar);
        let up = vec3::Y;
        let out = |d: f32| vec3::add(at, vec3::scale(n, d * r));
        match kind {
            EyeKind::Human => {
                p.whites.append(&blob([0.16 * r, 0.1 * r, 0.075 * r], at, up, n, 18));
                p.irises.append(&blob([0.075 * r, 0.075 * r, 0.04 * r], out(0.05), up, n, 14));
                p.pupils.append(&blob([0.036 * r, 0.036 * r, 0.02 * r], out(0.08), up, n, 10));
                p.skin.append(&lid(at, n, sg, 0.16 * r, 0.1 * r, 0.026 * r));
            }
            EyeKind::Yellow => {
                p.whites.append(&blob([0.17 * r, 0.11 * r, 0.08 * r], at, up, n, 18));
                p.pupils.append(&blob([0.035 * r, 0.075 * r, 0.03 * r], out(0.06), up, n, 10));
                p.skin.append(&lid(at, n, sg, 0.17 * r, 0.11 * r, 0.03 * r));
            }
            EyeKind::Amber => {
                p.whites.append(&blob([0.2 * r, 0.16 * r, 0.12 * r], at, up, n, 18));
                p.pupils.append(&blob([0.025 * r, 0.13 * r, 0.03 * r], out(0.105), up, n, 10));
                let ridge_at = face.point(sg * face.eye_azimuth, face.eye_polar - 0.1 * PI, -0.05 * r);
                p.skin.append(&blob([0.24 * r, 0.09 * r, 0.16 * r], ridge_at, up, n, 14));
            }
            EyeKind::Small => {
                p.whites.append(&blob([0.07 * r, 0.07 * r, 0.05 * r], out(0.02), up, n, 12));
            }
            EyeKind::Bovine => {
                p.whites.append(&blob([0.2 * r, 0.16 * r, 0.1 * r], at, up, n, 18));
                // A wide horizontal pupil, under a heavy brow.
                p.pupils.append(&blob([0.11 * r, 0.03 * r, 0.03 * r], out(0.085), up, n, 10));
                let across = vec3::normalize_or(vec3::cross(vec3::Y, n), vec3::X);
                let brow: Vec<V3> = (-3..=3)
                    .map(|k| {
                        let k = k as f32 / 3.0;
                        let o = vec3::add(vec3::add(at, vec3::scale(across, 0.26 * r * k)), vec3::scale(n, 0.02 * r));
                        vec3::add(o, [0.0, (0.15 - 0.05 * k * k) * r, 0.0])
                    })
                    .collect();
                p.skin.append(&tube(&brow, 8, &SweepOptions::default(), |t| 0.07 * r * (1.0 - 0.4 * (t - 0.5).abs())));
            }
            EyeKind::One => {}
        }
    }
}

/// A cyclops's one large eye, centred under its brow: a white, a seeded iris and a round
/// pupil, with a heavy upper lid that droops by `droop` (0..=1) and a lower rim.
fn one_eye(face: &Face, droop: f32, p: &mut Pieces) {
    let r = face.r;
    let at = face.eye(1.0);
    let n = face.normal(0.0, face.eye_polar);
    let up = vec3::Y;
    let out = |d: f32| vec3::add(at, vec3::scale(n, d * r));
    // About 2.7 times a human eye's width, and nearly round: the face's one feature.
    let (w, h) = (0.44 * r, 0.34 * r);
    p.whites.append(&blob([w, h, 0.2 * r], at, up, n, 26));
    p.irises.append(&blob([0.25 * r, 0.25 * r, 0.07 * r], out(0.15), up, n, 20));
    p.pupils.append(&blob([0.115 * r, 0.115 * r, 0.04 * r], out(0.205), up, n, 14));
    // The lid comes down over the top of the iris as it droops.
    p.skin.append(&lid(at, n, 1.0, w, h * (1.0 - 0.45 * droop), 0.075 * r * (1.0 + 0.6 * droop)));
    // A low rim under the eye, as a pouch.
    let across = vec3::normalize_or(vec3::cross(vec3::Y, n), vec3::X);
    let pts: Vec<V3> = (0..=10)
        .map(|i| {
            let a = PI * (1.1 + 0.8 * i as f32 / 10.0);
            let (x, y) = (a.cos() * w * 1.02, a.sin() * h * 0.95 - h * 0.05);
            vec3::add(at, vec3::add(vec3::add(vec3::scale(across, x), vec3::scale(vec3::cross(n, across), y)), vec3::scale(n, w * 0.34 * (1.0 - (x / w).powi(2)).max(0.0).sqrt())))
        })
        .collect();
    p.skin.append(&tube(&pts, 8, &SweepOptions::default(), |t| 0.04 * r * (0.5 + 0.5 * (PI * t).sin())));
}

/// An upper eyelid: a fold of skin along the top of an eye `w` wide and `h` tall, drooping a
/// little toward the outer corner.
fn lid(at: V3, n: V3, sg: f32, w: f32, h: f32, thickness: f32) -> MeshBuilder {
    let across = vec3::normalize_or(vec3::cross(vec3::Y, n), vec3::X);
    let up = vec3::cross(n, across);
    let pts: Vec<V3> = (0..=10)
        .map(|i| {
            let a = PI * (0.08 + 0.84 * i as f32 / 10.0);
            let (x, y) = (a.cos() * w * 1.02, a.sin() * h * 0.92 + h * 0.08);
            // The outer corner (toward the side of the head) sits a touch lower.
            let droop = if x * sg < 0.0 { 0.0 } else { -0.15 * h * (x / w).abs() };
            vec3::add(at, vec3::add(vec3::add(vec3::scale(across, x), vec3::scale(up, y + droop)), vec3::scale(n, w * 0.42 * (1.0 - (x / w).powi(2)).max(0.0).sqrt())))
        })
        .collect();
    tube(&pts, 8, &SweepOptions::default(), |t| thickness * (0.55 + 0.45 * (PI * t).sin()))
}

fn nose(face: &Face, kind: Nose, nv: f32, p: &mut Pieces) {
    let r = face.r;
    let hp = |az: f32, polar: f32, off: f32| face.point(az, polar, off);
    match kind {
        Nose::Human | Nose::Broad => {
            // A one-eyed face's nose is broader still, and low and flat under the eye.
            let k = if face.one_eyed() { 1.75 } else if kind == Nose::Broad { 1.35 } else { 1.0 };
            // Under a single eye the bridge starts lower, below its lower rim.
            let top = if face.one_eyed() { 0.545 } else { 0.45 };
            let path = CatmullRom::centripetal(vec![
                hp(0.0, top * PI, -0.03 * r),
                hp(0.0, if face.one_eyed() { (top + (0.635 - top) * 0.43) * PI } else { 0.53 * PI }, 0.08 * r * nv),
                hp(0.0, if face.one_eyed() { (top + (0.635 - top) * 0.81) * PI } else { 0.6 * PI }, 0.18 * r * nv),
                hp(0.0, 0.635 * PI, 0.05 * r),
            ])
            .sample_evenly(20);
            p.skin.append(&tube(&path, 14, &SweepOptions::default(), |t| {
                r * k * if t < 0.7 { 0.04 + (0.1 * nv - 0.04) * t / 0.7 } else { 0.1 * nv + (0.06 - 0.1 * nv) * (t - 0.7) / 0.3 }
            }));
            for sg in [1.0f32, -1.0] {
                let at = hp(sg * 0.09 * k, 0.61 * PI, 0.03 * r);
                p.skin.append(&blob([0.065 * r * k, 0.055 * r * k, 0.055 * r * k], at, vec3::Y, face.normal(sg * 0.3, 0.6 * PI), 12));
                // Nostril: a dark dent under each wing.
                let under = vec3::add(at, [sg * -0.02 * r, -0.05 * r * k, 0.02 * r]);
                p.mouth.append(&blob([0.03 * r * k, 0.014 * r, 0.03 * r * k], under, vec3::Y, vec3::Z, 8));
            }
        }
        Nose::Bulb => {
            let path = vec![hp(0.0, 0.46 * PI, -0.02 * r), hp(0.0, 0.54 * PI, 0.12 * r)];
            p.skin.append(&tube(&path, 12, &SweepOptions::default(), |_| 0.08 * r));
            let at = hp(0.0, 0.585 * PI, 0.12 * r);
            p.skin.append(&blob([0.26 * r * nv, 0.24 * r * nv, 0.24 * r * nv], at, vec3::Y, vec3::Z, 18));
        }
        Nose::Long => {
            let base = hp(0.0, 0.5 * PI, 0.0);
            let tip = vec3::add(hp(0.0, 0.6 * PI, 0.0), [0.0, -0.15 * r, 0.62 * r * nv]);
            let path = CatmullRom::centripetal(vec![base, hp(0.0, 0.55 * PI, 0.2 * r), tip]).sample_evenly(18);
            p.skin.append(&tube(&path, 12, &SweepOptions::default(), |t| r * (0.14 * (1.0 - t) + 0.025) * (1.0 - (t - 0.92).max(0.0) * 6.0)));
        }
        Nose::Pig => {
            let at = hp(0.0, 0.58 * PI, -0.06 * r);
            let n = vec3::normalize_or(vec3::add(face.normal(0.0, 0.58 * PI), [0.0, -0.15, 0.0]), vec3::Z);
            let mut disc = lathe(
                &[[0.36 * r, 0.0], [0.39 * r, 0.2 * r], [0.38 * r, 0.36 * r], [0.34 * r, 0.41 * r], [0.002, 0.42 * r]],
                &LatheOptions::new(28, Shading::SMOOTH),
            );
            disc.place_basis(at, n, vec3::Y);
            p.shade.append(&disc);
            let across = vec3::normalize_or(vec3::cross(vec3::Y, n), vec3::X);
            let up = vec3::cross(n, across);
            for sg in [1.0f32, -1.0] {
                let c = vec3::add(at, vec3::add(vec3::scale(n, 0.415 * r), vec3::scale(across, sg * 0.13 * r)));
                p.mouth.append(&blob([0.06 * r, 0.09 * r, 0.02 * r], c, up, n, 10));
            }
        }
        Nose::Snout => snout(face, p),
        Nose::Muzzle => {}
    }
}

/// A reptile's snout: a long flattened cone from the face, nostrils near its tip, the jaw
/// line along each side with a row of teeth under it.
fn snout(face: &Face, p: &mut Pieces) {
    let r = face.r;
    let at = face.point(0.0, 0.6 * PI, -0.42 * r);
    let fwd = vec3::normalize_or([0.0, -0.12, 1.0], vec3::Z);
    let profile = [[0.6 * r, 0.0], [0.6 * r, 0.35 * r], [0.5 * r, 0.8 * r], [0.32 * r, 1.2 * r], [0.15 * r, 1.31 * r], [0.002, 1.34 * r]];
    let radius_at = |along: f32| armory::geom::profile_radius_at(&profile, along, false).unwrap_or(0.0);
    let mut sn = lathe(&profile, &LatheOptions::new(32, Shading::SMOOTH));
    // Flatter top to bottom than across: the lathe's z becomes up once placed.
    sn.scale([1.0, 1.0, 0.72]);
    let up = vec3::normalize_or(vec3::sub(vec3::Y, vec3::scale(fwd, fwd[1])), vec3::Y);
    let across = vec3::cross(up, fwd);
    let local = |x: f32, along: f32, z: f32| vec3::add(at, vec3::add(vec3::add(vec3::scale(across, x), vec3::scale(fwd, along)), vec3::scale(up, z)));
    // Lathe space: y along the snout, z up. Map it with columns (across, fwd, up).
    sn.transform(&armory::geom::Mat3::from_cols(across, fwd, up)).translate(at);
    p.skin.append(&sn);
    for sg in [1.0f32, -1.0] {
        p.mouth.append(&blob([0.045 * r, 0.03 * r, 0.035 * r], local(sg * 0.13 * r, 1.16 * r, 0.17 * r), up, fwd, 8));
        // The jaw line, a little below the snout's widest, and teeth hanging from it.
        let jaw: Vec<V3> = (0..=12)
            .map(|i| {
                let t = i as f32 / 12.0;
                let along = 0.2 * r + 1.0 * r * t;
                // On the snout's flank, a little below its widest.
                let half = radius_at(along) * 1.01;
                local(sg * half, along, -0.12 * half)
            })
            .collect();
        p.mouth.append(&tube(&jaw, 6, &SweepOptions::default(), |_| 0.022 * r));
        for (i, &pt) in jaw.iter().enumerate().skip(2).step_by(2) {
            let mut tooth = spike(0.035 * r, (0.09 - 0.003 * i as f32) * r, 6, 0.4, Shading::SMOOTH);
            tooth.place(vec3::add(pt, vec3::scale(up, 0.01 * r)), vec3::normalize_or(vec3::add(vec3::neg(up), vec3::scale(across, sg * 0.15)), vec3::neg(vec3::Y)));
            p.ivory.append(&tooth);
        }
    }
}

/// The mouth as a dark line across the face, set by `smile`, with a lower lip under it; goblins
/// show fangs and pigfolk tusks.
fn mouth(face: &Face, smile: f32, width: f32, woman: bool, p: &mut Pieces) {
    let r = face.r;
    let folk = face.folk;
    let mw = width * match folk {
        Folk::Cyclops => 0.42,
        Folk::Goblin => 0.55,
        Folk::Pigfolk => 0.36,
        _ => 0.3,
    };
    let line: Vec<V3> = (0..=8)
        .map(|i| {
            let th = (i as f32 / 8.0 * 2.0 - 1.0) * mw;
            face.point(th, (0.69 + smile * (1.0 - (th / mw).powi(2))) * PI, 0.012 * r)
        })
        .collect();
    let path = CatmullRom::centripetal(line).sample_evenly(24);
    p.mouth.append(&tube(&path, 6, &SweepOptions::default(), |t| 0.026 * r * (0.6 + 0.4 * (PI * t).sin())));
    if folk != Folk::Pigfolk {
        let lip = face.point(0.0, 0.725 * PI, -0.012 * r);
        p.skin.append(&blob([mw * r * 0.5, 0.035 * r * if woman { 1.5 } else { 1.0 }, 0.03 * r], lip, vec3::Y, face.normal(0.0, 0.72 * PI), 14));
    }
    match folk {
        Folk::Goblin => {
            for sg in [1.0f32, -1.0] {
                let mut fang = spike(0.05 * r, 0.16 * r, 8, 0.3, Shading::SMOOTH);
                fang.place(face.point(sg * 0.22, 0.705 * PI, 0.0), vec3::normalize_or([0.0, 1.0, 0.25], vec3::Y));
                p.ivory.append(&fang);
            }
        }
        Folk::Pigfolk => {
            for sg in [1.0f32, -1.0] {
                let b = face.point(sg * 0.32, 0.7 * PI, 0.0);
                let path = CatmullRom::centripetal(vec![b, vec3::add(b, [sg * 0.08 * r, 0.22 * r, 0.06 * r]), vec3::add(b, [sg * 0.16 * r, 0.38 * r, 0.02 * r])]).sample_evenly(12);
                p.ivory.append(&tube(&path, 8, &SweepOptions::default(), |t| r * (0.065 * (1.0 - t) + 0.012)));
            }
        }
        _ => {}
    }
}

/// An ear as a cupped sheet with a rolled rim, `width` across and `length` long, its outline
/// sharpened toward the tip by `point` (0 round, 1 a blade), built flat in its own space: x
/// across, y along toward the tip, the bowl opening toward +z. `fold` curls the tip toward -z.
fn ear_sheet(width: f32, length: f32, point: f32, cup: f32, fold: f32, thickness: f32, place: impl Fn(V3) -> V3) -> MeshBuilder {
    let outline = |th: f32| -> [f32; 2] {
        let (s, c) = th.sin_cos();
        let up = s.max(0.0);
        let x = width * c * (1.0 - point * 0.75 * up.powf(1.5));
        let y = length * 0.5 * s * (1.0 + point * up);
        [x, y]
    };
    let lift = |x: f32, y: f32, rho: f32| -> V3 {
        let tip = (y / length).max(0.0);
        [x, y, cup * (rho * rho - 1.0) - fold * tip * tip * length]
    };
    let mut mesh = patch(36, 8, &ShellOptions { wrap_around: true, ..ShellOptions::sheet(thickness) }, |u, v| {
        let [x, y] = outline(-TAU * u);
        place(lift(x * v, y * v, v))
    });
    let rim: Vec<V3> = (0..48)
        .map(|i| {
            let [x, y] = outline(TAU * i as f32 / 48.0);
            place(vec3::add(lift(x, y, 1.0), [0.0, 0.0, thickness * 0.5]))
        })
        .collect();
    mesh.append(&rolled_edge(&rim, thickness * 0.9, true));
    mesh
}

/// An ear on side `sg`. Folded, it lies back flat along the side of the head from its root
/// (its length running back round the head and a little down, its width up and down), as an
/// ear does under a hood; our own, the plate has no headgear over ears.
fn ear(face: &Face, kind: Ears, sg: f32, size: f32, folded: bool, p: &mut Pieces) {
    let r = face.r;
    // Root (azimuth and polar angle), and the sheet: width, length, point, cup, fold, thickness
    // (all in head radii) and how far its root is set in along its length.
    let (az, polar, w, l, point, cup, fold, t, set) = match kind {
        Ears::None => return,
        Ears::Leaf => return leaf_ear(face, sg, size, folded, p),
        Ears::Round => (sg * 1.57, 0.53 * PI, 0.17, 0.56, 0.0, 0.05, 0.0, 0.04, 0.0),
        Ears::Pointed => (sg * 1.55, 0.52 * PI, 0.2, 0.62, 0.85, 0.05, 0.0, 0.04, 0.4),
        Ears::Long => (sg * 1.55, 0.52 * PI, 0.2, 1.15, 0.85, 0.05, 0.0, 0.04, 0.4),
        Ears::Flop => (sg * 1.05, 0.24 * PI, 0.34, 0.62 * 1.15, 0.6, 0.05, 0.55, 0.045, 0.4 / 1.15),
    };
    let (w, l) = (w * r * size, l * r * size);
    let shift = move |q: V3| [q[0], q[1] + l * set, q[2]];
    if folded {
        // Laid back round the head: length along the azimuth away from the face (shortened a
        // little, as a folded ear doubles on itself), width along the meridian, the sheet's
        // shallow cup off the skin.
        let lat = (r * polar.sin()).max(0.3 * r);
        let lay = |q: V3| {
            let q = shift(q);
            let along = q[1] * 0.7;
            face.point(az + sg * along / lat, polar + q[0] * sg / r + 0.25 * along / r, 0.03 * r + (q[2] + 0.02 * r).max(0.0))
        };
        p.ears.append(&ear_sheet(w, l, point, 0.02 * r, 0.0, t * r, lay));
        return;
    }
    let mut m = ear_sheet(w, l, point, cup * r, fold * l, t * r, shift);
    match kind {
        Ears::Round => {
            let root = face.point(az, polar, -0.05 * r);
            m.place_basis(root, vec3::normalize_or([0.0, 1.0, -0.18], vec3::Y), vec3::normalize_or([sg, 0.0, 0.2], vec3::X));
        }
        Ears::Pointed | Ears::Long => {
            let dir = vec3::normalize_or(if kind == Ears::Long { [sg * 0.9, 0.32, -0.25] } else { [sg * 0.65, 0.62, -0.22] }, vec3::Y);
            let root = face.point(az, polar, -0.08 * r);
            // The bowl faces forward and out; the sheet's root sits on the head.
            let out = vec3::normalize_or(vec3::sub([sg * 0.3, 0.3, 1.0], vec3::scale(dir, vec3::dot([sg * 0.3, 0.3, 1.0], dir))), vec3::Z);
            m.place_basis(root, dir, out);
        }
        _ => {
            let root = face.point(az, polar, -0.06 * r);
            let dir = vec3::normalize_or([sg * 0.5, -0.18, 0.82], vec3::Z);
            // The ear's inside faces down toward the face as it folds forward.
            let inside = vec3::normalize_or([-sg * 0.6, -1.0, 0.0], vec3::neg(vec3::Y));
            m.place_basis(root, dir, vec3::sub(vec3::neg(inside), vec3::scale(dir, vec3::dot(vec3::neg(inside), dir))));
        }
    }
    p.ears.append(&m);
}

/// A lizard's frill folded back flat: each spine laid along the crown toward the nape from
/// its root, a low ridge from brow to nape.
fn folded_frill(face: &Face, p: &mut Pieces) {
    let r = face.r;
    // Along the midline: negative toward the brow, positive over the crown to the nape.
    let at = |a: f32, off: f32| {
        let a = a.min(0.9);
        let (az, polar) = if a < 0.0 { (0.0, -a * PI) } else { (PI, a * PI) };
        face.point(az, polar.max(0.001), off)
    };
    for i in 0..7 {
        let a0 = -0.15 + i as f32 * 0.17;
        let length = (0.58 - 0.06 * (i as f32 - 2.5).abs()) * r * 0.85;
        let span = length / (PI * r);
        let path: Vec<V3> = (0..=10)
            .map(|k| {
                let t = k as f32 / 10.0;
                at(a0 + span * t, 0.02 * r + 0.08 * r * (1.0 - t))
            })
            .collect();
        p.frill.append(&tube(&path, 8, &SweepOptions::default(), |t| 0.08 * r * (1.0 - 0.85 * t)));
    }
}

/// A lizard's frill: spines along the top of the head from the brow over the crown to the
/// nape, joined by a membrane.
fn frill(face: &Face, p: &mut Pieces) {
    let r = face.r;
    let n = 7;
    let mut tips = vec![];
    let mut roots = vec![];
    for i in 0..n {
        let a = -0.15 + i as f32 * 0.17;
        let (az, polar) = if a < 0.0 { (0.0, -a * PI) } else { (PI, a * PI) };
        let root = face.point(az, polar.max(0.001), -0.04 * r);
        let dir = vec3::normalize_or(vec3::add(vec3::normalize_or(vec3::sub(root, face.head.centre), vec3::Y), [0.0, 0.0, -0.5]), vec3::Y);
        let length = (0.58 - 0.06 * (i as f32 - 2.5).abs()) * r;
        let mut spine = spike(0.09 * r, length, 8, 0.3, Shading::SMOOTH);
        spine.place(root, dir);
        p.frill.append(&spine);
        roots.push(root);
        tips.push(vec3::add(root, vec3::scale(dir, length * 0.82)));
    }
    // The membrane: a thin sheet from the roots to just short of the tips, scalloped between.
    let rows: Vec<Vec<V3>> = (0..=6)
        .map(|j| {
            let v = j as f32 / 6.0;
            (0..=(n - 1) * 6)
                .map(|k| {
                    let f = k as f32 / 6.0;
                    let i = (f.floor() as usize).min(n - 2);
                    let t = f - i as f32;
                    let root = vec3::lerp(roots[i], roots[i + 1], t);
                    let tip = vec3::lerp(tips[i], tips[i + 1], t);
                    let sag = 1.0 - 0.45 * (PI * t).sin();
                    vec3::lerp(root, tip, v * sag)
                })
                .collect()
        })
        .collect();
    let mut sheet = stitch_grid(&rows, &GridOptions::default());
    sheet.make_double_sided();
    p.frill.append(&sheet);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::BodySpec;

    fn extents(body: &Body) -> [f32; 3] {
        let mesh = body.head_mesh();
        let c = body.head.centre;
        let r = body.face().r;
        let mut e = [0.0f32; 3];
        for part in &mesh.parts {
            for p in &part.positions {
                e[0] = e[0].max((p[0] - c[0]).abs() / r);
                e[1] = e[1].max((p[1] - c[1]) / r);
                e[2] = e[2].max((p[2] - c[2]) / r);
            }
        }
        e
    }

    #[test]
    fn every_head_part_validates() {
        for folk in Folk::fitted() {
            for seed in [1, 1207] {
                let body = Body::new(&BodySpec::roll(folk, seed));
                let mesh = body.head_mesh();
                for part in &mesh.parts {
                    part.validate().unwrap_or_else(|e| panic!("{folk:?}: {e}"));
                }
                assert!(mesh.parts.iter().any(|p| p.name == "head"));
            }
        }
    }

    #[test]
    fn each_folk_has_a_head_of_its_own() {
        let heads: Vec<(Folk, [f32; 3])> = Folk::fitted().iter().map(|&f| (f, extents(&Body::new(&BodySpec::typical(f))))).collect();
        let get = |f: Folk| heads.iter().find(|h| h.0 == f).unwrap().1;
        let (human, goblin, gnome, lizard, pig) = (get(Folk::Human), get(Folk::Goblin), get(Folk::Gnome), get(Folk::Lizardfolk), get(Folk::Pigfolk));
        // A lizard's snout and a goblin's nose reach far forward; a pig's disc further than a human's nose.
        assert!(lizard[2] > human[2] + 0.6, "{lizard:?} {human:?}");
        assert!(goblin[2] > human[2] + 0.3, "{goblin:?}");
        assert!(pig[2] > human[2] + 0.06, "{pig:?} {human:?}");
        // Goblin ears stand far out to the sides, gnome ears less so.
        assert!(goblin[0] > human[0] + 0.6 && goblin[0] > gnome[0] + 0.2, "{goblin:?} {gnome:?}");
        assert!(gnome[0] > human[0] + 0.1, "{gnome:?} {human:?}");
        // A lizard's frill rises over the crown.
        assert!(lizard[1] > human[1] + 0.05, "{lizard:?} {human:?}");
        // No two folk share a silhouette.
        for (i, a) in heads.iter().enumerate() {
            for b in &heads[i + 1..] {
                let d = (0..3).map(|k| (a.1[k] - b.1[k]).abs()).fold(0.0, f32::max);
                assert!(d > 0.04, "{:?} and {:?} look alike: {:?} {:?}", a.0, b.0, a.1, b.1);
            }
        }
    }

    #[test]
    fn faces_are_seeded() {
        for folk in Folk::fitted() {
            let a = Body::new(&BodySpec::roll(folk, 5)).head_mesh();
            assert_eq!(a, Body::new(&BodySpec::roll(folk, 5)).head_mesh());
            assert_ne!(a, Body::new(&BodySpec::roll(folk, 6)).head_mesh(), "{folk:?}");
        }
    }

    #[test]
    fn eyes_sit_in_the_face() {
        for folk in Folk::fitted() {
            let body = Body::new(&BodySpec::roll(folk, 9));
            let face = body.face();
            let c = body.head.centre;
            for sg in [1.0f32, -1.0] {
                let eye = face.eye(sg);
                // On the skin, in front of the head's centre, between chin and crown, on its side.
                let one = face.one_eyed();
                let skin = face.point(if one { 0.0 } else { sg * face.eye_azimuth }, face.eye_polar, 0.0);
                assert!(vec3::distance(eye, skin) < 0.05 * face.r);
                assert!(eye[2] > c[2], "{folk:?} eye {eye:?}");
                assert!(eye[1] > body.chin_y && eye[1] < body.height);
                // One eye sits on the middle; two sit one to a side.
                assert!(if one { eye[0].abs() < 0.01 * face.r } else { eye[0] * sg > 0.0 });
            }
            let whites = body.head_mesh().parts.into_iter().find(|p| p.name == "eye_whites").unwrap();
            for p in &whites.positions {
                assert!(vec3::distance(*p, c) < 1.4 * face.r, "{folk:?}: an eye floats off the face");
            }
        }
    }
}

impl Body {
    /// The size an ear is drawn at: a seeded nudge for most, the race row's range for the broad
    /// leaf ears of a bull.
    fn ear_size(&self, nudge: f32) -> f32 {
        match &self.race().muzzle {
            Some(row) if self.race().ears == Ears::Leaf => row.ears[0] + (row.ears[1] - row.ears[0]) * self.traits().ears,
            _ => nudge,
        }
    }

    /// Both horns, if the race has any.
    fn horns_into(&self, face: &Face, p: &mut Pieces) {
        for sg in [1.0f32, -1.0] {
            self.horn_into(face, sg, p);
        }
    }

    /// The horn on side `sg` (+1 the left): rooted on the skull's top behind the brow, with a boss.
    fn horn_into(&self, face: &Face, sg: f32, p: &mut Pieces) {
        let Some(row) = &self.race().horns else { return };
        let side = usize::from(sg > 0.0);
        let spec = self.traits().horn_spec(row, side, face.head.half_width);
        let root = face.point(sg * 1.6, (0.2 + 0.4 * row.root) * PI, -0.05 * face.r);
        let h = crate::parts::horn(root, sg, &spec);
        p.horn.append(&h.base);
        p.horn_tip.append(&h.tip);
        p.horn.append(&crate::parts::ball(root, 1.18 * spec.radius, [1.0, 0.8, 1.0]));
    }
}

/// A bull's ear: a broad leaf set out from under the horn, drooping and swept back; folded
/// under a hood it is a flat lobe lying on the side of the head.
fn leaf_ear(face: &Face, sg: f32, size: f32, folded: bool, p: &mut Pieces) {
    let (h, r) = (&face.head, face.r);
    if folded {
        let at = face.point(sg * 1.7, 0.47 * PI, 0.03 * r);
        p.ears.append(&blob([0.22 * r, 0.3 * r, 0.05 * r], at, vec3::Y, face.normal(sg * 1.7, 0.47 * PI), 12));
        return;
    }
    // A cupped leaf set below and behind the horn, out and drooping, its inside paler.
    let l = 0.85 * size * h.half_width;
    let w = 0.36 * l;
    let root = [h.centre[0] + sg * 0.86 * h.half_width, h.centre[1] + 0.18 * h.half_height, h.centre[2] - 0.3 * h.half_depth];
    let dir = vec3::normalize_or([sg * 1.0, -0.4, -0.45], vec3::X);
    let want = vec3::normalize_or([sg * 0.2, 0.25, 1.0], vec3::Z);
    let out = vec3::normalize_or(vec3::sub(want, vec3::scale(dir, vec3::dot(want, dir))), vec3::Z);
    let across = vec3::cross(dir, out);
    let place = |q: V3, lift: f32| vec3::add(root, vec3::add(vec3::scale(dir, q[1] + 0.5 * l), vec3::add(vec3::scale(across, q[0]), vec3::scale(out, q[2] + lift))));
    p.ears.append(&ear_sheet(w, l, 0.35, 0.22 * w, 0.18 * l, 0.05 * r, |q| place(q, 0.0)));
    p.ear_in.append(&ear_sheet(0.8 * w, 0.85 * l, 0.35, 0.22 * w, 0.18 * l, 0.02 * r, |q| place([q[0], q[1] + 0.05 * l, q[2]], 0.07 * r)));
}

/// A bull's muzzle: a long, deep, blunt snout lofted forward and a little down from the broad
/// skull, with a damp nose pad, flared nostrils, a heavy lower lip and, in four of ten, a ring
/// through the septum.
fn muzzle(face: &Face, row: &crate::folk::MuzzleRow, tr: &crate::parts::Traits, p: &mut Pieces) {
    use crate::parts::{Loft, ball, ring};
    let h = &face.head;
    let (hw, hh, hd, c) = (h.half_width, h.half_height, h.half_depth, h.centre);
    let total = row.length[0] + (row.length[1] - row.length[0]) * tr.muzzle;
    let l = (total - 1.1) * hd;
    let at = |k: &[(f32, f32)], s: f32| -> Vec<(f32, f32)> { k.iter().map(|&(f, v)| (f * l, v * s)).collect() };
    let loft = Loft::new(
        [0.0, c[1] - 0.2 * hh, c[2] + 0.1 * hd],
        l,
        vec![(0.0, FRAC_PI_2 + 0.3)],
        at(&[(0.0, 0.8), (0.2, 0.75), (0.55, 0.6), (0.88, 0.56), (1.0, 0.47)], hw),
        at(&[(0.0, 0.62), (0.5, 0.5), (0.9, 0.46), (1.0, 0.36)], hh),
        at(&[(0.0, 0.7), (0.5, 0.5), (0.9, 0.46), (1.0, 0.38)], hh),
        2.3,
    );
    p.skin.append(&loft.mesh(0.0, l, 20, 36));
    let tip = loft.centre(l);
    p.shade.append(&ball(vec3::add(tip, [0.0, 0.02 * hh, -0.08 * hd]), 1.0, [0.54 * hw, 0.42 * hh, 0.4 * hd]));
    p.skin.append(&ball(vec3::add(loft.centre(0.8 * l), [0.0, -0.3 * hh, 0.0]), 1.0, [0.6 * hw, 0.2 * hh, 0.6 * hd]));
    for sg in [1.0f32, -1.0] {
        let nostril = vec3::add(tip, [sg * 0.25 * hw, 0.04 * hh, 0.26 * hd]);
        let mut hole = uv_sphere(1.0, 12, 8);
        hole.scale([0.13 * hw, 0.1 * hh, 0.08 * hd]).rotate_z(-sg * 0.5).translate(nostril);
        p.mouth.append(&hole);
        // The mouth's line along the muzzle's side.
        let line: Vec<V3> = (2..=9)
            .map(|i| {
                let a = l * i as f32 / 10.0;
                let q = loft.point(a, if sg > 0.0 { 0.0 } else { PI }, 0.01 * hh);
                [q[0], q[1] - 0.14 * hh, q[2]]
            })
            .collect();
        p.mouth.append(&tube(&line, 6, &SweepOptions::default(), |_| 0.03 * hh));
    }
    if tr.ring.is_some() {
        p.ring.append(&ring([tip[0], tip[1] - 0.1 * hh, tip[2] + 0.2 * hd], vec3::Z, vec3::neg(vec3::Y), 0.42 * hh, 0.06 * hh));
    }
}

/// A blaze: a white stripe down the face onto the muzzle.
fn blaze(face: &Face, p: &mut Pieces) {
    let r = face.r;
    let pts: Vec<V3> = (0..=8).map(|i| face.point(0.0, (0.05 + 0.4 * i as f32 / 8.0) * PI, 0.012 * r)).collect();
    p.mark.append(&tube(&pts, 10, &SweepOptions::default(), |t| (0.2 - 0.07 * t) * r));
}
