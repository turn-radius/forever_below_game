//! What a trader has on the shelf: wares rolled from their line of work, their grade and the
//! level's depth, each with a maker's line. Pure in `(shop, depth, seed)`; the game rolls it
//! from a trader's spot seed and the game day, so a shelf restocks daily. See `docs/items.md`,
//! "Stock". (`fb_faction::Trade::shop` builds a [`Shop`] from a folk's trade; the types are
//! mirrored here because `fb_faction` already sits above this crate.)

use crate::{Item, Odds, Rarity, dress, roll_item_with};
use armory::cond::{Condition, ConditionState};
use procgen_rng::Mulberry32;

/// What a trader sells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Line {
    /// Body armour, helms, coifs, gauntlets, boots and shields.
    Armour,
    /// Swords, daggers, maces, clubs, axes, spears and staves.
    Weapons,
    /// Bows, crossbows, arrows, bolts, quivers and bolt cases.
    Bows,
    /// Tents, bedrolls, campfire kits and tinderboxes.
    Camp,
    /// Lanterns, torches, lamp oil, food, drink, waterskins, rope, bags and pouches.
    General,
    /// Potions, healing draughts and elixirs.
    Potions,
    /// Incense and censers: a healer's wares (prayer scrolls come with the healers' later work).
    Sacred,
}

/// How skilled the trader is, which caps what rarity their wares come in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Grade {
    Novice,
    Apprentice,
    Expert,
    Master,
}

impl Grade {
    pub fn label(self) -> &'static str {
        match self {
            Grade::Novice => "novice",
            Grade::Apprentice => "apprentice",
            Grade::Expert => "expert",
            Grade::Master => "master",
        }
    }

    /// The best rarity their wares come in: novices' common, apprentices' magic, and rare for
    /// experts and masters, never legendary.
    pub fn cap(self) -> Rarity {
        match self {
            Grade::Novice => Rarity::Common,
            Grade::Apprentice => Rarity::Magic,
            Grade::Expert | Grade::Master => Rarity::Rare,
        }
    }

    /// How many levels deeper their wares roll than the level, so a master's rare pieces come
    /// at better odds (our own).
    pub fn bonus_depth(self) -> u32 {
        match self {
            Grade::Novice | Grade::Apprentice => 0,
            Grade::Expert => 1,
            Grade::Master => 3,
        }
    }

    /// The wear a piece leaves the bench with, `(least, most)`: a novice's work shows its
    /// making, a master's hardly at all (our own). Every piece is otherwise new.
    pub fn finish_wear(self) -> (f32, f32) {
        match self {
            Grade::Novice => (0.10, 0.20),
            Grade::Apprentice => (0.02, 0.06),
            Grade::Expert | Grade::Master => (0.0, 0.02),
        }
    }

    /// How many pieces are on the shelf, inclusive (our own).
    pub fn size(self) -> (u32, u32) {
        match self {
            Grade::Novice => (3, 4),
            Grade::Apprentice => (5, 6),
            Grade::Expert => (7, 8),
            Grade::Master => (9, 10),
        }
    }
}

/// What a smith specialises in: three in four of their pieces are of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lean {
    Padded,
    Leather,
    Mail,
    Scale,
    Plate,
    Blades,
    AxesAndClubs,
    Maces,
    SpearsAndStaves,
}

impl Lean {
    /// The bases of this style (our own grouping).
    pub fn bases(self) -> &'static [&'static str] {
        match self {
            Lean::Padded => &["Gambeson"],
            Lean::Leather => &["Leather jerkin", "Boots"],
            Lean::Mail => &["Mail", "Mail coif"],
            Lean::Scale => &["Scale coat", "Brigandine"],
            Lean::Plate => &["Cuirass", "Helm", "Gauntlets"],
            Lean::Blades => &["Sword", "Dagger"],
            Lean::AxesAndClubs => &["Hand axe", "Club"],
            Lean::Maces => &["Mace"],
            Lean::SpearsAndStaves => &["Spear", "Staff"],
        }
    }

    /// The trade's noun in our own words: "mailwright", "bladesmith".
    fn noun(self) -> &'static str {
        match self {
            Lean::Padded => "quilter",
            Lean::Leather => "leatherworker",
            Lean::Mail => "mailwright",
            Lean::Scale => "scalesmith",
            Lean::Plate => "platesmith",
            Lean::Blades => "bladesmith",
            Lean::AxesAndClubs => "axewright",
            Lean::Maces => "macesmith",
            Lean::SpearsAndStaves => "spearwright",
        }
    }
}

/// A trader as the shelf sees them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shop {
    pub line: Line,
    pub grade: Grade,
    /// A smith's specialty.
    pub lean: Option<Lean>,
    /// Their name, as the maker's line has it.
    pub maker: String,
    /// The faction they belong to, as its name reads ("the Ash Choir").
    pub house: String,
}

impl Line {
    /// Every base this line sells, held-back ones too: traders are where those come from.
    pub fn bases(self) -> &'static [&'static str] {
        match self {
            Line::Armour => &["Gambeson", "Leather jerkin", "Mail", "Scale coat", "Brigandine", "Cuirass", "Helm", "Mail coif", "Gauntlets", "Boots", "Shield"],
            Line::Weapons => &["Sword", "Dagger", "Mace", "Club", "Hand axe", "Spear", "Staff"],
            Line::Bows => &["Bow", "Crossbow", "Arrow", "Bolt", "Quiver", "Bolt case"],
            Line::Camp => &["Tent", "Bedroll", "Campfire kit", "Tinderbox"],
            Line::General => &[
                "Lantern", "Torch", "Spare torch", "Lamp oil", "Candle", "Bread", "Cheese", "Smoked fish", "Apple", "Meat pie", "Sausages", "Tankard", "Wine", "Waterskin",
                "Rope", "Pack", "Satchel", "Sack", "Purse",
            ],
            Line::Potions => &["Vial", "Small potion", "Potion", "Large potion", "Healing vial", "Small healing draught", "Healing draught", "Large healing draught", "Elixir of the Hare", "Elixir of Vigil", "Elixir of Fasting", "Elixir of Embers"],
            Line::Sacred => &["Incense", "Censer"],
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Line::Armour => "armorer",
            Line::Weapons => "weaponsmith",
            Line::Bows => "bowyer",
            Line::Camp => "tentmaker",
            Line::General => "general goods merchant",
            Line::Potions => "alchemist",
            Line::Sacred => "healer",
        }
    }
}

impl Shop {
    /// The style bases this shop leans to: the lean's, where they are in its line.
    fn own_bases(&self) -> Vec<&'static str> {
        self.lean.map_or_else(Vec::new, |l| l.bases().iter().copied().filter(|b| self.line.bases().contains(b)).collect())
    }

    /// The maker's line every piece carries: "Made by Bera, master mailwright of the Ash
    /// Choir." A general goods merchant sells what others made, so "Sold by".
    pub fn maker_line(&self) -> String {
        let noun = match self.lean {
            Some(lean) if !self.own_bases().is_empty() => lean.noun(),
            _ => self.line.noun(),
        };
        let verb = if self.line == Line::General { "Sold" } else { "Made" };
        format!("{verb} by {}, {} {noun} of {}.", self.maker, self.grade.label(), self.house)
    }

    /// The maker's line for a piece made to order: "Made by Oda, apprentice weaponsmith of the
    /// Brothers of the Last Door, for Wren."
    pub fn made_for_line(&self, delver: &str) -> String {
        let line = self.maker_line();
        format!("{}, for {delver}.", line.trim_end_matches('.'))
    }
}

/// Whether an affix is a flaw or ages the piece: nothing flawed is sold new.
pub(crate) fn flawed(item: &Item) -> bool {
    let pristine = Condition::of(ConditionState::Pristine);
    item.affixes.iter().any(|a| {
        let mut c = pristine;
        a.apply_to_condition(&mut c);
        !a.good || c != pristine
    })
}

/// The wares on `shop`'s shelf for `seed` at a level `depth` down, in shelf order. Grows with
/// the grade; rarity is capped by it ([`Grade::cap`]); a smith's own style is three pieces in
/// four. Each piece carries the maker's line in its history. Rolls meshes, so keep counts
/// modest.
pub fn stock(shop: &Shop, depth: u32, seed: u32) -> Vec<Item> {
    stock_with(shop, depth, seed, &[])
}

/// [`stock`], with the prayers a healer may sell: each a Divine term id and the mana it costs,
/// from the game's rules. A healer keeps one to three scrolls of distinct terms from them; empty,
/// none, and no other shop sells any. The game prices a scroll by its term's cost
/// ([`crate::value::prayer_factor`]).
pub fn stock_with(shop: &Shop, depth: u32, seed: u32, prayers: &[String]) -> Vec<Item> {
    if shop.line == Line::Sacred {
        let mut wares = sacred_stock(shop, depth, seed);
        wares.extend(prayer_wares(shop, seed, prayers));
        return wares;
    }
    let mut rng = Mulberry32::new(seed ^ 0x570C_45E1);
    let (lo, hi) = shop.grade.size();
    let count = rng.int(lo, hi) as usize;
    let odds = Odds::loot(depth + shop.grade.bonus_depth()).capped(shop.grade.cap());
    let own = shop.own_bases();
    // The rest cycle through a shuffled line so a shelf does not repeat itself soon.
    let mut order: Vec<&str> = shop.line.bases().to_vec();
    for i in (1..order.len()).rev() {
        order.swap(i, rng.int(0, i as u32) as usize);
    }
    let mut others = 0;
    let maker = shop.maker_line();
    let mut wares = Vec::with_capacity(count);
    for i in 0..count {
        let base = if !own.is_empty() && i % 4 != 3 {
            *rng.pick(&own)
        } else {
            others += 1;
            order[(others - 1) % order.len()]
        };
        let piece_seed = seed.wrapping_mul(0x9E37_79B1).wrapping_add((i as u32 + 1).wrapping_mul(0x85EB_CA6B));
        if let Some(mut item) = (0..6).filter_map(|k| roll_item_with(base, piece_seed.wrapping_add(k), odds)).find(|i| !flawed(i)) {
            let c = dress::condition_mut(&mut item.shape);
            *c = Condition::of(ConditionState::Pristine);
            c.state = ConditionState::Custom;
            c.wear = rng.range_f32(shop.grade.finish_wear().0, shop.grade.finish_wear().1);
            item.history.push(maker.clone());
            wares.push(item);
        }
    }
    wares
}

/// How many censers and dishes of incense a healer of `grade` keeps, as `(censers, dishes)`
/// (our own): every grade shows all three incenses and two censers at least, and finer healers
/// keep more of each.
fn sacred_counts(grade: Grade, rng: &mut Mulberry32) -> (usize, usize) {
    let (censers, dishes) = match grade {
        Grade::Novice => (2, (3, 4)),
        Grade::Apprentice => (2, (5, 5)),
        Grade::Expert => (3, (6, 7)),
        Grade::Master => (4, (8, 9)),
    };
    (censers, rng.int(dishes.0, dishes.1) as usize)
}

/// The prayer scrolls on a healer's shelf: one to three, distinct terms from `prayers`, rolled on
/// a stream of their own so the incense and censers stay as they were.
fn prayer_wares(shop: &Shop, seed: u32, prayers: &[String]) -> Vec<Item> {
    let mut rng = Mulberry32::new(seed ^ 0x9A3E_C0DE);
    let mut order: Vec<&String> = prayers.iter().collect();
    for i in (1..order.len()).rev() {
        order.swap(i, rng.int(0, i as u32) as usize);
    }
    let count = (rng.int(1, 3) as usize).min(order.len());
    let maker = shop.maker_line();
    order
        .into_iter()
        .take(count)
        .enumerate()
        .filter_map(|(i, term)| {
            let mut scroll = crate::prayers::prayer_scroll(seed.wrapping_mul(0xC2B2_AE35).wrapping_add(i as u32 + 1), term)?;
            let c = dress::condition_mut(&mut scroll.shape);
            *c = Condition::of(ConditionState::Pristine);
            scroll.history.push(maker.clone());
            Some(scroll)
        })
        .collect()
}

/// A healer's shelf: always every kind of incense and at least two censers, however the dice
/// fall. The kinds go round in a seeded order so the extra dishes repeat evenly.
fn sacred_stock(shop: &Shop, depth: u32, seed: u32) -> Vec<Item> {
    use armory::incense::IncenseKind;
    let mut rng = Mulberry32::new(seed ^ 0x570C_45E1);
    let (censers, dishes) = sacred_counts(shop.grade, &mut rng);
    let odds = Odds::loot(depth + shop.grade.bonus_depth()).capped(shop.grade.cap());
    let mut kinds = IncenseKind::ALL;
    for i in (1..kinds.len()).rev() {
        kinds.swap(i, rng.int(0, i as u32) as usize);
    }
    let maker = shop.maker_line();
    let mut wares = Vec::with_capacity(censers + dishes);
    let plan = (0..dishes).map(|n| ("Incense", Some(kinds[n % kinds.len()]))).chain((0..censers).map(|_| ("Censer", None)));
    for (i, (base, kind)) in plan.enumerate() {
        let piece_seed = seed.wrapping_mul(0x9E37_79B1).wrapping_add((i as u32 + 1).wrapping_mul(0x85EB_CA6B));
        let fits = |item: &Item| (kind.is_none() || item.incense_kind() == kind) && !flawed(item);
        let rolled = (0..48).filter_map(|k| roll_item_with(base, piece_seed.wrapping_add(k), odds)).find(fits);
        // Plain common stock if the dice never gave one: the shelf must not come up short.
        let rolled = rolled.or_else(|| (48..96).filter_map(|k| roll_item_with(base, piece_seed.wrapping_add(k), Odds::default().capped(Rarity::Common))).find(fits));
        if let Some(mut item) = rolled {
            let c = dress::condition_mut(&mut item.shape);
            *c = Condition::of(ConditionState::Pristine);
            c.state = ConditionState::Custom;
            c.wear = rng.range_f32(shop.grade.finish_wear().0, shop.grade.finish_wear().1);
            item.history.push(maker.clone());
            wares.push(item);
        }
    }
    wares
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shop(line: Line, grade: Grade, lean: Option<Lean>) -> Shop {
        Shop { line, grade, lean, maker: "Bera".into(), house: "the Ash Choir".into() }
    }

    #[test]
    fn the_grade_caps_rarity() {
        for (grade, cap) in [(Grade::Novice, Rarity::Common), (Grade::Apprentice, Rarity::Magic), (Grade::Expert, Rarity::Rare), (Grade::Master, Rarity::Rare)] {
            let mut best = Rarity::Common;
            for seed in 0..4 {
                for item in stock(&shop(Line::Armour, grade, None), 12, seed) {
                    assert!(item.rarity <= cap, "{} {:?}", grade.label(), item.rarity);
                    best = best.max(item.rarity);
                }
            }
            assert!(grade == Grade::Novice || best > Rarity::Common, "{} sells nothing above common deep down", grade.label());
        }
    }

    #[test]
    fn a_master_mailwright_sells_mostly_mail_and_never_legendary() {
        let mut mail = 0;
        let mut all = 0;
        for seed in 0..4 {
            for item in stock(&shop(Line::Armour, Grade::Master, Some(Lean::Mail)), 8, seed) {
                assert_ne!(item.rarity, Rarity::Legendary);
                all += 1;
                mail += usize::from(Lean::Mail.bases().contains(&item.base.as_str()));
            }
        }
        assert!(mail * 10 >= all * 7, "{mail} of {all} are mail");
    }

    #[test]
    fn every_piece_has_the_maker_line() {
        let items = stock(&shop(Line::Armour, Grade::Master, Some(Lean::Mail)), 8, 3);
        let line = "Made by Bera, master mailwright of the Ash Choir.";
        assert!(items.iter().all(|i| i.history.last().map(String::as_str) == Some(line)));
        let goods = stock(&shop(Line::General, Grade::Novice, None), 1, 3);
        assert_eq!(goods[0].history, vec!["Sold by Bera, novice general goods merchant of the Ash Choir.".to_string()]);
    }

    #[test]
    fn stock_grows_with_the_grade() {
        let sizes: Vec<usize> = [Grade::Novice, Grade::Apprentice, Grade::Expert, Grade::Master].iter().map(|g| stock(&shop(Line::Camp, *g, None), 3, 5).len()).collect();
        assert!(sizes.windows(2).all(|w| w[0] < w[1]), "{sizes:?}");
    }

    #[test]
    fn camp_goods_and_elixirs_are_stocked_though_held_back_from_loot() {
        assert!(Line::Camp.bases().iter().chain(Line::Potions.bases()).any(|b| crate::armory_base::held_back(b)));
        for item in stock(&shop(Line::Potions, Grade::Master, None), 8, 2) {
            assert!(Line::Potions.bases().contains(&item.base.as_str()));
        }
    }

    #[test]
    fn pieces_leave_the_bench_new_and_unflawed() {
        let mut max = [0.0f32; 2];
        for (n, grade) in [Grade::Novice, Grade::Master].into_iter().enumerate() {
            for seed in 0..4 {
                for item in stock(&shop(Line::Armour, grade, Some(Lean::Mail)), 8, seed) {
                    let c = item.shape.condition();
                    assert!(c.wear >= grade.finish_wear().0 && c.wear <= grade.finish_wear().1, "{} wear {}", item.title(), c.wear);
                    assert!((c.rust, c.damage, c.broken) == (0.0, 0.0, false), "{}", item.title());
                    assert!(item.affixes.iter().all(|a| a.good), "{} is flawed", item.title());
                    max[n] = max[n].max(c.wear);
                }
            }
        }
        assert!(max[0] >= max[1], "{max:?}");
    }

    #[test]
    fn every_named_base_rolls() {
        let lines = [Line::Armour, Line::Weapons, Line::Bows, Line::Camp, Line::General, Line::Potions];
        let leans = [Lean::Padded, Lean::Leather, Lean::Mail, Lean::Scale, Lean::Plate, Lean::Blades, Lean::AxesAndClubs, Lean::Maces, Lean::SpearsAndStaves];
        for base in lines.iter().flat_map(|l| l.bases()).chain(leans.iter().flat_map(|l| l.bases())) {
            assert!(roll_item_with(base, 1, Odds::default()).is_some(), "{base} rolls nothing");
        }
    }

    fn sacred_shelf(grade: Grade, seed: u32) -> Vec<Item> {
        stock(&shop(Line::Sacred, grade, None), 4, seed)
    }

    #[test]
    fn a_healer_always_shows_every_incense_and_two_censers() {
        for grade in [Grade::Novice, Grade::Apprentice, Grade::Expert, Grade::Master] {
            for seed in 0..40 {
                let wares = sacred_shelf(grade, seed);
                let mut kinds: Vec<_> = wares.iter().filter_map(Item::incense_kind).map(|k| k.name()).collect();
                kinds.sort();
                kinds.dedup();
                assert_eq!(kinds.len(), 3, "{} seed {seed}: {kinds:?}", grade.label());
                assert!(wares.iter().filter(|i| i.is_censer()).count() >= 2, "{} seed {seed}", grade.label());
                assert!(wares.iter().all(|i| Line::Sacred.bases().contains(&i.base.as_str())));
            }
        }
    }

    #[test]
    fn a_finer_healer_keeps_more_and_finer_wares() {
        let grades = [Grade::Novice, Grade::Apprentice, Grade::Expert];
        let counts: Vec<(usize, usize)> = grades
            .iter()
            .map(|&g| (0..12).map(|s| sacred_shelf(g, s)).fold((usize::MAX, usize::MAX), |(a, b), w| (a.min(w.iter().filter(|i| i.is_censer()).count()), b.min(w.len()))))
            .collect();
        assert!(counts.windows(2).all(|w| w[0].0 <= w[1].0 && w[0].1 < w[1].1), "{counts:?}");
        let best = |g: Grade| (0..12).flat_map(|s| sacred_shelf(g, s)).map(|i| i.rarity).max().unwrap();
        assert!(best(Grade::Novice) == Rarity::Common && best(Grade::Expert) >= best(Grade::Apprentice), "rarity");
    }

    #[test]
    fn only_healers_stock_censers_and_incense() {
        for line in [Line::Armour, Line::Weapons, Line::Bows, Line::Camp, Line::General, Line::Potions] {
            assert!(!line.bases().iter().any(|b| *b == "Censer" || *b == "Incense"), "{line:?}");
        }
        for seed in 0..3 {
            for item in stock(&shop(Line::General, Grade::Master, None), 8, seed) {
                assert!(!item.is_censer() && item.incense_kind().is_none());
            }
        }
    }

    fn pool() -> Vec<String> {
        ["cone", "cure", "heal"].map(String::from).to_vec()
    }

    #[test]
    fn a_healer_sells_one_to_three_prayer_scrolls_of_distinct_terms() {
        let s = shop(Line::Sacred, Grade::Expert, None);
        let mut counts = [0; 4];
        for seed in 0..60 {
            let scrolls: Vec<_> = stock_with(&s, 4, seed, &pool()).into_iter().filter_map(|i| i.prayer().map(str::to_string)).collect();
            assert!((1..=3).contains(&scrolls.len()), "seed {seed}: {scrolls:?}");
            let mut distinct = scrolls.clone();
            distinct.sort();
            distinct.dedup();
            assert_eq!(distinct.len(), scrolls.len());
            assert!(scrolls.iter().all(|t| pool().contains(t)));
            counts[scrolls.len()] += 1;
        }
        assert!(counts[1] > 0 && counts[2] > 0 && counts[3] > 0, "{counts:?}");
    }

    #[test]
    fn an_empty_pool_and_other_shops_sell_no_prayers_and_incense_is_unchanged() {
        let s = shop(Line::Sacred, Grade::Master, None);
        for seed in 0..12 {
            assert!(stock_with(&s, 4, seed, &[]).iter().all(|i| i.prayer().is_none()));
            assert_eq!(stock_with(&s, 4, seed, &[]), sacred_shelf(Grade::Master, seed));
            let with: Vec<_> = stock_with(&s, 4, seed, &pool()).into_iter().filter(|i| i.prayer().is_none()).collect();
            assert_eq!(with, sacred_shelf(Grade::Master, seed));
            assert!(stock_with(&shop(Line::Armour, Grade::Master, None), 4, seed, &pool()).iter().all(|i| i.prayer().is_none()));
        }
    }

    #[test]
    fn a_healers_shelf_with_prayers_is_pure() {
        let s = shop(Line::Sacred, Grade::Apprentice, None);
        assert_eq!(stock_with(&s, 4, 9, &pool()), stock_with(&s, 4, 9, &pool()));
    }

    #[test]
    fn a_healers_shelf_is_pure_in_its_arguments() {
        let s = shop(Line::Sacred, Grade::Apprentice, None);
        assert_eq!(stock(&s, 4, 9), stock(&s, 4, 9));
        assert_ne!(stock(&s, 4, 9), stock(&s, 4, 10));
    }

    #[test]
    fn stock_is_pure_in_its_arguments() {
        let s = shop(Line::Weapons, Grade::Expert, Some(Lean::Blades));
        assert_eq!(stock(&s, 4, 9), stock(&s, 4, 9));
        assert_ne!(stock(&s, 4, 9), stock(&s, 4, 10));
    }
}
