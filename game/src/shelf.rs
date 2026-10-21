//! What a tradesperson has to sell today: [`Shelf`], rolled by [`fb_items::stock::stock`] from
//! their spot's seed and the game day, so it restocks each game day (#548). A shelf left in
//! the record ([`crate::record::ShelfRecord`]) comes back the same on a revisit until the next
//! restock. Nothing buys or sells yet (#351); this only keeps the wares.

use bevy::prelude::*;
use fb_items::Item;

use crate::folk::{Denizen, Person};
use crate::game_clock::GameClock;
use crate::play::{CurrentLevel, DelveSystems};
use crate::record::{WorldRecord, rebuild_item};

/// A tradesperson's wares as of one game day.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Shelf {
    /// The game day it was stocked on (day 0 is the first).
    pub day: u32,
    pub wares: Vec<Item>,
}

/// The game day `clock` is on: whole days elapsed.
pub fn day_of(clock: &GameClock) -> u32 {
    clock.time.days().floor().max(0.0) as u32
}

/// The seed a trader's shelf is rolled from on `day`: their spot's seed, turned by the day.
pub fn shelf_seed(spot_seed: u32, day: u32) -> u32 {
    spot_seed ^ day.wrapping_mul(0x9E37_79B1).wrapping_add(0x5E1F_0D41)
}

/// What `person` has to sell on `day`: empty for someone with no trade, or one who sells
/// nothing (guards, enchanters). Pure in the person and the day.
pub fn roll_shelf(person: &Person, day: u32) -> Vec<Item> {
    let Some(shop) = person.spot.trade.and_then(|t| t.shop(&person.name, &person.faction.name)) else { return vec![] };
    fb_items::stock::stock(&shop, person.depth, shelf_seed(person.spot.seed, day))
}

/// Gives each tradesperson their shelf as they appear: the one the record kept if it is still
/// today's, else a fresh roll. Restocks any shelf whose day has passed.
pub fn stock_shelves(
    mut commands: Commands,
    clock: Option<Res<GameClock>>,
    level: Res<CurrentLevel>,
    record: Option<Res<WorldRecord>>,
    new: Query<(Entity, &Denizen), Without<Shelf>>,
    mut old: Query<(&Denizen, &mut Shelf)>,
) {
    let today = clock.as_deref().map_or(0, day_of);
    for (entity, denizen) in &new {
        let person = &denizen.person;
        if !person.spot.trade.is_some_and(|t| t.shop("", "").is_some()) {
            continue;
        }
        let kept = record.as_deref().and_then(|r| r.level(level.0.depth)).and_then(|l| {
            let index = level.0.folk.iter().position(|s| *s == person.spot)?;
            l.shelves.get(&index).filter(|s| s.day == today)
        });
        let wares = match kept {
            Some(saved) => saved.wares.iter().filter_map(rebuild_item).collect(),
            None => roll_shelf(person, today),
        };
        commands.entity(entity).insert(Shelf { day: today, wares });
    }
    for (denizen, mut shelf) in &mut old {
        if shelf.day != today {
            *shelf = Shelf { day: today, wares: roll_shelf(&denizen.person, today) };
        }
    }
}

pub struct ShelfPlugin;

impl Plugin for ShelfPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, stock_shelves.in_set(DelveSystems));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_faction::{Profession, Style, Tier, Trade};
    use fb_level::FolkSpot;

    /// A master mailwright standing at a first-level spot of 1207.
    fn mailwright() -> Person {
        let spot = fb_level::generate(1207, 1).folk[0];
        let trade = Trade { profession: Profession::Armorer, tier: Tier::Master, style: Some(Style::Mail) };
        crate::folk::person_1207(&FolkSpot { trade: Some(trade), ..spot })
    }

    #[test]
    fn a_shelf_restocks_on_a_new_day_and_not_before() {
        let person = mailwright();
        let titles = |day| roll_shelf(&person, day).iter().map(|i| (i.title().to_string(), i.seed)).collect::<Vec<_>>();
        assert!(!titles(0).is_empty());
        assert_eq!(titles(0), titles(0));
        assert_ne!(titles(0), titles(1));
        let mut clock = GameClock::default();
        assert_eq!(day_of(&clock), 0);
        clock.advance(86_399.0);
        assert_eq!(day_of(&clock), 0);
        clock.advance(2.0);
        assert_eq!(day_of(&clock), 1);
    }

    #[test]
    fn a_revisit_shows_the_recorded_shelf_until_the_day_turns() {
        use crate::record::{LevelRecord, ShelfRecord, item_record};
        let mut person = mailwright();
        let level = fb_level::generate(1207, 1);
        person.spot = level.folk[0];
        person.spot.trade = Some(Trade { profession: Profession::Armorer, tier: Tier::Master, style: Some(Style::Mail) });
        let mut level = level;
        level.folk[0] = person.spot;
        // What the delver left on the shelf: one dagger, not what the roll would be.
        let kept = fb_items::roll_item("Dagger", 3).unwrap();
        let mut record = WorldRecord::default();
        let mut left = LevelRecord::default();
        left.shelves.insert(0, ShelfRecord { day: 0, wares: vec![item_record(&kept)] });
        record.levels.insert(level.depth, left);
        let mut app = App::new();
        app.insert_resource(CurrentLevel(level)).insert_resource(record).insert_resource(GameClock::default()).add_systems(Update, stock_shelves);
        let folk = app.world_mut().spawn(Denizen { person, radius: 1.0, min: Vec3::ZERO, max: Vec3::ONE }).id();
        app.update();
        assert_eq!(app.world().get::<Shelf>(folk).unwrap().wares, vec![kept]);
        app.world_mut().resource_mut::<GameClock>().advance(86_400.0);
        app.update();
        let shelf = app.world().get::<Shelf>(folk).unwrap();
        assert_eq!(shelf.day, 1);
        assert!(shelf.wares.len() > 1 && shelf.wares.iter().all(|i| i.history.len() == 1));
    }

    #[test]
    fn the_shelf_carries_the_makers_line_with_the_traders_name_and_faction() {
        let person = mailwright();
        let wares = roll_shelf(&person, 0);
        let line = format!("Made by {}, master mailwright of {}.", person.name, person.faction.name);
        assert!(wares.iter().all(|i| i.history == vec![line.clone()]), "{line}");
    }

    #[test]
    fn folk_without_a_trade_or_who_sell_nothing_have_no_shelf() {
        let mut person = mailwright();
        person.spot.trade = None;
        assert!(roll_shelf(&person, 0).is_empty());
        person.spot.trade = Some(Trade::guard());
        assert!(roll_shelf(&person, 0).is_empty());
    }
}
