//! Rarity and the affixes that come with it: the plate's `rollAffixes` and `pickAffix`
//! (`reference/forever-below-plate.html`; rules in `docs/items.md`, "Rarity and affixes").

use crate::tables::{Affix, AffixKind, affixes};
use crate::{Rarity, Tag};
use procgen_rng::Mulberry32;

impl Rarity {
    /// The rarity for a uniform roll `x` in `[0, 1)`: below 0.58 common, below 0.85 magic,
    /// below 0.97 rare, otherwise legendary.
    pub fn from_roll(x: f64) -> Rarity {
        match x {
            x if x < 0.58 => Rarity::Common,
            x if x < 0.85 => Rarity::Magic,
            x if x < 0.97 => Rarity::Rare,
            _ => Rarity::Legendary,
        }
    }

    /// One draw from `rng`. Containers cap at rare.
    pub fn roll(rng: &mut Mulberry32, tags: &[Tag]) -> Rarity {
        match Rarity::from_roll(rng.next_f64()) {
            Rarity::Legendary if tags.contains(&Tag::Container) => Rarity::Rare,
            r => r,
        }
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

/// Rarity, then its affixes, from one stream.
pub fn roll_rarity(rng: &mut Mulberry32, tags: &[Tag]) -> (Rarity, Vec<&'static Affix>) {
    let rarity = Rarity::roll(rng, tags);
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
            let (r, _) = roll_rarity(&mut game_rng(seed), SWORD);
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
            assert_ne!(roll_rarity(&mut game_rng(seed), PACK).0, Rarity::Legendary);
        }
    }

    #[test]
    fn affixes_fit_their_rarity_and_tags() {
        for tags in [SWORD, TUNIC, PACK, &[Tag::Light, Tag::Wood][..], &[Tag::Trinket][..]] {
            let mut flaws = 0;
            let mut commons = 0;
            for seed in 0..3_000 {
                let (rarity, chosen) = roll_rarity(&mut game_rng(seed), tags);
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
        let full = (0..3_000).map(|seed| roll_rarity(&mut game_rng(seed), SWORD)).find(|(r, _)| *r == Rarity::Legendary).unwrap();
        assert_eq!(full.1.len(), 3);
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
