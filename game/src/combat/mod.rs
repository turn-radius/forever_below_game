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
        app.init_resource::<RunDice>().init_resource::<Tuning>().add_plugins((swing::SwingPlugin, feedback::FeedbackPlugin, vitals::VitalsPlugin, orbs::OrbPlugin, eye::EyePlugin, kick::KickPlugin));
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
pub fn first_level_hit_points(character: &Character) -> i32 {
    let die = match character.class {
        Class::Fighter | Class::Ranger => 10,
        Class::Cleric => 8,
        Class::Mage => 6,
    };
    (die + modifier(character.constitution())).max(1)
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
    /// Whether it bludgeons (a mace, a club, a staff, a fist): a skeleton takes double.
    pub bludgeoning: bool,
}

impl Weapon {
    /// A bare fist: 1 + Strength modifier, bludgeoning (SRD 5.1 p. 95, "Unarmed strike"),
    /// reaching an arm's length.
    pub fn fist() -> Weapon {
        Weapon { kind: SwingKind::Punch, dice: None, bonus: 0, reach: ARM_TO_FINGERTIPS * STATURE, hold: None, bludgeoning: true }
    }

    /// What the delver's main hand swings. A melee weapon rolls its own damage dice
    /// (`fb_items` stats, the plate's); anything else held, a bow or a torch among them, is an
    /// improvised weapon of 1d4 (SRD 5.1 p. 65, "Improvised Weapons"). The reach is the arm to
    /// the wrist and the item's length beyond it: a sword reaches about 1.5 m, near the SRD's
    /// 5-foot reach for most creatures (p. 95), and a long spear further.
    pub fn in_hand(inventory: &Inventory, character: &Character) -> Weapon {
        use armory::Family;
        let Some(item) = inventory.worn(fb_items::Slot::Mainhand).and_then(|id| inventory.item(id)) else { return Weapon::fist() };
        let melee = matches!(item.shape.family(), Family::Sword | Family::Mace | Family::Axe | Family::Club | Family::Spear | Family::Dagger | Family::Staff);
        let stats = &item.profile.stats;
        let dice = if melee { stats.damage.unwrap_or(Dice::new(1, 4)) } else { Dice::new(1, 4) };
        let length = item.shape.build().bounds().map_or(0.0, |(lo, hi)| (0..3).map(|i| hi[i] - lo[i]).fold(0.0, f32::max));
        let hold = Hold::of(&item.shape, &fb_body::Body::new(&character.spec()));
        Weapon { kind: SwingKind::of(Some(&item.shape)), dice: Some(dice), bonus: stats.damage_bonus.round() as i32, reach: ARM_TO_WRIST * STATURE + length, hold, bludgeoning: matches!(item.shape.family(), Family::Mace | Family::Club | Family::Staff) || !melee }
    }

    /// The range a blow's damage falls in with a Strength modifier of `strength`.
    pub fn damage_range(&self, strength: i32) -> (u32, u32) {
        let (lo, hi) = match self.dice {
            Some(d) => (d.count as i32, (d.count as i32) * (d.sides as i32)),
            None => (1, 1),
        };
        (((lo + self.bonus + strength).max(1)) as u32, ((hi + self.bonus + strength).max(1)) as u32)
    }

    /// One blow's damage: the dice, plus the bonus and the Strength modifier (SRD 5.1 p. 96,
    /// "Damage Rolls": roll the weapon's dice and add your ability modifier). The SRD lets a
    /// penalty bring a blow to 0; here every blow that lands does at least 1 (our own).
    pub fn roll(&self, strength: i32, rng: &mut Mulberry32) -> u32 {
        let dice = self.dice.map_or(1, |d| (0..d.count).map(|_| rng.int(1, d.sides.max(1) as u32) as i32).sum());
        (dice + self.bonus + strength).max(1) as u32
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

/// A poisonous snake's venom after the delver's save: a Constitution saving throw (d20 +
/// Constitution modifier) against DC 10 halves it, rounded down (SRD 5.1 p. 386, "Poisonous
/// Snake"; rounding down is our own). A placeholder until the character simulation has
/// poison of its own.
pub fn venom_after_save(poison: u32, constitution: i32, rng: &mut Mulberry32) -> u32 {
    if poison == 0 {
        return 0;
    }
    let save = rng.int(1, 20) as i32 + modifier(constitution);
    if save >= 10 { poison / 2 } else { poison }
}

/// The damage a bite does the delver: its own, and its venom after the save.
pub fn bite_damage(attack: &CreatureAttack, constitution: i32, rng: &mut Mulberry32) -> u32 {
    attack.damage + venom_after_save(attack.poison, constitution, rng)
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
        assert_eq!(first_level_hit_points(&delver(Class::Fighter, 14)), 12);
        assert_eq!(first_level_hit_points(&delver(Class::Ranger, 10)), 10);
        assert_eq!(first_level_hit_points(&delver(Class::Cleric, 9)), 7);
        assert_eq!(first_level_hit_points(&delver(Class::Mage, 3)), 2);
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
        let sword = Weapon { kind: SwingKind::Slash, dice: Some(Dice::new(1, 8)), bonus: 1, reach: 14.0, hold: None, bludgeoning: false };
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
        let sword = Weapon { kind: SwingKind::Slash, dice: Some(dice), bonus: 0, reach: 14.0, hold: None, bludgeoning: false };
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
        let level = fb_level::generate(1207);
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

    // Skeletons: what they fight with and what they drop.

    /// A skeleton of `seed` in `outfit` (its own rolled gear when `None`), alone in seed 1207's
    /// level at the dev spot in front of the start, and a delver standing at the start looking
    /// at it with a sword, with the whole of combat, the floor machinery and the creatures'
    /// bones but no drawing. Stepped 1/60 s a frame.
    fn skeleton_fight(seed: u32, outfit: Option<fb_items::outfit::OutfitSet>) -> (App, Entity, Entity) {
        use crate::creatures::{animate, behaviour::BehaviourPlugin, idle, skeletons::{BoneSounds, SkeletonKit, SkeletonsPlugin}, spawn_skeleton_with, dev_spots_for};
        use crate::play::{CurrentLevel, Player};
        use bevy::time::TimeUpdateStrategy;
        let weak = |n: u128| Handle::<AudioSource>::Uuid(bevy::asset::uuid::Uuid::from_u128(n), std::marker::PhantomData);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin, bevy::state::app::StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .insert_state(crate::launch::AppState::Delving)
            .init_resource::<crate::keymap::Keymap>()
            .init_resource::<Character>()
            .init_resource::<crate::play::ScreenOpen>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::render::ToonMaterial>>()
            .init_resource::<Assets<crate::render::OutlineMaterial>>()
            .init_resource::<crate::render::OutlineMaterials>()
            .add_plugins((BehaviourPlugin, CombatPlugin, SkeletonsPlugin))
            .insert_resource(RunDice(Mulberry32::new(9)))
            // The rat's and the snake's voices are all distinct handles, to tell if one is used.
            .insert_resource(feedback::CreatureSounds { squeak: weak(1), hiss: weak(2), rat_squeal: weak(3), snake_rise: weak(4), thud: weak(5) })
            .insert_resource(BoneSounds { steps: [weak(0x20), weak(0x21), weak(0x22)], rattle: weak(0x23), hit: weak(0x24), block: weak(0x25), collapse: weak(0x26) })
            .add_systems(Update, (idle, animate, crate::inventory::underfoot::apply_floor_changes).chain());
        app.insert_resource(Inventory::default());
        let mut level = fb_level::generate(1207);
        level.items.clear();
        level.creatures.clear();
        let spot = dev_spots_for(&level, &format!("skeleton:{seed}")).remove(0);
        app.insert_resource(CurrentLevel(level.clone()));
        let state = fb_creatures::SkeletonState::roll(seed);
        let kit = SkeletonKit::new(&state, outfit.unwrap_or_else(|| fb_items::undead::skeleton_outfit(seed)), [None, None]);
        let skeleton = {
            let mut commands = app.world_mut().commands();
            spawn_skeleton_with(&mut commands, &level, &spot, &state.build(), Some(kit)).0
        };
        app.world_mut().flush();
        let (sx, sy, _) = level.start();
        let start = Vec3::from_array(fb_level::Level::world(sx as f32 + 0.5, sy as f32 + 0.5));
        let at = app.world().get::<Transform>(skeleton).unwrap().translation;
        let mut inventory = Inventory::default();
        let sword = inventory.add(fb_items::roll_item("Sword", 7).unwrap());
        inventory.wear(sword, fb_items::Slot::Mainhand).unwrap();
        app.insert_resource(inventory);
        let player = app.world_mut().spawn((Player, Transform::from_translation(Vec3::new(start.x, at.y, start.z)).looking_at(at, Vec3::Y))).id();
        (app, player, skeleton)
    }

    /// A seed whose skeleton rolls most of a kit of gear.
    fn kitted_seed() -> u32 {
        (0..400).find(|&s| fb_items::undead::skeleton_outfit(s).worn.len() >= 5).expect("some skeleton is kitted")
    }

    fn strike_down(app: &mut App, skeleton: Entity) {
        use feedback::{Corpse, Struck};
        for _ in 0..400 {
            if app.world().get::<Corpse>(skeleton).is_some() {
                return;
            }
            app.world_mut().write_message(Struck { target: skeleton, damage: 30, from: Vec2::new(-30.0, 0.0), bludgeoning: false });
            app.update();
        }
        panic!("the skeleton never fell");
    }

    fn on_the_floor(app: &mut App) -> Vec<crate::carry::FloorItem> {
        app.world_mut().query::<&crate::carry::FloorItem>().iter(app.world()).map(|f| crate::carry::FloorItem { id: f.id, spot: f.spot, min: f.min, max: f.max }).collect()
    }

    #[test]
    fn killing_a_skeleton_drops_every_item_it_wore_once_to_pick_up_and_wield() {
        use crate::inventory::{FloorChange, FloorChanges};
        use crate::play::CurrentLevel;
        let seed = kitted_seed();
        let gear = fb_items::undead::skeleton_outfit(seed);
        let (mut app, _, skeleton) = skeleton_fight(seed, None);
        app.update();
        assert!(on_the_floor(&mut app).is_empty(), "nothing lies there while it stands");
        let dropped_before = app.world().resource::<CurrentLevel>().0.items.len();
        strike_down(&mut app, skeleton);
        for _ in 0..120 {
            app.update();
        }
        // Every item it wore and held, as the same item, once.
        let floor = on_the_floor(&mut app);
        assert_eq!(floor.len(), gear.worn.len(), "one for each piece of gear");
        let inventory = app.world().resource::<Inventory>();
        let mut lying: Vec<(String, u32)> = floor.iter().map(|f| inventory.item(f.id).map(|i| (i.base.to_string(), i.seed)).unwrap()).collect();
        let mut wore: Vec<(String, u32)> = gear.worn.iter().map(|(_, i)| (i.base.to_string(), i.seed)).collect();
        lying.sort();
        wore.sort();
        assert_eq!(lying, wore);
        for f in &floor {
            let item = inventory.item(f.id).unwrap();
            let original = gear.worn.iter().map(|(_, i)| i).find(|i| i.seed == item.seed && i.base == item.base).unwrap();
            assert_eq!(item, original, "the same rarity, affixes and wear");
        }
        assert_eq!(app.world().resource::<CurrentLevel>().0.items.len(), dropped_before + gear.worn.len(), "the level records them");
        // They fall near it, not far away.
        let at = app.world().get::<Transform>(skeleton).unwrap().translation.xz();
        for f in &floor {
            let world = fb_level::Level::world(f.spot.x, f.spot.y);
            let there = Vec2::new(world[0], world[2]);
            assert!(there.distance(at) < 12.0, "{} from where it fell", there.distance(at));
        }
        // Picked up, they are the player's: lifted from the floor, worn and wielded.
        let slots: Vec<(crate::inventory::ItemId, fb_items::Slot)> = floor.iter().map(|f| (f.id, inventory.item(f.id).unwrap().profile.slot.unwrap())).collect();
        for &(id, _) in &slots {
            app.world_mut().resource_mut::<FloorChanges>().0.push(FloorChange::Lifted(id));
        }
        app.update();
        assert!(on_the_floor(&mut app).is_empty(), "all taken up");
        let mut inventory = app.world_mut().resource_mut::<Inventory>();
        for &(id, slot) in &slots {
            // Swapping with whatever the delver already wears there, as equipping does.
            inventory.equip(id, slot).unwrap_or_else(|e| panic!("{slot:?}: {e:?}"));
            assert_eq!(inventory.worn(slot), Some(id), "{slot:?} is worn");
        }
        assert_eq!(inventory.worn(fb_items::Slot::Mainhand).is_some(), gear.item(fb_items::Slot::Mainhand).is_some());
    }

    #[test]
    fn a_skeletons_gear_is_dropped_once_and_a_body_brought_back_drops_none() {
        use feedback::Corpse;
        let seed = kitted_seed();
        let n = fb_items::undead::skeleton_outfit(seed).worn.len();
        let (mut app, _, skeleton) = skeleton_fight(seed, None);
        app.update();
        strike_down(&mut app, skeleton);
        for _ in 0..600 {
            app.update();
        }
        assert_eq!(on_the_floor(&mut app).len(), n);
        // Its marker lost with a body long fallen: still no more.
        app.world_mut().entity_mut(skeleton).remove::<crate::creatures::skeletons::Looted>();
        for _ in 0..120 {
            app.update();
        }
        assert_eq!(on_the_floor(&mut app).len(), n, "an old body never drops its gear again");
        // A dead skeleton put back as it was (what leaving a level and returning does) has
        // dropped what it had and does not drop it a second time.
        let (mut app, _, restored) = skeleton_fight(seed, None);
        app.update();
        let fell = *app.world().get::<Transform>(restored).unwrap();
        app.world_mut().entity_mut(restored).insert(Corpse { since: 30.0, fell });
        for _ in 0..120 {
            app.update();
        }
        assert!(on_the_floor(&mut app).is_empty(), "a restored body drops nothing");
    }

    #[test]
    fn a_skeleton_collapses_onto_its_back_where_it_fell() {
        let (mut app, _, skeleton) = skeleton_fight(kitted_seed(), None);
        app.update();
        let stood = app.world().get::<Transform>(skeleton).unwrap().translation;
        strike_down(&mut app, skeleton);
        for _ in 0..90 {
            app.update();
        }
        let down = app.world().get::<Transform>(skeleton).unwrap();
        // Its up now points along the floor (it lies down), and it is where it stood.
        assert!((down.rotation * Vec3::Y).y.abs() < 0.2, "lying down");
        assert!(down.translation.xz().distance(stood.xz()) < 6.0);
        assert_eq!(app.world().get::<crate::creatures::Intent>(skeleton).unwrap().fallen, 1.0);
    }

    /// The distance from the delver at which a skeleton in `base` first lands a blow.
    fn reach_in_use(base: &str) -> f32 {
        use crate::creatures::behaviour::CreatureAttack;
        let outfit = fb_items::outfit::OutfitSet { worn: vec![(fb_items::Slot::Mainhand, fb_items::roll_item(base, 5).unwrap())], goods: Vec::new(), ground: Vec::new() };
        let (mut app, player, skeleton) = skeleton_fight(3, Some(outfit));
        let me = app.world().get::<Transform>(player).unwrap().translation.xz();
        for _ in 0..900 {
            app.update();
            let attacks: Vec<CreatureAttack> = app.world().resource::<Messages<CreatureAttack>>().iter_current_update_messages().copied().collect();
            if let Some(attack) = attacks.first() {
                assert_eq!(attack.attacker, skeleton);
                assert!(attack.damage >= 1);
                return attack.attacker_pos.distance(me);
            }
        }
        panic!("the skeleton with a {base} never struck");
    }

    #[test]
    fn a_skeleton_with_a_spear_strikes_from_further_than_one_with_a_dagger() {
        let (spear, dagger) = (reach_in_use("Spear"), reach_in_use("Dagger"));
        assert!(spear > dagger + 4.0, "spear {spear} dagger {dagger}");
    }

    #[test]
    fn a_skeleton_makes_its_own_sounds_never_a_rats_or_a_snakes() {
        use crate::creatures::skeletons::BoneSounds;
        let (mut app, _, skeleton) = skeleton_fight(kitted_seed(), None);
        let bones = app.world().resource::<BoneSounds>().clone();
        let voices = app.world().resource::<feedback::CreatureSounds>().clone();
        // Let it come at the delver and swing, take a blow and fall.
        for _ in 0..240 {
            app.update();
        }
        app.world_mut().write_message(feedback::Struck { target: skeleton, damage: 1, from: Vec2::new(-30.0, 0.0), bludgeoning: false });
        for _ in 0..30 {
            app.update();
        }
        strike_down(&mut app, skeleton);
        for _ in 0..60 {
            app.update();
        }
        let heard: Vec<Handle<AudioSource>> = app.world_mut().query::<&AudioPlayer<AudioSource>>().iter(app.world()).map(|p| p.0.clone()).collect();
        assert!(!heard.is_empty());
        let ours = [bones.rattle, bones.hit, bones.block, bones.collapse];
        for sound in &heard {
            // Its own, or the thud of its blow landing on the delver.
            assert!(ours.contains(sound) || *sound == voices.thud, "a sound that is not a skeleton's: {sound:?}");
            assert!(![&voices.squeak, &voices.hiss, &voices.rat_squeal, &voices.snake_rise].contains(&sound));
        }
        assert!(heard.contains(&ours[3]), "it clatters as it falls");
    }

    #[test]
    fn bludgeoning_hurts_a_skeleton_twice_as_much() {
        use feedback::{Struck, damage_to};
        let (app, _, skeleton) = skeleton_fight(kitted_seed(), None);
        let critter = app.world().get::<crate::creatures::Critter>(skeleton).unwrap();
        let blow = |bludgeoning| Struck { target: skeleton, damage: 4, from: Vec2::ZERO, bludgeoning };
        assert_eq!((damage_to(critter, &blow(false)), damage_to(critter, &blow(true))), (4, 8));
        // The delver's mace and club bludgeon, a sword does not.
        let delver = Character::roll(3);
        let weapon = |base: &str| {
            let mut inventory = Inventory::default();
            let id = inventory.add(fb_items::roll_item(base, 7).unwrap());
            inventory.wear(id, fb_items::Slot::Mainhand).unwrap();
            Weapon::in_hand(&inventory, &delver)
        };
        assert!(weapon("Mace").bludgeoning && weapon("Club").bludgeoning && Weapon::fist().bludgeoning);
        assert!(!weapon("Sword").bludgeoning && !weapon("Spear").bludgeoning && !weapon("Hand axe").bludgeoning);
    }
}
