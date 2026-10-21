//! The delver's standing with each faction (#542): a number from -100 to +100, read as a [`Band`].
//!
//! Standing is shared across levels: a faction thinks of the delver the same way in every camp it
//! has, for the length of the delve. It starts from the distance between the faction's alignment
//! and the delver's ([`StandingRules::start`]) and moves by [`Standings::shift`]. A new delve
//! starts a fresh [`Standings`] in a new [`World`].
//!
//! Every number that decides a band or a start lives in [`StandingRules`], so a lab can tune
//! them; [`StandingRules::default`] holds the values the design (#314) decided.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{Alignment, Faction, World};

/// The lowest standing.
pub const MIN: i32 = -100;
/// The highest standing.
pub const MAX: i32 = 100;

/// One standing, clamped to -100..=100.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(from = "i32", into = "i32")]
pub struct Standing(i32);

impl From<i32> for Standing {
    fn from(value: i32) -> Standing {
        Standing::new(value)
    }
}

impl From<Standing> for i32 {
    fn from(s: Standing) -> i32 {
        s.0
    }
}

impl Standing {
    pub fn new(value: i32) -> Standing {
        Standing(value.clamp(MIN, MAX))
    }

    pub fn value(self) -> i32 {
        self.0
    }

    /// This standing moved by `amount`, kept in range.
    pub fn shifted(self, amount: i32) -> Standing {
        Standing::new(self.0.saturating_add(amount))
    }

    /// The band this standing reads as under the default rules.
    pub fn band(self) -> Band {
        self.band_with(&StandingRules::default())
    }

    pub fn band_with(self, rules: &StandingRules) -> Band {
        let v = self.0;
        if v <= rules.hostile_max {
            Band::Hostile
        } else if v <= rules.wary_max {
            Band::Wary
        } else if v < rules.friendly_min {
            Band::Neutral
        } else if v < rules.allied_min {
            Band::Friendly
        } else {
            Band::Allied
        }
    }
}

/// How a standing reads, worst to best.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    Hostile,
    Wary,
    Neutral,
    Friendly,
    Allied,
}

impl Band {
    pub const ALL: [Band; 5] = [Band::Hostile, Band::Wary, Band::Neutral, Band::Friendly, Band::Allied];

    pub fn label(self) -> &'static str {
        match self {
            Band::Hostile => "Hostile",
            Band::Wary => "Wary",
            Band::Neutral => "Neutral",
            Band::Friendly => "Friendly",
            Band::Allied => "Allied",
        }
    }
}

/// The numbers standing runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StandingRules {
    /// Hostile is this and below (-100 to -50).
    pub hostile_max: i32,
    /// Wary is above `hostile_max` up to this (-49 to -10).
    pub wary_max: i32,
    /// Friendly starts here (+10 to +49); Neutral is between `wary_max` and this (-9 to +9).
    pub friendly_min: i32,
    /// Allied starts here (+50 to +100).
    pub allied_min: i32,
    /// Where a delver of the faction's own alignment starts.
    pub start_same: i32,
    /// Where a delver of the opposite alignment (the far corner of the grid) starts.
    pub start_opposite: i32,
}

impl Default for StandingRules {
    fn default() -> Self {
        StandingRules { hostile_max: -50, wary_max: -10, friendly_min: 10, allied_min: 50, start_same: 10, start_opposite: -30 }
    }
}

/// Steps between two alignments across the grid, one per notch on each axis: 0 for the same, 4
/// for the opposite corners.
pub const MAX_ALIGNMENT_DISTANCE: i32 = 4;

impl Alignment {
    /// (law, good): 0 lawful/good, 1 neutral, 2 chaotic/evil.
    fn grid(self) -> (i32, i32) {
        match self {
            Alignment::LawfulGood => (0, 0),
            Alignment::NeutralGood => (1, 0),
            Alignment::ChaoticGood => (2, 0),
            Alignment::LawfulNeutral => (0, 1),
            Alignment::TrueNeutral => (1, 1),
            Alignment::ChaoticNeutral => (2, 1),
            Alignment::LawfulEvil => (0, 2),
            Alignment::NeutralEvil => (1, 2),
            Alignment::ChaoticEvil => (2, 2),
        }
    }

    /// Notches apart on the alignment grid, 0 to [`MAX_ALIGNMENT_DISTANCE`].
    pub fn distance(self, other: Alignment) -> i32 {
        let (a, b) = (self.grid(), other.grid());
        (a.0 - b.0).abs() + (a.1 - b.1).abs()
    }
}

impl StandingRules {
    /// The starting standing between two alignments: `start_same` for the same, falling evenly to
    /// `start_opposite` at the far corner.
    pub fn start(&self, delver: Alignment, faction: Alignment) -> Standing {
        let d = delver.distance(faction);
        let span = self.start_opposite - self.start_same;
        Standing::new(self.start_same + span * d / MAX_ALIGNMENT_DISTANCE)
    }
}

/// The delver's standing with each faction of a world, and which they have met.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Standings {
    standing: BTreeMap<Faction, Standing>,
    met: BTreeSet<Faction>,
}

impl Standings {
    /// Every faction of `world` at its alignment start for a delver of `delver`'s alignment.
    pub fn start_from_alignment(delver: Alignment, world: &World) -> Standings {
        Standings::start_with(&StandingRules::default(), delver, world)
    }

    pub fn start_with(rules: &StandingRules, delver: Alignment, world: &World) -> Standings {
        let standing = world.factions().iter().map(|f| (f.id, rules.start(delver, f.alignment))).collect();
        Standings { standing, met: BTreeSet::new() }
    }

    /// Standing with `faction`; neutral (0) for one that was never started.
    pub fn get(&self, faction: Faction) -> Standing {
        self.standing.get(&faction).copied().unwrap_or_default()
    }

    pub fn band(&self, faction: Faction) -> Band {
        self.get(faction).band()
    }

    /// Moves standing with `faction` by `amount`, kept in range. Returns the new standing.
    pub fn shift(&mut self, faction: Faction, amount: i32) -> Standing {
        let now = self.get(faction).shifted(amount);
        self.standing.insert(faction, now);
        now
    }

    /// The faction has turned on the delver: standing falls to the top of Hostile if it was
    /// higher, and stays lower if it already was.
    pub fn turn_hostile(&mut self, faction: Faction) {
        self.turn_hostile_with(&StandingRules::default(), faction);
    }

    pub fn turn_hostile_with(&mut self, rules: &StandingRules, faction: Faction) {
        let now = self.get(faction).value().min(rules.hostile_max);
        self.standing.insert(faction, Standing::new(now));
        self.met.insert(faction);
    }

    /// Notes that the delver has met `faction` this delve.
    pub fn meet(&mut self, faction: Faction) {
        self.met.insert(faction);
    }

    /// Whether the delver has met `faction` this delve.
    pub fn has_met(&self, faction: Faction) -> bool {
        self.met.contains(&faction)
    }

    /// The factions met this delve, in pool order.
    pub fn met(&self) -> impl Iterator<Item = Faction> + '_ {
        self.met.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_starts_run_from_plus_ten_to_minus_thirty() {
        let rules = StandingRules::default();
        for a in Alignment::ALL {
            assert_eq!(rules.start(a, a).value(), 10);
        }
        assert_eq!(rules.start(Alignment::LawfulGood, Alignment::ChaoticEvil).value(), -30);
        assert_eq!(rules.start(Alignment::ChaoticEvil, Alignment::LawfulGood).value(), -30);
        assert_eq!(rules.start(Alignment::LawfulGood, Alignment::TrueNeutral).value(), -10);
        assert_eq!(rules.start(Alignment::LawfulGood, Alignment::NeutralGood).value(), 0);
        for a in Alignment::ALL {
            for b in Alignment::ALL {
                let s = rules.start(a, b).value();
                assert!((-30..=10).contains(&s) && s == rules.start(b, a).value(), "{a:?} {b:?}");
            }
        }
        assert_eq!(Standing::new(10).band(), Band::Friendly);
        assert_eq!(Standing::new(-30).band(), Band::Wary);
    }

    #[test]
    fn bands_break_at_their_edges() {
        let band = |v| Standing::new(v).band();
        for (v, b) in [
            (-100, Band::Hostile),
            (-50, Band::Hostile),
            (-49, Band::Wary),
            (-10, Band::Wary),
            (-9, Band::Neutral),
            (0, Band::Neutral),
            (9, Band::Neutral),
            (10, Band::Friendly),
            (49, Band::Friendly),
            (50, Band::Allied),
            (100, Band::Allied),
        ] {
            assert_eq!(band(v), b, "{v}");
        }
    }

    #[test]
    fn standing_is_clamped_to_range() {
        assert_eq!(Standing::new(500).value(), 100);
        assert_eq!(Standing::new(-500).value(), -100);
        let mut s = Standings::default();
        assert_eq!(s.shift(Faction(0), 70).value(), 70);
        assert_eq!(s.shift(Faction(0), 70).value(), 100);
        assert_eq!(s.shift(Faction(0), -1000).value(), -100);
        assert_eq!(Standing::new(i32::MAX).shifted(i32::MAX).value(), 100);
    }

    #[test]
    fn a_world_starts_from_each_factions_alignment() {
        let world = World::new(3);
        let delver = Alignment::NeutralGood;
        let standings = Standings::start_from_alignment(delver, &world);
        for f in world.factions() {
            assert_eq!(standings.get(f.id), StandingRules::default().start(delver, f.alignment));
        }
        assert_eq!(standings.met().count(), 0);
    }

    #[test]
    fn turning_hostile_drops_to_hostile_and_never_raises() {
        let mut s = Standings::default();
        s.shift(Faction(1), 40);
        s.turn_hostile(Faction(1));
        assert_eq!(s.band(Faction(1)), Band::Hostile);
        assert_eq!(s.get(Faction(1)).value(), -50);
        s.shift(Faction(2), -90);
        s.turn_hostile(Faction(2));
        assert_eq!(s.get(Faction(2)).value(), -90);
        assert_eq!(s.met().collect::<Vec<_>>(), vec![Faction(1), Faction(2)]);
    }

    #[test]
    fn standings_round_trip_through_serde() {
        let world = World::new(8);
        let mut s = Standings::start_from_alignment(Alignment::ChaoticNeutral, &world);
        s.shift(Faction(2), 33);
        s.turn_hostile(Faction(0));
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Standings>(&json).unwrap(), s);
        assert_eq!(serde_json::to_string(&Standing::new(7)).unwrap(), "7");
        assert_eq!(serde_json::from_str::<Standing>("500").unwrap().value(), 100);
    }
}
