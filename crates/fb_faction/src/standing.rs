//! The delver's standing with each faction (#542): a number from -100 to +100, read as a [`Band`].
//!
//! Standing is shared across levels: a faction thinks of the delver the same way in every camp it
//! has, for the length of the delve. It starts from the distance between the faction's alignment
//! and the delver's ([`StandingRules::start`]) and moves by [`Standings::shift`]. A new delve
//! starts a fresh [`Standings`] in a new [`World`].
//!
//! Standing moves by [`Standings::on_kill`] (kills of what a faction likes or hates, each kill of
//! the same kind counting for less than the last), [`Standings::on_blow`], [`Standings::on_trade`],
//! [`Standings::on_gift`] and [`Standings::on_theft`] (#547), and starts shifted by how the delver
//! is dressed ([`Delver`]).
//!
//! Every number that decides a band or a start lives in [`StandingRules`], so a lab can tune
//! them; [`StandingRules::default`] holds the values the design (#314) decided.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use fb_body::Folk;

use crate::{Alignment, ClassKind, CreatureKind, Faction, Preference, World};

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
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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
    /// What the first kill of something a faction hates is worth (a liked one costs the same).
    /// Each further kill of that kind counts `kill_decay` times the one before, so the kills of
    /// one kind add up to at most `kill_amount / (1 - kill_decay)` (22 by default).
    pub kill_amount: f32,
    /// What each kill of a kind counts for, against the one before it (0 to 1).
    pub kill_decay: f32,
    /// Killing one of a faction's own members, to that faction. Not diminished.
    pub member_kill: i32,
    /// Striking one of a faction's members: standing falls by this and then to Hostile if it was
    /// higher ([`Standings::on_blow`]).
    pub blow: i32,
    /// A trade with a member.
    pub trade: i32,
    /// A gift to a member.
    pub gift: i32,
    /// Stealing from a member, when caught.
    pub theft: i32,
    /// At the start of a delve, a faction that likes the delver's class or folk starts this much
    /// higher for each, and one that hates it this much lower ([`Delver`]).
    pub dressed: i32,
}

impl Default for StandingRules {
    fn default() -> Self {
        StandingRules { hostile_max: -50, wary_max: -10, friendly_min: 10, allied_min: 50, start_same: 10,
            start_opposite: -30,
            // Our own numbers. TODO: a lab tunes these (#558).
            kill_amount: 2.2,
            kill_decay: 0.9,
            member_kill: -40,
            blow: -20,
            trade: 2,
            gift: 5,
            theft: -15,
            dressed: 8,
        }
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

impl StandingRules {
    /// The total standing `kills` kills of one liked or hated kind have moved a faction by,
    /// before the sign: `kill_amount` falling by `kill_decay` each kill, kept in whole points.
    pub fn kills_worth(&self, kills: u32) -> i32 {
        let sum = |n: u32| {
            let (a, d) = (self.kill_amount as f64, self.kill_decay as f64);
            if (1.0 - d).abs() < 1e-9 { a * n as f64 } else { a * (1.0 - d.powi(n as i32)) / (1.0 - d) }
        };
        sum(kills).trunc() as i32
    }
}

/// Who the delver is to a faction that cares: what they dress and live as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delver {
    pub alignment: Alignment,
    pub class: Option<ClassKind>,
    pub folk: Option<Folk>,
}

impl Delver {
    /// A delver known by alignment alone, which no faction's liking of a class or folk touches.
    pub fn of_alignment(alignment: Alignment) -> Delver {
        Delver { alignment, class: None, folk: None }
    }
}

/// What the delver has killed, as the factions weigh it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Victim {
    Creature(CreatureKind),
    /// A member of `faction`, of this `folk`, dressed as this `class` if it is one.
    Member { faction: Faction, folk: Folk, class: Option<ClassKind> },
}

/// How many kills of one liked or hated thing have counted toward one faction, for the decay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct KillCount {
    faction: Faction,
    thing: Preference,
    count: u32,
}

/// The delver's standing with each faction of a world, and which they have met.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Standings {
    standing: BTreeMap<Faction, Standing>,
    met: BTreeSet<Faction>,
    /// Kills counted per faction and liked or hated thing, which only matter to the decay.
    #[serde(default)]
    kills: Vec<KillCount>,
}

impl Standings {
    /// Every faction of `world` at its alignment start for a delver of `delver`'s alignment.
    pub fn start_from_alignment(delver: Alignment, world: &World) -> Standings {
        Standings::start_with(&StandingRules::default(), delver, world)
    }

    pub fn start_with(rules: &StandingRules, delver: Alignment, world: &World) -> Standings {
        Standings::start_as(rules, &Delver::of_alignment(delver), world)
    }

    /// Every faction of `world` at its alignment start for `delver`, moved by `rules.dressed` for
    /// each of the delver's class and folk it likes (up) or hates (down). Done once, here: the
    /// delver's class and folk do not change in a delve.
    pub fn start_as(rules: &StandingRules, delver: &Delver, world: &World) -> Standings {
        let standing = world
            .factions()
            .iter()
            .map(|f| {
                let mut value = rules.start(delver.alignment, f.alignment).value();
                for ours in delver.class.map(Preference::Class).into_iter().chain(delver.folk.map(Preference::Folk)) {
                    value += rules.dressed * (f.likes.contains(&ours) as i32 - f.hates.contains(&ours) as i32);
                }
                (f.id, Standing::new(value))
            })
            .collect();
        Standings { standing, met: BTreeSet::new(), kills: Vec::new() }
    }

    /// The delver has killed `victim`: every faction of `world` that likes or hates it moves
    /// ([`Standings::on_kill_with`] under the default rules).
    pub fn on_kill(&mut self, world: &World, victim: Victim) {
        self.on_kill_with(&StandingRules::default(), world, victim);
    }

    /// Standing is shared by the whole delve, so every faction of the world weighs the kill, not
    /// just those on this level. A faction that hates the victim (its kind, its folk, its
    /// faction or its class) rises, one that likes it falls, each by `kill_amount` for the first
    /// kill of that thing and `kill_decay` times less for each after
    /// ([`StandingRules::kills_worth`]); one that both likes and hates it ignores it. The victim's
    /// own faction falls by `member_kill`. Every faction whose standing moves is marked met.
    pub fn on_kill_with(&mut self, rules: &StandingRules, world: &World, victim: Victim) {
        let things: Vec<Preference> = match victim {
            Victim::Creature(kind) => vec![Preference::Creature(kind)],
            Victim::Member { faction, folk, class } => [Some(Preference::Faction(faction)), Some(Preference::Folk(folk)), class.map(Preference::Class)].into_iter().flatten().collect(),
        };
        for spec in world.factions() {
            if let Victim::Member { faction, .. } = victim
                && faction == spec.id
            {
                self.dealt(spec.id, rules.member_kill);
            }
            let hated = things.iter().copied().find(|t| spec.hates.contains(t));
            let liked = things.iter().copied().find(|t| spec.likes.contains(t));
            let (thing, sign) = match (hated, liked) {
                (Some(t), None) => (t, 1),
                (None, Some(t)) => (t, -1),
                _ => continue,
            };
            let count = match self.kills.iter_mut().find(|k| k.faction == spec.id && k.thing == thing) {
                Some(k) => {
                    k.count += 1;
                    k.count
                }
                None => {
                    self.kills.push(KillCount { faction: spec.id, thing, count: 1 });
                    1
                }
            };
            self.dealt(spec.id, sign * (rules.kills_worth(count) - rules.kills_worth(count - 1)));
        }
    }

    /// How many kills of `thing` have counted toward `faction`'s standing so far.
    pub fn kills_counted(&self, faction: Faction, thing: Preference) -> u32 {
        self.kills.iter().find(|k| k.faction == faction && k.thing == thing).map_or(0, |k| k.count)
    }

    /// The delver struck one of `faction`'s members: standing falls by `blow`, then to Hostile if
    /// it was higher, and the faction is met.
    pub fn on_blow(&mut self, faction: Faction) {
        self.on_blow_with(&StandingRules::default(), faction);
    }

    pub fn on_blow_with(&mut self, rules: &StandingRules, faction: Faction) {
        self.shift(faction, rules.blow);
        self.turn_hostile_with(rules, faction);
    }

    /// A trade with a member of `faction`. Called by the barter work. Meets the faction.
    pub fn on_trade(&mut self, faction: Faction) -> Standing {
        self.on_trade_with(&StandingRules::default(), faction)
    }

    pub fn on_trade_with(&mut self, rules: &StandingRules, faction: Faction) -> Standing {
        self.dealt(faction, rules.trade)
    }

    /// A gift to a member of `faction`.
    pub fn on_gift(&mut self, faction: Faction) -> Standing {
        self.on_gift_with(&StandingRules::default(), faction)
    }

    pub fn on_gift_with(&mut self, rules: &StandingRules, faction: Faction) -> Standing {
        self.dealt(faction, rules.gift)
    }

    /// Stealing from a member of `faction`, caught.
    pub fn on_theft(&mut self, faction: Faction) -> Standing {
        self.on_theft_with(&StandingRules::default(), faction)
    }

    pub fn on_theft_with(&mut self, rules: &StandingRules, faction: Faction) -> Standing {
        self.dealt(faction, rules.theft)
    }

    /// A dealing moved `faction`'s standing, so the delver has met them.
    fn dealt(&mut self, faction: Faction, amount: i32) -> Standing {
        self.met.insert(faction);
        self.shift(faction, amount)
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

    /// A world, and a faction in it that hates `kind`, set to standing 0.
    fn hater_of(kind: CreatureKind) -> (World, Faction, Standings) {
        for seed in 0..500 {
            let world = World::new(seed);
            if let Some(f) = world.factions().iter().find(|f| f.hates.contains(&Preference::Creature(kind))) {
                let mut s = Standings::start_from_alignment(Alignment::TrueNeutral, &world);
                let id = f.id;
                s.shift(id, -s.get(id).value());
                return (world, id, s);
            }
        }
        panic!("no faction hates {kind:?}");
    }

    #[test]
    fn six_spider_kills_take_a_spider_hater_from_neutral_to_friendly() {
        let (world, id, mut s) = hater_of(CreatureKind::Spiders);
        assert_eq!(s.band(id), Band::Neutral);
        let mut seen = Vec::new();
        for _ in 0..5 {
            s.on_kill(&world, Victim::Creature(CreatureKind::Spiders));
            seen.push(s.get(id).value());
        }
        assert_eq!(s.band(id), Band::Neutral, "{seen:?}");
        s.on_kill(&world, Victim::Creature(CreatureKind::Spiders));
        assert_eq!(s.band(id), Band::Friendly, "{seen:?} then {}", s.get(id).value());
    }

    #[test]
    fn a_hundred_rat_kills_are_worth_far_less_than_a_hundred_times_one() {
        let rules = StandingRules::default();
        let one = rules.kills_worth(1);
        assert!(one >= 2);
        let hundred = rules.kills_worth(100);
        assert!(hundred < one * 100 / 5, "{hundred} vs {}", one * 100);
        assert!(hundred > rules.kills_worth(10), "kills still count a little");
        let steps: Vec<i32> = (1..=40).map(|n| rules.kills_worth(n) - rules.kills_worth(n - 1)).collect();
        // Whole points make a step wobble by one, but the first ten kills are worth more than the second ten.
        assert!(steps[..10].iter().sum::<i32>() > steps[10..20].iter().sum::<i32>() && steps[10..20].iter().sum::<i32>() > steps[30..40].iter().sum::<i32>(), "{steps:?}");
        let (world, id, mut s) = hater_of(CreatureKind::Rats);
        for _ in 0..100 {
            s.on_kill(&world, Victim::Creature(CreatureKind::Rats));
        }
        assert_eq!(s.get(id).value(), hundred);
        assert_eq!(s.kills_counted(id, Preference::Creature(CreatureKind::Rats)), 100);
    }

    #[test]
    fn killing_a_liked_kind_lowers_standing() {
        let like = Preference::Creature(CreatureKind::Bats);
        let world = (0..500).map(World::new).find(|w| w.factions().iter().any(|f| f.likes.contains(&like))).unwrap();
        let id = world.factions().iter().find(|f| f.likes.contains(&like)).unwrap().id;
        let mut s = Standings::start_from_alignment(Alignment::TrueNeutral, &world);
        let before = s.get(id).value();
        for _ in 0..3 {
            s.on_kill(&world, Victim::Creature(CreatureKind::Bats));
        }
        assert!(s.get(id).value() < before, "{} vs {before}", s.get(id).value());
    }

    #[test]
    fn striking_a_member_drops_to_hostile_and_killing_one_costs_more() {
        let world = World::new(3);
        let (a, b) = (world.factions()[0].id, world.factions()[1].id);
        let mut s = Standings::start_from_alignment(Alignment::TrueNeutral, &world);
        s.shift(a, 80);
        s.on_blow(a);
        assert_eq!(s.band(a), Band::Hostile);
        assert!(s.has_met(a));
        let before = s.get(b).value();
        s.on_kill(&world, Victim::Member { faction: b, folk: world.faction(b).folk, class: None });
        assert!(s.get(b).value() < before, "{} vs {before}", s.get(b).value());
    }

    #[test]
    fn trades_and_gifts_raise_and_theft_lowers() {
        let mut s = Standings::default();
        let f = Faction(0);
        assert_eq!(s.on_trade(f).value(), 2);
        assert_eq!(s.on_gift(f).value(), 7);
        assert_eq!(s.on_theft(f).value(), -8);
    }

    #[test]
    fn a_faction_that_likes_or_hates_the_delvers_class_or_folk_starts_apart() {
        let rules = StandingRules::default();
        let mage = Preference::Class(ClassKind::Mage);
        let world = (0..500).map(World::new).find(|w| w.factions().iter().any(|f| f.hates.contains(&mage))).unwrap();
        let plain = Standings::start_as(&rules, &Delver::of_alignment(Alignment::TrueNeutral), &world);
        let as_mage = Standings::start_as(&rules, &Delver { alignment: Alignment::TrueNeutral, class: Some(ClassKind::Mage), folk: None }, &world);
        for f in world.factions() {
            let want = rules.dressed * (f.likes.contains(&mage) as i32 - f.hates.contains(&mage) as i32);
            assert_eq!(as_mage.get(f.id).value() - plain.get(f.id).value(), want, "{}", f.name);
        }
    }

    #[test]
    fn kill_counts_are_per_faction_and_kind_and_round_trip() {
        let rats = Preference::Creature(CreatureKind::Rats);
        let world = (0..500).map(World::new).find(|w| w.factions().iter().filter(|f| f.hates.contains(&rats)).count() >= 2).unwrap();
        let haters: Vec<Faction> = world.factions().iter().filter(|f| f.hates.contains(&rats)).map(|f| f.id).collect();
        let mut s = Standings::default();
        for _ in 0..4 {
            s.on_kill(&world, Victim::Creature(CreatureKind::Rats));
        }
        s.on_kill(&world, Victim::Creature(CreatureKind::Trolls));
        for h in &haters {
            assert_eq!(s.kills_counted(*h, rats), 4);
            assert_eq!(s.kills_counted(*h, Preference::Creature(CreatureKind::Snakes)), 0);
        }
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Standings>(&json).unwrap(), s);
        let old: Standings = serde_json::from_str(r#"{"standing":{},"met":[]}"#).unwrap();
        assert_eq!(old.kills_counted(Faction(0), rats), 0);
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
