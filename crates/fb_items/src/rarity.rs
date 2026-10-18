//! Rarity and the affixes that come with it: the plate's `rollAffixes` and `pickAffix`
//! (`reference/forever-below-plate.html`; rules in `docs/items.md`, "Rarity and affixes"),
//! with odds that improve with depth ([`Odds`], our own).

use crate::tables::{Affix, AffixKind, affixes};
use crate::{Rarity, Slot, Tag};
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

/// The rarity thresholds on the first level: below 0.83 common, below 0.95 magic, otherwise
/// rare, never legendary. Our own, lower than the plate's (0.58, 0.85, 0.97), so the first
/// level gives out less.
const SHALLOW: [f64; 3] = [0.83, 0.95, 1.0];
/// The thresholds the odds tend to far below: 7% common, 33% magic, 48% rare, 12% legendary.
/// Our own proposal.
const DEEP: [f64; 3] = [0.07, 0.40, 0.88];
/// How far toward [`DEEP`] the odds are four levels down. Our own proposal.
const FOUR_LEVELS_DOWN: f64 = 1.0 / 3.0;

/// What a rarity roll is drawn against: how deep the find is, and the best rarity it may be.
///
/// `depth` is an effective depth, not always the level's: a find that should roll better
/// than its level (a locked room's, say) passes a deeper one, such as `depth + 2`. Kept on
/// the item, so its base and seed roll it again the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Odds {
    pub depth: u32,
    pub cap: Rarity,
}

impl Default for Odds {
    /// The first level's loot.
    fn default() -> Odds {
        Odds::loot(1)
    }
}

impl Odds {
    /// Loot found at `depth`, on the floor or in a container: at most rare on the first level,
    /// uncapped below it. The cap holds however much deeper the find rolls ([`Odds::deeper`]),
    /// so nothing found on the first level is legendary.
    pub fn loot(depth: u32) -> Odds {
        let cap = if depth <= 1 { Rarity::Rare } else { Rarity::Legendary };
        Odds { depth, cap }
    }

    /// These odds rolled `levels` deeper, keeping their cap: a find that should roll better
    /// than its level, such as a jackalope's drop.
    pub fn deeper(self, levels: u32) -> Odds {
        Odds { depth: self.depth + levels, ..self }
    }

    /// These odds for a finder with `luck` (the worn affixes' [`crate::stats::Stat::Luck`]):
    /// each whole point rolls one level deeper, keeping the cap.
    pub fn with_luck(self, luck: f32) -> Odds {
        self.deeper(luck.max(0.0).floor() as u32)
    }

    /// The gear folk and the risen dead carry at `depth`: as loot, but common or magic only on
    /// the first level and at most rare on the second, so that they grow better equipped as one
    /// descends.
    pub fn kit(depth: u32) -> Odds {
        let cap = match depth {
            0 | 1 => Rarity::Magic,
            2 => Rarity::Rare,
            _ => Rarity::Legendary,
        };
        Odds::loot(depth).capped(cap)
    }

    /// These odds at most `cap`.
    pub fn capped(self, cap: Rarity) -> Odds {
        Odds { cap: self.cap.min(cap), ..self }
    }

    /// The upper bounds of common, magic and rare for a uniform roll. Depth 1 is
    /// [`SHALLOW`]; each level deeper moves them along a curve that never quite reaches
    /// [`DEEP`], a third of the way there four levels down (`1 - (2/3)^((depth - 1) / 4)`).
    /// The curve is our own proposal; no source.
    pub fn thresholds(&self) -> [f64; 3] {
        let below = self.depth.saturating_sub(1) as f64;
        let t = 1.0 - (1.0 - FOUR_LEVELS_DOWN).powf(below / 4.0);
        std::array::from_fn(|i| SHALLOW[i] + (DEEP[i] - SHALLOW[i]) * t)
    }

    /// The share of rolls that come up each rarity, common first, before any cap.
    pub fn shares(&self) -> [f64; 4] {
        let [c, m, r] = self.thresholds();
        [c, m - c, r - m, 1.0 - r]
    }
}

impl Rarity {
    /// The rarity for a uniform roll `x` in `[0, 1)` at the first level: below 0.83 common,
    /// below 0.95 magic, otherwise rare.
    pub fn from_roll(x: f64) -> Rarity {
        Rarity::from_roll_at(x, &Odds::default())
    }

    /// The rarity for a uniform roll `x` in `[0, 1)` against `odds`, at most its cap.
    pub fn from_roll_at(x: f64, odds: &Odds) -> Rarity {
        let [c, m, r] = odds.thresholds();
        let rarity = match x {
            x if x < c => Rarity::Common,
            x if x < m => Rarity::Magic,
            x if x < r => Rarity::Rare,
            _ => Rarity::Legendary,
        };
        rarity.min(odds.cap)
    }

    /// One draw from `rng` against `odds`. Containers cap at rare.
    pub fn roll(rng: &mut Mulberry32, tags: &[Tag], odds: &Odds) -> Rarity {
        let odds = if tags.contains(&Tag::Container) { odds.capped(Rarity::Rare) } else { *odds };
        Rarity::from_roll_at(rng.next_f64(), &odds)
    }

    /// The rarity hand-picked affixes imply: two good ones make it rare, one magic, none
    /// common (the plate's armory, when affixes are chosen rather than rolled).
    pub fn of_affixes(chosen: &[&Affix]) -> Rarity {
        match chosen.iter().filter(|a| a.good).count() {
            0 => Rarity::Common,
            1 => Rarity::Magic,
            _ => Rarity::Rare,
        }
    }
}

/// A uniform pick among the affixes of `kind` and goodness that fit `tags`, are not already
/// `taken` and are in play ([`Affix::in_play`]); `None` when none fit (`pickAffix`).
///
/// The draw is made against every fitting affix, held back or not, as it was before any were
/// held back, so an item that rolled an affix in play still rolls the same one. A draw that lands
/// on a held-back affix takes the same place among those in play instead, and with none in play
/// the slot stays empty. When a held-back affix comes back into play, only the items that land
/// on it change.
///
/// Only affixes for an item of `rarity` or better ([`Affix::least`]) that fit its tags and slot
/// ([`Affix::fits_item`]) and do not share a group with one `taken` ([`Affix::stacks_with`]) are
/// in the pool.
pub fn pick_affix(rng: &mut Mulberry32, item: Rolling, kind: AffixKind, good: bool, taken: &[&Affix]) -> Option<&'static Affix> {
    let pool: Vec<&'static Affix> = affixes()
        .iter()
        .filter(|a| a.kind == kind && a.good == good && a.least <= item.rarity && a.fits_item(item.tags, item.slot) && taken.iter().all(|t| a.stacks_with(t)))
        .collect();
    if pool.is_empty() {
        return None;
    }
    let x = rng.next_f64();
    let at = |len: usize| ((x * len as f64).floor() as usize).min(len - 1);
    let drawn = pool[at(pool.len())];
    if drawn.in_play() {
        return Some(drawn);
    }
    let rolling: Vec<&'static Affix> = pool.into_iter().filter(|a| a.in_play()).collect();
    (!rolling.is_empty()).then(|| rolling[at(rolling.len())])
}

/// What an affix is being rolled for: the item's tags and slot, and the rarity it came up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rolling<'a> {
    pub tags: &'a [Tag],
    pub slot: Option<Slot>,
    pub rarity: Rarity,
}

/// [`roll_affixes_for`] an item with `tags` in no particular slot.
pub fn roll_affixes(rng: &mut Mulberry32, rarity: Rarity, tags: &[Tag]) -> Vec<&'static Affix> {
    roll_affixes_for(rng, rarity, tags, None)
}

/// The chance an item of each rarity, common first, rolls a flaw prefix besides its good affixes:
/// the plate's 30% for common, and our own, falling with rarity, for the rest.
const FLAW_CHANCE: [f64; 4] = [0.3, 0.2, 0.1, 0.05];

/// The affixes a `rarity` rolls on an item with `tags` worn in `slot`. Common: no good affix.
/// Magic: one good prefix or suffix, even odds, or the other when nothing of the one drawn fits
/// (a ring has no prefix), so a magic item always does something. Rare: a good prefix and a good
/// suffix. Legendary: a good prefix and two good suffixes. Then a flaw prefix, at
/// [`FLAW_CHANCE`] for the rarity, after the good ones so it never takes their place or their
/// name, and never of a group they have. Slots with nothing that fits stay empty.
pub fn roll_affixes_for(rng: &mut Mulberry32, rarity: Rarity, tags: &[Tag], slot: Option<Slot>) -> Vec<&'static Affix> {
    use AffixKind::{Prefix, Suffix};
    let good: Vec<AffixKind> = match rarity {
        Rarity::Common => vec![],
        Rarity::Magic => vec![if rng.chance(0.5) { Prefix } else { Suffix }],
        Rarity::Rare => vec![Prefix, Suffix],
        Rarity::Legendary => vec![Prefix, Suffix, Suffix],
    };
    let rolling = Rolling { tags, slot, rarity };
    let mut chosen = Vec::new();
    for kind in good {
        let picked = pick_affix(rng, rolling, kind, true, &chosen).or_else(|| {
            let other = if kind == Prefix { Suffix } else { Prefix };
            (rarity == Rarity::Magic).then(|| pick_affix(rng, rolling, other, true, &chosen)).flatten()
        });
        if let Some(a) = picked {
            chosen.push(a);
        }
    }
    if rng.chance(FLAW_CHANCE[rarity as usize])
        && let Some(a) = pick_affix(rng, rolling, Prefix, false, &chosen)
    {
        chosen.push(a);
    }
    chosen
}

/// Rarity against `odds`, then its affixes, from one stream, for an item in no particular slot.
pub fn roll_rarity(rng: &mut Mulberry32, tags: &[Tag], odds: &Odds) -> (Rarity, Vec<&'static Affix>) {
    roll_rarity_for(rng, tags, None, odds)
}

/// [`roll_rarity`] for an item worn in `slot`, so slot-bound affixes can fit it.
pub fn roll_rarity_for(rng: &mut Mulberry32, tags: &[Tag], slot: Option<Slot>, odds: &Odds) -> (Rarity, Vec<&'static Affix>) {
    let rarity = Rarity::roll(rng, tags, odds);
    let chosen = roll_affixes_for(rng, rarity, tags, slot);
    (rarity, chosen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_rng;

    const SWORD: &[Tag] = &[Tag::Weapon, Tag::Metal];
    const TUNIC: &[Tag] = &[Tag::Cloth, Tag::Wear];
    const PACK: &[Tag] = &[Tag::Container, Tag::Wear, Tag::Leather];

    #[test]
    fn the_distribution_matches_the_odds() {
        let mut counts = [0u32; 4];
        for seed in 0..10_000 {
            let (r, _) = roll_rarity(&mut game_rng(seed), SWORD, &Odds::default());
            counts[r as usize] += 1;
        }
        for (i, odds) in [0.83, 0.12, 0.05, 0.0].into_iter().enumerate() {
            let share = counts[i] as f64 / 10_000.0;
            assert!((share - odds).abs() < 0.01, "rarity {i}: {share} against {odds}");
        }
    }

    #[test]
    fn containers_cap_at_rare() {
        for seed in 0..5_000 {
            assert_ne!(roll_rarity(&mut game_rng(seed), PACK, &Odds::loot(8)).0, Rarity::Legendary);
        }
    }

    #[test]
    fn affixes_fit_their_rarity_and_tags() {
        for tags in [SWORD, TUNIC, PACK, &[Tag::Light, Tag::Wood][..], &[Tag::Trinket][..]] {
            // Items and flaws rolled, by rarity.
            let (mut items, mut flaws) = ([0u32; 4], [0u32; 4]);
            for seed in 0..3_000 {
                let (rarity, chosen) = roll_rarity(&mut game_rng(seed), tags, &Odds::loot(8));
                for a in &chosen {
                    assert!(a.fits(tags), "{} on {tags:?}", a.name);
                }
                let good: Vec<_> = chosen.iter().filter(|a| a.good).collect();
                let flawed = chosen.len() - good.len();
                assert!(good.len() <= rarity as usize && flawed <= 1, "{rarity:?}: {chosen:?}");
                assert!(good.iter().filter(|a| a.kind == AffixKind::Prefix).count() <= 1);
                assert!(chosen.iter().take(good.len()).all(|a| a.good), "the flaw comes last, so the name keeps the good prefix");
                assert!(chosen.iter().filter(|a| !a.good).all(|a| a.kind == AffixKind::Prefix), "flaws are prefixes");
                items[rarity as usize] += 1;
                flaws[rarity as usize] += flawed as u32;
                let mut names: Vec<_> = chosen.iter().map(|a| &a.name).collect();
                names.dedup();
                assert_eq!(names.len(), chosen.len(), "an affix rolled twice");
            }
            // Where every flaw that fits is ungrouped, nothing keeps a flaw from its slot, so each
            // rarity rolls flaws at its chance.
            let fitting: Vec<_> = affixes().iter().filter(|a| !a.good && a.fits_item(tags, None) && a.in_play()).collect();
            if !fitting.is_empty() && fitting.iter().all(|a| a.group.is_none()) {
                for (i, chance) in FLAW_CHANCE.into_iter().enumerate() {
                    let share = flaws[i] as f64 / items[i].max(1) as f64;
                    assert!((share - chance).abs() < 0.05, "{tags:?} rarity {i}: {share} against {chance}");
                }
            }
        }
    }

    #[test]
    fn a_sword_rolls_full_rare_and_legendary_sets() {
        let full = (0..3_000).map(|seed| roll_rarity(&mut game_rng(seed), SWORD, &Odds::loot(8))).find(|(r, _)| *r == Rarity::Legendary).unwrap();
        assert_eq!(full.1.len(), 3);
    }

    /// The share of each rarity a sword rolls over 10 000 seeds against `odds`.
    fn measured(odds: Odds) -> [f64; 4] {
        let mut counts = [0u32; 4];
        for seed in 0..10_000 {
            counts[Rarity::roll(&mut game_rng(seed), SWORD, &odds) as usize] += 1;
        }
        counts.map(|n| n as f64 / 10_000.0)
    }

    #[test]
    fn depth_1_rolls_no_legendary() {
        assert_eq!(Odds::loot(1).thresholds(), SHALLOW);
        for x in (0..1_000).map(|i| i as f64 / 1_000.0) {
            let wanted = match x {
                x if x < 0.83 => Rarity::Common,
                x if x < 0.95 => Rarity::Magic,
                _ => Rarity::Rare,
            };
            assert_eq!(Rarity::from_roll_at(x, &Odds::loot(1)), wanted, "{x}");
        }
    }

    #[test]
    fn no_luck_finds_a_legendary_on_the_first_level() {
        for luck in [0.0, 1.0, 3.0, 9.0, 40.0] {
            let odds = Odds::loot(1).with_luck(luck);
            assert_eq!(measured(odds)[3], 0.0, "luck {luck}");
        }
        assert!(measured(Odds::loot(2))[3] > 0.0, "the second level can");
    }

    #[test]
    fn luck_rolls_a_level_deeper_a_point() {
        assert_eq!(Odds::loot(3).with_luck(2.0), Odds { depth: 5, cap: Rarity::Legendary });
        assert_eq!(Odds::loot(3).with_luck(1.9), Odds::loot(4), "only whole points count");
        assert_eq!(Odds::loot(3).with_luck(-2.0), Odds::loot(3), "no luck is never bad luck");
        assert_eq!(Odds::kit(2).with_luck(3.0).cap, Rarity::Rare, "the cap holds");
        let [c0, _, r0, _] = measured(Odds::loot(1));
        let [c3, _, r3, _] = measured(Odds::loot(1).with_luck(3.0));
        assert!(c3 < c0 && r3 > 2.0 * r0, "common {c0} to {c3}, rare {r0} to {r3}");
    }

    #[test]
    fn depth_5_rolls_rare_or_better_four_times_as_often() {
        let [_, _, r1, l1] = measured(Odds::loot(1));
        let [_, _, r5, l5] = measured(Odds::loot(5));
        let ratio = (r5 + l5) / (r1 + l1);
        assert!((3.5..5.0).contains(&ratio), "rare and legendary: {} at depth 1, {} at depth 5", r1 + l1, r5 + l5);
        assert!(l5 > 0.03, "legendary at depth 5: {l5}");
    }

    #[test]
    fn odds_improve_smoothly_with_depth() {
        let shares: Vec<[f64; 4]> = (1..=40).map(|d| Odds::loot(d).shares()).collect();
        for pair in shares.windows(2) {
            let ([c0, m0, r0, l0], [c1, m1, r1, l1]) = (pair[0], pair[1]);
            assert!(c1 < c0 && m1 > m0 && r1 > r0 && l1 > l0, "{pair:?}");
        }
        assert_eq!(shares[0][3], 0.0, "no legendary on the first level");
        assert!(shares[1..].iter().flatten().all(|&s| s > 0.0));
    }

    #[test]
    fn a_cap_at_magic_never_rolls_rare_or_legendary() {
        for depth in [1, 3, 5, 12, 40] {
            let odds = Odds::loot(depth).capped(Rarity::Magic);
            for seed in 0..5_000 {
                assert!(Rarity::roll(&mut game_rng(seed), SWORD, &odds) <= Rarity::Magic, "depth {depth} seed {seed}");
            }
        }
        assert_eq!(Odds::kit(1).cap, Rarity::Magic);
        assert_eq!(Odds::kit(2).cap, Rarity::Rare);
        assert_eq!(Odds::kit(3).cap, Rarity::Legendary);
    }

    #[test]
    fn the_same_seed_and_depth_roll_the_same() {
        for depth in [1, 2, 5, 9] {
            for seed in 0..500 {
                let roll = || roll_rarity(&mut game_rng(seed), SWORD, &Odds::loot(depth));
                assert_eq!(roll(), roll(), "depth {depth} seed {seed}");
            }
        }
    }

    #[test]
    fn the_disease_affixes_roll_again_now_that_disease_is_caught() {
        for name in ["Mildewed", "Lice-ridden", "of Lesser Warding"] {
            assert!(affixes().iter().find(|a| a.name == name).is_some_and(|a| a.in_play()), "{name} is in play");
        }
        let rolled: Vec<&str> = (0..3_000).flat_map(|seed| roll_rarity(&mut game_rng(seed), TUNIC, &Odds::loot(4)).1).map(|a| a.name.as_str()).collect();
        assert!(rolled.contains(&"Mildewed") || rolled.contains(&"Lice-ridden"), "a disease flaw rolls on some tunic");
    }

    #[test]
    fn no_rolled_item_carries_a_held_back_affix() {
        let held: Vec<&str> = affixes().iter().filter(|a| !a.in_play()).map(|a| a.name.as_str()).collect();
        assert_eq!(held, ["Silent", "Squeaking", "Cracked", "of Thieves", "of Quiet Steps", "of Presence", "of the Silver Tongue"]);
        let quiet = &[Tag::Quiet, Tag::Leather, Tag::Wear][..];
        for tags in [SWORD, TUNIC, PACK, quiet, &[Tag::Wood, Tag::Weapon][..], &[Tag::Trinket][..]] {
            for seed in 0..5_000 {
                for depth in [1, 4] {
                    let (_, chosen) = roll_rarity(&mut game_rng(seed), tags, &Odds::loot(depth));
                    assert!(chosen.iter().all(|a| a.in_play()), "{tags:?} seed {seed}: {:?}", chosen.iter().map(|a| &a.name).collect::<Vec<_>>());
                }
            }
        }
        for base in crate::armory_base::ARMORY_BASES.iter().map(|(name, _)| *name) {
            for seed in 0..20 {
                if let Some(item) = crate::roll_item_with(base, seed, Odds::loot(6)) {
                    assert!(item.affixes.iter().all(|a| a.in_play()), "{base} {seed}: {:?}", item.affixes.iter().map(|a| &a.name).collect::<Vec<_>>());
                }
            }
        }
    }

    #[test]
    fn an_item_that_rolled_an_affix_in_play_still_rolls_it() {
        // The draw against the whole pool, as before any were held back.
        let before = |seed: u32, tags: &[Tag]| {
            let mut rng = game_rng(seed);
            let pool: Vec<&Affix> = affixes().iter().filter(|a| a.kind == AffixKind::Suffix && a.good && a.least == Rarity::Common && a.fits_item(tags, None)).collect();
            *rng.pick(&pool)
        };
        for seed in 0..2_000 {
            let was = before(seed, TUNIC);
            let now = pick_affix(&mut game_rng(seed), Rolling { tags: TUNIC, slot: None, rarity: Rarity::Common }, AffixKind::Suffix, true, &[]).unwrap();
            if was.in_play() {
                assert_eq!(now.name, was.name, "seed {seed}");
            }
        }
    }

    /// Every armory base rolled `seeds` times at depth 12, where every rarity turns up.
    fn deep_rolls(seeds: u32) -> Vec<crate::Item> {
        crate::armory_base::ARMORY_BASES.iter().flat_map(|(base, _)| (0..seeds).filter_map(move |seed| crate::roll_item_with(base, seed, Odds::loot(12)))).collect()
    }

    #[test]
    fn no_item_carries_two_affixes_of_one_group_and_tiers_keep_to_their_rarity() {
        let items = deep_rolls(80);
        let mut seen = std::collections::HashSet::new();
        for item in &items {
            for (i, a) in item.affixes.iter().enumerate() {
                seen.insert(a.name.clone());
                assert!(a.least <= item.rarity, "{} on a {:?} {}", a.name, item.rarity, item.base);
                assert!(a.fits_item(&item.profile.tags, item.profile.slot), "{} on {}", a.name, item.base);
                for b in &item.affixes[i + 1..] {
                    assert!(a.stacks_with(b), "{} and {} on one {}", a.name, b.name, item.base);
                }
            }
        }
        for name in ["Fleet", "of Haste", "Vicious", "of Carnage", "Accurate", "of Precision", "Reinforced", "of the Bulwark", "of Deflection", "of the Rampart", "of the Owl", "of the Bat", "of Might", "of the Giant", "of Vigour", "of the Bear", "Sluggish", "Frail"] {
            assert!(seen.contains(name), "{name} never rolled");
        }
    }

    #[test]
    fn slot_bound_affixes_roll_only_in_their_slots() {
        let by_name = |n: &str| affixes().iter().find(|a| a.name == n).unwrap();
        let fleet = by_name("Fleet");
        assert!(fleet.fits_item(&[Tag::Leather, Tag::Wear, Tag::Quiet], Some(Slot::Feet)));
        assert!(!fleet.fits_item(&[Tag::Leather, Tag::Wear, Tag::Quiet], Some(Slot::Hands)));
        let might = by_name("of Might");
        assert!(might.fits_item(&[Tag::Trinket], Some(Slot::Ring2)), "either ring");
        assert!(!might.fits_item(&[Tag::Trinket], None));
        let rampart = by_name("of the Rampart");
        assert!(rampart.fits_item(&[Tag::Armor, Tag::Wood], Some(Slot::Offhand)), "a shield");
        assert!(!rampart.fits_item(&[Tag::Metal, Tag::Armor, Tag::Wear], Some(Slot::Head)), "not a helm");
        assert!(!rampart.fits_item(&[Tag::Wood], Some(Slot::Offhand)), "not a torch");
    }

    #[test]
    fn hand_picked_affixes_set_rarity() {
        let by_name = |n: &str| affixes().iter().find(|a| a.name == n).unwrap();
        assert_eq!(Rarity::of_affixes(&[]), Rarity::Common);
        assert_eq!(Rarity::of_affixes(&[by_name("Rusted")]), Rarity::Common);
        assert_eq!(Rarity::of_affixes(&[by_name("Keen")]), Rarity::Magic);
        assert_eq!(Rarity::of_affixes(&[by_name("Keen"), by_name("of the Ox")]), Rarity::Rare);
    }
}
