//! The delver's enchanting resources for one delve: what scrapping gear gave them
//! (`fb_items::scrap`), kept until the run ends. The enchanter's talk (#350) calls
//! [`scrap_item`] and spends the stock.

use crate::inventory::{Inventory, ItemId};
use bevy::prelude::*;
use fb_items::enchant::{self, Cost, Enchanter, Refusal, add_cost, remove_cost, reforge_cost};
use fb_items::scrap::{EnchantingStock, Salvage, scrap};
use fb_items::tables::Affix;
use fb_items::{Coins, Item, ItemParts};

/// The delver's essence and materials. Reset with the run (`travel::begin_the_run`).
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct Enchanting(pub EnchantingStock);

/// Scraps item `id` into `stock`: the item is gone and its salvage is added. `None`, and
/// nothing changes, for what is not gear or a bag with something in it.
pub fn scrap_item(inventory: &mut Inventory, stock: &mut EnchantingStock, id: ItemId) -> Option<Salvage> {
    if inventory.contents_count(id) > 0 {
        return None;
    }
    let salvage = scrap(inventory.item(id)?)?;
    inventory.forget(id);
    stock.add(&salvage);
    Some(salvage)
}

/// The coin the delver carries: the sum of the purses they have on them, in a pack or worn.
/// There is no wallet of its own yet, so coin lives in purses.
pub fn wallet(inventory: &Inventory) -> Coins {
    inventory.ids().filter(|&id| inventory.is_carried(id)).filter_map(|id| inventory.item(id)?.coins).sum()
}

/// What `copper` taken out of the delver's carried purses, the first ones first, would leave in
/// each: the purses as they would be, nothing changed yet. `None` when they carry less, or a
/// purse cannot be rebuilt.
fn plan_pay(inventory: &Inventory, copper: u64) -> Option<Vec<(ItemId, Item)>> {
    if wallet(inventory).total_copper() < copper {
        return None;
    }
    let mut owed = copper;
    let mut paid = Vec::new();
    for id in inventory.ids().filter(|&id| inventory.is_carried(id)) {
        if owed == 0 {
            break;
        }
        let Some(item) = inventory.item(id) else { continue };
        let Some(coins) = item.coins else { continue };
        let take = coins.total_copper().min(owed);
        let mut parts = ItemParts::from(item.clone());
        parts.coins = Some(Coins::from_copper(coins.total_copper() - take));
        paid.push((id, Item::from_parts(parts)?));
        owed -= take;
    }
    (owed == 0).then_some(paid)
}

/// Takes `copper` out of the delver's carried purses, the first ones first. `false`, and nothing
/// taken, when they carry less.
pub fn pay(inventory: &mut Inventory, copper: u64) -> bool {
    let Some(paid) = plan_pay(inventory, copper) else { return false };
    for (id, purse) in paid {
        inventory.replace_item(id, purse);
    }
    true
}

/// A piece of work at the bench.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Work {
    Add(&'static Affix),
    Strip(&'static Affix),
    Reforge,
}

/// Why the work was not done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Declined {
    /// The enchanter will not, or the item cannot.
    Refused(Refusal),
    /// The delver cannot pay; says what is short.
    Short(String),
}

impl Declined {
    pub fn reason(&self) -> String {
        match self {
            Declined::Refused(r) => r.reason().to_string(),
            Declined::Short(what) => what.clone(),
        }
    }
}

/// What the work costs `by`, or why the item will not take it.
pub fn quote(item: &Item, work: Work, by: &Enchanter) -> Result<Cost, Refusal> {
    // The refusal comes from doing it, so the price and the answer are never two things.
    let done = match work {
        Work::Add(affix) => enchant::add(item, affix, by),
        Work::Strip(affix) => enchant::remove(item, affix, by),
        Work::Reforge => enchant::reforge(item, reforge_seed(item), by),
    };
    done?;
    Ok(match work {
        Work::Add(affix) => add_cost(affix, by.tier),
        Work::Strip(affix) => remove_cost(affix, by.tier),
        Work::Reforge => reforge_cost(item, by.tier),
    })
}

/// The seed a reforge of `item` rolls from: its own, turned by the work already done on it, so
/// reforging again gives something new and the same item always reforges the same.
pub fn reforge_seed(item: &Item) -> u32 {
    item.seed ^ (item.history.len() as u32 + 1).wrapping_mul(0x9E37_79B9)
}

/// What is short of paying `cost`, if anything: "Needs 24 essence, have 10."
pub fn shortfall(stock: &EnchantingStock, wallet: Coins, cost: &Cost) -> Option<String> {
    let mut short = Vec::new();
    if stock.essence < cost.essence {
        short.push(format!("{} essence, have {}", cost.essence, stock.essence));
    }
    for m in &cost.materials {
        if stock.count(&m.id) < m.count {
            short.push(format!("{} ×{}", fb_items::scrap::material_name(&m.id), m.count));
        }
    }
    if wallet.total_copper() < cost.copper {
        short.push(format!("{}, have {}", Coins::from_copper(cost.copper).short(), wallet.short()));
    }
    (!short.is_empty()).then(|| format!("Needs {}.", short.join(", ")))
}

/// Does `work` on item `id` for `by`: the enchanter's rules ([`quote`]) first, then payment from
/// the stock and the purses, then the item in the inventory is replaced by the changed one. Nothing
/// is spent for work that is declined. Returns the cost paid.
pub fn do_work(inventory: &mut Inventory, stock: &mut EnchantingStock, id: ItemId, work: Work, by: &Enchanter) -> Result<Cost, Declined> {
    let item = inventory.item(id).ok_or(Declined::Refused(Refusal::NotEnchantable))?.clone();
    let cost = quote(&item, work, by).map_err(Declined::Refused)?;
    if let Some(short) = shortfall(stock, wallet(inventory), &cost) {
        return Err(Declined::Short(short));
    }
    let changed = match work {
        Work::Add(affix) => enchant::add(&item, affix, by),
        Work::Strip(affix) => enchant::remove(&item, affix, by),
        Work::Reforge => enchant::reforge(&item, reforge_seed(&item), by),
    }
    .map_err(Declined::Refused)?;
    // Everything is worked out before anything is taken, and taken only if all of it can be.
    let Some(paid) = plan_pay(inventory, cost.copper) else { return Err(Declined::Short("Cannot pay.".into())) };
    let snapshot = stock.clone();
    if !stock.spend(cost.essence, &cost.materials) {
        return Err(Declined::Short("Cannot pay.".into()));
    }
    let before: Vec<(ItemId, Item)> = paid.iter().filter_map(|(id, _)| Some((*id, inventory.item(*id)?.clone()))).collect();
    let committed = paid.into_iter().all(|(id, purse)| inventory.replace_item(id, purse)) && inventory.replace_item(id, changed);
    if !committed {
        *stock = snapshot;
        for (id, purse) in before {
            inventory.replace_item(id, purse);
        }
        inventory.replace_item(id, item);
        return Err(Declined::Short("Cannot pay.".into()));
    }
    Ok(cost)
}

/// A cost on one line: "24 essence · 3 sp · Heartstone".
pub fn cost_line(cost: &Cost) -> String {
    let mut parts = vec![format!("{} essence", cost.essence)];
    parts.extend(cost.materials.iter().map(|m| fb_items::scrap::material_name(&m.id).to_string()));
    if cost.copper > 0 {
        parts.push(Coins::from_copper(cost.copper).short());
    }
    parts.join(" \u{b7} ")
}

/// The enchanter's tier from the trade's.
pub fn tier_of(tier: fb_faction::Tier) -> enchant::Tier {
    match tier {
        fb_faction::Tier::Novice => enchant::Tier::Novice,
        fb_faction::Tier::Apprentice => enchant::Tier::Apprentice,
        fb_faction::Tier::Expert => enchant::Tier::Expert,
        fb_faction::Tier::Master => enchant::Tier::Master,
    }
}

/// For captures (`FB_ENCHANT`): gives the delver essence, materials and coin, and a pack with
/// things worth a visit: a rusted sword, a magic dagger, a rare helm, a plain tunic.
pub fn bench_kit_for_capture(mut inventory: ResMut<Inventory>, mut stock: ResMut<Enchanting>, mut frames: Local<u32>) {
    *frames += 1;
    if *frames != 6 {
        return;
    }
    stock.0.essence = 180;
    for (id, n) in [("heartstone", 2), ("blood_salt", 1), ("gravesteel", 1)] {
        stock.0.materials.insert(id.into(), n);
    }
    let rolled = |base: &str, want: &dyn Fn(&Item) -> bool| (0..20_000).filter_map(|s| fb_items::roll_item_with(base, s, fb_items::Odds::loot(8))).find(|i| want(i));
    let rusted = fb_items::tables::affix_named("Rusted");
    let kit = [
        rolled("Sword", &|i| i.rarity == fb_items::Rarity::Magic && rusted.is_some_and(|r| i.affixes.contains(&r))).or_else(|| rolled("Sword", &|i| i.affixes.iter().any(|a| !a.good))),
        rolled("Dagger", &|i| i.rarity == fb_items::Rarity::Magic && i.affixes.iter().all(|a| a.good)),
        rolled("Helm", &|i| i.rarity == fb_items::Rarity::Rare && i.affixes.iter().all(|a| a.good)),
        rolled("Tunic", &|i| i.rarity == fb_items::Rarity::Common && i.affixes.is_empty()),
        rolled("Ring", &|i| i.rarity == fb_items::Rarity::Rare && i.affixes.iter().all(|a| a.good)),
        fb_items::roll_item("Purse", 31).map(|mut purse| {
            purse.coins = Some(Coins { gp: 12, sp: 5, cp: 0 });
            let mut parts = ItemParts::from(purse);
            parts.coins = Some(Coins { gp: 12, sp: 5, cp: 0 });
            Item::from_parts(parts).expect("a purse")
        }),
    ];
    for item in kit.into_iter().flatten() {
        let id = inventory.add(item);
        // A weapon is too long for the pouch: it goes in a hand.
        if inventory.stow(id).is_err() {
            let hand = if inventory.worn(fb_items::Slot::Mainhand).is_none() { fb_items::Slot::Mainhand } else { fb_items::Slot::Offhand };
            let _ = inventory.equip(id, hand);
        }
    }
}

/// For captures (`FB_SCRAP`): a few frames in, a rare sword and a legendary one are scrapped
/// into the delver's stock, so the character sheet has something to show.
pub fn scrap_for_capture(mut inventory: ResMut<Inventory>, mut stock: ResMut<Enchanting>, mut frames: Local<u32>) {
    *frames += 1;
    if *frames != 4 {
        return;
    }
    for rarity in [fb_items::Rarity::Rare, fb_items::Rarity::Legendary] {
        let sword = (0..6_000).filter_map(|s| fb_items::roll_item_with("Sword", s, fb_items::Odds::loot(8))).find(|i| i.rarity == rarity);
        if let Some(sword) = sword {
            let id = inventory.add(sword);
            scrap_item(&mut inventory, &mut stock.0, id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_items::enchant::Tier;
    use fb_items::tables::affix_named;
    use fb_items::{Odds, Rarity, roll_item, roll_item_with};

    fn by(tier: Tier) -> Enchanter {
        Enchanter { name: "Ysolde".into(), tier, faction: "the Ash Rite".into() }
    }

    /// An inventory with a worn sword of `want` and a purse of `gp` in the pack.
    fn kit(want: impl Fn(&Item) -> bool, gp: u32) -> (Inventory, ItemId) {
        let mut inventory = Inventory::default();
        let sword = (0..20_000).filter_map(|s| roll_item_with("Sword", s, Odds::loot(8))).find(want).unwrap();
        let id = inventory.add(sword);
        inventory.equip(id, fb_items::Slot::Mainhand).unwrap();
        let mut parts = ItemParts::from(roll_item("Purse", 4).unwrap());
        parts.coins = Some(Coins { gp, sp: 0, cp: 0 });
        let purse = inventory.add(Item::from_parts(parts).unwrap());
        inventory.equip(purse, fb_items::Slot::Belt).ok();
        // The purse is carried if it is worn or packed; a loose one is not.
        if !inventory.is_carried(purse) {
            let belt = roll_item("Belt", 3).unwrap();
            let belt = inventory.add(belt);
            inventory.equip(belt, fb_items::Slot::Belt).unwrap();
            inventory.stow(purse).unwrap();
        }
        (inventory, id)
    }

    #[test]
    fn an_expert_strips_rusted_for_essence_and_silver_and_pays_for_it() {
        let rusted = affix_named("Rusted").unwrap();
        let (mut inventory, id) = kit(|i| i.affixes.contains(&rusted), 5);
        let mut stock = EnchantingStock { essence: 10, ..Default::default() };
        let cost = do_work(&mut inventory, &mut stock, id, Work::Strip(rusted), &by(Tier::Expert)).unwrap();
        assert_eq!((cost.essence, cost.copper), (3, 30));
        assert_eq!(stock.essence, 7);
        assert_eq!(wallet(&inventory).total_copper(), 500 - 30);
        let item = inventory.item(id).unwrap();
        assert!(!item.affixes.contains(&rusted) && item.history.last().unwrap().contains("Stripped by Ysolde, expert enchanter"));
    }

    #[test]
    fn work_is_refused_when_short_and_nothing_is_spent() {
        let rusted = affix_named("Rusted").unwrap();
        let (mut inventory, id) = kit(|i| i.affixes.contains(&rusted), 5);
        let before = inventory.item(id).unwrap().clone();
        let mut broke = EnchantingStock::default();
        let err = do_work(&mut inventory, &mut broke, id, Work::Strip(rusted), &by(Tier::Expert)).unwrap_err();
        assert!(matches!(&err, Declined::Short(s) if s.contains("essence")), "{err:?}");
        let mut stock = EnchantingStock { essence: 50, ..Default::default() };
        let (mut poor, poor_id) = kit(|i| i.affixes.contains(&rusted), 0);
        let (purses_before, item_before) = (poor.clone(), poor.item(poor_id).unwrap().clone());
        let err = do_work(&mut poor, &mut stock, poor_id, Work::Strip(rusted), &by(Tier::Expert)).unwrap_err();
        assert!(matches!(&err, Declined::Short(s) if s.contains("sp")), "{err:?}");
        assert_eq!(stock.essence, 50);
        assert_eq!(inventory.item(id), Some(&before));
        // Enough essence but too little coin: the stock, the purses and the item are as they were.
        assert_eq!((wallet(&poor), poor.item(poor_id)), (wallet(&purses_before), Some(&item_before)));
        assert!(plan_pay(&poor, 1).is_none() && !pay(&mut poor, 1));
    }

    #[test]
    fn an_apprentice_cannot_add_the_bat_and_a_master_pays_a_heartstone_for_it() {
        let bat = affix_named("of the Bat").unwrap();
        let (mut inventory, id) = kit(|i| i.rarity == Rarity::Rare, 100);
        let mut stock = EnchantingStock { essence: 500, ..Default::default() };
        // A sword takes no Bat, whoever is asked; a helm does.
        assert_eq!(do_work(&mut inventory, &mut stock, id, Work::Add(bat), &by(Tier::Master)), Err(Declined::Refused(Refusal::DoesNotFit)));
        let mut inventory = Inventory::default();
        let helm = (0..20_000).filter_map(|s| roll_item_with("Helm", s, Odds::loot(8))).find(|i| i.rarity == Rarity::Rare && enchant::addable(i, Tier::Master).contains(&bat)).unwrap();
        let helm = inventory.add(helm);
        inventory.equip(helm, fb_items::Slot::Head).unwrap();
        assert_eq!(do_work(&mut inventory, &mut stock, helm, Work::Add(bat), &by(Tier::Apprentice)), Err(Declined::Refused(Refusal::TooHigh)));
        // A master, with the Heartstone and the coin, does it; without the Heartstone, not.
        let mut coins = ItemParts::from(roll_item("Purse", 4).unwrap());
        coins.coins = Some(Coins { gp: 100, sp: 0, cp: 0 });
        let purse = inventory.add(Item::from_parts(coins).unwrap());
        let belt = inventory.add(roll_item("Belt", 3).unwrap());
        inventory.equip(belt, fb_items::Slot::Belt).unwrap();
        inventory.stow(purse).unwrap();
        let err = do_work(&mut inventory, &mut stock, helm, Work::Add(bat), &by(Tier::Master)).unwrap_err();
        assert!(matches!(&err, Declined::Short(s) if s.contains("Heartstone")), "{err:?}");
        stock.materials.insert("heartstone".into(), 1);
        do_work(&mut inventory, &mut stock, helm, Work::Add(bat), &by(Tier::Master)).unwrap();
        assert_eq!(inventory.item(helm).unwrap().rarity, Rarity::Legendary);
        assert_eq!(stock.count("heartstone"), 0);
    }

    #[test]
    fn reforging_a_magic_dagger_comes_back_the_same_from_a_record() {
        let mut inventory = Inventory::default();
        let dagger = (0..20_000).filter_map(|s| roll_item_with("Dagger", s, Odds::loot(8))).find(|i| i.rarity == Rarity::Magic).unwrap();
        let id = inventory.add(dagger);
        inventory.equip(id, fb_items::Slot::Mainhand).unwrap();
        let mut stock = EnchantingStock { essence: 100, ..Default::default() };
        let mut coins = ItemParts::from(roll_item("Purse", 4).unwrap());
        coins.coins = Some(Coins { gp: 5, sp: 0, cp: 0 });
        let purse = inventory.add(Item::from_parts(coins).unwrap());
        let belt = inventory.add(roll_item("Belt", 3).unwrap());
        inventory.equip(belt, fb_items::Slot::Belt).unwrap();
        inventory.stow(purse).unwrap();
        let before: Vec<String> = inventory.item(id).unwrap().affixes.iter().map(|a| a.name.clone()).collect();
        do_work(&mut inventory, &mut stock, id, Work::Reforge, &by(Tier::Expert)).unwrap();
        let after = inventory.item(id).unwrap().clone();
        assert_ne!(after.affixes.iter().map(|a| a.name.clone()).collect::<Vec<_>>(), before);
        let json = serde_json::to_string(&crate::record::item_record(&after)).unwrap();
        let again = crate::record::rebuild_item(&serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(again, after);
        assert!(again.history.last().unwrap().starts_with("Reforged by Ysolde"));
    }

    #[test]
    fn coin_comes_out_of_the_carried_purses_only() {
        let (mut inventory, _) = kit(|_| true, 3);
        assert_eq!(wallet(&inventory).gp, 3);
        assert!(!pay(&mut inventory, 301));
        assert_eq!(wallet(&inventory).gp, 3);
        assert!(pay(&mut inventory, 120));
        assert_eq!(wallet(&inventory).total_copper(), 180);
    }

    #[test]
    fn scrapping_gear_removes_it_and_fills_the_stock() {
        let mut inventory = Inventory::default();
        let mut stock = EnchantingStock::default();
        let cloak = (0..200).filter_map(|s| roll_item("Cloak", s)).find(|i| i.affixes.is_empty()).unwrap();
        let id = inventory.add(cloak);
        let salvage = scrap_item(&mut inventory, &mut stock, id).unwrap();
        assert_eq!(stock.essence, salvage.essence);
        assert!(inventory.item(id).is_none());
        let purse = inventory.add(roll_item("Purse", 4).unwrap());
        assert!(scrap_item(&mut inventory, &mut stock, purse).is_none());
        assert!(inventory.item(purse).is_some());
    }
}
