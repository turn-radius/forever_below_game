use super::*;
use crate::folk::Person;
use crate::game_clock::GameClock;
use crate::play::CurrentLevel;
use crate::inventory::Toast;
use crate::keymap::Keymap;
use crate::record::{LevelRecord, ShelfRecord, item_record};
use fb_faction::{Profession, Style};
use fb_items::ItemParts;
use fb_level::FolkSpot;

fn smith() -> Option<Trade> {
    Some(Trade { profession: Profession::Weaponsmith, tier: Tier::Apprentice, style: None })
}

fn armorer() -> Option<Trade> {
    Some(Trade { profession: Profession::Armorer, tier: Tier::Apprentice, style: Some(Style::Mail) })
}

fn person(trade: Option<Trade>) -> Person {
    let spot = fb_level::generate(1207, 1).folk[0];
    crate::folk::person_1207(&FolkSpot { trade, ..spot })
}

fn sword(seed: u32) -> Item {
    fb_items::roll_item("Sword", seed).unwrap()
}

/// A purse holding exactly `copper`, however the coins are split.
fn purse_of(coins: Coins) -> Item {
    let mut parts = ItemParts::from(fb_items::roll_item("Purse", 3).unwrap());
    parts.coins = Some(coins);
    Item::from_parts(parts).unwrap()
}

/// A delver with a pack on their back and `coins` in a purse in it.
fn delver(coins: Coins) -> Inventory {
    let mut inventory = Inventory::default();
    let pack = inventory.add(fb_items::roll_item("Pack", 1).unwrap());
    inventory.wear(pack, Slot::Back).unwrap();
    let purse = inventory.add(purse_of(coins));
    inventory.stow(purse).unwrap();
    inventory
}

fn shelf_of(wares: Vec<Item>, purse: u64) -> Shelf {
    Shelf { day: 0, wares, sold: vec![], purse }
}

/// The shelf's items, by their titles.
fn on_shelf(inventory: &Inventory, session: &Session) -> Vec<String> {
    inventory.grid(session.shelf).unwrap().entries().iter().filter_map(|e| inventory.item(e.id)).map(|i| i.title().to_string()).collect()
}

fn shelf_id(inventory: &Inventory, session: &Session, title: &str) -> ItemId {
    inventory.grid(session.shelf).unwrap().entries().iter().map(|e| e.id).find(|&id| inventory.item(id).unwrap().title() == title).expect("on the shelf")
}

#[test]
fn the_band_changes_what_stock_costs_and_what_the_trader_pays() {
    let item = sword(5);
    let trade = smith().unwrap();
    let value = item.value_copper() as f64;
    let ask = |band| price(&item, trade, band).unwrap();
    assert_eq!(ask(Band::Neutral), (value * 1.1 * 1.3).round() as u64);
    assert_eq!(ask(Band::Friendly), (value * 1.1).round() as u64);
    assert_eq!(ask(Band::Allied), (value * 1.1 * 0.85).round() as u64);
    assert!(ask(Band::Neutral) > ask(Band::Friendly) && ask(Band::Friendly) > ask(Band::Allied));
    assert_eq!(price(&item, trade, Band::Wary), None);
    assert_eq!(price(&item, trade, Band::Hostile), None);
    let pays = |band| offer(&item, trade, band).unwrap();
    assert_eq!(pays(Band::Neutral), (value * 0.6 * 0.8).round() as u64);
    assert!(pays(Band::Neutral) < pays(Band::Friendly) && pays(Band::Friendly) < pays(Band::Allied));
    // Never a profit in buying and selling the same thing back and forth.
    for band in [Band::Neutral, Band::Friendly, Band::Allied] {
        assert!(pays(band) < ask(band));
    }
    // A better tier asks more.
    let master = Trade { tier: Tier::Master, ..trade };
    assert!(price(&item, master, Band::Neutral).unwrap() > ask(Band::Neutral));
}

#[test]
fn goods_of_the_trade_fetch_twice_what_anything_else_does() {
    let item = sword(5);
    let own = offer(&item, smith().unwrap(), Band::Friendly).unwrap();
    let other = offer(&item, armorer().unwrap(), Band::Friendly).unwrap();
    assert!(is_their_trade(smith().unwrap(), &item) && !is_their_trade(armorer().unwrap(), &item));
    assert!(own.abs_diff(other * 2) <= 1, "{own} {other}");
}

#[test]
fn a_deal_moves_the_wares_and_the_coin_together() {
    let person = person(smith());
    let ware = sword(5);
    let cost = price(&ware, smith().unwrap(), Band::Neutral).unwrap();
    let mut inventory = delver(Coins::from_copper(cost + 700));
    let mut shelf = shelf_of(vec![ware.clone()], 6000);
    let mut session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    let id = shelf_id(&inventory, &session, ware.title());
    assert_eq!(session.tag(&inventory, id), Some(Price::Costs { price: Coins::from_copper(cost), worth: ware.value(), afford: true }));
    assert_eq!(session.tag(&inventory, id).unwrap().line(), format!("Costs {} \u{b7} worth {}", Coins::from_copper(cost).short(), ware.value().short()));
    // Nothing is bought until it is in the pack.
    assert!(session.deal(&inventory).is_empty());
    inventory.stow(id).unwrap();
    let deal = session.deal(&inventory);
    assert_eq!((deal.buying.clone(), deal.cost, deal.balance()), (vec![id], cost, -(cost as i64)));
    let done = settle(&mut inventory, &mut session, &mut shelf).unwrap();
    assert_eq!(done, Receipt { bought: 1, sold: 0, balance: -(cost as i64) });
    assert_eq!(wallet(&inventory).total_copper(), 700);
    assert!(inventory.is_carried(id));
    assert!(shelf.wares.is_empty() && shelf.sold.is_empty());
    assert_eq!(shelf.purse, 6000 + cost);
    // Leaving now takes the shelf down and keeps the purchase.
    end(&mut inventory, session);
    assert!(inventory.is_carried(id) && inventory.item(id).is_some());
}

#[test]
fn selling_pays_into_the_coin_purse_and_the_trader_runs_down() {
    let person = person(smith());
    let mine = sword(9);
    let pays = offer(&mine, smith().unwrap(), Band::Neutral).unwrap();
    let mut inventory = delver(Coins::from_copper(50));
    let id = inventory.add(mine.clone());
    inventory.stow(id).unwrap();
    let mut shelf = shelf_of(vec![sword(5)], 6000);
    let mut session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    inventory.shelve(id, session.shelf).unwrap();
    assert_eq!(session.deal(&inventory).offer, pays);
    assert_eq!(session.tag(&inventory, id), Some(Price::SellsFor { price: Coins::from_copper(pays), worth: mine.value() }));
    settle(&mut inventory, &mut session, &mut shelf).unwrap();
    assert_eq!(wallet(&inventory).total_copper(), 50 + pays);
    // Into the delver's coin purse; the purse they carry is as it was.
    assert_eq!(inventory.coins().total_copper(), pays);
    assert_eq!(shelf.purse, 6000 - pays);
    assert_eq!(shelf.sold, vec![mine]);
    assert!(!inventory.is_carried(id));
}

#[test]
fn a_sale_with_no_purse_carried_adds_no_item() {
    let person = person(smith());
    let mine = sword(9);
    let mut inventory = Inventory::default();
    let pack = inventory.add(fb_items::roll_item("Pack", 1).unwrap());
    inventory.wear(pack, Slot::Back).unwrap();
    let id = inventory.add(mine);
    inventory.stow(id).unwrap();
    let mut shelf = shelf_of(vec![], 6000);
    let mut session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    inventory.shelve(id, session.shelf).unwrap();
    let pays = session.deal(&inventory).offer;
    assert_eq!(wallet(&inventory).total_copper(), 0);
    settle(&mut inventory, &mut session, &mut shelf).unwrap();
    assert_eq!(wallet(&inventory).total_copper(), pays);
    assert_eq!(inventory.coins().total_copper(), pays);
    assert_eq!(inventory.ids().filter(|&i| inventory.item(i).is_some_and(|x| x.coins.is_some())).count(), 0, "no purse made");
}

#[test]
fn buying_takes_from_the_coin_purse_before_a_purse_carried() {
    let person = person(smith());
    let ware = sword(5);
    let cost = price(&ware, smith().unwrap(), Band::Neutral).unwrap();
    let mut inventory = delver(Coins::from_copper(cost));
    inventory.set_coins(Coins::from_copper(cost - 1));
    let mut shelf = shelf_of(vec![ware], 6000);
    let mut session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    let id = shelf_id(&inventory, &session, &session_title(&inventory, &session));
    inventory.stow(id).unwrap();
    settle(&mut inventory, &mut session, &mut shelf).unwrap();
    assert_eq!(inventory.coins(), Coins::default());
    let purse = inventory.ids().find(|&i| inventory.item(i).is_some_and(|x| x.coins.is_some())).unwrap();
    assert_eq!(inventory.item(purse).unwrap().coins.unwrap().total_copper(), cost - 1, "one copper came from the purse");
}

#[test]
fn short_coin_refuses_and_nothing_moves() {
    let person = person(smith());
    let ware = sword(5);
    let cost = price(&ware, smith().unwrap(), Band::Neutral).unwrap();
    let mut inventory = delver(Coins::from_copper(cost - 1));
    let mut shelf = shelf_of(vec![ware], 6000);
    let mut session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    let id = shelf_id(&inventory, &session, &session_title(&inventory, &session));
    assert!(matches!(session.tag(&inventory, id), Some(Price::Costs { afford: false, .. })), "a ware out of reach is marked so");
    inventory.stow(id).unwrap();
    let before = shelf.clone();
    let refused = settle(&mut inventory, &mut session, &mut shelf).unwrap_err();
    assert!(refused.starts_with("You are 1 cp short"), "{refused}");
    assert_eq!(wallet(&inventory).total_copper(), cost - 1);
    assert_eq!(shelf, before);
    // The ware is still only in the pack, to go back by Cancel.
    assert_eq!(session.deal(&inventory).buying, vec![id]);
}

fn session_title(inventory: &Inventory, session: &Session) -> String {
    on_shelf(inventory, session).remove(0)
}

#[test]
fn a_trader_who_cannot_afford_it_refuses_and_nothing_moves() {
    let person = person(smith());
    let mine = sword(9);
    let pays = offer(&mine, smith().unwrap(), Band::Neutral).unwrap();
    let mut inventory = delver(Coins::from_copper(0));
    let id = inventory.add(mine);
    inventory.stow(id).unwrap();
    let mut shelf = shelf_of(vec![], pays - 1);
    let mut session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    inventory.shelve(id, session.shelf).unwrap();
    let before = shelf.clone();
    let refused = settle(&mut inventory, &mut session, &mut shelf).unwrap_err();
    assert!(refused.contains("cannot afford"), "{refused}");
    assert_eq!((wallet(&inventory).total_copper(), shelf), (0, before));
}

#[test]
fn coins_are_compared_by_their_total_in_copper() {
    let person = person(smith());
    let ware = sword(5);
    let cost = price(&ware, smith().unwrap(), Band::Neutral).unwrap();
    // The exact sum, in coins no purse would make: field by field it is not `from_copper`'s.
    let odd = Coins { gp: 0, sp: (cost / 10) as u32, cp: (cost % 10) as u32 };
    assert_ne!(odd, Coins::from_copper(cost));
    let mut inventory = delver(odd);
    let mut shelf = shelf_of(vec![ware], 6000);
    let mut session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    let id = shelf_id(&inventory, &session, &session_title(&inventory, &session));
    inventory.stow(id).unwrap();
    assert!(settle(&mut inventory, &mut session, &mut shelf).is_ok());
    assert_eq!(wallet(&inventory).total_copper(), 0);
    // And the trader's side: a purse of exactly what they owe is enough.
    let mine = sword(9);
    let pays = offer(&mine, smith().unwrap(), Band::Neutral).unwrap();
    let id = inventory.add(mine);
    inventory.stow(id).unwrap();
    shelf.purse = pays;
    inventory.shelve(id, session.shelf).unwrap();
    assert!(settle(&mut inventory, &mut session, &mut shelf).is_ok());
    assert_eq!(shelf.purse, 0);
}

#[test]
fn cancelling_puts_everything_back() {
    let person = person(smith());
    let mut inventory = delver(Coins::from_copper(5000));
    let mine = inventory.add(sword(9));
    inventory.stow(mine).unwrap();
    let shelf = shelf_of(vec![sword(5)], 6000);
    let before = (inventory.ids().count(), wallet(&inventory));
    let session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    let ware = shelf_id(&inventory, &session, &session_title(&inventory, &session));
    let mut barter = Barter { session: Some(session), ..Default::default() };
    assert!(barter.quick(&mut inventory, ware).unwrap().is_ok());
    assert!(barter.quick(&mut inventory, mine).unwrap().is_ok());
    assert_eq!(barter.session.as_ref().unwrap().deal(&inventory).buying, vec![ware]);
    end(&mut inventory, barter.session.take().unwrap());
    assert_eq!((inventory.ids().count(), wallet(&inventory)), before);
    assert!(inventory.is_carried(mine));
}

#[test]
fn dropped_wares_must_stay_in_the_pack_or_the_hands() {
    let person = person(smith());
    let mut inventory = delver(Coins::from_copper(5000));
    let shelf = shelf_of(vec![sword(5)], 6000);
    let session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    let id = shelf_id(&inventory, &session, &session_title(&inventory, &session));
    let size = inventory.footprint(id).unwrap();
    let drag = Drag { id, grab: IVec2::ZERO, size, turned: false, press: Vec2::ZERO, active: true, target: None };
    let barter = Barter { session: Some(session), ..Default::default() };
    for target in [Target::Level, Target::Cell { view: GridView::Underfoot, cell: IVec2::ZERO }] {
        assert_eq!(barter.check(&inventory, &drag, target), Some(Err(Refusal::Bartering)));
    }
    // A purse is no ware, and a full bag is not sold full.
    let purse = inventory.ids().find(|&i| inventory.item(i).is_some_and(|x| x.coins.is_some())).unwrap();
    let coin = Drag { id: purse, ..drag };
    let shelf_cell = Target::Cell { view: GridView::Container(barter.session.as_ref().unwrap().shelf), cell: IVec2::new(5, 5) };
    assert_eq!(barter.check(&inventory, &coin, shelf_cell), Some(Err(Refusal::NotAWare)));
}

#[test]
fn a_sold_sword_stays_on_the_shelf_through_a_restock() {
    let person = person(smith());
    let mine = sword(9);
    let shelf = Shelf { day: 0, wares: roll_wares(&person, 0), sold: vec![mine.clone()], purse: 100 };
    let next = shelf.restocked(&person, 1);
    assert_eq!(next.day, 1);
    assert_eq!(next.sold, vec![mine]);
    assert_ne!(next.wares, roll_wares(&person, 0));
    // The purse is topped up to the tier's, never cut.
    assert_eq!(next.purse, purse_copper(Tier::Apprentice));
    assert_eq!(Shelf { purse: 99_999, ..next }.restocked(&person, 2).purse, 99_999);
}

fn roll_wares(person: &Person, day: u32) -> Vec<Item> {
    crate::shelf::roll_shelf(person, day)
}

#[test]
fn a_sold_sword_is_still_there_after_leaving_and_returning_even_after_the_day_turns() {
    let mut person = person(smith());
    let mut level = fb_level::generate(1207, 1);
    person.spot = level.folk[0];
    person.spot.trade = smith();
    level.folk[0] = person.spot;
    let mine = sword(9);
    let left = Shelf { day: 0, wares: roll_wares(&person, 0), sold: vec![mine.clone()], purse: 1234 };
    let mut record = WorldRecord::default();
    let mut recorded = LevelRecord::default();
    recorded.shelves.insert(
        0,
        ShelfRecord {
            day: 0,
            wares: left.wares.iter().map(item_record).collect(),
            sold: left.sold.iter().map(item_record).collect(),
            purse: Some(left.purse),
        },
    );
    record.levels.insert(level.depth, recorded);
    let mut app = App::new();
    app.insert_resource(CurrentLevel(level)).insert_resource(record).insert_resource(GameClock::default()).add_systems(Update, crate::shelf::stock_shelves);
    let folk = app.world_mut().spawn(Denizen { person, radius: 1.0, min: Vec3::ZERO, max: Vec3::ONE }).id();
    app.update();
    let back = app.world().get::<Shelf>(folk).unwrap().clone();
    assert_eq!((back.wares.clone(), back.sold.clone(), back.purse), (left.wares, vec![mine.clone()], 1234));
    app.world_mut().resource_mut::<GameClock>().advance(86_400.0);
    app.update();
    let next = app.world().get::<Shelf>(folk).unwrap();
    assert_eq!((next.day, next.sold.clone()), (1, vec![mine]));
}

/// An app with the barter, the talk's message, and the level's first folk standing in.
fn barter_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin, bevy::state::app::StatesPlugin))
        .insert_state(crate::launch::AppState::Delving)
        .init_resource::<Keymap>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ScreenOpen>()
        .init_resource::<Toast>()
        .init_resource::<WorldRecord>()
        .init_resource::<AimedFolk>()
        .init_resource::<Inventory>()
        .init_resource::<OpenContainer>()
        .add_message::<ServiceChosen>()
        .insert_resource(CurrentLevel(fb_level::generate(1207, 1)));
    crate::play::run_delve_systems_while_delving(&mut app);
    app.add_plugins(BarterPlugin);
    app.update();
    app
}

fn stand_in(app: &mut App, trade: Option<Trade>) -> Entity {
    let person = person(trade);
    let shelf = trade.is_some_and(|t| t.shop("", "").is_some()).then(|| shelf_of(crate::shelf::roll_shelf(&person, 0), 6000));
    let denizen = Denizen { person, radius: 3.0, min: Vec3::new(-2.0, 0.0, -2.0), max: Vec3::new(2.0, 17.0, 2.0) };
    let mut entity = app.world_mut().spawn(denizen);
    if let Some(shelf) = shelf {
        entity.insert(shelf);
    }
    let id = entity.id();
    app.update();
    id
}

fn press_barter(app: &mut App, folk: Entity) {
    app.world_mut().resource_mut::<AimedFolk>().0 = Some(folk);
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyG);
    app.update();
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.release(KeyCode::KeyG);
    keys.clear();
    app.update();
}

fn toast(app: &App) -> String {
    app.world().resource::<Toast>().text.clone()
}

fn set_standing(app: &mut App, folk: Entity, amount: i32) {
    let faction = app.world().get::<Denizen>(folk).unwrap().person.spot.faction;
    app.world_mut().resource_mut::<WorldRecord>().standings.shift(faction, amount);
}

#[test]
fn barter_on_a_hostile_folk_refuses_with_the_factions_name() {
    let mut app = barter_app();
    let folk = stand_in(&mut app, smith());
    set_standing(&mut app, folk, -100);
    press_barter(&mut app, folk);
    let faction = app.world().get::<Denizen>(folk).unwrap().person.faction.name.clone();
    assert_eq!(toast(&app), format!("{} won't trade with you.", capital(&faction)));
    assert_eq!(app.world().resource::<ScreenOpen>().0, None);
    assert!(!app.world().resource::<Barter>().active());
    // A Wary one too.
    let mut app = barter_app();
    let folk = stand_in(&mut app, smith());
    set_standing(&mut app, folk, -30);
    press_barter(&mut app, folk);
    assert!(toast(&app).ends_with("won't trade with you."));
    assert_eq!(app.world().resource::<ScreenOpen>().0, None);
}

#[test]
fn barter_on_a_guard_or_plain_folk_has_nothing_to_sell() {
    for trade in [Some(Trade::guard()), None, Some(Trade { profession: Profession::Enchanter, tier: Tier::Expert, style: None })] {
        let mut app = barter_app();
        let folk = stand_in(&mut app, trade);
        press_barter(&mut app, folk);
        let name = app.world().get::<Denizen>(folk).unwrap().person.name.clone();
        assert_eq!(toast(&app), format!("{name} has nothing to sell."), "{trade:?}");
        assert_eq!(app.world().resource::<ScreenOpen>().0, None);
    }
}

#[test]
fn barter_on_a_trader_opens_the_screen_and_leaving_puts_everything_back() {
    let mut app = barter_app();
    let folk = stand_in(&mut app, smith());
    let before = app.world().resource::<Inventory>().ids().count();
    press_barter(&mut app, folk);
    assert!(app.world().resource::<ScreenOpen>().is(Screen::Inventory));
    let shelf = app.world().resource::<Barter>().session.as_ref().expect("open").shelf;
    assert_eq!(app.world().resource::<OpenContainer>().0, Some(shelf));
    assert!(app.world().resource::<Inventory>().contents_count(shelf) > 0);
    // Take a ware to a pack that is not there: the deal stays empty. Then leave.
    app.world_mut().resource_mut::<ScreenOpen>().0 = None;
    app.update();
    assert!(!app.world().resource::<Barter>().active());
    assert_eq!(app.world().resource::<OpenContainer>().0, None);
    assert_eq!(app.world().resource::<Inventory>().ids().count(), before);
}

#[test]
fn the_talks_barter_topic_opens_the_screen() {
    let mut app = barter_app();
    let folk = stand_in(&mut app, smith());
    app.world_mut().write_message(ServiceChosen { folk, service: Service::Barter });
    app.update();
    assert!(app.world().resource::<ScreenOpen>().is(Screen::Inventory));
    assert!(app.world().resource::<Barter>().active());
}

#[test]
fn a_trade_moves_the_standing_a_little_and_meets_the_faction() {
    let mut standings = fb_faction::Standings::default();
    let faction = Faction(2);
    assert!(!standings.has_met(faction));
    let after = standings.on_trade_with(&StandingRules::default(), faction);
    assert!(after.value() > 0 && after.value() <= 5);
    assert!(standings.has_met(faction));
}

#[test]
fn the_refusal_line_names_the_faction_with_a_capital() {
    assert_eq!(refusal_line("the Mudfoot clan", Band::Hostile, false).as_deref(), Some("The Mudfoot clan won't trade with you."));
    assert_eq!(refusal_line("the Mudfoot clan", Band::Wary, false).as_deref(), Some("The Mudfoot clan won't trade with you."));
    assert_eq!(refusal_line("the Mudfoot clan", Band::Allied, true).as_deref(), Some("The Mudfoot clan won't trade with you."));
    assert_eq!(refusal_line("the Mudfoot clan", Band::Neutral, false), None);
}

#[test]
fn a_shelf_too_crowded_to_lay_out_refuses_and_adds_nothing() {
    let person = person(smith());
    let mut inventory = delver(Coins::from_copper(10));
    let before = inventory.ids().count();
    let sold: Vec<Item> = (0..600).map(|i| fb_items::roll_item("Dagger", i).unwrap()).collect();
    let shelf = Shelf { day: 0, wares: vec![sword(5)], sold, purse: 100 };
    let refused = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap_err();
    assert!(refused.contains("too crowded"), "{refused}");
    assert_eq!(inventory.ids().count(), before);
    // A shelf that merely needs a tall grid is laid out whole.
    let some: Vec<Item> = (0..40).map(|i| fb_items::roll_item("Dagger", i).unwrap()).collect();
    let shelf = Shelf { day: 0, wares: vec![], sold: some, purse: 100 };
    let session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    assert_eq!(inventory.contents_count(session.shelf), 40);
}

#[test]
fn cancelling_leaves_unrelated_changes_alone_and_never_reuses_ids() {
    let person = person(smith());
    let mut inventory = delver(Coins::from_copper(5000));
    let mine = inventory.add(sword(9));
    inventory.stow(mine).unwrap();
    let shelf = shelf_of(vec![sword(5)], 6000);
    let session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    let mut barter = Barter { session: Some(session), ..Default::default() };
    let session_shelf = barter.session.as_ref().unwrap().shelf;
    let ware = shelf_id(&inventory, barter.session.as_ref().unwrap(), &session_title(&inventory, barter.session.as_ref().unwrap()));
    // The deal moves two things, through the ordinary drag paths.
    assert!(barter.quick(&mut inventory, ware).unwrap().is_ok());
    assert!(barter.quick(&mut inventory, mine).unwrap().is_ok());
    // The world goes on: a torch picked up and an apple eaten, nothing to do with the deal.
    let torch = inventory.add(fb_items::roll_item("Torch", 3).unwrap());
    inventory.stow(torch).unwrap();
    let apple = inventory.add(fb_items::roll_item("Apple", 3).unwrap());
    inventory.stow(apple).unwrap();
    inventory.forget(apple);
    let next_before = inventory.add(fb_items::roll_item("Rope", 3).unwrap());
    inventory.forget(next_before);
    end(&mut inventory, barter.session.take().unwrap());
    assert!(inventory.is_carried(torch), "the unrelated torch survived");
    assert!(inventory.item(apple).is_none());
    assert!(inventory.is_carried(mine) && inventory.item(ware).is_none(), "the sword is back with the delver and the ware is gone with the shelf");
    assert!(inventory.item(session_shelf).is_none());
    // No id is handed out twice, even those of what was taken down.
    let fresh = inventory.add(fb_items::roll_item("Rope", 4).unwrap());
    assert!(fresh.0 > next_before.0 && fresh != session_shelf && fresh != ware);
}

#[test]
fn leaving_the_delve_takes_the_shelf_down() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = barter_app();
    let folk = stand_in(&mut app, smith());
    let before = app.world().resource::<Inventory>().ids().count();
    press_barter(&mut app, folk);
    assert!(app.world().resource::<Barter>().active());
    assert!(app.world().resource::<Inventory>().ids().count() > before);
    app.world_mut().run_system_once(leave_the_delve).unwrap();
    assert!(!app.world().resource::<Barter>().active());
    assert_eq!(app.world().resource::<OpenContainer>().0, None);
    assert_eq!(app.world().resource::<Inventory>().ids().count(), before);
}

#[test]
fn a_trade_moves_standing_once_per_folk_per_day() {
    let mut barter = Barter::default();
    let mut standings = fb_faction::Standings::default();
    let faction = Faction(1);
    assert!(barter.trade_standing(&mut standings, 77, faction, 0));
    let after = standings.get(faction);
    for _ in 0..20 {
        assert!(!barter.trade_standing(&mut standings, 77, faction, 0), "a small deal again the same day");
    }
    assert_eq!(standings.get(faction), after);
    // The next day, or another folk, counts.
    assert!(barter.trade_standing(&mut standings, 77, faction, 1));
    assert!(barter.trade_standing(&mut standings, 78, faction, 1));
    assert_eq!(standings.get(faction).value(), after.value() * 3);
}

#[test]
fn the_portrait_is_the_look_of_the_trader_s_own_person() {
    let person = person(smith());
    let mut inventory = delver(Coins::from_copper(100));
    let session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf_of(vec![sword(1)], 50)).unwrap();
    assert_eq!(session.look, Look::of(&person));
    assert_eq!(session.look.spec, person.spec);
    assert_eq!(session.look.gear, person.gear);
    assert_eq!(session.look.dirt, person.dirt);
    // The same figure the world dresses them in.
    assert_eq!(session.look.key(), crate::fitting::figure_key(&person.spec, &person.gear, person.dirt));
}

#[test]
fn two_folk_of_one_faction_look_different() {
    let level = fb_level::generate(1207, 1);
    let looks: Vec<_> = level.folk.iter().map(|spot| (spot.faction, Look::of(&Person::of(spot, level.faction(spot.faction), 1)).key())).collect();
    let mut seen = 0;
    for (i, (faction, key)) in looks.iter().enumerate() {
        for (other, other_key) in &looks[i + 1..] {
            if other == faction {
                seen += 1;
                assert_ne!(key, other_key, "two folk of faction {faction:?} share a look");
            }
        }
    }
    assert!(seen > 0, "no two folk of one faction in the level");
}

#[test]
fn the_portrait_texture_is_let_go_with_the_barter() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = barter_app();
    app.init_resource::<Assets<Image>>().init_resource::<portrait::TraderStage>();
    let image = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());
    app.world_mut().resource_mut::<Barter>().portrait = Some(image);
    app.world_mut().run_system_once(portrait::clear_trader).unwrap();
    assert!(app.world().resource::<Barter>().portrait.is_none());
}

#[test]
fn leaving_the_delve_forgets_the_trader_s_stage() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = barter_app();
    app.world_mut().resource_mut::<portrait::TraderStage>().mark_dressed_for_test("a trader");
    app.world_mut().run_system_once(leave_the_delve).unwrap();
    assert!(!app.world().resource::<portrait::TraderStage>().is_dressed(), "a later barter dresses the trader afresh");
}

#[test]
fn an_unknown_or_partly_known_item_fetches_what_a_plain_one_does() {
    let ring = (1..20_000).filter_map(|s| fb_items::roll_item_with("Ring", s, fb_items::Odds::loot(10))).find(|i| i.rarity >= fb_items::Rarity::Rare && i.affixes.len() >= 2).unwrap();
    let plain = Item { rarity: fb_items::Rarity::Common, affixes: vec![], ..ring.clone() };
    let trade = smith().unwrap();
    let known = offer(&ring, trade, Band::Neutral).unwrap();
    let mut unknown = ring.clone().unidentified();
    assert_eq!(offer(&unknown, trade, Band::Neutral), offer(&plain, trade, Band::Neutral));
    assert!(known > offer(&unknown, trade, Band::Neutral).unwrap());
    unknown.reveal_at(unknown.identify_dc() - 3, 0);
    assert!(!unknown.is_known());
    assert_eq!(offer(&unknown, trade, Band::Neutral), offer(&plain, trade, Band::Neutral));
    assert_eq!(price(&unknown, trade, Band::Neutral), price(&plain, trade, Band::Neutral));
}

fn arrows(n: u32) -> Item {
    fb_items::roll_item("Arrow", 7).unwrap().with_quantity(n)
}

fn arrow_pieces(inventory: &Inventory, container: ItemId) -> Vec<u32> {
    let mut counts: Vec<u32> = inventory.grid(container).unwrap().entries().iter().filter_map(|e| inventory.item(e.id)).filter(|i| i.base == "Arrow").map(|i| i.quantity).collect();
    counts.sort();
    counts
}

#[test]
fn a_stack_costs_and_sells_by_the_piece() {
    let trade = smith().unwrap();
    let (one, six) = (arrows(1), arrows(6));
    assert_eq!(price(&six, trade, Band::Neutral).unwrap(), (six.value_copper() as f64 * 6.0 * 1.1 * 1.3).round() as u64);
    assert!(offer(&six, trade, Band::Neutral).unwrap() > offer(&one, trade, Band::Neutral).unwrap());
    assert_eq!(six.stack_value_copper(), one.value_copper() * 6);
}

#[test]
fn a_bought_stack_is_not_folded_during_the_deal_so_cancel_finds_it() {
    let person = person(smith());
    let mut inventory = delver(Coins::from_copper(100_000));
    let pack = inventory.carried_containers()[0];
    let mine = inventory.add(arrows(8));
    inventory.stow(mine).unwrap();
    let shelf = shelf_of(vec![arrows(5)], 6000);
    let session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    let ware = inventory.grid(session.shelf).unwrap().entries()[0].id;
    let shelf_id = session.shelf;
    let mut barter = Barter { session: Some(session), ..Default::default() };
    assert!(barter.quick(&mut inventory, ware).unwrap().is_ok());
    assert!(inventory.item(ware).is_some());
    assert_eq!(arrow_pieces(&inventory, pack), vec![5, 8], "side by side, none folded");
    end(&mut inventory, barter.session.take().unwrap());
    assert_eq!(arrow_pieces(&inventory, pack), vec![8], "the pack is as it was, with no free arrows");
    assert!(inventory.item(shelf_id).is_none());
}

#[test]
fn settling_folds_what_was_bought_into_the_stacks_carried() {
    let person = person(smith());
    let mut inventory = delver(Coins::from_copper(100_000));
    let pack = inventory.carried_containers()[0];
    let mine = inventory.add(arrows(8));
    inventory.stow(mine).unwrap();
    let mut shelf = shelf_of(vec![arrows(5)], 6000);
    let mut session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    let ware = inventory.grid(session.shelf).unwrap().entries()[0].id;
    inventory.stow_apart(ware).unwrap();
    settle(&mut inventory, &mut session, &mut shelf).unwrap();
    assert_eq!(arrow_pieces(&inventory, pack), vec![3, 10]);
}

/// The drag of `id` for the barter checks.
fn drag_of(inventory: &Inventory, id: ItemId) -> Drag {
    Drag { id, grab: IVec2::ZERO, size: inventory.footprint(id).unwrap(), turned: false, press: Vec2::ZERO, active: true, target: None }
}

#[test]
fn a_pack_with_a_tent_tied_on_or_a_spear_strapped_is_not_sold_until_it_is_emptied() {
    use fb_items::Accessory::{Lashing, WeaponStrap};
    let person = person(smith());
    let mut inventory = delver(Coins::from_copper(0));
    let pack = (1..2000).filter_map(|s| fb_items::roll_item("Pack", s)).find(|p| p.has_accessory(Lashing) && p.has_accessory(WeaponStrap)).unwrap();
    let pack = inventory.add(pack);
    let _ = inventory.equip(pack, Slot::Back);
    let tent = inventory.add(fb_items::roll_item("Tent", 3).unwrap());
    let spear = inventory.add(fb_items::roll_item("Spear", 3).unwrap());
    inventory.lash(tent).unwrap();
    let strap = inventory.fitting(pack, WeaponStrap).unwrap();
    inventory.put(spear, strap, Placement::new(0, 0, false)).unwrap();
    let shelf = shelf_of(vec![], 6000);
    let session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    let on_shelf = Target::Cell { view: GridView::Container(session.shelf), cell: IVec2::ZERO };
    let barter = Barter { session: Some(session), ..Default::default() };
    let drag = drag_of(&inventory, pack);
    assert_eq!(barter.check(&inventory, &drag, on_shelf), Some(Err(Refusal::HoldsThings)), "what hangs on it keeps it off the shelf");
    // The tent and spear stay tied and strapped, in reach of the deal.
    assert_eq!(inventory.hung_on(pack).len(), 2);
    // Emptied, it is a ware like any pack.
    inventory.take_out(tent);
    inventory.take_out(spear);
    assert!(matches!(barter.check(&inventory, &drag, on_shelf), Some(Ok(_))));
}

#[test]
fn a_belt_with_a_framed_sword_or_a_pouch_potion_is_not_sold_until_it_is_emptied() {
    use fb_items::Accessory::{LeftFrog, PotionPouch};
    let person = person(smith());
    let mut inventory = delver(Coins::from_copper(0));
    let belt = (1..3000).filter_map(|s| fb_items::roll_item("Belt", s)).find(|b| b.has_accessory(LeftFrog) && b.has_accessory(PotionPouch)).unwrap();
    let belt = inventory.add(belt);
    let _ = inventory.equip(belt, Slot::Belt);
    let (sword, vial) = (inventory.add(fb_items::roll_item("Sword", 3).unwrap()), inventory.add(fb_items::roll_item("Vial", 3).unwrap()));
    let (frog, pouch) = (inventory.fitting(belt, LeftFrog).unwrap(), inventory.fitting(belt, PotionPouch).unwrap());
    inventory.put(sword, frog, Placement::new(0, 0, false)).unwrap();
    inventory.put(vial, pouch, Placement::new(0, 0, false)).unwrap();
    let shelf = shelf_of(vec![], 6000);
    let session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    let on_shelf = Target::Cell { view: GridView::Container(session.shelf), cell: IVec2::ZERO };
    let barter = Barter { session: Some(session), ..Default::default() };
    assert_eq!(barter.check(&inventory, &drag_of(&inventory, belt), on_shelf), Some(Err(Refusal::HoldsThings)));
    // One of the two taken off is not enough.
    inventory.take_out(sword);
    assert_eq!(barter.check(&inventory, &drag_of(&inventory, belt), on_shelf), Some(Err(Refusal::HoldsThings)));
    // A sword can be drawn from its frog into the hand while bartering is going on, or moved between the pack's grids.
    assert!(inventory.is_carried(vial));
}

#[test]
fn cancelling_a_deal_that_moved_an_emptied_pack_puts_it_back_with_its_fittings() {
    use fb_items::Accessory::{Lashing, WeaponStrap};
    let person = person(smith());
    let mut inventory = delver(Coins::from_copper(0));
    let pack = (1..2000).filter_map(|s| fb_items::roll_item("Pack", s)).find(|p| p.has_accessory(Lashing) && p.has_accessory(WeaponStrap)).unwrap();
    let pack = inventory.add(pack);
    let _ = inventory.equip(pack, Slot::Back);
    let (lashing, strap) = (inventory.fitting(pack, Lashing).unwrap(), inventory.fitting(pack, WeaponStrap).unwrap());
    let shelf = shelf_of(vec![], 6000);
    let mut session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    session.note(&inventory, pack);
    inventory.shelve(pack, session.shelf).unwrap();
    assert_eq!(inventory.worn(Slot::Back), None);
    // On the shelf, it still has its grids, and a tent could be tied on only when it is carried again.
    assert_eq!((inventory.fitting(pack, Lashing), inventory.fitting(pack, WeaponStrap)), (Some(lashing), Some(strap)));
    end(&mut inventory, session);
    assert_eq!(inventory.worn(Slot::Back), Some(pack));
    let tent = inventory.add(fb_items::roll_item("Tent", 3).unwrap());
    assert_eq!(inventory.lash(tent), Ok(()));
}

#[test]
fn a_shelf_stack_cannot_be_split_into_the_pouch_mid_deal() {
    use fb_items::Accessory::{LeftFrog, PotionPouch};
    let person = person(smith());
    let mut inventory = delver(Coins::from_copper(5000));
    let belt = (1..3000).filter_map(|s| fb_items::roll_item("Belt", s)).find(|b| b.has_accessory(LeftFrog) && b.has_accessory(PotionPouch)).unwrap();
    let belt = inventory.add(belt);
    let _ = inventory.equip(belt, Slot::Belt);
    let pouch = inventory.fitting(belt, PotionPouch).unwrap();
    let mut vials = fb_items::roll_item("Vial", 3).unwrap();
    vials.quantity = 3;
    let shelf = shelf_of(vec![vials], 6000);
    let session = begin(&mut inventory, Entity::PLACEHOLDER, &person, Band::Neutral, &shelf).unwrap();
    let stack = *session.stock.iter().next().unwrap();
    let drag = drag_of(&inventory, stack);
    let barter = Barter { session: Some(session), ..Default::default() };
    let into = Target::Cell { view: GridView::Container(pouch), cell: IVec2::ZERO };
    assert_eq!(barter.check(&inventory, &drag, into), Some(Err(Refusal::NoRoom)), "a free piece is refused");
    let onto_slot = Target::Fitting { container: pouch, index: 0 };
    assert_eq!(barter.check(&inventory, &drag, onto_slot), Some(Err(Refusal::NoRoom)), "and so is a piece onto a pouch slot");
    assert_eq!(inventory.item(stack).unwrap().quantity, 3);
}

