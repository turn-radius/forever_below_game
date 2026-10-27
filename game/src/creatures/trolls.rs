//! Trolls in the game: regenerating giants that rise again unless burned.
//!
//! A troll carries its measured joints and its [`Regeneration`] clock (`fb_creatures::troll`,
//! the rule itself, tested without Bevy) as a [`Troll`] on its root.
//!
//! - **Regaining.** Every [`TURN_SECONDS`](fb_creatures::troll::TURN_SECONDS) a living troll
//!   regains [`REGAIN`](fb_creatures::troll::REGAIN) hit points, up to its maximum.
//! - **Burning.** A blow from a lit torch counts as fire ([`crate::combat::swing::flaming_blow`]
//!   sends a [`Burned`] with it): the troll's next regeneration is stopped. A torch is held in
//!   either hand to be swung, and its flame reaches a troll lying down as well as one on its
//!   feet ([`crate::combat::swing::Strikeable::fallen_trolls`]).
//! - **Falling and rising.** At 0 hit points it falls like any creature ([`Corpse`], its mind
//!   gone). A turn after, unless it was burned since, it rises with the hit points it
//!   regenerated: its collapse is undone, its [`Corpse`] removed and a new mind given
//!   ([`brain_for`]) so it walks and fights again. Burned, it dies for good: its [`Troll`] is
//!   spent and the body stays where it lies.
//! - A body brought back from the record (a level left behind) is long past rising, and stays.
//! - **Burn.** Looking at a troll lying down with a lit torch in either hand, the delver is
//!   offered Burn on the Primary key ([`crate::interact::Verb::Burn`], [`prompt_burn`]); using
//!   it sends a [`Burned`] with no swing ([`burn_aimed`]).
//! - **Flames and char.** A burned troll has flames on its body until its turn, and its skin
//!   chars, further with each burn, fading as it regains hit points and black for good once it
//!   dies ([`super::scorch`]).
//! - **Callouts.** Each turn of its story is written over the troll ([`Outcome::Note`]): "Its
//!   wounds begin to knit" when it falls, "It burns" when fire catches it, "It rises" and
//!   "Dead for good".
//! - **Arms.** About half of trolls hold a crude weapon ([`TrollArms`], `fb_items::troll`) in the
//!   right hand, fitted as a skeleton's is (`fb_creatures::SkeletonGear`). Dead for good, a troll
//!   sets it down on the floor once ([`crate::belongings::drop_belongings`]), as a skeleton sheds its gear.

use bevy::prelude::*;
use fb_creatures::pose::ArmStance;
use fb_creatures::skeleton::SkeletonDims;
use fb_creatures::troll::{Regeneration, Turn};
use fb_creatures::{Creature, Dice, SkeletonGear};
use fb_items::Item;

use super::behaviour::brain_for;
use super::scorch::{self, Scorch};
use super::{Critter, Intent};
use crate::combat::Health;
use crate::character::Character;
use crate::combat::callouts::{Callout, Outcome, Tone};
use crate::combat::feedback::{Burned, Corpse, take_blows};
use crate::combat::swing::{Strikeable, flaming_blow};
use crate::combat::{HAND_HEIGHT, Weapon, target_in_reach_from};
use crate::inventory::Inventory;
use crate::interact::{CrosshairPrompt, Delver, Target, key_line, verbs_for};
use crate::keymap::{Action, ActionInput, Keymap};
use crate::play::{CurrentLevel, DelveSystems, Player, level_has_focus};
use fb_items::Slot;

/// What a troll's weapon adds to its damage: its Strength modifier, 4 (SRD 5.1 p. 351, "Claw:
/// 2d6 + 4"). The weapon hits at the stat block's attack bonus, +7, as the claw does.
pub const STRENGTH_BONUS: i32 = 4;

pub struct TrollsPlugin;

impl Plugin for TrollsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Burned>().add_systems(
            Update,
            (burn, fall, regenerate).chain().after(take_blows).in_set(DelveSystems),
        )
        .add_systems(
            Update,
            (
                (scorch::light_flames, scorch::douse_flames).chain().after(regenerate),
                scorch::char_trolls.after(regenerate).before(crate::combat::feedback::flash),
                scorch::dev_burn.before(take_blows),
                burn_aimed.run_if(level_has_focus).before(burn).after(crate::carry::pick_up),
                prompt_burn.after(crate::travel::prompt_stair).before(crate::interact::show_prompt),
            )
                .in_set(DelveSystems),
        );
    }
}

/// On a troll: its measured joints (so nothing rebuilds its body each frame), its height, and
/// its regeneration.
#[derive(Component, Clone, Debug)]
pub struct Troll {
    pub dims: SkeletonDims,
    /// Crown to soles, stooped, world units.
    pub height: f32,
    pub regeneration: Regeneration,
    /// It died for good: burned while down, or long fallen. Nothing brings it back.
    pub spent: bool,
}

impl Troll {
    pub fn new(state: &fb_creatures::TrollState) -> Troll {
        Troll { dims: state.dims(), height: state.height(), regeneration: Regeneration::new(), spent: false }
    }
}

/// What a troll's story says over its body.
const KNITS: Outcome = Outcome::Note { words: "Its wounds begin to knit", tone: Tone::Quiet };
const BURNS: Outcome = Outcome::Note { words: "It burns", tone: Tone::Fire };
const RISES: Outcome = Outcome::Note { words: "It rises", tone: Tone::Warning };
const DIES: Outcome = Outcome::Note { words: "Dead for good", tone: Tone::Final };

/// Fire on a troll stops its next regeneration. A troll already dead for good, or already
/// burning, has nothing to say.
fn burn(mut burned: MessageReader<Burned>, mut trolls: Query<(&mut Troll, Option<&mut Scorch>)>, mut callouts: MessageWriter<Callout>) {
    for blow in burned.read() {
        if let Ok((mut troll, scorch)) = trolls.get_mut(blow.target)
            && !troll.spent
        {
            // One burn a turn: the flames already on it chars it no further.
            if !troll.regeneration.is_stopped() {
                callouts.write(Callout { target: blow.target, outcome: BURNS });
                if let Some(mut scorch) = scorch {
                    scorch.burn();
                }
            }
            troll.regeneration.burn();
            info!("The troll burns: it will not regenerate");
        }
    }
}

/// A troll just felled has a whole turn to wait before it may rise; a body that had already
/// lain a while when it appeared (from the record) never will.
fn fall(mut trolls: Query<(Entity, &mut Troll, &Corpse), Added<Corpse>>, mut callouts: MessageWriter<Callout>) {
    for (entity, mut troll, corpse) in &mut trolls {
        if corpse.since > 1.0 {
            troll.spent = true;
        } else {
            troll.regeneration.fall();
            if !troll.regeneration.is_stopped() {
                callouts.write(Callout { target: entity, outcome: KNITS });
            }
        }
    }
}

/// What a troll's turn does to it, apart from rising: `health` regains what it regained. The
/// turn's outcome, if one ended.
pub fn take_turn(troll: &mut Troll, health: &mut Health, dt: f32) -> Option<Turn> {
    let turn = troll.regeneration.tick(dt, health.hp.max(0) as u32, health.max.max(0) as u32)?;
    match turn {
        Turn::Regained(n) => {
            health.heal(n);
        }
        Turn::Rose(n) => health.hp = n as i32,
        Turn::Died => troll.spent = true,
        Turn::Stopped => {}
    }
    Some(turn)
}

/// Each troll's turns: living ones regain, a fallen one rises or stays down.
#[allow(clippy::type_complexity)]
pub(crate) fn regenerate(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<CurrentLevel>,
    mut trolls: Query<(Entity, &Critter, &mut Troll, &mut Health, &mut Transform, &mut Intent, Option<&Corpse>, Option<&mut Scorch>)>,
    mut callouts: MessageWriter<Callout>,
) {
    for (entity, critter, mut troll, mut health, mut transform, mut intent, corpse, mut scorch) in &mut trolls {
        if troll.spent || !matches!(critter.creature, Creature::Troll(_)) {
            continue;
        }
        let max = health.max.max(0) as u32;
        let turn = take_turn(&mut troll, &mut health, time.delta_secs());
        // What it regains takes the char off its skin.
        if let (Some(scorch), Some(Turn::Regained(n) | Turn::Rose(n))) = (scorch.as_mut(), turn) {
            scorch.heal(n, max);
        }
        match turn {
            Some(Turn::Rose(hp)) => {
                let Some(corpse) = corpse else { continue };
                // On its feet where it lay, collapse undone, with a mind again.
                *transform = corpse.fell;
                *intent = Intent::default();
                commands.entity(entity).remove::<Corpse>().insert(brain_for(&level.0, critter, corpse.fell.translation.xz()));
                callouts.write(Callout { target: entity, outcome: RISES });
                info!("The troll rises with {hp} hit points");
            }
            Some(Turn::Died) => {
                if let Some(mut scorch) = scorch {
                    scorch.die();
                }
                callouts.write(Callout { target: entity, outcome: DIES });
                info!("The troll, burned, does not rise");
            }
            Some(Turn::Regained(n)) if n > 0 => debug!("The troll regains {n} hit points"),
            _ => {}
        }
    }
}

/// The troll lying down, not yet dead for good, that a lit torch in either hand reaches from
/// where the delver stands, facing as they do: the one Burn is offered on. `None` without a
/// lit torch or a troll in reach.
fn burnable_troll(at: &Transform, inventory: &Inventory, character: &Character, tuning: &crate::tuning::Tuning, strikeable: &Strikeable) -> Option<Entity> {
    let hand = [Slot::Mainhand, Slot::Offhand].into_iter().find(|&hand| flaming_blow(inventory, hand))?;
    let combat = &tuning.combat;
    let reach = Weapon::held_in(hand, inventory, character).reach * combat.reach_scale;
    target_in_reach_from(at.translation.xz(), Some(at.translation.y + HAND_HEIGHT), at.forward().as_vec3().xz(), reach, combat.hit_cone(), &strikeable.fallen_trolls())
}

/// Primary at a fallen troll with a lit torch in hand puts the flame to it, without a swing,
/// unless an item or a container is under the crosshair.
#[allow(clippy::too_many_arguments)]
fn burn_aimed(
    input: ActionInput,
    aimed: Option<Res<crate::carry::Aimed>>,
    inventory: Res<Inventory>,
    character: Res<Character>,
    tuning: Res<crate::tuning::Tuning>,
    players: Query<&Transform, With<Player>>,
    strikeable: Strikeable,
    mut burned: MessageWriter<Burned>,
) {
    if !input.just_pressed(Action::Primary) || aimed.is_some_and(|a| a.0.is_some()) {
        return;
    }
    let Some(at) = players.iter().next() else { return };
    if let Some(target) = burnable_troll(at, &inventory, &character, &tuning, &strikeable) {
        burned.write(Burned { target });
    }
}

/// Says Burn over the crosshair while [`burn_aimed`] would. Runs after the stair's prompt and
/// clears only its own line.
#[allow(clippy::too_many_arguments)]
fn prompt_burn(
    aimed: Option<Res<crate::carry::Aimed>>,
    inventory: Res<Inventory>,
    character: Res<Character>,
    tuning: Res<crate::tuning::Tuning>,
    keymap: Option<Res<Keymap>>,
    players: Query<&Transform, With<Player>>,
    strikeable: Strikeable,
    mut prompt: ResMut<CrosshairPrompt>,
    mut said: Local<Option<String>>,
) {
    let line = (|| {
        if aimed.is_some_and(|a| a.0.is_some()) {
            return None;
        }
        burnable_troll(players.iter().next()?, &inventory, &character, &tuning, &strikeable)?;
        let fallback = Keymap::default();
        Some(key_line(keymap.as_deref().unwrap_or(&fallback), &verbs_for(&Target::FallenTroll { torch: true }, &Delver::default()))).filter(|line| !line.is_empty())
    })();
    if *said != line {
        if line.is_some() || prompt.0 == *said {
            prompt.0.clone_from(&line);
        }
        *said = line;
    } else if line.is_some() && prompt.0 != line {
        prompt.0.clone_from(&line);
    }
}

/// What a troll holds, worked out once when it appears: the weapon it rolled, the stance of the
/// arm that holds it (right then left) and the blow it deals in place of a claw.
#[derive(Clone, Debug)]
pub struct TrollArms {
    pub weapon: Item,
    pub stance: [Option<ArmStance>; 2],
    /// The weapon's dice and the troll's Strength modifier.
    pub blow: Dice,
}

impl TrollArms {
    /// The arms of a troll fitted with `gear`, or `None` for one with nothing to hold.
    pub fn of(gear: &SkeletonGear) -> Option<TrollArms> {
        let weapon = gear.outfit.item(Slot::Mainhand)?.clone();
        let dice = weapon.profile.stats.damage?;
        Some(TrollArms { blow: Dice::new(dice.count as u32, dice.sides as u32, STRENGTH_BONUS), stance: gear.stance, weapon })
    }

    /// The blows of one attack, in order: the bite, the weapon in place of the right claw, and
    /// the left claw.
    pub fn attack(&self, bite: Dice) -> [Dice; 3] {
        [bite, self.blow, fb_creatures::troll::CLAW]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_creatures::{Species, TrollState};
    use crate::creatures::skeletons::Looted;
    use crate::inventory::{FloorChanges, Inventory};

    /// A seed whose troll holds a weapon.
    fn armed_seed() -> u32 {
        (0..40).find(|&s| !fb_items::troll::troll_outfit(s, fb_items::Odds::kit(1)).worn.is_empty()).expect("some troll is armed")
    }

    #[test]
    fn an_armed_trolls_attack_is_a_bite_the_weapon_and_a_claw() {
        let seed = armed_seed();
        let gear = SkeletonGear::roll_troll(&TrollState::roll(seed), fb_items::Odds::kit(1));
        let arms = TrollArms::of(&gear).expect("armed");
        let bite = Species::Troll.stat_block().bite;
        let [first, second, third] = arms.attack(bite);
        assert_eq!(first, bite);
        let dice = gear.outfit.item(Slot::Mainhand).unwrap().profile.stats.damage.unwrap();
        assert_eq!(second, Dice::new(dice.count as u32, dice.sides as u32, 4), "the weapon's dice and the Strength modifier");
        assert_eq!(third, fb_creatures::troll::CLAW);
        assert_eq!(Species::Troll.stat_block().attack_bonus, 7);
    }

    // The troll in play: spawned with the whole of combat running, struck, burned and left.

    use crate::combat::feedback::Struck;
    use crate::combat::swing::Swing;
    use crate::combat::{CombatPlugin, RunDice, Weapon};
    use crate::creatures::behaviour::{AttackOnDelver, BehaviourPlugin, Brain, MissedDelver};
    use crate::creatures::{animate, idle, spawn_creature_with};
    use crate::inventory::underfoot::apply_floor_changes;
    use crate::play::Player;
    use bevy::time::TimeUpdateStrategy;
    use crate::character::Character;

    /// How far from the troll the delver stands, world units: a club's swing from its hide.
    const AWAY: f32 = 12.0;
    /// Out of its senses, and of its reach.
    const FAR: f32 = 600.0;

    /// The troll of `seed` in the level's start, in an app that runs the whole of combat and the
    /// trolls, stepped 1/60 s a frame, with a delver holding a torch (lit or not) `AWAY` from it
    /// and facing it. The app, the delver and the troll.
    fn troll_fight(seed: u32, torch_lit: bool, away: f32) -> (App, Entity, Entity) {
        let weak = |n: u128| Handle::<AudioSource>::Uuid(bevy::asset::uuid::Uuid::from_u128(n), std::marker::PhantomData);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin, bevy::state::app::StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .insert_state(crate::launch::AppState::Delving)
            .init_resource::<crate::keymap::Keymap>()
            .init_resource::<Character>()
            .init_resource::<crate::play::ScreenOpen>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::render::ToonMaterial>>()
            .init_resource::<Assets<crate::render::OutlineMaterial>>()
            .init_resource::<crate::render::OutlineMaterials>()
            .init_resource::<FloorChanges>()
            .init_resource::<CrosshairPrompt>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_plugins((BehaviourPlugin, CombatPlugin, TrollsPlugin, crate::belongings::BelongingsPlugin))
            .insert_resource(RunDice(procgen_rng::Mulberry32::new(9)))
            .insert_resource(crate::combat::feedback::CreatureSounds { squeak: weak(1), hiss: weak(2), rat_squeal: weak(3), snake_rise: weak(4), thud: weak(5) })
            .add_systems(Update, (idle, animate, apply_floor_changes).chain());
        let mut level = fb_level::generate(1207, 1);
        level.items.clear();
        level.creatures.clear();
        let spot = crate::creatures::dev_spots_for(&level, &format!("troll:{seed}")).remove(0);
        app.insert_resource(CurrentLevel(level.clone()));
        let built = crate::creatures::build_creature(&spot, level.depth);
        let rig = built.gear.as_ref().map_or_else(|| built.creature.rig(), |g| g.rig.clone());
        let troll = {
            let mut commands = app.world_mut().commands();
            spawn_creature_with(&mut commands, &level, &spot, &rig, None, built.arms).0
        };
        app.world_mut().flush();
        let mut inventory = Inventory::default();
        let torch = inventory.add(fb_items::roll_item("Torch", 7).unwrap());
        inventory.wear(torch, Slot::Mainhand).unwrap();
        if torch_lit {
            inventory.toggle_light(torch).unwrap();
        }
        app.insert_resource(inventory);
        app.update();
        let at = app.world().get::<Transform>(troll).unwrap().translation;
        let from = at + Vec3::X * away;
        let player = app.world_mut().spawn((Player, Transform::from_translation(from).looking_at(at, Vec3::Y))).id();
        (app, player, troll)
    }

    fn strike_down(app: &mut App, troll: Entity) {
        for _ in 0..400 {
            if app.world().get::<Corpse>(troll).is_some() {
                return;
            }
            app.world_mut().write_message(Struck { target: troll, damage: 500, from: Vec2::new(0.0, 0.0), bludgeoning: true, critical: false, by_ally: false, graze: false });
            app.update();
        }
        panic!("the troll never fell");
    }

    fn run(app: &mut App, seconds: f32) {
        for _ in 0..(seconds * 60.0) as usize {
            app.update();
        }
    }

    /// The delver swings what they hold at the troll in front of them and the blow lands.
    fn swing(app: &mut App, player: Entity, troll: Entity) {
        app.world_mut().entity_mut(troll).insert(crate::combat::ArmourClass(-100));
        let weapon = Weapon::in_hand(app.world().resource::<Inventory>(), app.world().resource::<Character>());
        app.world_mut().entity_mut(player).insert(Swing::new(weapon));
        run(app, 0.6);
    }

    fn health(app: &App, troll: Entity) -> i32 {
        app.world().get::<Health>(troll).unwrap().hp
    }

    #[test]
    fn a_troll_struck_down_rises_after_its_regeneration_with_its_mind_and_body_back() {
        let (mut app, player, troll) = troll_fight(unarmed_seed(), false, FAR);
        assert_eq!(health(&app, troll), 84, "the SRD's 84 hit points");
        strike_down(&mut app, troll);
        run(&mut app, 1.0);
        assert_eq!(health(&app, troll), 0);
        assert!(app.world().get::<Brain>(troll).is_none(), "a body has no mind");
        let lying = app.world().get::<Transform>(troll).unwrap().rotation * Vec3::Y;
        assert!(lying.y.abs() < 0.3, "laid on its back: {lying}");
        assert!(app.world().get::<Intent>(troll).unwrap().fallen > 0.9);
        // A whole turn after it fell, it rises.
        run(&mut app, 6.0);
        assert!(app.world().get::<Corpse>(troll).is_none(), "it got up");
        assert_eq!(health(&app, troll), 10, "with the hit points it regenerated");
        assert!(app.world().get::<Brain>(troll).is_some(), "with its mind back");
        let up = app.world().get::<Transform>(troll).unwrap().rotation * Vec3::Y;
        assert!(up.y > 0.95, "on its feet: {up}");
        assert_eq!(app.world().get::<Intent>(troll).unwrap().fallen, 0.0);
        assert!(!app.world().get::<Troll>(troll).unwrap().spent);
        // And it fights: with the delver brought to its side it goes for them.
        let at = app.world().get::<Transform>(troll).unwrap().translation;
        app.world_mut().get_mut::<Transform>(player).unwrap().translation = at + Vec3::X * AWAY;
        // (It stops at its first blow: a delver it fells ends the run.)
        let mut blows = 0;
        for _ in 0..600 {
            app.update();
            blows = app.world().resource::<Messages<AttackOnDelver>>().iter_current_update_messages().count() + app.world().resource::<Messages<MissedDelver>>().iter_current_update_messages().count();
            if blows > 0 {
                break;
            }
        }
        assert_eq!(blows, 3, "a risen troll goes for the delver, a bite and two claws (or its weapon) in one swing");
        // And it can be struck down again.
        app.world_mut().get_mut::<Transform>(player).unwrap().translation = at + Vec3::X * FAR;
        strike_down(&mut app, troll);
        assert_eq!(health(&app, troll), 0);
    }

    /// A seed whose troll holds nothing.
    fn unarmed_seed() -> u32 {
        (0..40).find(|&s| fb_items::troll::troll_outfit(s, fb_items::Odds::kit(1)).worn.is_empty()).expect("some troll is unarmed")
    }

    #[test]
    fn a_lit_torch_blow_stops_the_next_regeneration() {
        let burned = |lit: bool| {
            let (mut app, player, troll) = troll_fight(unarmed_seed(), lit, AWAY);
            // Wounded first, so there is something to regain; the torch's blow does the little it does.
            app.world_mut().get_mut::<Health>(troll).unwrap().hp = 50;
            swing(&mut app, player, troll);
            let after_blow = health(&app, troll);
            assert!(after_blow <= 50, "the blow landed: {after_blow}");
            assert_eq!(app.world().get::<Troll>(troll).unwrap().regeneration.is_stopped(), lit, "lit: {lit}");
            // Out of its reach, so only its turn changes its health.
            app.world_mut().get_mut::<Transform>(player).unwrap().translation += Vec3::X * 400.0;
            run(&mut app, 6.5);
            health(&app, troll) - after_blow
        };
        assert_eq!(burned(false), 10, "a troll regains 10 each turn");
        assert_eq!(burned(true), 0, "burned, it regains nothing");
    }

    #[test]
    fn a_troll_burned_while_down_stays_dead_and_drops_its_weapon_once() {
        let seed = armed_seed();
        let (mut app, player, troll) = troll_fight(seed, true, AWAY);
        strike_down(&mut app, troll);
        run(&mut app, 1.5);
        // The delver puts the lit torch to the fallen troll.
        swing(&mut app, player, troll);
        assert!(app.world().get::<Troll>(troll).unwrap().regeneration.is_stopped(), "the fire caught it where it lay");
        run(&mut app, 6.5);
        assert!(app.world().get::<Corpse>(troll).is_some(), "it did not rise");
        assert!(app.world().get::<Troll>(troll).unwrap().spent, "dead for good");
        assert_eq!(health(&app, troll), 0);
        let on_the_floor = |app: &mut App| app.world_mut().query::<&crate::carry::FloorItem>().iter(app.world()).count();
        assert_eq!(on_the_floor(&mut app), 1, "its weapon lies where it fell");
        assert!(app.world().get::<Looted>(troll).is_some());
        run(&mut app, 20.0);
        assert!(app.world().get::<Corpse>(troll).is_some() && health(&app, troll) == 0);
        assert_eq!(on_the_floor(&mut app), 1, "set down once and never again");
    }

    #[test]
    fn a_troll_felled_and_left_to_burn_by_other_means_is_just_as_dead() {
        // The message itself, as any fire would send it.
        let (mut app, _, troll) = troll_fight(armed_seed(), false, FAR);
        strike_down(&mut app, troll);
        app.world_mut().write_message(Burned { target: troll });
        run(&mut app, 7.5);
        assert!(app.world().get::<Corpse>(troll).is_some() && app.world().get::<Troll>(troll).unwrap().spent);
    }

    /// A second, standing troll of `seed`, set `beside` the first on the floor.
    fn another_troll(app: &mut App, seed: u32, beside: Vec3) -> Entity {
        let level = app.world().resource::<CurrentLevel>().0.clone();
        let spot = crate::creatures::dev_spots_for(&level, &format!("troll:{seed}")).remove(0);
        let built = crate::creatures::build_creature(&spot, level.depth);
        let rig = built.gear.as_ref().map_or_else(|| built.creature.rig(), |g| g.rig.clone());
        let troll = {
            let mut commands = app.world_mut().commands();
            spawn_creature_with(&mut commands, &level, &spot, &rig, None, built.arms).0
        };
        app.world_mut().flush();
        app.update();
        app.world_mut().get_mut::<Transform>(troll).unwrap().translation = beside;
        troll
    }

    #[test]
    fn a_troll_felled_by_a_lit_torch_blow_stays_dead() {
        let (mut app, player, troll) = troll_fight(unarmed_seed(), true, AWAY);
        app.world_mut().get_mut::<Health>(troll).unwrap().hp = 1;
        swing(&mut app, player, troll);
        assert!(app.world().get::<Corpse>(troll).is_some(), "the blow felled it");
        app.world_mut().get_mut::<Transform>(player).unwrap().translation += Vec3::X * 400.0;
        run(&mut app, 7.0);
        assert!(app.world().get::<Corpse>(troll).is_some(), "it did not rise");
        assert!(app.world().get::<Troll>(troll).unwrap().spent, "dead for good");
    }

    #[test]
    fn a_burn_just_before_its_turn_still_kills_a_fallen_troll() {
        let (mut app, _, troll) = troll_fight(unarmed_seed(), false, FAR);
        strike_down(&mut app, troll);
        run(&mut app, 5.5);
        assert!(!app.world().get::<Troll>(troll).unwrap().spent);
        app.world_mut().write_message(Burned { target: troll });
        run(&mut app, 1.0);
        assert!(app.world().get::<Corpse>(troll).is_some() && app.world().get::<Troll>(troll).unwrap().spent, "its turn found it burned");
    }

    #[test]
    fn a_lit_torch_burns_a_fallen_troll_even_while_a_standing_one_takes_the_blow() {
        let (mut app, player, fallen) = troll_fight(unarmed_seed(), true, AWAY);
        strike_down(&mut app, fallen);
        run(&mut app, 1.5);
        let at = app.world().get::<Transform>(fallen).unwrap().translation;
        // The standing one is nearer the delver, between them and the body.
        let standing = another_troll(&mut app, unarmed_seed() + 1, at + Vec3::X * 5.0);
        app.world_mut().entity_mut(standing).insert(crate::combat::ArmourClass(-100));
        let before = health(&app, standing);
        swing(&mut app, player, standing);
        assert!(health(&app, standing) < before, "the standing troll took the blow");
        assert!(app.world().get::<Troll>(fallen).unwrap().regeneration.is_stopped(), "and the one lying down burns");
    }

    #[test]
    fn burn_at_a_fallen_troll_with_a_lit_torch_burns_it_without_a_swing() {
        let (mut app, player, troll) = troll_fight(unarmed_seed(), true, AWAY);
        strike_down(&mut app, troll);
        run(&mut app, 1.5);
        app.update();
        assert_eq!(app.world().resource::<CrosshairPrompt>().0.as_deref(), Some("F Burn"));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyF);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
        assert!(app.world().get::<Swing>(player).is_none(), "no swing");
        assert!(app.world().get::<Troll>(troll).unwrap().regeneration.is_stopped());
        run(&mut app, 6.5);
        assert!(app.world().get::<Troll>(troll).unwrap().spent, "dead for good");
        app.update();
        assert_eq!(app.world().resource::<CrosshairPrompt>().0, None, "nothing left to burn");
    }

    #[test]
    fn no_burn_is_offered_without_a_lit_torch() {
        let (mut app, _, troll) = troll_fight(unarmed_seed(), false, AWAY);
        strike_down(&mut app, troll);
        run(&mut app, 1.5);
        assert_eq!(app.world().resource::<CrosshairPrompt>().0, None);
    }

    #[test]
    fn a_trolls_story_is_called_out_over_it() {
        let (mut app, _, troll) = troll_fight(unarmed_seed(), false, FAR);
        let mut said: Vec<&'static str> = Vec::new();
        let mut note = |app: &mut App| {
            for callout in app.world().resource::<Messages<Callout>>().iter_current_update_messages() {
                if let Outcome::Note { words, .. } = callout.outcome {
                    said.push(words);
                }
            }
        };
        for _ in 0..400 {
            if app.world().get::<Corpse>(troll).is_some() {
                break;
            }
            app.world_mut().write_message(Struck { target: troll, damage: 500, from: Vec2::ZERO, bludgeoning: true, critical: false, by_ally: false, graze: false });
            app.update();
            note(&mut app);
        }
        for step in 0..(8.0 * 60.0) as usize {
            if step == 60 * 2 {
                app.world_mut().write_message(Burned { target: troll });
            }
            app.update();
            note(&mut app);
        }
        assert_eq!(said, ["Its wounds begin to knit", "It burns", "Dead for good"]);
    }

    // Fire on the body: flames and char.

    use crate::creatures::scorch::{Aflame, CHAR, Fire, Scorch};
    use crate::creatures::Bones;
    use crate::render::ToonMaterial;

    /// The green a dressed troll's parts start as.
    const SKIN: Color = Color::srgb(0.30, 0.45, 0.20);

    /// Gives `troll` a toon material of its own on each of its first bones, as spawning gives
    /// every part, and returns their handles.
    fn dress(app: &mut App, troll: Entity) -> Vec<Handle<ToonMaterial>> {
        let bones: Vec<Entity> = app.world().get::<Bones>(troll).unwrap().0.iter().take(4).copied().collect();
        bones
            .into_iter()
            .map(|bone| {
                let material = ToonMaterial { base: StandardMaterial { base_color: SKIN, ..default() }, extension: default() };
                let handle = app.world_mut().resource_mut::<Assets<ToonMaterial>>().add(material);
                app.world_mut().spawn((MeshMaterial3d(handle.clone()), ChildOf(bone)));
                handle
            })
            .collect()
    }

    fn colour_of(app: &App, handle: &Handle<ToonMaterial>) -> Srgba {
        app.world().resource::<Assets<ToonMaterial>>().get(handle).unwrap().base.base_color.to_srgba()
    }

    fn glow_of(app: &App, handle: &Handle<ToonMaterial>) -> f32 {
        app.world().resource::<Assets<ToonMaterial>>().get(handle).unwrap().extension.glow
    }

    fn luma(colour: Srgba) -> f32 {
        colour.red + colour.green + colour.blue
    }

    fn flames(app: &mut App) -> usize {
        app.world_mut().query::<&Fire>().iter(app.world()).count()
    }

    fn scorch_of(app: &App, troll: Entity) -> Scorch {
        app.world().get::<Scorch>(troll).unwrap().clone()
    }

    #[test]
    fn charring_darkens_only_the_troll_that_burned() {
        let (mut app, _, burnt) = troll_fight(unarmed_seed(), false, FAR);
        let burnt_skin = dress(&mut app, burnt);
        let other = another_troll(&mut app, unarmed_seed() + 1, Vec3::new(0.0, 0.0, 300.0));
        let other_skin = dress(&mut app, other);
        app.world_mut().write_message(Burned { target: burnt });
        run(&mut app, 2.0);
        for handle in &burnt_skin {
            assert!(luma(colour_of(&app, handle)) < 0.7 * luma(SKIN.to_srgba()), "the burned troll's skin is charred");
        }
        for handle in &other_skin {
            assert_eq!(colour_of(&app, handle), SKIN.to_srgba(), "the other troll is untouched");
        }
    }

    #[test]
    fn each_burn_takes_the_char_further_but_one_burn_a_turn() {
        let (mut app, _, troll) = troll_fight(unarmed_seed(), false, FAR);
        dress(&mut app, troll);
        let mut levels = Vec::new();
        for _ in 0..3 {
            app.world_mut().write_message(Burned { target: troll });
            app.update();
            levels.push(scorch_of(&app, troll).level);
            // Its turn comes and goes, the fire out, before the next burn.
            run(&mut app, 6.5);
        }
        assert!(levels[0] > 0.4 && levels[0] < levels[1] && levels[1] < levels[2], "{levels:?}");
        // Burned again while it already burns: no further.
        let (mut app, _, troll) = troll_fight(unarmed_seed(), false, FAR);
        dress(&mut app, troll);
        for _ in 0..3 {
            app.world_mut().write_message(Burned { target: troll });
            app.update();
        }
        assert_eq!(scorch_of(&app, troll).level, levels[0], "one burn a turn");
    }

    #[test]
    fn a_flash_on_a_charred_troll_comes_back_to_the_char() {
        let (mut app, _, troll) = troll_fight(unarmed_seed(), false, FAR);
        let skin = dress(&mut app, troll);
        app.world_mut().write_message(Burned { target: troll });
        run(&mut app, 2.0);
        let charred = colour_of(&app, &skin[0]);
        assert!(luma(charred) < luma(SKIN.to_srgba()));
        let struck = Struck { target: troll, damage: 1, from: Vec2::new(0.0, 50.0), bludgeoning: true, critical: false, by_ally: false, graze: false };
        app.world_mut().write_message(struck);
        app.update();
        assert!(luma(colour_of(&app, &skin[0])) > luma(charred) + 0.2, "it flashes");
        run(&mut app, 1.5);
        assert!(app.world().get::<crate::combat::feedback::Flinch>(troll).is_none(), "the flash is over");
        assert_eq!(colour_of(&app, &skin[0]), charred, "back to the char, not the green");
        // Burned again in the middle of a flash (after its turn), it comes back to the new char.
        run(&mut app, 5.0);
        app.world_mut().write_message(struck);
        app.update();
        app.world_mut().write_message(Burned { target: troll });
        run(&mut app, 1.5);
        let darker = colour_of(&app, &skin[0]);
        assert!(luma(darker) < luma(charred), "the second burn went further: {darker:?} against {charred:?}");
    }

    #[test]
    fn flames_stand_on_a_burning_troll_and_are_gone_after_its_turn() {
        let (mut app, _, troll) = troll_fight(unarmed_seed(), false, FAR);
        assert_eq!(flames(&mut app), 0);
        app.world_mut().write_message(Burned { target: troll });
        run(&mut app, 0.5);
        let lit = flames(&mut app);
        assert!((1..=crate::creatures::scorch::FLAMES).contains(&lit), "flames on the body: {lit}");
        assert!(app.world().get::<Aflame>(troll).is_some());
        // Each is an Armory flame, animated as a torch's: a veil and a core.
        let animated = app.world_mut().query::<&crate::render::flame::Flame>().iter(app.world()).count();
        assert_eq!(animated, 2 * lit);
        // Still burning until its turn.
        run(&mut app, 4.0);
        assert_eq!(flames(&mut app), lit);
        // Its turn comes: they gutter out over a second.
        run(&mut app, 1.5);
        let sizes: Vec<f32> = app.world_mut().query::<&Fire>().iter(app.world()).map(Fire::size).collect();
        assert!(!sizes.is_empty() && sizes.iter().all(|&size| size < 1.0), "guttering: {sizes:?}");
        run(&mut app, 1.5);
        assert_eq!(flames(&mut app), 0, "gone");
        assert!(app.world().get::<Aflame>(troll).is_none());
    }

    #[test]
    fn a_troll_dead_for_good_is_fully_charred_glows_and_stays_charred() {
        let (mut app, _, troll) = troll_fight(unarmed_seed(), false, FAR);
        let skin = dress(&mut app, troll);
        strike_down(&mut app, troll);
        app.world_mut().write_message(Burned { target: troll });
        run(&mut app, 7.0);
        assert!(app.world().get::<Troll>(troll).unwrap().spent);
        let scorch = scorch_of(&app, troll);
        assert_eq!(scorch.level, 1.0);
        assert!(scorch.ember > 0.5, "glowing: {}", scorch.ember);
        assert!(glow_of(&app, &skin[0]) > 0.1, "a dull glow in the skin");
        // The ember dies down over some seconds.
        run(&mut app, 9.0);
        assert_eq!(scorch_of(&app, troll).ember, 0.0);
        assert_eq!(glow_of(&app, &skin[0]), 0.0);
        assert_eq!(colour_of(&app, &skin[0]), CHAR.to_srgba(), "charred through");
        run(&mut app, 20.0);
        assert_eq!(colour_of(&app, &skin[0]), CHAR.to_srgba(), "and it stays so");
        assert_eq!(flames(&mut app), 0, "the flames are out");
    }

    #[test]
    fn a_living_trolls_scorch_fades_as_it_regains_hit_points() {
        let (mut app, _, troll) = troll_fight(unarmed_seed(), false, FAR);
        let skin = dress(&mut app, troll);
        app.world_mut().get_mut::<Health>(troll).unwrap().hp = 20;
        app.world_mut().write_message(Burned { target: troll });
        run(&mut app, 1.0);
        let burnt = scorch_of(&app, troll).level;
        assert!(burnt > 0.4);
        // The burned turn regains nothing; the next regains 10 and takes some of the char off.
        run(&mut app, 5.5);
        assert_eq!(scorch_of(&app, troll).level, burnt, "burned, it regains nothing");
        run(&mut app, 6.0);
        let after = scorch_of(&app, troll).level;
        assert!(after < burnt && after > 0.0, "{after} against {burnt}");
        // Hit points all back, the scorch is gone and the skin its own again.
        run(&mut app, 30.0);
        assert_eq!(scorch_of(&app, troll).level, 0.0);
        run(&mut app, 1.0);
        assert_eq!(colour_of(&app, &skin[0]), SKIN.to_srgba());
    }

    #[test]
    fn a_charred_body_from_the_record_is_charred_and_not_glowing() {
        let (mut app, _, troll) = troll_fight(unarmed_seed(), false, FAR);
        let skin = dress(&mut app, troll);
        // As the level spawns it: the record's char, and the long-fallen body.
        let at = *app.world().get::<Transform>(troll).unwrap();
        app.world_mut().entity_mut(troll).insert((Scorch::restored(1.0), Corpse { since: 60.0, fell: at }));
        run(&mut app, 0.5);
        assert_eq!(colour_of(&app, &skin[0]), CHAR.to_srgba());
        assert_eq!(glow_of(&app, &skin[0]), 0.0, "long cold");
        assert!(app.world().get::<Troll>(troll).unwrap().spent);
        assert_eq!(flames(&mut app), 0);
    }

    #[test]
    fn a_trolls_char_is_written_to_the_record() {
        let (mut app, _, troll) = troll_fight(unarmed_seed(), false, FAR);
        strike_down(&mut app, troll);
        app.world_mut().write_message(Burned { target: troll });
        run(&mut app, 7.0);
        let spot = app.world().get::<Critter>(troll).unwrap().spot;
        app.world_mut().resource_mut::<CurrentLevel>().0.creatures.push(spot);
        app.init_resource::<crate::record::WorldRecord>();
        bevy::ecs::system::RunSystemOnce::run_system_once(app.world_mut(), crate::record::record_level_left).unwrap();
        let depth = app.world().resource::<CurrentLevel>().0.depth;
        let record = app.world().resource::<crate::record::WorldRecord>().level(depth).unwrap().creatures.values().next().cloned().unwrap();
        assert!(record.dead);
        assert_eq!(record.charred, 1.0);
        // An older record has no char.
        let old: crate::record::CreatureRecord = serde_json::from_str(r#"{"hp":0,"max":84,"at":[0,0,0],"turn":[0,0,0,1],"scale":1.0,"dead":true}"#).unwrap();
        assert_eq!(old.charred, 0.0);
    }
}
