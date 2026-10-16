//! Rarity and the affixes that come with it: the plate's `rollAffixes` and `pickAffix`
//! (`reference/forever-below-plate.html`; rules in `docs/items.md`, "Rarity and affixes"),
//! with odds that improve with depth ([`Odds`], our own).

use crate::tables::{Affix, AffixKind, affixes};
use crate::{Rarity, Tag};
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

/// The plate's rarity thresholds, which are the odds on the first level: below 0.58 common,
/// below 0.85 magic, below 0.97 rare, otherwise legendary.
const SHALLOW: [f64; 3] = [0.58, 0.85, 0.97];
/// The thresholds the odds tend to far below: 7% common, 33% magic, 48% rare, 12% legendary.
/// Our own proposal, chosen so that depth 5 doubles the first level's rare and legendary odds.
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
    /// The first level, uncapped: the plate's odds.
    fn default() -> Odds {
        Odds::loot(1)
    }
}

impl Odds {
    /// Loot found at `depth`, on the floor or in a container: uncapped.
    pub fn loot(depth: u32) -> Odds {
        Odds { depth, cap: Rarity::Legendary }
    }

    /// The gear folk and the risen dead carry at `depth`: as loot, but common or magic only on
    /// the first level, so that they grow better equipped as one descends.
    pub fn kit(depth: u32) -> Odds {
        Odds::loot(depth).capped(if depth <= 1 { Rarity::Magic } else { Rarity::Legendary })
    }

    /// These odds at most `cap`.
    pub fn capped(self, cap: Rarity) -> Odds {
        Odds { cap: self.cap.min(cap), ..self }
    }

    /// The upper bounds of common, magic and rare for a uniform roll. Depth 1 is the plate's
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
    /// The rarity for a uniform roll `x` in `[0, 1)` at the first level: below 0.58 common,
    /// below 0.85 magic, below 0.97 rare, otherwise legendary.
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

/// A uniform pick among the affixes of `kind` and goodness that fit `tags` and are not
/// already `taken` (`pickAffix`). `None` when none fit.
pub fn pick_affix(rng: &mut Mulberry32, tags: &[Tag], kind: AffixKind, good: bool, taken: &[&Affix]) -> Option<&'static Affix> {
    let pool: Vec<&'static Affix> = affixes()
        .iter()
        .filter(|a| a.kind == kind && a.good == good && a.fits(tags) && !taken.iter().any(|t| t.name == a.name))
        .collect();
    (!pool.is_empty()).then(|| *rng.pick(&pool))
}

/// The affixes a `rarity` rolls on an item with `tags`. Common: a flaw prefix 30% of the
/// time. Magic: one good prefix or suffix, even odds. Rare: a good prefix and a good suffix.
/// Legendary: a good prefix and two good suffixes. Slots with nothing that fits stay empty.
pub fn roll_affixes(rng: &mut Mulberry32, rarity: Rarity, tags: &[Tag]) -> Vec<&'static Affix> {
    use AffixKind::{Prefix, Suffix};
    let wanted: Vec<(AffixKind, bool)> = match rarity {
        Rarity::Common => if rng.chance(0.3) { vec![(Prefix, false)] } else { vec![] },
        Rarity::Magic => vec![(if rng.chance(0.5) { Prefix } else { Suffix }, true)],
        Rarity::Rare => vec![(Prefix, true), (Suffix, true)],
        Rarity::Legendary => vec![(Prefix, true), (Suffix, true), (Suffix, true)],
    };
    let mut chosen = Vec::new();
    for (kind, good) in wanted {
        if let Some(a) = pick_affix(rng, tags, kind, good, &chosen) {
            chosen.push(a);
        }
    }
    chosen
}

/// Rarity against `odds`, then its affixes, from one stream.
pub fn roll_rarity(rng: &mut Mulberry32, tags: &[Tag], odds: &Odds) -> (Rarity, Vec<&'static Affix>) {
    let rarity = Rarity::roll(rng, tags, odds);
    let chosen = roll_affixes(rng, rarity, tags);
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
        for (i, odds) in [0.58, 0.27, 0.12, 0.03].into_iter().enumerate() {
            let share = counts[i] as f64 / 10_000.0;
            assert!((share - odds).abs() < 0.01, "rarity {i}: {share} against {odds}");
        }
    }

    #[test]
    fn containers_cap_at_rare() {
        for seed in 0..5_000 {
            assert_ne!(roll_rarity(&mut game_rng(seed), PACK, &Odds::default()).0, Rarity::Legendary);
        }
    }

    #[test]
    fn affixes_fit_their_rarity_and_tags() {
        for tags in [SWORD, TUNIC, PACK, &[Tag::Light, Tag::Wood][..], &[Tag::Trinket][..]] {
            let mut flaws = 0;
            let mut commons = 0;
            for seed in 0..3_000 {
                let (rarity, chosen) = roll_rarity(&mut game_rng(seed), tags, &Odds::default());
                for a in &chosen {
                    assert!(a.fits(tags), "{} on {tags:?}", a.name);
                }
                let good = chosen.iter().filter(|a| a.good).count();
                let prefixes = chosen.iter().filter(|a| a.kind == AffixKind::Prefix).count();
                match rarity {
                    Rarity::Common => {
                        commons += 1;
                        flaws += chosen.len();
                        assert!(good == 0 && chosen.len() <= 1);
                    }
                    Rarity::Magic => assert!(good == chosen.len() && chosen.len() <= 1),
                    Rarity::Rare => assert!(good == chosen.len() && prefixes <= 1 && chosen.len() <= 2),
                    Rarity::Legendary => assert!(good == chosen.len() && prefixes <= 1 && chosen.len() <= 3),
                }
                let mut names: Vec<_> = chosen.iter().map(|a| &a.name).collect();
                names.dedup();
                assert_eq!(names.len(), chosen.len(), "an affix rolled twice");
            }
            if affixes().iter().any(|a| !a.good && a.fits(tags)) {
                let share = flaws as f64 / commons as f64;
                assert!((share - 0.3).abs() < 0.04, "{tags:?}: {share}");
            }
        }
    }

    #[test]
    fn a_sword_rolls_full_rare_and_legendary_sets() {
        let full = (0..3_000).map(|seed| roll_rarity(&mut game_rng(seed), SWORD, &Odds::default())).find(|(r, _)| *r == Rarity::Legendary).unwrap();
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
    fn depth_1_rolls_the_plate_odds() {
        assert_eq!(Odds::loot(1).thresholds(), SHALLOW);
        for x in (0..1_000).map(|i| i as f64 / 1_000.0) {
            let plate = match x {
                x if x < 0.58 => Rarity::Common,
                x if x < 0.85 => Rarity::Magic,
                x if x < 0.97 => Rarity::Rare,
                _ => Rarity::Legendary,
            };
            assert_eq!(Rarity::from_roll_at(x, &Odds::loot(1)), plate, "{x}");
        }
    }

    #[test]
    fn depth_5_doubles_rare_and_legendary() {
        let [_, _, r1, l1] = measured(Odds::loot(1));
        let [_, _, r5, l5] = measured(Odds::loot(5));
        let ratio = (r5 + l5) / (r1 + l1);
        assert!((1.8..2.2).contains(&ratio), "rare and legendary: {} at depth 1, {} at depth 5", r1 + l1, r5 + l5);
        assert!(l5 > 1.6 * l1, "legendary: {l1} at depth 1, {l5} at depth 5");
    }

    #[test]
    fn odds_improve_smoothly_with_depth() {
        let shares: Vec<[f64; 4]> = (1..=40).map(|d| Odds::loot(d).shares()).collect();
        for pair in shares.windows(2) {
            let ([c0, m0, r0, l0], [c1, m1, r1, l1]) = (pair[0], pair[1]);
            assert!(c1 < c0 && m1 > m0 && r1 > r0 && l1 > l0, "{pair:?}");
        }
        assert!(shares.iter().flatten().all(|&s| s > 0.0));
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
        assert_eq!(Odds::kit(2).cap, Rarity::Legendary);
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
    fn hand_picked_affixes_set_rarity() {
        let by_name = |n: &str| affixes().iter().find(|a| a.name == n).unwrap();
        assert_eq!(Rarity::of_affixes(&[]), Rarity::Common);
        assert_eq!(Rarity::of_affixes(&[by_name("Rusted")]), Rarity::Common);
        assert_eq!(Rarity::of_affixes(&[by_name("Keen")]), Rarity::Magic);
        assert_eq!(Rarity::of_affixes(&[by_name("Keen"), by_name("of the Ox")]), Rarity::Rare);
    }
}
