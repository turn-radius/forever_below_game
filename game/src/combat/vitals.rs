//! The delver's hit points: bites take them, the view shows them, and at 0 the run ends.
//!
//! - The delver starts with a 1st-level character's hit points
//!   ([`super::first_level_hit_points_with`]).
//! - The delver carries an [`ArmourClass`], the one the sheet shows (10, the Dexterity modifier
//!   and what they wear, `Character::armour_class`), kept up as they dress ([`armour_the_delver`]).
//!   Every creature's and folk's blow rolls to hit against it
//!   (`creatures::behaviour::BlowsAtDelver`); one that misses whooshes past ([`whoosh_past`]).
//! - Each [`AttackOnDelver`] (a blow that hit) takes its bite, and a poisonous snake's venom after the delver's
//!   Constitution save ([`super::bite_damage_with`]). The whole view flashes red, harder for a harder blow ([`flash_strength`]).
//!   A failed save also poisons the delver for a while ([`DelverEffects`],
//!   [`fb_character::poison`]), which turns the life orb green ([`super::orbs`]).
//! - A constrictor's bite also holds the delver ([`Constricted`]), who moves at a quarter of
//!   their pace until they strike it (or it dies, or they break more than a body's length
//!   away). The SRD's constrictor grapples with a separate Constrict attack and restrains its
//!   target, whose speed becomes 0 (SRD 5.1 p. 370, "Constrict"; p. 358, "Grappled"); the
//!   slowing, and breaking free by a blow, are our own placeholder.
//! - Each blow also draws a grunt of pain from the delver ([`super::voice`]).
//! - The hit points show as the life orb at the bottom left of the view ([`super::orbs`]).
//! - At 0 they have a last gasp ([`Dying`], [`LAST_GASP`] seconds) to drink a healing draught,
//!   while blood blooms in from the view's edge and blinks ([`super::blood`], [`gasp_blink`]);
//!   healed above 0 they stand on. Otherwise the run ends ([`Fallen`]): the delver stops, can no
//!   longer swing, shoot or guard, the view fades to near black with "You
//!   fall in the abyss", and after [`FALL_TIME`] seconds the game goes back to the title, where
//!   New delve begins again. Leaving the delve clears everything it spawned (each is scoped
//!   to it, [`crate::play::DelveSystems`]) and what it kept ([`end_the_delve`]). The SRD's death saving throws (p. 97) are left out.
//! - Brought down by a blow with a revive left in this world, the delver falls unconscious
//!   instead and chooses to come to or let go ([`super::revival`]).

use bevy::{
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_creatures::Species;

use super::feedback::{Corpse, Slain, Struck};
use super::{ArmourClass, Health, RunDice, bite_damage_with, first_level_hit_points_with};
use crate::carry::{Aimed, Hud, ViewModelCamera};
use crate::character::{Character, DelverEffects};
use crate::creatures::Critter;
use crate::creatures::behaviour::{AttackOnDelver, MissedDelver, reach};
use crate::exhaustion::Exhaustion;
use crate::inventory::Inventory;
use crate::launch::AppState;
use crate::rules::GameRules;
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, LevelRooms, Obstacles, Player, ScreenOpen};
use crate::player::Slowed;
use crate::ui::{BAD, BG, BRIGHT, UiFonts};

pub struct VitalsPlugin;

impl Plugin for VitalsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AttackOnDelver>()
            .add_message::<super::voice::DelverHurt>()
            .add_message::<MissedDelver>()
            .add_message::<Struck>()
            .add_message::<Slain>()
            .init_resource::<Pain>()
            .init_resource::<crate::rules::GameRules>()
            .add_systems(OnExit(AppState::Delving), end_the_delve)
            .add_systems(Update, (give_player_health, fit_hit_points, armour_the_delver, take_bites, heal_on_kill, whoosh_past, show_vitals, break_free, fall).chain().in_set(DelveSystems))
            .add_systems(Update, spawn_vitals.in_set(DelveSystems));
    }
}

/// How long the red flash over the whole view fades out after a blow, seconds. Our own.
pub const FLASH_TIME: f32 = 0.33;
/// The faintest red flash, for a blow of next to no damage. Our own.
pub const FLASH_FLOOR: f32 = 0.08;
/// The strongest red flash, however hard the blow. Our own.
pub const FLASH_CAP: f32 = 0.6;
/// How long "You fall in the abyss" shows before the title, seconds. Our own.
pub const FALL_TIME: f32 = 3.5;
/// How long a delver at 0 hit points has to drink a healing draught before they fall, seconds.
/// Our own, long enough to reach a quickslot or the inventory once the edge starts blinking.
pub const LAST_GASP: f32 = 3.0;
/// How often the view's edge blinks red through the last gasp, a second. Our own, kept under the
/// three flashes a second that WCAG 2.3.1 allows.
pub const GASP_BLINKS: f32 = 2.0;
/// How far in the blood reaches as the last gasp starts and as it runs out, in screen heights.
/// Our own.
pub const GASP_SPREAD: (f32, f32) = (0.08, 0.2);
/// How fast a held delver moves, as a share of their pace. Our own placeholder.
pub const HELD_PACE: f32 = 0.25;

/// Below this share of maximum hit points the view's edge darkens. Our own.
pub const HURT_BADLY: f32 = 0.25;
/// Seconds one slow pulse of the darkened edge takes. Our own.
pub const VIGNETTE_PULSE: f32 = 3.0;
/// The darkened edge's colour: a dark red. Our own.
const VIGNETTE_COLOUR: Color = Color::srgb(0.32, 0.02, 0.03);

/// How strong the dark red edge is, 0 to 1, for `hp` of `max` hit points `seconds` into the
/// delve: nothing at a quarter of `max` or more, and below that stronger the nearer 0, with a
/// slow pulse. Our own.
pub fn vignette_strength(hp: i32, max: i32, seconds: f32) -> f32 {
    let share = hp.max(0) as f32 / max.max(1) as f32;
    if share >= HURT_BADLY {
        return 0.0;
    }
    let base = 0.35 + 0.65 * (1.0 - share / HURT_BADLY);
    let pulse = 0.85 + 0.15 * (std::f32::consts::TAU * seconds / VIGNETTE_PULSE).sin();
    base * pulse
}

/// How strong the blood blinks `into` seconds into the last gasp, 0.3 to 1: at full as it
/// starts, [`GASP_BLINKS`] times a second. Our own.
pub fn gasp_blink(into: f32) -> f32 {
    0.3 + 0.7 * (0.5 + 0.5 * (std::f32::consts::TAU * GASP_BLINKS * into.max(0.0)).cos())
}

/// How opaque the red flash over the view starts, 0 to 1, for a blow of `damage` against `max`
/// hit points: [`FLASH_FLOOR`] for a scratch, rising with the share of hit points taken, up to
/// [`FLASH_CAP`]. Our own.
pub fn flash_strength(damage: i32, max: i32) -> f32 {
    let share = damage.max(0) as f32 / max.max(1) as f32;
    (FLASH_FLOOR + 0.9 * share).clamp(FLASH_FLOOR, FLASH_CAP)
}

/// The red flash over the whole view: how strong it began and how many seconds it has left.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Pain {
    pub strength: f32,
    pub left: f32,
}

impl Pain {
    /// The flash's opacity now: its strength, fading linearly to nothing over [`FLASH_TIME`].
    pub fn alpha(&self) -> f32 {
        self.strength * (self.left / FLASH_TIME).clamp(0.0, 1.0)
    }

    /// A blow of `strength` flashes afresh unless a stronger flash is still showing.
    fn strike(&mut self, strength: f32) {
        if strength >= self.alpha() {
            *self = Pain { strength, left: FLASH_TIME };
        }
    }
}

/// The delver is at 0 hit points but has not fallen yet: seconds left to heal ([`LAST_GASP`]).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Dying(pub f32);

/// The run has ended: seconds left before the title.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Fallen(pub f32);

/// On the delver while a constrictor holds them.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Constricted {
    pub by: Entity,
}

/// The delver's hit points, from who they are, once they stand in the level; `FB_PLAYER_HP=<n>`
/// starts them on `n` of those instead, to check a fall quickly.
fn give_player_health(mut commands: Commands, character: Res<Character>, rules: Res<crate::rules::GameRules>, new: Query<Entity, (With<Player>, Without<Health>)>) {
    let start = std::env::var("FB_PLAYER_HP").ok().and_then(|n| n.trim().parse::<i32>().ok());
    for player in &new {
        commands.entity(player).insert(starting_health(first_level_hit_points_with(&rules, &character), start));
    }
}

/// Full hit points of `max`, or `start` of them (at least 1, at most `max`).
pub fn starting_health(max: i32, start: Option<i32>) -> Health {
    Health { hp: start.map_or(max, |n| n.clamp(1, max)), max }
}

/// The delver's Armor Class, as the sheet works it out from who they are and what they wear,
/// on the delver for every blow at them to roll against, and the chance their shield turns a
/// hit aside ([`BlockChance`]).
#[allow(clippy::type_complexity)]
fn armour_the_delver(mut commands: Commands, character: Res<Character>, inventory: Option<Res<Inventory>>, players: Query<(Entity, Option<&ArmourClass>, Option<&BlockChance>, Option<&Shunned>), With<Player>>) {
    let none = Inventory::default();
    let inventory = inventory.as_deref().unwrap_or(&none);
    let ac = ArmourClass(character.armour_class(inventory).value().round() as i32);
    let gear = character.gear(inventory);
    let block = BlockChance(gear.block_chance as f32);
    let shun = Shunned(gear.shun.map(|c| c as f32));
    for (player, had, blocked, shunned) in &players {
        if shunned.copied().unwrap_or_default() != shun {
            commands.entity(player).insert(shun);
        }
        if had != Some(&ac) {
            commands.entity(player).insert(ac);
        }
        if blocked.copied().unwrap_or_default() != block {
            commands.entity(player).insert(block);
        }
    }
}

/// The chance, 0 to 1, that the shield the delver holds turns aside a blow that would hit them
/// while it is raised and the blow comes from ahead (`super::guard`): the shield's own, by its
/// size, and an affix "of Deflection"'s on top (`fb_items::stats::Stats::block_chance`). Our own
/// mechanic: a blow turned aside does nothing but clang on the shield
/// (`creatures::behaviour::BlowsAtDelver`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct BlockChance(pub f32);

impl BlockChance {
    /// Whether the shield turns aside a blow that hit, from one draw of `rng`; with no chance
    /// nothing is drawn.
    pub fn turns_aside(self, rng: &mut procgen_rng::Mulberry32) -> bool {
        self.0 > 0.0 && rng.next_f64() < self.0 as f64
    }
}

/// The chance each kind of creature never notices the delver, from what they wear (an affix "of
/// Vermin's Peace"'s), by [`fb_items::Foe`] index: what `creatures::behaviour::behave` weighs
/// each creature's own roll against.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Shunned(pub [f32; fb_items::Foe::COUNT]);

impl Shunned {
    /// Whether a creature of `foe` whose own roll is `roll` (0 to 1, fixed for that creature)
    /// does not notice the delver: its roll falls under the chance.
    pub fn passes_over(&self, foe: fb_items::Foe, roll: f32) -> bool {
        roll < self.0[foe.index()]
    }
}

/// Keeps the delver's maximum hit points to their Constitution as it stands (a ring "of Vigour"
/// raises it; SRD 5.1, "Using Each Ability", Constitution, "Hit Points": when the modifier
/// changes, so does the hit point maximum, as though it had been the modifier from 1st level),
/// moving their maximum and hit points by as much as the first-level hit points moved, never
/// below 1 for one still standing. `last` is the first-level hit points last seen; the first
/// look only notes them, so a delver started on other hit points keeps them.
pub(crate) fn fit_hit_points(character: Res<Character>, rules: Option<Res<crate::rules::GameRules>>, mut last: Local<Option<i32>>, mut players: Query<&mut Health, With<Player>>) {
    if !character.is_changed() && !rules.as_ref().is_some_and(|r| r.is_changed()) {
        return;
    }
    let now = first_level_hit_points_with(crate::rules::GameRules::of(&rules), &character);
    let Some(was) = last.replace(now) else { return };
    let by = now - was;
    if by == 0 {
        return;
    }
    for mut health in &mut players {
        if !health.is_down() {
            let max = (health.max + by).max(1);
            *health = Health { hp: (health.hp + by).clamp(1, max), max };
        }
    }
}

/// A blow that misses the delver whooshes past them, from the side it came; one their shield
/// turned aside clangs on it instead (`super::guard`).
fn whoosh_past(
    mut commands: Commands,
    mut misses: MessageReader<MissedDelver>,
    sound: Option<Res<super::swing::SwingSound>>,
    players: Query<&Transform, With<Player>>,
    settings: Option<Res<crate::settings::GameSettings>>,
    tuning: Option<Res<crate::tuning::Tuning>>,
) {
    let game = settings.as_deref().copied().unwrap_or_default();
    let tuning = tuning.as_deref().cloned().unwrap_or_default();
    for miss in misses.read().filter(|m| !m.blocked) {
        let (Some(sound), Some(delver)) = (&sound, players.iter().next()) else { continue };
        let at = Vec3::new(miss.attacker_pos.x, delver.translation.y, miss.attacker_pos.y);
        crate::audio::play_at(&mut commands, "blow misses", sound.0.clone(), at, &game, &tuning);
    }
}

/// How the log names a blow that landed: "A folk strikes", a skeleton's by its weapon
/// ("Skeleton slashes" with a sword, "Skeleton strikes" with a mace, "Skeleton claws" bare),
/// a troll's bite and claws ("Troll mauls") and a beast's bite ("Rat bites").
pub fn blow_line(species: Option<Species>, kit: Option<&crate::creatures::skeletons::SkeletonKit>) -> String {
    match (species, kit) {
        (None, _) => "A folk strikes".to_string(),
        (Some(s), Some(kit)) if s.is_skeleton() => format!("{} {}", s.stat_block().name, kit.blow_verb()),
        (Some(s), _) if s.is_skeleton() => format!("{} strikes", s.stat_block().name),
        // Its Multiattack is a bite and two claws in one go.
        (Some(s), _) if s.is_troll() => format!("{} mauls", s.stat_block().name),
        (Some(s), _) => format!("{} bites", s.stat_block().name),
    }
}

/// The damage a bite takes from a delver at `hp`: all of it, unless its venom `spares` them
/// ([`Species::venom_spares`]) and it is the venom, not the `bite`'s own damage, that would drop
/// them to 0. Then they are left at 1 hit point (the SRD's "stable", which the game has no
/// state for).
fn spared_by_venom(hp: i32, bite: u32, total: u32, spares: bool) -> u32 {
    if spares && hp > bite as i32 && hp <= total as i32 { (hp - 1) as u32 } else { total }
}

/// Each bite takes its damage from the delver; a constrictor's holds them.
#[allow(clippy::too_many_arguments)]
fn take_bites(
    mut commands: Commands,
    mut bites: MessageReader<AttackOnDelver>,
    character: Res<Character>,
    mut dice: ResMut<RunDice>,
    mut pain: ResMut<Pain>,
    bodies: Query<(), With<Corpse>>,
    critters: Query<&Critter>,
    mut players: Query<(Entity, &mut Health, Option<&DelverEffects>), With<Player>>,
    sounds: Option<Res<super::feedback::CreatureSounds>>,
    settings: Option<Res<crate::settings::GameSettings>>,
    tuning: Option<Res<crate::tuning::Tuning>>,
    rules: Res<crate::rules::GameRules>,
    inventory: Option<Res<crate::inventory::Inventory>>,
    mut hurt: MessageWriter<super::voice::DelverHurt>,
) {
    let game = settings.as_deref().copied().unwrap_or_default();
    let tuning = tuning.as_deref().cloned().unwrap_or_default();
    // What they wear and how dirty they are make catching a disease likelier.
    let gear = inventory.as_deref().map(|i| character.gear(i)).unwrap_or_default();
    let disease_risk = gear.disease_risk * (1.0 + character.dirt.max(0.0) as f64);
    // Gear that wards against poison adds to the save against venom.
    let poison_resistance = gear.poison_resistance.round() as i32;
    for bite in bites.read() {
        if bodies.contains(bite.attacker) {
            continue;
        }
        for (player, mut health, effects) in &mut players {
            if health.is_down() {
                continue;
            }
            let (damage, poisons_for) = bite_damage_with(&rules, bite, character.constitution(), poison_resistance, &mut dice.0);
            // Gear that wards against the attacker's kind takes a fraction off, never the last 1.
            let ward = bite.species.map_or(0.0, |s| gear.ward[s.foe().index()] as f32);
            let damage = fb_items::foe::warded_damage(damage, ward);
            let damage = spared_by_venom(health.hp, bite.damage, damage, bite.species.is_some_and(Species::venom_spares));
            health.hurt(damage);
            // A failed save poisons them for the poisoned effect's hours, and a rat's bite may
            // give them sewer plague, which shows nothing until it has incubated.
            let by = bite.species.map_or_else(|| "a folk".to_string(), |s| s.stat_block().name.to_string());
            let mut caught = vec![];
            if poisons_for.is_some() {
                caught.push(("poisoned", 1.0));
            }
            if let Some(plague) = bite.species.filter(|s| carries_plague(*s)).and_then(|_| rules.effects.get(SEWER_PLAGUE))
                && dice.0.next_f64() < plague.catch_chance_for(disease_risk) {
                    info!("the bite carries sewer plague");
                    caught.push((SEWER_PLAGUE, PLAGUE_ONSET));
                }
            if !caught.is_empty() {
                let mut effects = effects.map_or_else(Default::default, |e| e.0.clone());
                for (id, intensity) in caught {
                    effects.add(&rules.effects, id, intensity, None, fb_character::effect::Source::Bite { by: by.clone() });
                }
                commands.entity(player).insert(DelverEffects(effects));
            }
            pain.strike(flash_strength(damage as i32, health.max));
            // The delver grunts under it ([`super::voice`]).
            hurt.write(super::voice::DelverHurt { damage: damage as i32, max: health.max, down: health.is_down() });
            // One dull thud for each blow that lands.
            if let Some(sounds) = &sounds {
                commands.spawn((Name::new("blow lands"), AudioPlayer::new(sounds.thud.clone()), crate::audio::sound_effect(&game, &tuning), DespawnOnExit(AppState::Delving)));
            }
            let who = blow_line(bite.species, critters.get(bite.attacker).ok().and_then(|c| c.kit.as_deref()));
            info!("{who} for {damage}: {} of {} hit points left", health.hp, health.max);
            if bite.species == Some(Species::ConstrictorSnake) && !health.is_down() {
                commands.entity(player).insert((Constricted { by: bite.attacker }, Slowed(HELD_PACE)));
            }
        }
    }
}

/// Each kill heals the delver by what the weapon in their main hand gives on a kill (an affix "of
/// the Leech"'s `HealOnKill`, [`crate::character::Derived::heal_on_kill`]), never above their
/// maximum. A fallen delver heals nothing.
fn heal_on_kill(mut slain: MessageReader<Slain>, character: Res<Character>, inventory: Option<Res<Inventory>>, mut players: Query<&mut Health, With<Player>>) {
    let kills = slain.read().filter(|s| !s.by_ally).count();
    if kills == 0 {
        return;
    }
    let none = Inventory::default();
    let heal = character.gear(inventory.as_deref().unwrap_or(&none)).heal_on_kill.round().max(0.0) as u32;
    if heal == 0 {
        return;
    }
    for mut health in &mut players {
        if health.is_down() {
            continue;
        }
        let healed = health.heal(heal * kills as u32);
        if healed > 0 {
            info!("The kill heals the delver for {healed}: {} of {} hit points", health.hp, health.max);
        }
    }
}

/// A held delver breaks free by striking the constrictor, or when it dies or they get away.
fn break_free(
    mut commands: Commands,
    mut struck: MessageReader<Struck>,
    tuning: Option<Res<crate::tuning::Tuning>>,
    players: Query<(Entity, &Constricted, &Transform), With<Player>>,
    snakes: Query<(&Critter, &Transform), Without<Corpse>>,
) {
    let blows: Vec<Entity> = struck.read().map(|s| s.target).collect();
    let radius = tuning.as_deref().cloned().unwrap_or_default().moving.body_radius;
    for (player, held, at) in &players {
        let still = snakes.get(held.by).is_ok_and(|(critter, snake)| snake.translation.xz().distance(at.translation.xz()) <= reach(critter, radius) + radius);
        if blows.contains(&held.by) || !still {
            commands.entity(player).remove::<(Constricted, Slowed)>();
        }
    }
}

/// The overlay that says the run is over.
#[derive(Component, Debug)]
#[require(DespawnOnExit<crate::launch::AppState> = DespawnOnExit(crate::launch::AppState::Delving))]
pub struct FallOverlay;

/// What [`fall`] reads to decide between falling unconscious and falling in the abyss.
type Revival<'w> = (Option<Res<'w, super::revival::Revives>>, Option<Res<'w, super::revival::Unconscious>>, Option<Res<'w, GameRules>>);

/// At exhaustion level 6, or [`LAST_GASP`] seconds at 0 hit points, the delver stops and the
/// view fades out to "You fall in the abyss"; then back to the title. A delver healed above 0
/// within the gasp stands on.
#[allow(clippy::too_many_arguments)]
fn fall(
    mut commands: Commands,
    time: Res<Time>,
    fallen: Option<ResMut<Fallen>>,
    dying: Option<ResMut<Dying>>,
    fonts: Option<Res<UiFonts>>,
    players: Query<(Entity, &Health, &Exhaustion), With<Player>>,
    view_models: Query<Entity, With<ViewModelCamera>>,
    mut overlay: Query<(&mut BackgroundColor, &Children), With<FallOverlay>>,
    mut words: Query<&mut TextColor>,
    mut screen: Option<ResMut<ScreenOpen>>,
    next: Option<ResMut<NextState<AppState>>>,
    revival: Revival,
) {
    let (revives, unconscious, rules) = revival;
    // Lying unconscious, the delver waits on the choice ([`super::revival`]).
    if unconscious.is_some() {
        return;
    }
    let Some(mut fallen) = fallen else {
        let down = players.iter().find(|(_, health, _)| health.is_down()).map(|(player, ..)| player);
        let spent = players.iter().find(|(.., exhaustion)| exhaustion.0.is_dead()).map(|(player, ..)| player);
        // At 0 hit points the gasp runs down; a draught that heals them in time ends it.
        let gasped = match (down, dying) {
            (None, None) => None,
            (None, Some(_)) => {
                commands.remove_resource::<Dying>();
                None
            }
            (Some(_), None) => {
                commands.insert_resource(Dying(LAST_GASP));
                None
            }
            (Some(player), Some(mut dying)) => {
                dying.0 -= time.delta_secs();
                (dying.0 <= 0.0).then_some(player)
            }
        };
        let Some(player) = spent.or(gasped) else { return };
        commands.remove_resource::<Dying>();
        commands.entity(player).insert(Slowed(0.0)).remove::<(Constricted, super::swing::Swing, super::guard::Guard)>();
        if let Some(screen) = screen.as_deref_mut() {
            screen.0 = None;
        }
        // Brought down by a blow, a delver with a revive left in this world falls unconscious
        // instead; spent by exhaustion, they do not.
        let revival = &GameRules::of(&rules).revival;
        if let Some(revives) = revives.filter(|_| spent.is_none())
            && revival.left(revives.0) > 0
        {
            info!("the delver falls unconscious");
            super::revival::fall_unconscious(&mut commands, fonts.as_deref(), view_models.iter().next(), revival.left(revives.0));
            return;
        }
        begin_the_fall(&mut commands, fonts.as_deref(), view_models.iter().next());
        return;
    };
    fallen.0 -= time.delta_secs();
    // Fades in over the first second and a half.
    let shown = ((FALL_TIME - fallen.0) / 1.5).clamp(0.0, 1.0);
    for (mut background, children) in &mut overlay {
        background.0 = BG.with_alpha(0.9 * shown);
        for &child in children {
            if let Ok(mut colour) = words.get_mut(child) {
                colour.0 = BRIGHT.with_alpha(shown);
            }
        }
    }
    if fallen.0 <= 0.0
        && let Some(mut next) = next
    {
        next.set(AppState::Title);
    }
}

/// Ends the run: "You fall in the abyss" fades in over the view, and after [`FALL_TIME`]
/// seconds the title returns ([`fall`]).
pub fn begin_the_fall(commands: &mut Commands, fonts: Option<&UiFonts>, camera: Option<Entity>) {
    info!("the delver falls in the abyss");
    commands.insert_resource(Fallen(FALL_TIME));
    let mut root = commands.spawn((
        Name::new("You fall"),
        FallOverlay,
        Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
        BackgroundColor(BG.with_alpha(0.0)),
        GlobalZIndex(10),
    ));
    if let Some(camera) = camera {
        root.insert(UiTargetCamera(camera));
    }
    let font = fonts.map(|f| f.display.clone()).unwrap_or_default();
    root.with_child((Text::new("You fall in the abyss"), TextFont::from_font_size(52.0).with_font(font), TextColor(BRIGHT.with_alpha(0.0))));
}

/// The HUD root the vitals draw into, which the orbs ([`super::orbs`]), the eye and the
/// quickslot bar hang from.
#[derive(Component, Debug)]
pub(crate) struct VitalsRoot;

/// The disease a rat's bite can carry ([`carries_plague`]).
pub const SEWER_PLAGUE: &str = "sewer_plague";
/// How strong sewer plague is when caught, before it worsens by itself.
const PLAGUE_ONSET: f64 = 0.1;

/// Whether `species`' bite can carry sewer plague: rats, from the filth they live in (SRD 5.1
/// p. 199, "Sewer Plague", a disease of rats and filth).
pub fn carries_plague(species: Species) -> bool {
    matches!(species, Species::Rat | Species::GiantRat)
}

/// The dark red edge round the view while badly hurt.
#[derive(Component, Debug)]
struct PainEdge;
/// The red flash over the whole view when a blow lands.
#[derive(Component, Debug)]
struct PainFlash;

/// Draws the pain flash and the dark edge over each new view model, as part of the HUD that the
/// orbs, the eye and the quickslot bar join.
fn spawn_vitals(mut commands: Commands, new: Query<Entity, Added<ViewModelCamera>>) {
    for camera in &new {
        commands
            .spawn((Name::new("Vitals"), Hud, VitalsRoot, UiTargetCamera(camera), Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() }, Pickable::IGNORE))
            .with_children(|hud| {
                hud.spawn((PainFlash, Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() }, BackgroundColor(BAD.with_alpha(0.0)), Pickable::IGNORE));
                hud.spawn((PainEdge, Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, border: UiRect::all(px(28)), ..default() }, BorderColor::all(BAD.with_alpha(0.0))));
            });
    }
}

/// How far in the blood reaches `into` seconds into the last gasp: creeping in as it runs out.
pub fn gasp_spread(into: f32) -> f32 {
    let (start, end) = GASP_SPREAD;
    start + (end - start) * (into / LAST_GASP).clamp(0.0, 1.0)
}

/// Fades the red flash over the view, draws the dark edge while badly hurt, and through the last
/// gasp blooms blinking blood in from the edge ([`super::blood::GaspBlood`]).
fn show_vitals(
    time: Res<Time>,
    mut pain: ResMut<Pain>,
    dying: Option<Res<Dying>>,
    mut blood: Option<ResMut<super::blood::GaspBlood>>,
    players: Query<&Health, With<Player>>,
    mut edges: Query<&mut BorderColor, With<PainEdge>>,
    mut flashes: Query<&mut BackgroundColor, With<PainFlash>>,
) {
    pain.left = (pain.left - time.delta_secs()).max(0.0);
    let flash = pain.alpha();
    for mut colour in &mut flashes {
        colour.set_if_neq(BackgroundColor(BAD.with_alpha(flash)));
    }
    // Through the last gasp blood blooms in from the edge and blinks: drink now.
    let gasp = dying.map_or_else(super::blood::GaspBlood::default, |dying| {
        let into = LAST_GASP - dying.0;
        super::blood::GaspBlood { strength: gasp_blink(into), spread: gasp_spread(into) }
    });
    if let Some(blood) = blood.as_mut() {
        blood.set_if_neq(gasp);
    }
    let Some(health) = players.iter().next() else { return };
    let dark = 0.7 * vignette_strength(health.hp, health.max, time.elapsed_secs());
    for mut edge in &mut edges {
        edge.set_if_neq(BorderColor::all(VIGNETTE_COLOUR.with_alpha(dark)));
    }
}

/// The clockwise-from-forward angle to an attacker, accounting for the delver's facing.
/// `player_pos` and `attacker_pos` are xz coordinates on the floor; `yaw` is the delver's rotation
/// around the y axis. Returns the angle in radians, normalized to [0, 2π): 0 is in front,
/// π/2 is to the right, π is behind, 3π/2 is to the left. The view jolt uses it to push away
/// from the attacker. Our own.
pub fn bearing_from_facing(player_pos: Vec2, yaw: f32, attacker_pos: Vec2) -> f32 {
    let (dx, dz) = (attacker_pos.x - player_pos.x, attacker_pos.y - player_pos.y);
    (dx.atan2(-dz) + yaw).rem_euclid(std::f32::consts::TAU)
}

/// Leaving the delve clears what it kept besides its entities: the level, what the delver
/// carried and the state of their view, so the next delve starts afresh. Everything the delve
/// spawned carries `DespawnOnExit(AppState::Delving)` (on its root, or required by its marker
/// component) and Bevy despawns it on the way out, as its state-scoped entities do
/// (<https://docs.rs/bevy/0.20.0/bevy/state/state_scoped/struct.DespawnOnExit.html>).
pub fn end_the_delve(mut commands: Commands, mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>) {
    commands.remove_resource::<CurrentLevel>();
    commands.remove_resource::<LevelRooms>();
    commands.insert_resource(Inventory::default());
    commands.insert_resource(ScreenOpen::default());
    commands.insert_resource(Obstacles::default());
    commands.insert_resource(BlockedCells::default());
    commands.insert_resource(Aimed::default());
    commands.insert_resource(Pain::default());
    commands.remove_resource::<Fallen>();
    commands.remove_resource::<Dying>();
    commands.remove_resource::<super::revival::Unconscious>();
    if let Some(cursor) = cursor.as_deref_mut() {
        crate::player::set_grab(cursor, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;

    /// A delve with a delver of 10 hit points and one constrictor, stepped 1/60 s a frame.
    fn delve() -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .insert_state(AppState::Delving)
            .insert_resource(RunDice(procgen_rng::Mulberry32::new(4)))
            .init_resource::<Character>()
            .init_resource::<ScreenOpen>()
            .add_plugins(VitalsPlugin);
        let player = app.world_mut().spawn((Player, Transform::default(), Health::full(10))).id();
        let level = fb_level::generate(crate::creatures::seed_holding(&[Species::ConstrictorSnake]), 1);
        let spot = *level.creatures.iter().find(|c| c.species == Species::ConstrictorSnake).unwrap();
        let creature = spot.creature();
        let snake = app.world_mut().spawn((Critter::new(spot, creature), Transform::from_xyz(10.0, 0.0, 0.0))).id();
        (app, player, snake)
    }

    fn bite(app: &mut App, attacker: Entity, species: Species, damage: u32) {
        let attacker_pos = app.world().get::<Transform>(attacker).map(|t| t.translation.xz()).unwrap_or_default();
        app.world_mut().write_message(AttackOnDelver { attacker, species: Some(species), damage, poison: 0, attacker_pos });
        app.update();
    }

    #[test]
    fn a_delver_can_start_on_fewer_hit_points() {
        assert_eq!(starting_health(12, None), Health::full(12));
        assert_eq!(starting_health(12, Some(1)), Health { hp: 1, max: 12 });
        assert_eq!(starting_health(12, Some(40)), Health::full(12));
        assert_eq!(starting_health(12, Some(0)), Health { hp: 1, max: 12 });
    }

    #[test]
    fn bites_take_hit_points_and_start_the_flash() {
        let (mut app, player, snake) = delve();
        app.update();
        bite(&mut app, snake, Species::Rat, 3);
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 7);
        assert!(app.world().resource::<Pain>().alpha() > 0.0);
    }

    #[test]
    fn a_skeletons_blow_is_logged_by_its_weapon_and_a_rats_as_a_bite() {
        use crate::creatures::skeletons::SkeletonKit;
        let state = fb_creatures::SkeletonState::roll(3);
        let armed = |base: &str| {
            let item = fb_items::roll_item(base, 41).unwrap();
            SkeletonKit::new(&state, fb_items::outfit::OutfitSet { worn: vec![(fb_items::Slot::Mainhand, item)], goods: Vec::new(), ground: Vec::new() }, [None, None])
        };
        assert_eq!(blow_line(Some(Species::Skeleton), Some(&armed("Sword"))), "Skeleton slashes");
        assert_eq!(blow_line(Some(Species::Skeleton), Some(&armed("Mace"))), "Skeleton strikes");
        assert_eq!(blow_line(Some(Species::Skeleton), Some(&SkeletonKit::bare(&state))), "Skeleton claws");
        assert_eq!(blow_line(Some(Species::Rat), None), "Rat bites");
        // A troll's swing is a bite and two claws at once, which reads as a mauling.
        assert_eq!(blow_line(Some(Species::Troll), None), "Troll mauls");
        assert_eq!(blow_line(None, None), "A folk strikes");
        for line in [armed("Sword"), armed("Mace"), armed("Dagger"), SkeletonKit::bare(&state)].iter().map(|k| blow_line(Some(Species::Skeleton), Some(k))) {
            assert!(!line.contains("bites"), "{line}");
        }
    }

    #[test]
    fn a_shield_of_the_rampart_turns_aside_a_fifth_of_the_hits() {
        let mut rng = procgen_rng::Mulberry32::new(17);
        let turned = (0..10_000).filter(|_| BlockChance(0.2).turns_aside(&mut rng)).count();
        assert!((turned as f64 / 10_000.0 - 0.2).abs() < 0.015, "{turned}");
        let mut none = procgen_rng::Mulberry32::new(17);
        assert!(!BlockChance(0.0).turns_aside(&mut none));
        assert_eq!(none.next_f64(), procgen_rng::Mulberry32::new(17).next_f64(), "no shield, no draw");
    }

    #[test]
    fn a_ring_of_vigour_raises_the_hit_point_maximum_and_taking_it_off_lowers_it() {
        let (mut app, player, _) = delve();
        app.update();
        let max = app.world().get::<Health>(player).unwrap().max;
        app.world_mut().get_mut::<Health>(player).unwrap().hp = max - 3;
        // CON +2 is +1 modifier.
        let con = app.world().resource::<Character>().constitution();
        let bonus = if con % 2 == 0 { 2 } else { 1 };
        app.world_mut().resource_mut::<Character>().worn.constitution = bonus;
        app.update();
        let raised = *app.world().get::<Health>(player).unwrap();
        assert_eq!((raised.hp, raised.max), (max - 2, max + 1));
        app.world_mut().resource_mut::<Character>().worn.constitution = 0;
        app.update();
        assert_eq!(*app.world().get::<Health>(player).unwrap(), Health { hp: max - 3, max });
    }

    #[test]
    fn an_ossuary_amulet_takes_a_quarter_off_a_skeletons_blow_but_never_the_last_point() {
        let (mut app, player, snake) = delve();
        app.update();
        bite(&mut app, snake, Species::Skeleton, 6);
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 4, "unwarded");
        let mut inventory = Inventory::default();
        let mut amulet = fb_items::roll_item("Amulet", 5).unwrap();
        let ossuary = fb_items::tables::affixes().iter().find(|a| a.name == "of the Ossuary").unwrap();
        ossuary.apply_to_stats(&mut amulet.profile.stats);
        let id = inventory.add(amulet);
        inventory.wear(id, fb_items::Slot::Neck).unwrap();
        app.insert_resource(inventory);
        app.world_mut().get_mut::<Health>(player).unwrap().hp = 10;
        bite(&mut app, snake, Species::Skeleton, 6);
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 5, "6 becomes 5");
        bite(&mut app, snake, Species::Skeleton, 1);
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 4, "1 stays 1");
        bite(&mut app, snake, Species::Rat, 1);
        bite(&mut app, snake, Species::Rat, 1);
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 2, "a rat's blow is not warded");
    }

    #[test]
    fn a_kill_with_a_leech_weapon_heals_the_delver_and_without_one_does_not() {
        let (mut app, player, snake) = delve();
        app.update();
        bite(&mut app, snake, Species::Rat, 4);
        app.world_mut().write_message(Slain { target: snake, by_ally: false });
        app.update();
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 6, "a plain fist heals nothing");
        let mut inventory = Inventory::default();
        let mut sword = fb_items::roll_item("Sword", 5).unwrap();
        let leech = fb_items::tables::affixes().iter().find(|a| a.name == "of the Leech").unwrap();
        leech.apply_to_stats(&mut sword.profile.stats);
        let id = inventory.add(sword);
        inventory.wear(id, fb_items::Slot::Mainhand).unwrap();
        app.insert_resource(inventory);
        for _ in 0..2 {
            app.world_mut().write_message(Slain { target: snake, by_ally: false });
            app.update();
        }
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 8, "one each kill");
        for _ in 0..5 {
            app.world_mut().write_message(Slain { target: snake, by_ally: false });
            app.update();
        }
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 10, "never above the maximum");
    }

    #[test]
    fn a_rats_bite_can_carry_sewer_plague_which_hides_until_it_incubates() {
        let defs = fb_character::effect::shipped();
        let plague = &defs[SEWER_PLAGUE];
        // A seed whose first draw after the bite's (none: a flat bite rolls nothing) catches it.
        let seed = (0..10_000).find(|s| procgen_rng::Mulberry32::new(*s).next_f64() < plague.catch_chance).unwrap();
        let (mut app, player, snake) = delve();
        app.world_mut().get_mut::<Health>(player).unwrap().hp = 10;
        app.world_mut().resource_mut::<RunDice>().0 = procgen_rng::Mulberry32::new(seed);
        app.update();
        bite(&mut app, snake, Species::Rat, 1);
        let effects = app.world().get::<DelverEffects>(player).expect("the bite carried it");
        let caught = effects.0.get(SEWER_PLAGUE, None).expect("sewer plague");
        assert!(caught.stage(plague).is_none(), "nothing shows while it incubates");
        // A snake's bite never carries it.
        let (mut app, player, snake) = delve();
        app.world_mut().resource_mut::<RunDice>().0 = procgen_rng::Mulberry32::new(seed);
        app.update();
        bite(&mut app, snake, Species::ConstrictorSnake, 1);
        assert!(app.world().get::<DelverEffects>(player).is_none_or(|e| e.0.get(SEWER_PLAGUE, None).is_none()));
    }

    #[test]
    fn a_centipedes_venom_leaves_the_delver_at_one_hit_point_but_its_bite_can_kill() {
        // 10 hit points, a bite of 4 and 14 with its venom: spared, they keep 1.
        assert_eq!(spared_by_venom(10, 4, 14, true), 9);
        // A snake's venom spares no one.
        assert_eq!(spared_by_venom(10, 4, 14, false), 14);
        // The bite alone fells them.
        assert_eq!(spared_by_venom(4, 5, 15, true), 15);
        // Not enough to fell them: all of it lands.
        assert_eq!(spared_by_venom(20, 4, 14, true), 14);
        assert!(Species::GiantCentipede.venom_spares() && Species::GiantSpider.venom_spares() && !Species::PoisonousSnake.venom_spares());
    }

    #[test]
    fn a_failed_save_against_venom_poisons_the_delver_and_a_good_one_does_not() {
        // Seeds whose first d20, the save, is a 1 (fails whatever the CON) and a 20 (saves).
        let seed_for = |natural| (0..10_000).find(|s| procgen_rng::Mulberry32::new(*s).int(1, 20) == natural).unwrap();
        for (natural, poisoned) in [(1, true), (20, false)] {
            let (mut app, player, snake) = delve();
            app.world_mut().resource_mut::<RunDice>().0 = procgen_rng::Mulberry32::new(seed_for(natural));
            app.update();
            app.world_mut().write_message(AttackOnDelver { attacker: snake, species: Some(Species::PoisonousSnake), damage: 1, poison: 4, attacker_pos: Vec2::ZERO });
            app.update();
            let poison = app.world().get::<DelverEffects>(player).and_then(|e| e.poisoned_hours());
            assert_eq!(poison, poisoned.then_some(fb_character::poison::VENOM_POISONED_HOURS), "a natural {natural}");
        }
    }

    #[test]
    fn a_constrictor_holds_you_until_you_strike_it() {
        let (mut app, player, snake) = delve();
        app.update();
        bite(&mut app, snake, Species::ConstrictorSnake, 2);
        assert_eq!(app.world().get::<Constricted>(player), Some(&Constricted { by: snake }));
        assert_eq!(app.world().get::<Slowed>(player), Some(&Slowed(HELD_PACE)));
        app.update();
        assert!(app.world().get::<Constricted>(player).is_some(), "still held");
        app.world_mut().write_message(Struck { target: snake, damage: 1, from: Vec2::ZERO, bludgeoning: false, critical: false, by_ally: false, graze: false });
        app.update();
        assert!(app.world().get::<Constricted>(player).is_none());
        assert!(app.world().get::<Slowed>(player).is_none());
    }

    #[test]
    fn at_zero_hit_points_the_run_ends_and_the_title_returns() {
        let (mut app, player, snake) = delve();
        app.update();
        bite(&mut app, snake, Species::Rat, 25);
        assert!(app.world().get::<Health>(player).unwrap().is_down());
        app.update();
        assert!(app.world().get_resource::<Fallen>().is_none(), "a last gasp first");
        assert!(app.world().get_resource::<Dying>().is_some());
        for _ in 0..((LAST_GASP * 60.0) as usize + 2) {
            app.update();
        }
        assert!(app.world().get_resource::<Fallen>().is_some());
        assert!(app.world().get_resource::<Dying>().is_none());
        assert_eq!(app.world().get::<Slowed>(player), Some(&Slowed(0.0)), "the delver stops");
        for _ in 0..((FALL_TIME * 60.0) as usize + 5) {
            app.update();
        }
        assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Title);
        // Leaving the delve cleared it.
        assert!(app.world().get_entity(player).is_err(), "the delver is gone");
        assert!(app.world().get_entity(snake).is_err(), "the creatures are gone");
        assert!(app.world().get_resource::<Fallen>().is_none());
    }

    #[test]
    fn the_last_gasp_lasts_three_seconds() {
        let (mut app, _, snake) = delve();
        app.update();
        bite(&mut app, snake, Species::Rat, 25);
        for _ in 0..((LAST_GASP * 60.0) as usize - 2) {
            app.update();
        }
        assert!(app.world().get_resource::<Fallen>().is_none(), "still gasping");
        assert_eq!(app.world_mut().query::<&FallOverlay>().iter(app.world()).count(), 0, "no overlay yet");
        for _ in 0..4 {
            app.update();
        }
        assert!(app.world().get_resource::<Fallen>().is_some());
        assert_eq!(app.world_mut().query::<&FallOverlay>().iter(app.world()).count(), 1);
    }

    #[test]
    fn a_draught_drunk_in_the_last_gasp_keeps_the_delver_standing() {
        let (mut app, player, snake) = delve();
        app.update();
        bite(&mut app, snake, Species::Rat, 25);
        for _ in 0..10 {
            app.update();
        }
        let mut dice = procgen_rng::Mulberry32::new(1);
        assert!(crate::eat::drink_healing(&mut app.world_mut().get_mut::<Health>(player).unwrap(), &mut dice, armory::families::food::PotionSize::Medium) > 0);
        for _ in 0..60 {
            app.update();
        }
        assert!(app.world().get_resource::<Fallen>().is_none(), "healed in time");
        assert!(app.world().get_resource::<Dying>().is_none(), "the gasp is over");
        assert_eq!(app.world().get::<Slowed>(player), None, "they still move");
        // Down again, the gasp starts afresh.
        bite(&mut app, snake, Species::Rat, 25);
        app.update();
        let left = app.world().get_resource::<Dying>().unwrap().0;
        assert!(left > LAST_GASP - 0.05, "{left}");
    }

    #[test]
    fn at_exhaustion_level_6_the_run_ends_as_at_zero_hit_points() {
        let (mut app, player, snake) = delve();
        app.update();
        bite(&mut app, snake, Species::ConstrictorSnake, 2);
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(crate::play::Screen::Inventory);
        app.update();
        assert!(app.world().get_resource::<Fallen>().is_none(), "level 0 does not fall");
        app.world_mut().get_mut::<Exhaustion>(player).unwrap().0 = fb_character::Exhaustion::new(6);
        app.update();
        assert!(app.world().get_resource::<Fallen>().is_some());
        assert_eq!(app.world().get::<Slowed>(player), Some(&Slowed(0.0)), "the delver stops");
        assert!(app.world().get::<Constricted>(player).is_none(), "the grip lets go");
        assert_eq!(app.world().resource::<ScreenOpen>().0, None, "the screens close");
        assert!(app.world().get::<Health>(player).unwrap().hp > 0, "hit points were not what ended it");
        assert_eq!(app.world_mut().query::<&FallOverlay>().iter(app.world()).count(), 1, "the overlay shows");
        for _ in 0..((FALL_TIME * 60.0) as usize + 5) {
            app.update();
        }
        assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Title);
        assert!(app.world().get_entity(player).is_err(), "the delver is gone");
        assert!(app.world().get_resource::<Fallen>().is_none());
    }

    #[test]
    fn a_new_delve_after_a_fall_starts_clean() {
        use crate::inventory::{Hovered, OpenContainer, leave_the_delve};
        let (mut app, player, snake) = delve();
        app.init_resource::<crate::inventory::DollStage>()
            .init_resource::<crate::inventory::icon::IconCache>()
            .insert_resource(OpenContainer(Some(crate::inventory::ItemId(3))))
            .add_systems(OnExit(AppState::Delving), leave_the_delve);
        // A hover card made for this delve's view.
        let card = app.world_mut().spawn((Node::default(), DespawnOnExit(AppState::Delving))).id();
        app.update();
        bite(&mut app, snake, Species::Rat, 25);
        for _ in 0..(((LAST_GASP + FALL_TIME) * 60.0) as usize + 5) {
            app.update();
        }
        assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Title);
        for gone in [player, snake, card] {
            assert!(app.world().get_entity(gone).is_err(), "{gone} outlived the delve");
        }
        assert_eq!(*app.world().resource::<OpenContainer>(), OpenContainer(None));
        assert_eq!(*app.world().resource::<Hovered>(), Hovered(None));
        assert_eq!(*app.world().resource::<Pain>(), Pain::default());
        // New delve: a fresh delver stands up whole, and nothing of the last run is left over.
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Delving);
        app.update();
        let fresh = app.world_mut().spawn((Player, Transform::default())).id();
        app.update();
        let health = *app.world().get::<Health>(fresh).unwrap();
        assert!(health.hp == health.max && health.hp > 0);
        assert!(app.world().get_resource::<Fallen>().is_none());
        app.update();
        assert!(app.world().get_resource::<Fallen>().is_none(), "the new delver is not fallen");
    }

    #[test]
    fn bearing_from_facing_with_yaw_0_places_attacker_on_the_right_at_90_degrees() {
        let player_pos = Vec2::ZERO;
        let yaw = 0.0;
        let attacker_pos = Vec2::new(5.0, 0.0);
        let angle = bearing_from_facing(player_pos, yaw, attacker_pos);
        // Should be π/2 (90 degrees).
        assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5, "angle {}", angle);
    }

    #[test]
    fn bearing_from_facing_with_yaw_0_places_attacker_on_the_left_at_270_degrees() {
        let player_pos = Vec2::ZERO;
        let yaw = 0.0;
        let attacker_pos = Vec2::new(-5.0, 0.0);
        let angle = bearing_from_facing(player_pos, yaw, attacker_pos);
        // Should be 3π/2 (270 degrees).
        let expected = 3.0 * std::f32::consts::FRAC_PI_2;
        assert!((angle - expected).abs() < 1e-5, "angle {}", angle);
    }

    #[test]
    fn bearing_from_facing_with_yaw_0_places_attacker_behind_at_180_degrees() {
        let player_pos = Vec2::ZERO;
        let yaw = 0.0;
        let attacker_pos = Vec2::new(0.0, 5.0);
        let angle = bearing_from_facing(player_pos, yaw, attacker_pos);
        // Should be π (180 degrees).
        assert!((angle - std::f32::consts::PI).abs() < 1e-5, "angle {}", angle);
    }

    #[test]
    fn bearing_from_facing_with_yaw_90_rotates_the_perspective() {
        let player_pos = Vec2::ZERO;
        let yaw = std::f32::consts::FRAC_PI_2; // 90 degrees
        // Attacker to the player's right in world space (5, 0) is behind in rotated perspective.
        let attacker_pos = Vec2::new(5.0, 0.0);
        let angle = bearing_from_facing(player_pos, yaw, attacker_pos);
        // Should be π (180 degrees, behind).
        assert!((angle - std::f32::consts::PI).abs() < 1e-5, "angle {}", angle);
    }

    #[test]
    fn bearing_from_facing_with_yaw_negative_90_places_attacker_behind() {
        let player_pos = Vec2::ZERO;
        let yaw = -std::f32::consts::FRAC_PI_2; // -90 degrees
        // Attacker to the left in world space (-5, 0) is behind when facing right (-90°).
        let attacker_pos = Vec2::new(-5.0, 0.0);
        let angle = bearing_from_facing(player_pos, yaw, attacker_pos);
        // Should be π (180 degrees, behind).
        assert!((angle - std::f32::consts::PI).abs() < 1e-5, "angle {}", angle);
    }

    #[test]
    fn bearing_from_facing_with_yaw_180_reverses_direction() {
        let player_pos = Vec2::ZERO;
        let yaw = std::f32::consts::PI; // 180 degrees (facing backward)
        // Attacker ahead in world space (0, -5) is behind from reversed perspective.
        let attacker_pos = Vec2::new(0.0, -5.0);
        let angle = bearing_from_facing(player_pos, yaw, attacker_pos);
        // Should be π (180 degrees, behind).
        assert!((angle - std::f32::consts::PI).abs() < 1e-5, "angle {}", angle);
    }

    #[test]
    fn bearing_from_facing_with_yaw_90_and_attacker_ahead_in_world() {
        let player_pos = Vec2::ZERO;
        let yaw = std::f32::consts::FRAC_PI_2; // 90 degrees (facing right)
        // Attacker ahead in world space (0, -5) is to the right from rightward perspective.
        let attacker_pos = Vec2::new(0.0, -5.0);
        let angle = bearing_from_facing(player_pos, yaw, attacker_pos);
        // Should be π/2 (90 degrees, right).
        assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5, "angle {}", angle);
    }

    /// A delve with a view model camera, so the vitals HUD exists.
    fn delve_with_hud() -> (App, Entity, Entity) {
        let (mut app, player, snake) = delve();
        let follows = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(ViewModelCamera { follows });
        app.update();
        app.update();
        (app, player, snake)
    }

    /// The flash over the view: its red and opacity.
    fn flash_colour(app: &mut App) -> Srgba {
        let mut query = app.world_mut().query_filtered::<&BackgroundColor, With<PainFlash>>();
        query.single(app.world()).unwrap().0.to_srgba()
    }

    fn bite_all(app: &mut App, attacker: Entity, damages: &[u32]) {
        let attacker_pos = app.world().get::<Transform>(attacker).map(|t| t.translation.xz()).unwrap_or_default();
        for &damage in damages {
            app.world_mut().write_message(AttackOnDelver { attacker, species: Some(Species::Rat), damage, poison: 0, attacker_pos });
        }
        app.update();
    }

    #[test]
    fn the_flash_is_faint_for_a_scratch_strong_for_a_hard_blow_and_capped() {
        assert_eq!(flash_strength(0, 12), FLASH_FLOOR);
        let rat = flash_strength(1, 12);
        let constrictor = flash_strength(5, 12);
        assert!(rat > FLASH_FLOOR && rat < 0.2, "a rat's bite is faint: {rat}");
        assert!(constrictor > 0.4 && constrictor <= FLASH_CAP, "a constrictor's is strong: {constrictor}");
        assert!(constrictor > rat);
        assert_eq!(flash_strength(50, 12), FLASH_CAP);
        assert_eq!(flash_strength(3, 0), FLASH_CAP, "no divide by zero");
    }

    #[test]
    fn the_flash_fades_linearly_over_its_time() {
        let pain = Pain { strength: 0.4, left: FLASH_TIME };
        assert_eq!(pain.alpha(), 0.4);
        assert!((Pain { left: FLASH_TIME / 2.0, ..pain }.alpha() - 0.2).abs() < 1e-6);
        assert_eq!(Pain { left: 0.0, ..pain }.alpha(), 0.0);
        assert_eq!(Pain::default().alpha(), 0.0);
    }

    #[test]
    fn a_bite_flashes_the_whole_view_and_thuds() {
        let (mut app, _player, snake) = delve_with_hud();
        app.insert_resource(super::super::feedback::CreatureSounds::default()).init_resource::<crate::tuning::Tuning>();
        assert_eq!(flash_colour(&mut app).alpha, 0.0);
        bite(&mut app, snake, Species::Rat, 1);
        let flash = flash_colour(&mut app);
        assert!(flash.alpha > 0.0 && flash.red > flash.blue, "a red flash: {flash:?}");
        assert_eq!(playing(&mut app).len(), 1, "one thud");
        // It covers the view, and clicks pass through it.
        let mut query = app.world_mut().query_filtered::<(&Node, &Pickable), With<PainFlash>>();
        let (node, pickable) = query.single(app.world()).unwrap();
        assert_eq!((node.width, node.height, node.position_type), (percent(100), percent(100), PositionType::Absolute));
        assert_eq!(*pickable, Pickable::IGNORE);
    }

    #[test]
    fn a_harder_bite_flashes_stronger() {
        let (mut soft, _p, snake) = delve_with_hud();
        bite(&mut soft, snake, Species::Rat, 1);
        let (mut hard, _p, snake) = delve_with_hud();
        bite(&mut hard, snake, Species::Rat, 5);
        assert!(flash_colour(&mut hard).alpha > flash_colour(&mut soft).alpha);
    }

    #[test]
    fn two_bites_in_a_frame_flash_once_at_the_stronger_and_thud_twice() {
        let (mut app, _player, snake) = delve_with_hud();
        app.insert_resource(super::super::feedback::CreatureSounds::default()).init_resource::<crate::tuning::Tuning>();
        bite_all(&mut app, snake, &[1, 5]);
        let both = flash_colour(&mut app).alpha;
        assert_eq!(playing(&mut app).len(), 2, "a thud each");
        let mut flashes = app.world_mut().query_filtered::<(), With<PainFlash>>();
        assert_eq!(flashes.iter(app.world()).count(), 1, "one flash");
        // The stronger of the two, whichever order they came in.
        let (mut lone, _p, snake) = delve_with_hud();
        bite(&mut lone, snake, Species::Rat, 5);
        assert_eq!(both, flash_colour(&mut lone).alpha);
        let (mut reversed, _p, snake) = delve_with_hud();
        bite_all(&mut reversed, snake, &[5, 1]);
        assert_eq!(flash_colour(&mut reversed).alpha, both);
    }

    #[test]
    fn the_flash_is_gone_after_a_third_of_a_second() {
        let (mut app, _player, snake) = delve_with_hud();
        bite(&mut app, snake, Species::Rat, 5);
        let start = flash_colour(&mut app).alpha;
        for _ in 0..10 {
            app.update();
        }
        let mid = flash_colour(&mut app).alpha;
        assert!(mid > 0.0 && mid < start, "fading: {start} then {mid}");
        for _ in 0..12 {
            app.update();
        }
        assert_eq!(flash_colour(&mut app).alpha, 0.0);
    }

    #[test]
    fn the_jolt_bearing_is_clockwise_from_the_facing() {
        // Turned 90 degrees to face -x, an attacker at -z is on the delver's right.
        let right = bearing_from_facing(Vec2::ZERO, std::f32::consts::FRAC_PI_2, Vec2::new(0.0, -10.0));
        assert!((right - std::f32::consts::FRAC_PI_2).abs() < 1e-4, "bearing {right}");
    }

    /// The volumes of the sound effects playing, loudest last.
    fn playing(app: &mut App) -> Vec<f32> {
        let mut query = app.world_mut().query_filtered::<&PlaybackSettings, With<crate::audio::SoundEffect>>();
        let mut volumes: Vec<f32> = query.iter(app.world()).map(|p| p.volume.to_linear()).collect();
        volumes.sort_by(f32::total_cmp);
        volumes
    }

    #[test]
    fn each_landed_blow_sounds_one_thud_at_the_effects_volume() {
        let (mut app, _player, snake) = delve();
        app.insert_resource(super::super::feedback::CreatureSounds::default())
            .insert_resource(crate::settings::GameSettings { sound_volume: 0.5, ..default() })
            .init_resource::<crate::tuning::Tuning>();
        let wanted = crate::audio::effect_volume(app.world().resource(), app.world().resource());
        assert!(wanted > 0.0);
        app.update();
        assert!(playing(&mut app).is_empty(), "no blow, no thud");
        bite(&mut app, snake, Species::Rat, 1);
        assert_eq!(playing(&mut app), vec![wanted]);
        bite(&mut app, snake, Species::Rat, 1);
        assert_eq!(playing(&mut app), vec![wanted, wanted], "one thud a blow");
        app.update();
        assert_eq!(playing(&mut app).len(), 2, "and no more without one");
    }

    #[test]
    fn the_thud_is_silent_with_the_effects_volume_at_zero() {
        let (mut app, _player, snake) = delve();
        app.insert_resource(super::super::feedback::CreatureSounds::default())
            .insert_resource(crate::settings::GameSettings { sound_volume: 0.0, ..default() })
            .init_resource::<crate::tuning::Tuning>();
        bite(&mut app, snake, Species::Rat, 1);
        assert_eq!(playing(&mut app), vec![0.0]);
    }

    #[test]
    fn the_vignette_is_off_above_a_quarter_and_stronger_nearer_zero() {
        for t in [0.0, 0.7, 1.5, 2.2, 2.9] {
            assert_eq!(vignette_strength(30, 100, t), 0.0, "off at 30%");
            assert_eq!(vignette_strength(25, 100, t), 0.0, "off at exactly 25%");
            assert!(vignette_strength(20, 100, t) > 0.0, "on at 20%");
            assert!(vignette_strength(5, 100, t) > vignette_strength(20, 100, t), "stronger at 5% than 20% at {t}");
            assert!(vignette_strength(0, 100, t) >= vignette_strength(5, 100, t));
            assert!(vignette_strength(1, 100, t) <= 1.0);
        }
        // It pulses.
        assert_ne!(vignette_strength(10, 100, 0.0), vignette_strength(10, 100, VIGNETTE_PULSE / 4.0));
    }

    fn edge_colour(app: &mut App) -> Srgba {
        let mut query = app.world_mut().query_filtered::<&BorderColor, With<PainEdge>>();
        query.single(app.world()).unwrap().top.to_srgba()
    }

    #[test]
    fn the_edge_darkens_while_badly_hurt_and_clears_on_healing() {
        let (mut app, player, _snake) = delve_with_hud();
        assert_eq!(edge_colour(&mut app).alpha, 0.0);
        *app.world_mut().get_mut::<Health>(player).unwrap() = Health { hp: 2, max: 10 };
        app.update();
        let low = edge_colour(&mut app);
        assert!(low.alpha > 0.0, "dark at 20%");
        assert!(low.red < 0.5, "a dark red, not the flash: {low:?}");
        app.world_mut().get_mut::<Health>(player).unwrap().hp = 1;
        app.update();
        app.update();
        assert!(edge_colour(&mut app).alpha > 0.0);
        app.world_mut().get_mut::<Health>(player).unwrap().hp = 3;
        app.update();
        assert_eq!(edge_colour(&mut app).alpha, 0.0, "off at 30%");
    }

    #[test]
    fn the_low_health_edge_remains_when_the_flash_is_gone() {
        let (mut app, player, snake) = delve_with_hud();
        *app.world_mut().get_mut::<Health>(player).unwrap() = Health { hp: 20, max: 100 };
        bite(&mut app, snake, Species::Rat, 1);
        assert!(app.world().get::<Health>(player).unwrap().hp < 25);
        assert!(flash_colour(&mut app).alpha > 0.0);
        for _ in 0..25 {
            app.update();
        }
        assert_eq!(flash_colour(&mut app).alpha, 0.0);
        let edge = edge_colour(&mut app);
        assert!(edge.alpha > 0.0 && edge.red < 0.5, "the dark edge stays: {edge:?}");
    }

    #[test]
    fn a_blow_that_lands_tells_the_voice_how_hard_it_was() {
        let (mut app, _, snake) = delve();
        app.update();
        bite(&mut app, snake, Species::Rat, 3);
        let hurts: Vec<_> = app.world().resource::<Messages<super::super::voice::DelverHurt>>().iter_current_update_messages().copied().collect();
        assert_eq!(hurts.len(), 1);
        assert_eq!((hurts[0].max, hurts[0].down), (10, false));
        assert!(hurts[0].damage > 0);
    }

    #[test]
    fn the_gasp_blinks_twice_a_second_from_full() {
        assert_eq!(gasp_blink(0.0), 1.0);
        assert!((gasp_blink(0.25) - 0.3).abs() < 1e-5, "dim half a blink in");
        assert!((gasp_blink(0.5) - 1.0).abs() < 1e-5, "full again a blink later");
        // Under three flashes a second.
        const { assert!(GASP_BLINKS < 3.0) };
        assert_eq!(gasp_spread(0.0), GASP_SPREAD.0);
        assert_eq!(gasp_spread(LAST_GASP), GASP_SPREAD.1);
    }

    #[test]
    fn through_the_last_gasp_blood_blinks_in_from_the_edge_and_a_draught_clears_it() {
        let (mut app, player, snake) = delve_with_hud();
        app.init_resource::<super::super::blood::GaspBlood>();
        bite(&mut app, snake, Species::Rat, 25);
        app.update();
        assert!(app.world().get_resource::<Dying>().is_some());
        let mut seen = vec![];
        for _ in 0..90 {
            app.update();
            seen.push(*app.world().resource::<super::super::blood::GaspBlood>());
        }
        let (low, high) = seen.iter().fold((1.0f32, 0.0f32), |(l, h), b| (l.min(b.strength), h.max(b.strength)));
        assert!(high > 0.9 && low < 0.4, "it blinks: {low} to {high}");
        assert!(seen.windows(2).all(|w| w[1].spread >= w[0].spread), "it only creeps in");
        assert!(seen[89].spread > seen[0].spread);
        // A draught in time: the blood is gone.
        app.world_mut().get_mut::<Health>(player).unwrap().hp = 10;
        app.update();
        app.update();
        assert_eq!(*app.world().resource::<super::super::blood::GaspBlood>(), Default::default());
    }

    #[test]
    fn the_flash_and_edge_are_part_of_the_hud_and_hide_under_screens() {
        use crate::carry::hide_hud_under_screens;
        let (mut app, _player, _snake) = delve_with_hud();
        app.add_systems(Update, hide_hud_under_screens);
        let mut query = app.world_mut().query_filtered::<&ChildOf, With<PainEdge>>();
        let root = query.single(app.world()).unwrap().parent();
        let mut flash = app.world_mut().query_filtered::<&ChildOf, With<PainFlash>>();
        assert_eq!(flash.single(app.world()).unwrap().parent(), root, "the flash hangs from the same HUD");
        assert!(app.world().get::<Hud>(root).is_some());
        assert!(app.world().get::<DespawnOnExit<AppState>>(root).is_some(), "scoped to the delve");
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(crate::play::Screen::Inventory);
        app.update();
        assert_eq!(*app.world().get::<Visibility>(root).unwrap(), Visibility::Hidden);
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
        app.update();
        assert_eq!(*app.world().get::<Visibility>(root).unwrap(), Visibility::Inherited);
    }
}
