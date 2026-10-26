//! Identifying unknown items: the Identify key (Y) over an unknown item in the inventory rolls a
//! d20 check, shown as dice ([`crate::dice`]), against the item's difficulty
//! ([`fb_items::identify::dc`]). The delver's Intelligence modifier and their class (a mage
//! studies magic, a cleric knows blessings) are its modifiers. A roll that meets the difficulty
//! reveals every affix; a lower one reveals some, or nothing, and the item may be tried again
//! ([`fb_items::identify::learned`]).
//!
//! Magic and better items from a level's containers are found unknown ([`found_unknown`]) and
//! show as "Unknown ring" on their card. What an item is, and what it does, is not changed by
//! being unknown: only the card hides it. Each item's state is in its record, so it stays
//! identified through saves, and a record from before reads as fully known.
//!
//! `FB_DEBUG_UNKNOWN=1` puts an unknown ring in the pack at the start, for captures; `=2` adds a
//! partly known sword, `=3` makes the ring identified and `=4` has a try on it that fell short.

use bevy::prelude::*;
use fb_character::check::Check;
use fb_faction::ClassKind;
use fb_items::Item;
use procgen_rng::Mulberry32;

use crate::character::{Character, modifier};
use crate::camp::Asleep;
use crate::combat::RunDice;
use crate::dice::{RollDone, RollPurpose, RollRequest};
use crate::inventory::{Hovered, Inventory, ItemId, Toasts};
use crate::keymap::{Action, ActionInput};
use crate::launch::AppState;
use crate::play::{DelveSystems, inventory_open};
use crate::record::WorldRecord;

pub struct IdentifyPlugin;

impl Plugin for IdentifyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Identifying>().add_message::<RollRequest>().add_message::<RollDone>();
        if std::env::var_os("FB_DEBUG_UNKNOWN").is_some() {
            app.add_systems(Update, give_unknown_for_capture.in_set(DelveSystems));
        }
        app.add_systems(OnExit(AppState::Delving), |mut identifying: ResMut<Identifying>| identifying.0 = None);
        app.add_systems(Update, (count_rests, start_identifying.run_if(inventory_open), finish_identifying).chain().in_set(DelveSystems));
    }
}

/// The item whose dice are showing.
#[derive(Resource, Default, Debug)]
pub struct Identifying(Option<ItemId>);

/// What a class adds to identifying: a mage studies magic, a cleric knows a blessing.
pub fn class_bonus(class: ClassKind) -> (&'static str, i32) {
    match class {
        ClassKind::Mage => ("Mage", 2),
        ClassKind::Cleric => ("Cleric", 1),
        ClassKind::Fighter => ("Fighter", 0),
        ClassKind::Ranger => ("Ranger", 0),
    }
}

/// The check that identifies `item`: its difficulty against the delver's Intelligence and class.
pub fn check_for(item: &Item, intelligence: i32, class: ClassKind) -> Check {
    let (name, bonus) = class_bonus(class);
    Check::d20(format!("Identify the {}", item.base.to_lowercase()), item.identify_dc()).stat("INT", intelligence).class(name, bonus)
}

/// `item` as found in a level's container: unknown if it is magic or better.
pub fn found_unknown(item: Item) -> Item {
    item.unidentified()
}

/// Counts each waking from a sleep in [`WorldRecord::rests`], which a failed identify or
/// persuasion waits for.
fn count_rests(asleep: Option<Res<Asleep>>, record: Option<ResMut<WorldRecord>>, mut was_asleep: Local<bool>) {
    let (Some(asleep), Some(mut record)) = (asleep, record) else { return };
    let now = asleep.0.is_some();
    if *was_asleep && !now {
        record.rests += 1;
    }
    *was_asleep = now;
}

/// The Identify key over an unknown item rolls to identify it. One roll at a time.
fn start_identifying(
    input: ActionInput,
    hovered: Res<Hovered>,
    inventory: Res<Inventory>,
    record: Option<Res<WorldRecord>>,
    character: Option<Res<Character>>,
    dice: Option<ResMut<RunDice>>,
    mut identifying: ResMut<Identifying>,
    mut rolls: MessageWriter<RollRequest>,
    mut toasts: Toasts,
) {
    if !input.just_pressed(Action::Identify) || identifying.0.is_some() {
        return;
    }
    let Some((id, _)) = hovered.0 else { return };
    let Some(item) = inventory.item(id) else { return };
    if item.is_known() {
        toasts.say("There is nothing more to learn of it");
        return;
    }
    if !item.can_identify(record.as_deref().map_or(0, |r| r.rests)) {
        toasts.say("Try again after resting");
        return;
    }
    let (intelligence, class) = character.as_deref().map_or((10, ClassKind::Fighter), |c| (c.scores().intelligence, c.class.faction_class()));
    let check = check_for(item, modifier(intelligence), class);
    let roll = match dice {
        Some(mut dice) => check.roll(&mut dice.0),
        None => check.roll(&mut Mulberry32::new(item.seed)),
    };
    identifying.0 = Some(id);
    rolls.write(RollRequest { roll, purpose: RollPurpose::Identify(id) });
}

/// Applies the roll once its dice have closed.
fn finish_identifying(mut done: MessageReader<RollDone>, mut identifying: ResMut<Identifying>, mut inventory: ResMut<Inventory>, record: Option<Res<WorldRecord>>, mut toasts: Toasts) {
    for finished in done.read() {
        let RollPurpose::Identify(id) = finished.purpose else { continue };
        identifying.0 = None;
        let Some(mut item) = inventory.item(id).cloned() else { continue };
        let hidden = item.hidden_affixes();
        item.reveal_at(finished.roll.total, record.as_deref().map_or(0, |r| r.rests));
        let learned = hidden - item.hidden_affixes();
        let line = if item.is_known() {
            format!("You identify the {}", item.title().to_lowercase())
        } else if learned > 0 {
            format!("You learn some of what the {} holds", item.base.to_lowercase())
        } else {
            format!("You learn nothing of the {}", item.base.to_lowercase())
        };
        if inventory.replace_item(id, item) {
            toasts.say(line);
        }
    }
}

/// For captures (`FB_DEBUG_UNKNOWN`): an unknown ring in the pack, and with `=2` also a partly
/// known one and an unknown sword.
fn give_unknown_for_capture(mut inventory: ResMut<Inventory>, mut frames: Local<u32>) {
    *frames += 1;
    if *frames != 3 {
        return;
    }
    let both = std::env::var("FB_DEBUG_UNKNOWN").is_ok_and(|v| v.trim() == "2");
    let identified = std::env::var("FB_DEBUG_UNKNOWN").is_ok_and(|v| v.trim() == "3");
    let tried = std::env::var("FB_DEBUG_UNKNOWN").is_ok_and(|v| v.trim() == "4");
    let found = |base: &str, min_affixes: usize| (1..20_000).filter_map(|s| fb_items::roll_item_with(base, s, fb_items::Odds::loot(10))).find(|i| i.rarity >= fb_items::Rarity::Rare && i.affixes.len() >= min_affixes);
    let mut items = vec![found("Ring", 2).map(found_unknown).map(|mut ring| {
        if identified {
            ring.reveal(ring.identify_dc());
        }
        if tried {
            ring.reveal_at(ring.identify_dc() - 3, 0);
        }
        ring
    })];
    if both {
        items.push(found("Sword", 3).map(|mut i| {
            i = found_unknown(i);
            i.reveal(i.identify_dc() - 2);
            i
        }));
    }
    for item in items.into_iter().flatten() {
        let id = inventory.add(item);
        inventory.pick_up(id).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Toast;
    use fb_items::{Known, Rarity, roll_item_with};

    fn unknown_ring() -> Item {
        (1..20_000).filter_map(|s| roll_item_with("Ring", s, fb_items::Odds::loot(10))).find(|i| i.rarity >= Rarity::Rare && i.affixes.len() >= 2).expect("a ring with affixes").unidentified()
    }

    fn app_with(item: Item) -> (App, ItemId) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<Toast>().init_resource::<Identifying>().insert_resource(Inventory::default()).add_message::<RollDone>().add_message::<RollRequest>();
        app.add_systems(Update, finish_identifying);
        let id = app.world_mut().resource_mut::<Inventory>().add(item);
        (app, id)
    }

    fn close(app: &mut App, id: ItemId, total: i32) {
        let roll = Check::d20("Identify", 1).stat("INT", total - 1).with_natural(1);
        app.world_mut().write_message(RollDone { roll, purpose: RollPurpose::Identify(id) });
        app.update();
    }

    #[test]
    fn a_high_roll_identifies_the_item_in_the_pack() {
        let (mut app, id) = app_with(unknown_ring());
        let dc = app.world().resource::<Inventory>().item(id).unwrap().identify_dc();
        close(&mut app, id, dc);
        assert!(app.world().resource::<Inventory>().item(id).unwrap().is_known());
    }

    #[test]
    fn a_low_roll_reveals_some_and_a_bad_one_nothing() {
        let (mut app, id) = app_with(unknown_ring());
        let dc = app.world().resource::<Inventory>().item(id).unwrap().identify_dc();
        close(&mut app, id, dc - fb_items::identify::NOTHING_BELOW);
        assert_eq!(app.world().resource::<Inventory>().item(id).unwrap().known, Known::Unknown);
        close(&mut app, id, dc - 2);
        assert!(matches!(app.world().resource::<Inventory>().item(id).unwrap().known, Known::Partly(_)));
    }

    #[test]
    fn a_failed_identify_waits_for_the_next_rest() {
        let (mut app, id) = app_with(unknown_ring());
        app.insert_resource(WorldRecord::default()).insert_resource(Asleep::default());
        app.add_systems(Update, count_rests.before(finish_identifying));
        let dc = app.world().resource::<Inventory>().item(id).unwrap().identify_dc();
        close(&mut app, id, dc - 3);
        let item = app.world().resource::<Inventory>().item(id).unwrap().clone();
        assert!(!item.is_known() && !item.can_identify(app.world().resource::<WorldRecord>().rests));
        // Sleeping and waking is one rest.
        app.world_mut().resource_mut::<Asleep>().0 = Some(crate::camp::Sleeping { camp: fb_items::camp::Camp::Bedroll, bed: Entity::PLACEHOLDER, hit_points: 1, since: 0.0, began: 0.0 });
        app.update();
        assert_eq!(app.world().resource::<WorldRecord>().rests, 0);
        app.world_mut().resource_mut::<Asleep>().0 = None;
        app.update();
        let rests = app.world().resource::<WorldRecord>().rests;
        assert_eq!(rests, 1);
        assert!(item.can_identify(rests));
    }

    #[test]
    fn the_check_reads_the_delvers_intelligence_and_class() {
        let ring = unknown_ring();
        let roll = check_for(&ring, 2, ClassKind::Mage).with_natural(5);
        assert_eq!(roll.total, 5 + 2 + 2);
        assert_eq!(roll.target, ring.identify_dc());
        assert!(roll.name.contains("ring"));
    }
}
