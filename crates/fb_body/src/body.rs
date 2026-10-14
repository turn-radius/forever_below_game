//! [`Body`]: a folk's proportions from its race row and its [`BodySpec`] (the plate
//! prototype's `bodyDims`), the Armory forms sized to it, the anchors gear hangs from, and
//! its skin.
//!
//! Body space: feet on y = 0, the body centred on the Y axis, the face toward +Z, the
//! wearer's left toward +X, 1 unit = 10 cm. The Armory's torso and head forms use the
//! torso's space instead, the same axes with the waist at y = 0
//! ([`Body::torso_placement`] moves them onto the body).

use std::f32::consts::TAU;

use armory::build::boot::foot_mesh;
use armory::build::form::{BACK, Bulge, FRONT, FormRing, HandForm, HeadForm, LegForm, LowerBodyForm, TorsoForm, TorsoProfile};
use armory::build::hafted::smoothstep;
use armory::fabric::{PatternKind, SurfacePattern};
use armory::geom::vec3::{self, V3};
use armory::geom::{CatmullRom, GridOptions, MeshBuilder, OutlineMode, SweepOptions, spike, stitch_grid, tube};
use armory::{ItemMesh, MaterialKind, Rgb};

use crate::folk::{Folk, Race};
use crate::hand::{HandPose, grip_centre, hand_meshes};
use crate::limb::{Limb, Side};
use crate::place::Placement;
use crate::spec::BodySpec;

/// The superellipse exponent of the trunk's sections (`tSurf`): a little boxy, carved.
pub const TRUNK_EXPONENT: f32 = 0.77;
/// The back's depth relative to the front (`tSurf`'s 0.92).
pub const TRUNK_BACK: f32 = 0.92;

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
    /// Right then left ([`Side::BOTH`]).
    pub arms: [Limb; 2],
    pub legs: [Limb; 2],
    skin: TorsoProfile,
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
        let ws = s.powf(0.72) * race.girth * (0.88 + spec.frame * 0.3);
        let fem = spec.figure;
        let head_r = race.head_r * s.powf(0.35);
        let half_height = head_r * race.hh;
        let head_y = h - half_height;
        let chin_y = head_y - half_height;
        let shoulder_y = chin_y - race.neck * s * 0.55;
        let hip_y = h * race.leg;
        let t = shoulder_y - hip_y;
        let sw = 2.1 * ws * (1.07 - fem * 0.15);
        let hip = FormRing::new(hip_y, 1.66 * ws * (0.95 + fem * 0.13), 1.12 * ws);
        let waist_y = hip_y + 0.3 * t;
        let chest_y = hip_y + 0.7 * t;
        let waist = FormRing::new(waist_y, 1.42 * ws * (1.0 - fem * 0.06), 0.98 * ws);
        let chest = FormRing::new(chest_y, 1.72 * ws, 1.1 * ws);
        let neck_radius = 0.42 * ws * race.neck_r;
        let belly = Bulge { y: waist_y, spread: 0.3 * t, depth: race.belly * (0.3 + spec.frame * 0.9) * ws, sharpness: 2, toward: FRONT };
        // The plate swells a bust only past figure 0.55; the breast muscles' slight swell
        // under it and the seat are our own, so the trunk is not flat front and back.
        let bust_depth = if fem > 0.55 { (fem - 0.55) * 0.5 * ws } else { 0.0 };
        let bust = Bulge { y: chest_y, spread: 0.14 * t, depth: 0.05 * ws + bust_depth, sharpness: 4, toward: FRONT };
        let seat = Bulge { y: hip_y - 0.04 * t, spread: 0.15 * t, depth: 0.14 * ws * (1.0 + 0.4 * fem), sharpness: 2, toward: BACK };
        let ankle_y = 0.048 * h;
        let head = HeadForm {
            centre: [0.0, head_y, 0.12 * s],
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
                FormRing::new(hip_y - 0.14 * t, hip.half_width * 0.58, hip.half_depth * 0.66),
                hip,
                waist,
                chest,
                FormRing::new(shoulder_y - 0.05 * t, sw * 0.95, chest.half_depth * 0.88),
                FormRing::new(shoulder_y + 0.05 * t, sw * 0.62, chest.half_depth * 0.66),
                FormRing::new(shoulder_y + 0.11 * t, neck_radius * 1.2, neck_radius * 1.1),
                FormRing::new(shoulder_y + 0.17 * t, neck_radius, neck_radius * 0.95),
            ],
            TRUNK_EXPONENT,
            TRUNK_BACK,
            vec![belly, bust, seat],
        );
        let arm = |side: Side| arm_limb(race, h, ws, fem, sw, shoulder_y, side, pose.arm(side));
        let leg = |side: Side| leg_limb(race, s, ws, fem, &hip, ankle_y, side);
        Body {
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
            arms: [arm(Side::Right), arm(Side::Left)],
            legs: [leg(Side::Right), leg(Side::Left)],
            skin,
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
        let t = self.trunk_length;
        let s = self.scale;
        let w = self.waist_y;
        let sw = self.shoulder_half_width;
        let (hip, chest) = (self.hip, self.chest);
        let below = |y: f32, k: f32| FormRing::new(y - w, hip.half_width * k, hip.half_depth * (k + 0.04));
        TorsoForm {
            neck: FormRing::new(self.shoulder_y + 0.11 * t - w, self.neck_radius * 1.2, self.neck_radius * 1.1),
            shoulder_top: FormRing::new(self.shoulder_y + 0.05 * t - w, sw * 0.62, chest.half_depth * 0.66),
            shoulder: FormRing::new(self.shoulder_y - 0.05 * t - w, sw * 0.95, chest.half_depth * 0.88),
            chest: chest.raised(-w),
            waist: self.waist.raised(-w),
            hip: FormRing::new(hip.y - w, hip.half_width * 1.01, hip.half_depth * 1.03),
            thigh: below(self.hip_y - 1.2 * s, 1.035),
            knee: below(self.leg(Side::Left).joints[1][1], 1.04),
            shin: below(self.ankle_y + 0.5 * s, 1.04),
            exponent: TRUNK_EXPONENT,
            back: TRUNK_BACK,
            belly: self.belly.raised(-w),
            bust: self.bust.raised(-w),
            seat: self.seat.raised(-w),
        }
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
        LowerBodyForm {
            torso: self.torso_form(),
            leg: self.leg_form(),
            waist_height: self.waist_y,
            crotch_height: self.hip_y - 0.14 * self.trunk_length,
            thigh_half_width: (self.hip.half_width * 0.47).max(leg.radius * 0.9),
            thigh_half_depth: leg.radius * 1.08,
            stance: leg.joints[2][0].abs(),
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
            let (mut h, mut n) = hand_meshes(&self.hand_form(), &self.hand_pose(side));
            let at = self.hand_placement(side);
            at.apply(&mut h);
            at.apply(&mut n);
            hands.append(&h);
            nails.append(&n);
        }
        add(arms, "arms", skin, true);
        add(hands, "hands", skin, true);
        add(nails, "nails", lighten(skin, 0.22), false);
        let mut legs = MeshBuilder::new();
        let mut feet = MeshBuilder::new();
        for side in Side::BOTH {
            legs.append(&self.leg_mesh(side));
            let mut f = foot_mesh(&self.leg_form(), 0.0);
            self.foot_placement(side).apply(&mut f);
            feet.append(&f);
        }
        add(legs, "legs", skin, true);
        // Pigfolk walk on darker trotters (`buildBody`: the skin at 0.62).
        add(feet, "feet", if self.folk() == Folk::Pigfolk { skin.scaled(0.62) } else { skin }, true);
        add(self.tail_mesh(), "tail", skin, true);
        let mut out = ItemMesh { parts };
        if self.folk() == Folk::Lizardfolk {
            let claws = self.claws_mesh().into_part("claws", OutlineMode::Welded).with_material(MaterialKind::Bone);
            out.parts.push(claws);
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
        let [th, tk] = leg.t_joints;
        let calf_t = tk + 0.3 * (1.0 - tk);
        leg.mesh(28, vec3::Z, None, |t, a| {
            // Angle 0 faces the front (+Z), PI the back.
            let front = a.cos();
            let calf = 0.1 * (-((t - calf_t) / 0.12).powi(2)).exp() * (-front).max(0.0).powi(2);
            let knee = 0.05 * (-((t - tk) / 0.05).powi(2)).exp() * front.max(0.0).powi(3);
            // The thigh is a little deeper front to back than across, below the seat.
            let thigh = 0.05 * smoothstep(0.0, th, t) * (1.0 - smoothstep(th, tk, t)) * (front * front - 0.5);
            leg.radius_at(t) * (1.0 + calf + knee + thigh)
        })
    }

    /// A lizard's tail sweeping down and back from the base of the spine, or a pig's curl.
    fn tail_mesh(&self) -> MeshBuilder {
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
                tube(&path, 18, &SweepOptions::default(), |u| 0.62 * ws * (1.0 - u).powf(0.9) + 0.03)
            }
            Folk::Pigfolk => {
                let path = CatmullRom::centripetal(vec![
                    [0.0, hy + 0.05 * t, -hd * 0.9],
                    [0.15, hy + 0.12 * t, -hd * 1.15],
                    [-0.1, hy + 0.2 * t, -hd * 1.1],
                    [0.05, hy + 0.15 * t, -hd * 1.0],
                ])
                .sample_evenly(24);
                tube(&path, 8, &SweepOptions::default(), |u| 0.08 * s * (1.0 - u * 0.6))
            }
            _ => MeshBuilder::new(),
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
fn arm_limb(race: &Race, h: f32, ws: f32, fem: f32, sw: f32, shoulder_y: f32, side: Side, pose: ArmPose) -> Limb {
    let sg = side.sign();
    let ar = 0.5 * ws * race.limb * (1.0 - fem * 0.12);
    let (upper, fore) = (0.186 * h * race.arm, 0.152 * h * race.arm);
    let s = [sg * (sw - ar * 0.9), shoulder_y - ar * 0.55, 0.0];
    let ud = vec3::normalize_or([sg * pose.spread.sin(), -pose.spread.cos(), pose.forward], vec3::neg(vec3::Y));
    let e = vec3::add(s, vec3::scale(ud, upper));
    let fd = vec3::normalize_or(vec3::lerp(ud, vec3::normalize_or([sg * 0.12, -0.28, 1.0], vec3::Z), pose.bend), ud);
    let w = vec3::add(e, vec3::scale(fd, fore));
    let root = vec3::add(s, [-sg * ar * 1.15, -ar * 0.15, 0.0]);
    let controls = vec![root, s, vec3::lerp(s, e, 0.5), e, vec3::lerp(e, w, 0.5), w];
    Limb::new(side, root, [s, e, w], controls, ar, |ts, te| {
        vec![(0.0, 0.85), (ts, 1.06), (ts + 0.35 * (te - ts), 0.88), (te, 0.65), (te + 0.35 * (1.0 - te), 0.68), (1.0, 0.46)]
    })
}

/// A leg through hip, knee and ankle (`mkLeg`), its root high inside the hips, the knee a
/// little forward.
fn leg_limb(race: &Race, s: f32, ws: f32, fem: f32, hip: &FormRing, ankle_y: f32, side: Side) -> Limb {
    let sg = side.sign();
    let lr = 0.78 * ws * race.limb * (0.95 + fem * 0.06);
    let hw = hip.half_width;
    let hj = [sg * hw * 0.46, hip.y - 0.05 * s, 0.0];
    let a = [sg * hw * 0.36, ankle_y, 0.0];
    let k = [sg * hw * 0.41, a[1] + (hj[1] - a[1]) * 0.48, 0.12 * s];
    let root = vec3::add(hj, [0.0, s, 0.0]);
    Limb::new(side, root, [hj, k, a], vec![root, hj, k, a], lr, |th, tk| {
        vec![(0.0, 1.0), (th, 1.0), (th + 0.35 * (tk - th), 0.86), (tk, 0.58), (tk + 0.35 * (1.0 - tk), 0.64), (1.0, 0.4)]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn bodies() -> impl Iterator<Item = Body> {
        Folk::ALL.into_iter().flat_map(|f| [1u32, 1207, 40_401].map(move |s| Body::new(&BodySpec::roll(f, s))))
    }

    #[test]
    fn the_same_seed_builds_the_same_body() {
        for folk in Folk::ALL {
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
            // The garment form's chest is the skin's chest.
            let skin = body.trunk().point(FRONT, body.chest_y);
            let form = torso.profile().point(FRONT, torso.chest.y);
            assert!((skin[2] - form[2]).abs() < 0.02, "{skin:?} {form:?}");
            let lower = body.lower_body_form();
            assert!(lower.crotch_height < lower.waist_height && lower.leg.knee_height < lower.crotch_height);
            let foot = body.foot_placement(Side::Left);
            assert!(!foot.is_mirrored() && foot.x()[2] > 0.9);
        }
    }
}
