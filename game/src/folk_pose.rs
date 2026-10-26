//! Folk drawn as skinned figures, and posed to walk or stand.
//!
//! [`spawn_rigged`] builds a figure's skeleton (one entity per `fb_body` joint, parented as the
//! rig is, [`FolkJoint`]) and draws the body and its gear as Bevy skinned meshes bound to it, the
//! weights from `fb_body` ([`fb_body::Skinner`]). What a figure holds rides a joint rigidly, a
//! hand's grip on its wrist. A figure is skinned rather than drawn as rigid bone entities, as the
//! creatures are, because a folk's knees, elbows and clothes would open gaps at rigid joints;
//! a rat's would not.
//!
//! [`pose_folk`] poses every [`FolkAnim`] each frame: it advances the gait's phase by the
//! distance walked (the speed in the parent's [`Stride`], world units a second, 0 standing) and
//! writes the joints' transforms from `fb_body`'s gait, a walk and an idle blended by speed
//! ([`fb_body::Gait::pose`]). Only the joints' transforms change: the meshes are never rebuilt.
//! Folk far from the player are not posed ([`POSE_RANGE`]), and those a little way off are
//! posed less often ([`NEAR_RANGE`]), so a level of folk costs the frame little.
//!
//! The head joint also carries the turn toward the player ([`crate::folk::FolkHead`]).
//!
//! A folk swinging at the delver ([`ArmSwing`], set by `crate::folk_combat::fight`) raises the
//! main hand's arm over the shoulder and brings it down through the blow ([`swing_turn`]), on top
//! of the gait.

use bevy::{
    app::Propagate,
    light::NotShadowCaster,
    mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes},
    prelude::*,
};
use fb_body::{Gait, IdleCycle, JOINTS, Joint, Rig};

use crate::figure::Figure;
use crate::folk::{FolkHead, Stride, head_rotation};
use crate::play::PlayerCamera;
use crate::render::ItemSpawner;

/// Adds [`pose_folk`].
pub struct FolkPosePlugin;

impl Plugin for FolkPosePlugin {
    fn build(&self, app: &mut App) {
        // `FB_FOLK_WALK=<n>` has every folk walk in place at n times its natural pace, to see the
        // gait and measure its cost before anything moves them (README, "Debug switches").
        if let Some(multiple) = std::env::var("FB_FOLK_WALK")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
        {
            app.insert_resource(WalkInPlace(multiple));
        }
        app.add_systems(
            Update,
            (
                walk_in_place.run_if(resource_exists::<WalkInPlace>),
                pose_folk,
            )
                .chain(),
        );
    }
}

/// Every folk walks in place at this multiple of its natural pace ([`FolkPosePlugin`]).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct WalkInPlace(pub f32);

fn walk_in_place(
    walk: Res<WalkInPlace>,
    anims: Query<(&ChildOf, &FolkAnim)>,
    mut strides: Query<&mut Stride>,
) {
    for (child_of, anim) in &anims {
        if let Ok(mut stride) = strides.get_mut(child_of.parent()) {
            stride.speed = anim.gait.natural_speed() * walk.0;
        }
    }
}

/// How far from the player folk are posed at all, world units (20 m): past it they hold the pose
/// they had. Our own, as the creatures' ([`crate::creatures`]) range is.
pub const POSE_RANGE: f32 = 200.0;
/// Within this (10 m) folk are posed every frame; between it and [`POSE_RANGE`] every third.
pub const NEAR_RANGE: f32 = 100.0;
/// How quickly the speed that blends walk and stand follows [`Stride::speed`], per second, so
/// a folk that stops settles into standing rather than snapping to it.
const SPEED_FOLLOW: f32 = 8.0;

/// One joint of a figure's skeleton.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FolkJoint(pub Joint);

/// A figure's skeleton and how it moves: on the entity the joints and the skinned meshes hang
/// under, whose parent carries the [`Stride`].
#[derive(Component, Clone, Debug)]
pub struct FolkAnim {
    pub rig: Rig,
    pub gait: Gait,
    /// The joint entities, by [`Joint::index`].
    pub joints: [Entity; JOINTS],
    /// How far through the walk, 0 to 1 over two steps.
    pub phase: f32,
    /// The speed the pose follows, easing toward the [`Stride`]'s.
    pub speed: f32,
    pub cycle: IdleCycle,
}

impl FolkAnim {
    pub fn joint(&self, joint: Joint) -> Entity {
        self.joints[joint.index()]
    }

    /// Where a thing at `at` in body space sits in `joint`'s own space at rest.
    pub fn in_joint(&self, joint: Joint, at: Vec3) -> Vec3 {
        at - Vec3::from_array(self.rig.rest_at(joint))
    }
}

/// Spawns `figure`'s skeleton and meshes under `parent`: the joints, the body and gear skinned to
/// them, the held items on their wrists. Returns the [`FolkAnim`] to put on `parent`. Every
/// entity is spawned on the spawner's own commands, so they apply in order. Lights the figure
/// holds are the caller's: [`FolkAnim::in_joint`] puts one on its joint.
///
/// `head_turns` puts a [`FolkHead`] on the head joint, so the head follows the player.
pub fn spawn_rigged(
    spawner: &mut ItemSpawner,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
    figure: &Figure,
    parent: Entity,
    head_turns: bool,
    seed: u32,
) -> FolkAnim {
    let mut joints = [Entity::PLACEHOLDER; JOINTS];
    for joint in Joint::ALL {
        let above = joint.parent().map_or(parent, |p| joints[p.index()]);
        let mut entity = spawner.commands().spawn((
            Name::new(joint.name()),
            FolkJoint(joint),
            Transform::from_translation(Vec3::from_array(figure.rig.local_offset(joint))),
            Visibility::default(),
            ChildOf(above),
        ));
        if joint == Joint::Head && head_turns {
            entity.insert(FolkHead {
                look: Vec2::ZERO,
                pivot: Some(figure.neck),
            });
        }
        joints[joint.index()] = entity.id();
    }
    let inverse = bindposes.add(SkinnedMeshInverseBindposes::from(
        Joint::ALL
            .map(|j| Mat4::from_translation(-Vec3::from_array(figure.rig.rest_at(j))))
            .to_vec(),
    ));
    let skinned = SkinnedMesh {
        inverse_bindposes: inverse,
        joints: joints.to_vec(),
    };

    for piece in figure.pieces.iter().filter(|p| !p.mesh.parts.is_empty()) {
        let name = Name::new(piece.name.clone());
        let mut entity = match (&piece.skin, piece.attach) {
            (Some(skin), _) => {
                let mut entity =
                    spawner.spawn_skinned(&piece.mesh, &piece.appearance, skin, &skinned);
                entity.insert((name, ChildOf(parent)));
                entity
            }
            (None, Some(joint)) => {
                let at = -Vec3::from_array(figure.rig.rest_at(joint));
                let mut entity = spawner.spawn(&piece.mesh, &piece.appearance);
                entity.insert((
                    name,
                    Transform::from_translation(at),
                    ChildOf(joints[joint.index()]),
                ));
                entity
            }
            (None, None) => {
                let mut entity = spawner.spawn(&piece.mesh, &piece.appearance);
                entity.insert((name, ChildOf(parent)));
                entity
            }
        };
        if piece.lit {
            entity.insert(Propagate(NotShadowCaster));
        }
    }
    FolkAnim {
        rig: figure.rig.clone(),
        gait: figure.gait.clone(),
        joints,
        phase: 0.0,
        speed: 0.0,
        cycle: IdleCycle::of(seed),
    }
}

/// How far through a swing of the main hand's arm a folk is, 0 to 1, or `None` with no swing
/// under way: on the [`crate::folk::Denizen`] root, beside its [`Stride`].
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ArmSwing(pub Option<f32>);

/// The turn of the swinging arm's shoulder `u` of the way through a swing, radians about the
/// body's left-right axis, negative raising the arm forward and up: drawn back over the
/// shoulder by 0.45, brought down through the blow by 0.62 (where it lands,
/// `crate::folk_combat::LANDS`), then back to rest. Our own, after the skeleton's swing
/// (`fb_creatures::pose::skeleton_armed`).
pub fn swing_turn(u: f32) -> f32 {
    const RAISED: f32 = -2.5;
    const STRUCK: f32 = -0.45;
    let ease = |x: f32| {
        let x = x.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    if u < 0.45 {
        RAISED * ease(u / 0.45)
    } else if u < 0.62 {
        RAISED + (STRUCK - RAISED) * ease((u - 0.45) / 0.17)
    } else {
        STRUCK * (1.0 - ease((u - 0.62) / 0.38))
    }
}

/// A joint's transform for `pose`'s rotation `q` and offset.
fn joint_transform(rig: &Rig, joint: Joint, pose: &fb_body::RigPose) -> Transform {
    let q = pose.rotation[joint.index()].0;
    Transform {
        translation: Vec3::from_array(rig.local_offset(joint))
            + Vec3::from_array(pose.offset[joint.index()]),
        rotation: Quat::from_xyzw(q[0], q[1], q[2], q[3]),
        scale: Vec3::ONE,
    }
}

/// Whether a figure between [`NEAR_RANGE`] and [`POSE_RANGE`] is posed this frame: one frame in
/// three, staggered by entity. Each is taken mod 3 before adding, so nothing overflows.
fn posed_this_frame(frame: u32, entity: Entity) -> bool {
    (frame % 3 + entity.index_u32() % 3).is_multiple_of(3)
}

/// Poses every figure: advances its gait by the distance its parent's [`Stride`] covers and writes
/// the joints, as the module says.
pub fn pose_folk(
    time: Res<Time>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    // A body lies as it fell: it is not posed.
    strides: Query<
        (&Stride, &GlobalTransform, Option<&ArmSwing>),
        Without<crate::combat::feedback::Corpse>,
    >,
    looks: Query<&FolkHead>,
    mut anims: Query<(Entity, &ChildOf, &mut FolkAnim)>,
    mut joints: Query<&mut Transform, With<FolkJoint>>,
    mut frame: Local<u32>,
) {
    *frame = frame.wrapping_add(1);
    let dt = time.delta_secs();
    let eye = eyes.iter().next().map(|e| e.translation());
    for (entity, child_of, mut anim) in &mut anims {
        let Ok((stride, at, swing)) = strides.get(child_of.parent()) else {
            continue;
        };
        let shoulder = Joint::arm(fb_body::MAIN_HAND)[0];
        let anim = &mut *anim;
        // The walk goes on by distance whether or not it is drawn, so folk come back in step.
        let stride_length = anim.gait.stride_length().max(1e-3);
        anim.phase = (anim.phase + stride.speed.max(0.0) * dt / stride_length).rem_euclid(1.0);
        anim.speed += (stride.speed - anim.speed) * (1.0 - (-SPEED_FOLLOW * dt).exp());
        if let Some(eye) = eye {
            let away = (at.translation() - eye).xz().length();
            if away > POSE_RANGE || (away > NEAR_RANGE && !posed_this_frame(*frame, entity)) {
                continue;
            }
        }
        let pose = anim
            .gait
            .pose(anim.phase, anim.speed, &anim.cycle, time.elapsed_secs());
        for joint in Joint::ALL {
            let Ok(mut transform) = joints.get_mut(anim.joint(joint)) else {
                continue;
            };
            let mut set = joint_transform(&anim.rig, joint, &pose);
            // The head turns toward whoever is near, over the walk's own counter-turn.
            if joint == Joint::Head
                && let Ok(head) = looks.get(anim.joint(joint))
            {
                set.rotation *= head_rotation(head.look);
            }
            // The swinging arm turns about the shoulder, in its parent's axes.
            if joint == shoulder
                && let Some(u) = swing.and_then(|s| s.0)
            {
                set.rotation = Quat::from_rotation_x(swing_turn(u)) * set.rotation;
            }
            if *transform != set {
                *transform = set;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::figure::Figure;
    use fb_body::{BodySpec, Folk};

    fn fighter(folk: Folk, seed: u32) -> Vec<(fb_items::Slot, fb_items::Item)> {
        fb_items::outfit::outfit("fighter")
            .unwrap()
            .roll(seed, folk.key())
            .worn
    }

    #[test]
    fn far_folk_take_turns_without_overflowing() {
        let mut world = World::new();
        let entities: Vec<Entity> = (0..9).map(|_| world.spawn_empty().id()).collect();
        for frame in [0, 1, 2, u32::MAX - 1, u32::MAX] {
            let posed = entities
                .iter()
                .filter(|&&e| posed_this_frame(frame, e))
                .count();
            assert_eq!(posed, 3, "a third of far folk pose on frame {frame}");
        }
    }

    #[test]
    fn a_figures_pieces_are_skinned_or_ride_a_joint() {
        for folk in Folk::ALL {
            let figure = Figure::dress(&BodySpec::roll(folk, 1207), &fighter(folk, 1207), 0.2);
            for piece in &figure.pieces {
                match (&piece.skin, piece.attach) {
                    (Some(skin), None) => {
                        assert_eq!(
                            skin.len(),
                            piece.mesh.parts.len(),
                            "{folk:?} {}",
                            piece.name
                        );
                        for (part, weights) in piece.mesh.parts.iter().zip(skin) {
                            assert_eq!(weights.joints.len(), part.positions.len());
                        }
                    }
                    (None, Some(_)) => {}
                    other => panic!("{folk:?} {}: {:?}", piece.name, other.1),
                }
            }
            // The weapon rides the main hand's wrist.
            assert!(
                figure
                    .pieces
                    .iter()
                    .any(|p| p.attach == Some(Joint::wrist(fb_body::MAIN_HAND))),
                "{folk:?}"
            );
        }
    }

    /// An app with one dressed folk standing at the origin, skeleton spawned, and `pose_folk` running.
    fn posing_app(folk: Folk) -> (App, Entity, Entity) {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(50),
            ))
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::render::ToonMaterial>>()
            .init_resource::<Assets<crate::render::OutlineMaterial>>()
            .init_resource::<Assets<SkinnedMeshInverseBindposes>>()
            .init_resource::<crate::render::OutlineMaterials>()
            .add_systems(Update, pose_folk);
        let root = app
            .world_mut()
            .spawn((Stride::default(), Transform::default()))
            .id();
        let stance = app
            .world_mut()
            .spawn((Transform::default(), ChildOf(root)))
            .id();
        let figure = Figure::dress(&BodySpec::roll(folk, 1207), &fighter(folk, 1207), 0.2);
        let anim = app
            .world_mut()
            .run_system_once(
                move |mut spawner: ItemSpawner,
                      mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                    spawn_rigged(&mut spawner, &mut bindposes, &figure, stance, true, 1207)
                },
            )
            .unwrap();
        app.world_mut().entity_mut(stance).insert(anim);
        (app, root, stance)
    }

    #[test]
    fn a_figure_is_a_skeleton_with_skinned_meshes_bound_to_it() {
        let (mut app, _, stance) = posing_app(Folk::Human);
        app.update();
        let anim = app.world().get::<FolkAnim>(stance).unwrap().clone();
        // Sixteen joints, each under its parent as the rig has it; the pelvis under the stance.
        let mut joints = app.world_mut().query::<(Entity, &FolkJoint, &ChildOf)>();
        let found: Vec<(Entity, Joint, Entity)> = joints
            .iter(app.world())
            .map(|(e, j, c)| (e, j.0, c.parent()))
            .collect();
        assert_eq!(found.len(), JOINTS);
        for (entity, joint, parent) in found {
            assert_eq!(anim.joint(joint), entity);
            assert_eq!(parent, joint.parent().map_or(stance, |p| anim.joint(p)));
        }
        // Every body and gear mesh and its ink hull are bound to all of them, in the rig's order.
        let mut skinned = app.world_mut().query::<&SkinnedMesh>();
        let count = skinned.iter(app.world()).count();
        assert!(count > 8, "{count} skinned meshes");
        assert!(skinned.iter(app.world()).all(|s| s.joints == anim.joints));
        // The head turns about the neck.
        assert!(
            app.world()
                .get::<FolkHead>(anim.joint(Joint::Head))
                .is_some()
        );
        // What is held rides a wrist joint, under it, not the stance.
        let mut held = app
            .world_mut()
            .query_filtered::<&ChildOf, (With<crate::render::ArmoryItem>, Without<SkinnedMesh>)>();
        let wrist = anim.joint(Joint::wrist(fb_body::MAIN_HAND));
        assert!(held.iter(app.world()).any(|c| c.parent() == wrist));
    }

    /// The pelvis joint's local transform, which carries the bob.
    fn pelvis(app: &App, stance: Entity) -> Transform {
        let anim = app.world().get::<FolkAnim>(stance).unwrap();
        *app.world()
            .get::<Transform>(anim.joint(Joint::Pelvis))
            .unwrap()
    }

    #[test]
    fn folk_walk_by_the_distance_they_cover_and_stand_when_stopped() {
        let (mut app, root, stance) = posing_app(Folk::Human);
        let natural = app
            .world()
            .get::<FolkAnim>(stance)
            .unwrap()
            .gait
            .natural_speed();
        app.world_mut().get_mut::<Stride>(root).unwrap().speed = natural;
        let bob = |app: &mut App, frames: usize| -> f32 {
            let ys: Vec<f32> = (0..frames)
                .map(|_| {
                    app.update();
                    pelvis(app, stance).translation.y
                })
                .collect();
            ys.iter().copied().fold(f32::MIN, f32::max)
                - ys.iter().copied().fold(f32::MAX, f32::min)
        };
        // Settled into the walk, the pelvis bobs a good fraction of a unit over two steps.
        bob(&mut app, 20);
        assert!(bob(&mut app, 30) > 0.25, "walking");
        let phase = app.world().get::<FolkAnim>(stance).unwrap().phase;
        assert!(phase > 0.0 && phase < 1.0);
        // The phase advances by distance: half the speed, half the way round.
        app.world_mut().get_mut::<Stride>(root).unwrap().speed = natural * 0.5;
        let before = app.world().get::<FolkAnim>(stance).unwrap().phase;
        app.update();
        app.update();
        let step = (app.world().get::<FolkAnim>(stance).unwrap().phase - before).rem_euclid(1.0);
        let stride = app
            .world()
            .get::<FolkAnim>(stance)
            .unwrap()
            .gait
            .stride_length();
        assert!((step - natural * 0.5 * 0.1 / stride).abs() < 1e-3, "{step}");
        // Stopped, it settles to standing: a breath's lift and a shift of weight, not a bob.
        app.world_mut().get_mut::<Stride>(root).unwrap().speed = 0.0;
        bob(&mut app, 60);
        assert!(bob(&mut app, 30) < 0.15, "standing");
    }

    #[test]
    fn folk_far_from_the_player_hold_their_pose() {
        let (mut app, root, stance) = posing_app(Folk::Human);
        let natural = app
            .world()
            .get::<FolkAnim>(stance)
            .unwrap()
            .gait
            .natural_speed();
        app.world_mut().get_mut::<Stride>(root).unwrap().speed = natural;
        app.world_mut().spawn((
            PlayerCamera,
            Transform::from_xyz(POSE_RANGE * 2.0, 16.0, 0.0),
        ));
        for _ in 0..5 {
            app.update();
        }
        let held = pelvis(&app, stance);
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(pelvis(&app, stance), held, "nobody sees them");
        // Though the walk goes on by distance, so they come back in step.
        assert!(app.world().get::<FolkAnim>(stance).unwrap().phase != 0.0);
        // Brought near, they move again.
        app.world_mut()
            .query_filtered::<&mut Transform, With<PlayerCamera>>()
            .single_mut(app.world_mut())
            .unwrap()
            .translation = Vec3::new(10.0, 16.0, 0.0);
        for _ in 0..10 {
            app.update();
        }
        assert_ne!(pelvis(&app, stance), held);
    }

    #[test]
    fn posing_every_folk_on_1207_each_frame_is_cheap() {
        let folk = fb_level::generate(1207, 1).folk;
        assert!(!folk.is_empty());
        let people: Vec<(fb_body::Gait, IdleCycle)> = folk
            .iter()
            .map(|spot| {
                let person = crate::folk::person_1207(spot);
                (
                    Figure::dress(&person.spec, &person.gear, person.dirt).gait,
                    IdleCycle::of(spot.seed),
                )
            })
            .collect();
        // A hundred frames of the whole level, each person walking.
        let start = crate::thread_cpu_seconds();
        let mut sum = 0.0;
        for frame in 0..100 {
            for (gait, cycle) in &people {
                let pose = gait.pose(
                    frame as f32 * 0.013,
                    gait.natural_speed(),
                    cycle,
                    frame as f32 * 0.016,
                );
                sum += pose.offset[0][1];
            }
        }
        let per_frame = (crate::thread_cpu_seconds() - start) / 100.0;
        assert!(sum.is_finite());
        // A sixtieth of a second is 16 ms; posing the level should take a small part of one frame
        // (a hundredth of it, with the optimisations tests get).
        assert!(
            per_frame < 0.0002,
            "{:.3} ms to pose {} folk",
            per_frame * 1000.0,
            people.len()
        );
    }

    #[test]
    fn a_skinned_part_makes_a_skinned_mesh() {
        use bevy::mesh::VertexAttributeValues;
        let figure = Figure::dress(
            &BodySpec::roll(Folk::Human, 1207),
            &fighter(Folk::Human, 1207),
            0.2,
        );
        let piece = &figure.pieces[0];
        let part = &piece.mesh.parts[0];
        let mesh = crate::render::part_to_skinned_mesh(part, &piece.skin.as_ref().unwrap()[0]);
        assert!(
            matches!(mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX), Some(VertexAttributeValues::Uint16x4(v)) if v.len() == part.positions.len())
        );
        assert!(mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).is_some());
        // Where it rests rides in the tangent, and the rest normal in the second uvs.
        let Some(VertexAttributeValues::Float32x4(rest)) = mesh.attribute(Mesh::ATTRIBUTE_TANGENT)
        else {
            panic!("no rest positions")
        };
        assert_eq!(rest[3][..3], part.positions[3]);
        let Some(VertexAttributeValues::Float32x2(octa)) = mesh.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("no rest normals")
        };
        assert_eq!(octa.len(), part.positions.len());
        assert!(
            octa.iter()
                .all(|e| e[0].abs() <= 1.0 + 1e-5 && e[1].abs() <= 1.0 + 1e-5)
        );
    }
}
