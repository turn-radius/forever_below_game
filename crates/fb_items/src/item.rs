//! A whole Forever Below item from a base name and a seed: the plate's `rollSA` and
//! `makeGood`, tying together the shape, profile, rarity, affixes, name, legend and dress.

use armory::families::bags::Accessory;
use crate::identify::Known;
use crate::armory_base::{base_for, roll_shape};
use crate::dress::dress;
use crate::naming::{Legend, item_name, legendize, legendize_in, noun};
use crate::notes::Lore;
use crate::profile::{Profile, profile};
use crate::rarity::{Odds, roll_rarity_for};
use crate::tables::Affix;
use crate::{Kind, Rarity, Slot, Tag, game_rng};
use fb_denizens::tongue::Teaching;
use std::fmt;

/// The fewest cells long (about 24 cm each) a weapon is to ride a pack's weapon strap.
pub const STRAP_CELLS: u8 = 4;

/// D&D coinage in a purse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Coins {
    pub gp: u32,
    pub sp: u32,
    pub cp: u32,
}

impl Coins {
    pub const CP_PER_SP: u64 = 10;
    pub const CP_PER_GP: u64 = 100;

    /// `cp` copper made into the fewest coins: 250 cp is 2 gp 5 sp.
    pub fn from_copper(cp: u64) -> Coins {
        Coins {
            gp: (cp / Self::CP_PER_GP).min(u64::from(u32::MAX)) as u32,
            sp: (cp % Self::CP_PER_GP / Self::CP_PER_SP) as u32,
            cp: (cp % Self::CP_PER_SP) as u32,
        }
    }

    /// Everything in copper: 1 gp is 100.
    pub fn total_copper(self) -> u64 {
        u64::from(self.gp) * Self::CP_PER_GP + u64::from(self.sp) * Self::CP_PER_SP + u64::from(self.cp)
    }

    /// The sum, with change.
    pub fn plus(self, other: Coins) -> Coins {
        Coins::from_copper(self.total_copper() + other.total_copper())
    }

    /// What is left after paying `price`, with change (2 gp less 15 sp is 5 sp); `None` if
    /// there is not enough.
    pub fn minus(self, price: Coins) -> Option<Coins> {
        self.total_copper().checked_sub(price.total_copper()).map(Coins::from_copper)
    }

    /// Whether it holds no coin.
    pub fn is_empty(self) -> bool {
        self.total_copper() == 0
    }

    /// Only the coins it has, for a card: "1 gp 5 sp", "3 cp", "0 cp" for none.
    pub fn short(self) -> String {
        let c = Coins::from_copper(self.total_copper());
        let parts: Vec<String> = [(c.gp, "gp"), (c.sp, "sp"), (c.cp, "cp")].into_iter().filter(|&(n, _)| n > 0).map(|(n, unit)| format!("{n} {unit}")).collect();
        if parts.is_empty() { "0 cp".to_string() } else { parts.join(" ") }
    }
}

impl std::ops::Add for Coins {
    type Output = Coins;
    fn add(self, other: Coins) -> Coins {
        self.plus(other)
    }
}

/// A delver's wallet: the sum of their purses (`purses.filter_map(|p| p.coins).sum()`).
impl std::iter::Sum for Coins {
    fn sum<I: Iterator<Item = Coins>>(iter: I) -> Coins {
        Coins::from_copper(iter.map(Coins::total_copper).sum())
    }
}

impl<'a> std::iter::Sum<&'a Coins> for Coins {
    fn sum<I: Iterator<Item = &'a Coins>>(iter: I) -> Coins {
        iter.copied().sum()
    }
}

impl fmt::Display for Coins {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} gp · {} sp · {} cp", self.gp, self.sp, self.cp)
    }
}

/// One rolled item.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub base: String,
    pub seed: u32,
    /// The Armory shape, dressed for its rarity and flaws.
    pub shape: armory::Item,
    pub rarity: Rarity,
    pub affixes: Vec<&'static Affix>,
    pub name: String,
    /// A legendary's title, old name and history.
    pub legend: Option<Legend>,
    /// Slot, tags, footprint and the like; its stats include the affixes.
    pub profile: Profile,
    /// What a purse or pouch holds.
    pub coins: Option<Coins>,
    /// The odds its rarity was rolled against, so its base and seed roll it again the same.
    pub odds: Odds,
    /// Lines of history beyond a legendary's legend ("Made by Brenna Holt"), in the order they
    /// happened; the hover card shows them after the legend's.
    pub history: Vec<String>,
    /// What is written on it: a note's words, written where it is found ([`Item::in_world`]).
    pub words: Option<String>,
    /// The words it teaches the reader, if it is written in a denizen tongue ([`Item::teaching`]).
    pub teaching: Option<Teaching>,
    /// How much of its magic the delver has found out ([`Known`]). Everything rolled is fully known
    /// until the game sets a found one unknown ([`Item::unidentified`]); the stats count either way.
    pub known: Known,
    /// The count of rests when an attempt to identify it last fell short: it cannot be tried
    /// again until the delver has rested since ([`Item::can_identify`]).
    pub rest_tried: Option<u32>,
    /// How many of it this one item stands for: a stack of arrows is one item of ten, sharing one
    /// seed and so one model ([`Item::stack_limit`], [`Item::can_stack_with`]). Always at least 1.
    pub quantity: u32,
}

/// The parts an [`Item`] is built from by [`Item::from_parts`], rather than rolled: what an item
/// is now, after being changed (new affixes, a maker's line) or recorded and restored.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemParts {
    pub base: String,
    pub seed: u32,
    pub shape: armory::Item,
    pub rarity: Rarity,
    pub affixes: Vec<&'static Affix>,
    pub name: String,
    pub legend: Option<Legend>,
    pub history: Vec<String>,
    pub coins: Option<Coins>,
    pub odds: Odds,
    pub words: Option<String>,
    pub teaching: Option<Teaching>,
    pub known: Known,
    pub rest_tried: Option<u32>,
    pub quantity: u32,
}

impl From<Item> for ItemParts {
    fn from(i: Item) -> Self {
        ItemParts { base: i.base, seed: i.seed, shape: i.shape, rarity: i.rarity, affixes: i.affixes, name: i.name, legend: i.legend, history: i.history, coins: i.coins, odds: i.odds, words: i.words, teaching: i.teaching, known: i.known, rest_tried: i.rest_tried, quantity: i.quantity }
    }
}

impl Item {
    /// The item `parts` describe, its profile worked out again from the shape and the affixes
    /// (their effects on the stats), with nothing rolled: the name, rarity and legend are as
    /// given. The shape is taken as it is, so condition and fuel stay. `None` for a base no
    /// table knows. An item taken apart ([`ItemParts::from`]) and put back together is equal.
    pub fn from_parts(parts: ItemParts) -> Option<Item> {
        crate::armory_base::armory_base(&parts.base)?;
        let ItemParts { base, seed, shape, rarity, affixes, name, legend, history, coins, odds, words, teaching, known, rest_tried, quantity } = parts;
        let mut profile = profile(&base, &shape);
        if let Some(c) = coins {
            profile.description = c.to_string();
        }
        for a in &affixes {
            a.apply_to_stats(&mut profile.stats);
        }
        profile.fit_capacity();
        Some(Item { base, seed, shape, rarity, affixes, name, legend, profile, coins, odds, history, words, teaching, known, rest_tried, quantity: quantity.max(1) })
    }
}

impl Item {
    /// How many of it share one place in a grid: 10 arrows or bolts, 1 for everything else. The
    /// goods table's `stack` column (bread 3...) is not read yet: only ammunition stacks until
    /// the game can eat or drink one of a stack.
    pub fn stack_limit(&self) -> u32 {
        if self.profile.tags.contains(&Tag::Ammunition) { u32::from(self.profile.stack.max(1)) } else { 1 }
    }

    /// Whether `other` could join this one's stack: a kind that stacks at all, and every part of
    /// it the same. The base and seed are one model, one seed for the stack; `shape` is compared
    /// with the Armory state's derived `==`, so every slider, the condition (wear, rust, fuel)
    /// and the palette must be bit for bit alike, and an arrow worn by a shot never joins a
    /// fresh one; the rest (rarity, affixes, name, legend, history, coins, words, teaching, how
    /// known, the rest an identify was tried at, the odds it was rolled against) must be equal
    /// too. Two arrow types never stack. Quantity is not compared.
    pub fn can_stack_with(&self, other: &Item) -> bool {
        self.stack_limit() > 1
            && self.base == other.base
            && self.seed == other.seed
            && self.shape == other.shape
            && self.legend == other.legend
            && self.history == other.history
            && self.teaching == other.teaching
            && self.rest_tried == other.rest_tried
            && self.odds == other.odds
            && self.rarity == other.rarity
            && self.affixes == other.affixes
            && self.name == other.name
            && self.coins == other.coins
            && self.words == other.words
            && self.known == other.known
    }

    /// How many more it can take before it is full.
    pub fn stack_room(&self) -> u32 {
        self.stack_limit().saturating_sub(self.quantity)
    }

    /// This item as a stack of `n`.
    pub fn with_quantity(mut self, n: u32) -> Item {
        self.quantity = n.max(1);
        self
    }

    /// The name shown first: a legendary's title, otherwise its name.
    pub fn title(&self) -> &str {
        self.legend.as_ref().map_or(&self.name, |l| &l.title)
    }

    /// This item found on a level that knows `lore` of itself. A legendary's history names the
    /// lore's factions rather than factions of the grammar's making: its title stays, and a legend
    /// made at an enchanter's bench (`Legend::made`) is left as it is, as it tells its making, not
    /// a found relic. A note with nothing written on it is written on, of the level
    /// ([`crate::notes::write_note`]), or now and then with a rumour of the denizens
    /// ([`crate::tongues::rumour`]) or, deep down, of the level's denizen factions ([`crate::tongues::knowledge`]); one already written keeps its words wherever it goes. Pure
    /// in the item's seed and the lore, and a no-op for anything else.
    pub fn in_world(mut self, lore: &Lore) -> Item {
        if self.words.is_none() && self.is_note() {
            self.words = Some(match (crate::tongues::rumour(self.seed, lore.depth), crate::tongues::knowledge(self.seed, lore)) {
                (Some((_, rumour)), _) => rumour,
                (None, Some(known)) => known,
                (None, None) => crate::notes::write_note(self.seed, lore),
            });
        }
        let factions = &lore.factions;
        if factions.is_empty() {
            return self;
        }
        let tags = self.profile.tags.clone();
        let seed = self.seed;
        let name = self.name.clone();
        if let Some(legend) = self.legend.as_mut().filter(|l| !l.made) {
            // Its own stream, so the item rolls the same whatever world it is found in.
            let told = legendize_in(&mut game_rng(seed ^ 0x1e6e_d15c), &name, &tags, Some(factions));
            legend.history = told.history;
        }
        self
    }

    /// Whether it is a note, which [`Item::in_world`] writes on.
    pub fn is_note(&self) -> bool {
        matches!(&self.shape, armory::Item::Writing(s) if s.piece.kind == armory::families::writing::WritingKind::Note)
    }

    /// How far its light reaches against its base's: 1.5 for a Gleaming torch (+10 ft on 20),
    /// 0.75 for a Smoky one, 1 for a light with no affix on its reach or a thing that gives
    /// none. The game scales the Armory's light by it, which knows only the shape.
    pub fn light_factor(&self) -> f32 {
        let added: f32 = self
            .affixes
            .iter()
            .flat_map(|a| &a.fx)
            .filter_map(|m| match *m {
                crate::modifier::Modifier::Add { stat: crate::stats::Stat::Light, by } => Some(by),
                _ => None,
            })
            .sum();
        let base = self.profile.stats.light - added;
        if base > 0.0 { self.profile.stats.light.max(0.0) / base } else { 1.0 }
    }

    /// Every slot it can be worn or held in, the one it is made for first; none for goods. A
    /// ring goes on either ring slot (the plate's `targetSlot`), and a weapon in either hand,
    /// the main hand first (the user's call: the plate held weapons only in the main hand). A
    /// lantern goes in either hand, the off hand first, or hangs from the belt at the hip
    /// (the user's call), whose belt the inventory checks.
    pub fn slots(&self) -> Vec<Slot> {
        let Some(made_for) = self.profile.slot else { return vec![] };
        if self.shape.family() == armory::Family::Lantern {
            return vec![made_for, Slot::Mainhand, Slot::Hip];
        }
        // A torch is held in either hand, the off hand first, so it can be swung: a lit torch's
        // blow is fire, which stops a troll's regeneration.
        if self.shape.family() == armory::Family::Torch {
            return vec![made_for, Slot::Mainhand];
        }
        // A candle or chamberstick is held in either hand, never on the hip: that is the lantern's.
        if matches!(&self.shape, armory::Item::Sundry(s) if s.article.kind == armory::families::sundry::SundryKind::Candle) {
            return vec![made_for, Slot::Mainhand];
        }
        match made_for {
            Slot::Ring1 | Slot::Ring2 => vec![Slot::Ring1, Slot::Ring2],
            // A robe is worn over body armour, in the cloak's place, as well as as a shirt.
            Slot::Shirt if matches!(&self.shape, armory::Item::Garment(g) if g.cut.style == armory::sections::garment::GarmentStyle::Robe) => vec![Slot::Shirt, Slot::Cloak],
            Slot::Mainhand if self.profile.tags.contains(&Tag::Weapon) => vec![Slot::Mainhand, Slot::Offhand],
            _ => vec![made_for],
        }
    }

    /// Game seconds a full torch or lantern of this item burns: one hour for a torch and six for
    /// a lantern, times its `BurnTime`; `None` for anything else.
    pub fn fuel_capacity(&self) -> Option<f32> {
        self.shape.fuel_capacity().map(|seconds| seconds * self.profile.stats.burn_time)
    }

    /// Game seconds of burning it has left; `None` unless it is a torch or a lantern.
    pub fn fuel_left(&self) -> Option<f32> {
        self.shape.fuel_left()
    }

    /// Burns `seconds` of game time off it if it is lit ([`armory::Item::burn`]).
    pub fn burn(&mut self, seconds: f32) {
        if let Some(capacity) = self.fuel_capacity() {
            self.shape.burn(seconds, capacity.max(1.0));
        }
    }

    /// Whether it can be worn or held in `slot` ([`Item::slots`]).
    pub fn fits(&self, slot: Slot) -> bool {
        self.slots().contains(&slot)
    }

    /// What a bag was rolled with: a pack's bedroll and weapon strap, in a fixed order. Accessories
    /// are part of the Armory's roll, so they are never found loose or fitted by the delver. Empty
    /// for anything but a bag.
    pub fn accessories(&self) -> Vec<Accessory> {
        match &self.shape {
            armory::Item::Bags(s) => s.bag.accessories(),
            _ => vec![],
        }
    }

    /// Whether it was rolled with `accessory`.
    pub fn has_accessory(&self, accessory: Accessory) -> bool {
        self.accessories().contains(&accessory)
    }

    /// Whether a weapon strap holds it: a weapon at least [`STRAP_CELLS`] cells long by its
    /// footprint, so a staff, spear, bow or long sword, and a polearm (an axe or mace with a long
    /// haft) as well, but not a dagger, a club or a short axe.
    pub fn strappable(&self) -> bool {
        let fp = self.profile.footprint;
        self.profile.tags.contains(&Tag::Weapon) && fp.cols.max(fp.rows) >= STRAP_CELLS
    }
}

/// `items` with the pieces that stack folded together, a full stack at a time, in the order of
/// each kind's first piece: twenty arrows of one seed come back as stacks of 10 and 10.
pub fn stack_up(items: impl IntoIterator<Item = Item>) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    for mut item in items {
        for have in out.iter_mut() {
            if !have.can_stack_with(&item) {
                continue;
            }
            let moved = have.stack_room().min(item.quantity);
            have.quantity += moved;
            item.quantity -= moved;
        }
        if item.quantity > 0 {
            out.push(item);
        }
    }
    out
}

/// Rolls base `base` from `seed` at the first level's odds ([`roll_item_with`]).
pub fn roll_item(base: &str, seed: u32) -> Option<Item> {
    roll_item_with(base, seed, Odds::default())
}

/// Rolls base `base` from `seed`, its rarity against `odds`. `None` for a name no table knows.
/// Pure in its arguments.
///
/// Gear rolls rarity and affixes from its tags, applies the affixes to its stats, is named
/// and, if legendary, titled with a history, and is dressed for its rarity. Goods are common
/// and plain, as in the plate; a purse that rolled coins gets a count of them.
pub fn roll_item_with(base: &str, seed: u32, odds: Odds) -> Option<Item> {
    let mut shape = roll_shape(base, seed)?;
    let mut profile = profile(base, &shape);
    let mut rng = game_rng(seed);
    let noun = noun(base, &shape);

    if profile.kind == Kind::Good {
        let coins = match &shape {
            armory::Item::Bags(s) if s.bag.coins => {
                let gp = (rng.next_f64() * rng.next_f64() * 14.0) as u32;
                let sp = rng.int(0, 29);
                let cp = rng.int(5, 64);
                let purse = Coins { gp, sp, cp };
                // Deeper hoards are richer; the first level's purses are as rolled.
                let scale = crate::value::coin_depth_factor(odds.depth);
                Some(if scale > 1.0 { Coins::from_copper((purse.total_copper() as f64 * scale).round() as u64) } else { purse })
            }
            _ => None,
        };
        if let Some(c) = coins {
            profile.description = c.to_string();
        }
        return Some(Item { base: base.to_string(), seed, shape, rarity: Rarity::Common, affixes: vec![], name: noun, legend: None, profile, coins, odds, history: vec![], words: None, teaching: None, known: Known::Full, rest_tried: None, quantity: 1 });
    }

    let (rarity, affixes) = roll_rarity_for(&mut rng, &profile.tags, profile.slot, &odds);
    for a in &affixes {
        a.apply_to_stats(&mut profile.stats);
    }
    profile.fit_capacity();
    let name = item_name(&noun, &affixes);
    let legend = (rarity == Rarity::Legendary).then(|| legendize(&mut rng, &name, &profile.tags));
    dress(&mut shape, rarity, &affixes, seed);
    // The fuel it starts with is what the item's `BurnTime` makes of a full one.
    shape.scale_fuel(profile.stats.burn_time);
    Some(Item { base: base.to_string(), seed, shape, rarity, affixes, name, legend, profile, coins: None, odds, history: vec![], words: None, teaching: None, known: Known::Full, rest_tried: None, quantity: 1 })
}

/// Loot that names only an Armory family and a seed (the level's item spots): a base of that
/// family chosen from the seed ([`base_for`]), rolled from the same seed against `odds`.
pub fn roll_loot(family: armory::Family, seed: u32, odds: Odds) -> Option<Item> {
    roll_item_with(base_for(family, seed)?, seed, odds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armory_base::{ARMORY_BASES, held_back};

    #[test]
    fn packs_roll_with_their_accessories_and_nothing_else_does() {
        let packs: Vec<Item> = (1..80).filter_map(|s| roll_item("Pack", s)).collect();
        assert!(packs.iter().any(|p| p.has_accessory(Accessory::Bedroll)), "a bedroll");
        assert!(packs.iter().any(|p| p.has_accessory(Accessory::WeaponStrap)), "a weapon strap");
        assert!(packs.iter().any(|p| p.accessories().is_empty()), "a plain pack");
        // The same seed rolls the same accessories.
        assert!(packs.iter().all(|p| roll_item("Pack", p.seed).unwrap().accessories() == p.accessories()));
        for base in ["Satchel", "Sack", "Purse", "Sword", "Bedroll"] {
            assert!((1..20).all(|s| roll_item(base, s).unwrap().accessories().is_empty()), "{base}");
        }
    }

    #[test]
    fn only_weapons_long_by_their_footprint_are_strapped() {
        for base in ["Staff", "Spear", "Bow"] {
            assert!((1..30).map(|s| roll_item(base, s).unwrap()).filter(|w| w.profile.footprint.rows.max(w.profile.footprint.cols) >= STRAP_CELLS).all(|w| w.strappable()), "{base}");
            assert!((1..30).map(|s| roll_item(base, s).unwrap()).any(|w| w.strappable()), "some {base} is long enough");
        }
        for base in ["Dagger", "Shield", "Torch", "Mail", "Pack"] {
            assert!((1..30).all(|s| !roll_item(base, s).unwrap().strappable()), "{base}");
        }
        // Whatever the family, it is the length that decides.
        for base in ["Sword", "Axe", "Mace", "Club", "Crossbow"] {
            assert!((1..30).filter_map(|s| roll_item(base, s)).all(|w| w.strappable() == (w.profile.footprint.rows.max(w.profile.footprint.cols) >= STRAP_CELLS)), "{base}");
        }
    }

    #[test]
    fn arrows_and_bolts_stack_to_ten_and_only_with_their_own_kind() {
        let arrow = roll_item("Arrow", 7).unwrap();
        assert_eq!((arrow.quantity, arrow.stack_limit(), arrow.stack_room()), (1, 10, 9));
        assert_eq!(roll_item("Bolt", 7).unwrap().stack_limit(), 10);
        assert!(arrow.can_stack_with(&roll_item("Arrow", 7).unwrap()));
        assert!(!arrow.can_stack_with(&roll_item("Arrow", 8).unwrap()), "another model is another stack");
        assert!(!arrow.can_stack_with(&roll_item("Bolt", 7).unwrap()));
        let mut told = roll_item("Arrow", 7).unwrap();
        told.history.push("Fletched by Brenna".into());
        assert!(!arrow.can_stack_with(&told), "a different history is a different piece");
        let full = roll_item("Arrow", 7).unwrap().with_quantity(10);
        assert_eq!(full.stack_room(), 0);
        assert_eq!(roll_item("Sword", 1).unwrap().stack_limit(), 1);
        assert!(!roll_item("Bread", 1).unwrap().can_stack_with(&roll_item("Bread", 1).unwrap()));
    }

    #[test]
    fn stacking_up_folds_pieces_ten_to_a_stack() {
        let pieces = (0..24).map(|i| roll_item(if i == 5 { "Bolt" } else { "Arrow" }, 7).unwrap());
        let stacks = stack_up(pieces);
        assert_eq!(stacks.iter().map(|i| (i.base.as_str(), i.quantity)).collect::<Vec<_>>(), vec![("Arrow", 10), ("Bolt", 1), ("Arrow", 10), ("Arrow", 3)]);
    }

    #[test]
    fn a_stack_keeps_its_count_through_parts() {
        let stack = roll_item("Arrow", 3).unwrap().with_quantity(6);
        assert_eq!(Item::from_parts(stack.clone().into()).unwrap().quantity, 6);
        assert_eq!(Item::from_parts(ItemParts { quantity: 0, ..stack.into() }).unwrap().quantity, 1);
    }

    #[test]
    fn tents_and_campfire_kits_roll_by_name_but_never_lie_loose() {
        for (family, base) in [(armory::Family::Tent, "Tent"), (armory::Family::Campfire, "Campfire kit")] {
            // Held back from loose loot until tentmakers stock them.
            assert!((0..200).all(|seed| roll_loot(family, seed, Odds::loot(3)).is_none()), "{base} lies loose");
            let by_name = roll_item(base, 9).unwrap();
            assert_eq!((by_name.profile.kind, by_name.rarity), (Kind::Good, Rarity::Common));
            assert!(by_name.profile.packable && by_name.profile.stats.weight >= 12.0, "{base}");
            assert_eq!(roll_item(base, 9), Some(by_name));
        }
    }

    /// Whether floor loot of the sundry family, `seeds` seeds at each depth from 1 to 10, is
    /// ever a held-back base.
    fn loose_sundries_hold_back(seeds: u32) -> bool {
        (1..=10).any(|depth| (0..seeds).filter_map(|seed| roll_loot(armory::Family::Sundry, seed, Odds::loot(depth))).any(|i| held_back(&i.base)))
    }

    #[test]
    fn no_loose_loot_is_a_held_back_base_smoke() {
        assert!(!loose_sundries_hold_back(10));
    }

    #[test]
    #[ignore = "slow sweep: 2000 seeds at each depth; run before merging"]
    fn no_loose_loot_is_a_held_back_base() {
        assert!(!loose_sundries_hold_back(2000));
    }

    #[test]
    fn a_torch_is_held_in_either_hand_the_off_hand_first_so_it_can_be_swung() {
        let torch = roll_item("Torch", 3).unwrap();
        assert_eq!(torch.slots(), vec![Slot::Offhand, Slot::Mainhand]);
    }

    #[test]
    fn every_base_rolls() {
        for (base, _) in ARMORY_BASES {
            for seed in [1, 2, 3, 1207, 65_537] {
                let item = roll_item(base, seed).unwrap_or_else(|| panic!("{base}"));
                assert_eq!(item, roll_item(base, seed).unwrap(), "{base} {seed}: not deterministic");
                assert!(!item.name.is_empty());
                for a in &item.affixes {
                    assert!(a.fits_item(&item.profile.tags, item.profile.slot) && a.least <= item.rarity, "{base}: {} on {:?}", a.name, item.profile.tags);
                }
                assert_eq!(item.legend.is_some(), item.rarity == Rarity::Legendary, "{base}");
                if item.profile.kind == Kind::Good {
                    assert_eq!((item.rarity, item.affixes.len()), (Rarity::Common, 0), "{base}");
                }
                if item.profile.tags.contains(&Tag::Container) {
                    assert_ne!(item.rarity, Rarity::Legendary, "{base}");
                }
            }
        }
        assert_eq!(roll_item("Spoon", 1), None);
    }

    #[test]
    fn every_magic_item_has_an_affix_that_does_something() {
        use armory::Family;
        let bases = [Family::Ring, Family::Amulet, Family::Charm, Family::Sword, Family::Garment].into_iter().flat_map(crate::armory_base::bases_of);
        for base in bases.chain(["Torch", "Belt", "Hood"]) {
            let magic: Vec<Item> = (0..120).filter_map(|seed| roll_item_with(base, seed, Odds::loot(1))).filter(|i| i.rarity == Rarity::Magic).collect();
            for item in &magic {
                assert!(item.affixes.iter().any(|a| a.in_play()), "{base} seed {}: magic with nothing in play: {:?}", item.seed, item.affixes.iter().map(|a| &a.name).collect::<Vec<_>>());
            }
        }
    }

    #[test]
    fn a_gleaming_light_reaches_further_and_a_smoky_one_less() {
        // A torch with that affix alone, since any rarity can also roll a flaw such as Smoky.
        let factor = |name: &str| (0..2000).filter_map(|seed| roll_item("Torch", seed)).find(|i| i.affixes.len() == 1 && i.affixes[0].name == name).map(|i| i.light_factor());
        assert_eq!(factor("Gleaming"), Some(1.5));
        assert_eq!(factor("Smoky"), Some(0.75));
        assert_eq!(factor("Long-burning"), Some(1.0));
    }

    #[test]
    fn the_same_base_seed_and_depth_roll_the_same_item() {
        for depth in [1, 3, 5, 8] {
            for seed in [1, 2, 1207, 65_537] {
                let item = roll_item_with("Sword", seed, Odds::loot(depth)).unwrap();
                assert_eq!(item, roll_item_with("Sword", seed, Odds::loot(depth)).unwrap(), "depth {depth} seed {seed}");
                assert_eq!(item.odds, Odds::loot(depth));
            }
        }
        for seed in 0..300 {
            assert_eq!(roll_item("Sword", seed), roll_item_with("Sword", seed, Odds::default()), "{seed}");
        }
    }

    /// A fresh torch whose `BurnTime` is `burn_time`, with the fuel that makes of a full one.
    fn torch_burning(burn_time: f32) -> Item {
        let mut item = roll_item("Spare torch", 3).unwrap();
        item.profile.stats.burn_time = burn_time;
        item.shape.scale_fuel(burn_time);
        item.shape.set_lit(true).unwrap();
        item
    }

    fn state(item: &Item) -> armory::families::torch::BurnState {
        let armory::Item::Torch(s) = &item.shape else { panic!("not a torch") };
        s.burn.state
    }

    #[test]
    fn a_plain_torch_lit_for_an_hour_is_spent() {
        use armory::families::torch::BurnState;
        let mut torch = torch_burning(1.0);
        assert_eq!(torch.fuel_left(), Some(3_600.0));
        torch.burn(3_599.0);
        assert_eq!(state(&torch), BurnState::Burning);
        torch.burn(1.0);
        assert_eq!((state(&torch), torch.fuel_left()), (BurnState::Spent, Some(0.0)));
    }

    #[test]
    fn a_long_burning_torch_burns_for_two_hours() {
        use armory::families::torch::BurnState;
        let mut torch = torch_burning(2.0);
        torch.burn(3_600.0);
        assert_eq!((state(&torch), torch.fuel_left()), (BurnState::Burning, Some(3_600.0)));
        torch.burn(3_600.0);
        assert_eq!(state(&torch), BurnState::Spent);
    }

    #[test]
    fn dousing_at_twenty_minutes_and_relighting_leaves_forty() {
        let mut torch = torch_burning(1.0);
        torch.burn(20.0 * 60.0);
        torch.shape.set_lit(false).unwrap();
        torch.burn(10.0 * 3_600.0);
        torch.shape.set_lit(true).unwrap();
        assert_eq!(torch.fuel_left(), Some(40.0 * 60.0));
    }

    #[test]
    fn the_char_follows_the_fuel_used() {
        let mut torch = torch_burning(1.0);
        let armory::Item::Torch(before) = &torch.shape else { panic!() };
        let before = before.burn.char;
        torch.burn(1_800.0);
        let armory::Item::Torch(after) = &torch.shape else { panic!() };
        assert!(after.burn.char > before && after.burn.char <= 1.0);
        assert!((after.burn.char - 0.5).abs() < 1e-3);
    }

    #[test]
    fn fuel_left_survives_a_serde_round_trip() {
        let mut torch = torch_burning(1.0);
        torch.burn(1_234.0);
        let json = serde_json::to_string(&torch.shape).unwrap();
        let back: armory::Item = serde_json::from_str(&json).unwrap();
        assert_eq!(back.fuel_left(), Some(3_600.0 - 1_234.0));
        let mut lantern = roll_item("Lantern", 7).unwrap();
        lantern.shape.set_lit(true).unwrap();
        lantern.burn(100.0);
        let back: armory::Item = serde_json::from_str(&serde_json::to_string(&lantern.shape).unwrap()).unwrap();
        assert_eq!(back.fuel_left(), lantern.fuel_left());
    }

    #[test]
    fn a_rolled_light_starts_with_its_burn_time_of_fuel() {
        let torch = roll_item("Spare torch", 3).unwrap();
        assert_eq!(torch.fuel_left(), Some(3_600.0));
        assert_eq!(torch.fuel_capacity(), Some(3_600.0));
        assert_eq!(roll_item("Sword", 1).unwrap().fuel_left(), None);
    }

    #[test]
    fn a_legend_in_a_world_names_that_worlds_factions() {
        let names: Vec<String> = ["the Scale Court", "the Ember Tithe", "the Low Choir"].iter().map(|s| s.to_string()).collect();
        let lore = Lore { factions: names.clone(), ..Lore::default() };
        let found: Vec<Item> = (1..2_000).filter_map(|seed| roll_item_with("Sword", seed, Odds::loot(8))).filter(|i| i.rarity == Rarity::Legendary).take(20).collect();
        assert!(found.len() >= 3);
        for item in found {
            let here = item.clone().in_world(&lore);
            let legend = here.legend.as_ref().unwrap();
            assert_eq!(legend.title, item.legend.as_ref().unwrap().title, "the title stays");
            assert!(names.iter().any(|n| legend.history[0].contains(&format!("by {n}."))), "{}", legend.history[0]);
            assert_eq!(here, item.clone().in_world(&lore), "deterministic");
            assert_eq!(item.clone().in_world(&Lore::default()), item, "no names leaves the grammar's");
            assert!(item.legend.as_ref().unwrap().history[0].contains(" by the "), "the grammar still names one");
        }
        let common = roll_item_with("Sword", 1, Odds::loot(1)).unwrap();
        assert_eq!(common.clone().in_world(&lore), common);
    }

    #[test]
    fn a_note_found_in_a_world_is_written_on_once() {
        let lore = Lore { depth: 4, creatures: vec![crate::notes::Thing::new("troll", "trolls")], ..Lore::default() };
        // A seed that is no rumour of the denizens ([`crate::tongues::rumour`]).
        let seed = (7..).find(|&s| crate::tongues::rumour(s, 4).is_none()).unwrap();
        let note = roll_item("Note", seed).unwrap();
        assert_eq!(note.words, None, "rolled outside a world it is blank");
        let found = note.clone().in_world(&lore);
        assert_eq!(found.words.as_deref(), Some(crate::notes::write_note(seed, &lore).as_str()));
        let elsewhere = found.clone().in_world(&Lore { depth: 9, ..Lore::default() });
        assert_eq!(elsewhere.words, found.words, "carried to another level, it keeps its words");
        for base in ["Scroll", "Book", "Sword"] {
            assert_eq!(roll_item(base, 7).unwrap().in_world(&lore).words, None, "{base}");
        }
    }

    #[test]
    fn affixes_show_in_name_stats_and_shape() {
        let (warm, base_warmth) = (1..2_000)
            .filter_map(|seed| {
                let item = roll_item("Cloak", seed)?;
                item.affixes.iter().any(|a| a.name == "Warm").then(|| {
                    let plain = crate::profile::profile("Cloak", &crate::armory_base::roll_shape("Cloak", seed).unwrap());
                    (item, plain.stats.warmth)
                })
            })
            .next()
            .expect("no warm cloak");
        assert!(warm.name.starts_with("Warm "));
        assert!(warm.profile.stats.warmth >= base_warmth + 2.0);

        let rusted = (1..3_000).filter_map(|seed| roll_item("Sword", seed)).find(|i| i.affixes.iter().any(|a| a.name == "Rusted")).expect("no rusted sword");
        assert_eq!(rusted.profile.stats.damage_bonus, -1.0);
        assert_eq!(rusted.shape.condition().rust, 1.5);
    }

    #[test]
    fn an_item_taken_apart_and_put_back_is_the_same() {
        for (base, seed) in [("Sword", 9), ("Cloak", 4), ("Purse", 11), ("Torch", 3), ("Sack", 3)] {
            let item = roll_item(base, seed).unwrap();
            assert_eq!(Item::from_parts(item.clone().into()), Some(item), "{base} {seed}");
        }
        let legend = (1..2_000).filter_map(|seed| roll_item_with("Sword", seed, Odds::loot(8))).find(|i| i.rarity == Rarity::Legendary).unwrap();
        assert_eq!(Item::from_parts(legend.clone().into()), Some(legend));
        assert_eq!(Item::from_parts(ItemParts { base: "Nothing".into(), ..roll_item("Sword", 1).unwrap().into() }), None);
    }

    #[test]
    fn changed_affixes_change_the_stats_and_name_parts_keep() {
        let mut parts = ItemParts::from(roll_item("Cloak", 4).unwrap());
        let plain = Item::from_parts(ItemParts { affixes: vec![], ..parts.clone() }).unwrap();
        let warm = crate::tables::affix_named("Warm").unwrap();
        parts.affixes = vec![warm];
        let item = Item::from_parts(parts).unwrap();
        assert_eq!(item.profile.stats.warmth, plain.profile.stats.warmth + 2.0);
        assert_eq!(crate::tables::affix_named("Nope"), None);
    }

    fn grid(item: &Item) -> (u8, u8) {
        item.profile.holds.as_ref().map(|g| (g.cols, g.rows)).unwrap()
    }

    #[test]
    fn a_satchel_of_holding_has_a_bigger_grid_and_a_cramped_one_a_smaller() {
        let plain = roll_item("Satchel", 3).unwrap();
        let with = |name: &str| {
            let mut parts = ItemParts::from(plain.clone());
            parts.affixes = vec![crate::tables::affix_named(name).unwrap()];
            Item::from_parts(parts).unwrap()
        };
        let base = grid(&Item::from_parts(ItemParts { affixes: vec![], ..plain.clone().into() }).unwrap());
        assert_eq!(base, (6, 3));
        assert_eq!(grid(&with("of Holding")), (6, 4));
        assert_eq!(grid(&with("of the Bottomless")), (6, 5));
        assert_eq!(grid(&with("Cramped")), (6, 2));
        assert_eq!(with("of Holding").profile.plain_grid(), Some((6, 3)));
        assert_eq!(Item::from_parts(ItemParts { affixes: vec![], ..plain.into() }).unwrap().profile.plain_grid(), None);
    }

    /// Rolls each of `bags` (a base and the seeds to roll it at) and checks its grid follows its
    /// capacity affixes; the capacity affixes that came up are marked in the result.
    fn capacity_rolls(bags: &[(&str, Vec<u32>)]) -> [bool; 3] {
        let mut seen = [false; 3];
        for (base, seeds) in bags {
            let plain_rows = grid(&Item::from_parts(ItemParts { affixes: vec![], ..roll_item(base, 1).unwrap().into() }).unwrap()).1 as i32;
            for &seed in seeds {
                let item = roll_item_with(base, seed, Odds::loot(6)).unwrap();
                let by = item.affixes.iter().map(|a| match a.name.as_str() {
                    "of Holding" => 1,
                    "of the Bottomless" => 2,
                    "Cramped" => -1,
                    _ => 0,
                }).sum::<i32>();
                assert_eq!(grid(&item).1 as i32, plain_rows + by, "{base} {seed}");
                assert!(item.affixes.iter().filter(|a| a.group.as_deref() == Some("capacity")).count() <= 1, "{base} {seed}");
                if item.affixes.iter().any(|a| a.name == "of the Bottomless") {
                    assert!(item.rarity >= Rarity::Rare);
                }
                for (i, n) in ["of Holding", "of the Bottomless", "Cramped"].into_iter().enumerate() {
                    seen[i] |= item.affixes.iter().any(|a| a.name == n);
                }
            }
        }
        seen
    }

    #[test]
    #[ignore = "slow sweep: 500 seeds of each of four bags; run before merging"]
    fn bags_roll_capacity_affixes_and_the_grid_follows_them() {
        let bags: Vec<_> = ["Pack", "Satchel", "Sack", "Belt"].into_iter().map(|b| (b, (1..500).collect())).collect();
        assert_eq!(capacity_rolls(&bags), [true; 3]);
    }

    #[test]
    fn bags_roll_capacity_affixes_and_the_grid_follows_them_smoke() {
        let bags = [("Pack", (1..30).collect())];
        assert_eq!(capacity_rolls(&bags), [true; 3]);
    }

    #[test]
    #[ignore = "slow sweep: 1000 seeds of a quiver and a purse; run before merging"]
    fn quivers_and_purses_roll_no_capacity() {
        no_capacity_on(1..1_000);
    }

    #[test]
    fn quivers_and_purses_roll_no_capacity_smoke() {
        no_capacity_on(1..30);
    }

    fn no_capacity_on(seeds: std::ops::Range<u32>) {
        for seed in seeds {
            for base in ["Quiver", "Purse"] {
                let item = roll_item_with(base, seed, Odds::loot(6)).unwrap();
                assert!(item.affixes.iter().all(|a| a.group.as_deref() != Some("capacity")), "{base} {seed}");
            }
        }
    }

    #[test]
    fn legendaries_have_titles() {
        let legend = (1..2_000).filter_map(|seed| roll_item_with("Sword", seed, Odds::loot(8))).find(|i| i.rarity == Rarity::Legendary).expect("no legendary sword");
        let l = legend.legend.as_ref().unwrap();
        assert_eq!(legend.title(), l.title);
        assert_eq!(l.subtitle, legend.name);
    }

    #[test]
    fn loot_rolls_a_base_of_its_family() {
        // A banner is scenery: no loot base names it.
        for &family in armory::Item::PORTED.iter().filter(|f| **f != armory::Family::Banner) {
            if crate::armory_base::bases_of(family).all(held_back) {
                continue;
            }
            for seed in [5, 1207] {
                let item = roll_loot(family, seed, Odds::default()).unwrap();
                assert_eq!(item.shape.family(), family);
                assert_eq!(Some(item.clone()), roll_loot(family, seed, Odds::default()));
            }
        }
    }

    /// The share of gear that rolls rare or legendary as floor loot at `depth`, over the
    /// families quickest to roll and 150 seeds each.
    fn rare_share_of_loot(depth: u32) -> f64 {
        use armory::Family::{Amulet, Axe, Charm, Club, Dagger, Mace, Ring};
        let gear: Vec<Item> = [Mace, Club, Axe, Dagger, Ring, Charm, Amulet]
            .into_iter()
            .flat_map(|family| (0..150).filter_map(move |seed| roll_loot(family, seed, Odds::loot(depth))))
            .filter(|i| i.profile.kind == Kind::Gear)
            .collect();
        gear.iter().filter(|i| i.rarity >= Rarity::Rare).count() as f64 / gear.len() as f64
    }

    #[test]
    fn floor_loot_grows_rarer_with_depth() {
        let shares: Vec<f64> = [1, 3, 5].into_iter().map(rare_share_of_loot).collect();
        for pair in shares.windows(2) {
            assert!(pair[1] > pair[0], "rare and legendary by depth: {shares:?}");
        }
        assert!(shares[2] > 1.7 * shares[0], "rare and legendary by depth: {shares:?}");
    }

    #[test]
    fn every_weapon_goes_in_either_hand_and_nothing_else_does() {
        for base in ["Sword", "Dagger", "Mace", "Club", "Hand axe", "Spear", "Staff", "Bow", "Crossbow"] {
            for seed in [1, 7, 1207] {
                let item = roll_item(base, seed).unwrap();
                assert_eq!(item.slots(), vec![Slot::Mainhand, Slot::Offhand], "{base} {seed}");
                assert!(item.fits(Slot::Offhand) && !item.fits(Slot::Belt), "{base} {seed}");
            }
        }
        assert_eq!(roll_item("Shield", 1).unwrap().slots(), vec![Slot::Offhand]);
        assert_eq!(roll_item("Helm", 1).unwrap().slots(), vec![Slot::Head]);
        assert_eq!(roll_item("Ring", 1).unwrap().slots(), vec![Slot::Ring1, Slot::Ring2]);
        assert!(roll_item("Bread", 1).unwrap().slots().is_empty());
        assert_eq!(roll_item("Lantern", 1).unwrap().slots(), vec![Slot::Offhand, Slot::Mainhand, Slot::Hip]);
        for seed in 1..40 {
            let mut candle = roll_item("Candle", seed).unwrap();
            for holder in [false, true] {
                if let armory::Item::Sundry(s) = &mut candle.shape {
                    s.article.holder = holder;
                }
                assert_eq!(candle.slots(), vec![Slot::Offhand, Slot::Mainhand], "{seed} holder {holder}");
                assert!(candle.fits(Slot::Offhand) && candle.fits(Slot::Mainhand));
                assert!(!candle.fits(Slot::Hip), "the hip stays the lantern's: {seed} holder {holder}");
            }
        }
        assert!(roll_item("Rope", 1).unwrap().slots().is_empty());
        assert!(!roll_item("Torch", 1).unwrap().fits(Slot::Hip));
        assert_eq!(roll_item("Quiver", 1).unwrap().slots(), vec![Slot::Quiver]);
        assert_eq!(roll_item("Bolt case", 1).unwrap().slots(), vec![Slot::Quiver]);
    }

    #[test]
    fn purses_hold_coins() {
        for seed in 1..50 {
            let purse = roll_item("Purse", seed).unwrap();
            let coins = purse.coins.expect("a purse always holds coins");
            assert!(coins.gp < 14 && coins.sp < 30 && (5..65).contains(&coins.cp));
            assert_eq!(purse.profile.description, coins.to_string());
        }
    }
}
