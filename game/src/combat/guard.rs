//! Raising a shield. With a shield in the off hand, holding the block key (right click) raises
//! it as a guard ([`Guard`]); with none, the same key swings what the off hand holds, or
//! punches with it empty (`super::swing::start_swing`). While the guard is up:
//!
//! - a blow that would hit the delver from ahead of them, within [`GUARD_ARC`] of where they
//!   face, is turned aside at the shield's block chance (its base by size, `fb_items::profile`,
//!   and any affix "of Deflection" on top; `super::vitals::BlockChance`). A blow from behind or
//!   the flank finds them open. A turned blow is a miss with a clang
//!   (`crate::creatures::behaviour::MissedDelver::blocked`), and takes
//!   `rules.stamina.block_cost` of their stamina ([`take_blocks`]);
//! - they cannot swing, and the guard drops once they are winded, so a delver who only blocks
//!   runs out of breath;
//! - they walk at [`GUARD_PACE`] of their pace and cannot sprint (`crate::player::walk`);
//! - the shield comes up across the left of the view, its face toward what is ahead
//!   ([`guard_held`]).
//!
//! The arc, the raise time and the pose are our own; the arc is the one skeletons block in
//! (`crate::creatures::skeletons`). `FB_GUARD=1` keeps the guard up, for captures.

use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use fb_body::Side;

use super::Health;
use super::swing::Swing;
use crate::carry::{HeldItem, ViewModelCamera};
use crate::character::DelverStamina;
use crate::creatures::behaviour::MissedDelver;
use crate::inventory::Inventory;
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, Player};

pub struct GuardPlugin;

impl Plugin for GuardPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MissedDelver>().add_systems(Startup, load_block_sound).add_systems(
            Update,
            (raise_guard.before(super::swing::start_swing).before(super::shoot::shoot), take_blocks, guard_held.after(crate::carry::sway_held)).in_set(DelveSystems),
        );
    }
}

/// How far to either side of where the delver faces a raised shield covers, radians: a blow
/// from within it can be turned aside.
pub const GUARD_ARC: f32 = std::f32::consts::FRAC_PI_3;
/// How fast a delver with their guard up moves, as a share of their pace. Our own.
pub const GUARD_PACE: f32 = 0.5;
/// Seconds the shield takes to come up, and to go down again.
pub const RAISE_TIME: f32 = 0.12;

/// On the [`Player`] while their shield is raised: where they stand and face (world `(x, z)`)
/// as of this frame, kept here so the blows rolled at them need not read their `Transform`
/// (`crate::creatures::behaviour::BlowsAtDelver`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Guard {
    pub at: Vec2,
    pub facing: Vec2,
}

impl Guard {
    /// Facing the way `transform` does, where it stands.
    pub fn of(transform: &Transform) -> Guard {
        Guard { at: transform.translation.xz(), facing: transform.forward().as_vec3().xz() }
    }

    /// Whether a blow from `from` (world `(x, z)`) comes at the raised shield ([`covers`]).
    pub fn covers(&self, from: Vec2) -> bool {
        covers(self.at, self.facing, from)
    }
}

/// Whether what the off hand holds is a shield.
pub fn holds_shield(inventory: &Inventory) -> bool {
    let slot = fb_items::Slot::Offhand;
    inventory.worn(slot).and_then(|id| inventory.item(id)).is_some_and(|item| crate::character::is_shield(slot, item))
}

/// Whether a delver at `at` facing `forward` (world `(x, z)`) covers a blow from `from` with a
/// raised shield: it comes from within [`GUARD_ARC`] of where they face.
pub fn covers(at: Vec2, forward: Vec2, from: Vec2) -> bool {
    let to = from - at;
    if to.length_squared() < 1e-6 || forward.length_squared() < 1e-6 {
        return true;
    }
    forward.angle_to(to).abs() <= GUARD_ARC
}

/// Raises the guard while the block key is held with a shield in the off hand, the mouse taken
/// and the delver standing, not winded and not mid-swing; lowers it otherwise.
#[allow(clippy::type_complexity)]
fn raise_guard(
    mut commands: Commands,
    input: ActionInput,
    cursor: Option<Single<&CursorOptions, With<PrimaryWindow>>>,
    inventory: Res<Inventory>,
    mut players: Query<(Entity, &Transform, Option<&Health>, Option<&DelverStamina>, Option<&mut Guard>, Has<Swing>), With<Player>>,
) {
    let looking = cursor.is_none_or(|c| c.grab_mode != CursorGrabMode::None);
    let held = (input.pressed(Action::Block) && looking) || crate::launch::env_set("FB_GUARD");
    let shield = holds_shield(&inventory);
    for (player, at, health, stamina, guard, swinging) in &mut players {
        let able = !health.is_some_and(Health::is_down) && stamina.is_none_or(|s| s.0.can_exert()) && !swinging;
        let up = held && shield && able;
        match (up, guard) {
            (true, Some(mut guard)) => {
                guard.set_if_neq(Guard::of(at));
            }
            (true, None) => {
                commands.entity(player).insert(Guard::of(at));
            }
            (false, Some(_)) => {
                commands.entity(player).remove::<Guard>();
            }
            (false, None) => {}
        }
    }
}

/// The clang of a blow on a shield, synthesised by `tools/synth_bones.py` (a skeleton's shield
/// sounds the same).
#[derive(Resource, Clone, Debug, Default)]
pub struct BlockSound(pub Handle<AudioSource>);

fn load_block_sound(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    if let Some(assets) = assets {
        commands.insert_resource(BlockSound(assets.load("audio/bone_block.ogg")));
    }
}

/// Each blow the raised shield turned aside clangs on it and takes its stamina.
fn take_blocks(
    mut commands: Commands,
    mut misses: MessageReader<MissedDelver>,
    sound: Option<Res<BlockSound>>,
    rules: Option<Res<crate::rules::GameRules>>,
    settings: Option<Res<crate::settings::GameSettings>>,
    tuning: Option<Res<crate::tuning::Tuning>>,
    mut players: Query<(&Transform, Option<&mut DelverStamina>), With<Player>>,
) {
    let rules = crate::rules::GameRules::of(&rules);
    let game = settings.as_deref().copied().unwrap_or_default();
    let tuning = tuning.as_deref().cloned().unwrap_or_default();
    for miss in misses.read().filter(|m| m.blocked) {
        let Some((at, stamina)) = players.iter_mut().next() else { continue };
        if let Some(mut stamina) = stamina {
            let mut spent = stamina.0;
            spent.spend_with(rules, rules.stamina.block_cost);
            stamina.0 = spent;
        }
        if let Some(sound) = &sound {
            // Where the shield is: a step from the delver toward the blow.
            let toward = (miss.attacker_pos - at.translation.xz()).normalize_or_zero();
            let on = at.translation + Vec3::new(toward.x, 0.0, toward.y) * 2.0;
            crate::audio::play_at(&mut commands, "blow blocked", sound.0.clone(), on, &game, &tuning);
        }
    }
}

/// Where the raised shield's fist goes in the view, for the left hand, view model units: in
/// and up from where it is carried ([`crate::view_model::view_hold`]), short of the crosshair.
const RAISED_FIST: Vec3 = Vec3::new(-3.9, -3.6, -10.0);
/// How far the raised shield is turned about the view's up axis, radians: its face from the
/// left edge round toward what is ahead.
const RAISED_YAW: f32 = -1.15;

/// The turn and move that take an item carried at `carried` (its fist, view model space) to
/// the raised guard, `up` of the way there (0 to 1).
pub fn raised(carried: Vec3, up: f32) -> (Quat, Vec3) {
    let up = up.clamp(0.0, 1.0);
    let ease = up * up * (3.0 - 2.0 * up);
    (Quat::from_rotation_y(RAISED_YAW * ease), carried.lerp(RAISED_FIST, ease))
}

/// Brings the off hand's shield up across the view while the guard is raised, and down again,
/// about its fist, over [`RAISE_TIME`].
fn guard_held(
    time: Res<Time>,
    mut up: Local<f32>,
    guards: Query<Has<Guard>, With<Player>>,
    mut held: Query<(&HeldItem, &mut Transform, &ChildOf)>,
    view_models: Query<(), With<ViewModelCamera>>,
) {
    let target = if guards.iter().any(|g| g) { 1.0 } else { 0.0 };
    let step = time.delta_secs() / RAISE_TIME;
    *up = if target > *up { (*up + step).min(target) } else { (*up - step).max(target) };
    if *up <= 0.0 {
        return;
    }
    let carried = crate::view_model::view_hold(fb_body::Hold::Shield, Side::Left).fist;
    let (turn, fist) = raised(carried, *up);
    for (item, mut transform, parent) in &mut held {
        if item.hand == fb_items::Slot::Offhand && item.item.family() == armory::Family::Shield && view_models.contains(parent.parent()) {
            transform.translation = fist + turn * (transform.translation - carried);
            transform.rotation = turn * transform.rotation;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_raised_shield_covers_what_is_ahead_and_not_behind() {
        let (at, ahead) = (Vec2::ZERO, Vec2::new(0.0, -1.0));
        assert!(covers(at, ahead, Vec2::new(0.0, -5.0)));
        assert!(covers(at, ahead, Vec2::new(2.0, -5.0)), "a little to the side");
        assert!(!covers(at, ahead, Vec2::new(5.0, 0.0)), "the flank");
        assert!(!covers(at, ahead, Vec2::new(0.0, 5.0)), "behind");
    }

    #[test]
    fn the_shield_rises_from_where_it_is_carried_to_the_guard() {
        let carried = crate::view_model::view_hold(fb_body::Hold::Shield, Side::Left).fist;
        assert_eq!(raised(carried, 0.0), (Quat::IDENTITY, carried));
        let (turn, fist) = raised(carried, 1.0);
        assert_eq!(fist, RAISED_FIST);
        // Its face, out toward the left edge as carried, turns toward what is ahead.
        let face = turn * Vec3::NEG_X;
        assert!(face.z < -0.8, "{face}");
        // Still clear of the crosshair: nearer the middle, but low and to the left.
        assert!(fist.x > carried.x && fist.x < -2.0 && fist.y < -2.0);
    }

    /// A delver holding `off` in the off hand, with right click down and the mouse taken.
    fn guarding(off: Option<&str>) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<crate::keymap::Keymap>().init_resource::<ButtonInput<MouseButton>>();
        let mut inventory = Inventory::default();
        if let Some(base) = off {
            let id = inventory.add(fb_items::roll_item(base, 3).unwrap());
            inventory.wear(id, fb_items::Slot::Offhand).unwrap();
        }
        app.insert_resource(inventory).add_systems(Update, raise_guard);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Right);
        let delver = app.world_mut().spawn((Player, Transform::default(), DelverStamina::default())).id();
        (app, delver)
    }

    #[test]
    fn right_click_raises_a_held_shield_and_letting_go_lowers_it() {
        let (mut app, delver) = guarding(Some("Shield"));
        app.update();
        assert!(app.world().entity(delver).contains::<Guard>());
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Right);
        app.update();
        assert!(!app.world().entity(delver).contains::<Guard>());
    }

    #[test]
    fn there_is_no_guard_without_a_shield_or_with_a_torch() {
        for off in [None, Some("Torch")] {
            let (mut app, delver) = guarding(off);
            app.update();
            assert!(!app.world().entity(delver).contains::<Guard>(), "{off:?}");
        }
    }

    #[test]
    fn a_winded_delver_drops_their_guard() {
        let (mut app, delver) = guarding(Some("Shield"));
        app.update();
        app.world_mut().get_mut::<DelverStamina>(delver).unwrap().0.spend(1.0);
        app.update();
        assert!(!app.world().entity(delver).contains::<Guard>());
    }
}
