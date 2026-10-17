//! What a blow does to a creature: its hit points fall, it flashes, it is knocked back a step
//! and squeaks or hisses; at 0 it collapses and its body stays for the run.
//!
//! - **Flash**: every part of the creature turns toward a warm white and glows for a moment
//!   (the toon material's base colour and `glow`), holds there for `flash_hold`, then fades
//!   back over `flash_fade` ([`CombatTuning`]), as damage flashes in games since the arcade. Glow alone multiplies a part's own
//!   colour, which leaves a black rat dark, so the colour itself is lifted too.
//! - **Knockback**: the creature is pushed away from the delver over a few frames, sliding round
//!   rock and props as it walks; less the bigger it is.
//! - **Sound**: a rat squeaks, a snake hisses (`tools/synth_combat.py`).
//! - **Collapse**: after the flash's hold, so a creature felled by one blow is
//!   seen to be struck before it falls, a rat rolls onto its back, legs up; a snake uncoils and
//!   lies limp along the floor, jaw slack. Its mind stops ([`Corpse`]): it no longer moves, bites or keeps others
//!   off, and it blocks nobody.
//!
//! The flash's length, hold, colour and strength, the knockback distances and the collapse are
//! our own.

use bevy::prelude::*;
use fb_creatures::{Creature, Size};

use super::Health;
use super::callouts::{Callout, Outcome};
use crate::creatures::behaviour::{Brain, body_radius};
use crate::creatures::{Critter, Intent};
use crate::folk::Denizen;
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Obstacle, Obstacles};
use crate::player::{blocked_by, slide_around};
use crate::render::ToonMaterial;
use crate::settings::GameSettings;
use crate::tuning::{CombatTuning, Tuning};

pub struct FeedbackPlugin;

impl Plugin for FeedbackPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Struck>()
            .add_message::<Callout>()
            .add_message::<Slain>()
            .init_resource::<Tuning>()
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
    /// Whether it is a bludgeoning blow (a mace, a club, a staff, a fist), which a skeleton
    /// takes double of.
    pub bludgeoning: bool,
    /// Whether its attack roll was a natural 20, for its callout ([`super::callouts`]).
    pub critical: bool,
}

/// A creature or a folk brought to 0 hit points by a blow of the delver's ([`Struck`]): what a
/// weapon "of the Leech" heals on (`crate::combat::vitals`).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Slain {
    pub target: Entity,
}

/// A creature or a folk at 0 hit points: its body, lying where it fell for the rest of the run.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Corpse {
    /// Seconds since it fell.
    pub since: f32,
    /// Where it stood as it fell.
    pub fell: Transform,
}

/// On a body whose gear has been set down on the floor (a skeleton's, a folk's): it is never
/// set down twice, and a body brought back from the record is spawned with it, bare.
#[derive(Component, Debug)]
pub struct Looted;

/// A creature or a folk just struck: how long the flash has left, and the knockback still to travel.
#[derive(Component, Clone, Debug, Default)]
pub struct Flinch {
    pub left: f32,
    /// The rest of the knockback, world units on the floor.
    pub push: Vec2,
    /// Each lit part's material and its own colour and glow, to come back to.
    materials: Vec<(Handle<ToonMaterial>, Color, f32)>,
}

/// What a [`Flinch`] needs to light a struck thing up: its descendants' toon materials, and the
/// flinch it may already be in.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Flinches<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    materials: Query<'w, 's, &'static MeshMaterial3d<ToonMaterial>>,
    flinches: Query<'w, 's, &'static Flinch>,
    toon: Option<Res<'w, Assets<ToonMaterial>>>,
}

impl Flinches<'_, '_> {
    /// A fresh flinch for `target`, pushed `push` along the floor, over the tuned flash: every
    /// lit part's own colour and glow, to come back to (struck again mid-flash, its own, not
    /// the flash's).
    pub fn of(&self, target: Entity, push: Vec2, combat: &CombatTuning) -> Flinch {
        let materials = match self.flinches.get(target) {
            Ok(old) => old.materials.clone(),
            Err(_) => self
                .children
                .iter_descendants(target)
                .filter_map(|e| self.materials.get(e).ok())
                .filter_map(|m| self.toon.as_deref().and_then(|t| t.get(&m.0)).map(|t| (m.0.clone(), t.base.base_color, t.extension.glow)))
                .collect(),
        };
        Flinch { left: combat.flash_time(), push, materials }
    }
}

/// How far toward [`FLASH_COLOUR`] a part's colour goes at full, and the glow added.
const FLASH_MIX: f32 = 0.65;
const FLASH_GLOW: f32 = 0.9;
/// The flash's colour: the page's cream ([`crate::ui::BRIGHT`]).
const FLASH_COLOUR: Color = crate::ui::BRIGHT;
/// How strong the flash is with `left` seconds of it to go: full through `combat`'s hold, then
/// easing out to nothing over its fade.
pub fn flash_strength(left: f32, combat: &CombatTuning) -> f32 {
    if left <= 0.0 {
        return 0.0;
    }
    let since = combat.flash_time() - left;
    if since < combat.flash_hold {
        return 1.0;
    }
    let fade = (left / combat.flash_fade.max(1e-3)).clamp(0.0, 1.0);
    fade * fade
}

/// How far into its collapse a body is, 0 to 1, `since` seconds after it fell: still through
/// the flash's `hold` (a felled creature stands, flashing, for it), then over [`COLLAPSE_TIME`].
pub fn collapse_share(since: f32, hold: f32) -> f32 {
    ((since - hold) / COLLAPSE_TIME).clamp(0.0, 1.0)
}
/// How far a blow knocks a creature back, by the size it is drawn at, world units: a rat a
/// third of a metre, a giant rat or viper 15 cm, a constrictor hardly at all. The tuning's
/// `knockback_scale` multiplies it.
pub fn knockback(creature: &Creature) -> f32 {
    let size = match creature {
        Creature::Rat(r) => r.drawn_size(),
        Creature::Snake(s) => s.drawn_size(),
        Creature::Skeleton(s) => s.drawn_size(),
        Creature::Spider(s) => s.drawn_size(),
    };
    knockback_for(size)
}

/// How far a blow knocks back something of `size`, world units ([`knockback`]); a folk of
/// any folk is Medium or Small.
pub fn knockback_for(size: Size) -> f32 {
    match size {
        Size::Tiny | Size::Small => 3.5,
        Size::Medium => 1.5,
        Size::Large => 0.4,
    }
}
/// How long a collapse takes, seconds.
pub const COLLAPSE_TIME: f32 = 0.45;

/// A rat's squeak and a snake's hiss when struck, their cues as they attack, and the thud of a
/// blow landing on the delver.
#[derive(Resource, Clone, Debug, Default)]
pub struct CreatureSounds {
    pub squeak: Handle<AudioSource>,
    pub hiss: Handle<AudioSource>,
    /// A rat's sharp squeal as it lunges.
    pub rat_squeal: Handle<AudioSource>,
    /// A snake's rising hiss as it rears.
    pub snake_rise: Handle<AudioSource>,
    pub thud: Handle<AudioSource>,
}

/// How far a blow's damage is changed by what struck a creature: a skeleton takes double from
/// a bludgeoning blow (its stat block's vulnerability). `blow` is the damage as rolled.
pub fn damage_to(critter: &Critter, blow: &Struck) -> u32 {
    if critter.spot.species.is_skeleton() {
        crate::creatures::skeletons::damage_taken(blow.damage, crate::creatures::skeletons::damage_type(blow.bludgeoning))
    } else {
        blow.damage
    }
}

fn load_creature_sounds(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    if let Some(assets) = assets {
        commands.insert_resource(CreatureSounds {
            squeak: assets.load("audio/squeak.ogg"),
            hiss: assets.load("audio/hiss.ogg"),
            rat_squeal: assets.load("audio/rat-squeal.ogg"),
            snake_rise: assets.load("audio/snake-rise.ogg"),
            thud: assets.load("audio/thud.ogg"),
        });
    }
}

/// Each creature's hit points: its stat block's (the average the SRD prints).
pub fn give_creatures_health(mut commands: Commands, rules: Option<Res<crate::rules::GameRules>>, new: Query<(Entity, &Critter), Without<Health>>) {
    let rules = crate::rules::GameRules::of(&rules);
    for (entity, critter) in &new {
        commands.entity(entity).insert(Health::full(rules.creature(critter.spot.species).hit_points as i32));
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
    tuning: Res<Tuning>,
    mut creatures: Query<(&Critter, &Transform, &mut Health), Without<Corpse>>,
    bone_sounds: Option<Res<crate::creatures::skeletons::BoneSounds>>,
    dice: Option<ResMut<super::RunDice>>,
    flinches: Flinches,
    mut callouts: MessageWriter<Callout>,
    mut slain: MessageWriter<Slain>,
) {
    let mut dice = dice;
    for blow in struck.read() {
        let Ok((critter, at, mut health)) = creatures.get_mut(blow.target) else { continue };
        let game = settings.as_deref().copied().unwrap_or_default();
        // A skeleton's shield turns some blows from the front: a clash, no harm and no flinch.
        if let Some(kit) = critter.kit.as_ref().filter(|_| critter.spot.species.is_skeleton()) {
            let roll = dice.as_deref_mut().map_or(1.0, |d| d.0.next_f64() as f32);
            if kit.blocks(at.translation.xz(), at.rotation.to_euler(EulerRot::YXZ).0, blow.from, roll) {
                if let Some(sounds) = &bone_sounds {
                    crate::creatures::skeletons::play_at(&mut commands, "shield blocks", sounds.block.clone(), at.translation, &game, &tuning);
                }
                callouts.write(Callout { target: blow.target, outcome: Outcome::Blocked });
                continue;
            }
        }
        let damage = damage_to(critter, blow);
        let fell = health.hurt(damage);
        callouts.write(Callout { target: blow.target, outcome: Outcome::Hit { damage, critical: blow.critical, fatal: fell } });
        let away = (at.translation.xz() - blow.from).normalize_or_zero();
        let push = away * knockback(&critter.creature) * tuning.combat.knockback_scale;
        commands.entity(blow.target).insert(flinches.of(blow.target, push, &tuning.combat));
        if critter.spot.species.is_skeleton() {
            // Its own clack of bone, never a rat's squeak or a snake's hiss.
            if let Some(sounds) = &bone_sounds {
                crate::creatures::skeletons::play_at(&mut commands, "bones clack", sounds.hit.clone(), at.translation, &game, &tuning);
            }
        } else if let Some(sounds) = &sounds {
            let sound = if critter.spot.species.is_rat() { sounds.squeak.clone() } else { sounds.hiss.clone() };
            commands.spawn((Name::new("creature hurt"), AudioPlayer::new(sound), crate::audio::sound_effect(&game, &tuning), DespawnOnExit(crate::launch::AppState::Delving)));
        }
        if fell {
            slain.write(Slain { target: blow.target });
            commands.entity(blow.target).insert(Corpse { since: 0.0, fell: *at }).remove::<Brain>();
            info!("{} falls", critter.spot.species.stat_block().name);
        }
    }
}

/// What [`knock_back`] moves: a creature or a folk, flinching, and its body if it fell.
type Pushed = (Option<&'static Critter>, Option<&'static Denizen>, &'static mut Flinch, &'static mut Transform, Option<&'static mut Corpse>);

/// Pushes a struck creature or folk back over its flash, round what is in the way (a folk round
/// everything but the circle it stands in itself).
fn knock_back(
    time: Res<Time>,
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Res<Obstacles>,
    mut struck: Query<Pushed>,
) {
    let dt = time.delta_secs();
    let blocked = blocked_by(&level.0, &blocked_cells);
    let relief = crate::level::geometry::Relief::new(&level.0);
    for (critter, denizen, mut flinch, mut transform, corpse) in &mut struck {
        if flinch.push == Vec2::ZERO || flinch.left <= 0.0 {
            continue;
        }
        let Some(radius) = critter.map(body_radius).or(denizen.map(|d| d.radius)) else { continue };
        let share = (dt / flinch.left).min(1.0);
        let step = flinch.push * share;
        flinch.push -= step;
        let at = transform.translation.xz();
        let own = |o: &&Obstacle| denizen.is_some() && matches!(o, Obstacle::Circle { centre, radius: r } if centre.distance(at) < 0.5 * radius && *r == radius);
        let others: Vec<Obstacle> = obstacles.0.iter().filter(|o| !own(o)).copied().collect();
        let to = slide_around(at, at + step, radius, &blocked, &others);
        transform.translation = Vec3::new(to.x, relief.floor_height(to.x, to.y), to.y);
        // A body falls where the blow put it.
        if let Some(mut corpse) = corpse {
            corpse.fell.translation = transform.translation;
        }
    }
}

/// Lights a struck creature up, held then fading ([`flash_strength`]). The first frame shows it
/// at full: the time is taken off after.
fn flash(mut commands: Commands, time: Res<Time>, tuning: Res<Tuning>, mut creatures: Query<(Entity, &mut Flinch)>, toon: Option<ResMut<Assets<ToonMaterial>>>) {
    let mut toon = toon;
    for (entity, mut flinch) in &mut creatures {
        let strength = flash_strength(flinch.left, &tuning.combat);
        if let Some(toon) = toon.as_deref_mut() {
            for (handle, colour, glow) in &flinch.materials {
                if let Some(mut material) = toon.get_mut(handle) {
                    material.base.base_color = colour.mix(&FLASH_COLOUR, FLASH_MIX * strength).with_alpha(colour.alpha());
                    material.extension.glow = glow + FLASH_GLOW * strength;
                }
            }
        }
        flinch.left -= time.delta_secs();
        if flinch.left <= 0.0 {
            if let Some(toon) = toon.as_deref_mut() {
                for (handle, colour, glow) in &flinch.materials {
                    if let Some(mut material) = toon.get_mut(handle) {
                        material.base.base_color = *colour;
                        material.extension.glow = *glow;
                    }
                }
            }
            commands.entity(entity).remove::<Flinch>();
        }
    }
}

/// Lays each body down once the flash's hold is over ([`collapse_share`]): a rat rolls onto its
/// back about its middle, a snake goes slack.
pub fn collapse(time: Res<Time>, tuning: Res<Tuning>, mut bodies: Query<(&Critter, &mut Corpse, &mut Transform, &mut Intent)>) {
    let hold = tuning.combat.flash_hold;
    for (critter, mut corpse, mut transform, mut intent) in &mut bodies {
        corpse.since += time.delta_secs();
        if corpse.since <= hold {
            continue;
        }
        // Limp: uncoiled and flat, the jaw a little open, the tongue lolling.
        *intent = Intent { coil: 0.0, rear: 0.0, strike: 0.0, jaw: 0.35, tongue: 0.6, look: 0.0, swing: None, fallen: 0.0 };
        match &critter.creature {
            Creature::Rat(rat) => *transform = rolled_over(rat, &corpse.fell, collapse_share(corpse.since, hold)),
            Creature::Skeleton(_) => {
                let share = collapse_share(corpse.since, hold);
                intent.fallen = share.max(1e-3);
                *transform = laid_back(critter.kit.as_ref().map_or(17.0, |k| k.height), &corpse.fell, share);
            }
            Creature::Snake(_) => {}
            Creature::Spider(spider) => {
                let share = collapse_share(corpse.since, hold);
                intent.fallen = share.max(1e-3);
                *transform = flipped_over(spider, &corpse.fell, share);
            }
        }
    }
}

/// A giant spider that stood at `fell`, `share` of the way through turning onto its back: rolled
/// about its own length through its body's middle, its legs drawing in under the belly as it
/// goes (`fb_creatures::pose::SpiderPose::curl`), and lowered so its back rests on the floor,
/// at the size it is drawn.
pub fn flipped_over(spider: &fb_creatures::SpiderState, fell: &Transform, share: f32) -> Transform {
    let d = spider.dims();
    let s = share * share * (3.0 - 2.0 * share);
    let roll = Quat::from_rotation_z(std::f32::consts::PI * s);
    let middle = Vec3::Y * d.centre_height;
    let drop = (d.centre_height - d.back_height()).max(0.0) * s;
    let local = middle - roll * middle - Vec3::Y * drop;
    Transform { translation: fell.translation + fell.rotation * (local * fell.scale), rotation: fell.rotation * roll, ..*fell }
}

/// A rat that stood at `fell`, `share` of the way through rolling onto its back: turned about
/// its own length through its body's middle, and lowered so its back rests on the floor, at the
/// size it is drawn.
pub fn rolled_over(rat: &fb_creatures::RatState, fell: &Transform, share: f32) -> Transform {
    let d = rat.dims();
    let s = share * share * (3.0 - 2.0 * share);
    // Creatures face +Z: rolling about it, nearly all the way over, a little to one side.
    let roll = Quat::from_rotation_z(0.9 * std::f32::consts::PI * s);
    let middle = Vec3::Y * d.centre_height;
    let drop = (d.centre_height - d.half_height).max(0.0) * s;
    let local = middle - roll * middle - Vec3::Y * drop;
    Transform { translation: fell.translation + fell.rotation * (local * fell.scale), rotation: fell.rotation * roll, ..*fell }
}

/// A skeleton of `height` that stood at `fell`, `share` of the way through falling onto its
/// back: turned about its feet, away from where it faced, and raised by half its body's depth so
/// its back rests on the floor and not in it.
pub fn laid_back(height: f32, fell: &Transform, share: f32) -> Transform {
    let s = share * share * (3.0 - 2.0 * share);
    let fall = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2 * s);
    let lift = Vec3::Y * (BACK_DEPTH * height * s * fell.scale.y);
    Transform { translation: fell.translation + lift, rotation: fell.rotation * fall, ..*fell }
}

/// How deep a lying skeleton's back is below its centre line, as a share of its height. Our own.
const BACK_DEPTH: f32 = 0.07;

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
    fn a_felled_creature_flashes_before_it_falls() {
        let combat = CombatTuning::default();
        let (time, hold) = (combat.flash_time(), combat.flash_hold);
        let strength = |left| flash_strength(left, &combat);
        // At full from the blow through the hold, then fading out.
        assert_eq!(strength(time), 1.0);
        assert_eq!(strength(time - hold + 0.01), 1.0);
        assert!(strength(0.5 * (time - hold)) < 0.5);
        assert_eq!(strength(0.0), 0.0);
        // The body stands through the hold (about a tenth of a second), then rolls.
        assert!((0.1..=0.15).contains(&hold));
        assert_eq!(collapse_share(0.0, hold), 0.0);
        assert_eq!(collapse_share(hold, hold), 0.0);
        assert!(collapse_share(hold + 0.5 * COLLAPSE_TIME, hold) > 0.4);
        assert_eq!(collapse_share(hold + COLLAPSE_TIME, hold), 1.0);
        // The flash is still at full when the roll starts.
        assert_eq!(strength(time - hold + 1e-4), 1.0);
        // A longer hold holds longer.
        let long = CombatTuning { flash_hold: 0.5, ..combat };
        assert_eq!(flash_strength(long.flash_time() - 0.4, &long), 1.0);
    }

    #[test]
    fn bigger_creatures_are_knocked_back_less() {
        let push = |species, seed| knockback(&Creature::of(species, seed));
        assert!(push(Species::Rat, 1) > push(Species::GiantRat, 1));
        assert!(push(Species::PoisonousSnake, 1) > push(Species::ConstrictorSnake, 1));
    }
}
