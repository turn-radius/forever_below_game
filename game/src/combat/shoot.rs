//! Shooting what the main hand holds: a bow is drawn by holding the attack key and loosed on
//! release; a crossbow fires on a press and is reloaded with the reload key. Each shot spends
//! one arrow or bolt from the worn quiver or bolt case, a real item that flies through the level
//! ([`fly_projectiles`]) and comes to rest on the floor, or in a body.
//!
//! - **Ammunition** (SRD 5.1 p. 64): a shot spends one piece, drawn from the quiver or case as
//!   part of the attack. With none worn, the attack swings the bow as an improvised club
//!   (`super::swing`).
//! - **Draw**: the bow draws over [`DRAW_SECONDS`]. A shot loosed before the draw is full flies
//!   slower, and so shorter, and does less ([`draw_damage_share`]). Our own.
//! - **Loading** (SRD 5.1 p. 65): a crossbow fires one bolt per load. [`reload_seconds`] is longer
//!   for a heavy crossbow and shorter with a stirrup. Our own times.
//! - **Flight**: speed from the draw, gravity ([`GRAVITY`]), the shaft turned along its flight.
//!   It is stepped in short substeps so no speed carries it through a wall or a body.
//! - **Hitting** (SRD 5.1 p. 95): where it meets a body, the range band comes from the distance it
//!   flew, and the attack rolls d20 + Dexterity (+ the weapon's `ToHit`) against the body's
//!   armour class, at disadvantage at long range, with a hostile within 5 feet of the shooter, or
//!   while poisoned ([`fb_character::combat::ranged_shot`]); a miss flies on. A hit does the
//!   weapon's dice + Dexterity and sticks in the body, and drops beside it when it falls, whole
//!   or broken ([`fb_character::combat::ammunition_breaks`]).
//! - **Landing**: one that meets a wall, a prop or the floor is set down there as an item, saved
//!   with the level like any dropped item.

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use fb_character::combat::{self as rules, FEET_TO_UNITS, Odds, Range, ToHit};
use fb_items::Slot;

use super::feedback::{Corpse, Struck};
use super::swing::{SwungAt, Strikeable};
use super::{ArmourClass, Health, RunDice};
use crate::carry::{HeldItem, ViewModelCamera, appearance_of};
use crate::character::{Character, DelverPoison, modifier};
use crate::creatures::Critter;
use crate::folk::Denizen;
use crate::hostility::Hostility;
use crate::inventory::{FloorChange, FloorChanges, Inventory, ItemId, Toasts};
use crate::keymap::{Action, ActionInput};
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Obstacles, Player, PlayerCamera};
use crate::render::{ArmoryPart, ItemSpawner, part_to_mesh};

pub struct ShootPlugin;

impl Plugin for ShootPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Marksman>().init_resource::<FloorChanges>().add_message::<Loosed>().add_systems(
            Update,
            (
                shoot.before(super::swing::start_swing),
                spawn_shots.after(shoot).run_if(resource_exists::<Assets<Mesh>>),
                show_loaded.after(shoot),
                fly_projectiles.after(shoot),
                release_stuck.after(fly_projectiles),
                (draw_held.after(crate::carry::sway_held).after(super::swing::swing_held), bend_held.after(crate::carry::show_held), show_ammunition_count).after(show_loaded),
            )
                .in_set(DelveSystems),
        );
    }
}

/// Seconds to draw a bow all the way back. Our own.
pub const DRAW_SECONDS: f32 = 1.0;
/// How fast a fully drawn bow's arrow leaves it, world units a second (60 m/s, a plausible
/// arrow). A crossbow's bolt leaves at [`BOLT_SPEED`]. Our own.
pub const FULL_DRAW_SPEED: f32 = 600.0;
pub const BOLT_SPEED: f32 = 700.0;
/// Gravity, world units a second squared (9.8 m/s).
pub const GRAVITY: f32 = 98.0;
/// The slowest a loosed arrow flies, as a share of full speed, for a draw of nothing.
const SLOWEST: f32 = 0.25;
/// The longest step the flight is cut into, world units, so nothing is skipped over.
const SUBSTEP: f32 = 2.0;
/// How long `FB_SHOOT` waits after the weapon's last change, seconds.
const AUTO_WAIT: f32 = 1.2;
/// How long a shot flies before it is called spent, seconds.
const LIFETIME: f32 = 12.0;
/// How far ahead of the eye a shot starts, world units, and how far under it.
const MUZZLE_AHEAD: f32 = 3.0;
const MUZZLE_DOWN: f32 = 0.4;
/// The arrow's own thickness for meeting bodies, world units.
const SHAFT_RADIUS: f32 = 0.25;

/// What the shooter has in hand this moment: the arrow on the string or the bolt on the deck,
/// how long the bow has been drawn and how far a reload is. On the run, not the player entity, so
/// the swing can read it the same frame.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct Marksman {
    pub nocked: Option<Nocked>,
    /// Seconds a bow has been drawn; `Some` from the press to the release.
    pub draw: Option<f32>,
    pub reload: Option<Reload>,
}

/// The ammunition in the weapon: a real item, out of its quiver while it is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nocked {
    pub ammo: ItemId,
    pub weapon: ItemId,
}

/// A crossbow being spanned and loaded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reload {
    pub t: f32,
    pub total: f32,
}

impl Marksman {
    /// How far through its draw the bow is, 0 to 1.
    pub fn drawn(&self) -> f32 {
        self.draw.map_or(0.0, |t| (t / DRAW_SECONDS).clamp(0.0, 1.0))
    }
}

/// A shot loosed, flying: the item that was spent, where it started and what its attack rolls.
#[derive(Component, Clone, Debug)]
pub struct Projectile {
    pub item: ItemId,
    pub velocity: Vec3,
    pub origin: Vec3,
    pub age: f32,
    pub shot: Shot,
    /// Bodies it has passed through (a miss, or out of range).
    pub passed: Vec<Entity>,
}

/// What a shot's attack rolls, fixed at the loosing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub range: Range,
    /// A hostile stood within 5 feet of the shooter.
    pub close: bool,
    pub dexterity: i32,
    pub to_hit: i32,
    pub dice: Option<(u32, u32)>,
    pub bonus: i32,
    /// The draw, 0 to 1 (1 for a crossbow).
    pub draw: f32,
    /// Odds the shooter brings of their own (poisoned).
    pub odds: Odds,
}

/// On an arrow stuck in a body: the item, and whether it broke on hitting. It drops with the body.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stuck {
    pub item: ItemId,
    pub broken: bool,
}

/// A bow or crossbow in the main hand, what it takes and how far it shoots.
pub fn ranged_in_hand(inventory: &Inventory) -> Option<(ItemId, fb_items::profile::Ranged)> {
    let id = inventory.worn(Slot::Mainhand)?;
    Some((id, inventory.item(id)?.profile.ranged?))
}

/// The worn container that takes `ammunition`: a quiver for arrows, a bolt case for bolts.
pub fn holder_of(inventory: &Inventory, ammunition: armory::Family) -> Option<ItemId> {
    inventory
        .carried_containers()
        .into_iter()
        .find(|&c| inventory.item(c).and_then(|i| i.profile.holds.as_ref()).is_some_and(|g| g.accepts.contains(&ammunition)))
}

/// The ammunition in `holder`, in its grid's order.
pub fn ammunition_in(inventory: &Inventory, holder: ItemId) -> Vec<ItemId> {
    inventory.grid(holder).map_or_else(Vec::new, |g| g.entries().iter().map(|e| e.id).collect())
}

/// How many pieces the delver can still shoot with what is in hand: those in the worn holder, and
/// the one on the string or the deck.
pub fn ammunition_left(inventory: &Inventory, marksman: &Marksman) -> Option<(armory::Family, usize)> {
    let (_, ranged) = ranged_in_hand(inventory)?;
    let held = holder_of(inventory, ranged.ammunition).map_or(0, |h| ammunition_in(inventory, h).len());
    Some((ranged.ammunition, held + usize::from(marksman.nocked.is_some())))
}

/// Whether the attack key shoots rather than swings: a bow or crossbow in hand with something to
/// shoot. With none, it is swung as an improvised club.
pub fn shoots(inventory: &Inventory, marksman: Option<&Marksman>) -> bool {
    let nocked = marksman.is_some_and(|m| m.nocked.is_some() || m.draw.is_some());
    match ranged_in_hand(inventory) {
        Some((_, ranged)) => nocked || holder_of(inventory, ranged.ammunition).is_some_and(|h| !ammunition_in(inventory, h).is_empty()),
        None => false,
    }
}

/// How fast a shot leaves the weapon: a bow's by how far it was drawn, a crossbow's always full.
pub fn launch_speed(crossbow: bool, draw: f32) -> f32 {
    if crossbow { BOLT_SPEED } else { FULL_DRAW_SPEED * (SLOWEST + (1.0 - SLOWEST) * draw.clamp(0.0, 1.0)) }
}

/// The share of its damage a shot of `draw` does: a short draw is weak. Our own.
pub fn draw_damage_share(draw: f32) -> f32 {
    0.4 + 0.6 * draw.clamp(0.0, 1.0)
}

/// Seconds to span and load a crossbow: a light one 1.5, a heavy one 3, a quarter off with a
/// stirrup to put the foot in. Our own.
pub fn reload_seconds(heavy: bool, stirrup: bool) -> f32 {
    (if heavy { 3.0 } else { 1.5 }) * if stirrup { 0.75 } else { 1.0 }
}

/// One step of flight: gravity pulls the velocity down, then the shot moves. Returns where it
/// is and how it moves after `dt`.
pub fn fly(position: Vec3, velocity: Vec3, dt: f32) -> (Vec3, Vec3) {
    let velocity = velocity - Vec3::Y * GRAVITY * dt;
    (position + velocity * dt, velocity)
}

/// The share of segment `a` to `b` at which it first meets the sphere `centre`, `radius`, or
/// `None` if it never does. Already inside is 0.
pub fn segment_sphere(a: Vec3, b: Vec3, centre: Vec3, radius: f32) -> Option<f32> {
    let d = b - a;
    let f = a - centre;
    let c = f.length_squared() - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    let aa = d.length_squared();
    if aa < 1e-9 {
        return None;
    }
    let bb = f.dot(d);
    let disc = bb * bb - aa * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-bb - disc.sqrt()) / aa;
    (0.0..=1.0).contains(&t).then_some(t)
}

/// How far (feet) a shot has flown to `at` from where it started.
fn feet_flown(origin: Vec3, at: Vec3) -> f32 {
    rules::units_to_feet(origin.distance(at))
}

/// Takes the ammunition at `nock` back into its holder (or the first carried container with
/// room), or loses it if there is no room.
fn put_back(inventory: &mut Inventory, ammo: ItemId, family: armory::Family) {
    let at = holder_of(inventory, family).and_then(|h| inventory.grid(h).and_then(|g| g.first_free(fb_items::footprint::Footprint { cols: 1, rows: 1 })).map(|at| (h, at)));
    match at {
        Some((holder, at)) if inventory.put(ammo, holder, at).is_ok() => {}
        _ => {
            if inventory.stow(ammo).is_err() {
                inventory.forget(ammo);
            }
        }
    }
}

/// What the shooter's input reads of the world around it.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Around<'w, 's> {
    cursor: Option<Single<'w, 's, &'static CursorOptions, With<PrimaryWindow>>>,
    screens: Option<Res<'w, crate::play::ScreenOpen>>,
    journey: Option<Res<'w, crate::travel::Journey>>,
    tuning: Res<'w, crate::tuning::Tuning>,
    sound: Option<Res<'w, super::swing::SwingSound>>,
    settings: Option<Res<'w, crate::settings::GameSettings>>,
}

/// The input of shooting: nocking and drawing a bow, loosing it, firing and reloading a crossbow,
/// and putting the ammunition back when the weapon is put away or the screen opens.
#[allow(clippy::too_many_arguments)]
pub fn shoot(
    mut commands: Commands,
    time: Res<Time>,
    input: ActionInput,
    around: Around,
    mut inventory: ResMut<Inventory>,
    mut marksman: ResMut<Marksman>,
    character: Res<Character>,
    mut toasts: Toasts,
    mut loosed: MessageWriter<Loosed>,
    mut state: Local<(f32, bool)>,
    players: Query<(&Transform, Option<&Health>, Option<&DelverPoison>), With<Player>>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    strikeable: Strikeable,
    folk: Query<(&Transform, &Hostility, &Denizen), Without<Corpse>>,
) {
    let dt = time.delta_secs();
    let weapon = ranged_in_hand(&inventory);
    let (auto_gate, auto_nocked) = &mut *state;
    // Whatever is on the string or the deck goes back if its weapon is no longer in hand.
    if let Some(nocked) = marksman.nocked
        && (weapon.map(|w| w.0) != Some(nocked.weapon) || inventory.item(nocked.ammo).is_none())
    {
        if inventory.item(nocked.ammo).is_some() {
            let family = inventory.item(nocked.ammo).map_or(armory::Family::Arrow, |i| i.shape.family());
            put_back(&mut inventory, nocked.ammo, family);
        }
        *marksman = Marksman::default();
    }
    let Some((weapon_id, ranged)) = weapon else {
        if *marksman != Marksman::default() {
            *marksman = Marksman::default();
        }
        return;
    };
    if marksman.reload.is_some() && marksman.nocked.is_some() {
        marksman.reload = None;
    }
    let screens = around.screens.as_ref().is_some_and(|s| s.0.is_some()) || around.journey.is_some();
    let Ok((at, health, poison)) = players.single() else { return };
    if screens || health.is_some_and(Health::is_down) {
        // The screen opened mid-draw: lower the bow and put the arrow back.
        if marksman.draw.is_some() || marksman.reload.is_some() {
            if let Some(nocked) = marksman.nocked.take() {
                put_back(&mut inventory, nocked.ammo, ranged.ammunition);
            }
            marksman.draw = None;
            marksman.reload = None;
        }
        return;
    }
    let looking = around.cursor.as_ref().is_none_or(|c| c.grab_mode != CursorGrabMode::None);
    let crossbow = ranged.ammunition == armory::Family::Bolt;
    // `FB_SHOOT=1` shoots over and over, for captures: a bow is drawn and loosed, a crossbow
    // reloaded and fired, each a moment after the last change so the view model is fitted
    // (the fitting is dropped if the weapon's state changes under it).
    *auto_gate += dt;
    if marksman.nocked.is_some() != *auto_nocked {
        *auto_nocked = marksman.nocked.is_some();
        *auto_gate = 0.0;
    }
    let auto = crate::launch::env_set("FB_SHOOT") && *auto_gate >= AUTO_WAIT;
    let pressed = (input.just_pressed(Action::Attack) && looking) || auto;

    let mut loose = None;
    if crossbow {
        if let Some(reload) = marksman.reload.as_mut() {
            reload.t += dt;
            if reload.t >= reload.total {
                marksman.reload = None;
                let ammo = holder_of(&inventory, ranged.ammunition).and_then(|h| ammunition_in(&inventory, h).first().copied());
                if let Some(ammo) = ammo {
                    inventory.take_out(ammo);
                    marksman.nocked = Some(Nocked { ammo, weapon: weapon_id });
                }
            }
        } else if marksman.nocked.is_none() && (input.just_pressed(Action::Reload) || auto) {
            let has = holder_of(&inventory, ranged.ammunition).is_some_and(|h| !ammunition_in(&inventory, h).is_empty());
            if has {
                let item = inventory.item(weapon_id);
                let heavy = item.and_then(|i| i.profile.stats.damage).is_some_and(|d| d.sides >= 10);
                let stirrup = matches!(item.map(|i| &i.shape), Some(armory::Item::Crossbow(s)) if s.stock.stirrup);
                marksman.reload = Some(Reload { t: 0.0, total: reload_seconds(heavy, stirrup) });
            } else {
                toasts.say_once("No bolts to load");
            }
        }
        if pressed {
            if marksman.nocked.is_some() {
                loose = Some(1.0);
            } else if marksman.reload.is_none() && shoots(&inventory, Some(&marksman)) {
                toasts.say_once("Reload first");
            }
        }
    } else {
        if pressed && marksman.nocked.is_none() {
            let ammo = holder_of(&inventory, ranged.ammunition).and_then(|h| ammunition_in(&inventory, h).first().copied());
            if let Some(ammo) = ammo {
                inventory.take_out(ammo);
                marksman.nocked = Some(Nocked { ammo, weapon: weapon_id });
                marksman.draw = Some(0.0);
            }
        }
        if let Some(t) = marksman.draw {
            if input.pressed(Action::Attack) || (crate::launch::env_set("FB_SHOOT") && t < DRAW_SECONDS) {
                marksman.draw = Some(t + dt);
            } else {
                loose = Some(marksman.drawn());
            }
        }
    }
    let Some(draw) = loose else { return };
    let Some(eye) = eyes.iter().next() else { return };
    let Some(nocked) = marksman.nocked.take() else { return };
    marksman.draw = None;
    let forward = eye.forward().as_vec3();
    let origin = eye.translation() + forward * MUZZLE_AHEAD - Vec3::Y * MUZZLE_DOWN;
    let stats = inventory.item(weapon_id).map(|i| i.profile.stats).unwrap_or_default();
    let hostile_close = {
        let me = at.translation.xz();
        let reach = rules::CLOSE_COMBAT_FEET * FEET_TO_UNITS;
        strikeable.hostile_within(me, reach) || folk.iter().any(|(t, h, d)| h.is_hostile() && t.translation.xz().distance(me) - d.radius <= reach)
    };
    let shot = Shot {
        range: ranged.range,
        close: hostile_close,
        dexterity: modifier(character.dexterity()),
        to_hit: stats.to_hit.round() as i32,
        dice: stats.damage.map(|d| (d.count as u32, d.sides as u32)),
        bonus: stats.damage_bonus.round() as i32,
        draw,
        odds: poison.map_or(Odds::Even, |p| p.0.attack_odds()),
    };
    if inventory.item(nocked.ammo).is_none() {
        return;
    }
    let velocity = forward * launch_speed(crossbow, draw);
    loosed.write(Loosed(Projectile { item: nocked.ammo, velocity, origin, age: 0.0, shot, passed: Vec::new() }));
    if let Some(sound) = &around.sound {
        let game = around.settings.as_deref().copied().unwrap_or_default();
        commands.spawn((Name::new("loose"), AudioPlayer::new(sound.0.clone()), crate::audio::sound_effect(&game, &around.tuning), DespawnOnExit(crate::launch::AppState::Delving)));
    }
}

/// A shot loosed this frame, to be drawn flying ([`spawn_shots`]).
#[derive(Message, Clone, Debug)]
pub struct Loosed(pub Projectile);

/// Draws each shot loosed as the item it is, turned along its flight.
pub fn spawn_shots(mut loosed: MessageReader<Loosed>, inventory: Res<Inventory>, tuning: Res<crate::tuning::Tuning>, mut spawner: ItemSpawner) {
    for Loosed(projectile) in loosed.read() {
        let Some(item) = inventory.item(projectile.item) else { continue };
        let mesh = item.shape.build();
        let appearance = appearance_of(&item.shape);
        let rotation = Quat::from_rotation_arc(Vec3::X, projectile.velocity.normalize_or(Vec3::NEG_Z));
        spawner.spawn(&mesh, &appearance).insert((
            Name::new("Shot"),
            projectile.clone(),
            Transform { translation: projectile.origin, rotation, scale: Vec3::splat(tuning.view.floor_item_scale) },
            DespawnOnExit(crate::launch::AppState::Delving),
        ));
    }
}

/// Keeps the models of the weapon and its holder to what they hold: a bow with an arrow on the
/// string only while one is nocked, a crossbow spanned with a bolt on its deck only while one is
/// loaded, and a quiver or bolt case showing the pieces in its grid.
pub fn show_loaded(mut inventory: ResMut<Inventory>, marksman: Res<Marksman>) {
    // Peek first: a write through the resource marks the whole inventory changed.
    let weapon = ranged_in_hand(&inventory).map(|(id, _)| id);
    let ammo_state = |inventory: &Inventory, id: ItemId| match inventory.item(id).map(|i| &i.shape) {
        Some(armory::Item::Arrow(s)) | Some(armory::Item::Bolt(s)) => Some(s.clone()),
        _ => None,
    };
    let mut want_weapon = None;
    if let Some(weapon) = weapon {
        let loaded = marksman.nocked.filter(|n| n.weapon == weapon).and_then(|n| ammo_state(&inventory, n.ammo));
        let differs = match inventory.item(weapon).map(|i| &i.shape) {
            Some(armory::Item::Bow(s)) => s.arrow.on != loaded.is_some(),
            Some(armory::Item::Crossbow(s)) => s.arrow.on != loaded.is_some() || s.spanned != loaded.is_some(),
            _ => false,
        };
        if differs {
            want_weapon = Some((weapon, loaded));
        }
    }
    let mut holders = Vec::new();
    for container in inventory.carried_containers() {
        let Some(armory::Item::Quiver(state) | armory::Item::BoltCase(state)) = inventory.item(container).map(|i| &i.shape) else { continue };
        let capacity = if matches!(inventory.item(container).map(|i| &i.shape), Some(armory::Item::Quiver(_))) {
            armory::families::quiver::Quivers::capacity(state)
        } else {
            armory::families::quiver::BoltCases::capacity(state)
        };
        let contents: Vec<_> = ammunition_in(&inventory, container).into_iter().filter_map(|id| ammo_state(&inventory, id)).take(capacity).collect();
        if contents != state.contents {
            holders.push((container, contents));
        }
    }
    if want_weapon.is_none() && holders.is_empty() {
        return;
    }
    if let Some((weapon, loaded)) = want_weapon
        && let Some(shape) = inventory.shape_mut(weapon)
    {
        match shape {
            armory::Item::Bow(s) => match &loaded {
                Some(ammo) => s.load(ammo),
                None => s.unload(),
            },
            armory::Item::Crossbow(s) => match &loaded {
                Some(ammo) => {
                    s.load(ammo);
                    s.spanned = true;
                }
                // Fired: the string let go as well as the bolt.
                None => {
                    s.unload();
                    s.spanned = false;
                }
            },
            _ => {}
        }
    }
    for (container, contents) in holders {
        if let Some(armory::Item::Quiver(state) | armory::Item::BoltCase(state)) = inventory.shape_mut(container) {
            state.contents = contents;
        }
    }
}

/// Flies every shot a step: against bodies as they are posed, then the walls, props, floor and
/// ceiling. A hit rolls the attack; what stops is set down on the floor or stuck in the body.
#[allow(clippy::too_many_arguments)]
pub fn fly_projectiles(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<CurrentLevel>,
    blocked: Res<BlockedCells>,
    obstacles: Option<Res<Obstacles>>,
    mut dice: ResMut<RunDice>,
    mut changes: ResMut<FloorChanges>,
    mut struck: MessageWriter<Struck>,
    mut swung: MessageWriter<SwungAt>,
    mut shots: Query<(Entity, &mut Projectile, &mut Transform), (Without<Critter>, Without<Denizen>)>,
    strikeable: Strikeable,
    armour: Query<&ArmourClass>,
    globals: Query<&GlobalTransform>,
) {
    let level = &level.0;
    let relief = crate::level::geometry::Relief::new(level);
    let solid = crate::player::blocked_by(level, &blocked);
    let obstacles = obstacles.as_deref().cloned().unwrap_or_default();
    for (entity, mut shot, mut transform) in &mut shots {
        let dt = time.delta_secs().min(0.1);
        let speed = shot.velocity.length();
        let steps = ((speed * dt) / SUBSTEP).ceil().max(1.0) as u32;
        let sub = dt / steps as f32;
        let mut position = transform.translation;
        let mut done = false;
        for _ in 0..steps {
            let (next, velocity) = fly(position, shot.velocity, sub);
            // Bodies first: the nearest along this step.
            let far = rules::units_to_feet(shot.origin.distance(position)) > shot.shot.range.long_feet;
            let skip = shot.passed.clone();
            if let Some((t, target)) = strikeable.first_body_hit(position, next, SHAFT_RADIUS, &skip) {
                let at = position.lerp(next, t);
                let flown = feet_flown(shot.origin, at);
                let incoming = (next - position).normalize_or(Vec3::NEG_Z);
                match rules::ranged_shot(shot.shot.range, flown, shot.shot.close) {
                    Some(roll) if !far => {
                        swung.write(SwungAt { target });
                        let ac = armour.get(target).copied().unwrap_or(ArmourClass::UNARMOURED);
                        let odds = roll.odds.and(shot.shot.odds);
                        let attack = ToHit::dexterity(shot.shot.dexterity, shot.shot.to_hit).roll(ac.0, odds, &mut dice.0);
                        info!("shot: {} ({})", attack.line(), flown.round());
                        if attack.hit {
                            let full = rules::weapon_damage(shot.shot.dice, shot.shot.bonus, "DEX", shot.shot.dexterity, &mut dice.0).value() as f32;
                            let damage = ((full * draw_damage_share(shot.shot.draw)).round() as u32).max(1);
                            struck.write(Struck { target, damage, from: shot.origin.xz(), bludgeoning: false });
                            let broken = rules::ammunition_breaks(&attack);
                            if let Ok(body) = globals.get(target) {
                                let world = Transform { translation: at, rotation: Quat::from_rotation_arc(Vec3::X, incoming), scale: transform.scale };
                                let local = GlobalTransform::from(world).reparented_to(body);
                                commands.entity(entity).remove::<Projectile>().insert((local, ChildOf(target), Stuck { item: shot.item, broken }));
                            } else {
                                commands.entity(entity).despawn();
                            }
                            done = true;
                            break;
                        }
                    }
                    _ => {}
                }
                shot.passed.push(target);
            }
            // Then the stone: a wall or a prop, the floor, the ceiling.
            let cell = (next.xz() / fb_level::CELL).floor().as_ivec2();
            let wall = solid(cell.x, cell.y);
            let prop = obstacles.0.iter().any(|o| o.push_out(next.xz(), 0.0, position.xz()).is_some());
            let floor = relief.floor_height(next.x, next.z);
            let ceiling = crate::level::geometry::stone::ceiling_at(level, next.x, next.z);
            if wall || prop || next.y <= floor || next.y >= ceiling || shot.age > LIFETIME {
                let rest = if wall || prop { position } else { next };
                land(&mut commands, &mut changes, entity, shot.item, rest.xz(), shot.velocity.xz());
                done = true;
                break;
            }
            position = next;
            shot.velocity = velocity;
            shot.age += sub;
        }
        if !done {
            transform.translation = position;
            if shot.velocity.length_squared() > 1e-3 {
                transform.rotation = Quat::from_rotation_arc(Vec3::X, shot.velocity.normalize());
            }
        }
    }
}

/// Sets the shot's item down on the floor at `at` (world `(x, z)`), lying along `heading`, and
/// takes its flying model away; the floor machinery spawns it as any dropped item.
fn land(commands: &mut Commands, changes: &mut FloorChanges, entity: Entity, item: ItemId, at: Vec2, heading: Vec2) {
    let forward = heading.normalize_or(Vec2::NEG_Y);
    let yaw = (-forward.x).atan2(-forward.y);
    changes.0.push(FloorChange::SetDownAt(item, at / fb_level::CELL, yaw));
    commands.entity(entity).despawn();
}

/// Drops what is stuck in a body that has fallen: a whole arrow beside it, a broken one gone.
#[allow(clippy::too_many_arguments)]
pub fn release_stuck(
    mut commands: Commands,
    stuck: Query<(Entity, &Stuck, &ChildOf)>,
    corpses: Query<&Transform, With<Corpse>>,
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    level: Res<CurrentLevel>,
    blocked: Res<BlockedCells>,
    obstacles: Option<Res<Obstacles>>,
    tuning: Res<crate::tuning::Tuning>,
) {
    for (entity, stuck, parent) in &stuck {
        let Ok(body) = corpses.get(parent.parent()) else { continue };
        commands.entity(entity).despawn();
        if stuck.broken {
            inventory.forget(stuck.item);
            continue;
        }
        let Some(item) = inventory.item(stuck.item).cloned() else { continue };
        inventory.forget(stuck.item);
        let obstacles = obstacles.as_deref().cloned().unwrap_or_default();
        let solid = crate::player::blocked_by(&level.0, &blocked);
        crate::inventory::underfoot::set_down_around(body.translation.xz(), std::iter::once(&item), item.seed, &mut inventory, &mut changes, solid, &obstacles.0, tuning.view.floor_item_scale);
    }
}

/// Where the nock of a drawn bow's arrow comes to in the view: low and a little right of the
/// eye, as an archer anchors at the jaw. Our own.
const ANCHOR: Vec3 = Vec3::new(0.7, -1.6, -5.0);
/// How far a drawn bow leans its top in toward the crosshair, radians.
const CANT: f32 = 0.12;
/// The share of the draw over which the bow is raised from the carry into the aim.
const RAISE: f32 = 0.3;

/// A bow drawn `draw` aimed: the arrow on its string running from [`ANCHOR`] to the
/// crosshair, the limbs upright with a slight cant. `None` for anything but a bow.
fn aimed_bow(item: &armory::Item, draw: f32, scale: Vec3) -> Option<(Vec3, Quat)> {
    let armory::Item::Bow(bow) = item else { return None };
    let (nock, dir) = armory::families::bow::BowState { draw, ..bow.clone() }.nocked_arrow();
    let (nock, dir) = (Vec3::from_array(nock), Vec3::from_array(dir));
    // The bow on its edge: the arrow (toward -X in the bow's space) ahead, the limbs up.
    let upright = Quat::from_rotation_z(CANT) * Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
    let rotation = crate::view_model::aimed_from(ANCHOR) * Quat::from_rotation_arc(upright * dir, Vec3::NEG_Z) * upright;
    Some((ANCHOR - rotation * (scale * nock), rotation))
}

/// The share of a reload spent swinging the crossbow across the body, and again back.
const RELOAD_EASE: f32 = 0.2;
/// How far a reloaded crossbow is swung across the body, radians.
const RELOAD_SWING: f32 = 0.9;

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// What the main hand's bow or crossbow looks like this moment: a bow at its draw, a crossbow
/// part way through spanning while reloaded. `None` for anything else.
fn bent(item: &armory::Item, marksman: &Marksman) -> Option<(armory::Item, f32)> {
    match item {
        armory::Item::Bow(s) => {
            let draw = marksman.drawn();
            Some((armory::Item::Bow(armory::families::bow::BowState { draw, ..s.clone() }), draw))
        }
        armory::Item::Crossbow(s) => {
            let span = marksman.reload.map_or(0.0, |r| (r.t / r.total).clamp(0.0, 1.0));
            Some((armory::Item::Crossbow(armory::families::crossbow::CrossbowState { span, ..s.clone() }), span))
        }
        _ => None,
    }
}

/// Bends the bow or crossbow in hand to its draw or reload as it happens: the model is built
/// again at that draw and written over the meshes already shown, so the string comes back
/// smoothly and nothing is spawned again. Holds what it last showed for each held entity.
pub fn bend_held(
    marksman: Res<Marksman>,
    held: Query<(Entity, &HeldItem, &Children, &ChildOf)>,
    view_models: Query<(), With<ViewModelCamera>>,
    parts: Query<(&Name, &Mesh3d), With<ArmoryPart>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut shown: Local<Option<(Entity, f32)>>,
) {
    for (entity, item, children, parent) in &held {
        if item.hand != Slot::Mainhand || !view_models.contains(parent.parent()) {
            continue;
        }
        let Some((shape, amount)) = bent(&item.item, &marksman) else { continue };
        // A freshly spawned model is built at rest.
        let last = shown.filter(|(e, _)| *e == entity).map_or(0.0, |(_, a)| a);
        if (amount - last).abs() < 0.002 {
            continue;
        }
        let mesh = shape.build();
        let mut built = mesh.parts.iter();
        for child in children.iter() {
            let Ok((name, handle)) = parts.get(child) else { continue };
            let Some(part) = built.next() else { break };
            if part.name != name.as_str() {
                // A different set of parts: leave it for the next fitting.
                break;
            }
            if let Some(mut target) = meshes.get_mut(&handle.0) {
                *target = part_to_mesh(part);
            }
        }
        *shown = Some((entity, amount));
    }
}

/// Draws the bow in the view: raised into the aim at the start of the draw, the arrow pointing
/// at the crosshair, trembling at full draw; a crossbow dips as it is reloaded. Our own.
pub fn draw_held(marksman: Res<Marksman>, mut held: Query<(&HeldItem, &mut Transform, &ChildOf)>, view_models: Query<(), With<ViewModelCamera>>) {
    let draw = marksman.drawn();
    let reload = marksman.reload.map_or(0.0, |r| (r.t / r.total).clamp(0.0, 1.0));
    if draw <= 0.0 && reload <= 0.0 {
        return;
    }
    for (item, mut transform, parent) in &mut held {
        if item.hand != Slot::Mainhand || !view_models.contains(parent.parent()) {
            continue;
        }
        if draw > 0.0 {
            let Some((at, turn)) = aimed_bow(&item.item, draw, transform.scale) else { continue };
            let raised = (draw / RAISE).clamp(0.0, 1.0);
            let raised = raised * raised * (3.0 - 2.0 * raised);
            let shake = if draw >= 1.0 { (marksman.draw.unwrap_or(0.0) * 40.0).sin() * 0.03 } else { 0.0 };
            transform.translation = transform.translation.lerp(at, raised) + Vec3::Y * shake;
            transform.rotation = transform.rotation.slerp(turn, raised);
        } else {
            // Swung across the body about the nut, the stock turned to the left and its nose
            // dipped, so the deck is seen from the side as the string is hauled back to the
            // nut; brought back to the shoulder as the bolt is laid.
            let armory::Item::Crossbow(bow) = &item.item else { continue };
            let dip = smoothstep((reload / RELOAD_EASE).min((1.0 - reload) / RELOAD_EASE));
            let nut = transform.transform_point(Vec3::Y * bow.stock.length * bow.stock.nut);
            let swing = Quat::from_rotation_y(RELOAD_SWING * dip) * Quat::from_rotation_x(-0.3 * dip);
            // Rolled on the stock so the deck faces the eye.
            let stock = swing * transform.rotation * Vec3::Y;
            transform.rotate_around(nut, Quat::from_axis_angle(stock, 0.6 * dip) * swing);
            transform.translation += Vec3::new(-0.5, 0.6, -2.5) * dip;
        }
    }
}

/// The count shown next to the quickslots: "arrows 19" or "bolts 3", with "loading" while a
/// crossbow is reloaded. Empty with nothing to shoot with in hand.
#[derive(Component, Debug)]
pub struct AmmunitionCount;

fn show_ammunition_count(inventory: Res<Inventory>, marksman: Res<Marksman>, mut labels: Query<&mut Text, With<AmmunitionCount>>) {
    let text = match ammunition_left(&inventory, &marksman) {
        Some((family, count)) => {
            let noun = if family == armory::Family::Bolt { "bolts" } else { "arrows" };
            let state = if marksman.reload.is_some() { " (loading)" } else { "" };
            format!("{noun} {count}{state}")
        }
        None => String::new(),
    };
    for mut label in &mut labels {
        if label.0 != text {
            label.0 = text.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_segment_meets_a_sphere_where_it_enters() {
        let c = Vec3::new(10.0, 0.0, 0.0);
        assert_eq!(segment_sphere(Vec3::ZERO, Vec3::new(20.0, 0.0, 0.0), c, 2.0), Some(0.4));
        assert_eq!(segment_sphere(Vec3::ZERO, Vec3::new(5.0, 0.0, 0.0), c, 2.0), None, "stops short");
        assert_eq!(segment_sphere(Vec3::ZERO, Vec3::new(20.0, 5.0, 0.0), c, 2.0), None, "passes over");
        assert_eq!(segment_sphere(c, Vec3::new(20.0, 0.0, 0.0), c, 2.0), Some(0.0), "starts inside");
    }

    #[test]
    fn a_short_draw_flies_slower_and_hits_softer() {
        let speeds: Vec<f32> = [0.0, 0.5, 1.0].iter().map(|&d| launch_speed(false, d)).collect();
        assert!(speeds[0] < speeds[1] && speeds[1] < speeds[2]);
        assert_eq!(speeds[2], FULL_DRAW_SPEED);
        assert_eq!(launch_speed(true, 0.0), BOLT_SPEED);
        assert!(draw_damage_share(0.0) < draw_damage_share(0.5) && draw_damage_share(1.0) == 1.0);
    }

    #[test]
    fn a_heavy_crossbow_takes_longer_to_reload_and_a_stirrup_shortens_it() {
        assert!(reload_seconds(true, false) > reload_seconds(false, false));
        assert!(reload_seconds(false, true) < reload_seconds(false, false));
        assert!(reload_seconds(true, true) < reload_seconds(true, false));
    }

    #[test]
    fn gravity_bends_the_flight_down() {
        let (mut p, mut v) = (Vec3::new(0.0, 16.0, 0.0), Vec3::new(FULL_DRAW_SPEED, 0.0, 0.0));
        for _ in 0..60 {
            (p, v) = fly(p, v, 1.0 / 60.0);
        }
        assert!((p.x - FULL_DRAW_SPEED).abs() < 1.0);
        assert!((16.0 - p.y - 0.5 * GRAVITY).abs() < 2.0, "fell {} in a second", 16.0 - p.y);
    }

    #[test]
    fn out_of_range_shots_cannot_hit_and_long_ones_are_at_disadvantage() {
        let range = Range::new(80.0, 320.0);
        assert_eq!(rules::ranged_shot(range, 40.0, false).map(|s| s.odds), Some(Odds::Even));
        assert_eq!(rules::ranged_shot(range, 120.0, false).map(|s| s.odds), Some(Odds::Disadvantage));
        assert_eq!(rules::ranged_shot(range, 40.0, true).map(|s| s.odds), Some(Odds::Disadvantage), "a hostile within 5 feet");
        assert!(rules::ranged_shot(range, 400.0, false).is_none(), "beyond long range");
        assert!(rules::ranged_shot(range, 1000.0 / FEET_TO_UNITS * FEET_TO_UNITS, false).is_none());
    }
}
