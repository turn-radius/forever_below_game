//! Changing what an item carries at an enchanter: adding an affix, stripping one, or rerolling
//! them all (`docs/items.md`, "Enchanting"). Every function is pure and rebuilds the item through
//! [`Item::from_parts`], so its stats follow its affixes and the record of it round-trips. The
//! numbers are our own, in `data/enchant.json`.
//!
//! Rarity follows the good affixes an item carries: one is magic, two rare, three legendary.
//! The enchanter's [`Tier`] decides what they will do: a novice only strips flaws, an apprentice
//! adds affixes of the magic tier (the ones that roll on a common or magic item) and so makes
//! magic items, an expert rare ones, a master legendary ones. Legendaries themselves (their
//! legends) are left alone here.

use crate::dress::{condition_mut, dress, etching_mut};
use crate::item::{Item, ItemParts};
use crate::naming::{item_name, legendize, noun};
use crate::rarity::roll_affixes_for;
use crate::scrap::Material;
use crate::tables::{Affix, AffixKind, affixes};
use crate::{Kind, Rarity, Tag, game_rng};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// How skilled an enchanter is. Mirrors `fb_faction::Tier`, which sits above this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Novice,
    Apprentice,
    Expert,
    Master,
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Tier::Novice => "novice",
            Tier::Apprentice => "apprentice",
            Tier::Expert => "expert",
            Tier::Master => "master",
        }
    }

    /// The best rarity of item they will make by adding.
    pub fn ceiling(self) -> Rarity {
        match self {
            Tier::Novice => Rarity::Common,
            Tier::Apprentice => Rarity::Magic,
            Tier::Expert => Rarity::Rare,
            Tier::Master => Rarity::Legendary,
        }
    }
}

/// Who does the work, for the line it leaves in the item's history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Enchanter {
    pub name: String,
    pub tier: Tier,
    /// "the Ash Rite".
    pub faction: String,
}

impl Enchanter {
    fn by(&self) -> String {
        format!("{}, {} enchanter of {}", self.name, self.tier.label(), self.faction)
    }
}

/// Why an enchanter will not do it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not gear, or a legendary: nothing here to change.
    NotEnchantable,
    /// The affix is not one the item could roll, or it has one of its group.
    DoesNotFit,
    /// Beyond what the enchanter's tier will do.
    TooHigh,
    /// The item has no room: three good affixes, or a prefix and two suffixes already.
    Full,
    /// The item does not carry it.
    NotCarried,
    /// A common item has nothing to reroll.
    NothingToReforge,
}

impl Refusal {
    pub fn reason(self) -> &'static str {
        match self {
            Refusal::NotEnchantable => "Nothing here to enchant.",
            Refusal::DoesNotFit => "That would not take on this.",
            Refusal::TooHigh => "Beyond this enchanter's craft.",
            Refusal::Full => "It can carry no more.",
            Refusal::NotCarried => "It does not carry that.",
            Refusal::NothingToReforge => "Nothing to reforge.",
        }
    }
}

/// What a piece of work costs: essence, special materials and coin.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cost {
    pub essence: u32,
    pub materials: Vec<Material>,
    pub copper: u64,
}

#[derive(Deserialize)]
struct Table {
    essence: BTreeMap<Rarity, u32>,
    copper: BTreeMap<Rarity, u32>,
    strip_keeps: f64,
    flaw_essence: u32,
    flaw_copper: u32,
    reforge_essence: BTreeMap<Rarity, u32>,
    reforge_copper: BTreeMap<Rarity, u32>,
    essence_by_tier: BTreeMap<Tier, f64>,
    copper_by_tier: BTreeMap<Tier, f64>,
    material_for: BTreeMap<String, String>,
    default_material: String,
}

static TABLE: LazyLock<Table> = LazyLock::new(|| serde_json::from_str(include_str!("../../../data/enchant.json")).unwrap_or_else(|e| panic!("data/enchant.json: {e}")));

fn scaled(base: u32, by: f64) -> u32 {
    ((f64::from(base) * by).round() as u32).max(1)
}

/// The material a legendary-tier `affix` asks for: by its group (Blood salt for damage, Wardstone
/// chip for any bane), else a Heartstone.
fn material_of(affix: &Affix) -> Material {
    let t = &*TABLE;
    let group = affix.group.as_deref().unwrap_or_default();
    let id = t.material_for.iter().find(|(key, _)| if key.ends_with('_') { group.starts_with(key.as_str()) } else { group == key.as_str() }).map_or(&t.default_material, |(_, id)| id);
    Material { id: id.clone(), count: 1 }
}

/// What adding `affix` costs at `tier`: essence and coin by the affix's tier (the least rarity it
/// rolls on), scaled by the enchanter's craft (a master wastes less essence and charges more);
/// legendary-tier affixes also ask for a special material.
pub fn add_cost(affix: &Affix, tier: Tier) -> Cost {
    let t = &*TABLE;
    Cost {
        essence: scaled(t.essence[&affix.least], t.essence_by_tier[&tier]),
        materials: if affix.least == Rarity::Legendary { vec![material_of(affix)] } else { vec![] },
        copper: u64::from(scaled(t.copper[&affix.least], t.copper_by_tier[&tier])),
    }
}

/// What stripping `affix` costs at `tier`: a flaw is cheap to take off, a good affix half what
/// adding it costs.
pub fn remove_cost(affix: &Affix, tier: Tier) -> Cost {
    let t = &*TABLE;
    if !affix.good {
        return Cost { essence: scaled(t.flaw_essence, t.essence_by_tier[&tier]), materials: vec![], copper: u64::from(scaled(t.flaw_copper, t.copper_by_tier[&tier])) };
    }
    let full = add_cost(affix, tier);
    Cost { essence: scaled(full.essence, t.strip_keeps), materials: vec![], copper: ((full.copper as f64 * t.strip_keeps).round() as u64).max(1) }
}

/// What rerolling every affix of `item` costs at `tier`: by the item's rarity, less than choosing
/// the same number of affixes one by one.
pub fn reforge_cost(item: &Item, tier: Tier) -> Cost {
    let t = &*TABLE;
    let rarity = item.rarity.max(Rarity::Magic);
    Cost { essence: scaled(t.reforge_essence[&rarity], t.essence_by_tier[&tier]), materials: vec![], copper: u64::from(scaled(t.reforge_copper[&rarity], t.copper_by_tier[&tier])) }
}

/// Whether an enchanter can work on `item`: gear, not a container, a purse or a legendary.
pub fn enchantable(item: &Item) -> bool {
    item.profile.kind == Kind::Gear && item.coins.is_none() && item.rarity != Rarity::Legendary && item.legend.is_none() && !item.profile.tags.contains(&Tag::Container)
}

fn good(item: &Item) -> impl Iterator<Item = &&'static Affix> {
    item.affixes.iter().filter(|a| a.good)
}

fn rarity_of(good_count: usize) -> Rarity {
    match good_count {
        0 => Rarity::Common,
        1 => Rarity::Magic,
        2 => Rarity::Rare,
        _ => Rarity::Legendary,
    }
}

/// Why `tier` could not add `affix` to `item`, or `Ok` with the rarity it would make.
fn check_add(item: &Item, affix: &Affix, tier: Tier) -> Result<Rarity, Refusal> {
    if !enchantable(item) {
        return Err(Refusal::NotEnchantable);
    }
    if !affix.good || !affix.in_play() || !affix.fits_item(&item.profile.tags, item.profile.slot) || item.affixes.iter().any(|a| !affix.stacks_with(a)) {
        return Err(Refusal::DoesNotFit);
    }
    let count = good(item).count();
    let of_kind = good(item).filter(|a| a.kind == affix.kind).count();
    if count >= 3 || of_kind >= if affix.kind == AffixKind::Prefix { 1 } else { 2 } {
        return Err(Refusal::Full);
    }
    let made = rarity_of(count + 1);
    // The affix rolls only on its rarity or better, and the enchanter will make only so much.
    if affix.least > made || made > tier.ceiling() || affix.least > tier.ceiling() {
        return Err(Refusal::TooHigh);
    }
    Ok(made)
}

/// Every affix `tier` could add to `item`: good ones from the pools it could roll from (its tags
/// and slot, `not` and `at` respected), none of a group it already has, with room left, whose
/// tier the enchanter will do and which leaves the item no richer than they make. Empty for a
/// novice.
pub fn addable(item: &Item, tier: Tier) -> Vec<&'static Affix> {
    affixes().iter().filter(|a| check_add(item, a, tier).is_ok()).collect()
}

/// The affixes `tier` would take off `item`: flaws for a novice, any for the rest. None for what
/// cannot be enchanted.
pub fn removable(item: &Item, tier: Tier) -> Vec<&'static Affix> {
    if !enchantable(item) {
        return vec![];
    }
    item.affixes.iter().copied().filter(|a| tier >= Tier::Apprentice || !a.good).collect()
}

/// The item with its affixes replaced: renamed, restyled for its new rarity, stats worked out
/// again, and a line of `history` added.
/// A fresh piece of `base` from `seed`, dressed for `rarity` and `affixes`.
fn dressed(base: &str, seed: u32, rarity: Rarity, affixes: &[&Affix]) -> Option<armory::Item> {
    let mut shape = crate::armory_base::roll_shape(base, seed)?;
    dress(&mut shape, rarity, affixes, seed);
    Some(shape)
}

fn rebuilt(item: &Item, mut affixes: Vec<&'static Affix>, line: String) -> Item {
    // Good ones first, so the name takes them before a flaw.
    affixes.sort_by_key(|a| !a.good);
    let rarity = rarity_of(affixes.iter().filter(|a| a.good).count());
    let mut parts = ItemParts::from(item.clone());
    parts.name = item_name(&noun(&parts.base, &parts.shape), &affixes);
    parts.legend = (rarity == Rarity::Legendary).then(|| item.legend.clone().unwrap_or_else(|| legendize(&mut game_rng(item.seed ^ 0xE4C4), &parts.name, &item.profile.tags)));
    // The item keeps its own condition (earned wear, nicks, a snapped blade). Only what the
    // change itself makes of it moves: the difference between a fresh piece dressed for the old
    // rarity and affixes and one dressed for the new, in wear, rust, patina, rot and fray.
    if let (Some(old), Some(mut new)) = (dressed(&item.base, item.seed, item.rarity, &item.affixes), dressed(&item.base, item.seed, rarity, &affixes)) {
        let (o, n) = (old.condition(), new.condition());
        let c = condition_mut(&mut parts.shape);
        // What did not change is left exactly as it is.
        let shift = |have: f32, new: f32, old: f32| if new == old { have } else { (have + new - old).max(0.0) };
        c.wear = shift(c.wear, n.wear, o.wear).min(1.0);
        c.rust = shift(c.rust, n.rust, o.rust);
        c.patina = shift(c.patina, n.patina, o.patina);
        c.rot = shift(c.rot, n.rot, o.rot);
        c.fray = shift(c.fray, n.fray, o.fray);
        let etching = etching_mut(&mut new).map(|e| *e);
        if let (Some(e), Some(fresh)) = (etching_mut(&mut parts.shape), etching) {
            *e = fresh;
        }
    }
    parts.rarity = rarity;
    parts.affixes = affixes;
    parts.history.push(line);
    Item::from_parts(parts).expect("an item of a known base stays one")
}

/// `item` with `affix` added by `by`, if their tier and the item allow ([`addable`]). The rarity
/// follows the count, and a history line is kept: "Enchanted by Ysolde, expert enchanter of the
/// Ash Rite: of the Hawk."
pub fn add(item: &Item, affix: &'static Affix, by: &Enchanter) -> Result<Item, Refusal> {
    check_add(item, affix, by.tier)?;
    let mut chosen = item.affixes.clone();
    chosen.push(affix);
    Ok(rebuilt(item, chosen, format!("Enchanted by {}: {}.", by.by(), affix.name)))
}

/// `item` with `affix` taken off by `by`: a novice strips only flaws. The rarity follows the
/// count: stripping a good affix from a rare item makes it magic.
pub fn remove(item: &Item, affix: &Affix, by: &Enchanter) -> Result<Item, Refusal> {
    if !enchantable(item) {
        return Err(Refusal::NotEnchantable);
    }
    if !item.affixes.iter().any(|a| a.name == affix.name) {
        return Err(Refusal::NotCarried);
    }
    if affix.good && by.tier < Tier::Apprentice {
        return Err(Refusal::TooHigh);
    }
    let chosen: Vec<&'static Affix> = item.affixes.iter().copied().filter(|a| a.name != affix.name).collect();
    Ok(rebuilt(item, chosen, format!("Stripped by {}: {}.", by.by(), affix.name)))
}

/// `item` with every affix rolled again at its rarity from `seed`, flaws possible as when it was
/// rolled, from the pools its tags and slot allow. A pure function of the item and the seed; a
/// roll that comes out the same set is tried again from the next seed a few times, so a reforge
/// changes something when it can. A novice cannot: it takes an apprentice.
pub fn reforge(item: &Item, seed: u32, by: &Enchanter) -> Result<Item, Refusal> {
    if !enchantable(item) {
        return Err(Refusal::NotEnchantable);
    }
    if item.rarity == Rarity::Common {
        return Err(Refusal::NothingToReforge);
    }
    if by.tier < Tier::Apprentice {
        return Err(Refusal::TooHigh);
    }
    let names = |list: &[&Affix]| -> Vec<String> {
        let mut n: Vec<String> = list.iter().map(|a| a.name.clone()).collect();
        n.sort();
        n
    };
    let before = names(&item.affixes);
    let mut rolled = Vec::new();
    for tried in 0..8u32 {
        let mut rng = game_rng(seed.wrapping_add(tried).wrapping_mul(0x9E37_79B1) ^ item.seed);
        rolled = roll_affixes_for(&mut rng, item.rarity, &item.profile.tags, item.profile.slot);
        if names(&rolled) != before {
            break;
        }
    }
    // Rolled for the item's rarity, so a slot nothing fits can leave it short: the rarity follows.
    Ok(rebuilt(item, rolled, format!("Reforged by {}.", by.by())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::affix_named;
    use crate::{Odds, roll_item_with};

    fn by(tier: Tier) -> Enchanter {
        Enchanter { name: "Ysolde".into(), tier, faction: "the Ash Rite".into() }
    }

    fn find(base: &str, want: impl Fn(&Item) -> bool) -> Item {
        (0..20_000).filter_map(|s| roll_item_with(base, s, Odds::loot(8))).find(want).unwrap_or_else(|| panic!("no {base}"))
    }

    fn named(item: &Item) -> Vec<&str> {
        item.affixes.iter().map(|a| a.name.as_str()).collect()
    }

    /// An item rebuilt from its parts is itself: stats and name follow the affixes.
    fn consistent(item: &Item) {
        let again = Item::from_parts(ItemParts::from(item.clone())).unwrap();
        assert_eq!(&again, item);
        assert_eq!(item.rarity, rarity_of(good(item).count()));
        assert_eq!(item.legend.is_some(), item.rarity == Rarity::Legendary);
        for a in &item.affixes {
            assert!(a.fits_item(&item.profile.tags, item.profile.slot), "{} on {}", a.name, item.name);
        }
    }

    #[test]
    fn a_novice_adds_nothing_and_strips_only_flaws() {
        let sword = find("Sword", |i| i.rarity == Rarity::Magic && i.affixes.iter().any(|a| !a.good));
        let novice = by(Tier::Novice);
        assert!(addable(&sword, Tier::Novice).is_empty());
        let flaw = *sword.affixes.iter().find(|a| !a.good).unwrap();
        let good_one = *sword.affixes.iter().find(|a| a.good).unwrap();
        assert_eq!(remove(&sword, good_one, &novice), Err(Refusal::TooHigh));
        assert_eq!(removable(&sword, Tier::Novice), vec![flaw]);
        let clean = remove(&sword, flaw, &novice).unwrap();
        assert!(clean.affixes.iter().all(|a| a.good) && clean.rarity == sword.rarity);
        consistent(&clean);
        assert_eq!(reforge(&sword, 1, &novice), Err(Refusal::TooHigh));
    }

    #[test]
    fn an_apprentice_adds_only_magic_tier_and_makes_only_magic() {
        let bat = affix_named("of the Bat").unwrap();
        let helm = find("Helm", |i| i.rarity == Rarity::Rare);
        let apprentice = by(Tier::Apprentice);
        assert_eq!(add(&helm, bat, &apprentice), Err(Refusal::TooHigh));
        // A rare helm is beyond an apprentice's ceiling for any addition.
        assert!(addable(&helm, Tier::Apprentice).is_empty());
        let plain = find("Helm", |i| i.rarity == Rarity::Common && i.affixes.is_empty());
        let options = addable(&plain, Tier::Apprentice);
        assert!(!options.is_empty() && options.iter().all(|a| a.least <= Rarity::Magic));
        let made = add(&plain, options[0], &apprentice).unwrap();
        assert_eq!(made.rarity, Rarity::Magic);
        assert!(made.history.last().unwrap().starts_with("Enchanted by Ysolde, apprentice enchanter of the Ash Rite: "));
        consistent(&made);
    }

    #[test]
    fn a_master_adds_the_bat_to_a_rare_helm_for_a_heartstone_and_makes_it_a_legend() {
        let bat = affix_named("of the Bat").unwrap();
        let helm = find("Helm", |i| i.rarity == Rarity::Rare && check_add(i, bat, Tier::Master).is_ok());
        assert_eq!(add(&helm, bat, &by(Tier::Expert)), Err(Refusal::TooHigh));
        let legend = add(&helm, bat, &by(Tier::Master)).unwrap();
        assert_eq!(legend.rarity, Rarity::Legendary);
        consistent(&legend);
        assert_eq!(add_cost(bat, Tier::Master).materials, vec![Material { id: "heartstone".into(), count: 1 }]);
        assert_eq!(add_cost(affix_named("of Carnage").unwrap(), Tier::Master).materials[0].id, "blood_salt");
        assert!(removable(&legend, Tier::Master).is_empty());
    }

    #[test]
    fn an_expert_strips_rusted_from_a_sword() {
        let rusted = affix_named("Rusted").unwrap();
        let sword = find("Sword", |i| i.affixes.contains(&rusted));
        let before = sword.shape.condition();
        let clean = remove(&sword, rusted, &by(Tier::Expert)).unwrap();
        assert!(!clean.affixes.contains(&rusted));
        assert!(clean.shape.condition().rust < before.rust, "rust {} to {}", before.rust, clean.shape.condition().rust);
        assert!(clean.history.last().unwrap().contains("Stripped by Ysolde, expert enchanter of the Ash Rite: Rusted."));
        assert!(!clean.name.contains("Rusted"));
        consistent(&clean);
    }

    #[test]
    fn adding_and_stripping_move_the_stats_and_the_rarity_together() {
        let keen = affix_named("Keen").unwrap();
        let plain = find("Sword", |i| i.rarity == Rarity::Common && i.affixes.is_empty());
        let keener = add(&plain, keen, &by(Tier::Expert)).unwrap();
        assert_eq!(keener.profile.stats.damage_bonus, plain.profile.stats.damage_bonus + 1.0);
        assert_eq!((keener.rarity, keener.name.clone()), (Rarity::Magic, format!("Keen {}", plain.name)));
        let back = remove(&keener, keen, &by(Tier::Expert)).unwrap();
        assert_eq!((back.rarity, back.profile.stats.damage_bonus), (Rarity::Common, plain.profile.stats.damage_bonus));
        // Adding it twice, or a second of its group, is refused.
        assert_eq!(add(&keener, keen, &by(Tier::Expert)), Err(Refusal::DoesNotFit));
        assert_eq!(remove(&plain, keen, &by(Tier::Expert)), Err(Refusal::NotCarried));
    }

    #[test]
    fn an_addable_affix_is_one_the_item_could_roll() {
        let sword = find("Sword", |i| i.rarity == Rarity::Common && i.affixes.is_empty());
        let options = addable(&sword, Tier::Master);
        assert!(!options.is_empty());
        assert!(options.iter().all(|a| a.good && a.fits_item(&sword.profile.tags, sword.profile.slot)));
        assert!(options.iter().all(|a| a.name != "Warm" && a.name != "Gleaming"));
    }

    #[test]
    fn reforging_rerolls_at_the_rarity_and_survives_a_rebuild() {
        let dagger = find("Dagger", |i| i.rarity == Rarity::Magic);
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..40 {
            let re = reforge(&dagger, seed, &by(Tier::Apprentice)).unwrap();
            assert_eq!(re, reforge(&dagger, seed, &by(Tier::Apprentice)).unwrap());
            assert!(re.history.last().unwrap().starts_with("Reforged by Ysolde"));
            consistent(&re);
            seen.extend(named(&re).into_iter().map(String::from));
        }
        assert!(seen.len() > 3, "{seen:?}");
        assert_eq!(reforge(&find("Sword", |i| i.rarity == Rarity::Common), 1, &by(Tier::Master)), Err(Refusal::NothingToReforge));
    }

    #[test]
    fn a_reforge_changes_the_affixes_when_it_can() {
        let ring = find("Ring", |i| i.rarity == Rarity::Magic);
        for seed in 0..30 {
            let re = reforge(&ring, seed, &by(Tier::Expert)).unwrap();
            assert_ne!(named(&re), named(&ring));
        }
    }

    #[test]
    fn reforging_costs_less_than_choosing_and_costs_rise_with_the_work() {
        let rare = find("Sword", |i| i.rarity == Rarity::Rare);
        let keen = affix_named("Keen").unwrap();
        let fox = affix_named("of the Fox").unwrap();
        for tier in [Tier::Apprentice, Tier::Expert, Tier::Master] {
            let reroll = reforge_cost(&rare, tier);
            let choose = add_cost(keen, tier).essence + add_cost(fox, tier).essence;
            assert!(reroll.essence < choose && reroll.copper < add_cost(keen, tier).copper + add_cost(fox, tier).copper, "{tier:?}");
        }
        assert!(add_cost(fox, Tier::Expert).essence > add_cost(keen, Tier::Expert).essence);
        assert!(add_cost(keen, Tier::Master).copper > add_cost(keen, Tier::Apprentice).copper);
        assert!(add_cost(keen, Tier::Master).essence < add_cost(keen, Tier::Apprentice).essence);
        let rusted = affix_named("Rusted").unwrap();
        assert!(remove_cost(rusted, Tier::Expert).essence < add_cost(keen, Tier::Expert).essence);
    }

    #[test]
    fn enchanting_keeps_the_wear_an_item_has_earned() {
        let rusted = affix_named("Rusted").unwrap();
        let sword = find("Sword", |i| i.affixes.contains(&rusted) && i.rarity == Rarity::Magic);
        let mut worn = sword.clone();
        let mut c = worn.shape.condition();
        (c.wear, c.fray, c.damage) = (0.9, 0.2, 0.4);
        worn.shape.set_condition(c);
        let before = worn.shape.condition();
        let clean = remove(&worn, rusted, &by(Tier::Expert)).unwrap();
        let after = clean.shape.condition();
        assert!(after.rust < before.rust, "the flaw's rust goes");
        // Rusted dressed the piece worn to 0.75; the 0.15 over that is earned and stays.
        let fresh = dressed(&sword.base, sword.seed, sword.rarity, &sword.affixes).unwrap().condition().wear;
        let bare = dressed(&clean.base, clean.seed, clean.rarity, &clean.affixes).unwrap().condition().wear;
        assert!((after.wear - (bare + (0.9 - fresh))).abs() < 1e-5, "wear {} from {}", after.wear, before.wear);
        assert_eq!((after.fray, after.damage), (before.fray, before.damage), "earned wear stays");
        // Adding an affix to a worn item moves no wear, rust or fray.
        let keen = affix_named("Keen").unwrap();
        let plain = find("Sword", |i| i.rarity == Rarity::Common && i.affixes.is_empty());
        let mut worn = plain.clone();
        let mut c = worn.shape.condition();
        (c.wear, c.rust, c.fray) = (0.45, 0.3, 0.2);
        worn.shape.set_condition(c);
        let after = add(&worn, keen, &by(Tier::Expert)).unwrap().shape.condition();
        let dressed_for = |r| dressed(&plain.base, plain.seed, r, &[]).unwrap().condition().wear;
        assert_eq!((after.rust, after.fray), (0.3, 0.2));
        assert!((after.wear - (0.45 + dressed_for(Rarity::Magic) - dressed_for(Rarity::Common))).abs() < 1e-5);
    }
}
