//! What a delver has done, counted: one tally for every kind of deed.
//!
//! A [`Tally`] counts [`Deed`]s by what they involved ([`Subject`]): a kill counts against the
//! exact kind slain (a giant rat, a goblin, a succubus), the family it belongs to
//! ([`Kin`]) and the kind of weapon used; a hit taken counts against each kind of armour and
//! clothing worn. Rules read it ("20 kills of goblins", [`crate::devotion`]) and nothing else
//! needs to count. A new kind of event is a [`Deed`] variant, a new thing it can involve a
//! [`Subject`] variant; the counts are keyed by their names, so the saved tally needs no
//! migration for either.
//!
//! The game records into the tally in one place (`game/src/deeds.rs`), where kills and hits
//! are already resolved.

use std::collections::BTreeMap;

use armory::Family;
use serde::{Deserialize, Serialize};

use crate::kin::Kin;

/// Something a delver did or had done to them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Deed {
    /// Brought something to 0 hit points.
    Kill,
    /// Took a hit that hurt.
    HitTaken,
    /// Answered an offer made to them (taken or declined); the subject is what it was about.
    Answered,
    /// Spent a blessing charge (a cleric's, [`crate::devotion::DevotionRules::blessing_kills`]).
    Blessed,
}

impl Deed {
    fn key(self) -> &'static str {
        match self {
            Deed::Kill => "kill",
            Deed::HitTaken => "hit_taken",
            Deed::Answered => "answered",
            Deed::Blessed => "blessed",
        }
    }
}

/// What a deed involved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Subject {
    /// The exact kind of creature or person ("creature:giant spider", "folk:goblin").
    Kind(String),
    /// The family of creature ([`Kin`]).
    Family(Kin),
    /// The kind of weapon used.
    Weapon(Family),
    /// The kind of armour or clothing worn.
    Worn(Family),
}

impl Subject {
    fn key(&self) -> String {
        match self {
            Subject::Kind(kind) => format!("kind:{kind}"),
            Subject::Family(kin) => format!("family:{}", kin.key()),
            Subject::Weapon(family) => format!("weapon:{}", family.key()),
            Subject::Worn(family) => format!("worn:{}", family.key()),
        }
    }
}

/// The count of each deed against each subject, saved with the delver.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Tally(BTreeMap<String, u32>);

impl Tally {
    fn key(deed: Deed, subject: &Subject) -> String {
        format!("{}/{}", deed.key(), subject.key())
    }

    /// Counts `deed` once against `subject`.
    pub fn record(&mut self, deed: Deed, subject: Subject) {
        *self.0.entry(Tally::key(deed, &subject)).or_default() += 1;
    }

    /// How many times `deed` has been done against `subject`.
    pub fn count(&self, deed: Deed, subject: &Subject) -> u32 {
        self.0.get(&Tally::key(deed, subject)).copied().unwrap_or(0)
    }

    /// A kill of `kind` (its exact kind, "folk:goblin"), of `kin`'s family, with a weapon of
    /// `weapon` if there was one in hand.
    pub fn kill(&mut self, kind: impl Into<String>, kin: Kin, weapon: Option<Family>) {
        self.record(Deed::Kill, Subject::Kind(kind.into()));
        self.record(Deed::Kill, Subject::Family(kin));
        if let Some(weapon) = weapon {
            self.record(Deed::Kill, Subject::Weapon(weapon));
        }
    }

    /// A hit taken while wearing armour and clothing of each of the `worn` kinds.
    pub fn hit_taken(&mut self, worn: impl IntoIterator<Item = Family>) {
        let mut seen = Vec::new();
        for family in worn {
            if !seen.contains(&family) {
                seen.push(family);
                self.record(Deed::HitTaken, Subject::Worn(family));
            }
        }
    }

    /// Kills of the family `kin`: "kills of undead".
    pub fn kills_of(&self, kin: Kin) -> u32 {
        self.count(Deed::Kill, &Subject::Family(kin))
    }

    /// Kills of exactly `kind`.
    pub fn kills_of_kind(&self, kind: &str) -> u32 {
        self.count(Deed::Kill, &Subject::Kind(kind.to_string()))
    }

    /// Kills with a weapon of `family`: "kills with maces".
    pub fn kills_with(&self, family: Family) -> u32 {
        self.count(Deed::Kill, &Subject::Weapon(family))
    }

    /// Blessing charges spent.
    pub fn blessings_spent(&self) -> u32 {
        self.count(Deed::Blessed, &Subject::Kind("charge".into()))
    }

    /// Spends `charges` blessing charges.
    pub fn spend_blessings(&mut self, charges: u32) {
        for _ in 0..charges {
            self.record(Deed::Blessed, Subject::Kind("charge".into()));
        }
    }

    /// Hits taken in armour or clothing of `family`: "hits taken in mail".
    pub fn hits_in(&self, family: Family) -> u32 {
        self.count(Deed::HitTaken, &Subject::Worn(family))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_body::Folk;
    use fb_items::Foe;

    #[test]
    fn a_kill_counts_its_kind_its_family_and_its_weapon() {
        let mut tally = Tally::default();
        tally.kill("creature:skeleton", Kin::Foe(Foe::Undead), Some(Family::Mace));
        tally.kill("creature:skeleton", Kin::Foe(Foe::Undead), Some(Family::Mace));
        tally.kill("creature:giant spider", Kin::Foe(Foe::Spider), None);
        assert_eq!(tally.kills_of(Kin::Foe(Foe::Undead)), 2);
        assert_eq!(tally.kills_of_kind("creature:skeleton"), 2);
        assert_eq!(tally.kills_with(Family::Mace), 2);
        assert_eq!(tally.kills_with(Family::Sword), 0);
        assert_eq!(tally.kills_of(Kin::Foe(Foe::Spider)), 1);
        assert_eq!(tally.kills_of(Kin::Folk(Folk::Goblin)), 0);
    }

    #[test]
    fn exact_kinds_of_one_family_count_together_by_family() {
        let mut tally = Tally::default();
        tally.kill("creature:rat", Kin::Foe(Foe::Vermin), None);
        tally.kill("creature:giant centipede", Kin::Foe(Foe::Vermin), None);
        assert_eq!(tally.kills_of(Kin::Foe(Foe::Vermin)), 2);
        assert_eq!((tally.kills_of_kind("creature:rat"), tally.kills_of_kind("creature:giant centipede")), (1, 1));
    }

    #[test]
    fn a_hit_counts_each_kind_worn_once() {
        let mut tally = Tally::default();
        tally.hit_taken([Family::Garment, Family::Helm, Family::Garment]);
        tally.hit_taken([Family::Garment]);
        assert_eq!((tally.hits_in(Family::Garment), tally.hits_in(Family::Helm), tally.hits_in(Family::Feet)), (2, 1, 0));
    }

    #[test]
    fn the_tally_round_trips_through_json_and_an_old_save_has_none() {
        let mut tally = Tally::default();
        tally.kill("folk:goblin", Kin::Folk(Folk::Goblin), Some(Family::Axe));
        tally.hit_taken([Family::Legs]);
        tally.record(Deed::Answered, Subject::Family(Kin::Folk(Folk::Goblin)));
        let back: Tally = serde_json::from_str(&serde_json::to_string(&tally).unwrap()).unwrap();
        assert_eq!(back, tally);
        assert_eq!(back.kills_of(Kin::Folk(Folk::Goblin)), 1);
        assert_eq!(serde_json::from_str::<Tally>("{}").unwrap(), Tally::default());
    }
}
