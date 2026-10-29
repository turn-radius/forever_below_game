//! Keeps the delver's deed tally ([`fb_character::deeds::Tally`], on [`Character`]).
//!
//! The one place events are recorded: every kill of the delver's ([`Slain`]) counts against the
//! exact kind slain, its family and the weapon in their main hand, and every hit that hurt them
//! ([`DelverHurt`]) counts against each kind of armour and clothing they wear. Rules read the
//! tally; nothing else counts ([`crate::devotion`]).

use bevy::prelude::*;
use fb_character::deeds::Tally;
use fb_character::kin::Kin;
use fb_items::Slot;

use crate::character::Character;
use crate::combat::feedback::Slain;
use crate::combat::voice::DelverHurt;
use crate::creatures::Critter;
use crate::folk::Denizen;
use crate::inventory::Inventory;
use crate::play::DelveSystems;

pub struct DeedsPlugin;

impl Plugin for DeedsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Slain>().add_message::<DelverHurt>().add_systems(Update, record_deeds.in_set(DelveSystems).in_set(RecordsDeeds));
    }
}

/// The system that records, for those that read the tally to run after.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct RecordsDeeds;

/// The slots whose items are armour and clothing; a shield held in the off hand counts too.
const WORN_SLOTS: [Slot; 7] = [Slot::Head, Slot::Shirt, Slot::Cloak, Slot::Body, Slot::Hands, Slot::Legs, Slot::Feet];

/// The exact kind of a creature, as the tally keys it: "creature:giant spider".
pub fn creature_kind(species: fb_creatures::Species) -> String {
    format!("creature:{}", species.stat_block().name.to_lowercase())
}

/// The exact kind of a person, as the tally keys it: "folk:goblin", "folk:winged_demon".
pub fn folk_kind(folk: fb_body::Folk) -> String {
    format!("folk:{}", folk.key())
}

/// The kinds of armour and clothing `inventory` has on.
pub fn worn_kinds(inventory: &Inventory) -> Vec<armory::Family> {
    let mut kinds: Vec<armory::Family> = WORN_SLOTS.into_iter().filter_map(|slot| inventory.worn(slot).and_then(|id| inventory.item(id))).map(|item| item.shape.family()).collect();
    kinds.extend(inventory.worn(Slot::Offhand).and_then(|id| inventory.item(id)).map(|item| item.shape.family()).filter(|f| *f == armory::Family::Shield));
    kinds
}

/// Records this frame's kills and hits into the delver's tally.
pub fn record_deeds(mut slain: MessageReader<Slain>, mut hurt: MessageReader<DelverHurt>, creatures: Query<&Critter>, folk: Query<&Denizen>, inventory: Option<Res<Inventory>>, mut character: ResMut<Character>) {
    let kills: Vec<(String, Kin)> = slain
        .read()
        .filter(|s| !s.by_ally)
        .filter_map(|s| match (creatures.get(s.target), folk.get(s.target)) {
            (Ok(critter), _) => Some((creature_kind(critter.creature.species()), Kin::of_species(critter.creature.species()))),
            (_, Ok(denizen)) => Some((folk_kind(denizen.person.spec.folk), Kin::of_folk(denizen.person.spec.folk))),
            _ => None,
        })
        .collect();
    let hits = hurt.read().filter(|h| h.damage > 0).count();
    if kills.is_empty() && hits == 0 {
        return;
    }
    let weapon = inventory.as_deref().and_then(|i| i.worn(Slot::Mainhand).and_then(|id| i.item(id))).map(|item| item.shape.family());
    let tally: &mut Tally = &mut character.deeds;
    for (kind, kin) in kills {
        tally.kill(kind, kin, weapon);
    }
    if hits > 0 {
        let worn = inventory.as_deref().map(worn_kinds).unwrap_or_default();
        for _ in 0..hits {
            tally.hit_taken(worn.iter().copied());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_character::deeds::Tally;
    use fb_items::Foe;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_message::<Slain>().add_message::<DelverHurt>().init_resource::<Character>().init_resource::<Inventory>().add_systems(Update, record_deeds);
        app
    }

    #[test]
    fn a_kill_is_recorded_by_kind_and_family() {
        let mut app = app();
        let spot = fb_level::generate(1207, 1).creatures[0];
        let target = app.world_mut().spawn(Critter::new(spot, spot.creature())).id();
        app.world_mut().write_message(Slain { target, by_ally: false });
        app.update();
        let deeds = &app.world().resource::<Character>().deeds;
        assert_eq!(deeds.kills_of(Kin::of_species(spot.species)), 1);
        assert_eq!(deeds.kills_of_kind(&creature_kind(spot.species)), 1);
    }

    #[test]
    fn a_kill_with_a_bow_is_recorded_against_bows() {
        let mut app = app();
        let mut inventory = Inventory::default();
        let bow = inventory.add(fb_items::roll_item("Bow", 7).unwrap());
        inventory.wear(bow, Slot::Mainhand).unwrap();
        app.insert_resource(inventory);
        let spot = fb_level::generate(1207, 1).creatures[0];
        let target = app.world_mut().spawn(Critter::new(spot, spot.creature())).id();
        app.world_mut().write_message(Slain { target, by_ally: false });
        app.update();
        assert_eq!(app.world().resource::<Character>().deeds.kills_with(armory::Family::Bow), 1);
    }

    #[test]
    fn an_allys_kill_is_not_the_delvers() {
        let mut app = app();
        let spot = fb_level::generate(1207, 1).creatures[0];
        let target = app.world_mut().spawn(Critter::new(spot, spot.creature())).id();
        app.world_mut().write_message(Slain { target, by_ally: true });
        app.update();
        assert_eq!(app.world().resource::<Character>().deeds, Tally::default());
    }

    #[test]
    fn a_hit_is_recorded_against_the_armour_worn_and_a_hit_that_did_nothing_is_not() {
        let mut app = app();
        let mut inventory = Inventory::default();
        let gambeson = fb_items::roll_item("Gambeson", 3).unwrap();
        let family = gambeson.shape.family();
        let tunic = inventory.add(gambeson);
        inventory.wear(tunic, Slot::Body).unwrap();
        app.insert_resource(inventory);
        app.world_mut().write_message(DelverHurt { damage: 0, max: 10, down: false });
        app.world_mut().write_message(DelverHurt { damage: 3, max: 10, down: false });
        app.update();
        let deeds = &app.world().resource::<Character>().deeds;
        assert_eq!(deeds.hits_in(family), 1);
        assert_eq!(deeds.kills_of(Kin::Foe(Foe::Undead)), 0);
    }
}
