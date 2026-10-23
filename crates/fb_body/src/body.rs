//! [`Body`]: a folk's proportions from its race row and its [`BodySpec`] (the plate
//! prototype's `bodyDims`), the Armory forms sized to it, the anchors gear hangs from, and
//! its skin.
//!
//! Body space: feet on y = 0, the body centred on the Y axis, the face toward +Z, the
//! wearer's left toward +X, 1 unit = 10 cm. The Armory's torso and head forms use the
//! torso's space instead, the same axes with the waist at y = 0
//! ([`Body::torso_placement`] moves them onto the body).

use std::f32::consts::{PI, TAU};

use armory::build::boot::foot_mesh;
use armory::build::form::{BACK, Bulge, FRONT, FormRing, HandForm, HeadForm, FILL_ROWS, LEG_SECTIONS, LegForm, LegSection, LowerBodyForm, TorsoForm, TorsoProfile};
use armory::build::hafted::smoothstep;
use armory::fabric::{PatternKind, SurfacePattern};
use armory::geom::vec3::{self, V3};
use armory::geom::{CatmullRom, GridOptions, MeshBuilder, OutlineMode, SweepOptions, spike, stitch_grid, tube, uv_sphere};
use armory::{ItemMesh, MaterialKind, Rgb};

use crate::folk::{Folk, Race};
use crate::hand::{HandPose, grip_centre, hand_meshes_with};
use crate::limb::{Limb, Side};
use crate::place::Placement;
use crate::spec::BodySpec;

/// The superellipse exponent of the trunk's sections: nearly an ellipse. The plate's `tSurf`
/// used 0.77, a boxy, carved section that read as a block.
pub const TRUNK_EXPONENT: f32 = 0.9;
/// The back's depth relative to the front (`tSurf`'s 0.92).
pub const TRUNK_BACK: f32 = 0.92;
/// How far (a share of the trunk's length) the pelvis closes creases with the thighs.
const RUN: f32 = 0.15;
/// The share of the front filled out to the thighs; more raises a pooch over the hip on a body whose thighs are as deep as its hips.
const FRONT_FILL: f32 = 0.4;
/// The hips' half width at the widest, in units of the girth: the hip bone (the top of the hip, a little above the hip joint). A human's ends up about 1.12 to 1.15 times the waist's at a typical figure.
const HIP_WIDTH: f32 = 1.56;
/// Where the widest is, above the hip joint, as a share of the trunk's length.
const ILIAC: f32 = 0.14;
/// The skin's width at the hip bone, as a share of the hip ring's (which the legs and the garment forms hang from): the visible hips are narrower than the ring.
const ILIAC_WIDTH: f32 = 0.94;
/// The skin's width at the hip joint, below the widest, as a share of it.
const HIP_JOINT: f32 = 0.88;
/// How far each hip joint is from the midline, as a share of the hips' half width.
const HIP_JOINT_X: f32 = 0.48;
/// How much deeper than wide a thigh is toward its top, as a share of its radius: the narrower hips left a thigh round, which stood the hips out in front of it.
const THIGH_DEEP: f32 = 0.2;
/// How much deeper still the back of a thigh is, as a share of its radius: the seat's weight hangs on the thigh's back, and with the hips narrower the thigh's own depth no longer carries it, so the seat overhangs the thigh.
const THIGH_BACK: f32 = 0.2;
/// How much thicker legs widen the hips (they stand out past the hips otherwise), per unit of the folk's limb thickness over a human's.
const HIP_LIMB: f32 = 0.5;
/// How far out the top of a thigh reaches, as a share of the hips' half width: inside the hip line, so the thigh never stands out past it.
const THIGH_TOP: f32 = 0.92;
/// How far below the hock a hoofed leg's cloth may hang, in the body's scale.
const HOCK_HEM: f32 = 0.3;
/// Angles sampled round a section to find how wide it is.
const SIDE_STEPS: usize = 48;
/// How far past the thighs the garment form's section over them reaches sideways, as a share of their reach.
const THIGH_WIDTH: f32 = 1.03;
/// How far below the hips (in the body's scale) the garment form's section over the thighs lies.
const THIGH_RING: f32 = 1.2;
/// How far below the hips (in the body's scale) the garment fill reaches, so it carries on down the thighs past the crotch.
const FILL_DEPTH: f32 = 1.8;
/// How far past the skin the garment form reaches over the pelvis, before a garment's own fit, as a share of the body's scale.
const GARMENT_MARGIN: f32 = 0.01;
/// Over how far (in the body's scale) the garment fill builds up from the thigh section, so no step shows at the crotch.
const FILL_ONSET: f32 = 0.2;
/// The same for the front, which reaches down to [`FILL_DEPTH`] and builds up quickly.
const FILL_ONSET_FRONT: f32 = 0.3;
/// Where the crotch is, below the hips, as a share of the trunk's length.
const CROTCH_DROP: f32 = 0.14;
/// The heights the pelvis is measured at when drawing the groin in.
const PUBIS_SAMPLES: usize = 24;
/// Heights the trunk is fitted to the thighs at, from end to end.
const FIT_SAMPLES: usize = 64;
/// How many closing runs the fitted heights reach beyond the waist and the crotch.
const FIT_MARGIN: f32 = 2.0;
/// Passes of the fit: the second adds what the first left short.
const FIT_PASSES: usize = 2;
/// Halvings of the search along a thigh for a height: finer than any mesh row.
const BISECTIONS: usize = 24;
/// Angles sampled either side of the front or the back of the trunk, to find how deep it reaches
/// (the seat has two lobes, not one midline): the count, the radians either side, and the step.
const SEAT_STEPS: usize = 12;
const SEAT_SPAN: f32 = 0.9;
const SEAT_STEP: f32 = 0.15;

/// How one arm hangs (the plate's `poseFor`): `spread` out from the body and `forward`
/// lean of the upper arm, `bend` of the forearm toward the front (0 straight down, 1
/// level and forward), and whether the hand closes on a grip of that radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArmPose {
    pub spread: f32,
    pub bend: f32,
    pub forward: f32,
    pub grip: Option<f32>,
}

impl ArmPose {
    /// Empty-handed, hanging at the side (`poseFor` with no item).
    pub const RELAXED: ArmPose = ArmPose { spread: 0.18, bend: 0.06, forward: 0.02, grip: None };

    /// The same arm with its hand closed on a grip of `radius`.
    pub fn holding(self, radius: f32) -> ArmPose {
        ArmPose { grip: Some(radius), ..self }
    }
}

/// Both arms' poses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub right: ArmPose,
    pub left: ArmPose,
}

impl Pose {
    pub const RELAXED: Pose = Pose { right: ArmPose::RELAXED, left: ArmPose::RELAXED };

    pub fn arm(&self, side: Side) -> ArmPose {
        match side {
            Side::Right => self.right,
            Side::Left => self.left,
        }
    }
}

impl Default for Pose {
    fn default() -> Self {
        Pose::RELAXED
    }
}

/// A folk's body, measured and posed.
#[derive(Clone, Debug)]
pub struct Body {
    pub spec: BodySpec,
    pub pose: Pose,
    /// Crown to soles.
    pub height: f32,
    /// Height over a reference human's 17.5 (`B.s`): lengths scale with it.
    pub scale: f32,
    /// Girth (`B.ws`): `scale^0.72 * race.girth * (0.88 + frame * 0.3)`.
    pub girth: f32,
    /// The head in body space (its features are built on it).
    pub head: HeadForm,
    pub chin_y: f32,
    pub shoulder_y: f32,
    pub chest_y: f32,
    pub waist_y: f32,
    pub hip_y: f32,
    pub ankle_y: f32,
    /// Hips to shoulders (`B.T`).
    pub trunk_length: f32,
    /// Half the shoulders' span (`B.sw`).
    pub shoulder_half_width: f32,
    pub chest: FormRing,
    pub waist: FormRing,
    pub hip: FormRing,
    pub neck_radius: f32,
    pub belly: Bulge,
    /// The chest's swell in front: the breast muscles, and a bust with a higher `figure`.
    pub bust: Bulge,
    /// The buttocks.
    pub seat: Bulge,
    /// The garment form's fit to the skin's pelvis ([`Body::garment_fill_rows`]), in torso space.
    garment_fill: GarmentFill,
    /// Right then left ([`Side::BOTH`]).
    pub arms: [Limb; 2],
    pub legs: [Limb; 2],
    skin: TorsoProfile,
    /// How far back the body reaches at heights up it, for wings (made on first use).
    pub(crate) back_floors: std::sync::OnceLock<Vec<(f32, f32)>>,
}

fn index(side: Side) -> usize {
    match side {
        Side::Right => 0,
        Side::Left => 1,
    }
}

impl Body {
    /// The body for `spec`, arms relaxed.
    pub fn new(spec: &BodySpec) -> Body {
        Body::posed(spec, Pose::RELAXED)
    }

    /// The body for `spec` with its arms in `pose`.
    pub fn posed(spec: &BodySpec, pose: Pose) -> Body {
        let race = spec.folk.race();
        let h = race.h * spec.stature;
        let s = h / 17.5;
        let (mu, slim) = (spec.muscle + race.brawn, 1.12 - 0.24 * spec.slender);
        // Slenderness thins trunk and limbs alike at the same height; 0.5 is the plate's girth.
        let ws = s.powf(0.72) * race.girth * (0.88 + spec.frame * 0.3) * slim;
        let fem = spec.figure;
        let head_r = race.head_r * s.powf(0.35);
        let half_height = head_r * race.hh;
        let head_y = h - half_height;
        let chin_y = head_y - half_height;
        let shoulder_y = chin_y - race.neck * s * 0.55;
        let hip_y = h * race.leg;
        let t = shoulder_y - hip_y;
        let sw = 2.1 * ws * (1.07 - fem * 0.15) * (0.92 + 0.16 * mu) * race.shoulders;
        let hip = FormRing::new(hip_y, HIP_WIDTH * (1.0 + HIP_LIMB * (race.limb - 1.0).max(0.0)) * ws * (0.95 + fem * 0.13), 1.12 * ws);
        let waist_y = hip_y + 0.3 * t;
        let chest_y = hip_y + 0.7 * t;
        // Muscle narrows the waist under a broad chest; weight fills it.
        let waist = FormRing::new(waist_y, 1.38 * ws * (1.0 - fem * 0.3) * (1.05 - 0.1 * mu) * (0.95 + 0.12 * spec.frame) * (1.0 + 0.6 * (race.shoulders - 1.0)), 0.95 * ws * race.depth);
        // A hump at the withers (a bull's) deepens the chest and the shoulders, front and back, so
        // everything built on the form follows it.
        let hump = race.hump;
        let chest = FormRing::new(chest_y, 1.72 * ws * (0.97 + 0.06 * mu) * race.shoulders, 1.14 * ws * (0.97 + 0.06 * mu) * (1.0 + 0.3 * hump) * race.depth);
        let neck_radius = 0.42 * ws * race.neck_r * (0.92 + 0.16 * mu);
        let belly = Bulge { y: waist_y, spread: 0.3 * t, depth: (race.belly * (0.3 + spec.frame * 0.9) + 0.25 * spec.frame.powi(2)) * ws, sharpness: 2, toward: FRONT, fall: -0.35, ..Bulge::NONE };
        // The chest is two forms either side of the breastbone, the breast muscles (more with
        // muscle) and a bust over them that sits lower and fuller as it rounds. The plate swelled
        // one shelf across the front, past figure 0.55. The seat is two lobes as well, and the
        // small of the back draws in above it (skin only): our own, so the side view has an S.
        let (b, round) = (spec.bust, spec.bust_round);
        // Each breast is a dome of its own either side of the breastbone, with a definite edge
        // and a crease under it: a long slope from the collarbone down to its crest and a short,
        // rounded underside (fuller and a little lower as it rounds). It widens and reaches
        // round toward the side as it grows. Flat, the domes are the breast muscles: squarish
        // plates with a crisp lower edge. A Gaussian swell there blended down into the belly and
        // read as a gut; narrow pointed domes read as cones.
        let bust = Bulge {
            y: chest_y + (0.02 - 0.05 * round * b) * t,
            spread: (0.22 + 0.06 * b) * t,
            depth: (0.08 + 0.14 * mu) * ws + 0.8 * b * ws,
            toward: FRONT,
            pair: 0.43 + 0.06 * b,
            fall: -0.65 + b * (0.2 + 0.15 * round),
            cleft: 1.0,
            dome: 0.41 + 0.1 * b,
            power: 0.3 + b * (0.6 - 0.3 * round),
            upper: 0.6 + 0.7 * b,
            square: 4.0 - 2.0 * b,
            ..Bulge::NONE
        };
        let seat = Bulge { y: hip_y - 0.04 * t, spread: 0.2 * t, depth: 0.17 * ws * (1.0 + 0.4 * fem), sharpness: 5, toward: BACK, pair: 0.55, fall: -0.3, cleft: 1.0, ..Bulge::NONE };
        let lumbar = Bulge { y: waist_y + 0.05 * t, spread: 0.26 * t, depth: -0.05 * ws, sharpness: 2, toward: BACK, ..Bulge::NONE };
        // The front of the pelvis draws in below the belly to the thighs' own front, so the
        // groin narrows into the inner thighs instead of standing out over them (skin only).
        // Legs that stand on the toes lift the ankle to the hock.
        let ankle_y = race.legs.map_or(0.048 * h, |g| g.hock * hip_y);
        let head = HeadForm {
            centre: [0.0, head_y, (0.12 + race.hunch) * s],
            half_width: head_r * race.hw,
            half_height,
            half_depth: head_r * race.hd,
            jaw: race.jaw + (spec.jaw - 0.5) * 0.3,
            chin: race.chin,
            flat: race.flat,
            neck_radius,
            neck_base: shoulder_y + 0.05 * t,
        };
        let skin = TorsoProfile::new(
            &[
                FormRing::new(hip_y - 0.14 * t, hip.half_width * 0.62, hip.half_depth * 0.8),
                FormRing::new(hip_y, hip.half_width * HIP_JOINT, hip.half_depth),
                FormRing::new(hip_y + ILIAC * t, hip.half_width * ILIAC_WIDTH, hip.half_depth),
                waist,
                chest,
                FormRing::new(shoulder_y - 0.05 * t, sw * 0.95, chest.half_depth * 0.88 * (1.0 + 0.3 * hump)),
                FormRing::new(shoulder_y + 0.05 * t, sw * 0.62, chest.half_depth * 0.66 * (1.0 + 0.6 * hump)),
                FormRing::new(shoulder_y + 0.11 * t, neck_radius * 1.2, neck_radius * 1.1),
                FormRing::new(shoulder_y + 0.17 * t, neck_radius, neck_radius * 0.95),
            ],
            TRUNK_EXPONENT,
            TRUNK_BACK,
            vec![belly, bust, seat, lumbar],
        );
        let arm = |side: Side| arm_limb(race, h, ws, fem, mu, sw, shoulder_y, side, pose.arm(side));
        let leg = |side: Side| leg_limb(race, s, ws, fem, mu, &hip, ankle_y, side);
        let mut body = Body {
            spec: spec.clone(),
            pose,
            height: h,
            scale: s,
            girth: ws,
            head,
            chin_y,
            shoulder_y,
            chest_y,
            waist_y,
            hip_y,
            ankle_y,
            trunk_length: t,
            shoulder_half_width: sw,
            chest,
            waist,
            hip,
            neck_radius,
            belly,
            bust,
            seat,
            garment_fill: GarmentFill::default(),
            arms: [arm(Side::Right), arm(Side::Left)],
            legs: [leg(Side::Right), leg(Side::Left)],
            skin,
            back_floors: std::sync::OnceLock::new(),
        };
        body.draw_in_pubis();
        body.garment_fill = body.garment_fill_rows();
        body
    }

    /// Draws the front of the pelvis in to the thighs' own front, where the trunk would stand
    /// out past them below the belly (a rounded mass over the groin, read from the front as a
    /// second rear): the groin then narrows into the inner thighs. The swell it removes is
    /// measured on this body, so a dwarf whose thighs are already as deep as its hips loses
    /// none (skin only; garments are built on the form and measured over the skin).
    fn draw_in_pubis(&mut self) {
        let (t, s) = (self.trunk_length, self.scale);
        let (top, bottom) = (self.hip_y + 0.05 * t, self.hip_y - CROTCH_DROP * t);
        let (mut at, mut excess) = (self.hip_y, 0.0f32);
        for i in 0..=PUBIS_SAMPLES {
            let y = top + (bottom - top) * i as f32 / PUBIS_SAMPLES as f32;
            let Some((thigh_front, _)) = self.thigh_reach(y) else { continue };
            let over = self.skin.section(FRONT, y, 0.0)[1] - thigh_front - 0.02 * s;
            if over > excess {
                (at, excess) = (y, over);
            }
        }
        if excess > 0.0 {
            self.skin.bulges.push(Bulge { y: at, spread: 0.16 * t, depth: -excess * 0.7, sharpness: 2, toward: FRONT, fall: 0.6, ..Bulge::NONE });
        }
        // The thighs do not change between the passes: their reach is measured once.
        let ys = self.fit_heights();
        let thighs: Vec<Option<(f32, f32)>> = ys.iter().map(|&y| self.thigh_reach(y)).collect();
        for _ in 0..FIT_PASSES {
            self.fit_trunk_to_thighs(&ys, &thighs);
        }
    }

    /// The share along the left leg at height `y`: the leg is a line falling steadily from the
    /// root, so it is found by bisection. `None` above the root and below the ankle. (The body
    /// is symmetric, so the left leg stands for both.)
    fn along_leg(&self, y: f32) -> Option<f32> {
        let leg = self.leg(Side::Left);
        let (top, bottom) = (leg.point_at(0.0)[1], leg.point_at(1.0)[1]);
        if y > top || y < bottom {
            return None;
        }
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..BISECTIONS {
            let mid = 0.5 * (lo + hi);
            if leg.point_at(mid)[1] > y {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some(0.5 * (lo + hi))
    }

    /// How far the left thigh reaches in front of its axis and behind it, at height `y`: the
    /// thigh's radius at the front and at the back, from the axis' depth. `None` above the
    /// thigh's root and below its ankle.
    fn thigh_reach(&self, y: f32) -> Option<(f32, f32)> {
        let leg = self.leg(Side::Left);
        let u = self.along_leg(y)?;
        let c = leg.point_at(u)[2];
        Some((c + self.leg_radius(leg, u, 0.0), -(c - self.leg_radius(leg, u, PI))))
    }

    /// How far out from the midline the thigh reaches at height `y`.
    fn thigh_outer(&self, y: f32) -> Option<f32> {
        let leg = self.leg(Side::Left);
        let u = self.along_leg(y)?;
        Some(leg.point_at(u)[0].abs() + self.leg_radius(leg, u, 0.5 * PI))
    }

    /// The heights the trunk is fitted at: from the waist to the crotch, and a closing run
    /// beyond each end so the closing has neighbours at the ends.
    fn fit_heights(&self) -> Vec<f32> {
        let t = self.trunk_length;
        let (lo, hi) = (self.hip_y - CROTCH_DROP * t, self.waist_y);
        let run = RUN * t;
        let (y0, y1) = (lo - FIT_MARGIN * run, hi + FIT_MARGIN * run);
        (0..=FIT_SAMPLES).map(|i| y0 + (y1 - y0) * i as f32 / FIT_SAMPLES as f32).collect()
    }

    /// Fits the trunk's skin from the waist to the crotch to the thighs' own silhouette, front
    /// and back, at the heights `ys` ([`Body::fit_heights`]), where `thighs` is each one's
    /// [`Body::thigh_reach`]. The silhouette of the trunk and the thigh together is closed
    /// over a short run of height (a sliding maximum, then minimum), which fills the creases
    /// where the trunk falls away from a thigh that carries on, and the fill is added to the
    /// trunk, so the seat rounds into the backs of the thighs and the hips into the waist
    /// without a step. Closing never takes anything away, so each folk keeps its own
    /// proportions.
    fn fit_trunk_to_thighs(&mut self, ys: &[f32], thighs: &[Option<(f32, f32)>]) {
        let t = self.trunk_length;
        let (lo, hi) = (self.hip_y - CROTCH_DROP * t, self.waist_y);
        let run = RUN * t;
        // Where the trunk and a thigh reach the same depth the union takes a little more than
        // either, so one never shows as an edge on the other.
        let blend = 0.05 * self.scale;
        let smooth_max = |a: f32, b: f32, w: f32| a.max(b) + w * (1.0 + (-(a - b).abs() / w).exp()).ln();
        let n = FIT_SAMPLES;
        // Depth to the front, and to the back, of the trunk and of it with the thigh.
        let reach = |y: f32, toward: f32, sign: f32| (0..=SEAT_STEPS).map(|k| sign * self.skin.section(toward - SEAT_SPAN + SEAT_STEP * k as f32, y, 0.0)[1]).fold(f32::MIN, f32::max);
        let trunk: Vec<(f32, f32)> = ys.iter().map(|&y| (reach(y, FRONT, 1.0), reach(y, BACK, -1.0))).collect();
        let union: Vec<(f32, f32)> = trunk.iter().zip(thighs).map(|(&(f, b), thigh)| thigh.map_or((f, b), |(lf, lb)| (smooth_max(f, lf, blend), smooth_max(b, lb, blend)))).collect();
        let w = ((run / ((ys[n] - ys[0]) / n as f32)).round() as isize).max(1);
        let slide = |v: Vec<f32>, max: bool| -> Vec<f32> {
            let len = v.len() as isize;
            (0..len).map(|i| (i - w..=i + w).filter(|j| (0..len).contains(j)).map(|j| v[j as usize]).fold(if max { f32::MIN } else { f32::MAX }, |a, b| if max { a.max(b) } else { a.min(b) })).collect()
        };
        let close = |v: Vec<f32>| slide(slide(v, true), false);
        let front = close(union.iter().map(|u| u.0).collect());
        let back = close(union.iter().map(|u| u.1).collect());
        // A little smoothing of the fill itself, so it has no creases of its own.
        let soften = |v: Vec<f32>| -> Vec<f32> {
            let at = |i: isize| v[i.clamp(0, v.len() as isize - 1) as usize];
            (0..v.len() as isize).map(|i| (at(i - 2) + 2.0 * at(i - 1) + 3.0 * at(i) + 2.0 * at(i + 1) + at(i + 2)) / 9.0).collect()
        };
        let df = soften((0..=n).map(|i| FRONT_FILL * (front[i] - trunk[i].0).max(0.0)).collect());
        let db = soften((0..=n).map(|i| (back[i] - trunk[i].1).max(0.0)).collect());
        let window = |y: f32| smoothstep(lo, lo + run, y) * (1.0 - smoothstep(hi - run, hi + run, y));
        let fill: Vec<[f32; 3]> = (0..=n).filter(|&i| ys[i] >= lo && ys[i] <= hi + run).map(|i| [ys[i], df[i] * window(ys[i]), db[i] * window(ys[i])]).collect();
        // The ends of the run taper to nothing, so the fill joins the bare section.
        if let (Some(first), Some(last)) = (fill.first().copied(), fill.last().copied()) {
            let mut run = vec![[first[0] - 1e-3, 0.0, 0.0]];
            run.extend(fill);
            run.push([last[0] + 1e-3, 0.0, 0.0]);
            // A second pass adds what the first left short (the fill moves the silhouette
            // round the lobes of the seat a little).
            if self.skin.fill.len() == run.len() {
                for (row, add) in self.skin.fill.iter_mut().zip(&run) {
                    row[1] += add[1];
                    row[2] += add[2];
                }
            } else {
                self.skin.fill = run;
            }
        }
    }

    pub fn folk(&self) -> Folk {
        self.spec.folk
    }

    pub fn race(&self) -> &'static Race {
        self.spec.folk.race()
    }

    pub fn arm(&self, side: Side) -> &Limb {
        &self.arms[index(side)]
    }

    pub fn leg(&self, side: Side) -> &Limb {
        &self.legs[index(side)]
    }

    // ── The skin's trunk ──

    /// The skin's trunk, crotch to the top of the neck, in body space.
    pub fn trunk(&self) -> &TorsoProfile {
        &self.skin
    }

    /// The trunk's skin at `angle` (lathe convention: [`FRONT`], [`BACK`]) and height `y`,
    /// `inflate` further out, with the outward normal there.
    pub fn trunk_surface(&self, angle: f32, y: f32, inflate: f32) -> (V3, V3) {
        let at = |a: f32, y: f32| {
            let [x, z] = self.skin.section(a, y, inflate);
            [x, y, z]
        };
        let p = at(angle, y);
        let da = vec3::sub(at(angle + 1e-3, y), at(angle - 1e-3, y));
        let dy = vec3::sub(at(angle, y + 1e-3), at(angle, y - 1e-3));
        (p, vec3::normalize_or(vec3::cross(da, dy), [p[0], 0.0, p[2]]))
    }

    /// A frame on the trunk's skin at `angle` and `y`: `z` out of the skin, `y` up along it,
    /// `x = y × z`. Bags and straps hang from [`Body::back`]; brooches and pendants from
    /// points on the front.
    pub fn trunk_anchor(&self, angle: f32, y: f32) -> Placement {
        let (p, n) = self.trunk_surface(angle, y, 0.0);
        let up = vec3::normalize_or(vec3::sub(vec3::Y, vec3::scale(n, n[1])), vec3::Y);
        Placement::new(p, vec3::cross(up, n), up, n)
    }

    /// Between the shoulder blades, on the skin: where a pack rides.
    pub fn back(&self) -> Placement {
        self.trunk_anchor(BACK, self.chest_y)
    }

    /// The front of the chest, on the skin.
    pub fn breast(&self) -> Placement {
        self.trunk_anchor(FRONT, self.chest_y)
    }

    /// The base of the neck, on the axis: `y` up. The neck's radius there is about
    /// `1.2 * neck_radius`; a chain round the neck passes through
    /// `trunk_surface(angle, neck_base().origin[1], 0)`.
    pub fn neck_base(&self) -> Placement {
        Placement::translation([0.0, self.shoulder_y + 0.11 * self.trunk_length, 0.0])
    }

    // ── Armory forms sized to this body ──

    /// The torso's space in body space: the Armory's [`TorsoForm`] and [`HeadForm`] put the
    /// waist at y = 0.
    pub fn torso_placement(&self) -> Placement {
        Placement::translation([0.0, self.waist_y, 0.0])
    }

    /// The garment form for this body (torso space, see [`Body::torso_placement`]): the
    /// trunk's sections, eased a little over the hips, and below them the envelope of both
    /// legs that a skirt hangs over.
    pub fn torso_form(&self) -> TorsoForm {
        TorsoForm { fill: self.garment_fill.rows, fill_floor: self.garment_fill.floor, fill_side: self.garment_fill.side, ..self.torso_form_bare() }
    }

    /// [`Body::torso_form`] without its fit to the skin's pelvis.
    fn torso_form_bare(&self) -> TorsoForm {
        let t = self.trunk_length;
        let s = self.scale;
        let w = self.waist_y;
        let sw = self.shoulder_half_width;
        let (hip, chest) = (self.hip, self.chest);
        let hump = self.race().hump;
        let below = |y: f32, k: f32| FormRing::new(y - w, hip.half_width * k, hip.half_depth * (k + 0.04));
        TorsoForm {
            neck: FormRing::new(self.shoulder_y + 0.11 * t - w, self.neck_radius * 1.2, self.neck_radius * 1.1),
            shoulder_top: FormRing::new(self.shoulder_y + 0.05 * t - w, sw * 0.62, chest.half_depth * 0.66 * (1.0 + 0.6 * hump)),
            shoulder: FormRing::new(self.shoulder_y - 0.05 * t - w, sw * 0.95, chest.half_depth * 0.88 * (1.0 + 0.3 * hump)),
            chest: chest.raised(-w),
            waist: self.waist.raised(-w),
            hip: FormRing::new(hip.y - w, hip.half_width * 1.01, hip.half_depth * 1.03),
            thigh: self.thigh_ring(w),
            knee: below(self.leg(Side::Left).joints[1][1], 1.04),
            shin: below(self.ankle_y + 0.5 * s, 1.04),
            exponent: TRUNK_EXPONENT,
            back: TRUNK_BACK,
            belly: self.belly.raised(-w),
            // Cloth is drawn across the cleft between the lobes, not into it.
            bust: self.bust.bridged().raised(-w),
            seat: self.seat.bridged().raised(-w),
            fill: [[0.0; 3]; FILL_ROWS],
            fill_floor: [0.0; FILL_ROWS],
            fill_side: [0.0; FILL_ROWS],
        }
    }

    /// The form's section over the tops of the thighs: no wider than the hips, and no wider
    /// than the thighs themselves (a little over) where they are narrower, so the cloth
    /// narrows from the hips down the thighs as the legs do and not as a pear over them.
    fn thigh_ring(&self, w: f32) -> FormRing {
        let (hip, y) = (self.hip, self.hip_y - THIGH_RING * self.scale);
        let hips = hip.half_width * 1.035;
        let width = self.thigh_outer(y).map_or(hips, |x| (THIGH_WIDTH * x).min(hips));
        FormRing::new(y - w, width, hip.half_depth * 1.075)
    }

    /// The garment form's fit to the skin's pelvis, in torso space: at each of [`FILL_ROWS`]
    /// heights from the crotch to above the waist, how far the form (sections and swellings)
    /// falls short of, or stands past, the skin's reach in front and behind, and a little
    /// over ([`GARMENT_MARGIN`]), so cloth hangs over the pelvis the skin has (the groin drawn
    /// in, the seat rounding into the thighs) and not over the form's own, which stood out
    /// from it as a bulge. Tapered to nothing at both ends, so the form meets itself there.
    fn garment_fill_rows(&self) -> GarmentFill {
        let Some(last) = self.skin.fill.last() else { return GarmentFill::default() };
        let (t, w) = (self.trunk_length, self.waist_y);
        let (lo, hi, run) = (self.hip_y - FILL_DEPTH * self.scale, w, RUN * t);
        let hip_thigh = self.hip_y - THIGH_RING * self.scale;
        let form = self.torso_form_bare().profile();
        let margin = GARMENT_MARGIN * self.scale;
        let reach = |profile: &TorsoProfile, y: f32, toward: f32, sign: f32| (0..=SEAT_STEPS).map(|k| sign * profile.section(toward - SEAT_SPAN + SEAT_STEP * k as f32, y, 0.0)[1]).fold(f32::MIN, f32::max);
        // How far the form falls short of the skin at the worst of the angles round `toward`
        // (the skin's lobes of the seat reach further than the form's bridged one does).
        let short = |y: f32, toward: f32, sign: f32| {
            (0..=SEAT_STEPS)
                .map(|k| {
                    let a = toward - SEAT_SPAN + SEAT_STEP * k as f32;
                    sign * (self.skin.section(a, y, 0.0)[1] - form.section(a, y - w, 0.0)[1])
                })
                .fold(f32::MIN, f32::max)
        };
        let rows: [(f32, [f32; 3], f32); FILL_ROWS] = std::array::from_fn(|i| {
            let y = lo + (last[0] - lo) * i as f32 / (FILL_ROWS - 1) as f32;
            // Both fills end at the waist; in front it carries on down the thighs past the
            // crotch (so the cloth there narrows into the thigh without a step), behind it
            // builds up gently from the thigh section.
            let top = 1.0 - smoothstep(hi - run, hi + run, y);
            let (window_front, window_back) = (smoothstep(lo, lo + FILL_ONSET_FRONT * self.scale, y) * top, smoothstep(hip_thigh, hip_thigh + FILL_ONSET * self.scale, y) * top);
            // The skin the cloth hangs over is the trunk's and the thigh's together.
            let (thigh_front, thigh_back) = self.thigh_reach(y).unwrap_or((f32::MIN, f32::MIN));
            let front = short(y, FRONT, 1.0).max(thigh_front - reach(&form, y - w, FRONT, 1.0)) + margin;
            let back = short(y, BACK, -1.0).max(thigh_back - reach(&form, y - w, BACK, -1.0)) + margin;
            // At the sides the form may only be drawn in, to the body's width there: the cloth
            // narrows into the thigh as the thigh does.
            let side_reach = |profile: &TorsoProfile, y: f32| (0..=SIDE_STEPS).map(|k| profile.section(TAU * k as f32 / SIDE_STEPS as f32, y, 0.0)[0].abs()).fold(0.0f32, f32::max);
            let body_x = side_reach(&self.skin, y).max(self.thigh_outer(y).unwrap_or(0.0));
            let side = body_x + margin - side_reach(&form, y - w);
            (form.at(y - w).1, [y - w, front * window_front, back * window_back], side * window_front)
        });
        GarmentFill { rows: rows.map(|r| r.1), floor: rows.map(|r| r.0), side: rows.map(|r| r.2) }
    }

    /// The head form for this body, in torso space like [`Body::torso_form`] (hoods take
    /// both). [`Body::head`] is the same head in body space.
    pub fn head_form(&self) -> HeadForm {
        HeadForm { centre: vec3::sub(self.head.centre, [0.0, self.waist_y, 0.0]), neck_base: self.head.neck_base - self.waist_y, ..self.head }
    }

    /// The head's frame in body space: origin at its centre, `x` the wearer's left, `y` up,
    /// `z` the face. A helmet sized to [`HeadForm::half_width`] sits here.
    pub fn head_frame(&self) -> Placement {
        Placement::translation(self.head.centre)
    }

    /// The lower leg and foot for boots and greaves ([`LegForm`] space: toe +X, sole y = 0;
    /// see [`Body::foot_placement`]). Both feet share it.
    pub fn leg_form(&self) -> LegForm {
        let race = self.race();
        let leg = self.leg(Side::Left);
        let length = 0.148 * self.height * race.foot;
        let size = length / 2.6;
        let ankle = leg.radius_at(1.0);
        let mut form = LegForm::mannequin(size, 0.66 * leg.radius / (0.46 * size), 0.0);
        form.shin_half_width = ankle * 1.1;
        form.shin_half_depth = ankle * 0.95;
        // The calf swells to the leg's own girth there.
        form.calf = (leg.radius * 0.64 / form.shin_half_width - 1.0).max(0.1);
        form.knee_height = leg.joints[1][1];
        form
    }

    /// Where a foot's [`LegForm`] space sits: the ankle over the leg's last joint, the toe
    /// turned out a little (`footFn`'s 0.12 radians).
    pub fn foot_placement(&self, side: Side) -> Placement {
        let form = self.leg_form();
        let a = self.leg(side).joints[2];
        let turn = side.sign() * 0.12;
        let forward = [turn.sin(), 0.0, turn.cos()];
        let origin = vec3::sub([a[0], 0.0, a[2]], vec3::scale(forward, form.ankle_x));
        Placement::new(origin, forward, vec3::Y, vec3::cross(forward, vec3::Y))
    }

    /// Both legs and the hips, for trousers (body space: soles at y = 0).
    pub fn lower_body_form(&self) -> LowerBodyForm {
        let leg = self.leg(Side::Left);
        // The garment form's hips run on a spline of their own between hip and waist, a few
        // millimetres inside the skin's at the sides in places: ease them out a little, so even
        // close leggings cover the hips (our own allowance).
        let torso = self.torso_form();
        let mut rings = torso.rings();
        for ring in &mut rings[2..=4] {
            *ring = ring.inflated(0.05, 0.03);
        }
        // At the crotch the cloth takes in the leg's own top section, which fills out under the
        // seat behind.
        let sections = self.leg_sections();
        let top = sections[LEG_SECTIONS - 1];
        LowerBodyForm {
            torso: torso.with_rings(rings),
            leg: self.leg_form(),
            waist_height: self.waist_y,
            // Where the trunk's skin rounds under between the thighs, so the seat covers it.
            crotch_height: self.hip_y - 0.14 * self.trunk_length - 0.25 * self.scale,
            // Half of how far out the real thigh reaches there (the two halves meet at the
            // midline), not its own half width again: the crotch section stood out past the
            // thigh as a lobe.
            thigh_half_width: 0.5 * (top.x + top.half_width) * 1.02,
            thigh_half_depth: (leg.radius * 1.08).max(top.front.max(top.back) * 1.04),
            // The thigh's own reach at the crotch each way (flat in front, full behind), not the
            // deeper of the two both ways, which stood the cloth out in front as a pouch.
            thigh_front: top.front * if self.race().hooved() { 1.45 } else { 1.04 },
            thigh_back: top.back * if self.race().hooved() { 1.45 } else { 1.04 },
            stance: leg.joints[2][0].abs(),
            sections: Some(sections),
            // A leg on its toes ends its cloth at the hock.
            hem_floor: if self.race().hooved() { self.ankle_y - HOCK_HEM * self.scale } else { 0.0 },
        }
    }

    /// The hand for gloves and the bare hand alike, a right hand in [`HandForm`] space
    /// (mirror it with [`Body::hand_placement`] for the left). Its length is the plate's
    /// `handL` (0.105 of the height times the folk's arm length); the wrist matches the
    /// arm's, and the knuckles and fingers widen with it, so a dwarf's hand is broad and a
    /// goblin's slender.
    pub fn hand_form(&self) -> HandForm {
        let race = self.race();
        let length = 0.105 * self.height * race.arm;
        let size = length / 1.77;
        let mut form = HandForm::mannequin(size, 0.12);
        let wrist = self.arm(Side::Left).radius_at(1.0);
        form.wrist_half_width = wrist * 1.28;
        form.wrist_half_depth = wrist * 0.8;
        form.knuckle_half_width = form.wrist_half_width * 1.42;
        form.knuckle_half_depth = form.wrist_half_depth * 0.72;
        let k = form.knuckle_half_width / (0.48 * size);
        for d in form.fingers.iter_mut().chain(std::iter::once(&mut form.thumb)) {
            d.base[0] *= k;
            d.radius *= k.powf(0.7);
        }
        form.forearm_half_width = form.wrist_half_width;
        form.forearm_half_depth = form.wrist_half_depth;
        // The forearm fills out to about 1.5 times the wrist two thirds of the way up.
        let forearm = vec3::distance(self.arm(Side::Left).joints[1], self.arm(Side::Left).joints[2]);
        form.forearm_taper = 0.5 / (0.65 * forearm).max(0.1);
        form
    }

    /// How a hand is held: relaxed, or closed on its pose's grip.
    pub fn hand_pose(&self, side: Side) -> HandPose {
        match self.pose.arm(side).grip {
            Some(r) => HandPose::gripping(&self.hand_form(), r),
            None => HandPose::RELAXED,
        }
    }

    /// Where a hand's [`HandForm`] space sits on the wrist: `y` down the forearm's line,
    /// `z` (the back of the hand) facing out from the body, the thumb toward the front.
    /// The left hand's placement mirrors.
    pub fn hand_placement(&self, side: Side) -> Placement {
        let arm = self.arm(side);
        let sg = side.sign();
        let y = arm.tangent_at(1.0);
        let out = [sg, 0.0, 0.0];
        let z = vec3::normalize_or(vec3::sub(out, vec3::scale(y, vec3::dot(out, y))), vec3::Z);
        let x = vec3::scale(vec3::cross(y, z), -sg);
        Placement::new(arm.joints[2], x, y, z)
    }

    /// The grip frame of a hand: origin on the axis of the channel its fingers close
    /// round, `y` along that axis out past the thumb and forefinger (where a blade or a
    /// head goes), `z` out of the back of the hand. Never mirrored, so held items keep
    /// their handedness. Meaningful when the arm's pose has a grip.
    pub fn grip(&self, side: Side) -> Placement {
        let hand = self.hand_placement(side);
        let centre = grip_centre(&self.hand_form(), &self.hand_pose(side));
        let y = hand.direction(vec3::neg(vec3::X));
        let z = hand.direction(vec3::Z);
        Placement::new(hand.point(centre), vec3::cross(y, z), y, z)
    }

    /// The radius a grip may have in this hand (`docs/body.md`: `clamp(arm radius * 0.27,
    /// 0.07, GRIP_MAX)`, GRIP_MAX = 0.13 from `data/constants.json`).
    pub fn grip_radius(&self) -> f32 {
        (self.arm(Side::Left).radius * 0.27).clamp(0.07, GRIP_MAX)
    }

    // ── Skin ──

    /// The whole bare body: skin, head and hair.
    pub fn mesh(&self) -> ItemMesh {
        let mut mesh = self.skin_mesh();
        mesh.parts.extend(self.head_mesh().parts);
        mesh.parts.extend(self.hair_mesh(crate::hair::HairFit::default()).parts);
        mesh
    }

    /// Every bare-skin part: `"trunk"`, `"arms"`, `"hands"`, `"nails"`, `"legs"`, `"feet"`,
    /// and a lizard's `"tail"` and `"claws"` or a pig's `"tail"`.
    pub fn skin_mesh(&self) -> ItemMesh {
        let skin = self.spec.skin;
        let fur = self.fur_colour();
        let pattern = self.skin_pattern();
        let mut parts = vec![];
        let mut add = |m: MeshBuilder, name: &'static str, tint: Rgb, outline: bool| {
            if m.is_empty() {
                return;
            }
            let mut part = m.into_part(name, OutlineMode::Welded).with_material(MaterialKind::Skin).with_tint(tint).with_pattern(pattern);
            if !outline {
                part = part.without_outline();
                part.pattern = None;
            }
            parts.push(part);
        };
        add(self.trunk_mesh(), "trunk", skin, true);
        let mut arms = MeshBuilder::new();
        let mut hands = MeshBuilder::new();
        let mut nails = MeshBuilder::new();
        for side in Side::BOTH {
            arms.append(&self.arm_mesh(side));
            let (mut h, mut n) = hand_meshes_with(&self.hand_form(), &self.hand_pose(side), self.race().claws);
            let at = self.hand_placement(side);
            at.apply(&mut h);
            at.apply(&mut n);
            hands.append(&h);
            nails.append(&n);
        }
        add(arms, "arms", skin, true);
        add(hands, "hands", skin, true);
        add(nails, "nails", if self.race().claws { crate::parts::shade(skin, -0.45) } else { lighten(skin, 0.22) }, false);
        let mut legs = MeshBuilder::new();
        let mut feet = MeshBuilder::new();
        let mut hooves = MeshBuilder::new();
        for side in Side::BOTH {
            legs.append(&self.leg_mesh(side));
            if let Some(foot) = self.hooved_foot(side) {
                // A leg on its toes ends in a pastern and a cloven hoof, not a sole.
                feet.append(&foot.flesh);
                hooves.append(&foot.hoof);
                continue;
            }
            let mut f = foot_mesh(&self.leg_form(), 0.0);
            f.append(&self.toes_mesh(side));
            self.foot_placement(side).apply(&mut f);
            feet.append(&f);
        }
        add(legs, "legs", fur.unwrap_or(skin), true);
        // Pigfolk walk on darker trotters (`buildBody`: the skin at 0.62).
        let socks = self.race().coat && self.traits().marking == crate::parts::Marking::Socks;
        add(feet, "feet", if self.folk() == Folk::Pigfolk { skin.scaled(0.62) } else if socks { crate::coat::WHITE } else { fur.unwrap_or(skin) }, true);
        debug_assert!(!(self.race().coat && self.race().fur.is_some()), "a race has a coat or leg fur, not both");
        add(self.tail_mesh(), "tail", fur.unwrap_or(skin), true);
        let mut out = ItemMesh { parts };
        if !hooves.is_empty() {
            let tone = crate::parts::mix(Rgb::hex(0x1f1a17), Rgb::hex(0x8d8473), self.traits().hoof_tone);
            out.parts.push(hooves.into_part("hooves", OutlineMode::Welded).with_material(MaterialKind::Bone).with_tint(tone));
        }
        if let Some(spade) = self.tail_spade() {
            let tint = crate::parts::shade(skin, -0.25);
            out.parts.push(spade.into_part("tail_spade", OutlineMode::Welded).with_material(MaterialKind::Skin).with_tint(tint));
        }
        out.parts.extend(self.wing_parts(0.0));
        if let Some(hair) = self.tail_hair() {
            let tint = fur.unwrap_or(self.spec.hair_colour);
            out.parts.push(hair.into_part("tail_hair", OutlineMode::Welded).with_material(MaterialKind::Hair).with_tint(tint).without_outline());
        }
        if let Some(colour) = fur {
            let locks = self.leg_fur();
            if !locks.is_empty() {
                out.parts.push(locks.into_part("leg_fur", OutlineMode::Welded).with_material(MaterialKind::Hair).with_tint(colour).without_outline());
            }
        }
        out.parts.extend(self.markings());
        if self.folk() == Folk::Lizardfolk {
            let claws = self.claws_mesh().into_part("claws", OutlineMode::Welded).with_material(MaterialKind::Bone);
            out.parts.push(claws);
        }
        out
    }

    /// The toes of a bare foot in its [`LegForm`] space (empty for a race with none): the big
    /// toe on the inner side, the rest smaller and shorter toward the outer, each a rounded
    /// lobe over the foot's end, so a giant's foot has toes and not a bare club's end.
    fn toes_mesh(&self, side: Side) -> MeshBuilder {
        let mut out = MeshBuilder::new();
        let n = self.race().toes;
        if n == 0 {
            return out;
        }
        let form = self.leg_form();
        let l = form.foot_length;
        let (w, _) = form.half_width(0.9 * l);
        let toe_h = form.height(0.9 * l);
        for i in 0..n {
            let t = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.5 };
            // Across the ball, inner to outer, the big toe first.
            let z = side.sign() * (w * 0.78 - 2.0 * w * 0.78 * t);
            let size = 1.0 - 0.52 * t;
            let rad = 0.27 * w * (0.8 + 0.4 * size);
            let x = 0.94 * l + 0.04 * l * (1.0 - t) - 0.02 * l * t;
            let mut toe = uv_sphere(1.0, 14, 10);
            let ry = 0.5 * toe_h * (0.8 + 0.2 * size);
            toe.scale([rad * 1.5, ry, rad]);
            toe.translate([x, ry * 0.95, z]);
            out.append(&toe);
        }
        out
    }

    /// Lizardfolk skin is scaled (the plate's skin pattern 6); others are plain.
    fn skin_pattern(&self) -> Option<SurfacePattern> {
        (self.folk() == Folk::Lizardfolk).then(|| SurfacePattern { kind: PatternKind::Scales, color: self.spec.skin.scaled(0.72), scale: 3.2 })
    }

    /// The trunk from below the crotch to inside the head, closed at both ends.
    fn trunk_mesh(&self) -> MeshBuilder {
        let t = self.trunk_length;
        let (y0, y1) = (self.hip_y - 0.14 * t, self.shoulder_y + 0.17 * t);
        let cols = 72;
        let ring = |y: f32, k: f32, dy: f32| -> Vec<V3> {
            (0..cols)
                .map(|c| {
                    let a = TAU * c as f32 / cols as f32;
                    let [x, z] = self.skin.section(a, y, 0.0);
                    [x * k, y + dy, z * k]
                })
                .collect()
        };
        // The crotch rounds under, between the thighs' roots.
        let mut rows: Vec<Vec<V3>> = [(0.25, -0.3), (0.55, -0.22), (0.8, -0.12)].iter().map(|&(k, d)| ring(y0, k, d * self.scale)).collect();
        let n = 60;
        rows.extend((0..=n).map(|r| ring(y0 + (y1 - y0) * r as f32 / n as f32, 1.0, 0.0)));
        let mut mesh = stitch_grid(&rows, &GridOptions { wrap_around: true, ..Default::default() });
        mesh.push_fan_cap(&rows[0], vec3::neg(vec3::Y));
        mesh.push_fan_cap(&rows[rows.len() - 1], vec3::Y);
        mesh
    }

    /// One arm's skin, root to wrist. The section is round at the shoulder and flattens
    /// toward the wrist to the hand form's wrist (wide across the palm, thin through it), so
    /// arm and hand meet without a step; the frames are carried up from the hand.
    fn arm_mesh(&self, side: Side) -> MeshBuilder {
        let arm = self.arm(side);
        let hand = self.hand_form();
        let wrist = arm.radius_at(1.0);
        let (back, across) = (hand.wrist_half_depth / wrist, hand.wrist_half_width / wrist);
        let te = arm.t_joints[1];
        let place = self.hand_placement(side);
        arm.mesh(28, vec3::Z, Some(place.z()), |t, a| {
            let f = smoothstep(te + 0.1 * (1.0 - te), 1.0, t);
            let (kb, ka) = (1.0 + (back - 1.0) * f, 1.0 + (across - 1.0) * f);
            let (s, c) = a.sin_cos();
            arm.radius_at(t) / ((c / kb).powi(2) + (s / ka).powi(2)).sqrt()
        })
    }

    /// One leg's skin, root to ankle, with the calf swelling behind and the knee in front.
    fn leg_mesh(&self, side: Side) -> MeshBuilder {
        let leg = self.leg(side);
        leg.mesh(28, vec3::Z, None, |t, a| self.leg_radius(leg, t, a))
    }

    /// A leg's radius at `t` along it and `a` round it (0 the front, +Z; PI the back).
    pub(crate) fn leg_radius(&self, leg: &Limb, t: f32, a: f32) -> f32 {
        let [th, tk] = leg.t_joints;
        let calf_t = tk + 0.3 * (1.0 - tk);
        let front = a.cos();
        let calf = (0.06 + 0.1 * self.spec.muscle) * (-((t - calf_t) / 0.12).powi(2)).exp() * (-front).max(0.0).powi(2);
        let knee = 0.05 * (-((t - tk) / 0.05).powi(2)).exp() * front.max(0.0).powi(3);
        // The thigh is a little deeper front to back than across, below the seat.
        let window = smoothstep(0.0, th, t);
        let thigh = window * THIGH_DEEP * (1.0 - smoothstep(th, tk, t)) * front * front + window * THIGH_BACK * (1.0 - smoothstep(th + 0.2 * (tk - th), th + 0.6 * (tk - th), t)) * (-front).max(0.0).powi(2);
        // The back of the thigh's top fills out under the seat, so the buttock rounds into the
        // thigh in a fold instead of overhanging it as a rim.
        let under_seat = 0.24 * (-((t - 1.7 * th) / th).powi(2)).exp() * (-front).max(0.0).powi(2);
        leg.radius_at(t) * (1.0 + calf + knee + thigh + under_seat)
    }

    /// The left leg's skin in horizontal sections from the ankle up to the crotch, for
    /// legwear to follow ([`LowerBodyForm::sections`]): its middle, and how far it reaches to
    /// the side, in front and behind, sampled round the leg as drawn.
    pub fn leg_sections(&self) -> [LegSection; LEG_SECTIONS] {
        let leg = self.leg(Side::Left);
        let crotch = self.hip_y - 0.14 * self.trunk_length;
        let (y0, y1) = (leg.joints[2][1], crotch - 0.1 * self.scale);
        std::array::from_fn(|i| {
            let y = y0 + (y1 - y0) * i as f32 / (LEG_SECTIONS - 1) as f32;
            // The share along the leg at this height (its line falls steadily below the hip).
            let t = (0..=400).map(|k| k as f32 / 400.0).min_by(|a, b| (leg.point_at(*a)[1] - y).abs().total_cmp(&(leg.point_at(*b)[1] - y).abs())).unwrap_or(1.0);
            let c = leg.point_at(t);
            let along = leg.tangent_at(t);
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            // Round the leg's section, each point dropped onto this height along the leg.
            let a0 = vec3::normalize_or(vec3::sub(vec3::Z, vec3::scale(along, along[2])), vec3::Z);
            let b0 = vec3::cross(along, a0);
            for k in 0..48 {
                let a = TAU * k as f32 / 48.0;
                let r = self.leg_radius(leg, t, a);
                let p = vec3::add(c, vec3::add(vec3::scale(a0, r * a.cos()), vec3::scale(b0, r * a.sin())));
                lo = [lo[0].min(p[0]), lo[1].min(p[2])];
                hi = [hi[0].max(p[0]), hi[1].max(p[2])];
            }
            // Full thighs meet at the midline near the crotch: the cloth of each leg stops there
            // rather than crossing into the other's.
            lo[0] = lo[0].max(0.0);
            let (x, z) = (0.5 * (lo[0] + hi[0]), c[2]);
            LegSection { y, x, z, half_width: 0.5 * (hi[0] - lo[0]), front: hi[1] - z, back: z - lo[1] }
        })
    }

    /// A tail's centre line and its radius along it (`u` from 0 at the root to 1 at the
    /// tip): a lizard's sweeping down and back from the base of the spine, or a pig's curl.
    /// `None` for folk without one.
    pub fn tail(&self) -> Option<(Vec<V3>, TailRadius)> {
        let (t, s, h) = (self.trunk_length, self.scale, self.height);
        let (hy, hd) = (self.hip_y, self.hip.half_depth);
        match self.folk() {
            Folk::Lizardfolk => {
                let path = CatmullRom::centripetal(vec![
                    [0.0, hy + 0.1 * t, -hd * 0.5],
                    [0.0, hy - 0.4 * t, -hd * 1.3],
                    [0.6 * s, h * 0.06, -h * 0.32],
                    [1.6 * s, h * 0.03, -h * 0.44],
                ])
                .sample_evenly(48);
                let ws = self.girth;
                Some((path, Box::new(move |u: f32| 0.62 * ws * (1.0 - u).powf(0.9) + 0.03)))
            }
            Folk::Pigfolk => {
                let path = CatmullRom::centripetal(vec![
                    [0.0, hy + 0.05 * t, -hd * 0.9],
                    [0.15, hy + 0.12 * t, -hd * 1.15],
                    [-0.1, hy + 0.2 * t, -hd * 1.1],
                    [0.05, hy + 0.15 * t, -hd * 1.0],
                ])
                .sample_evenly(24);
                Some((path, Box::new(move |u: f32| 0.08 * s * (1.0 - u * 0.6))))
            }
            _ => {
                let (joints, radius) = self.jointed_tail()?;
                let end = joints.last().copied()?;
                let mut controls = joints;
                controls.push(end);
                let path = CatmullRom::centripetal(controls).sample_evenly(36);
                Some((path, radius))
            }
        }
    }

    /// The traits a seed rolled for the denizen parts ([`crate::parts`]): the horns' shape, a
    /// coat's marking, a nose ring, the mane. Meaningless for a race with none of them.
    pub fn traits(&self) -> crate::parts::Traits {
        crate::parts::Traits::roll(self.spec.seed)
    }

    /// The bone joints of a jointed tail (`tail_shape` in the race row) and its radius along it:
    /// the root behind the seat and one point per bone, each segment swinging a little less back
    /// from hanging, and the tip. `None` for a race without one.
    pub fn jointed_tail(&self) -> Option<(Vec<V3>, TailRadius)> {
        let shape = self.race().tail_shape?;
        let n = shape.bones.clamp(1, crate::parts::TAIL_BONES);
        let (hy, hd, t) = (self.hip_y, self.hip.half_depth, self.trunk_length);
        let seg = shape.length * hy / n as f32;
        let mut p = [0.0, hy + 0.06 * t, -0.95 * hd];
        let mut points = vec![p];
        for i in 0..n {
            // Leaving the rump backward and down, then hanging.
            let swing = 0.9 / (1.0 + 1.6 * i as f32) + shape.lift * (1.0 - 0.15 * i as f32).max(0.0) + shape.curve * i as f32;
            p = vec3::add(p, [0.0, -seg * swing.cos(), -seg * swing.sin()]);
            points.push(p);
        }
        let root = shape.radius * self.leg(Side::Left).radius;
        Some((points, Box::new(move |u: f32| root * (1.0 - 0.84 * u) + 0.02)))
    }

    /// The bones' rest joints of the jointed tail: where each bone starts.
    pub fn tail_joints(&self) -> Vec<V3> {
        self.jointed_tail().map(|(mut points, _)| {
            points.pop();
            points
        }).unwrap_or_default()
    }

    /// The tail's end of hair, if its shape ends in a tuft.
    fn tail_hair(&self) -> Option<MeshBuilder> {
        let shape = self.race().tail_shape?;
        if shape.end != crate::folk::TailEnd::Tuft {
            return None;
        }
        let (points, radius) = self.jointed_tail()?;
        let end = *points.last()?;
        let seg = vec3::distance(points[points.len() - 2], end);
        Some(crate::parts::tuft(end, seg, radius(1.0), self.traits().tuft, self.spec.seed))
    }

    /// The tail's spade, if its shape ends in one: a leaf-shaped blade, flat, on the last bone's line.
    fn tail_spade(&self) -> Option<MeshBuilder> {
        let shape = self.race().tail_shape?;
        if shape.end != crate::folk::TailEnd::Spade {
            return None;
        }
        let (points, radius) = self.jointed_tail()?;
        let end = *points.last()?;
        let dir = vec3::normalize_or(vec3::sub(end, points[points.len() - 2]), vec3::neg(vec3::Y));
        Some(crate::parts::spade(end, dir, radius(1.0), self.scale))
    }

    /// The foot of `side`'s leg if the race stands on its toes: the pastern, hoof and where they lie.
    pub fn hooved_foot(&self, side: Side) -> Option<crate::parts::Foot> {
        let g = self.race().legs?;
        let leg = self.leg(side);
        Some(crate::parts::digitigrade_foot(leg.joints[2], &g, leg.radius))
    }

    /// The line from the ankle joint to the sole a foot hangs along, for the skin weights: for
    /// folk down to the sole and out along the foot, for a hoofed leg along the pastern to the hoof.
    pub fn foot_line(&self, side: Side) -> Vec<V3> {
        let end = self.leg(side).joints[2];
        match self.hooved_foot(side) {
            Some(f) => vec![end, f.coronet, [f.coronet[0], 0.15, 0.5 * (f.heel + f.toe)]],
            None => {
                let sole = [end[0], 0.15, end[2]];
                vec![end, sole, vec3::add(sole, [0.0, 0.0, 0.148 * self.height * self.race().foot])]
            }
        }
    }

    fn tail_mesh(&self) -> MeshBuilder {
        match self.tail() {
            Some((path, radius)) => {
                let sides = if self.folk() == Folk::Lizardfolk { 18 } else { 8 };
                tube(&path, sides, &SweepOptions::default(), radius)
            }
            None => MeshBuilder::new(),
        }
    }

    /// Three claws at the end of each of a lizard's feet.
    fn claws_mesh(&self) -> MeshBuilder {
        let form = self.leg_form();
        let mut out = MeshBuilder::new();
        for side in Side::BOTH {
            let at = self.foot_placement(side);
            for i in [-1.0f32, 0.0, 1.0] {
                let x = form.foot_length * (0.88 - 0.05 * i.abs());
                let (w, _) = form.half_width(x);
                let base = [x, form.height(x) * 0.35, i * w * 0.55];
                let mut c = spike(0.07 * self.scale, 0.3 * self.scale, 8, 0.3, armory::geom::Shading::SMOOTH);
                c.place(base, vec3::normalize_or([1.0, -0.25, i * 0.25], vec3::X));
                at.apply(&mut c);
                out.append(&c);
            }
        }
        out
    }
}

/// A tail's radius by share of its length, root to tip ([`Body::tail`]).
pub type TailRadius = Box<dyn Fn(f32) -> f32>;

/// The largest grip radius a hand closes on (`data/constants.json`, `GRIP_MAX`).
pub const GRIP_MAX: f32 = 0.13;

/// A colour moved `k` of the way to white.
pub(crate) fn lighten(c: Rgb, k: f32) -> Rgb {
    Rgb(c.0 + (1.0 - c.0) * k, c.1 + (1.0 - c.1) * k, c.2 + (1.0 - c.2) * k)
}

/// An arm through shoulder, elbow and wrist (`mkArm`): the root starts inside the chest,
/// the radius swells over the upper arm, narrows at the elbow, fills the forearm and
/// tapers to the wrist.
#[allow(clippy::too_many_arguments)]
fn arm_limb(race: &Race, h: f32, ws: f32, fem: f32, mu: f32, sw: f32, shoulder_y: f32, side: Side, pose: ArmPose) -> Limb {
    let sg = side.sign();
    let ar = 0.5 * ws * race.limb * (1.0 - fem * 0.12) * (0.86 + 0.28 * mu);
    let (upper, fore) = (0.186 * h * race.arm, 0.152 * h * race.arm);
    let s = [sg * (sw - ar * 0.9), shoulder_y - ar * 0.55, 0.0];
    let ud = vec3::normalize_or([sg * pose.spread.sin(), -pose.spread.cos(), pose.forward], vec3::neg(vec3::Y));
    let e = vec3::add(s, vec3::scale(ud, upper));
    let fd = vec3::normalize_or(vec3::lerp(ud, vec3::normalize_or([sg * 0.12, -0.28, 1.0], vec3::Z), pose.bend), ud);
    let w = vec3::add(e, vec3::scale(fd, fore));
    let root = vec3::add(s, [-sg * ar * 1.15, -ar * 0.15, 0.0]);
    let controls = vec![root, s, vec3::lerp(s, e, 0.5), e, vec3::lerp(e, w, 0.5), w];
    // A deltoid cap over the shoulder, the upper arm full to the elbow, and a forearm that
    // fills out below the elbow before it tapers: the plate's forearm was a thin tube.
    Limb::new(side, root, [s, e, w], controls, ar, |ts, te| {
        vec![(0.0, 0.85), (ts, 1.06 + 0.04 * mu), (ts + 0.4 * (te - ts), 0.92 + 0.08 * mu), (te, 0.72), (te + 0.28 * (1.0 - te), 0.84 + 0.08 * mu), (1.0, 0.48)]
    })
}

/// A leg through hip, knee and ankle (`mkLeg`), its root high inside the hips, the knee a
/// little forward.
#[allow(clippy::too_many_arguments)]
fn leg_limb(race: &Race, s: f32, ws: f32, fem: f32, mu: f32, hip: &FormRing, ankle_y: f32, side: Side) -> Limb {
    let sg = side.sign();
    let lr = 0.78 * ws * race.limb * (0.95 + fem * 0.06) * (0.93 + 0.14 * mu);
    let hw = hip.half_width;
    let hj = [sg * hw * HIP_JOINT_X, hip.y - 0.05 * s, 0.0];
    let a = [sg * hw * 0.36, ankle_y, 0.0];
    // A leg on its toes folds: the knee well forward of the hip, higher up, and the hock (the
    // ankle joint) under the hip; the pastern and hoof lie beyond it ([`crate::parts`]).
    let k = match race.legs {
        Some(g) => [sg * hw * 0.41, a[1] + (hj[1] - a[1]) * g.knee_height, g.knee_forward * hj[1]],
        None => [sg * hw * 0.41, a[1] + (hj[1] - a[1]) * 0.48, 0.12 * s],
    };
    let root = vec3::add(hj, [0.0, s, 0.0]);
    // The thigh's top reaches out to the hips' own width, so wide hips flow into the thighs
    // instead of standing out over them as a ledge; then it tapers to the knee (the plate's stayed
    // even to mid-thigh). Its root, up inside the trunk, stays narrow so it never shows through
    // the waist.
    let top = (THIGH_TOP * hw - hj[0].abs()) / lr;
    Limb::new(side, root, [hj, k, a], vec![root, hj, k, a], lr, |th, tk| {
        vec![(0.0, 0.75), (th, top.max(0.8)), (th + 0.3 * (tk - th), (0.86 * top).max(0.68)), (tk, 0.6), (tk + 0.35 * (1.0 - tk), 0.66), (1.0, 0.42)]
    })
}

/// A garment form's fit to the body's pelvis, by height: see [`TorsoForm::fill`].
#[derive(Clone, Copy, Debug, PartialEq)]
struct GarmentFill {
    rows: [[f32; 3]; FILL_ROWS],
    floor: [f32; FILL_ROWS],
    side: [f32; FILL_ROWS],
}

impl Default for GarmentFill {
    fn default() -> Self {
        GarmentFill { rows: [[0.0; 3]; FILL_ROWS], floor: [0.0; FILL_ROWS], side: [0.0; FILL_ROWS] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn bodies() -> impl Iterator<Item = Body> {
        Folk::fitted().into_iter().flat_map(|f| [1u32, 1207, 40_401].map(move |s| Body::new(&BodySpec::roll(f, s))))
    }

    #[test]
    fn the_same_seed_builds_the_same_body() {
        for folk in Folk::fitted() {
            let a = Body::new(&BodySpec::roll(folk, 77)).skin_mesh();
            let b = Body::new(&BodySpec::roll(folk, 77)).skin_mesh();
            assert_eq!(a, b, "{folk:?}");
        }
    }

    #[test]
    fn a_human_is_about_seventeen_units_tall() {
        let body = Body::new(&BodySpec::typical(Folk::Human));
        let (lo, hi) = body.skin_mesh().bounds().unwrap();
        assert!(lo[1].abs() < 0.1, "soles on the ground: {lo:?}");
        // The skin stops at the neck until the head is built; the body's own height is the crown.
        assert!((body.height - 17.2).abs() < 17.2 * 0.05);
        assert!(hi[1] < body.height && hi[1] > body.chin_y, "{hi:?}");
        for seed in 0..50 {
            let h = Body::new(&BodySpec::roll(Folk::Human, seed)).height;
            assert!((h - 17.2).abs() <= 17.2 * 0.05 + 1e-4, "{h}");
        }
    }

    #[test]
    fn each_folk_follows_its_race_row() {
        let typical = |f: Folk| Body::new(&BodySpec::typical(f));
        let (human, dwarf, gnome, goblin, lizard, pig) =
            (typical(Folk::Human), typical(Folk::Dwarf), typical(Folk::Gnome), typical(Folk::Goblin), typical(Folk::Lizardfolk), typical(Folk::Pigfolk));
        for b in [&human, &dwarf, &gnome, &goblin, &lizard, &pig] {
            assert!((b.height - b.race().h).abs() < 1e-4);
            assert!((b.hip_y / b.height - b.race().leg).abs() < 1e-4);
        }
        // Dwarves and pigfolk are broad, goblins slight; dwarves short-legged; lizardfolk long-necked.
        assert!(dwarf.girth / dwarf.scale > human.girth / human.scale * 1.3);
        assert!(pig.girth / pig.scale > human.girth / human.scale * 1.3);
        assert!(goblin.girth / goblin.scale < human.girth / human.scale);
        assert!(dwarf.hip_y / dwarf.height < human.hip_y / human.height);
        assert!((lizard.chin_y - lizard.shoulder_y) / lizard.scale > (human.chin_y - human.shoulder_y) / human.scale * 1.4);
        // Goblin arms are long for their size.
        let reach = |b: &Body| b.arm(Side::Left).length() / b.height;
        assert!(reach(&goblin) > reach(&human) * 1.1);
        assert!(pig.belly.depth > human.belly.depth * 2.0);
    }

    #[test]
    fn the_shape_controls_move_the_body() {
        let spec = BodySpec::typical(Folk::Human);
        let shaped = |key: &str, value: f32| {
            let mut shape = spec.shape();
            shape.set(key, value);
            Body::new(&spec.clone().with_shape(shape))
        };
        let upper_arm = |b: &Body| b.arm(Side::Left).radius_at(0.5 * (b.arm(Side::Left).t_joints[0] + b.arm(Side::Left).t_joints[1]));
        let calf = |b: &Body| b.leg_form().calf * b.leg_form().shin_half_width;
        let (soft, strong) = (shaped("muscle", 0.0), shaped("muscle", 1.0));
        assert!(upper_arm(&strong) > upper_arm(&soft) * 1.2 && calf(&strong) > calf(&soft) * 1.1);
        assert!(strong.shoulder_half_width > soft.shoulder_half_width * 1.1 && strong.waist.half_width < soft.waist.half_width);
        let (stocky, slender) = (shaped("slender", 0.0), shaped("slender", 1.0));
        assert!(slender.waist.half_width < stocky.waist.half_width * 0.85 && slender.leg(Side::Left).radius < stocky.leg(Side::Left).radius * 0.85);
        assert!((slender.height - stocky.height).abs() < 1e-4);
        assert!(shaped("height", 1.1).height > shaped("height", 0.9).height * 1.2);
        // A full bust stands out in two lobes with a cleft between; a flat chest barely swells.
        let (flat, full) = (shaped("bust", 0.0), shaped("bust", 1.0));
        let front = |b: &Body, a: f32| b.trunk().point(a, b.bust.y)[2];
        let crest = FRONT + full.bust.pair;
        assert!(front(&full, crest) > front(&flat, crest) + 0.5, "{} {}", front(&full, crest), front(&flat, crest));
        assert!(front(&full, FRONT) < front(&full, crest) - 0.2, "a cleft");
        assert!(flat.bust.depth < 0.2 * flat.girth, "breast muscles, not a bust");
        // A round bust sits lower and fuller underneath than a high one (both full).
        let full_with = |round: f32| {
            let mut shape = spec.shape();
            shape.set("bust", 1.0);
            shape.set("round", round);
            Body::new(&spec.clone().with_shape(shape)).bust
        };
        let (high, round) = (full_with(0.0), full_with(1.0));
        assert!(round.y < high.y && round.fall > high.fall && round.power < high.power);
    }

    #[test]
    fn the_trunk_is_round_not_boxy() {
        let body = Body::new(&BodySpec::typical(Folk::Human));
        // A diagonal of the chest's section falls within 5% of an ellipse's (bulges aside).
        let (hw, hd) = body.trunk().at(body.waist_y);
        let a = FRONT + std::f32::consts::FRAC_PI_4;
        let radius = |exponent: f32| {
            let [x, z] = armory::build::form::section_point(a, hw, hd, exponent, 1.0);
            (x * x + z * z).sqrt()
        };
        assert!((radius(TRUNK_EXPONENT) / radius(1.0) - 1.0).abs() < 0.05, "{}", radius(TRUNK_EXPONENT) / radius(1.0));
    }

    #[test]
    fn arm_roots_are_inside_the_chest() {
        for body in bodies() {
            for side in Side::BOTH {
                let root = body.arm(side).root;
                let (p, _) = body.trunk_surface(if side == Side::Left { 0.0 } else { PI }, root[1], 0.0);
                assert!(root[0].abs() < p[0].abs() - 0.05, "{:?} {side:?}: root {root:?} skin {p:?}", body.folk());
            }
        }
    }

    #[test]
    fn every_part_validates() {
        for body in bodies() {
            let mesh = body.skin_mesh();
            for part in &mesh.parts {
                part.validate().unwrap_or_else(|e| panic!("{:?}: {e}", body.folk()));
                assert!(part.material.is_some());
            }
            let names: Vec<_> = mesh.parts.iter().map(|p| p.name).collect();
            for name in ["trunk", "arms", "hands", "legs", "feet"] {
                assert!(names.contains(&name), "{name} in {names:?}");
            }
            assert_eq!(names.contains(&"tail"), matches!(body.folk(), Folk::Lizardfolk | Folk::Pigfolk));
        }
    }

    #[test]
    fn the_trunk_is_closed_and_faces_out() {
        let body = Body::new(&BodySpec::typical(Folk::Human));
        let part = body.trunk_mesh().into_part("trunk", OutlineMode::Welded);
        let volume: f32 = part
            .indices
            .chunks(3)
            .map(|t| {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| part.positions[i as usize]);
                vec3::dot(a, vec3::cross(b, c)) / 6.0
            })
            .sum();
        assert!(volume > 10.0, "{volume}");
    }

    #[test]
    fn hands_sit_on_the_wrists_and_grips_in_the_fist() {
        let spec = BodySpec::typical(Folk::Human);
        let body = Body::posed(&spec, Pose { right: ArmPose { spread: 0.2, bend: 0.72, forward: 0.1, grip: Some(0.13) }, left: ArmPose::RELAXED });
        for side in Side::BOTH {
            let hand = body.hand_placement(side);
            assert_eq!(hand.is_mirrored(), side == Side::Left);
            // The back of the hand faces out from the body; the thumb forward or up.
            assert!(hand.z()[0] * side.sign() > 0.5);
            let thumb = hand.direction(vec3::neg(vec3::X));
            assert!(thumb[2] > 0.3 || thumb[1] > 0.3, "{side:?} thumb {thumb:?}");
        }
        let grip = body.grip(Side::Right);
        assert!(!grip.is_mirrored());
        let wrist = body.arm(Side::Right).joints[2];
        let palm = body.hand_form().palm_length;
        let d = vec3::distance(grip.origin, wrist);
        assert!(d > palm * 0.5 && d < palm * 1.6, "grip {d} from the wrist, palm {palm}");
        // A one-handed weapon held forward stands up out of the fist.
        assert!(grip.y()[1] > 0.6, "{:?}", grip.y());
    }

    #[test]
    fn forms_match_the_body() {
        for body in bodies() {
            let torso = body.torso_form();
            let place = body.torso_placement();
            assert!((place.point([0.0, torso.chest.y, 0.0])[1] - body.chest_y).abs() < 1e-4);
            let head = body.head_form();
            assert!((place.point(head.centre)[1] - body.head.centre[1]).abs() < 1e-4);
            // The garment form's chest is the skin's at each breast, and bridges the cleft
            // between them.
            let crest = FRONT + body.bust.pair;
            let skin = body.trunk().point(crest, body.chest_y);
            let form = torso.profile().point(crest, torso.chest.y);
            assert!((skin[2] - form[2]).abs() < 0.02, "{skin:?} {form:?}");
            let (skin, form) = (body.trunk().point(FRONT, body.chest_y), torso.profile().point(FRONT, torso.chest.y));
            assert!(form[2] >= skin[2] - 1e-4, "cloth over the cleft {form:?}, skin {skin:?}");
            let lower = body.lower_body_form();
            assert!(lower.crotch_height < lower.waist_height && lower.leg.knee_height < lower.crotch_height);
            let foot = body.foot_placement(Side::Left);
            assert!(!foot.is_mirrored() && foot.x()[2] > 0.9);
        }
    }
}
