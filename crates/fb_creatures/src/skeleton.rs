//! Skeletons: [`SkeletonState`] from a seed, and its [`Rig`] of named bones.
//!
//! The dead who walk the halls: a human's bones, built from the Armory's bone primitives
//! ([`armory::build::bone`]) on the joints of an `fb_body` human, so a skeleton stands as tall
//! as a person and every joint sits where the body's does. That is what lets gear be fitted to
//! it as to a body (`fb_body`'s worn and held fitting): the rig's rest pose is the body's
//! relaxed pose, and [`SkeletonDims::body`] is the very [`Body`] the bones were laid out on.
//!
//! The rig is eighteen bones: pelvis (the root), spine (the lumbar vertebrae), ribcage (the
//! thoracic vertebrae, ribs, breastbone, collarbones and shoulder blades), neck, skull, jaw, and
//! for each arm an upper arm, forearm and hand, and for each leg a thigh, shin and foot. In the
//! rest pose every bone's rotation is the identity and its origin sits on its joint, so a
//! bone's rest placement is just the offset from its parent's joint, and a pose is a rotation
//! about a joint in body axes (+X the creature's left, +Y up, +Z the way it faces). Meshes are
//! built between the joints, not along an axis, so the arms' slight spread and the knees' slight
//! forward bend are in the bones.
//!
//! Hands and feet are built in `fb_body`'s frames (the hand's `y` along the forearm and `z` the
//! back of the hand, the foot turned out 0.12 radians), so [`SkeletonDims::hand_frame`] and
//! [`SkeletonDims::foot_frame`] are the placements `fb_body` fits a worn glove, a boot or a held
//! weapon to, up to the mirror a left hand takes.
//!
//! Seeded variation: height and build, the bones' colour from bleached to yellowed to dark,
//! cracks across the skull and the long bones, the pits of age, and missing pieces: the jaw, ribs,
//! fingers and teeth. Our own throughout; the proportions follow a human's.

use armory::build::bone::{self, Foot, Hand, LongBone, Pelvis, Ribcage, SOCKET, SkullSpec, Vertebra};
use armory::geom::vec3::{self, V3};
use armory::geom::{MeshBuilder, SweepOptions, spike, tube, uv_sphere};
use armory::{MaterialKind, Part, Rgb};
use fb_body::{Body, BodySpec, Folk, HandPose, Placement, Side};
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::Species;
use crate::rat::{mix, part};
use crate::rig::Rig;
use crate::shape::keyed;

/// Mixed into a skeleton's seed so it does not roll the same numbers as a body of that seed.
const SKELETON_SALT: u32 = 0x5ce1_e704;

/// Bone indices in a skeleton's [`Rig`].
pub mod index {
    pub const PELVIS: usize = 0;
    pub const SPINE: usize = 1;
    pub const RIBCAGE: usize = 2;
    pub const NECK: usize = 3;
    pub const SKULL: usize = 4;
    pub const JAW: usize = 5;
    /// The right arm's upper arm; the forearm and hand follow it. The left arm's three come
    /// after the right's.
    pub const ARMS: usize = 6;
    /// The right leg's thigh; the shin and foot follow it, then the left leg's three.
    pub const LEGS: usize = 12;
    pub const COUNT: usize = 18;
}

/// Index of the first bone (upper arm or thigh) of a limb, in [`Side::BOTH`] order.
fn side_index(side: Side) -> usize {
    match side {
        Side::Right => 0,
        Side::Left => 1,
    }
}

/// The bone index of `side`'s upper arm.
pub fn arm_bone(side: Side) -> usize {
    index::ARMS + 3 * side_index(side)
}

/// The bone index of `side`'s thigh.
pub fn leg_bone(side: Side) -> usize {
    index::LEGS + 3 * side_index(side)
}

/// Ribs a skeleton has on a side.
pub const RIBS: usize = 12;

/// One skeleton, every choice the seed makes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkeletonState {
    pub seed: u32,
    /// Height as a share of a typical human's, 0.93..1.07.
    pub stature: f32,
    /// 0 gaunt to 1 heavy: the thickness of the bones, the width of the ribs.
    pub build: f32,
    /// 0 narrow hips to 1 wide.
    pub figure: f32,
    /// The bones' colour, 0 bleached to 0.5 yellowed to 1 dark.
    pub tone: f32,
    /// How pitted the bones are, 0..=1.
    pub wear: f32,
    /// Cracks across the bones.
    pub cracks: u32,
    /// How heavy the skull's brow and jaw are, 0..=1.
    pub brow: f32,
    pub jaw_heft: f32,
    /// How far the fingers rest curled, 0..=1.
    pub curl: f32,
    /// The jaw is gone.
    pub jaw_gone: bool,
    /// Chance each tooth is gone.
    pub teeth_gone: f32,
    /// Ribs gone, a bit each (the top rib is bit 0), for the right side then the left.
    pub ribs_gone: [u16; 2],
    /// Fingers gone, a bit each (the thumb is bit 0, then four fingers), for the right hand
    /// then the left.
    pub fingers_gone: [u8; 2],
}

impl SkeletonState {
    /// A skeleton rolled from `seed`: the same seed always gives the same skeleton.
    pub fn roll(seed: u32) -> SkeletonState {
        let mut rng = Mulberry32::new(seed ^ SKELETON_SALT);
        let stature = rng.range_f32(0.93, 1.07);
        let build = rng.next_f64() as f32;
        let figure = rng.next_f64() as f32;
        // Bleached 3 in 10, yellowed 4 in 10, dark 3 in 10.
        let tone = match rng.int(0, 9) {
            0..=2 => rng.range_f32(0.0, 0.2),
            3..=6 => rng.range_f32(0.3, 0.6),
            _ => rng.range_f32(0.75, 1.0),
        };
        let wear = rng.next_f64() as f32;
        let cracks = rng.int(0, 5) + if rng.chance(0.2) { rng.int(2, 5) } else { 0 };
        let brow = rng.next_f64() as f32;
        let jaw_heft = rng.next_f64() as f32;
        let curl = rng.range_f32(0.1, 0.4);
        let jaw_gone = rng.chance(0.13);
        let teeth_gone = if rng.chance(0.3) { rng.range_f32(0.2, 0.6) } else { rng.range_f32(0.0, 0.15) };
        let mut ribs_gone = [0u16; 2];
        for side in &mut ribs_gone {
            for rib in 0..RIBS {
                // Floating ribs go first, then the low ones.
                let chance = if rib >= 10 { 0.16 } else { 0.05 };
                if rng.chance(chance) {
                    *side |= 1 << rib;
                }
            }
        }
        let mut fingers_gone = [0u8; 2];
        for hand in &mut fingers_gone {
            for digit in 0..5 {
                if rng.chance(0.07) {
                    *hand |= 1 << digit;
                }
            }
        }
        SkeletonState { seed, stature, build, figure, tone, wear, cracks, brow, jaw_heft, curl, jaw_gone, teeth_gone, ribs_gone, fingers_gone }
    }

    pub fn species(&self) -> Species {
        Species::Skeleton
    }

    /// The bones' colour: bleached, yellowed or dark by [`SkeletonState::tone`]. Our own picks.
    pub fn bone_colour(&self) -> Rgb {
        const BLEACHED: Rgb = Rgb::hex(0xe9e2d0);
        const YELLOWED: Rgb = Rgb::hex(0xcdb98c);
        const DARK: Rgb = Rgb::hex(0x73614a);
        if self.tone < 0.5 { mix(BLEACHED, YELLOWED, self.tone / 0.5) } else { mix(YELLOWED, DARK, (self.tone - 0.5) / 0.5) }
    }

    /// The human body its bones are laid out on: stature and frame from the skeleton's, the
    /// rest of the figure its own. Relaxed arms.
    pub fn body(&self) -> Body {
        Body::new(&self.spec())
    }

    /// The spec of [`SkeletonState::body`], for fitting held gear, which poses the arms.
    pub fn spec(&self) -> BodySpec {
        let mut spec = BodySpec::roll(Folk::Human, self.seed ^ SKELETON_SALT);
        spec.stature = self.stature;
        spec.frame = 0.12 + 0.7 * self.build;
        spec.figure = self.figure;
        spec
    }

    /// The joints and measures the bones and poses are built from.
    pub fn dims(&self) -> SkeletonDims {
        SkeletonDims::new(self)
    }

    /// Crown to soles, world units.
    pub fn height(&self) -> f32 {
        self.dims().body.height
    }

    /// How far it reaches across the floor from its centre at rest: a hand hanging at its side
    /// or a foot out in front.
    pub fn radius(&self) -> f32 {
        let d = self.dims();
        d.arms.iter().map(|a| a[2][0].abs()).fold(0.0, f32::max).max(d.ankles[0][2].abs() + d.foot_length)
    }

    /// The size its body is drawn at: its SRD size.
    pub fn drawn_size(&self) -> crate::Size {
        Species::Skeleton.stat_block().size
    }

    /// Its bones and meshes, standing at rest.
    pub fn build(&self) -> Rig {
        build(self, &[None, None])
    }

    /// [`SkeletonState::build`] with the hands that hold something closed into `grips` (right
    /// then left): the fingers lie where a gripping `fb_body` hand's do, so what is held sits in
    /// them. A hand with no grip rests open as ever.
    pub fn build_gripping(&self, grips: &[Option<HandPose>; 2]) -> Rig {
        build(self, grips)
    }
}

/// The joints and measures of a skeleton's body, in body space (soles on y = 0, face +Z, the
/// creature's left +X). The bones and the poses both read these.
#[derive(Clone, Debug)]
pub struct SkeletonDims {
    pub body: Body,
    /// Shoulder, elbow and wrist for the right arm then the left.
    pub arms: [[V3; 3]; 2],
    /// Hip, knee and ankle for the right leg then the left.
    pub legs: [[V3; 3]; 2],
    /// The ankles at rest (the legs' third joints).
    pub ankles: [V3; 2],
    /// Where each bone's origin sits at rest, in body space, in [`index`] order.
    pub origins: [V3; index::COUNT],
    /// The spine's front-to-back place by height.
    spine: Vec<(f32, f32)>,
    /// The lengths of each leg's thigh and shin in the walking plane (YZ).
    pub thigh: f32,
    pub shin: f32,
    pub foot_length: f32,
    /// The jaw's hinge in body space.
    pub hinge: V3,
    /// Heights of the first lumbar and twelfth thoracic vertebrae, the seventh cervical and the atlas.
    pub lumbar_y: f32,
    pub thoracic_y: [f32; 2],
    pub cervical_y: [f32; 2],
}

impl SkeletonDims {
    fn new(state: &SkeletonState) -> SkeletonDims {
        let body = state.body();
        let arm_of = |s: Side| body.arm(s).joints;
        let leg_of = |s: Side| body.leg(s).joints;
        let arms = [arm_of(Side::Right), arm_of(Side::Left)];
        let legs = [leg_of(Side::Right), leg_of(Side::Left)];
        let ankles = [legs[0][2], legs[1][2]];
        let hip_y = legs[0][0][1];
        let hd = body.chest.half_depth;
        let head = body.head;
        let atlas_y = head.centre[1] - 0.3 * head.half_height;
        let c7_y = head.neck_base - 0.1;
        // Lumbar column from just over the sacrum to the lower ribs, thoracic to the shoulders.
        let lumbar_y = hip_y + 0.5;
        let t12_y = lumbar_y + 5.0 * 0.37;
        let t1_y = body.shoulder_y - 0.1;
        let spine = vec![
            (lumbar_y - 0.5, -0.5 * hd),
            (lumbar_y + 0.9, -0.33 * hd),
            (t12_y, -0.42 * hd),
            (0.5 * (t12_y + t1_y), -0.55 * hd),
            (t1_y, -0.42 * hd),
            (c7_y, -0.32 * hd),
            (atlas_y, head.centre[2] - 0.26 * head.half_depth),
        ];
        let at = |y: f32| [0.0, y, keyed(&spine, y)];
        let hinge = [0.0, head.centre[1] - 0.04 * head.half_height, head.centre[2] - 0.2 * head.half_depth];
        let mut origins = [vec3::ZERO; index::COUNT];
        origins[index::PELVIS] = [0.0, hip_y, 0.0];
        origins[index::SPINE] = at(lumbar_y);
        origins[index::RIBCAGE] = at(t12_y);
        origins[index::NECK] = at(c7_y);
        origins[index::SKULL] = at(atlas_y);
        origins[index::JAW] = hinge;
        for (k, side) in Side::BOTH.into_iter().enumerate() {
            let a = arms[k];
            for j in 0..3 {
                origins[arm_bone(side) + j] = a[j];
            }
            let l = legs[k];
            for j in 0..3 {
                origins[leg_bone(side) + j] = l[j];
            }
        }
        let planar = |a: V3, b: V3| (b[1] - a[1]).hypot(b[2] - a[2]);
        let thigh = planar(legs[0][0], legs[0][1]);
        let shin = planar(legs[0][1], legs[0][2]);
        let foot_length = 0.148 * body.height * body.race().foot;
        SkeletonDims {
            body,
            arms,
            legs,
            ankles,
            origins,
            spine,
            thigh,
            shin,
            foot_length,
            hinge,
            lumbar_y,
            thoracic_y: [t12_y, t1_y],
            cervical_y: [c7_y, atlas_y],
        }
    }

    /// The spine's centre at height `y`.
    pub fn spine_at(&self, y: f32) -> V3 {
        [0.0, y, keyed(&self.spine, y)]
    }

    /// The spine's direction (up it) at height `y`.
    pub fn spine_up(&self, y: f32) -> V3 {
        let dz = (keyed(&self.spine, y + 0.05) - keyed(&self.spine, y - 0.05)) / 0.1;
        vec3::normalize_or([0.0, 1.0, dz], vec3::Y)
    }

    /// Where bone `bone`'s origin sits on its parent's origin at rest: a [`Rig`] bone's rest
    /// offset.
    pub fn offset(&self, bone: usize) -> V3 {
        match parent_of(bone) {
            Some(p) => vec3::sub(self.origins[bone], self.origins[p]),
            None => self.origins[bone],
        }
    }

    /// The frame `fb_body` fits a hand to, on the skeleton's wrist: `y` along the forearm, `z`
    /// the back of the hand, `x` across it, always right handed. [`Body::hand_placement`] for the
    /// right hand; for the left, the same with its `x` flipped back (it mirrors).
    pub fn hand_frame(&self, side: Side) -> Placement {
        let p = self.body.hand_placement(side);
        let (y, z) = (p.y(), p.z());
        Placement::new(self.arms[side_index(side)][2], vec3::cross(y, z), y, z)
    }

    /// The frame `fb_body` fits a foot to: [`Body::foot_placement`], toe turned out a little.
    pub fn foot_frame(&self, side: Side) -> Placement {
        self.body.foot_placement(side)
    }
}

/// The parent of bone `bone` in the rig.
pub fn parent_of(bone: usize) -> Option<usize> {
    use index::*;
    match bone {
        PELVIS => None,
        SPINE => Some(PELVIS),
        RIBCAGE => Some(SPINE),
        NECK => Some(RIBCAGE),
        SKULL => Some(NECK),
        JAW => Some(SKULL),
        b if (ARMS..ARMS + 6).contains(&b) => {
            if (b - ARMS).is_multiple_of(3) { Some(RIBCAGE) } else { Some(b - 1) }
        }
        b => {
            if (b - LEGS).is_multiple_of(3) { Some(PELVIS) } else { Some(b - 1) }
        }
    }
}

/// The names of the bones, in [`index`] order.
pub const NAMES: [&str; index::COUNT] = [
    "pelvis",
    "spine",
    "ribcage",
    "neck",
    "skull",
    "jaw",
    "upper_arm_right",
    "forearm_right",
    "hand_right",
    "upper_arm_left",
    "forearm_left",
    "hand_left",
    "thigh_right",
    "shin_right",
    "foot_right",
    "thigh_left",
    "shin_left",
    "foot_left",
];

/// What the build carries along: the bone's colour and how it is weathered.
struct Kit {
    tone: Rgb,
    seed: u32,
    pits: f32,
}

impl Kit {
    /// A bone's main surface: pitted a little, in the bone colour.
    fn bone(&self, mut mesh: MeshBuilder, salt: u32) -> Part {
        bone::weather(&mut mesh, self.seed ^ salt, self.pits);
        part(mesh, "bone", MaterialKind::Bone, self.tone, true)
    }

    /// A bone left smooth (a foot, which must rest on the floor exactly).
    fn smooth(&self, mesh: MeshBuilder) -> Part {
        part(mesh, "bone", MaterialKind::Bone, self.tone, true)
    }

    fn teeth(&self, mesh: MeshBuilder) -> Part {
        part(mesh, "teeth", MaterialKind::Bone, mix(self.tone, Rgb::hex(0xf3eedc), 0.45), false)
    }

    fn cracks(&self, mesh: MeshBuilder) -> Part {
        part(mesh, "cracks", MaterialKind::Bone, mix(self.tone, SOCKET, 0.82), false)
    }

    fn dark(&self, mesh: MeshBuilder) -> Part {
        part(mesh, "sockets", MaterialKind::Bone, SOCKET, false)
    }
}

/// A part, or none when its mesh is empty.
fn some(parts: &mut Vec<Part>, mesh: MeshBuilder, make: impl FnOnce(MeshBuilder) -> Part) {
    if !mesh.is_empty() {
        parts.push(make(mesh));
    }
}

/// A crack on a long bone from `a` to `b`, wandering along it.
fn bone_crack(rng: &mut Mulberry32, a: V3, b: V3, shape: &LongBone) -> MeshBuilder {
    let axis = vec3::sub(b, a);
    let length = vec3::length(axis);
    let dir = vec3::scale(axis, 1.0 / length.max(1e-4));
    let n1 = vec3::normalize_or(vec3::cross(dir, vec3::X), vec3::Z);
    let n2 = vec3::cross(dir, n1);
    let surface = |u: f32, v: f32| {
        let t = u.clamp(0.1, 0.9);
        let r = shape.radius_at(t) * 1.0;
        let angle = v * 4.0;
        vec3::add(vec3::add(a, vec3::scale(dir, t * length)), vec3::add(vec3::scale(n1, angle.cos() * r), vec3::scale(n2, angle.sin() * r)))
    };
    let start = [rng.range_f32(0.2, 0.5), rng.range_f32(0.0, 1.5)];
    let heading = rng.range_f32(-0.6, 0.6);
    let steps = rng.int(6, 11) as usize;
    bone::crack_walk(rng, &surface, start, heading, steps, 0.09, 0.026)
}

fn build(state: &SkeletonState, grips: &[Option<HandPose>; 2]) -> Rig {
    let d = state.dims();
    let body = &d.body;
    let mut rng = Mulberry32::new(state.seed ^ SKELETON_SALT ^ 0xB0D7);
    let kit = Kit { tone: state.bone_colour(), seed: state.seed, pits: 0.014 + 0.026 * state.wear };
    let girth = 0.88 + 0.26 * state.build;
    // Cracks go on the skull and on long bones, by the roll.
    let mut skull_cracks = 0usize;
    let mut limb_cracks = [0u32; 6];
    for _ in 0..state.cracks {
        match rng.int(0, 9) {
            0..=2 => skull_cracks += 1,
            k => limb_cracks[(k - 3) as usize % 6] += 1,
        }
    }
    let mut meshes: Vec<Vec<Part>> = (0..index::COUNT).map(|_| Vec::new()).collect();
    // The cracks on each bone, in body space, laid as a dark part of their own.
    let mut scratches: Vec<MeshBuilder> = (0..index::COUNT).map(|_| MeshBuilder::new()).collect();
    let shift = |mesh: &mut MeshBuilder, bone: usize| {
        mesh.translate(vec3::neg(d.origins[bone]));
    };

    // ── Pelvis ──
    {
        let hip = d.legs[0][0];
        let hip_ring = body.hip;
        let spec = Pelvis {
            hip_x: hip[0].abs(),
            crest_x: 0.76 * hip_ring.half_width + 0.04 * state.figure,
            crest_y: 1.2 + 0.2 * state.figure,
            back_z: d.spine_at(hip[1] + 0.3)[2] * 1.05,
            front_z: 0.55,
            drop: 1.1,
            thickness: 0.085 * girth,
        };
        let mut mesh = bone::pelvis(&spec);
        // Pelvis space is centred between the sockets; the bone's origin is exactly that.
        meshes[index::PELVIS] = vec![kit.bone(std::mem::take(&mut mesh), 11)];
    }

    // ── Spine: five lumbar vertebrae ──
    {
        let mut mesh = MeshBuilder::new();
        for k in 0..5 {
            let y = d.lumbar_y + 0.37 * k as f32 + 0.15;
            let r = (0.24 + 0.012 * (4 - k) as f32) * girth;
            let mut v = bone::vertebra(&Vertebra { radius: r, height: 0.3, spine: 0.3, transverse: 0.45 });
            v.place_basis(d.spine_at(y), d.spine_up(y), vec3::Z);
            mesh.append(&v);
        }
        shift(&mut mesh, index::SPINE);
        meshes[index::SPINE] = vec![kit.bone(mesh, 12)];
    }

    // ── Ribcage: thoracic vertebrae, ribs, sternum, collarbones and shoulder blades ──
    {
        let [t12, t1] = d.thoracic_y;
        let mut mesh = MeshBuilder::new();
        let ys: Vec<f32> = (0..12).map(|k| t1 - (t1 - t12) * k as f32 / 11.0).collect();
        for (k, &y) in ys.iter().enumerate() {
            let r = (0.17 + 0.012 * k as f32) * girth;
            let mut v = bone::vertebra(&Vertebra { radius: r, height: 0.26, spine: 0.24, transverse: 0.36 });
            v.place_basis(d.spine_at(y), d.spine_up(y), vec3::Z);
            mesh.append(&v);
        }
        // The ribs: a cage that widens to the lower chest, closing on the breastbone.
        const WIDTH: [f32; RIBS] = [0.62, 0.9, 1.12, 1.25, 1.33, 1.37, 1.38, 1.36, 1.32, 1.25, 1.1, 0.95];
        const DROP: [f32; RIBS] = [0.35, 0.5, 0.62, 0.72, 0.8, 0.88, 0.95, 1.1, 1.25, 1.3, 0.7, 0.5];
        let cage_scale = (body.chest.half_width / 1.87) * (0.94 + 0.12 * state.build);
        let (sternum_top, z_top, tilt) = (t1 + 0.0, 0.55 * (body.chest.half_depth / 1.2), 0.12f32);
        let roots: Vec<V3> = ys.iter().map(|&y| d.spine_at(y)).collect();
        let widths: Vec<f32> = WIDTH.iter().map(|w| w * cage_scale).collect();
        let drops: Vec<f32> = DROP.to_vec();
        // Each rib's depth closes it on the breastbone's line, which leans forward as it descends.
        let depths: Vec<f32> = (0..RIBS)
            .map(|i| {
                let front_y = roots[i][1] - drops[i];
                let z_front = z_top + tilt * (sternum_top - front_y);
                ((z_front - roots[i][2] + 0.1) / Ribcage::front_over_depth()).max(0.2)
            })
            .collect();
        let cage = Ribcage { roots, widths, depths, drops, radius: 0.062 * girth };
        let ribs_gone = state.ribs_gone;
        mesh.append(&cage.mesh(&|i, side| ribs_gone[if side < 0.0 { 0 } else { 1 }] & (1 << i) != 0));
        let mut sternum = bone::sternum(2.85, 0.5 * (0.9 + 0.2 * state.build));
        sternum.rotate_x(-tilt).translate([0.0, sternum_top + 0.12, z_top]);
        mesh.append(&sternum);
        for side in [-1.0f32, 1.0] {
            let shoulder = d.arms[if side < 0.0 { 0 } else { 1 }][0];
            mesh.append(&bone::clavicle([side * 0.2, sternum_top + 0.05, z_top], vec3::add(shoulder, [-side * 0.1, 0.0, 0.0]), 0.075 * girth));
            // The blade lies on the back of the ribs, its border a hand's width from the spine,
            // and curves forward round them to the shoulder.
            let rib = cage.rib_path(3, side);
            let back = rib.iter().min_by(|a, b| (a[0] - side * 0.6).abs().total_cmp(&(b[0] - side * 0.6).abs())).copied().unwrap_or([side * 0.6, t1 - 0.8, -0.5]);
            let border = [side * 0.6, t1 - 1.0, back[2] - 0.2];
            mesh.append(&bone::scapula(border, 1.45, side, (shoulder[2] - border[2] - 0.15).max(0.3)));
        }
        shift(&mut mesh, index::RIBCAGE);
        meshes[index::RIBCAGE] = vec![kit.bone(mesh, 13)];
    }

    // ── Neck: seven cervical vertebrae ──
    {
        let [c7, atlas] = d.cervical_y;
        let mut mesh = MeshBuilder::new();
        for k in 0..7 {
            let y = c7 + (atlas - c7) * (k as f32 + 0.3) / 7.3;
            let r = (0.12 + 0.012 * (6 - k) as f32 * 0.5) * girth;
            let mut v = bone::vertebra(&Vertebra { radius: r, height: 0.15, spine: if k == 0 { 0.25 } else { 0.2 }, transverse: 0.2 });
            v.place_basis(d.spine_at(y), d.spine_up(y), vec3::Z);
            mesh.append(&v);
        }
        shift(&mut mesh, index::NECK);
        meshes[index::NECK] = vec![kit.bone(mesh, 14)];
    }

    // ── Skull and jaw ──
    {
        let head = body.head;
        let spec = SkullSpec {
            half_width: head.half_width * 0.97,
            half_height: head.half_height,
            half_depth: head.half_depth * 0.97,
            jaw: state.jaw_heft,
            brow: state.brow,
            missing_teeth: state.teeth_gone,
        };
        let skull = bone::skull(&spec, skull_cracks, &mut rng);
        // Head space is centred on the head; the bone's origin is the atlas.
        let to_bone = vec3::sub(head.centre, d.origins[index::SKULL]);
        let (mut bone_mesh, mut sockets, mut teeth, mut cracks) = (skull.bone, skull.sockets, skull.teeth, skull.cracks);
        for m in [&mut bone_mesh, &mut sockets, &mut teeth, &mut cracks] {
            m.translate(to_bone);
        }
        let mut parts = vec![kit.bone(bone_mesh, 15)];
        some(&mut parts, sockets, |m| kit.dark(m));
        some(&mut parts, teeth, |m| kit.teeth(m));
        some(&mut parts, cracks, |m| kit.cracks(m));
        meshes[index::SKULL] = parts;
        // The jaw hangs on its hinge, which is its origin.
        let hinge_head = vec3::sub(d.hinge, head.centre);
        if !state.jaw_gone {
            let (jaw, jaw_teeth) = bone::mandible(&spec, hinge_head, &mut rng);
            let mut parts = vec![kit.bone(jaw, 16)];
            some(&mut parts, jaw_teeth, |m| kit.teeth(m));
            meshes[index::JAW] = parts;
        }
    }

    // ── Arms and hands ──
    let mut crack_at = 0usize;
    for (k, side) in Side::BOTH.into_iter().enumerate() {
        let sg = side.sign(); // +1 left (+X)
        let [shoulder, elbow, wrist] = d.arms[k];
        let ub = arm_bone(side);
        // Humerus: ball in the shoulder, condyles at the elbow.
        let shape = LongBone { ends: [0.24 * girth, 0.19 * girth], shaft: 0.105 * girth, lobes: [0.0, 0.1 * girth], side: vec3::X };
        let mut mesh = bone::long_bone(vec3::add(shoulder, [-sg * 0.08, 0.0, 0.0]), elbow, &shape);
        let mut head = uv_sphere(0.3 * girth, 12, 9);
        head.translate(vec3::add(shoulder, [-sg * 0.1, 0.0, 0.0]));
        mesh.append(&head);
        if limb_cracks[crack_at % 6] > 0 {
            for _ in 0..limb_cracks[crack_at % 6] {
                scratches[ub].append(&bone_crack(&mut rng, shoulder, elbow, &shape));
            }
        }
        crack_at += 1;
        shift(&mut mesh, ub);
        meshes[ub] = vec![kit.bone(mesh, 20 + k as u32)];

        // Forearm: the radius on the thumb's side, the ulna behind with its elbow.
        let ulna = LongBone { ends: [0.17 * girth, 0.095 * girth], shaft: 0.075 * girth, lobes: [0.0; 2], side: vec3::X };
        let radius = LongBone { ends: [0.11 * girth, 0.14 * girth], shaft: 0.07 * girth, lobes: [0.0; 2], side: vec3::X };
        let mut mesh = bone::long_bone(vec3::add(elbow, [0.0, 0.0, -0.09]), vec3::add(wrist, [0.0, 0.05, -0.1]), &ulna);
        mesh.append(&bone::long_bone(vec3::add(elbow, [0.0, -0.02, 0.1]), vec3::add(wrist, [0.0, 0.03, 0.09]), &radius));
        let mut olecranon = spike(0.14 * girth, 0.3, 8, -0.4, armory::geom::Shading::SMOOTH);
        olecranon.place(vec3::add(elbow, [0.0, 0.05, -0.2]), vec3::normalize_or([0.0, 0.5, -0.8], vec3::Y));
        mesh.append(&olecranon);
        shift(&mut mesh, ub + 1);
        meshes[ub + 1] = vec![kit.bone(mesh, 22 + k as u32)];

        // Hand: built in the body's hand frame (thumb forward), a left one mirrored.
        let hand_form = body.hand_form();
        let frame = d.hand_frame(side);
        let spec = Hand { length: 0.105 * body.height * body.race().arm, half_width: hand_form.knuckle_half_width * 0.95, curl: state.curl, missing: state.fingers_gone[k] };
        let mut mesh = bone::hand(&spec, &mut rng);
        // A closed hand is built on its own stream, so closing it leaves the other bones' rolls as they were.
        if let Some(pose) = &grips[k] {
            let mut own = Mulberry32::new(state.seed ^ SKELETON_SALT ^ 0x6219 ^ k as u32);
            mesh = bone::hand_gripping(&spec, &hand_form, pose.fingers, pose.thumb, &mut own);
        }
        if sg > 0.0 {
            mesh.scale([-1.0, 1.0, 1.0]);
        }
        mesh.transform(&frame.rotation);
        // The mesh is about the wrist, which is the hand bone's origin.
        meshes[ub + 2] = vec![kit.bone(mesh, 24 + k as u32)];
    }

    // ── Legs and feet ──
    for (k, side) in Side::BOTH.into_iter().enumerate() {
        let sg = side.sign();
        let [hip, knee, ankle] = d.legs[k];
        let tb = leg_bone(side);
        // Femur: ball in the socket on a short neck, the great trochanter outside it, condyles at the knee.
        let shape = LongBone { ends: [0.2 * girth, 0.22 * girth], shaft: 0.13 * girth, lobes: [0.0, 0.13 * girth], side: vec3::X };
        let neck_end = vec3::add(hip, [sg * 0.32, -0.4, 0.0]);
        let mut mesh = bone::long_bone(neck_end, knee, &shape);
        let mut ball = uv_sphere(0.29 * girth, 12, 9);
        ball.translate(vec3::add(hip, [-sg * 0.03, 0.0, 0.0]));
        mesh.append(&ball);
        mesh.append(&tube(&[vec3::add(hip, [-sg * 0.02, 0.0, 0.0]), vec3::add(hip, [sg * 0.2, -0.22, 0.0]), neck_end], 8, &SweepOptions::default(), |_| 0.13 * girth));
        let mut trochanter = uv_sphere(0.2 * girth, 10, 8);
        trochanter.scale([0.8, 1.2, 1.0]).translate(vec3::add(hip, [sg * 0.36, -0.3, -0.02]));
        mesh.append(&trochanter);
        let mut patella = uv_sphere(0.2 * girth, 10, 8);
        patella.scale([1.0, 1.15, 0.5]).translate(vec3::add(knee, [0.0, 0.1, 0.34 * girth]));
        mesh.append(&patella);
        if limb_cracks[(crack_at + 2) % 6] > 0 {
            for _ in 0..limb_cracks[(crack_at + 2) % 6] {
                scratches[tb].append(&bone_crack(&mut rng, neck_end, knee, &shape));
            }
        }
        crack_at += 1;
        shift(&mut mesh, tb);
        meshes[tb] = vec![kit.bone(mesh, 30 + k as u32)];

        // Shin: tibia in front and fibula beside and behind it.
        let tibia = LongBone { ends: [0.25 * girth, 0.19 * girth], shaft: 0.115 * girth, lobes: [0.14 * girth, 0.08 * girth], side: vec3::X };
        let fibula = LongBone { ends: [0.105 * girth, 0.13 * girth], shaft: 0.055 * girth, lobes: [0.0; 2], side: vec3::X };
        let mut mesh = bone::long_bone(vec3::add(knee, [0.0, -0.05, 0.02]), vec3::add(ankle, [-sg * 0.03, 0.1, 0.05]), &tibia);
        mesh.append(&bone::long_bone(vec3::add(knee, [sg * 0.34, -0.38, -0.16]), vec3::add(ankle, [sg * 0.3, 0.12, -0.1]), &fibula));
        if limb_cracks[(crack_at + 4) % 6] > 0 {
            for _ in 0..limb_cracks[(crack_at + 4) % 6] {
                scratches[tb + 1].append(&bone_crack(&mut rng, vec3::add(knee, [0.0, -0.05, 0.02]), vec3::add(ankle, [0.0, 0.1, 0.05]), &tibia));
            }
        }
        crack_at += 1;
        shift(&mut mesh, tb + 1);
        meshes[tb + 1] = vec![kit.bone(mesh, 32 + k as u32)];

        // Foot: bones in the foot's own frame, the toe turned out as the body's is, its sole flat
        // on the floor.
        let length = d.foot_length;
        let spec = Foot { ankle_height: ankle[1], heel: 0.53 + 0.0, toe: length - 0.53, half_width: 0.2 * length * (0.9 + 0.2 * state.build) };
        let mut mesh = bone::foot(&spec);
        mesh.rotate_y(sg * 0.12);
        // Rest it on the plane exactly (the weathering of the other bones does not touch it).
        if let Some((lo, _)) = mesh.bounds() {
            mesh.translate([0.0, -ankle[1] - lo[1], 0.0]);
        }
        meshes[tb + 2] = vec![kit.smooth(mesh)];
    }

    for (bone, mut scratch) in scratches.into_iter().enumerate() {
        if !scratch.is_empty() {
            scratch.translate(vec3::neg(d.origins[bone]));
            meshes[bone].push(kit.cracks(scratch));
        }
    }
    let mut rig = Rig::default();
    for (bone, parts) in meshes.into_iter().enumerate() {
        let rest = Placement::translation(d.offset(bone));
        rig.add(NAMES[bone], parent_of(bone), rest, parts);
    }
    debug_assert_eq!(rig.bones.len(), index::COUNT);
    rig
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pose::{SkeletonPose, skeleton, skeleton_stride};
    use crate::{Creature, Species};
    use std::f32::consts::TAU;

    /// The lowest point of both feet, in creature space, with the rig posed by `local`.
    fn lowest_foot(rig: &Rig, local: &[Placement]) -> f32 {
        let world = rig.world(local);
        Side::BOTH
            .into_iter()
            .flat_map(|side| {
                let b = leg_bone(side) + 2;
                let place = world[b];
                rig.bones[b].mesh.parts.iter().flat_map(|p| p.positions.iter()).map(move |p| place.point(*p)[1])
            })
            .fold(f32::MAX, f32::min)
    }

    #[test]
    fn the_same_seed_gives_the_same_skeleton() {
        for seed in [0, 1, 7, 1207, u32::MAX] {
            let (a, b) = (SkeletonState::roll(seed), SkeletonState::roll(seed));
            assert_eq!(a, b);
            assert_eq!(a.build(), b.build(), "seed {seed}");
            assert_eq!(Creature::of(Species::Skeleton, seed), Creature::Skeleton(a));
        }
        assert_ne!(SkeletonState::roll(1), SkeletonState::roll(2));
        assert_ne!(SkeletonState::roll(1).build(), SkeletonState::roll(2).build());
    }

    #[test]
    fn the_rig_is_eighteen_named_bones_each_after_its_parent() {
        for seed in [1, 2, 3] {
            let rig = SkeletonState::roll(seed).build();
            rig.validate().unwrap();
            assert_eq!(rig.bones.len(), index::COUNT);
            for (i, bone) in rig.bones.iter().enumerate() {
                assert_eq!(bone.name, NAMES[i]);
                assert_eq!(bone.parent, parent_of(i));
                assert!(bone.parent.is_none_or(|p| p < i), "{} comes before its parent", bone.name);
                // Rest rotations are the identity: a pose is a turn about a joint in body axes.
                assert_eq!(bone.rest.rotation, armory::geom::Mat3::IDENTITY);
            }
            for name in ["pelvis", "spine", "ribcage", "skull", "jaw", "hand_left", "foot_right", "thigh_left"] {
                assert!(rig.bone(name).is_some(), "{name}");
            }
        }
    }

    #[test]
    fn a_skeleton_is_about_as_tall_as_a_human() {
        let mut shortest = f32::MAX;
        let mut tallest = f32::MIN;
        for seed in 0..100 {
            let state = SkeletonState::roll(seed);
            let rig = state.build();
            let (lo, hi) = rig.rest_mesh().bounds().unwrap();
            let body = state.body();
            // Crown to sole is the body's height, as the bones sit on its joints.
            assert!((hi[1] - lo[1] - body.height).abs() < 0.35, "seed {seed}: {} against {}", hi[1] - lo[1], body.height);
            assert!(lo[1].abs() < 0.02, "seed {seed}: soles at {}", lo[1]);
            assert!((state.height() - body.height).abs() < 1e-4);
            shortest = shortest.min(hi[1]);
            tallest = tallest.max(hi[1]);
        }
        // A human is about 17.2 units; skeletons run from 1.6 to 1.85 m.
        assert!(shortest > 15.8 && tallest < 18.8, "{shortest} to {tallest}");
        assert!(tallest - shortest > 1.0, "every skeleton is the same height");
    }

    #[test]
    fn feet_rest_on_the_floor_standing_and_walking() {
        for seed in 0..100 {
            let state = SkeletonState::roll(seed);
            let dims = state.dims();
            let rig = state.build();
            let stand = skeleton(&state, &SkeletonPose::default());
            assert!(lowest_foot(&rig, &stand).abs() < 0.02, "seed {seed}: standing at {}", lowest_foot(&rig, &stand));
            for k in 0..16 {
                let pose = SkeletonPose { stride: TAU * k as f32 / 16.0, pace: 1.0, time: 0.3 * k as f32, ..Default::default() };
                let local = crate::pose::skeleton_with(&dims, &pose);
                let low = lowest_foot(&rig, &local);
                assert!(low.abs() < 0.02, "seed {seed} step {k}: the lowest foot is at {low}");
                // The pelvis stays over the legs' reach: the hips never drop to the knees.
                let world = rig.world(&local);
                assert!(world[index::PELVIS].origin[1] > 0.8 * dims.legs[0][0][1], "seed {seed} step {k}");
            }
        }
    }

    #[test]
    fn a_swing_keeps_the_feet_planted_and_winds_the_arm_up_and_over() {
        let state = SkeletonState::roll(5);
        let dims = state.dims();
        let rig = state.build();
        let right_wrist = |p: f32| {
            let local = crate::pose::skeleton_with(&dims, &SkeletonPose { swing: Some(p), ..Default::default() });
            (rig.world(&local)[arm_bone(Side::Right) + 2].origin, local)
        };
        let shoulder = dims.arms[0][0];
        for k in 0..=20 {
            let (_, local) = right_wrist(k as f32 / 20.0);
            assert!(lowest_foot(&rig, &local).abs() < 0.02, "swing {k}: {}", lowest_foot(&rig, &local));
        }
        // Wound up, the hand is raised over the shoulder; thrown, it is out in front and lower.
        let (up, _) = right_wrist(0.38);
        assert!(up[1] > shoulder[1] + 1.0, "the hand is at {} with the shoulder at {}", up[1], shoulder[1]);
        let (out, _) = right_wrist(0.62);
        assert!(out[2] > shoulder[2] + 2.0 && out[1] < up[1] - 1.0, "{out:?} {up:?}");
        // And the swing ends where it began.
        let ((start, _), (end, _)) = (right_wrist(0.0), right_wrist(1.0));
        assert!(vec3::distance(start, end) < 0.05);
    }

    #[test]
    fn the_walk_is_a_cycle_that_swings_the_arms_against_the_legs() {
        let state = SkeletonState::roll(9);
        let dims = state.dims();
        let at = |stride: f32| crate::pose::skeleton_with(&dims, &SkeletonPose { stride, pace: 1.0, ..Default::default() });
        let (a, b) = (at(1.0), at(1.0 + TAU));
        for (p, q) in a.iter().zip(&b) {
            assert!(vec3::distance(p.origin, q.origin) < 1e-4);
            for k in 0..3 {
                assert!(vec3::distance(p.rotation.cols[k], q.rotation.cols[k]) < 1e-4);
            }
        }
        // A step is a fraction of the leg: a full cycle covers two of them.
        assert!((skeleton_stride(&dims) / (dims.thigh + dims.shin) - 1.4).abs() < 1e-4);
        // Half a cycle on, the legs and arms have swapped.
        let (a, b) = (at(0.5), at(0.5 + std::f32::consts::PI));
        let turn = |p: &[Placement], bone: usize| p[bone].rotation.cols[1][2];
        assert!((turn(&a, leg_bone(Side::Left)) - turn(&b, leg_bone(Side::Right))).abs() < 1e-3);
        assert!((turn(&a, arm_bone(Side::Left)) - turn(&b, arm_bone(Side::Right))).abs() < 1e-3);
        assert!(turn(&a, arm_bone(Side::Left)) * turn(&a, leg_bone(Side::Left)) < 0.0, "an arm swings against its own leg");
    }

    #[test]
    fn seeds_vary_the_bones_their_colour_their_cracks_and_what_is_missing() {
        let states: Vec<SkeletonState> = (0..200).map(SkeletonState::roll).collect();
        assert!(states.iter().any(|s| s.tone < 0.25) && states.iter().any(|s| (0.3..0.6).contains(&s.tone)) && states.iter().any(|s| s.tone > 0.75), "bleached, yellowed and dark");
        assert!(states.iter().any(|s| s.cracks >= 5) && states.iter().any(|s| s.cracks == 0));
        assert!(states.iter().any(|s| s.jaw_gone) && states.iter().any(|s| !s.jaw_gone));
        assert!(states.iter().any(|s| s.ribs_gone != [0, 0]) && states.iter().any(|s| s.fingers_gone != [0, 0]));
        assert!(states.iter().any(|s| s.build < 0.2) && states.iter().any(|s| s.build > 0.8));
        // Bleached bones are paler than dark ones.
        let light = |s: &SkeletonState| s.bone_colour().0 + s.bone_colour().1 + s.bone_colour().2;
        let (pale, dark) = (states.iter().find(|s| s.tone < 0.1).unwrap(), states.iter().find(|s| s.tone > 0.9).unwrap());
        assert!(light(pale) > light(dark) + 0.5);
    }

    #[test]
    fn missing_pieces_are_missing_from_the_rig() {
        let whole = SkeletonState { jaw_gone: false, ribs_gone: [0, 0], fingers_gone: [0, 0], cracks: 0, ..SkeletonState::roll(4) };
        let rig = whole.build();
        let count = |rig: &Rig, name: &str| rig.bone(name).unwrap().mesh.parts.iter().map(|p| p.positions.len()).sum::<usize>();
        assert!(count(&rig, "jaw") > 0);
        let no_jaw = SkeletonState { jaw_gone: true, ..whole.clone() }.build();
        assert_eq!(count(&no_jaw, "jaw"), 0);
        assert_eq!(no_jaw.bones.len(), index::COUNT, "the bone stays; its mesh is gone");
        let no_ribs = SkeletonState { ribs_gone: [0b11, 0b1100_0000_0000], ..whole.clone() }.build();
        assert!(count(&no_ribs, "ribcage") < count(&rig, "ribcage"));
        let no_fingers = SkeletonState { fingers_gone: [0b11110, 0b00001], ..whole.clone() }.build();
        assert!(count(&no_fingers, "hand_right") < count(&rig, "hand_right"));
        assert!(count(&no_fingers, "hand_left") < count(&rig, "hand_left"));
        let cracked = SkeletonState { cracks: 8, ..whole.clone() }.build();
        let cracks = |rig: &Rig| rig.bones.iter().flat_map(|b| &b.mesh.parts).filter(|p| p.name == "cracks").count();
        assert_eq!(cracks(&rig), 0);
        assert!(cracks(&cracked) > 0);
        // Only the skull has teeth and sockets.
        assert!(rig.bone("skull").unwrap().mesh.parts.iter().any(|p| p.name == "sockets") && rig.bone("skull").unwrap().mesh.parts.iter().any(|p| p.name == "teeth"));
    }

    #[test]
    fn the_rig_lines_up_with_the_body_it_was_laid_out_on() {
        for seed in [1, 2, 3, 4] {
            let state = SkeletonState::roll(seed);
            let dims = state.dims();
            let rig = state.build();
            let world = rig.world(&rig.rest());
            let body = &dims.body;
            for side in Side::BOTH {
                // Each joint's bone starts on the body's joint.
                for j in 0..3 {
                    assert!(vec3::distance(world[arm_bone(side) + j].origin, body.arm(side).joints[j]) < 1e-4, "{side:?} arm {j}");
                    assert!(vec3::distance(world[leg_bone(side) + j].origin, body.leg(side).joints[j]) < 1e-4, "{side:?} leg {j}");
                }
                // The hand's frame is the body's hand placement, a left one mirrored.
                let (mine, theirs) = (dims.hand_frame(side), body.hand_placement(side));
                assert!(vec3::distance(mine.origin, theirs.origin) < 1e-5 && vec3::distance(mine.y(), theirs.y()) < 1e-5 && vec3::distance(mine.z(), theirs.z()) < 1e-5);
                assert_eq!(theirs.is_mirrored(), side == Side::Left);
                assert!(!mine.is_mirrored(), "the bone's frame is a proper rotation");
                assert!(vec3::distance(dims.foot_frame(side).origin, body.foot_placement(side).origin) < 1e-6);
                // The hand's bones are about the wrist, within a hand's length of it.
                let hand = &rig.bones[arm_bone(side) + 2];
                let reach = 0.105 * body.height * body.race().arm;
                let far = hand.mesh.parts.iter().flat_map(|p| &p.positions).map(|p| vec3::length(*p)).fold(0.0, f32::max);
                assert!(far < 1.15 * reach && far > 0.8 * reach, "{side:?}: the hand reaches {far} of {reach}");
            }
            // The skull sits on the body's head, its jaw on the hinge.
            let skull = &world[index::SKULL];
            assert!(vec3::distance(skull.origin, dims.origins[index::SKULL]) < 1e-4);
            assert!(dims.origins[index::SKULL][1] < body.head.centre[1] && dims.hinge[1] < body.head.centre[1]);
            let (lo, hi) = rig.rest_mesh().bounds().unwrap();
            assert!(hi[1] > body.head.centre[1] + 0.8 * body.head.half_height && lo[0] > -3.6 && hi[0] < 3.6);
        }
    }

    #[test]
    fn the_skeleton_is_a_medium_undead_with_the_srd_numbers() {
        let block = Species::Skeleton.stat_block();
        assert_eq!((block.name, block.size, block.armour_class, block.hit_points, block.hit_dice, block.page), ("Skeleton", crate::Size::Medium, 13, 13, crate::Dice::new(2, 8, 4), 346));
        assert_eq!(block.hit_dice.average(), 13);
        let a = block.abilities;
        assert_eq!((a.strength, a.dexterity, a.constitution, a.intelligence, a.wisdom, a.charisma), (10, 14, 15, 6, 8, 5));
        assert_eq!([a.strength, a.dexterity, a.constitution, a.intelligence, a.wisdom, a.charisma].map(crate::Abilities::modifier), [0, 2, 2, -2, -1, -3]);
        assert_eq!(block.speed_feet, 30.0);
        assert_eq!(block.vulnerabilities, &[crate::DamageType::Bludgeoning]);
        assert_eq!(block.immunities, &[crate::DamageType::Poison]);
        assert_eq!(block.condition_immunities, &[crate::Condition::Exhaustion, crate::Condition::Poisoned]);
        assert_eq!(block.senses_feet, 60.0);
        // Shortsword: +4 to hit, 5 (1d6 + 2) piercing. Shortbow: +4, 5 (1d6 + 2), range 80/320.
        assert_eq!((block.attack_bonus, block.bite, block.bite.average()), (4, crate::Dice::new(1, 6, 2), 5));
        let bow = block.ranged.unwrap();
        assert_eq!((bow.to_hit, bow.damage, bow.range.normal_feet, bow.range.long_feet), (4, crate::Dice::new(1, 6, 2), 80.0, 320.0));
        assert_eq!((block.challenge, block.xp), ("1/4", 50));
        assert!(Species::Skeleton.is_skeleton() && !Species::Skeleton.is_snake() && !Species::Skeleton.is_rat());
        assert!((Species::Skeleton.speed() - 15.24).abs() < 0.01);
    }
}
