//! What a blow does to a creature: its hit points fall, it flashes, it is knocked back a step
//! and squeaks or hisses; at 0 it collapses and its body stays for the run.
//!
//! - **Flash**: every part of the creature glows with its own colour for a moment (the toon
//!   material's `glow`), then fades back, as damage flashes in games since the arcade.
//! - **Knockback**: the creature is pushed away from the delver over a few frames, sliding round
//!   rock and props as it walks; less the bigger it is.
//! - **Sound**: a rat squeaks, a snake hisses (`tools/synth_combat.py`).
//! - **Collapse**: a rat rolls onto its back, legs up; a snake uncoils and lies limp along the
//!   floor, jaw slack. Its mind stops ([`Corpse`]): it no longer moves, bites or keeps others
//!   off, and it blocks nobody.
//!
//! The flash's length and strength, the knockback distances and the collapse are our own.

use bevy::prelude::*;
use fb_creatures::{Creature, Size};

use super::Health;
use crate::creatures::behaviour::{Brain, body_radius};
use crate::creatures::{Critter, Intent};
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Obstacles};
use crate::player::{blocked_by, slide_around};
use crate::render::ToonMaterial;
use crate::settings::GameSettings;

pub struct FeedbackPlugin;

impl Plugin for FeedbackPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Struck>()
            .add_systems(Startup, load_creature_sounds)
            .add_systems(Update, (give_creatures_health, take_blows, knock_back, flash, collapse).chain().in_set(DelveSystems));
    }
}

/// A blow landing on a creature, for its damage and the feedback.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Struck {
    pub target: Entity,
    pub damage: u32,
    /// Where the blow came from on the floor (world `(x, z)`), to knock it away from.
    pub from: Vec2,
}

/// A creature at 0 hit points: its body, lying where it fell for the rest of the run.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Corpse {
    /// Seconds since it fell.
    pub since: f32,
    /// Where it stood as it fell.
    pub fell: Transform,
}

/// A creature just struck: how long the flash has left, and the knockback still to travel.
#[derive(Component, Clone, Debug, Default)]
pub struct Flinch {
    pub left: f32,
    /// The rest of the knockback, world units on the floor.
    pub push: Vec2,
    /// Each lit part's material and its own glow, to come back to.
    materials: Vec<(Handle<ToonMaterial>, f32)>,
}

/// How long a struck creature flashes, seconds, and how much brighter at first.
pub const FLASH_TIME: f32 = 0.22;
const FLASH_GLOW: f32 = 1.6;
/// How far a blow knocks a creature back, by the size it is drawn at, world units: a rat a
/// third of a metre, a giant rat or viper 15 cm, a constrictor hardly at all.
pub fn knockback(creature: &Creature) -> f32 {
    let size = match creature {
        Creature::Rat(r) => r.drawn_size(),
        Creature::Snake(s) => s.drawn_size(),
    };
    match size {
        Size::Tiny | Size::Small => 3.5,
        Size::Medium => 1.5,
        Size::Large => 0.4,
    }
}
/// How long a collapse takes, seconds.
pub const COLLAPSE_TIME: f32 = 0.45;

/// A rat's squeak and a snake's hiss.
#[derive(Resource, Clone, Debug, Default)]
pub struct CreatureSounds {
    pub squeak: Handle<AudioSource>,
    pub hiss: Handle<AudioSource>,
}

fn load_creature_sounds(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    if let Some(assets) = assets {
        commands.insert_resource(CreatureSounds { squeak: assets.load("audio/squeak.ogg"), hiss: assets.load("audio/hiss.ogg") });
    }
}

/// Each creature's hit points: its stat block's (the average the SRD prints).
pub fn give_creatures_health(mut commands: Commands, new: Query<(Entity, &Critter), Without<Health>>) {
    for (entity, critter) in &new {
        commands.entity(entity).insert(Health::full(critter.spot.species.stat_block().hit_points as i32));
    }
}

/// Applies each blow: hit points down, a flash and a push, a squeak or a hiss, and at 0 a
/// body.
#[allow(clippy::too_many_arguments)]
fn take_blows(
    mut commands: Commands,
    mut struck: MessageReader<Struck>,
    sounds: Option<Res<CreatureSounds>>,
    settings: Option<Res<GameSettings>>,
    mut creatures: Query<(&Critter, &Transform, &mut Health), Without<Corpse>>,
    children: Query<&Children>,
    materials: Query<&MeshMaterial3d<ToonMaterial>>,
    toon: Option<Res<Assets<ToonMaterial>>>,
) {
    for blow in struck.read() {
        let Ok((critter, at, mut health)) = creatures.get_mut(blow.target) else { continue };
        let fell = health.hurt(blow.damage);
        let away = (at.translation.xz() - blow.from).normalize_or_zero();
        let lit = children
            .iter_descendants(blow.target)
            .filter_map(|e| materials.get(e).ok())
            .map(|m| (m.0.clone(), toon.as_deref().and_then(|t| t.get(&m.0)).map_or(0.0, |t| t.extension.glow)))
            .collect();
        commands.entity(blow.target).insert(Flinch { left: FLASH_TIME, push: away * knockback(&critter.creature), materials: lit });
        if let Some(sounds) = &sounds {
            let sound = if critter.spot.species.is_rat() { sounds.squeak.clone() } else { sounds.hiss.clone() };
            let game = settings.as_deref().copied().unwrap_or_default();
            commands.spawn((Name::new("creature hurt"), AudioPlayer::new(sound), crate::audio::sound_effect(&game)));
        }
        if fell {
            commands.entity(blow.target).insert(Corpse { since: 0.0, fell: *at }).remove::<Brain>();
            info!("{} falls", critter.spot.species.stat_block().name);
        }
    }
}

/// Pushes a struck creature back over its flash, round what is in the way.
fn knock_back(
    time: Res<Time>,
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Res<Obstacles>,
    mut creatures: Query<(&Critter, &mut Flinch, &mut Transform, Option<&mut Corpse>)>,
) {
    let dt = time.delta_secs();
    let blocked = blocked_by(&level.0, &blocked_cells);
    let relief = crate::level::geometry::Relief::new(&level.0);
    for (critter, mut flinch, mut transform, corpse) in &mut creatures {
        if flinch.push == Vec2::ZERO || flinch.left <= 0.0 {
            continue;
        }
        let share = (dt / flinch.left).min(1.0);
        let step = flinch.push * share;
        flinch.push -= step;
        let at = transform.translation.xz();
        let to = slide_around(at, at + step, body_radius(critter), &blocked, &obstacles.0);
        transform.translation = Vec3::new(to.x, relief.floor_height(to.x, to.y), to.y);
        // A body falls where the blow put it.
        if let Some(mut corpse) = corpse {
            corpse.fell.translation = transform.translation;
        }
    }
}

/// Lights a struck creature up with its own colour, fading over [`FLASH_TIME`].
fn flash(mut commands: Commands, time: Res<Time>, mut creatures: Query<(Entity, &mut Flinch)>, toon: Option<ResMut<Assets<ToonMaterial>>>) {
    let mut toon = toon;
    for (entity, mut flinch) in &mut creatures {
        flinch.left -= time.delta_secs();
        let strength = FLASH_GLOW * (flinch.left / FLASH_TIME).clamp(0.0, 1.0);
        if let Some(toon) = toon.as_deref_mut() {
            for (handle, glow) in &flinch.materials {
                if let Some(mut material) = toon.get_mut(handle) {
                    material.extension.glow = glow + strength;
                }
            }
        }
        if flinch.left <= 0.0 {
            commands.entity(entity).remove::<Flinch>();
        }
    }
}

/// Lays each body down: a rat rolls onto its back about its middle, a snake goes slack.
fn collapse(time: Res<Time>, mut bodies: Query<(&Critter, &mut Corpse, &mut Transform, &mut Intent)>) {
    for (critter, mut corpse, mut transform, mut intent) in &mut bodies {
        corpse.since += time.delta_secs();
        // Limp: uncoiled and flat, the jaw a little open, the tongue lolling.
        *intent = Intent { coil: 0.0, rear: 0.0, strike: 0.0, jaw: 0.35, tongue: 0.6, look: 0.0 };
        if let Creature::Rat(rat) = &critter.creature {
            *transform = rolled_over(rat, &corpse.fell, (corpse.since / COLLAPSE_TIME).min(1.0));
        }
    }
}

/// A rat that stood at `fell`, `share` of the way through rolling onto its back: turned about
/// its own length through its body's middle, and lowered so its back rests on the floor.
pub fn rolled_over(rat: &fb_creatures::RatState, fell: &Transform, share: f32) -> Transform {
    let d = rat.dims();
    let s = share * share * (3.0 - 2.0 * share);
    // Creatures face +Z: rolling about it, nearly all the way over, a little to one side.
    let roll = Quat::from_rotation_z(0.9 * std::f32::consts::PI * s);
    let middle = Vec3::Y * d.centre_height;
    let drop = (d.centre_height - d.half_height).max(0.0) * s;
    let local = middle - roll * middle - Vec3::Y * drop;
    Transform { translation: fell.translation + fell.rotation * local, rotation: fell.rotation * roll, ..*fell }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_creatures::Species;

    #[test]
    fn a_rolled_rat_lies_on_its_back_on_the_floor() {
        let rat = fb_creatures::RatState::roll_as(3, Species::GiantRat);
        let fell = Transform::from_xyz(10.0, 0.0, 5.0).with_rotation(Quat::from_rotation_y(0.7));
        assert_eq!(rolled_over(&rat, &fell, 0.0), fell);
        let down = rolled_over(&rat, &fell, 1.0);
        // Its up is now mostly down.
        assert!((down.rotation * Vec3::Y).y < -0.9);
        // Its body's middle stays over where it stood, and it rests on the floor, not in it.
        let middle = down.transform_point(Vec3::Y * rat.dims().centre_height);
        assert!(middle.xz().distance(fell.translation.xz()) < 0.5 * rat.dims().half_width);
        let lowest = rat.build().rest_mesh().parts.iter().flat_map(|p| p.positions.clone()).map(|p| down.transform_point(Vec3::from_array(p)).y).fold(f32::MAX, f32::min);
        assert!(lowest > -0.06 * rat.length && lowest < 0.1 * rat.length, "{lowest}");
    }

    #[test]
    fn bigger_creatures_are_knocked_back_less() {
        let push = |species, seed| knockback(&Creature::of(species, seed));
        assert!(push(Species::Rat, 1) > push(Species::GiantRat, 1));
        assert!(push(Species::PoisonousSnake, 1) > push(Species::ConstrictorSnake, 1));
    }
}
