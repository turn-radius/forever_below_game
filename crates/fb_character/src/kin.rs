//! Families of creature, for a cleric's devotion.
//!
//! A [`Kin`] is what a favored foe is picked from and what kills are counted by: one of the
//! groups of creature that gear already wards against and is a bane to ([`Foe`]: the undead,
//! vermin, serpents, spiders, beasts, trolls), one of the six folk who delve (all the goblins are
//! one kin), or one of the denizens of the abyss ([`DenizenKind`]: a minotaur is its own kin, the
//! three bodies of demon are one). Any of them can be a favored foe or be aligned with; aligning
//! with a creature group takes a trait, with folk or denizens wins their factions' favour.
//!
//! The exact kind of each kill (a giant rat, a goblin, a succubus) is kept in the deed tally
//! ([`crate::deeds`]) next to its kin, so the kinds can be told apart later.

use fb_body::Folk;
use fb_creatures::Species;
use fb_denizens::DenizenKind;
use fb_items::Foe;
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

/// A family of creature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kin {
    /// A group of creatures gear wards against: the undead, vermin, serpents and the rest.
    Foe(Foe),
    /// One of the six folk who delve.
    Folk(Folk),
    /// A people of the abyss.
    Denizen(DenizenKind),
}

impl Kin {
    /// Every family there is.
    pub fn all() -> Vec<Kin> {
        Foe::ALL.into_iter().map(Kin::Foe).chain(Folk::ALL.into_iter().map(Kin::Folk)).chain(DenizenKind::ALL.into_iter().map(Kin::Denizen)).collect()
    }

    /// The family of a creature.
    pub fn of_species(species: Species) -> Kin {
        Kin::Foe(species.foe())
    }

    /// The family of a person of `folk`: a delving folk is its own, a body built on the folk body
    /// is its denizen people.
    pub fn of_folk(folk: Folk) -> Kin {
        folk.denizen_body().map_or(Kin::Folk(folk), |body| Kin::Denizen(body.kind()))
    }

    /// Whether factions are made of this family: the folk and the denizens, not the creatures.
    pub fn has_factions(self) -> bool {
        !matches!(self, Kin::Foe(_))
    }

    /// The creature group, if it is one.
    pub fn foe(self) -> Option<Foe> {
        if let Kin::Foe(foe) = self { Some(foe) } else { None }
    }

    /// The key in data tables and in the tally.
    pub fn key(self) -> &'static str {
        match self {
            Kin::Foe(Foe::Vermin) => "vermin",
            Kin::Foe(Foe::Undead) => "undead",
            Kin::Foe(Foe::Spider) => "spider",
            Kin::Foe(Foe::Serpent) => "serpent",
            Kin::Foe(Foe::Beast) => "beast",
            Kin::Foe(Foe::Troll) => "troll",
            Kin::Folk(folk) => folk.key(),
            Kin::Denizen(kind) => kind.key(),
        }
    }

    /// One of them as a name: "Goblin", "Undead", "Minotaur".
    pub fn noun(self) -> &'static str {
        match self {
            Kin::Foe(Foe::Vermin) => "Vermin",
            Kin::Foe(Foe::Undead) => "Undead",
            Kin::Foe(Foe::Spider) => "Spider",
            Kin::Foe(Foe::Serpent) => "Serpent",
            Kin::Foe(Foe::Beast) => "Beast",
            Kin::Foe(Foe::Troll) => "Troll",
            Kin::Folk(Folk::Human) => "Human",
            Kin::Folk(Folk::Dwarf) => "Dwarf",
            Kin::Folk(Folk::Gnome) => "Gnome",
            Kin::Folk(Folk::Goblin) => "Goblin",
            Kin::Folk(Folk::Lizardfolk) => "Lizardfolk",
            Kin::Folk(Folk::Pigfolk) => "Pigfolk",
            Kin::Folk(_) => "Folk",
            Kin::Denizen(kind) => kind.label(),
        }
    }

    /// The family as a plural phrase in a sentence: "goblins", "the undead", "vermin".
    pub fn plural(self) -> String {
        match self {
            Kin::Foe(Foe::Undead) => "the undead".to_string(),
            Kin::Foe(Foe::Vermin) => "vermin".to_string(),
            Kin::Folk(Folk::Lizardfolk) | Kin::Folk(Folk::Pigfolk) => self.noun().to_lowercase(),
            Kin::Folk(Folk::Dwarf) => "dwarves".to_string(),
            Kin::Denizen(DenizenKind::Cyclops) => "cyclopes".to_string(),
            other => format!("{}s", other.noun().to_lowercase()),
        }
    }

    /// The title a cleric who has devoted themselves against it carries: "Goblin Slayer", "Bane
    /// of the Minotaurs". Which of the patterns it is is rolled from `seed` (the delver's) and
    /// the family, so it is the same each time they are shown.
    pub fn title(self, seed: u32) -> String {
        let mut rng = Mulberry32::new(seed ^ hash(self.key()));
        let noun = self.noun();
        let plural = self.plural();
        let the = |s: &str| if s.starts_with("the ") { s.to_string() } else { format!("the {s}") };
        match rng.int(0, 3) {
            0 => format!("{noun} Slayer"),
            1 => format!("Bane of {}", capital_after_the(&the(&plural))),
            2 => format!("{noun}-Bane"),
            _ => format!("Scourge of {}", capital_after_the(&the(&plural))),
        }
    }
}

/// "the goblins" as "the Goblins": a bane's name is a name.
fn capital_after_the(text: &str) -> String {
    let rest = text.strip_prefix("the ").unwrap_or(text);
    let mut chars = rest.chars();
    let word = chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect());
    format!("the {word}")
}

fn hash(text: &str) -> u32 {
    text.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kin_has_a_unique_key_and_a_title() {
        let all = Kin::all();
        for (i, kin) in all.iter().enumerate() {
            assert!(all.iter().skip(i + 1).all(|other| other.key() != kin.key()), "{kin:?}");
            assert!(kin.title(7).to_lowercase().contains(&kin.noun().to_lowercase()[..3]), "{kin:?}: {}", kin.title(7));
        }
    }

    #[test]
    fn the_title_is_steady_for_a_delver_and_varies_between_them() {
        let goblin = Kin::Folk(Folk::Goblin);
        assert_eq!(goblin.title(5), goblin.title(5));
        let titles: std::collections::BTreeSet<String> = (0..40).map(|s| goblin.title(s)).collect();
        assert!(titles.contains("Goblin Slayer"), "{titles:?}");
        assert!(titles.len() > 1);
    }

    #[test]
    fn denizens_group_by_people() {
        assert_eq!(Kin::of_folk(Folk::Goblin), Kin::Folk(Folk::Goblin));
        assert_eq!(Kin::of_folk(Folk::Minotaur), Kin::Denizen(DenizenKind::Minotaur));
        for demon in [Folk::WingedDemon, Folk::HornedDemon, Folk::Succubus] {
            assert_eq!(Kin::of_folk(demon), Kin::Denizen(DenizenKind::Demon));
        }
        assert_eq!(Kin::of_species(Species::GiantCentipede), Kin::Foe(Foe::Vermin));
    }

    #[test]
    fn a_kin_round_trips_through_json() {
        for kin in Kin::all() {
            assert_eq!(serde_json::from_str::<Kin>(&serde_json::to_string(&kin).unwrap()).unwrap(), kin);
        }
    }
}
