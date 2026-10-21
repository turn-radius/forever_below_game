//! Professions, tiers and where the masters work.
//!
//! A folk may have a [`Trade`]: a [`Profession`], a [`Tier`] and, for smiths, a [`Style`]. Tiers
//! open by depth ([`TradeRules`]): novices and apprentices from level 1, experts from level 3,
//! and masters only where the world seed places them ([`Masters`], from [`World::masters`]), at
//! most one master of a profession on a level and none above level 6. A level reads the masters
//! placed on it and generates them there, so a folk can name a real master on a real level and
//! the rumour is always true. Everything is a pure function of the seeds.

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::{ArmourLean, World, mix, shuffle};

const MASTER_SALT: u32 = 0xFAC7_0010;

/// What a folk does for a living. Guards sell nothing and keep the faction's doors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profession {
    Armorer,
    Weaponsmith,
    Bowyer,
    Tentmaker,
    GeneralGoods,
    Alchemist,
    Enchanter,
    Guard,
}

impl Profession {
    /// The professions that trade (all but the guard).
    pub const TRADES: [Profession; 7] = [
        Profession::Armorer,
        Profession::Weaponsmith,
        Profession::Bowyer,
        Profession::Tentmaker,
        Profession::GeneralGoods,
        Profession::Alchemist,
        Profession::Enchanter,
    ];

    /// Lower case, as it reads after a tier: "weaponsmith", "general goods merchant".
    pub fn label(self) -> &'static str {
        match self {
            Profession::Armorer => "armorer",
            Profession::Weaponsmith => "weaponsmith",
            Profession::Bowyer => "bowyer",
            Profession::Tentmaker => "tentmaker",
            Profession::GeneralGoods => "general goods merchant",
            Profession::Alchemist => "alchemist",
            Profession::Enchanter => "enchanter",
            Profession::Guard => "guard",
        }
    }

    pub fn is_smith(self) -> bool {
        matches!(self, Profession::Armorer | Profession::Weaponsmith)
    }
}

/// How skilled a trader is, least to most.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Novice,
    Apprentice,
    Expert,
    Master,
}

impl Tier {
    pub const ALL: [Tier; 4] = [Tier::Novice, Tier::Apprentice, Tier::Expert, Tier::Master];

    pub fn label(self) -> &'static str {
        match self {
            Tier::Novice => "novice",
            Tier::Apprentice => "apprentice",
            Tier::Expert => "expert",
            Tier::Master => "master",
        }
    }
}

/// What a smith specialises in: an armorer's armour, a weaponsmith's weapons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
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

impl Style {
    pub const ARMOUR: [Style; 5] = [Style::Padded, Style::Leather, Style::Mail, Style::Scale, Style::Plate];
    pub const WEAPONS: [Style; 4] = [Style::Blades, Style::AxesAndClubs, Style::Maces, Style::SpearsAndStaves];

    pub fn label(self) -> &'static str {
        match self {
            Style::Padded => "padded",
            Style::Leather => "leather",
            Style::Mail => "mail",
            Style::Scale => "scale",
            Style::Plate => "plate",
            Style::Blades => "blades",
            Style::AxesAndClubs => "axes and clubs",
            Style::Maces => "maces",
            Style::SpearsAndStaves => "spears and staves",
        }
    }

    /// The armour style of a faction's lean.
    pub fn of_lean(lean: ArmourLean) -> Style {
        match lean {
            ArmourLean::Padded => Style::Padded,
            ArmourLean::Leather => Style::Leather,
            ArmourLean::Mail => Style::Mail,
            ArmourLean::Scale => Style::Scale,
            ArmourLean::Plate => Style::Plate,
        }
    }
}

/// A folk's trade.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Trade {
    pub profession: Profession,
    /// A guard's is always [`Tier::Novice`] and never shown.
    pub tier: Tier,
    /// A smith's specialty.
    pub style: Option<Style>,
}

impl Trade {
    pub fn guard() -> Trade {
        Trade { profession: Profession::Guard, tier: Tier::Novice, style: None }
    }

    /// As the folk card writes it: "Apprentice weaponsmith (blades)", "Master alchemist", "Guard".
    pub fn label(&self) -> String {
        if self.profession == Profession::Guard {
            return "Guard".to_string();
        }
        let mut text = format!("{} {}", self.tier.label(), self.profession.label());
        if let Some(style) = self.style {
            text = format!("{text} ({})", style.label());
        }
        let mut chars = text.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().chain(chars).collect(),
            None => text,
        }
    }
}

/// The numbers trades run on, here so a lab can tune them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TradeRules {
    /// First level with experts.
    pub expert_from: u32,
    /// First level that can hold a master.
    pub master_from: u32,
    /// Deepest level the world places masters on.
    pub deepest_master: u32,
    /// Chance that a level from `master_from` on holds a master.
    pub master_odds: f64,
    /// Chance that such a level holds a second master, of another trade.
    pub second_master_odds: f64,
    /// Share of a camp with a trade or a guard post, about half.
    pub trade_share: f64,
    /// Extra folk with trades in a camp of a trade-intention faction.
    pub trade_faction_extra: f64,
    /// Chance that a camp of at least four has a second guard.
    pub second_guard_odds: f64,
    /// Chance that an armorer takes the faction's own armour lean.
    pub style_lean: f64,
    /// Weights of novice, apprentice and expert among those who are not masters.
    pub tier_weights: [u32; 3],
}

impl TradeRules {
    pub const DEFAULT: TradeRules = TradeRules {
        expert_from: 3,
        master_from: 6,
        deepest_master: 30,
        master_odds: 0.6,
        second_master_odds: 0.3,
        trade_share: 0.5,
        trade_faction_extra: 1.0,
        second_guard_odds: 0.3,
        style_lean: 0.75,
        tier_weights: [3, 3, 2],
    };

    /// The tiers a folk may roll (never master) at `depth`, with weights.
    pub fn tiers_at(&self, depth: u32) -> Vec<(Tier, u32)> {
        let [n, a, e] = self.tier_weights;
        let mut tiers = vec![(Tier::Novice, n), (Tier::Apprentice, a)];
        if depth >= self.expert_from {
            tiers.push((Tier::Expert, e));
        }
        tiers
    }
}

impl Default for TradeRules {
    fn default() -> Self {
        TradeRules::DEFAULT
    }
}

/// A master the world places: where they work and what they make. Their faction is not fixed
/// here: a level resolves it from `slot` among the factions that hold folk on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Master {
    pub profession: Profession,
    pub depth: u32,
    pub style: Option<Style>,
    /// A stable draw that picks the faction, as `held[slot % held.len()]`.
    pub slot: u32,
}

impl Master {
    pub fn trade(&self) -> Trade {
        Trade { profession: self.profession, tier: Tier::Master, style: self.style }
    }
}

/// Every master of a world, shallowest first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Masters {
    masters: Vec<Master>,
}

impl Masters {
    pub fn all(&self) -> &[Master] {
        &self.masters
    }

    /// The masters who work on `depth`, at most one of each profession.
    pub fn on(&self, depth: u32) -> impl Iterator<Item = &Master> {
        self.masters.iter().filter(move |m| m.depth == depth)
    }

    /// The masters of `profession`, shallowest first.
    pub fn of(&self, profession: Profession) -> impl Iterator<Item = &Master> {
        self.masters.iter().filter(move |m| m.profession == profession)
    }

    /// The nearest master of `profession` below `depth`, for a rumour.
    pub fn deeper(&self, profession: Profession, depth: u32) -> Option<&Master> {
        self.of(profession).find(|m| m.depth > depth)
    }
}

impl World {
    /// Where the world places its masters.
    pub fn masters(&self, rules: &TradeRules) -> Masters {
        let mut masters = Vec::new();
        // Professions come off shuffled bags of all seven, so every one gets masters in turn
        // and none twice on a level.
        let mut bag_rng = Mulberry32::new(mix(self.seed, MASTER_SALT, 0));
        let mut bag: Vec<Profession> = Vec::new();
        for depth in rules.master_from..=rules.deepest_master {
            let mut rng = Mulberry32::new(mix(self.seed, MASTER_SALT, depth));
            if !rng.chance(rules.master_odds) {
                continue;
            }
            let count = if rng.chance(rules.second_master_odds) { 2 } else { 1 };
            let mut trades: Vec<Profession> = Vec::new();
            while trades.len() < count {
                if bag.is_empty() {
                    bag = Profession::TRADES.to_vec();
                    shuffle(&mut bag, &mut bag_rng);
                }
                let next = bag.pop().expect("a bag was just filled");
                if !trades.contains(&next) {
                    trades.push(next);
                } else {
                    // Put the repeat back under the next fill.
                    bag.insert(0, next);
                }
            }
            for &profession in &trades {
                let slot = rng.int(0, u32::MAX);
                let style = match profession {
                    Profession::Armorer => Some(*rng.pick(&Style::ARMOUR)),
                    Profession::Weaponsmith => Some(*rng.pick(&Style::WEAPONS)),
                    _ => None,
                };
                masters.push(Master { profession, depth, style, slot });
            }
        }
        Masters { masters }
    }
}

/// An armorer's style: the faction's lean with chance [`TradeRules::style_lean`].
pub fn armour_style(lean: ArmourLean, rules: &TradeRules, rng: &mut Mulberry32) -> Style {
    let own = rng.chance(rules.style_lean);
    let other = *rng.pick(&Style::ARMOUR);
    if own { Style::of_lean(lean) } else { other }
}
