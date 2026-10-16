//! Placeholder combat: the delver and the creatures trade blows until the character
//! simulation is designed (#122). One pool of hit points on each side, and every number below
//! either cites the D&D 5e SRD 5.1 ([`fb_creatures::SRD`], CC-BY-4.0, `#page=` is the PDF's
//! page) or is labelled our own.
//!
//! - **Swinging** ([`swing`]): the attack key swings what the main hand holds, in a shape
//!   that suits the hold ([`SwingKind`]): a chop for maces, axes and clubs, a slash for
//!   swords, a thrust for spears and daggers, a sweep for a staff, a punch with nothing in
//!   hand. Each swing is thrown as one of the strikes that kind suits ([`swing::Strike`]),
//!   from above, either side or below, or a stab, arcing to the creature in reach or the
//!   crosshair ([`swing::SwingPath`]). Partway through,
//!   the blow lands on the nearest creature whose body, as its bones are posed
//!   ([`swing::body_targets`]), is within the weapon's reach in a cone ahead
//!   ([`target_in_reach`]), for the weapon's damage dice ([`Weapon::roll`]).
//!   Every swing that finds a creature hits: no attack roll against its Armor Class yet.
//! - **Taking hits** ([`feedback`]): a struck creature flashes, twitches back and squeaks or
//!   hisses. At 0 hit points it holds, flashing, for a moment, then collapses, a rat rolling over and a snake going limp along the
//!   floor, and its body stays for the run. Bodies block nothing: they lie flat (a rat on its
//!   back, a snake stretched out), low enough to step over.
//! - **Being bitten** ([`vitals`]): a creature's bite ([`CreatureAttack`]) takes the delver's
//!   hit points, which show low on the left of the view with a red flash round its edge. A
//!   poisonous snake's venom is halved by a Constitution save; a constrictor's bite holds the
//!   delver, slowed, until they strike it. At 0 the run ends: "You fall in the abyss", then the
//!   title.
//!
//! Combat rolls come from [`RunDice`], seeded afresh each run: the world stays a pure function
//! of its seed, but no two fights roll alike.

use bevy::prelude::*;
use fb_body::Hold;
use fb_character::combat as rules;
use fb_character::rules::Rules;
use fb_items::dice::Dice;
use procgen_rng::Mulberry32;

use crate::character::{Character, Class, modifier};
use crate::creatures::behaviour::CreatureAttack;
use crate::inventory::Inventory;
use crate::tuning::Tuning;

pub mod feedback;
pub mod eye;
pub mod kick;
pub mod orbs;
pub mod swing;
pub mod vitals;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RunDice>().init_resource::<Tuning>().init_resource::<crate::rules::GameRules>().add_plugins((swing::SwingPlugin, feedback::FeedbackPlugin, vitals::VitalsPlugin, orbs::OrbPlugin, eye::EyePlugin, kick::KickPlugin));
    }
}

/// The dice every blow and save of this run is rolled with: seeded from the clock, so combat
/// differs run to run while everything generated stays a function of its seed.
#[derive(Resource, Debug)]
pub struct RunDice(pub Mulberry32);

impl Default for RunDice {
    fn default() -> Self {
        RunDice(Mulberry32::new(crate::launch::clock_seed()))
    }
}

/// Hit points: the one pool each side has until the character simulation is designed.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    pub hp: i32,
    pub max: i32,
}

impl Health {
    pub fn full(max: i32) -> Health {
        Health { hp: max, max }
    }

    /// Takes `damage`, never below 0. Returns whether this blow brought it to 0.
    pub fn hurt(&mut self, damage: u32) -> bool {
        let was_up = self.hp > 0;
        self.hp = (self.hp - damage as i32).max(0);
        was_up && self.hp == 0
    }

    /// Restores `amount`, never above the maximum. Returns how much it restored.
    pub fn heal(&mut self, amount: u32) -> u32 {
        let before = self.hp;
        self.hp = (self.hp + amount as i32).min(self.max).max(before);
        (self.hp - before) as u32
    }

    pub fn is_down(&self) -> bool {
        self.hp <= 0
    }
}

/// The delver's hit points: a 1st-level character's, the class's hit die at its highest plus
/// the Constitution modifier (SRD 5.1 "Hit Points at 1st Level": fighter 10 + Con, p. 24;
/// ranger 10 + Con, p. 35; cleric 8 + Con, p. 15; wizard, our mage, 6 + Con, p. 52), and never
/// below 1 (our own floor for a very frail delver).
pub fn first_level_hit_points_with(rules: &Rules, character: &Character) -> i32 {
    first_level_hit_points_explained_with(rules, character).value() as i32
}

/// [`first_level_hit_points_with`] with its terms: the hit die and the Constitution modifier.
pub fn first_level_hit_points_explained_with(rules: &Rules, character: &Character) -> fb_character::explain::Explained {
    let dice = &rules.combat.hit_dice;
    let die = match character.class {
        Class::Fighter => dice.fighter,
        Class::Ranger => dice.ranger,
        Class::Cleric => dice.cleric,
        Class::Mage => dice.mage,
    };
    rules::first_level_hit_points(die, modifier(character.constitution()))
}

/// Whether things of `family` are swung as melee weapons (anything else held is improvised).
pub fn is_melee(family: armory::Family) -> bool {
    use armory::Family;
    matches!(family, Family::Sword | Family::Mace | Family::Axe | Family::Club | Family::Spear | Family::Dagger | Family::Staff)
}

/// How a held thing is swung.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SwingKind {
    /// Overhead and down: maces, axes, clubs, and anything else used as a club.
    Chop,
    /// Across the body, right to left: swords.
    Slash,
    /// Straight ahead: spears and daggers.
    Thrust,
    /// A wide flat arc: a staff.
    Sweep,
    /// Nothing in hand.
    Punch,
}

impl SwingKind {
    /// The swing that suits `item` in the main hand, or a punch with nothing.
    pub fn of(item: Option<&armory::Item>) -> SwingKind {
        use armory::Family;
        match item.map(armory::Item::family) {
            None => SwingKind::Punch,
            Some(Family::Sword) => SwingKind::Slash,
            Some(Family::Spear | Family::Dagger) => SwingKind::Thrust,
            Some(Family::Staff) => SwingKind::Sweep,
            Some(_) => SwingKind::Chop,
        }
    }
}

/// A human arm from the shoulder, as a share of stature: to the wrist 0.332 (upper arm 0.186,
/// forearm 0.146) and to the fingertips 0.44 (hand 0.108), Drillis and Contini's segment
/// lengths as tabled in D. A. Winter, *Biomechanics and Motor Control of Human Movement*,
/// 4th ed., Wiley 2009, fig. 4.1.
const ARM_TO_WRIST: f32 = 0.332;
const ARM_TO_FINGERTIPS: f32 = 0.44;
/// A human's stature, world units.
const STATURE: f32 = 17.0;

/// What the main hand swings: its damage, its reach and how it is swung.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weapon {
    pub kind: SwingKind,
    /// Its damage dice, or `None` for a bare fist.
    pub dice: Option<Dice>,
    /// Added to each roll (an affix's damage bonus, rounded).
    pub bonus: i32,
    /// How far from the delver's centre a blow reaches, world units.
    pub reach: f32,
    /// The hold it is swung from, for the view model (`None` with nothing in hand).
    pub hold: Option<Hold>,
}

impl Weapon {
    /// A bare fist: 1 + Strength modifier, bludgeoning (SRD 5.1 p. 95, "Unarmed strike"),
    /// reaching an arm's length.
    pub fn fist() -> Weapon {
        Weapon { kind: SwingKind::Punch, dice: None, bonus: 0, reach: ARM_TO_FINGERTIPS * STATURE, hold: None }
    }

    /// What the delver's main hand swings. A melee weapon rolls its own damage dice
    /// (`fb_items` stats, the plate's); anything else held, a bow or a torch among them, is an
    /// improvised weapon of 1d4 (SRD 5.1 p. 65, "Improvised Weapons"). The reach is the arm to
    /// the wrist and the item's length beyond it: a sword reaches about 1.5 m, near the SRD's
    /// 5-foot reach for most creatures (p. 95), and a long spear further.
    pub fn in_hand(inventory: &Inventory, character: &Character) -> Weapon {
        let Some(item) = inventory.worn(fb_items::Slot::Mainhand).and_then(|id| inventory.item(id)) else { return Weapon::fist() };
        let length = item.shape.build().bounds().map_or(0.0, |(lo, hi)| (0..3).map(|i| hi[i] - lo[i]).fold(0.0, f32::max));
        let hold = Hold::of(&item.shape, &fb_body::Body::new(&character.spec()));
        Weapon { reach: ARM_TO_WRIST * STATURE + length, hold, ..Weapon::of_stats(Some(item)) }
    }

    /// What `item` in the main hand rolls, without its reach or hold (which need its built
    /// shape and the body): the damage dice and bonus [`Weapon::in_hand`] uses, so a view that
    /// only wants the numbers (the lab) takes the same ones. A bare fist with `None`.
    pub fn of_stats(item: Option<&fb_items::Item>) -> Weapon {
        let Some(item) = item else { return Weapon::fist() };
        let melee = is_melee(item.shape.family());
        let stats = &item.profile.stats;
        let dice = if melee { stats.damage.unwrap_or(Dice::new(1, 4)) } else { Dice::new(1, 4) };
        Weapon { kind: SwingKind::of(Some(&item.shape)), dice: Some(dice), bonus: stats.damage_bonus.round() as i32, reach: ARM_TO_FINGERTIPS * STATURE, hold: None }
    }

    /// The range a blow's damage falls in with a Strength modifier of `strength`.
    pub fn damage_range(&self, strength: i32) -> (u32, u32) {
        let range = self.damage_range_explained(strength);
        (range.min.value() as u32, range.max.value() as u32)
    }

    /// [`Weapon::damage_range`] with the terms of its least, mean and most.
    pub fn damage_range_explained(&self, strength: i32) -> rules::DamageRange {
        rules::damage_range(self.dice_sides(), self.bonus, strength)
    }

    /// The dice as the rules take them: a count and the sides.
    fn dice_sides(&self) -> Option<(u32, u32)> {
        self.dice.map(|d| (d.count as u32, d.sides as u32))
    }

    /// One blow's damage: the dice, plus the bonus and the Strength modifier (SRD 5.1 p. 96,
    /// "Damage Rolls": roll the weapon's dice and add your ability modifier). The SRD lets a
    /// penalty bring a blow to 0; here every blow that lands does at least 1 (our own).
    pub fn roll(&self, strength: i32, rng: &mut Mulberry32) -> u32 {
        self.roll_explained(strength, rng).value() as u32
    }

    /// [`Weapon::roll`] with every die and bonus listed.
    pub fn roll_explained(&self, strength: i32, rng: &mut Mulberry32) -> fb_character::explain::Explained {
        rules::weapon_damage(self.dice_sides(), self.bonus, strength, rng)
    }
}

/// A creature a blow might land on: its entity, its centre on the floor (world `(x, z)`) and
/// the radius of its body there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub entity: Entity,
    pub centre: Vec2,
    pub radius: f32,
}

/// The nearest of `targets` a blow from `from` facing `forward` (both on the floor) reaches:
/// its body's edge within `reach`, and some of it within `cone` radians of straight ahead
/// (`hit_cone_degrees`, [`crate::tuning::CombatTuning`]).
pub fn target_in_reach(from: Vec2, forward: Vec2, reach: f32, cone: f32, targets: &[Target]) -> Option<Entity> {
    let forward = forward.normalize_or_zero();
    targets
        .iter()
        .filter_map(|t| {
            let to = t.centre - from;
            let distance = to.length();
            let edge = distance - t.radius;
            if edge > reach {
                return None;
            }
            // Standing on it counts; otherwise the angle to its nearest side.
            if distance > t.radius {
                let off = forward.angle_to(to).abs() - (t.radius / distance).asin();
                if off > cone {
                    return None;
                }
            }
            Some((edge, t.entity))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, e)| e)
}

/// The damage a bite does the delver: its own, and its venom after a Constitution save
/// against the rules' difficulty class (SRD 5.1 p. 386, "Poisonous Snake"; rounding the half
/// down is our own).
pub fn bite_damage_with(rules: &Rules, attack: &CreatureAttack, constitution: i32, rng: &mut Mulberry32) -> u32 {
    rules::bite_damage_with(rules, attack.damage, attack.poison, modifier(constitution), rng).damage.value() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_character::Attributes;

    /// The hit cone as shipped.
    const CONE: f32 = 50.0 * std::f32::consts::PI / 180.0;

    #[test]
    fn the_cone_here_is_the_shipped_one() {
        assert!((Tuning::default().combat.hit_cone() - CONE).abs() < 1e-6);
    }

    fn delver(class: Class, con: i32) -> Character {
        Character { class, attributes: Attributes { strength: 12, dexterity: 12, constitution: con, intelligence: 10, wisdom: 10 }, ..Character::roll(3) }
    }

    #[test]
    fn hit_points_follow_the_srd_at_first_level() {
        assert_eq!(first_level_hit_points_with(fb_character::rules::defaults(), &delver(Class::Fighter, 14)), 12);
        assert_eq!(first_level_hit_points_with(fb_character::rules::defaults(), &delver(Class::Ranger, 10)), 10);
        assert_eq!(first_level_hit_points_with(fb_character::rules::defaults(), &delver(Class::Cleric, 9)), 7);
        assert_eq!(first_level_hit_points_with(fb_character::rules::defaults(), &delver(Class::Mage, 3)), 2);
    }

    #[test]
    fn health_stops_at_zero_and_says_when_it_gets_there() {
        let mut h = Health::full(7);
        assert!(!h.hurt(4));
        assert!(h.hurt(5));
        assert_eq!(h.hp, 0);
        assert!(!h.hurt(3), "already down");
        assert!(h.is_down());
    }

    #[test]
    fn swings_suit_the_hold() {
        let roll = |base: &str| fb_items::roll_item(base, 3).unwrap().shape;
        assert_eq!(SwingKind::of(Some(&roll("Mace"))), SwingKind::Chop);
        assert_eq!(SwingKind::of(Some(&roll("Hand axe"))), SwingKind::Chop);
        assert_eq!(SwingKind::of(Some(&roll("Club"))), SwingKind::Chop);
        assert_eq!(SwingKind::of(Some(&roll("Sword"))), SwingKind::Slash);
        assert_eq!(SwingKind::of(Some(&roll("Spear"))), SwingKind::Thrust);
        assert_eq!(SwingKind::of(Some(&roll("Dagger"))), SwingKind::Thrust);
        assert_eq!(SwingKind::of(Some(&roll("Staff"))), SwingKind::Sweep);
        assert_eq!(SwingKind::of(None), SwingKind::Punch);
    }

    #[test]
    fn only_the_main_hand_swings() {
        // A weapon in the off hand is held but does not attack yet (#122): a bare main hand
        // still punches, and a main-hand weapon swings whatever the off hand holds.
        let character = Character::roll(3);
        let mut inventory = Inventory::default();
        let dagger = inventory.add(fb_items::roll_item("Dagger", 7).unwrap());
        inventory.wear(dagger, fb_items::Slot::Offhand).unwrap();
        assert_eq!(Weapon::in_hand(&inventory, &character).kind, SwingKind::Punch);
        let sword = inventory.add(fb_items::roll_item("Sword", 7).unwrap());
        inventory.wear(sword, fb_items::Slot::Mainhand).unwrap();
        assert_eq!(Weapon::in_hand(&inventory, &character).kind, SwingKind::Slash);
    }

    #[test]
    fn damage_rolls_stay_within_their_dice() {
        let mut rng = Mulberry32::new(5);
        let sword = Weapon { kind: SwingKind::Slash, dice: Some(Dice::new(1, 8)), bonus: 1, reach: 14.0, hold: None };
        let greatsword = Weapon { dice: Some(Dice::new(2, 6)), bonus: 0, ..sword };
        let mut seen = std::collections::HashSet::new();
        for _ in 0..2000 {
            let d = sword.roll(2, &mut rng);
            assert!((4..=11).contains(&d), "{d}");
            seen.insert(d);
            assert!((2..=12).contains(&greatsword.roll(0, &mut rng)));
            // A fist is 1 + Strength, at least 1.
            assert_eq!(Weapon::fist().roll(3, &mut rng), 4);
            assert_eq!(Weapon::fist().roll(-2, &mut rng), 1);
        }
        assert_eq!(seen.len(), 8, "every face turns up");
        assert_eq!(sword.damage_range(2), (4, 11));
    }

    #[test]
    fn a_sword_kills_a_rat_in_a_few_hits() {
        use fb_creatures::Species;
        let mut rng = Mulberry32::new(11);
        let inventory_sword = fb_items::roll_item("Sword", 7).unwrap();
        let dice = inventory_sword.profile.stats.damage.unwrap();
        let sword = Weapon { kind: SwingKind::Slash, dice: Some(dice), bonus: 0, reach: 14.0, hold: None };
        for species in [Species::Rat, Species::GiantRat] {
            let mut most = 0;
            for _ in 0..500 {
                let mut rat = Health::full(species.stat_block().hit_points as i32);
                let mut hits = 0;
                while !rat.is_down() {
                    rat.hurt(sword.roll(0, &mut rng));
                    hits += 1;
                }
                most = most.max(hits);
            }
            // A rat (1 hit point) falls to any blow; a giant rat (7) in a few.
            let bound = if species == Species::Rat { 1 } else { 7 };
            assert!(most <= bound, "{species:?}: {most} hits");
        }
    }

    #[test]
    fn blows_land_only_in_reach_and_ahead() {
        let e = |i| Entity::from_raw_u32(i).unwrap();
        let near_ahead = Target { entity: e(1), centre: Vec2::new(0.0, -8.0), radius: 1.5 };
        let far_ahead = Target { entity: e(2), centre: Vec2::new(0.0, -30.0), radius: 1.5 };
        let behind = Target { entity: e(3), centre: Vec2::new(0.0, 6.0), radius: 1.5 };
        let beside = Target { entity: e(4), centre: Vec2::new(9.0, -1.0), radius: 1.5 };
        let forward = Vec2::new(0.0, -1.0);
        assert_eq!(target_in_reach(Vec2::ZERO, forward, 14.0, CONE, &[far_ahead, behind, beside, near_ahead]), Some(e(1)));
        assert_eq!(target_in_reach(Vec2::ZERO, forward, 14.0, CONE, &[far_ahead, behind, beside]), None);
        // A long reach finds the far one; a short one finds nothing.
        assert_eq!(target_in_reach(Vec2::ZERO, forward, 30.0, CONE, &[far_ahead]), Some(e(2)));
        assert_eq!(target_in_reach(Vec2::ZERO, forward, 5.0, CONE, &[near_ahead]), None);
        // A big body off to the side is still caught by its near edge.
        let big = Target { entity: e(5), centre: Vec2::new(8.0, -4.0), radius: 7.0 };
        assert_eq!(target_in_reach(Vec2::ZERO, forward, 10.0, CONE, &[big]), Some(e(5)));
    }

    /// A poisonous snake's venom after the delver's save under the default rules.
    fn venom_after_save(poison: u32, constitution: i32, rng: &mut Mulberry32) -> u32 {
        rules::venom_after_save(poison, modifier(constitution), rng).damage.value() as u32
    }

    #[test]
    fn venom_is_halved_on_a_good_save() {
        let mut rng = Mulberry32::new(2);
        let (mut halved, mut full) = (0, 0);
        for _ in 0..2000 {
            match venom_after_save(7, 10, &mut rng) {
                3 => halved += 1,
                7 => full += 1,
                other => panic!("{other}"),
            }
        }
        // d20 ≥ 10 with no modifier: 11 in 20.
        assert!((1000..1200).contains(&halved), "{halved} / {full}");
        assert_eq!(venom_after_save(0, 10, &mut rng), 0);
    }

    /// Seed 1207's creatures as bones, thinking and moving, with a delver holding `base` (or
    /// nothing) standing `away` world units from the first rat and facing it, stepped 1/60 s a
    /// frame with the whole of combat but the drawing.
    fn fight(base: Option<&str>, away: f32) -> (App, Entity, Entity) {
        use crate::creatures::{Critter, animate, behaviour::BehaviourPlugin, idle, spawn_skeleton};
        use crate::play::{CurrentLevel, Player};
        use bevy::time::TimeUpdateStrategy;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin, bevy::state::app::StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .insert_state(crate::launch::AppState::Delving)
            .init_resource::<crate::keymap::Keymap>()
            .init_resource::<Character>()
            .init_resource::<crate::play::ScreenOpen>()
            .add_plugins((BehaviourPlugin, CombatPlugin))
            .insert_resource(RunDice(Mulberry32::new(9)))
            .add_systems(Update, (idle, animate).chain());
        let mut inventory = Inventory::default();
        if let Some(base) = base {
            let id = inventory.add(fb_items::roll_item(base, 7).unwrap());
            inventory.wear(id, fb_items::Slot::Mainhand).unwrap();
        }
        app.insert_resource(inventory);
        let level = fb_level::generate(1207, 1);
        app.insert_resource(CurrentLevel(level.clone()));
        let roots: Vec<Entity> = {
            let mut commands = app.world_mut().commands();
            level.creatures.iter().map(|spot| spawn_skeleton(&mut commands, &level, spot, &spot.creature().rig()).0).collect()
        };
        app.world_mut().flush();
        let rat = *roots.iter().find(|&&r| app.world().get::<Critter>(r).unwrap().spot.species == fb_creatures::Species::Rat).unwrap();
        let at = app.world().get::<Transform>(rat).unwrap().translation;
        // West of the rat, facing east toward it (a camera's -Z turned to +X).
        let from = at - Vec3::X * away;
        let player = app.world_mut().spawn((Player, Transform::from_translation(from).looking_at(at, Vec3::Y))).id();
        (app, player, rat)
    }

    fn swing_at(app: &mut App, player: Entity, frames: usize) {
        let weapon = Weapon::in_hand(app.world().resource::<Inventory>(), app.world().resource::<Character>());
        app.world_mut().entity_mut(player).insert(swing::Swing::new(weapon));
        for _ in 0..frames {
            // The rat stays put for the blow, as if caught mid-step.
            app.update();
        }
    }

    #[test]
    fn a_sword_fells_a_rat_and_its_body_stays() {
        use crate::creatures::Critter;
        use feedback::Corpse;
        let (mut app, player, rat) = fight(Some("Sword"), 9.0);
        app.update();
        let hp = app.world().get::<Health>(rat).unwrap().hp;
        assert_eq!(hp, 1, "a rat has the SRD's 1 hit point");
        swing_at(&mut app, player, 60);
        assert!(app.world().get::<Corpse>(rat).is_some(), "the rat fell to one blow");
        let fell = app.world().get::<Transform>(rat).unwrap().translation;
        // Its body stays for the run, still, and blocks nobody.
        for _ in 0..600 {
            app.update();
        }
        assert!(app.world().get::<Critter>(rat).is_some(), "the body is still there");
        let now = app.world().get::<Transform>(rat).unwrap();
        assert!(now.translation.xz().distance(fell.xz()) < 0.5, "the body lies where it fell");
        assert!((now.rotation * Vec3::Y).y < -0.5, "on its back");
        assert!(app.world().get::<crate::creatures::behaviour::Brain>(rat).is_none());
    }

    #[test]
    fn a_killed_rat_flashes_before_it_rolls() {
        use feedback::{COLLAPSE_TIME, Corpse, Flinch, flash_strength};
        let combat = Tuning::default().combat;
        let death_hold = combat.flash_hold;
        let (mut app, player, rat) = fight(Some("Sword"), 9.0);
        app.update();
        let weapon = Weapon::in_hand(app.world().resource::<Inventory>(), app.world().resource::<Character>());
        app.world_mut().entity_mut(player).insert(swing::Swing::new(weapon));
        let mut frames = 0;
        while app.world().get::<Corpse>(rat).is_none() {
            app.update();
            frames += 1;
            assert!(frames < 60, "the blow never landed");
        }
        // Through the hold it stands where it was struck, flashing at full.
        let mut held = 0;
        while app.world().get::<Corpse>(rat).unwrap().since < death_hold - 1e-3 {
            let flinch = app.world().get::<Flinch>(rat).expect("flashing");
            assert_eq!(flash_strength(flinch.left, &combat), 1.0);
            let now = app.world().get::<Transform>(rat).unwrap();
            assert!((now.rotation * Vec3::Y).y > 0.99, "rolling during the hold");
            held += 1;
            app.update();
        }
        assert!(held >= 6, "held {held} frames");
        // Then it rolls over.
        for _ in 0..((COLLAPSE_TIME * 60.0) as usize + 2) {
            app.update();
        }
        let now = app.world().get::<Transform>(rat).unwrap();
        assert!((now.rotation * Vec3::Y).y < -0.5, "on its back");
        // About its own middle, where it was struck.
        let fell = app.world().get::<Corpse>(rat).unwrap().fell;
        let fb_creatures::Creature::Rat(state) = &app.world().get::<crate::creatures::Critter>(rat).unwrap().creature else { unreachable!() };
        let middle = Vec3::Y * state.dims().centre_height;
        assert!(now.transform_point(middle).xz().distance(fell.transform_point(middle).xz()) < 0.5 * state.dims().half_width);
    }

    #[test]
    fn a_blow_to_a_laid_out_snakes_mid_body_lands() {
        use crate::creatures::{Bones, Critter, Driven, Intent, behaviour::{Brain, body_radius}};
        use fb_creatures::Creature;
        let (mut app, player, _) = fight(Some("Sword"), 9.0);
        app.update();
        let snake = {
            let mut q = app.world_mut().query::<(Entity, &Critter)>();
            q.iter(app.world()).find(|(_, c)| c.spot.species == fb_creatures::Species::ConstrictorSnake).map(|(e, _)| e).unwrap()
        };
        // Laid out and still, its mind stopped so it stays that way.
        app.world_mut().entity_mut(snake).remove::<Brain>().insert((Driven, Intent::default()));
        // The delver nearby, so its pose is kept up (`creatures::POSE_RANGE`).
        let near = app.world().get::<Transform>(snake).unwrap().translation + Vec3::X * 60.0;
        app.world_mut().get_mut::<Transform>(player).unwrap().translation = near;
        for _ in 0..240 {
            app.update();
        }
        let bones = app.world().get::<Bones>(snake).unwrap().0.clone();
        let spine: Vec<Vec3> = (0..fb_creatures::snake::SEGMENTS).map(|k| app.world().get::<GlobalTransform>(bones[fb_creatures::snake::bone::SPINE + k]).unwrap().translation()).collect();
        let root = *app.world().get::<Transform>(snake).unwrap();
        let critter = app.world().get::<Critter>(snake).unwrap().clone();
        let Creature::Snake(state) = &critter.creature else { unreachable!() };
        assert!(spine[0].xz().distance(spine[spine.len() - 1].xz()) > state.dims().body_length * 0.25, "laid out, not coiled");
        let weapon = Weapon::in_hand(app.world().resource::<Inventory>(), app.world().resource::<Character>());
        // The segment of its body farthest from its root, and a place beside it, facing it,
        // in the sword's reach of it but not of one circle at the root.
        let mid = *spine.iter().max_by(|a, b| a.xz().distance(root.translation.xz()).total_cmp(&b.xz().distance(root.translation.xz()))).unwrap();
        let at_root = Target { entity: snake, centre: root.translation.xz(), radius: body_radius(&critter) };
        let from = (0..24)
            .map(|i| mid.xz() + Vec2::from_angle(i as f32 * std::f32::consts::TAU / 24.0) * 0.6 * weapon.reach)
            .filter(|p| spine.iter().all(|s| s.xz().distance(*p) > critter.radius))
            .find(|&p| target_in_reach(p, mid.xz() - p, weapon.reach, CONE, &[at_root]).is_none())
            .expect("nowhere beside its body out of reach of its root");
        let floor = Vec3::new(from.x, root.translation.y, from.y);
        app.world_mut().entity_mut(player).insert(Transform::from_translation(floor).looking_at(Vec3::new(mid.x, root.translation.y, mid.z), Vec3::Y));
        let before = app.world().get::<Health>(snake).unwrap().hp;
        swing_at(&mut app, player, 40);
        assert!(app.world().get::<Health>(snake).unwrap().hp < before, "the blow to its body landed");
    }

    #[test]
    fn a_blow_out_of_reach_or_behind_lands_on_nothing() {
        use feedback::Corpse;
        // Too far for a fist.
        let (mut app, player, rat) = fight(None, 30.0);
        app.update();
        swing_at(&mut app, player, 40);
        assert!(app.world().get::<Corpse>(rat).is_none());
        // In reach, but facing away.
        let (mut app, player, rat) = fight(Some("Sword"), 9.0);
        app.update();
        app.world_mut().get_mut::<Transform>(player).unwrap().rotate_y(std::f32::consts::PI);
        swing_at(&mut app, player, 40);
        assert!(app.world().get::<Corpse>(rat).is_none());
    }
}
