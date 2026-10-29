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
        .filter(|a| a.kind == kind && !a.holy && a.good == good && a.least <= item.rarity && a.fits_item(item.tags, item.slot) && taken.iter().all(|t| a.stacks_with(t)))
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
/// suffix. Legendary: a good prefix and two good suffixes. A rare or legendary suffix slot with
/// no suffix that fits takes a second good prefix instead ([`good_suffix_fits`]), never an affix
/// already chosen nor one of its group; a prefix slot with nothing stays empty, so a ring still
/// rolls one good affix fewer than its rarity. A pool that was not empty draws exactly as it did. Then a flaw prefix, at
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
            // Magic falls back both ways; a rare or legendary slot only from a suffix to a prefix, so
            // a ring, which has no good prefix, still rolls one good affix fewer than its rarity.
            let other = if kind == Prefix { Suffix } else { Prefix };
            (rarity == Rarity::Magic || kind == Suffix).then(|| pick_affix(rng, rolling, other, true, &chosen)).flatten()
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

/// Whether some good suffix that is in play still fits an item with `tags` worn in `slot` at
/// `rarity` beside the `taken` affixes. When none does, a second good prefix may stand in its
/// place: the roll takes one for an empty suffix slot, and the enchanter's bench offers one.
pub fn good_suffix_fits(tags: &[Tag], slot: Option<Slot>, rarity: Rarity, taken: &[&Affix]) -> bool {
    affixes().iter().any(|a| a.kind == AffixKind::Suffix && a.good && a.in_play() && a.least <= rarity && a.fits_item(tags, slot) && taken.iter().all(|t| a.stacks_with(t)))
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

    const SWORD: &[Tag] = &[Tag::Weapon, Tag::Metal, Tag::Blade];
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
                // Two prefixes only where no good suffix is left that fits (a slot falls back to the other kind).
                if good.iter().filter(|a| a.kind == AffixKind::Prefix).count() > 1 {
                    let left = affixes().iter().any(|a| a.good && a.kind == AffixKind::Suffix && a.least <= rarity && a.fits_item(tags, None) && a.in_play() && chosen.iter().all(|c| a.stacks_with(c)));
                    assert!(!left, "{rarity:?}: {chosen:?} has two prefixes with a suffix left");
                }
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
    fn banes_roll_only_on_what_they_suit() {
        const MACE: &[Tag] = &[Tag::Weapon, Tag::Metal, Tag::Blunt];
        const BOW: &[Tag] = &[Tag::Weapon, Tag::Wood, Tag::Ranged];
        let named = |tags: &[Tag], rarity| {
            let mut names = std::collections::HashSet::new();
            for seed in 0..5_000 {
                let mut rng = game_rng(seed);
                for kind in [AffixKind::Prefix, AffixKind::Suffix] {
                    let item = Rolling { tags, slot: Some(Slot::Mainhand), rarity };
                    names.extend(pick_affix(&mut rng, item, kind, true, &[]).map(|a| a.name.as_str()));
                }
            }
            names
        };
        let sword = named(SWORD, Rarity::Legendary);
        assert!(!sword.contains("of the Bonebreaker") && !sword.contains("Bone-crushing"), "no sword breaks bones");
        assert!(sword.contains("Hunter's") && sword.contains("Ratbane") && sword.contains("of the Spider-slayer"));
        let mace = named(MACE, Rarity::Legendary);
        assert!(mace.contains("of the Bonebreaker") && mace.contains("Bone-crushing") && mace.contains("Ratbane"));
        assert!(!mace.contains("Hunter's"), "a mace is no hunter's weapon");
        assert!(named(BOW, Rarity::Legendary).contains("Hunter's"));
        // Banes of a rarer tier stay off lesser items.
        let magic = named(MACE, Rarity::Magic);
        assert!(magic.contains("Ratbane") && !magic.contains("of the Bonebreaker") && !magic.contains("Bone-crushing"));
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
    fn of_thieves_rolls_again_now_that_picks_count_lockpicking() {
        let thieves = affixes().iter().find(|a| a.name == "of Thieves").expect("of Thieves is in the table");
        assert!(thieves.in_play(), "a pick rolls lockpicking as a line of its own");
        let gloves = &[Tag::Leather, Tag::Wear][..];
        let rolled: Vec<_> = (0..3_000).filter(|&seed| roll_rarity_for(&mut game_rng(seed), gloves, Some(Slot::Hands), &Odds::loot(6)).1.iter().any(|a| a.name == "of Thieves")).collect();
        assert!(!rolled.is_empty(), "of Thieves rolls on some glove");
        // And it gives the item the lockpicking its card shows and a pick counts.
        let item = (0..20_000).filter_map(|seed| crate::roll_item_with("Gloves", seed, Odds::loot(8))).find(|i| i.affixes.iter().any(|a| a.name == "of Thieves")).expect("gloves of Thieves turn up");
        assert!(item.profile.stats.lockpicking >= 1.0, "{:?}", item.profile.stats.lockpicking);
    }

    /// The roll as it was before a rare or legendary slot could fall back to the other kind.
    fn roll_before_the_fallback(rng: &mut Mulberry32, rarity: Rarity, tags: &[Tag], slot: Option<Slot>) -> Vec<&'static Affix> {
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
        if rng.chance(FLAW_CHANCE[rarity as usize]) && let Some(a) = pick_affix(rng, rolling, Prefix, false, &chosen) {
            chosen.push(a);
        }
        chosen
    }

    const RARITIES: [Rarity; 4] = [Rarity::Common, Rarity::Magic, Rarity::Rare, Rarity::Legendary];

    fn lantern() -> (Vec<Tag>, Option<Slot>) {
        let item = crate::roll_item_with("Lantern", 1, Odds::default()).expect("a lantern rolls");
        (item.profile.tags.clone(), item.profile.slot)
    }

    #[test]
    fn of_thieves_rolls_only_on_thieves_gear() {
        let names = |tags: &[Tag], slot: Option<Slot>| -> std::collections::HashSet<&'static str> {
            let mut seen = std::collections::HashSet::new();
            for seed in 0..1_500 {
                for rarity in RARITIES {
                    seen.extend(roll_affixes_for(&mut game_rng(seed), rarity, tags, slot).iter().map(|a| a.name.as_str()));
                }
            }
            seen
        };
        let (tags, slot) = lantern();
        assert!(!names(&tags, slot).contains("of Thieves"), "a lantern rolls no of Thieves");
        for (tags, slot) in [(&[Tag::Leather, Tag::Wear][..], Slot::Hands), (&[Tag::Trinket][..], Slot::Ring1), (&[Tag::Trinket][..], Slot::Ring2), (&[Tag::Trinket][..], Slot::Neck), (&[Tag::Trinket][..], Slot::Charm), (&[Tag::Leather, Tag::Wear][..], Slot::Belt)] {
            assert!(names(tags, Some(slot)).contains("of Thieves"), "{slot:?} can roll of Thieves");
        }
        assert!(!names(SWORD, Some(Slot::Mainhand)).contains("of Thieves"), "a sword rolls none");
    }

    #[test]
    fn a_legendary_lantern_can_be_gleaming_and_long_burning() {
        let (tags, slot) = lantern();
        let mut both = 0;
        for seed in 0..2_000 {
            let chosen = roll_affixes_for(&mut game_rng(seed), Rarity::Legendary, &tags, slot);
            let mut names: Vec<&str> = chosen.iter().map(|a| a.name.as_str()).collect();
            let n = names.len();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), n, "seed {seed}: no affix twice");
            for (i, a) in chosen.iter().enumerate() {
                assert!(chosen[i + 1..].iter().all(|b| a.stacks_with(b)), "seed {seed}: groups stay apart");
            }
            both += (names.contains(&"Gleaming") && names.contains(&"Long-burning")) as u32;
        }
        assert!(both > 0, "some legendary lantern is both");
    }

    #[test]
    fn items_whose_pools_did_not_change_roll_what_they_rolled() {
        let wanted = |r: Rarity| match r {
            Rarity::Common => 0,
            Rarity::Magic => 1,
            Rarity::Rare => 2,
            Rarity::Legendary => 3,
        };
        // Rings, amulets and charms roll exactly as they did (a ring, with no good prefix, at most one good affix when rare and two when legendary).
        for slot in [Slot::Ring1, Slot::Ring2, Slot::Neck, Slot::Charm] {
            for seed in 0..2_000 {
                for rarity in RARITIES {
                    let tags = &[Tag::Trinket][..];
                    let now = roll_affixes_for(&mut game_rng(seed), rarity, tags, Some(slot));
                    assert_eq!(now, roll_before_the_fallback(&mut game_rng(seed), rarity, tags, Some(slot)), "{slot:?} {rarity:?} seed {seed}");
                    let good = now.iter().filter(|a| a.good).count();
                    let most = match rarity {
                        Rarity::Common => 0,
                        Rarity::Magic | Rarity::Rare => 1,
                        Rarity::Legendary => 2,
                    };
                    // Only a ring has no good prefix at all.
                    assert!(good <= most || slot != Slot::Ring1 && slot != Slot::Ring2, "{slot:?} {rarity:?}: {good} good");
                }
            }
        }
        // Gloves and belts roll as they did wherever every good slot was filled.
        let mut compared = 0;
        for (tags, slot) in [(&[Tag::Leather, Tag::Wear][..], Slot::Hands), (&[Tag::Leather, Tag::Wear][..], Slot::Belt)] {
            for seed in 0..2_000 {
                for rarity in RARITIES {
                    let before = roll_before_the_fallback(&mut game_rng(seed), rarity, tags, Some(slot));
                    if before.iter().filter(|a| a.good).count() == wanted(rarity) {
                        assert_eq!(roll_affixes_for(&mut game_rng(seed), rarity, tags, Some(slot)), before, "{slot:?} {rarity:?} seed {seed}");
                        compared += 1;
                    }
                }
            }
        }
        assert!(compared > 1_000, "{compared}");
    }

    #[test]
    fn a_legendary_sword_can_still_be_of_carnage_and_of_precision() {
        let mut seen = std::collections::HashSet::new();
        for seed in 0..600 {
            seen.extend(roll_affixes_for(&mut game_rng(seed), Rarity::Legendary, SWORD, Some(Slot::Mainhand)).iter().map(|a| a.name.as_str()));
        }
        assert!(seen.contains("of Carnage") && seen.contains("of Precision"), "{seen:?}");
    }

    fn lantern_rolls(rarity: Rarity, seeds: u32) -> Vec<Vec<&'static Affix>> {
        let (tags, slot) = lantern();
        (0..seeds).map(|seed| roll_affixes_for(&mut game_rng(seed), rarity, &tags, slot)).collect()
    }

    #[test]
    fn lantern_light_tiers_come_by_rarity_and_never_stack() {
        let tier_names = ["Radiant", "Blazing", "Enduring", "Everlasting", "of the Furnace"];
        for rarity in [Rarity::Common, Rarity::Magic] {
            for chosen in lantern_rolls(rarity, 500) {
                assert!(chosen.iter().all(|a| !tier_names.contains(&a.name.as_str())), "{rarity:?}: {chosen:?}");
            }
        }
        for rarity in [Rarity::Rare, Rarity::Legendary] {
            let rolls = lantern_rolls(rarity, 500);
            for name in ["Radiant", "Enduring", "of the Furnace"] {
                assert!(rolls.iter().flatten().any(|a| a.name == name), "{rarity:?} lantern rolls {name}");
            }
        }
        for name in ["Blazing", "Everlasting"] {
            assert!(!lantern_rolls(Rarity::Rare, 500).iter().flatten().any(|a| a.name == name), "{name} waits for legendary");
            assert!(lantern_rolls(Rarity::Legendary, 500).iter().flatten().any(|a| a.name == name), "a legendary lantern rolls {name}");
        }
        for rarity in RARITIES {
            for chosen in lantern_rolls(rarity, 500) {
                for (i, a) in chosen.iter().enumerate() {
                    assert!(chosen[i + 1..].iter().all(|b| a.stacks_with(b)), "{rarity:?}: {chosen:?}");
                }
            }
        }
    }

    #[test]
    fn rare_lanterns_differ_and_legendary_lanterns_carry_three_good_affixes() {
        let combos: std::collections::HashSet<Vec<&str>> = lantern_rolls(Rarity::Rare, 500).iter().map(|c| c.iter().map(|a| a.name.as_str()).collect()).collect();
        assert!(combos.len() > 3, "{combos:?}");
        for chosen in lantern_rolls(Rarity::Rare, 500) {
            assert_eq!(chosen.iter().filter(|a| a.good).count(), 2, "{chosen:?}");
        }
        for chosen in lantern_rolls(Rarity::Legendary, 500) {
            assert_eq!(chosen.iter().filter(|a| a.good).count(), 3, "{chosen:?}");
        }
    }

    #[test]
    fn the_light_and_burn_time_the_game_reads_follow_the_tier() {
        let plain = crate::roll_item_with("Lantern", 1, Odds::default()).expect("a lantern");
        let (light0, burn0) = (plain.profile.stats.light - plain.affixes.iter().flat_map(|a| &a.fx).filter_map(|m| match *m { crate::modifier::Modifier::Add { stat: crate::stats::Stat::Light, by } => Some(by), _ => None }).sum::<f32>(), 1.0);
        let (mut blazing, mut everlasting) = (false, false);
        for seed in 0..800 {
            let Some(item) = crate::roll_item_with("Lantern", seed, Odds::loot(14)) else { continue };
            let has = |n: &str| item.affixes.iter().any(|a| a.name == n);
            if has("Blazing") && !has("Smoky") && !has("Gleaming") {
                blazing = true;
                assert!((item.profile.stats.light - (light0 + 30.0)).abs() < 1e-3 && item.light_factor() > 1.0, "{:?}", item.profile.stats.light);
            }
            if has("Everlasting") && !has("Long-burning") {
                everlasting = true;
                assert!((item.profile.stats.burn_time - burn0 * 4.0).abs() < 1e-3, "{:?}", item.profile.stats.burn_time);
            }
        }
        assert!(blazing && everlasting, "a Blazing and an Everlasting lantern turn up");
    }

    #[test]
    fn what_has_no_good_suffix_at_legendary() {
        let mut none = Vec::new();
        for (base, _) in crate::armory_base::ARMORY_BASES.iter() {
            let Some(item) = crate::roll_item_with(base, 1, Odds::loot(14)) else { continue };
            let mut slots: Vec<Option<Slot>> = item.slots().into_iter().map(Some).collect();
            if slots.is_empty() {
                slots.push(None);
            }
            for slot in slots {
                if !good_suffix_fits(&item.profile.tags, slot, Rarity::Legendary, &[]) {
                    none.push(format!("{base} ({})", slot.map_or("no slot".to_string(), |s| format!("{s:?}"))));
                }
            }
        }
        eprintln!("NOSUFFIX {none:?}");
        assert!(!none.iter().any(|n| n.starts_with("Lantern") || n.starts_with("Torch")), "{none:?}");
    }

    #[test]
    fn no_rolled_item_carries_a_held_back_affix() {
        let held: Vec<&str> = affixes().iter().filter(|a| !a.in_play()).map(|a| a.name.as_str()).collect();
        assert_eq!(held, ["Silent", "Squeaking", "Cracked", "of Quiet Steps", "of Presence", "of the Silver Tongue"]);
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
        for name in ["Fleet", "of Haste", "Vicious", "Accurate", "of Precision", "Reinforced", "of the Bulwark", "of Deflection", "of the Rampart", "of the Owl", "of the Bat", "of Might", "of the Giant", "of Vigour", "of the Bear", "Sluggish", "Frail", "of the Hare", "of Mending", "of Purity"] {
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
        assert!(!rampart.fits_item(&[Tag::Light, Tag::Wood], Some(Slot::Offhand)), "not a torch");
        for name in ["of the Hare", "of the Wind", "of Mending", "of the Troll", "of Purity", "of the Mongoose"] {
            let ward = by_name(name);
            for slot in [Slot::Ring1, Slot::Ring2, Slot::Neck, Slot::Charm] {
                assert!(ward.fits_item(&[Tag::Trinket], Some(slot)), "{name} on {slot:?}");
            }
            assert!(!ward.fits_item(&[Tag::Leather, Tag::Wear, Tag::Quiet], Some(Slot::Feet)), "{name} on boots");
            assert!(ward.in_play(), "{name} is read");
        }
    }

    /// The comfort suffixes and the slots they list in `data/afx.json`.
    const COMFORT: [(&str, &[Slot]); 5] = [
        ("of Plenty", &[Slot::Belt, Slot::Neck, Slot::Charm, Slot::Ring1]),
        ("of Wakefulness", &[Slot::Belt, Slot::Neck, Slot::Charm, Slot::Ring1]),
        ("of the Ox", &[Slot::Belt, Slot::Back, Slot::Satchel, Slot::Body]),
        ("of Lesser Warding", &[Slot::Head, Slot::Hands, Slot::Neck, Slot::Charm]),
        ("of the Mole", &[Slot::Head, Slot::Neck, Slot::Charm]),
    ];

    /// Base `base`'s tags and slot, from the profile of its seed 0 shape (they are the base's, not the
    /// seed's), and the rarity and affixes the game layer rolls from `seed` at `depth`: what
    /// `roll_item_with` draws first, without building a shape for every seed.
    fn game_layer(base: &str) -> Option<(Vec<Tag>, Option<Slot>)> {
        let shape = crate::armory_base::roll_shape(base, 0)?;
        let profile = crate::profile::profile(base, &shape);
        Some((profile.tags, profile.slot))
    }

    fn rolled_affixes(tags: &[Tag], slot: Option<Slot>, seed: u32, depth: u32) -> Vec<&'static Affix> {
        roll_rarity_for(&mut game_rng(seed), tags, slot, &Odds::loot(depth)).1
    }

    #[test]
    fn comfort_suffixes_stay_off_weapons_and_shields() {
        for base in ["Sword", "Mace", "Bow", "Shield"] {
            let (tags, slot) = game_layer(base).unwrap();
            for seed in 0..2000 {
                let chosen = rolled_affixes(&tags, slot, seed, 5);
                for (name, _) in COMFORT {
                    assert!(chosen.iter().all(|a| a.name != name), "{name} on {base} {seed}");
                }
            }
        }
    }

    #[test]
    fn comfort_suffixes_still_roll_in_their_slots() {
        let worn: Vec<(&str, Vec<Tag>, Option<Slot>)> =
            crate::armory_base::ARMORY_BASES.iter().filter_map(|(base, _)| game_layer(base).map(|(tags, slot)| (*base, tags, slot))).filter(|(_, _, slot)| slot.is_some()).collect();
        for (name, slots) in COMFORT {
            let affix = affixes().iter().find(|a| a.name == name).unwrap();
            assert!(affix.in_play(), "{name} is read");
            let mut rolled = false;
            for (base, tags, slot) in &worn {
                for seed in 0..200 {
                    if rolled_affixes(tags, *slot, seed, 12).iter().all(|a| a.name != name) {
                        continue;
                    }
                    rolled = true;
                    let slot = slot.map(|s| if s == Slot::Ring2 { Slot::Ring1 } else { s });
                    assert!(slot.is_some_and(|s| slots.contains(&s)), "{name} on {base} in {slot:?}");
                }
            }
            assert!(rolled, "{name} never rolled");
        }
    }

    #[test]
    fn sighted_rolls_on_bows_and_crossbows_only() {
        let sighted = affixes().iter().find(|a| a.name == "Sighted").unwrap();
        assert!(sighted.in_play());
        for base in ["Bow", "Crossbow"] {
            let item = crate::roll_item(base, 1).unwrap();
            assert!(sighted.fits_item(&item.profile.tags, item.profile.slot), "{base}");
        }
        for base in ["Sword", "Mace", "Dagger", "Spear", "Staff", "Shield", "Ring", "Gloves", "Helm"] {
            let item = crate::roll_item(base, 1).unwrap();
            assert!(!sighted.fits_item(&item.profile.tags, item.profile.slot), "{base}");
        }
    }

    #[test]
    fn block_suffixes_roll_on_shields_gloves_and_charms_but_not_torches() {
        for name in ["of Deflection", "of the Rampart"] {
            let affix = affixes().iter().find(|a| a.name == name).unwrap();
            assert!(affix.in_play(), "{name} is read");
            for (base, slot) in [("Shield", Slot::Offhand), ("Gloves", Slot::Hands), ("Gauntlets", Slot::Hands), ("Charm", Slot::Charm)] {
                let Some(item) = crate::roll_item(base, 3) else { continue };
                assert_eq!(item.profile.slot, Some(slot), "{base}");
                assert!(affix.fits_item(&item.profile.tags, item.profile.slot), "{name} on {base}");
            }
            for base in ["Torch", "Lantern", "Helm", "Boots", "Ring", "Amulet", "Sword"] {
                let item = crate::roll_item(base, 3).unwrap();
                assert!(!affix.fits_item(&item.profile.tags, item.profile.slot), "{name} on {base}");
            }
        }
    }

    #[test]
    fn attack_rate_affixes_stay_on_weapons_and_gloves() {
        let by_name = |n: &str| affixes().iter().find(|a| a.name == n).unwrap();
        let (quick, swift, duellist) = (by_name("Quick"), by_name("of Swiftness"), by_name("of the Duellist"));
        assert_eq!(swift.least, Rarity::Rare);
        assert!(quick.in_play() && swift.in_play() && duellist.in_play());
        assert!(!quick.stacks_with(swift), "one group of weapon speed");
        assert!(quick.stacks_with(duellist), "gloves stack with a quick weapon");
        for base in ["Sword", "Dagger", "Mace", "Staff", "Spear", "Bow", "Crossbow"] {
            let item = crate::roll_item(base, 1).unwrap();
            assert!(quick.fits_item(&item.profile.tags, item.profile.slot) && swift.fits_item(&item.profile.tags, item.profile.slot), "{base}");
        }
        for base in ["Helm", "Gloves", "Boots", "Shield", "Ring", "Tunic", "Torch"] {
            let Some(item) = crate::roll_item(base, 1) else { continue };
            assert!(!quick.fits_item(&item.profile.tags, item.profile.slot) && !swift.fits_item(&item.profile.tags, item.profile.slot), "{base} is no weapon");
        }
        let any = &[Tag::Leather, Tag::Wear][..];
        assert!(duellist.fits_item(any, Some(Slot::Hands)));
        for slot in [Slot::Mainhand, Slot::Offhand, Slot::Head, Slot::Feet, Slot::Ring1, Slot::Body] {
            assert!(!duellist.fits_item(any, Some(slot)), "{slot:?}");
        }
    }

    #[test]
    fn stamina_affixes_keep_to_their_slots_and_groups() {
        let by_name = |n: &str| affixes().iter().find(|a| a.name == n).unwrap();
        let (tireless, wind, light, endurance) = (by_name("Tireless"), by_name("of Second Wind"), by_name("Light-handed"), by_name("of Endurance"));
        assert_eq!((wind.kind, tireless.kind, light.kind, endurance.kind), (AffixKind::Suffix, AffixKind::Prefix, AffixKind::Prefix, AffixKind::Suffix));
        assert_eq!(wind.least, Rarity::Rare);
        assert!([tireless, wind, light, endurance].iter().all(|a| a.in_play()));
        assert!(!tireless.stacks_with(wind) && !light.stacks_with(endurance), "one refill and one cost");
        assert!(tireless.stacks_with(light) && wind.stacks_with(endurance));
        let any = &[Tag::Leather, Tag::Wear][..];
        for (affix, yes, no) in [
            (tireless, &[Slot::Body, Slot::Feet][..], &[Slot::Legs, Slot::Neck, Slot::Mainhand][..]),
            (wind, &[Slot::Neck, Slot::Charm, Slot::Body][..], &[Slot::Feet, Slot::Belt][..]),
            (endurance, &[Slot::Legs, Slot::Feet, Slot::Belt][..], &[Slot::Body, Slot::Neck][..]),
        ] {
            assert!(yes.iter().all(|s| affix.fits_item(any, Some(*s))) && no.iter().all(|s| !affix.fits_item(any, Some(*s))), "{}", affix.name);
        }
        for base in ["Sword", "Bow", "Shield", "Buckler"] {
            if let Some(item) = crate::roll_item(base, 1) {
                assert!(light.fits_item(&item.profile.tags, item.profile.slot), "{base}");
            }
        }
        for base in ["Torch", "Lantern", "Helm", "Boots"] {
            let item = crate::roll_item(base, 1).unwrap();
            assert!(!light.fits_item(&item.profile.tags, item.profile.slot), "{base} is no weapon or shield");
        }
    }

    #[test]
    fn the_hawk_and_the_steady_hand_keep_to_their_slots() {
        let by_name = |n: &str| affixes().iter().find(|a| a.name == n).unwrap();
        let (hawk, steady) = (by_name("of the Hawk"), by_name("of the Steady Hand"));
        assert_eq!(hawk.least, Rarity::Rare);
        let any = &[Tag::Trinket][..];
        for slot in [Slot::Ring1, Slot::Ring2, Slot::Hands, Slot::Head] {
            assert!(hawk.fits_item(any, Some(slot)), "hawk on {slot:?}");
        }
        for slot in [Slot::Ring1, Slot::Hands, Slot::Neck] {
            assert!(steady.fits_item(any, Some(slot)), "steady hand on {slot:?}");
        }
        for slot in [Slot::Mainhand, Slot::Offhand, Slot::Body, Slot::Feet, Slot::Charm] {
            assert!(!hawk.fits_item(any, Some(slot)) && !steady.fits_item(any, Some(slot)), "{slot:?}");
        }
        assert!(!hawk.fits_item(any, Some(Slot::Neck)) && !steady.fits_item(any, Some(Slot::Head)));
        assert!(hawk.stacks_with(steady), "missile and melee hit stack");
    }

    #[test]
    fn trinket_wards_give_one_or_two_points() {
        let by_name = |n: &str| affixes().iter().find(|a| a.name == n).unwrap();
        let on = |n: &str| {
            let mut s = crate::stats::Stats::default();
            by_name(n).apply_to_stats(&mut s);
            s
        };
        assert_eq!((on("of Mending").recovery, on("of the Troll").recovery), (1.0, 2.0));
        assert_eq!((on("of Purity").poison_resistance, on("of the Mongoose").poison_resistance), (1.0, 2.0));
        assert_eq!((on("of the Hare").speed, on("of the Wind").speed), (1.05, 1.1));
        assert_eq!(by_name("of the Troll").least, Rarity::Rare);
        assert!(!by_name("of the Hare").stacks_with(by_name("Fleet")), "one speed affix to a charm");
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
