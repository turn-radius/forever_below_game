//! What a tradesperson has to sell today: [`Shelf`], rolled by [`fb_items::stock::stock`] from
//! their spot's seed and the game day, so it restocks each game day (#548). A shelf left in
//! the record ([`crate::record::ShelfRecord`]) comes back the same on a revisit until the next
//! restock. The delver buys and sells at the shelf ([`crate::barter`]): what they sell stays on
//! it ([`Shelf::sold`]) through every restock, and only the rolled wares are rolled again.

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
    /// What the day's roll put out.
    pub wares: Vec<Item>,
    /// What the delver sold them, which stays until someone buys it.
    pub sold: Vec<Item>,
    /// The coin they have to buy with, in copper. It runs down as they buy and is topped up to
    /// their tier's purse at each restock ([`crate::barter::purse_copper`]).
    pub purse: u64,
}

impl Shelf {
    /// A new day's shelf: the rolled wares again, the sold-in ones kept, and the purse
    /// topped up (never cut) to what the trade's tier carries.
    pub fn restocked(self, person: &Person, day: u32, prayers: &[(String, f32)]) -> Shelf {
        Shelf { day, wares: roll_shelf(person, day, prayers), purse: self.purse.max(starting_purse(person)), sold: self.sold }
    }

    /// Every ware on the shelf, rolled ones first and then the sold-in.
    pub fn all(&self) -> impl Iterator<Item = &Item> {
        self.wares.iter().chain(&self.sold)
    }
}

/// The coin a trader begins with, by tier.
pub fn starting_purse(person: &Person) -> u64 {
    person.spot.trade.map_or(0, |t| crate::barter::purse_copper(t.tier))
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
/// nothing (guards, enchanters). Pure in the person, the day and the prayers a healer may sell
/// ([`crate::rules::prayer_offers`]).
pub fn roll_shelf(person: &Person, day: u32, prayers: &[(String, f32)]) -> Vec<Item> {
    let Some(shop) = person.spot.trade.and_then(|t| t.shop(&person.name, &person.faction.name)) else { return vec![] };
    fb_items::stock::stock_with(&shop, person.depth, shelf_seed(person.spot.seed, day), prayers)
}

/// Gives each tradesperson their shelf as they appear: the one the record kept if it is still
/// today's, else a fresh roll. Restocks any shelf whose day has passed.
pub fn stock_shelves(
    mut commands: Commands,
    clock: Option<Res<GameClock>>,
    level: Res<CurrentLevel>,
    record: Option<Res<WorldRecord>>,
    rules: Option<Res<crate::rules::GameRules>>,
    new: Query<(Entity, &Denizen), Without<Shelf>>,
    mut old: Query<(&Denizen, &mut Shelf)>,
) {
    let today = clock.as_deref().map_or(0, day_of);
    let prayers = crate::rules::prayer_offers(crate::rules::GameRules::of(&rules));
    for (entity, denizen) in &new {
        let person = &denizen.person;
        if !person.spot.trade.is_some_and(|t| t.shop("", "").is_some()) {
            continue;
        }
        // What the record kept, if any: today's shelf as it was left, or an older one, which
        // keeps what the delver sold it and rolls the rest again.
        let left = record.as_deref().and_then(|r| r.level(level.0.depth)).and_then(|l| {
            let index = level.0.folk.iter().position(|s| *s == person.spot)?;
            l.shelves.get(&index)
        });
        let shelf = match left {
            Some(saved) => {
                let was = Shelf {
                    day: saved.day,
                    wares: saved.wares.iter().filter_map(rebuild_item).collect(),
                    sold: saved.sold.iter().filter_map(rebuild_item).collect(),
                    purse: saved.purse.unwrap_or_else(|| starting_purse(person)),
                };
                if saved.day == today { was } else { was.restocked(person, today, &prayers) }
            }
            None => Shelf { day: today, wares: roll_shelf(person, today, &prayers), sold: vec![], purse: starting_purse(person) },
        };
        commands.entity(entity).insert(shelf);
    }
    for (denizen, mut shelf) in &mut old {
        if shelf.day != today {
            let old = std::mem::replace(&mut *shelf, Shelf { day: today, wares: vec![], sold: vec![], purse: 0 });
            *shelf = old.restocked(&denizen.person, today, &prayers);
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
        let titles = |day| roll_shelf(&person, day, &[]).iter().map(|i| (i.title().to_string(), i.seed)).collect::<Vec<_>>();
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
        left.shelves.insert(0, ShelfRecord { day: 0, wares: vec![item_record(&kept)], sold: vec![], purse: None });
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
        let wares = roll_shelf(&person, 0, &[]);
        let line = format!("Made by {}, master mailwright of {}.", person.name, person.faction.name);
        assert!(wares.iter().all(|i| i.history == vec![line.clone()]), "{line}");
    }

    #[test]
    fn folk_without_a_trade_or_who_sell_nothing_have_no_shelf() {
        let mut person = mailwright();
        person.spot.trade = None;
        assert!(roll_shelf(&person, 0, &[]).is_empty());
        person.spot.trade = Some(Trade::guard());
        assert!(roll_shelf(&person, 0, &[]).is_empty());
    }
}
