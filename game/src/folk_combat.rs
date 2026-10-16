//! Folk in a fight: their hit points and Armor Class, what a blow does to them, and their fall.
//!
//! - **Who they are in a fight** ([`FolkStats`]): a folk's abilities are their folk's scores in
//!   `data/races.json` (the scores the delver's are rolled about, `crate::character`), and their
//!   class is their outfit's (`data/outfits.json`: a fighter, ranger, cleric or mage). Hit points
//!   are a 1st-level character's, as the delver's are: the class's hit die at its highest plus
//!   the Constitution modifier (SRD 5.1 "Hit Points at 1st Level", [`crate::combat::first_level_hit_points`]).
//!   A scavenger or someone freshly fallen has no class: they have a commoner's hit die, a d8, at
//!   its average, 4 (SRD 5.1 "Commoner": 4 (1d8)), plus the modifier, at least 1. Armor Class is
//!   the delver's sum over what they wear and hold (10, the Dexterity modifier and each item's
//!   armour, [`fb_character::explain::armour_class`]), as a skeleton's is.
//! - **Struck** ([`take_folk_blows`]): a swing finds a folk at the circle they stand in
//!   (`combat::swing::Strikeable`) and rolls to hit against their [`ArmourClass`]; a blow that
//!   lands takes its damage from their [`Health`], flashes them and knocks them back a step as it
//!   does a creature ([`Flinches`], a Medium creature's push), with a thud from where they stand.
//! - **Felled**: at 0 they hold, flashing, for the flash's hold, then fall onto their back about
//!   their feet ([`collapse_folk`], `feedback::laid_back`, as a skeleton falls). The body is a
//!   [`Corpse`]: it stops its routine, no longer turns its head, blocks nobody and shows no card.
//!   "Name is slain" shows low in the view.
//!
//! A folk's stance while lying is the one they had standing: the skinned figure is turned as a
//! whole, not ragdolled. Our own simplification.

use bevy::prelude::*;
use fb_character::explain::{Explained, armour_class};
use fb_character::modifier;
use fb_items::Slot;

use crate::character::Class;
use crate::combat::feedback::{Corpse, Flinches, Struck, collapse_share, knockback_for, laid_back};
use crate::combat::{ArmourClass, Health, first_level_hit_points_explained_for};
use crate::folk::{Denizen, Person};
use crate::folk_routine::Routine;
use crate::inventory::Toasts;
use crate::play::{BlocksTheWay, DelveSystems};
use crate::settings::GameSettings;
use crate::tuning::Tuning;

pub struct FolkCombatPlugin;

impl Plugin for FolkCombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Struck>().init_resource::<Tuning>().add_systems(Update, (arm_folk, take_folk_blows, collapse_folk).chain().in_set(DelveSystems));
    }
}

/// A commoner's hit die and its average (SRD 5.1 "Commoner": Hit Points 4 (1d8)).
const COMMONER_DIE: u32 = 8;
const COMMONER_HIT_POINTS: f64 = 4.0;

/// What a folk is in a fight, from who they are: a pure function of their spot.
#[derive(Clone, Debug, PartialEq)]
pub struct FolkStats {
    /// Their outfit's class, or `None` for a look that is none (scavenger, freshly fallen).
    pub class: Option<Class>,
    pub strength: i32,
    pub dexterity: i32,
    pub constitution: i32,
    pub hit_points: Explained,
    pub armour_class: Explained,
}

impl FolkStats {
    /// `person`'s stats, as the module says.
    pub fn of(person: &Person) -> FolkStats {
        let [strength, dexterity, constitution, _, _] = person.spot.folk.race().stats;
        let class = Class::of_outfit(person.outfit);
        let hit_points = match class {
            Some(class) => first_level_hit_points_explained_for(class, constitution),
            None => {
                let e = Explained::new("Hit points", "").base(format!("d{COMMONER_DIE} average"), COMMONER_HIT_POINTS).add("CON", modifier(constitution) as f64);
                if e.value() < 1.0 { e.clamp("at least 1", 1.0, f64::INFINITY) } else { e }
            }
        };
        let armour = person.gear.iter().map(|(_, item)| (item.name.clone(), item.profile.stats.armour)).filter(|(_, a)| *a != 0.0);
        FolkStats { class, strength, dexterity, constitution, hit_points, armour_class: armour_class(dexterity, armour) }
    }

    pub fn max_hit_points(&self) -> i32 {
        self.hit_points.value().round() as i32
    }

    pub fn armour_class_value(&self) -> i32 {
        self.armour_class.value().round() as i32
    }
}

/// What `person` holds in their main hand, if anything.
pub fn main_hand(person: &Person) -> Option<&fb_items::Item> {
    person.item(Slot::Mainhand)
}

/// Gives each folk their hit points and Armor Class ([`FolkStats`]) once they stand in the
/// level; one brought back from the record keeps the hit points it was left with.
pub fn arm_folk(mut commands: Commands, new: Query<(Entity, &Denizen, Has<Health>), Without<ArmourClass>>) {
    for (entity, denizen, has_health) in &new {
        let stats = FolkStats::of(&denizen.person);
        let mut entity = commands.entity(entity);
        entity.insert(ArmourClass(stats.armour_class_value()));
        if !has_health {
            entity.insert(Health::full(stats.max_hit_points()));
        }
    }
}

/// Applies each blow that lands on a folk: hit points down, a flash and a push, a thud; at 0 a
/// body, which stops its routine and blocks nobody.
#[allow(clippy::too_many_arguments)]
pub fn take_folk_blows(
    mut commands: Commands,
    mut struck: MessageReader<Struck>,
    tuning: Res<Tuning>,
    settings: Option<Res<GameSettings>>,
    sounds: Option<Res<crate::combat::feedback::CreatureSounds>>,
    mut toasts: Toasts,
    flinches: Flinches,
    mut folk: Query<(&Denizen, &Transform, &mut Health), Without<Corpse>>,
) {
    let game = settings.as_deref().copied().unwrap_or_default();
    for blow in struck.read() {
        let Ok((denizen, at, mut health)) = folk.get_mut(blow.target) else { continue };
        let fell = health.hurt(blow.damage);
        let away = (at.translation.xz() - blow.from).normalize_or_zero();
        let push = away * knockback_for(fb_creatures::Size::Medium) * tuning.combat.knockback_scale;
        commands.entity(blow.target).insert(flinches.of(blow.target, push, &tuning.combat));
        if let Some(sounds) = &sounds {
            crate::audio::play_at(&mut commands, "folk struck", sounds.thud.clone(), at.translation, &game, &tuning);
        }
        info!("{} takes {}: {} of {} hit points", denizen.person.name, blow.damage, health.hp, health.max);
        if fell {
            commands.entity(blow.target).insert(Corpse { since: 0.0, fell: *at }).remove::<(BlocksTheWay, Routine)>();
            toasts.add(&format!("{} is slain", denizen.person.name));
            info!("{} falls", denizen.person.name);
        }
    }
}

/// Lays each fallen folk on their back once the flash's hold is over ([`collapse_share`]).
pub fn collapse_folk(time: Res<Time>, tuning: Res<Tuning>, mut bodies: Query<(&Denizen, &mut Corpse, &mut Transform)>) {
    let hold = tuning.combat.flash_hold;
    for (denizen, mut corpse, mut transform) in &mut bodies {
        corpse.since += time.delta_secs();
        if corpse.since <= hold {
            continue;
        }
        let laid = laid_back(denizen.max.y, &corpse.fell, collapse_share(corpse.since, hold));
        if *transform != laid {
            *transform = laid;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::combat::{CombatPlugin, RunDice, Weapon, swing::Swing};
    use crate::inventory::Inventory;
    use crate::play::{CurrentLevel, Player};
    use bevy::time::TimeUpdateStrategy;
    use procgen_rng::Mulberry32;

    /// Seed 1207's level with its first folk standing at their spot as a stand-in (no figure),
    /// and a delver holding a sword `away` world units west of them, facing them, with the whole
    /// of combat and the folk's fighting but no drawing. Stepped 1/60 s a frame.
    pub(crate) fn folk_fight(away: f32) -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin, bevy::state::app::StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .insert_state(crate::launch::AppState::Delving)
            .init_resource::<crate::keymap::Keymap>()
            .init_resource::<crate::character::Character>()
            .init_resource::<crate::play::ScreenOpen>()
            .init_resource::<crate::play::Obstacles>()
            .init_resource::<crate::play::BlockedCells>()
            .init_resource::<crate::inventory::Toast>()
            .add_plugins((CombatPlugin, FolkCombatPlugin))
            .insert_resource(RunDice(Mulberry32::new(9)));
        let mut inventory = Inventory::default();
        let sword = inventory.add(fb_items::roll_item("Sword", 7).unwrap());
        inventory.wear(sword, Slot::Mainhand).unwrap();
        app.insert_resource(inventory);
        let level = fb_level::generate(1207, 1);
        let spot = level.folk[0];
        app.insert_resource(CurrentLevel(level));
        let denizen = Denizen { person: Person::of(&spot), radius: 3.0, min: Vec3::new(-2.0, 0.0, -2.0), max: Vec3::new(2.0, 17.0, 2.0) };
        let at = crate::folk::stand_at(&spot);
        let folk = app.world_mut().spawn((denizen, BlocksTheWay(fb_level::FloorShape::Round(3.0)), at)).id();
        let from = at.translation - Vec3::X * away;
        let player = app.world_mut().spawn((Player, Transform::from_translation(from).looking_at(at.translation, Vec3::Y))).id();
        app.update();
        (app, player, folk)
    }

    /// Swings the delver's weapon once, for a swing's time.
    pub(crate) fn swing_once(app: &mut App, player: Entity) {
        let weapon = Weapon::in_hand(app.world().resource::<Inventory>(), app.world().resource::<crate::character::Character>());
        app.world_mut().entity_mut(player).insert(Swing::new(weapon));
        for _ in 0..40 {
            app.update();
        }
    }

    #[test]
    fn a_swing_in_reach_hits_a_folk_and_lowers_its_hit_points() {
        let (mut app, player, folk) = folk_fight(9.0);
        let full = app.world().get::<Health>(folk).copied().expect("folk have hit points");
        assert_eq!(full.hp, FolkStats::of(&app.world().get::<Denizen>(folk).unwrap().person).max_hit_points());
        assert!(app.world().get::<ArmourClass>(folk).is_some());
        // Against an Armor Class anything but a natural 1 beats, a few swings must land.
        app.world_mut().entity_mut(folk).insert((ArmourClass(-10), Health::full(500)));
        for _ in 0..3 {
            swing_once(&mut app, player);
        }
        let now = app.world().get::<Health>(folk).unwrap().hp;
        assert!(now < 500, "no blow landed");
        // Out of reach, nothing lands.
        let (mut app, player, folk) = folk_fight(40.0);
        app.world_mut().entity_mut(folk).insert((ArmourClass(-10), Health::full(500)));
        swing_once(&mut app, player);
        assert_eq!(app.world().get::<Health>(folk).unwrap().hp, 500);
    }

    /// How many of `swings` swings land on a folk of Armor Class `ac`.
    fn landed(ac: i32, swings: usize) -> usize {
        let (mut app, player, folk) = folk_fight(9.0);
        app.world_mut().entity_mut(folk).insert((ArmourClass(ac), Health::full(100_000)));
        let stood = *app.world().get::<Transform>(folk).unwrap();
        let mut hits = 0;
        for _ in 0..swings {
            // Back where it stood, however far the last blow knocked it.
            app.world_mut().entity_mut(folk).insert(stood);
            let before = app.world().get::<Health>(folk).unwrap().hp;
            swing_once(&mut app, player);
            if app.world().get::<Health>(folk).unwrap().hp < before {
                hits += 1;
            }
        }
        hits
    }

    #[test]
    fn armour_lowers_the_chance_to_hit() {
        let (light, heavy) = (landed(8, 60), landed(19, 60));
        assert!(light > heavy + 15, "AC 8: {light} of 60, AC 19: {heavy} of 60");
        assert!(heavy > 0, "a natural 20 still hits");
    }

    #[test]
    fn at_0_hit_points_a_folk_dies_and_falls_where_it_stood() {
        let (mut app, _, folk) = folk_fight(9.0);
        let stood = *app.world().get::<Transform>(folk).unwrap();
        app.world_mut().write_message(Struck { target: folk, damage: 1000, from: stood.translation.xz() - Vec2::X * 9.0, bludgeoning: false });
        for _ in 0..90 {
            app.update();
        }
        assert!(app.world().get::<Health>(folk).unwrap().is_down());
        assert!(app.world().get::<Corpse>(folk).is_some(), "a body");
        assert!(app.world().get::<BlocksTheWay>(folk).is_none(), "a body blocks nobody");
        let down = app.world().get::<Transform>(folk).unwrap();
        assert!((down.rotation * Vec3::Y).y.abs() < 0.2, "lying down");
        assert!(down.translation.xz().distance(stood.translation.xz()) < 4.0, "where it stood");
        assert!(app.world().resource::<crate::inventory::Toast>().text.ends_with("is slain"));
    }

    fn people() -> Vec<Person> {
        fb_level::generate(1207, 1).folk.iter().map(Person::of).collect()
    }

    #[test]
    fn folk_hit_points_follow_their_class_and_constitution() {
        for person in people() {
            let stats = FolkStats::of(&person);
            let con = modifier(stats.constitution);
            let expected = match stats.class {
                Some(class) => (class.hit_die() as i32 + con).max(1),
                None => (4 + con).max(1),
            };
            assert_eq!(stats.max_hit_points(), expected, "{} ({})", person.name, person.outfit);
            assert_eq!(FolkStats::of(&person), stats, "a pure function of the spot");
        }
    }

    #[test]
    fn worn_armour_raises_a_folks_armour_class() {
        let person = people().remove(0);
        let bare = FolkStats::of(&Person { gear: Vec::new(), ..person.clone() });
        assert_eq!(bare.armour_class_value(), 10 + modifier(bare.dexterity));
        let mail = fb_items::roll_item("Mail", 7).unwrap();
        let shield = fb_items::roll_item("Shield", 7).unwrap();
        let armour = (mail.profile.stats.armour + shield.profile.stats.armour).round() as i32;
        let kitted = FolkStats::of(&Person { gear: vec![(Slot::Body, mail), (Slot::Offhand, shield)], ..person });
        assert!(armour > 0);
        assert_eq!(kitted.armour_class_value(), bare.armour_class_value() + armour);
    }
}
