//! A cleric's devotion: what they have given themselves to, and what it does for them.
//!
//! A cleric is devoted *against* a favored foe (more damage to that family of creature, and a
//! ward from its blows), *aligned* with a creature family (taking one of its traits) or
//! *dedicated* to a kind of weapon. A family is a [`Kin`]: any creature group, any of the folk
//! (aligned with, one racial trait) or any denizen of the abyss.
//!
//! The offer comes from the delver's deeds ([`crate::deeds::Tally`]): enough kills of one family,
//! or with one kind of weapon ([`due`]). They may decline, and the same offer comes again at
//! double the count. A later offer can switch them, and the bonuses are not stored, they are
//! worked out from the choice every time the gear is, so switching drops the old ones.
//!
//! A devotion becomes a [`Worn`], like an item's stats, so everything that reads
//! [`Gear`](crate::Gear) (damage dealt, damage taken, saves, webs) sees it with no new path.
//! A new kind of devotion is one more variant of [`Devotion`] that makes its own [`Worn`] in
//! [`Devotion::worn`] (the hallowing of a dedicated weapon will go there; dedicating to armour
//! and clothing can take the same [`Family`] with a trigger on hits taken).
//!
//! The numbers are `[devotion]` in `data/rules.toml` ([`DevotionRules`]).

use std::collections::BTreeMap;

use armory::Family;
use fb_items::dice::Dice;
use serde::{Deserialize, Serialize};

use crate::Worn;
use crate::deeds::{Deed, Subject, Tally};
use crate::kin::Kin;

/// What a cleric has devoted themselves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Devotion {
    /// Devoted against a family of creature: bonus damage against it and a ward from it.
    FavoredFoe(Kin),
    /// Aligned with a family of creature: some of its traits.
    Aligned(Kin),
    /// Dedicated to a kind of weapon, which becomes holy to them. Chosen and saved now; what
    /// hallowing does comes with the blessings of gear, so it adds nothing yet.
    Dedicated(Family),
}

/// The kinds of weapon a cleric can dedicate themselves to.
pub const WEAPON_KINDS: [Family; 9] = [Family::Staff, Family::Mace, Family::Sword, Family::Spear, Family::Dagger, Family::Club, Family::Axe, Family::Bow, Family::Crossbow];

/// The traits of being aligned with one family of creature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlignedTraits {
    /// Points added to saves against venom.
    pub poison_resistance: f64,
    /// Multiple of the chance of catching a disease.
    pub disease_risk: f64,
    /// Feet seen without light.
    pub darkvision: f64,
    /// Webs never hold them back.
    pub web_walker: bool,
    /// Points of Stealth.
    pub stealth: f64,
    /// Pounds of carrying capacity.
    pub carry: f64,
    /// Points of recovery.
    pub recovery: f64,
    /// Multiple of their pace.
    pub speed: f64,
    /// Points added to the shot roll.
    pub ranged_to_hit: f64,
    /// Points of luck.
    pub luck: f64,
    /// How the trait reads on the sheet.
    pub text: String,
}

impl Default for AlignedTraits {
    fn default() -> Self {
        AlignedTraits { poison_resistance: 0.0, disease_risk: 1.0, darkvision: 0.0, web_walker: false, stealth: 0.0, carry: 0.0, recovery: 0.0, speed: 1.0, ranged_to_hit: 0.0, luck: 0.0, text: String::new() }
    }
}

/// The numbers of a devotion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DevotionRules {
    /// The fraction of a favored foe's damage the cleric is warded from.
    pub favored_ward: f64,
    /// The extra dice the cleric rolls against a favored foe.
    pub favored_bane: Dice,
    /// How near, in world units (10 cm each), a living creature or hostile folk may be before an
    /// offer waits for a quieter moment. Our own.
    pub quiet_distance: f32,
    /// Kills with one kind of weapon before a cleric is offered to dedicate themselves to it.
    /// Declining makes the next offer come at double.
    pub weapon_kills: u32,
    /// Kills of one family of creature before a cleric is offered a devotion against or to it.
    pub creature_kills: u32,
    /// How much more a faction of a family the cleric is aligned with thinks of them: added to
    /// the standing with each faction whose folk or denizens are that family.
    pub aligned_standing: i32,
    /// The share of a loss of standing that lands with those factions, in percent.
    pub aligned_loss_percent: u32,
    /// How much less a faction of a family the cleric is devoted against thinks of them (a
    /// negative number added to the standing).
    pub favored_standing: i32,
    /// The trait of each creature group to be aligned with, by the group's key ([`Kin::key`]).
    /// Folk and denizens give favour instead of a trait.
    pub aligned: BTreeMap<String, AlignedTraits>,
}

impl Default for DevotionRules {
    fn default() -> Self {
        let entry = |kin: Kin, traits: AlignedTraits| (kin.key().to_string(), traits);
        use fb_items::Foe as F;
        DevotionRules {
            favored_ward: 0.25,
            favored_bane: Dice::new(1, 4),
            aligned_standing: 15,
            aligned_loss_percent: 50,
            favored_standing: -15,
            quiet_distance: 250.0,
            weapon_kills: 50,
            creature_kills: 20,
            aligned: BTreeMap::from([
                entry(Kin::Foe(F::Spider), AlignedTraits { web_walker: true, text: "Webs never hold you back.".into(), ..Default::default() }),
                entry(Kin::Foe(F::Serpent), AlignedTraits { poison_resistance: 3.0, text: "+3 to saves against venom.".into(), ..Default::default() }),
                entry(Kin::Foe(F::Vermin), AlignedTraits { disease_risk: 0.5, text: "Half as likely to catch a disease.".into(), ..Default::default() }),
                entry(Kin::Foe(F::Undead), AlignedTraits { darkvision: 60.0, text: "See 60 ft in the dark.".into(), ..Default::default() }),
                entry(Kin::Foe(F::Beast), AlignedTraits { stealth: 2.0, text: "+2 Stealth.".into(), ..Default::default() }),
                entry(Kin::Foe(F::Troll), AlignedTraits { recovery: 1.0, text: "+1 recovery.".into(), ..Default::default() }),
            ]),
        }
    }
}

impl DevotionRules {
    /// Whether a cleric can align with `kin`: a creature group with a trait to take, or any folk
    /// or denizen (whose factions favour them).
    pub fn can_align(&self, kin: Kin) -> bool {
        kin.has_factions() || self.aligned.contains_key(kin.key())
    }
}

impl Devotion {
    /// The family of creature it is about, if it is about one.
    pub fn kin(self) -> Option<Kin> {
        match self {
            Devotion::FavoredFoe(kin) | Devotion::Aligned(kin) => Some(kin),
            Devotion::Dedicated(_) => None,
        }
    }

    /// How the choice reads: "Goblin Slayer", "Aligned with spiders", "Dedicated to maces".
    /// `seed` (the delver's) rolls which title a favored foe's is.
    pub fn label(self, seed: u32) -> String {
        match self {
            Devotion::FavoredFoe(kin) => kin.title(seed),
            Devotion::Aligned(kin) => format!("Aligned with {}", kin.plural()),
            Devotion::Dedicated(family) => format!("Dedicated to {}", plural(family)),
        }
    }

    /// What it does, in a line.
    pub fn effect_text(self, rules: &DevotionRules) -> String {
        match self {
            Devotion::FavoredFoe(kin) => format!("+{} damage to {} and {:.0}% less from them.", rules.favored_bane, kin.plural(), rules.favored_ward * 100.0),
            Devotion::Aligned(kin) if kin.has_factions() => format!("+{} standing with {} and they forgive more.", rules.aligned_standing, kin.plural()),
            Devotion::Aligned(kin) => rules.aligned.get(kin.key()).map_or_else(String::new, |t| t.text.clone()),
            Devotion::Dedicated(family) => format!("{} are holy to you.", capital(plural(family))),
        }
    }

    /// What the devotion does to the standing with the factions of a family of folk or denizens:
    /// aligned, a bonus and smaller losses; against, a penalty. `None` for the rest.
    pub fn favour(self, rules: &DevotionRules) -> Option<(Kin, i32, u32)> {
        match self {
            Devotion::Aligned(kin) if kin.has_factions() => Some((kin, rules.aligned_standing, rules.aligned_loss_percent)),
            Devotion::FavoredFoe(kin) if kin.has_factions() => Some((kin, rules.favored_standing, 100)),
            _ => None,
        }
    }

    /// What it adds to the gear, under `rules`: the same shape an item's stats take.
    pub fn worn(self, rules: &DevotionRules) -> Worn {
        let mut worn = Worn::default();
        match self {
            Devotion::Dedicated(_) => {}
            Devotion::FavoredFoe(kin) => {
                worn.favored = Some(kin);
                worn.favored_ward = rules.favored_ward;
                worn.favored_bane = Some(rules.favored_bane);
            }
            Devotion::Aligned(kin) => {
                if let Some(t) = rules.aligned.get(kin.key()) {
                    worn.poison_resistance = t.poison_resistance;
                    worn.disease_risk = t.disease_risk;
                    worn.darkvision = t.darkvision;
                    worn.web_walker = t.web_walker;
                    worn.stealth = t.stealth;
                    worn.carry = t.carry;
                    worn.recovery = t.recovery;
                    worn.speed = t.speed;
                    worn.ranged_to_hit = t.ranged_to_hit;
                    worn.luck = t.luck;
                }
            }
        }
        worn
    }
}

/// A kind of weapon as a plural: "maces".
pub fn plural(family: Family) -> String {
    match family {
        Family::Staff => "staves".to_string(),
        other => format!("{}s", other.key()),
    }
}

fn capital(text: String) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// What a delver's deeds have earned them an offer of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offer {
    /// Enough kills with one kind of weapon: to dedicate themselves to it.
    Weapon(Family),
    /// Enough kills of one family of creature: to be devoted against it or aligned with it.
    Creature(Kin),
}

impl Offer {
    /// The devotions the offer lets them take.
    pub fn choices(self, rules: &DevotionRules) -> Vec<Devotion> {
        match self {
            Offer::Weapon(family) => vec![Devotion::Dedicated(family)],
            Offer::Creature(kin) => std::iter::once(Devotion::FavoredFoe(kin)).chain(rules.can_align(kin).then_some(Devotion::Aligned(kin))).collect(),
        }
    }

    /// What the deeds were: "50 kills with maces", "20 kills of goblins".
    pub fn headline(self, tally: &Tally) -> String {
        match self {
            Offer::Weapon(family) => format!("{} kills with {}", tally.kills_with(family), plural(family)),
            Offer::Creature(kin) => format!("{} kills of {}", tally.kills_of(kin), kin.plural()),
        }
    }

    fn subject(self) -> Subject {
        match self {
            Offer::Weapon(family) => Subject::Weapon(family),
            Offer::Creature(kin) => Subject::Family(kin),
        }
    }

    fn kills(self, tally: &Tally) -> u32 {
        match self {
            Offer::Weapon(family) => tally.kills_with(family),
            Offer::Creature(kin) => tally.kills_of(kin),
        }
    }
}

/// The kills `offer` next comes at: the rules' count, doubled for each time it was answered.
pub fn threshold(tally: &Tally, rules: &DevotionRules, offer: Offer) -> u32 {
    let base = match offer {
        Offer::Weapon(_) => rules.weapon_kills,
        Offer::Creature(_) => rules.creature_kills,
    };
    base.saturating_mul(1u32 << tally.count(Deed::Answered, &offer.subject()).min(20))
}

/// The offer the deeds have earned and has not been answered, if any: weapons first.
pub fn due(tally: &Tally, rules: &DevotionRules) -> Option<Offer> {
    let earned = |offer: Offer| offer.kills(tally) >= threshold(tally, rules, offer);
    WEAPON_KINDS.into_iter().map(Offer::Weapon).find(|&o| earned(o)).or_else(|| Kin::all().into_iter().map(Offer::Creature).find(|&o| earned(o)))
}

/// Notes that `offer` was answered, taken or declined; the next comes at double the count.
pub fn answer(tally: &mut Tally, offer: Offer) {
    tally.record(Deed::Answered, offer.subject());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Gear;
    use fb_body::Folk;
    use fb_denizens::DenizenKind;
    use fb_items::Foe;

    fn gear(devotion: Option<Devotion>) -> Gear {
        let rules = DevotionRules::default();
        Gear::new(0.0, devotion.map(|d| d.worn(&rules)), [])
    }

    fn slay(tally: &mut Tally, kin: Kin, n: u32) {
        for _ in 0..n {
            tally.kill(format!("kin:{}", kin.key()), kin, None);
        }
    }

    const UNDEAD: Kin = Kin::Foe(Foe::Undead);
    const SPIDER: Kin = Kin::Foe(Foe::Spider);
    const GOBLIN: Kin = Kin::Folk(Folk::Goblin);
    const MINOTAUR: Kin = Kin::Denizen(DenizenKind::Minotaur);

    #[test]
    fn a_foe_devotion_wards_and_adds_dice_against_that_family_only() {
        let g = gear(Some(Devotion::FavoredFoe(UNDEAD)));
        assert_eq!(g.ward_against(UNDEAD), 0.25);
        assert_eq!(g.bane_against(UNDEAD), Some(Dice::new(1, 4)));
        assert_eq!((g.ward_against(SPIDER), g.bane_against(SPIDER)), (0.0, None));
    }

    #[test]
    fn a_favored_foe_can_be_any_family_and_the_bonus_applies_against_it() {
        for kin in Kin::all() {
            let g = gear(Some(Devotion::FavoredFoe(kin)));
            assert_eq!((g.ward_against(kin), g.bane_against(kin).is_some()), (0.25, true), "{kin:?}");
        }
        let g = gear(Some(Devotion::FavoredFoe(GOBLIN)));
        assert_eq!(g.ward_against(Kin::Folk(Folk::Human)), 0.0);
        assert!(g.bane_against(MINOTAUR).is_none());
    }

    #[test]
    fn a_favored_foe_ward_adds_to_an_items_ward_to_the_cap() {
        let item = Worn { ward: { let mut w = [0.0; Foe::COUNT]; w[Foe::Undead.index()] = 0.6; w }, ..Worn::default() };
        let g = Gear::new(0.0, [item, Devotion::FavoredFoe(UNDEAD).worn(&DevotionRules::default())], []);
        assert_eq!(g.ward_against(UNDEAD), fb_items::foe::MAX_WARD as f64);
    }

    #[test]
    fn aligning_with_spiders_walks_through_webs() {
        let g = gear(Some(Devotion::Aligned(SPIDER)));
        assert!(g.web_walker);
        assert_eq!(g.ward_against(SPIDER), 0.0, "no ward from an alignment");
        assert!(!gear(None).web_walker);
    }

    #[test]
    fn changing_from_undead_to_spiders_drops_the_undead_bonuses() {
        let before = gear(Some(Devotion::FavoredFoe(UNDEAD)));
        let after = gear(Some(Devotion::Aligned(SPIDER)));
        assert!(before.ward_against(UNDEAD) > 0.0 && before.bane_against(UNDEAD).is_some());
        assert_eq!((after.ward_against(UNDEAD), after.bane_against(UNDEAD)), (0.0, None));
        assert!(after.web_walker);
    }

    #[test]
    fn every_family_can_be_aligned_with_creatures_for_a_trait_and_folk_and_denizens_for_favour() {
        let rules = DevotionRules::default();
        for kin in Kin::all() {
            assert!(rules.can_align(kin), "{kin:?}");
            if kin.has_factions() {
                assert_eq!(Devotion::Aligned(kin).worn(&rules), Worn::default(), "{kin:?} gives favour, not a trait");
                assert_eq!(Devotion::Aligned(kin).favour(&rules), Some((kin, 15, 50)));
                assert_eq!(Devotion::FavoredFoe(kin).favour(&rules), Some((kin, -15, 100)));
            } else {
                assert!(!rules.aligned[kin.key()].text.is_empty());
                assert_ne!(Devotion::Aligned(kin).worn(&rules), Worn::default(), "{kin:?}");
                assert_eq!(Devotion::Aligned(kin).favour(&rules), None);
            }
        }
    }

    #[test]
    fn twenty_kills_of_goblins_offer_goblin_slayer() {
        let rules = DevotionRules::default();
        assert_eq!((rules.creature_kills, rules.weapon_kills), (20, 50));
        let mut tally = Tally::default();
        slay(&mut tally, GOBLIN, 19);
        assert_eq!(due(&tally, &rules), None);
        slay(&mut tally, GOBLIN, 1);
        let offer = due(&tally, &rules).unwrap();
        assert_eq!(offer, Offer::Creature(GOBLIN));
        assert_eq!(offer.choices(&rules), vec![Devotion::FavoredFoe(GOBLIN), Devotion::Aligned(GOBLIN)], "folk offer both");
        let titles: Vec<String> = (0..40).map(|seed| Devotion::FavoredFoe(GOBLIN).label(seed)).collect();
        assert!(titles.contains(&"Goblin Slayer".to_string()), "{titles:?}");
    }

    #[test]
    fn a_minotaur_is_its_own_family_and_can_be_a_foe_or_an_alignment() {
        let rules = DevotionRules::default();
        let mut tally = Tally::default();
        slay(&mut tally, MINOTAUR, 20);
        slay(&mut tally, Kin::Denizen(DenizenKind::Cyclops), 19);
        let offer = due(&tally, &rules).unwrap();
        assert_eq!(offer, Offer::Creature(MINOTAUR));
        assert_eq!(offer.choices(&rules), vec![Devotion::FavoredFoe(MINOTAUR), Devotion::Aligned(MINOTAUR)]);
    }

    #[test]
    fn creature_kills_of_a_group_count_together() {
        let rules = DevotionRules::default();
        let mut tally = Tally::default();
        for i in 0..20 {
            tally.kill(if i % 2 == 0 { "creature:rat" } else { "creature:giant centipede" }, Kin::Foe(Foe::Vermin), None);
        }
        assert_eq!(due(&tally, &rules), Some(Offer::Creature(Kin::Foe(Foe::Vermin))));
    }

    #[test]
    fn fifty_kills_with_one_weapon_offer_to_dedicate_to_it() {
        let rules = DevotionRules::default();
        let mut tally = Tally::default();
        let kins = Kin::all();
        for i in 0..49 {
            tally.kill("x", kins[i % kins.len()], Some(Family::Mace));
            tally.kill("x", kins[i % kins.len()], Some(Family::Sword));
        }
        assert_eq!(due(&tally, &rules), None, "counts are per kind of weapon");
        tally.kill("x", kins[0], Some(Family::Mace));
        let offer = due(&tally, &rules).unwrap();
        assert_eq!(offer, Offer::Weapon(Family::Mace));
        assert_eq!(offer.choices(&rules), vec![Devotion::Dedicated(Family::Mace)]);
    }

    #[test]
    fn declining_offers_again_at_double_the_count() {
        let rules = DevotionRules::default();
        let mut tally = Tally::default();
        slay(&mut tally, SPIDER, 20);
        let offer = due(&tally, &rules).unwrap();
        answer(&mut tally, offer);
        assert_eq!(due(&tally, &rules), None);
        slay(&mut tally, SPIDER, 19);
        assert_eq!(due(&tally, &rules), None, "39 kills");
        slay(&mut tally, SPIDER, 1);
        assert_eq!(due(&tally, &rules), Some(offer), "40 kills");
        answer(&mut tally, offer);
        assert_eq!(threshold(&tally, &rules, offer), 80);
    }

    #[test]
    fn armour_hits_are_recorded_and_no_devotion_reads_them() {
        let rules = DevotionRules::default();
        let mut tally = Tally::default();
        for _ in 0..500 {
            tally.hit_taken([Family::Garment, Family::Helm]);
        }
        assert_eq!(tally.hits_in(Family::Garment), 500);
        assert_eq!(due(&tally, &rules), None);
    }

    #[test]
    fn dedicating_to_a_weapon_gives_nothing_yet_and_switching_drops_the_old_bonuses() {
        let rules = DevotionRules::default();
        assert_eq!(Devotion::Dedicated(Family::Mace).worn(&rules), Worn::default());
        let before = gear(Some(Devotion::FavoredFoe(UNDEAD)));
        let after = gear(Some(Devotion::Dedicated(Family::Mace)));
        assert!(before.bane_against(UNDEAD).is_some());
        assert_eq!((after.bane_against(UNDEAD), after.ward_against(UNDEAD)), (None, 0.0));
    }

    #[test]
    fn the_shipped_file_says_what_the_defaults_say() {
        let rules: crate::rules::Rules = toml::from_str(include_str!("../../../data/rules.toml")).unwrap();
        assert_eq!(rules.devotion, DevotionRules::default());
    }

    #[test]
    fn a_devotion_round_trips_through_json() {
        let all = Kin::all().into_iter().flat_map(|k| [Devotion::FavoredFoe(k), Devotion::Aligned(k)]).chain(WEAPON_KINDS.map(Devotion::Dedicated));
        for d in all {
            assert_eq!(serde_json::from_str::<Devotion>(&serde_json::to_string(&d).unwrap()).unwrap(), d);
        }
    }
}
