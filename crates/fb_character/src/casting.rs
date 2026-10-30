//! Casting: the layer every caster shares. A prayer or a spell is not a fixed ability but is
//! written: a [`Written`] is an effect term (light, ward, frighten...), a reach term (self,
//! around you, touch, a cone) and a duration term, composed from the terms the caster has
//! [`Spellbook::unlocked`]. The terms are a table in `data/rules.toml` under `[casting.terms]`:
//! each has a kind, a school, a mana cost, a range in feet and a duration in game seconds. The
//! written ability's cost is the sum of its terms' costs; its range is its reach term's and its
//! duration its duration term's; the tier from 1 to [`MAX_TIER`] multiplies all three.
//! Divine Light, light + around + short, costs 3 at its first tier and lights 20 ft around the
//! caster; at its third it costs 9 and lights 60 ft.
//!
//! Clerics write prayers ([`Casting::Divine`] terms) and mages will write spells
//! ([`Casting::Arcane`]). Some terms are known from the start ([`TermRules::known`]); others are
//! unlocked by something else ([`Spellbook::unlock`]): a devotion unlocks a bane term.
//!
//! The writing screen needs: [`Spellbook::choices`] (the unlocked terms of a kind),
//! [`compose`] / [`Written::preview`] (cost, range and duration of what is written so far),
//! [`Written::validate`], and [`Spellbook::save`] (keep it under its name). Casting is
//! [`cast_with`], which spends the mana and returns the [`Cast`] the game reacts to.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::rules::{Rules, defaults};
use crate::{Casting, Mana};

/// The highest tier a written ability can be cast at.
pub const MAX_TIER: u8 = 3;

/// What a term of a written ability is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TermKind {
    /// What it does.
    Effect,
    /// Whom or where it reaches.
    Reach,
    /// How long it lasts.
    Duration,
}

/// One term, as the rules set it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TermRules {
    pub kind: TermKind,
    /// What the writer shows.
    pub name: String,
    /// Whose term it is: divine for prayers, arcane for spells.
    pub school: Casting,
    /// Mana it adds at the first tier.
    pub cost: f64,
    /// Feet a reach term reaches at the first tier; 0 for the rest.
    pub range: f64,
    /// Game seconds a duration term lasts at the first tier; 0 for the rest.
    pub seconds: f64,
    /// Known from the start, rather than unlocked by something else.
    pub known: bool,
}

impl Default for TermRules {
    fn default() -> Self {
        TermRules { kind: TermKind::Effect, name: String::new(), school: Casting::Divine, cost: 0.0, range: 0.0, seconds: 0.0, known: true }
    }
}

fn term(kind: TermKind, name: &str, school: Casting, cost: f64, range: f64, seconds: f64, known: bool) -> TermRules {
    TermRules { kind, name: name.to_string(), school, cost, range, seconds, known }
}

/// Every term's numbers, by id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CastingRules {
    pub terms: BTreeMap<String, TermRules>,
    /// How often frighten and charm take hold ([`InfluenceRules`]).
    pub influence: InfluenceRules,
}

/// Whether a frightened or charmed creature is taken by it: each gets a save, and bigger
/// creatures resist more often. The chance it takes hold is `base`, less `per_size` for each size
/// above Tiny, plus `per_tier` for each tier above the first, kept between `floor` and `ceiling`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InfluenceRules {
    pub base: f64,
    pub per_tier: f64,
    pub per_size: f64,
    pub floor: f64,
    pub ceiling: f64,
}

impl Default for InfluenceRules {
    fn default() -> Self {
        InfluenceRules { base: 0.9, per_tier: 0.1, per_size: 0.2, floor: 0.05, ceiling: 0.95 }
    }
}

impl InfluenceRules {
    /// The chance it takes hold of a creature `size_steps` above Tiny (Tiny 0, Small 1, Medium 2,
    /// Large 3) at `tier`.
    pub fn chance(&self, size_steps: u8, tier: u8) -> f64 {
        let raw = self.base - self.per_size * f64::from(size_steps) + self.per_tier * f64::from(tier.saturating_sub(1));
        raw.clamp(self.floor, self.ceiling)
    }

    /// Whether it takes hold, given a roll from 0 to 1.
    pub fn takes_hold(&self, size_steps: u8, tier: u8, roll: f64) -> bool {
        roll < self.chance(size_steps, tier)
    }
}

impl Default for CastingRules {
    fn default() -> Self {
        use Casting::{Arcane, Divine};
        use TermKind::*;
        let terms = [
            ("light", term(Effect, "Light", Divine, 1.0, 0.0, 0.0, true)),
            ("ward", term(Effect, "Ward", Divine, 2.0, 0.0, 0.0, true)),
            ("smite", term(Effect, "Smite", Divine, 2.0, 0.0, 0.0, true)),
            ("shield", term(Effect, "Shield", Divine, 2.0, 0.0, 0.0, true)),
            ("frighten", term(Effect, "Frighten", Divine, 3.0, 0.0, 0.0, false)),
            ("charm", term(Effect, "Charm", Divine, 3.0, 0.0, 0.0, false)),
            ("bane", term(Effect, "Bane", Divine, 2.0, 0.0, 0.0, false)),
            ("self", term(Reach, "Self", Divine, 0.0, 0.0, 0.0, true)),
            ("sight", term(Reach, "At one you see", Divine, 1.0, 60.0, 0.0, true)),
            ("around", term(Reach, "Around you", Divine, 1.0, 20.0, 0.0, true)),
            ("touch", term(Reach, "Touch", Divine, 0.0, 5.0, 0.0, true)),
            ("cone", term(Reach, "Cone", Divine, 2.0, 30.0, 0.0, false)),
            ("instant", term(Duration, "Instant", Divine, 0.0, 0.0, 0.0, true)),
            ("short", term(Duration, "Short", Divine, 1.0, 0.0, 600.0, true)),
            ("long", term(Duration, "Long", Divine, 2.0, 0.0, 1800.0, true)),
            ("arcane_light", term(Effect, "Light", Arcane, 1.0, 0.0, 0.0, true)),
            ("arcane_ward", term(Effect, "Ward", Arcane, 2.0, 0.0, 0.0, true)),
            ("arcane_self", term(Reach, "Self", Arcane, 0.0, 0.0, 0.0, true)),
            ("arcane_around", term(Reach, "Around you", Arcane, 1.0, 20.0, 0.0, true)),
            ("arcane_short", term(Duration, "Short", Arcane, 1.0, 0.0, 600.0, true)),
        ]
        .into_iter()
        .map(|(id, t)| (id.to_string(), t))
        .collect();
        CastingRules { terms, influence: InfluenceRules::default() }
    }
}

/// A term the writer can refuse.
#[derive(Clone, Debug, PartialEq)]
pub enum ComposeError {
    /// No such term.
    Unknown(String),
    /// The term is of another kind than the place it was put.
    WrongKind { term: String, wanted: TermKind },
    /// The term belongs to another school.
    WrongSchool { term: String },
    /// The caster has not unlocked the term.
    Locked(String),
    /// A written ability needs a name to be kept.
    Unnamed,
}

impl ComposeError {
    /// What to tell the player.
    pub fn say(&self) -> String {
        match self {
            ComposeError::Unknown(t) => format!("There is no term {t}"),
            ComposeError::WrongKind { term, wanted } => format!("{term} is not {} term", match wanted { TermKind::Effect => "an effect", TermKind::Reach => "a reach", TermKind::Duration => "a duration" }),
            ComposeError::WrongSchool { term } => format!("{term} is not yours to write"),
            ComposeError::Locked(t) => format!("You have not unlocked {t}"),
            ComposeError::Unnamed => "Name it first".to_string(),
        }
    }
}

/// What a written ability costs, reaches and lasts at a tier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preview {
    pub tier: u8,
    pub cost: f64,
    /// Feet.
    pub range: f64,
    /// Game seconds.
    pub duration: f64,
}

/// A prayer or spell as written: its three terms by id. `school` is who writes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Written {
    pub name: String,
    pub school: Casting,
    pub effect: String,
    pub reach: String,
    pub duration: String,
}

impl Written {
    /// The default prayer every cleric starts with: light, around you, short.
    pub fn divine_light() -> Written {
        Written { name: "Divine Light".into(), school: Casting::Divine, effect: "light".into(), reach: "around".into(), duration: "short".into() }
    }

    /// Smite, which every cleric starts with: smite, at one you see, instant. A pillar of holy
    /// light on the creature nearest the crosshair that the cleric can see.
    pub fn smite() -> Written {
        Written { name: "Smite".into(), school: Casting::Divine, effect: "smite".into(), reach: "sight".into(), duration: "instant".into() }
    }

    /// Shield, which every cleric starts with: shield, self, short. A shimmering ward that raises
    /// the cleric's armour class ([`shield_bonus`]) for a while.
    pub fn shield() -> Written {
        Written { name: "Shield".into(), school: Casting::Divine, effect: "shield".into(), reach: "self".into(), duration: "short".into() }
    }

    fn terms(&self) -> [(&str, TermKind); 3] {
        [(&self.effect, TermKind::Effect), (&self.reach, TermKind::Reach), (&self.duration, TermKind::Duration)]
    }

    /// Whether the terms exist, are in their places, are the school's and are unlocked.
    pub fn validate_with(&self, rules: &Rules, unlocked: &BTreeSet<String>) -> Result<(), ComposeError> {
        for (id, kind) in self.terms() {
            let t = rules.casting.terms.get(id).ok_or_else(|| ComposeError::Unknown(id.to_string()))?;
            if t.kind != kind {
                return Err(ComposeError::WrongKind { term: t.name.clone(), wanted: kind });
            }
            if t.school != self.school {
                return Err(ComposeError::WrongSchool { term: t.name.clone() });
            }
            if !unlocked.contains(id) {
                return Err(ComposeError::Locked(t.name.clone()));
            }
        }
        Ok(())
    }

    /// [`Written::validate_with`] under the default rules.
    pub fn validate(&self, unlocked: &BTreeSet<String>) -> Result<(), ComposeError> {
        self.validate_with(defaults(), unlocked)
    }

    /// What it costs, reaches and lasts at `tier`: the terms' cost summed, the reach term's range
    /// and the duration term's seconds, each times the tier. Terms the rules do not know add
    /// nothing; [`Written::validate_with`] refuses them.
    pub fn preview_with(&self, rules: &Rules, tier: u8) -> Preview {
        let get = |id: &str| rules.casting.terms.get(id);
        let cost: f64 = self.terms().iter().filter_map(|(id, _)| get(id)).map(|t| t.cost).sum();
        let n = f64::from(tier);
        Preview { tier, cost: cost * n, range: get(&self.reach).map_or(0.0, |t| t.range) * n, duration: get(&self.duration).map_or(0.0, |t| t.seconds) * n }
    }

    /// [`Written::preview_with`] under the default rules.
    pub fn preview(&self, tier: u8) -> Preview {
        self.preview_with(defaults(), tier)
    }
}

/// Writes `name` from the three terms, refusing any the caster has not unlocked, any in the wrong
/// place or of another school.
pub fn compose_with(rules: &Rules, unlocked: &BTreeSet<String>, school: Casting, name: &str, effect: &str, reach: &str, duration: &str) -> Result<Written, ComposeError> {
    let written = Written { name: name.trim().to_string(), school, effect: effect.into(), reach: reach.into(), duration: duration.into() };
    written.validate_with(rules, unlocked)?;
    Ok(written)
}

/// [`compose_with`] under the default rules.
pub fn compose(unlocked: &BTreeSet<String>, school: Casting, name: &str, effect: &str, reach: &str, duration: &str) -> Result<Written, ComposeError> {
    compose_with(defaults(), unlocked, school, name, effect, reach, duration)
}

/// What a caster has: the terms they can write with and the abilities they have written. On the
/// delver, and saved with them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spellbook {
    pub unlocked: BTreeSet<String>,
    pub written: Vec<Written>,
}

impl Spellbook {
    /// What a new caster of `school` has: the terms known from the start, and Divine Light, Smite and
    /// Shield for a cleric. Empty for a martial character.
    pub fn starting_with(rules: &Rules, school: Casting) -> Spellbook {
        if school == Casting::Martial {
            return Spellbook::default();
        }
        let unlocked = rules.casting.terms.iter().filter(|(_, t)| t.known && t.school == school).map(|(id, _)| id.clone()).collect();
        let written = if school == Casting::Divine { vec![Written::divine_light(), Written::smite(), Written::shield()] } else { vec![] };
        Spellbook { unlocked, written }
    }

    /// [`Spellbook::starting_with`] under the default rules.
    pub fn starting(school: Casting) -> Spellbook {
        Spellbook::starting_with(defaults(), school)
    }

    /// Unlocks a term (a devotion unlocks a bane). False if it was already unlocked or is unknown.
    pub fn unlock_with(&mut self, rules: &Rules, id: &str) -> bool {
        rules.casting.terms.contains_key(id) && self.unlocked.insert(id.to_string())
    }

    /// [`Spellbook::unlock_with`] under the default rules.
    pub fn unlock(&mut self, id: &str) -> bool {
        self.unlock_with(defaults(), id)
    }

    /// The terms a devotion unlocks and a different one takes away again.
    pub const DEVOTION_TERMS: [&'static str; 3] = ["bane", "frighten", "charm"];

    /// Whether the unlocked devotion terms are exactly those `devotion` grants.
    pub fn follows_devotion(&self, devotion: Option<crate::devotion::Devotion>) -> bool {
        let grants = devotion.map_or(&[][..], |d| d.terms());
        Self::DEVOTION_TERMS.iter().all(|t| self.unlocked.contains(*t) == grants.contains(t))
    }

    /// Unlocks the terms `devotion` grants and locks the other devotion terms, so changing
    /// devotion changes what can be written. Prayers already written are kept (one with a locked
    /// term fails to cast). True if anything changed.
    pub fn follow_devotion_with(&mut self, rules: &Rules, devotion: Option<crate::devotion::Devotion>) -> bool {
        let grants = devotion.map_or(&[][..], |d| d.terms());
        let mut changed = false;
        for term in Self::DEVOTION_TERMS {
            if grants.contains(&term) {
                changed |= self.unlock_with(rules, term);
            } else {
                changed |= self.unlocked.remove(term);
            }
        }
        changed
    }

    /// [`Spellbook::follow_devotion_with`] under the default rules.
    pub fn follow_devotion(&mut self, devotion: Option<crate::devotion::Devotion>) -> bool {
        self.follow_devotion_with(defaults(), devotion)
    }

    /// The unlocked terms of `kind` for `school`, as (id, term) in id order: the writer's choices.
    pub fn choices<'a>(&self, rules: &'a Rules, school: Casting, kind: TermKind) -> Vec<(&'a str, &'a TermRules)> {
        rules.casting.terms.iter().filter(|(id, t)| t.kind == kind && t.school == school && self.unlocked.contains(*id)).map(|(id, t)| (id.as_str(), t)).collect()
    }

    /// Keeps `written` under its name, replacing a prayer of the same name. It must be valid for
    /// this book and named.
    pub fn save_with(&mut self, rules: &Rules, written: Written) -> Result<(), ComposeError> {
        if written.name.trim().is_empty() {
            return Err(ComposeError::Unnamed);
        }
        written.validate_with(rules, &self.unlocked)?;
        match self.written.iter_mut().find(|w| w.name == written.name) {
            Some(slot) => *slot = written,
            None => self.written.push(written),
        }
        Ok(())
    }

    /// [`Spellbook::save_with`] under the default rules.
    pub fn save(&mut self, written: Written) -> Result<(), ComposeError> {
        self.save_with(defaults(), written)
    }

    /// The written ability named `name`.
    pub fn find(&self, name: &str) -> Option<&Written> {
        self.written.iter().find(|w| w.name == name)
    }
}

/// The sides of the die Smite rolls, once for each tier. Our own.
pub const SMITE_DIE: u32 = 8;

/// What Smite deals at `tier`: a d8 for each tier, rolled one at a time from `rng`, and doubled
/// against the undead. Radiant, so nothing a creature wears or is vulnerable to changes it.
pub fn smite_damage(tier: u8, undead: bool, rng: &mut procgen_rng::Mulberry32) -> u32 {
    let rolled: u32 = (0..tier).map(|_| rng.int(1, SMITE_DIE)).sum();
    if undead { rolled * 2 } else { rolled }
}

/// The armour class Shield adds for each tier. Our own.
pub const SHIELD_AC_PER_TIER: i32 = 2;

/// What Shield cast at `tier` adds to the caster's armour class while it lasts.
pub fn shield_bonus(tier: u8) -> i32 {
    SHIELD_AC_PER_TIER * i32::from(tier)
}

/// Why a cast did not happen.
#[derive(Clone, Debug, PartialEq)]
pub enum CastError {
    /// The pool holds less than the cast costs.
    NotEnoughMana { needed: f64, have: f64 },
    /// The caster does not cast this school.
    WrongCaster { needs: Casting },
    /// A tier below 1 or above [`MAX_TIER`].
    BadTier,
    /// A term is not usable: unknown, or not unlocked.
    Terms(ComposeError),
}

impl CastError {
    /// What to tell the player, for the ability `name`.
    pub fn say(&self, name: &str) -> String {
        match self {
            CastError::NotEnoughMana { needed, have } => format!("Not enough mana for {name}: it needs {}, you have {}", points(*needed), points(*have)),
            CastError::WrongCaster { needs: Casting::Divine } => format!("Only a cleric can pray {name}"),
            CastError::WrongCaster { needs: Casting::Arcane } => format!("Only a mage can cast {name}"),
            CastError::WrongCaster { needs: Casting::Martial } => format!("You cannot cast {name}"),
            CastError::BadTier => format!("{name} has no such tier"),
            CastError::Terms(why) => format!("{name} fails: {}", why.say()),
        }
    }
}

/// Mana points as a short number: whole when they are.
pub fn points(value: f64) -> String {
    if (value - value.round()).abs() < 0.05 { format!("{}", value.round() as i64) } else { format!("{value:.1}") }
}

/// A cast that happened: what the game reacts to.
#[derive(Clone, Debug, PartialEq)]
pub struct Cast {
    pub written: Written,
    pub tier: u8,
    /// The mana it spent.
    pub cost: f64,
    /// Feet.
    pub range: f64,
    /// Game seconds.
    pub duration: f64,
}

/// Casts `written` at `tier` for a character who casts as `casting` with `unlocked` terms,
/// spending from `mana` under the default rules.
pub fn cast(mana: &mut Mana, casting: Casting, unlocked: &BTreeSet<String>, written: &Written, tier: u8) -> Result<Cast, CastError> {
    cast_with(defaults(), mana, casting, unlocked, written, tier)
}

/// [`cast`] under `rules`. Nothing is spent when it fails.
pub fn cast_with(rules: &Rules, mana: &mut Mana, casting: Casting, unlocked: &BTreeSet<String>, written: &Written, tier: u8) -> Result<Cast, CastError> {
    if !(1..=MAX_TIER).contains(&tier) {
        return Err(CastError::BadTier);
    }
    if written.school != casting {
        return Err(CastError::WrongCaster { needs: written.school });
    }
    written.validate_with(rules, unlocked).map_err(CastError::Terms)?;
    let p = written.preview_with(rules, tier);
    if !mana.spend(p.cost) {
        return Err(CastError::NotEnoughMana { needed: p.cost, have: mana.points });
    }
    Ok(Cast { written: written.clone(), tier, cost: p.cost, range: p.range, duration: p.duration })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleric() -> Spellbook {
        Spellbook::starting(Casting::Divine)
    }

    #[test]
    fn divine_light_composes_to_three_mana_and_twenty_feet() {
        let p = Written::divine_light().preview(1);
        assert_eq!((p.cost, p.range, p.duration), (3.0, 20.0, 600.0));
        let p3 = Written::divine_light().preview(3);
        assert_eq!((p3.cost, p3.range), (9.0, 60.0), "third tier lights 60 ft");
    }

    #[test]
    fn a_tier_two_prayer_with_ten_mana_spends_six_and_leaves_four() {
        let book = cleric();
        let mut mana = Mana::full(10.0);
        let cast = cast(&mut mana, Casting::Divine, &book.unlocked, &Written::divine_light(), 2).unwrap();
        assert_eq!((cast.cost, cast.tier, cast.range), (6.0, 2, 40.0));
        assert_eq!(mana.points, 4.0);
    }

    #[test]
    fn too_little_mana_fails_and_spends_none() {
        let book = cleric();
        let mut mana = Mana { points: 5.0, max: 10.0 };
        let err = cast(&mut mana, Casting::Divine, &book.unlocked, &Written::divine_light(), 2).unwrap_err();
        assert_eq!(err, CastError::NotEnoughMana { needed: 6.0, have: 5.0 });
        assert_eq!(mana.points, 5.0);
        assert!(err.say("Divine Light").contains("Not enough mana"));
    }

    #[test]
    fn changing_a_term_changes_cost_and_range() {
        let book = cleric();
        let ward = compose(&book.unlocked, Casting::Divine, "Ward", "ward", "around", "long").unwrap();
        assert_eq!(ward.preview(1), Preview { tier: 1, cost: 5.0, range: 20.0, duration: 1800.0 });
        let touch = compose(&book.unlocked, Casting::Divine, "Touch", "ward", "touch", "short").unwrap();
        assert_eq!(touch.preview(1), Preview { tier: 1, cost: 3.0, range: 5.0, duration: 600.0 });
        assert_ne!(ward.preview(1).cost, Written::divine_light().preview(1).cost);
    }

    #[test]
    fn a_locked_term_is_refused_until_unlocked() {
        let mut book = cleric();
        let err = compose(&book.unlocked, Casting::Divine, "Bane", "bane", "around", "short").unwrap_err();
        assert_eq!(err, ComposeError::Locked("Bane".into()));
        assert!(book.unlock("bane"));
        assert!(!book.unlock("bane"), "already unlocked");
        let w = compose(&book.unlocked, Casting::Divine, "Bane", "bane", "around", "short").unwrap();
        // A written ability whose term is lost will not cast.
        let mut mana = Mana::full(10.0);
        assert!(matches!(cast(&mut mana, Casting::Divine, &cleric().unlocked, &w, 1), Err(CastError::Terms(ComposeError::Locked(_)))));
        assert_eq!(mana.points, 10.0);
    }

    #[test]
    fn terms_must_be_in_their_place_and_known() {
        let book = cleric();
        assert!(matches!(compose(&book.unlocked, Casting::Divine, "x", "around", "around", "short"), Err(ComposeError::WrongKind { .. })));
        assert!(matches!(compose(&book.unlocked, Casting::Divine, "x", "nonsense", "around", "short"), Err(ComposeError::Unknown(_))));
        assert!(matches!(compose(&book.unlocked, Casting::Arcane, "x", "light", "around", "short"), Err(ComposeError::Locked(_) | ComposeError::WrongSchool { .. })));
    }

    #[test]
    fn the_choices_are_the_unlocked_terms_of_a_kind() {
        let mut book = cleric();
        let rules = defaults();
        let ids = |b: &Spellbook, k| b.choices(rules, Casting::Divine, k).into_iter().map(|(id, _)| id.to_string()).collect::<Vec<_>>();
        assert_eq!(ids(&book, TermKind::Effect), ["light", "shield", "smite", "ward"]);
        assert_eq!(ids(&book, TermKind::Reach), ["around", "self", "sight", "touch"]);
        assert_eq!(ids(&book, TermKind::Duration), ["instant", "long", "short"]);
        book.unlock("charm");
        assert_eq!(ids(&book, TermKind::Effect), ["charm", "light", "shield", "smite", "ward"]);
    }

    #[test]
    fn a_cleric_starts_with_divine_light_smite_and_shield_and_a_fighter_with_nothing() {
        assert_eq!(cleric().written, vec![Written::divine_light(), Written::smite(), Written::shield()]);
        assert_eq!(Spellbook::starting(Casting::Martial), Spellbook::default());
    }

    #[test]
    fn smite_is_three_mana_sixty_feet_and_over_at_once() {
        assert_eq!(Written::smite().preview(1), Preview { tier: 1, cost: 3.0, range: 60.0, duration: 0.0 });
        let p3 = Written::smite().preview(3);
        assert_eq!((p3.cost, p3.range, p3.duration), (9.0, 180.0, 0.0));
        let mut mana = Mana::full(10.0);
        let cast = cast(&mut mana, Casting::Divine, &cleric().unlocked, &Written::smite(), 1).unwrap();
        assert_eq!((cast.cost, cast.duration, mana.points), (3.0, 0.0, 7.0), "an instant prayer casts like any other");
    }

    #[test]
    fn shield_is_three_mana_for_ten_minutes_and_two_armour_a_tier() {
        assert_eq!(Written::shield().preview(1), Preview { tier: 1, cost: 3.0, range: 0.0, duration: 600.0 });
        assert_eq!(Written::shield().preview(2).duration, 1200.0);
        assert_eq!((1..=MAX_TIER).map(shield_bonus).collect::<Vec<_>>(), [2, 4, 6]);
        assert!(Written::shield().validate(&cleric().unlocked).is_ok());
    }

    #[test]
    fn smite_rolls_a_d8_a_tier_and_the_undead_take_double() {
        let mut dice = procgen_rng::Mulberry32::new(3);
        for tier in 1..=MAX_TIER {
            for _ in 0..200 {
                let n = smite_damage(tier, false, &mut dice);
                assert!((u32::from(tier)..=u32::from(tier) * SMITE_DIE).contains(&n), "tier {tier}: {n}");
                let m = smite_damage(tier, true, &mut dice);
                assert!(m.is_multiple_of(2) && (u32::from(tier) * 2..=u32::from(tier) * SMITE_DIE * 2).contains(&m), "tier {tier}: {m}");
            }
        }
        let (mut a, mut b) = (procgen_rng::Mulberry32::new(9), procgen_rng::Mulberry32::new(9));
        assert_eq!(smite_damage(2, true, &mut a), smite_damage(2, false, &mut b) * 2, "the same dice, doubled");
    }

    #[test]
    fn saving_keeps_a_valid_named_prayer_and_replaces_by_name() {
        let mut book = cleric();
        let ward = compose(&book.unlocked, Casting::Divine, "Ward", "ward", "self", "short").unwrap();
        book.save(ward).unwrap();
        assert_eq!(book.written.len(), 4);
        let wider = compose(&book.unlocked, Casting::Divine, "Ward", "ward", "around", "short").unwrap();
        book.save(wider).unwrap();
        assert_eq!((book.written.len(), book.find("Ward").unwrap().reach.as_str()), (4, "around"));
        let mut bad = Written::divine_light();
        bad.name = "  ".into();
        assert_eq!(book.save(bad), Err(ComposeError::Unnamed));
        let mut locked = Written::divine_light();
        locked.name = "Bane".into();
        locked.effect = "bane".into();
        assert!(matches!(book.save(locked), Err(ComposeError::Locked(_))));
    }

    #[test]
    fn a_spellbook_round_trips_and_an_old_save_reads_empty() {
        let mut book = cleric();
        book.unlock("bane");
        let json = serde_json::to_string(&book).unwrap();
        assert_eq!(serde_json::from_str::<Spellbook>(&json).unwrap(), book);
        assert_eq!(serde_json::from_str::<Spellbook>("{}").unwrap(), Spellbook::default());
    }

    #[test]
    fn only_clerics_pray_and_tiers_are_one_to_three() {
        let book = cleric();
        let mut mana = Mana::full(10.0);
        let w = Written::divine_light();
        assert_eq!(cast(&mut mana, Casting::Martial, &book.unlocked, &w, 1), Err(CastError::WrongCaster { needs: Casting::Divine }));
        assert_eq!(cast(&mut mana, Casting::Divine, &book.unlocked, &w, 0), Err(CastError::BadTier));
        assert_eq!(cast(&mut mana, Casting::Divine, &book.unlocked, &w, 4), Err(CastError::BadTier));
        assert_eq!(mana.points, 10.0);
    }

    #[test]
    fn a_mage_composes_and_casts_an_arcane_written_ability() {
        let book = Spellbook::starting(Casting::Arcane);
        assert!(book.written.is_empty());
        let w = compose(&book.unlocked, Casting::Arcane, "Spark", "arcane_light", "arcane_around", "arcane_short").unwrap();
        assert_eq!(w.preview(2), Preview { tier: 2, cost: 6.0, range: 40.0, duration: 1200.0 });
        assert!(matches!(compose(&book.unlocked, Casting::Arcane, "x", "light", "arcane_around", "arcane_short"), Err(ComposeError::WrongSchool { .. })));
        let mut mana = Mana::full(10.0);
        let cast = cast(&mut mana, Casting::Arcane, &book.unlocked, &w, 2).unwrap();
        assert_eq!((cast.cost, mana.points), (6.0, 4.0));
        assert_eq!(super::cast(&mut mana, Casting::Divine, &book.unlocked, &w, 1), Err(CastError::WrongCaster { needs: Casting::Arcane }));
    }

    #[test]
    fn the_rules_set_the_cost() {
        let mut rules = Rules::default();
        rules.casting.terms.get_mut("light").unwrap().cost = 4.0;
        assert_eq!(Written::divine_light().preview_with(&rules, 1).cost, 6.0);
    }

    #[test]
    fn a_devotion_unlocks_its_terms_and_changing_it_locks_the_rest() {
        use crate::devotion::Devotion;
        use crate::kin::Kin;
        use fb_items::Foe;
        let mut book = cleric();
        let locked = |b: &Spellbook| ["bane", "frighten", "charm"].map(|t| !b.unlocked.contains(t));
        assert_eq!(locked(&book), [true; 3]);
        assert!(book.follow_devotion(Some(Devotion::FavoredFoe(Kin::Foe(Foe::Vermin)))));
        assert_eq!(locked(&book), [false, false, true]);
        assert!(book.follows_devotion(Some(Devotion::FavoredFoe(Kin::Foe(Foe::Vermin)))));
        assert!(!book.follow_devotion(Some(Devotion::FavoredFoe(Kin::Foe(Foe::Undead)))), "nothing to change");
        assert!(book.follow_devotion(Some(Devotion::Aligned(Kin::Foe(Foe::Spider)))));
        assert_eq!(locked(&book), [true, true, false]);
        assert!(book.follow_devotion(None));
        assert_eq!(locked(&book), [true; 3]);
    }

    #[test]
    fn bigger_creatures_resist_more_and_tier_helps() {
        let r = InfluenceRules::default();
        assert!(r.chance(0, 1) > r.chance(1, 1) && r.chance(1, 1) > r.chance(3, 1));
        assert!(r.chance(3, 3) > r.chance(3, 1));
        assert!((0..=3).all(|s| (1..=3).all(|t| (0.05..=0.95).contains(&r.chance(s, t)))));
    }
}
