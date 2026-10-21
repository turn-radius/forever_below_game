//! Item names, and the titles and histories of legendary items: the plate's name line in
//! `rollSA`, `genName` and `legendize` (`reference/forever-below-plate.html`).

use crate::Tag;
use crate::armory_base::armory_base;
use crate::tables::{Affix, AffixKind, places, syllables};
use procgen_rng::Mulberry32;

/// What to call the shape rolled for `base`: the base's own name where the table says so
/// (`Blind cavefish`, `Shirt`), otherwise the Armory's noun (`forgeNoun`).
pub fn noun(base: &str, shape: &armory::Item) -> String {
    match armory_base(base) {
        Some(b) if b.named_by_base => base.to_string(),
        _ => shape.noun(),
    }
}

/// `<prefix> <noun> <suffix>`, from the first prefix and first suffix among `affixes`.
pub fn item_name(noun: &str, affixes: &[&Affix]) -> String {
    let first = |k: AffixKind| affixes.iter().find(|a| a.kind == k).map(|a| a.name.as_str());
    [first(AffixKind::Prefix), Some(noun), first(AffixKind::Suffix)].into_iter().flatten().collect::<Vec<_>>().join(" ")
}

/// A name for one of `folk` (a key of `data/syl.json`): two syllables, and a family name of
/// two more except for goblins and lizardfolk (`genName`).
pub fn folk_name(rng: &mut Mulberry32, folk: &str) -> String {
    let (a, b) = &syllables()[folk];
    let first = format!("{}{}", rng.pick(a), rng.pick(b));
    let last = format!("{}{}", rng.pick(a), rng.pick(b));
    if matches!(folk, "goblin" | "lizard") { first } else { format!("{first} {last}") }
}

/// A name for someone of any folk.
pub fn anyone(rng: &mut Mulberry32) -> String {
    let folk: Vec<&String> = syllables().keys().collect();
    let folk = rng.pick(&folk).as_str();
    folk_name(rng, folk)
}

/// A legendary's title, the name it had before, and three lines of history.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Legend {
    pub title: String,
    pub subtitle: String,
    /// Where it was made and by whom; who carried it and what they did; how it was lost.
    pub history: [String; 3],
}

/// `legendize`: a title (`<owner>'s <epithet>` or `The <adjective> <epithet>`, the epithet
/// chosen for what the item is) over its old `name`, and its history.
pub fn legendize(rng: &mut Mulberry32, name: &str, tags: &[Tag]) -> Legend {
    legendize_in(rng, name, tags, None)
}

/// [`legendize`] for a world: the factions its history names are picked from `factions`, the
/// world's own names, when there are any, and from the faction name grammar otherwise.
pub fn legendize_in(rng: &mut Mulberry32, name: &str, tags: &[Tag], factions: Option<&[String]>) -> Legend {
    let owner = anyone(rng).split(' ').next().unwrap_or_default().to_string();
    let epithets: &[&str] = if tags.contains(&Tag::Weapon) {
        &["Tooth", "Oath", "Answer", "Thorn", "Reckoning", "Mercy"]
    } else if tags.contains(&Tag::Light) {
        &["Wick", "Ember", "Vigil", "Last Light"]
    } else if tags.contains(&Tag::Trinket) {
        &["Eye", "Promise", "Keepsake", "Secret", "Heart"]
    } else {
        &["Comfort", "Promise", "Shroud", "Second Skin", "Wanderings"]
    };
    let adjectives = ["Grey", "Last", "Hollow", "Ninth", "Drowned", "Patient", "Ashen", "Quiet", "Long"];
    let title = if rng.chance(0.5) {
        format!("{owner}'s {}", rng.pick(epithets))
    } else {
        let adjective = rng.pick(&adjectives);
        format!("The {adjective} {}", rng.pick(epithets))
    };

    let made_on = rng.int(2, 10);
    let lost_on = made_on + rng.int(3, 22);
    let bearer = anyone(rng);
    let faction = legend_faction(rng, factions);
    let place = rng.pick(places()).clone();
    let made = format!(
        "{} on level {made_on} by {}.",
        rng.pick(&["Forged", "Stitched", "Carved", "Bound", "Made"]),
        legend_faction(rng, factions)
    );
    let deeds = [
        format!("settled {place} for a season"),
        format!("kept faith with {faction}"),
        format!("broke a quest-oath to {faction} and kept this instead"),
        "traded six days of rations for it".to_string(),
    ];
    let carried = format!("Carried by {bearer}, who {}.", rng.pick(&deeds));
    let lost = format!(
        "Lost on level {lost_on} when {}.",
        rng.pick(&[
            "a gelatinous cube took its bearer",
            "the last torch went out",
            "fever took them in their bedroll",
            "a ghoul pack caught them sleeping",
            "the stair gave way",
        ])
    );
    Legend { title, subtitle: name.to_string(), history: [made, carried, lost] }
}

/// A faction named in a legend's history, from the faction name grammar: any folk's, any
/// intention's. An item carries no world, so this is a faction of the sort the abyss has rather
/// than one of a particular world's pool, unless `world` gives that pool's names.
fn legend_faction(rng: &mut Mulberry32, world: Option<&[String]>) -> String {
    if let Some(names) = world.filter(|n| !n.is_empty()) {
        return rng.pick(names).clone();
    }
    const FOLK: [&str; 6] = ["human", "dwarf", "gnome", "goblin", "lizardfolk", "pigfolk"];
    const INTENTIONS: [&str; 4] = ["way_out", "trade", "worship", "plunder"];
    let (folk, intention) = (*rng.pick(&FOLK), *rng.pick(&INTENTIONS));
    procgen_grammar::faction_name(folk, intention, rng.int(0, u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_rng;
    use crate::tables::affixes;

    fn affix(name: &str) -> &'static Affix {
        affixes().iter().find(|a| a.name == name).unwrap()
    }

    #[test]
    fn names_wrap_the_noun() {
        assert_eq!(item_name("Longsword", &[]), "Longsword");
        assert_eq!(item_name("Longsword", &[affix("Keen")]), "Keen Longsword");
        assert_eq!(item_name("Longsword", &[affix("of the Ox")]), "Longsword of the Ox");
        assert_eq!(item_name("Longsword", &[affix("Keen"), affix("of the Ox"), affix("of Plenty")]), "Keen Longsword of the Ox");
    }

    #[test]
    fn some_bases_keep_their_own_name() {
        use crate::armory_base::roll_shape;
        let fish = roll_shape("Blind cavefish", 3).unwrap();
        assert_eq!(noun("Blind cavefish", &fish), "Blind cavefish");
        let sword = roll_shape("Sword", 3).unwrap();
        assert_eq!(noun("Sword", &sword), sword.noun());
    }

    #[test]
    fn goblins_and_lizardfolk_have_one_name() {
        let mut rng = game_rng(1);
        for _ in 0..20 {
            assert!(!folk_name(&mut rng, "goblin").contains(' '));
            assert!(!folk_name(&mut rng, "lizard").contains(' '));
            assert_eq!(folk_name(&mut rng, "dwarf").split(' ').count(), 2);
        }
    }

    #[test]
    fn legends_are_titled_and_told() {
        for seed in 0..200 {
            let legend = legendize(&mut game_rng(seed), "Keen Longsword", &[Tag::Weapon, Tag::Metal]);
            assert_eq!(legend.subtitle, "Keen Longsword");
            assert!(legend.title.starts_with("The ") || legend.title.contains("'s "), "{}", legend.title);
            assert!(["Tooth", "Oath", "Answer", "Thorn", "Reckoning", "Mercy"].iter().any(|e| legend.title.ends_with(e)));
            assert!(legend.history[0].contains(" on level ") && legend.history[0].contains(" by the "), "{}", legend.history[0]);
            assert!(!legend.history[0].contains("Nameless"));
            assert!(legend.history[1].starts_with("Carried by "));
            assert!(legend.history[2].starts_with("Lost on level "));
            assert_eq!(legend, legendize(&mut game_rng(seed), "Keen Longsword", &[Tag::Weapon, Tag::Metal]));
        }
    }
}
