//! Skeletons in the game: the bones of the dead, risen in what they died in and fighting with
//! what they hold.
//!
//! - A skeleton is drawn with its fitted gear on its bones ([`fb_creatures::SkeletonGear`], the
//!   `--skeleton` view's own), each worn piece a child of the bone it rides ([`WornGear`]), and
//!   posed with its stance so a raised shield stays up and a weapon stays in the fist.
//! - Everything about it that does not change is worked out once, at spawn, into its
//!   [`SkeletonKit`] (kept on its [`Critter`]): the bones' measures, its height, the weapon's
//!   reach, damage and swing, its Armor Class and the shield's chance to turn a blow. Nothing
//!   rebuilds a body per frame.
//! - Its weapon sets what it does. A blow reaches an arm to the wrist and the weapon's length
//!   beyond it (as the delver's does, `combat::Weapon::in_hand`), so a spear outreaches a
//!   dagger; it rolls the weapon's dice and bonus; a heavy weapon is swung slower than a light
//!   one. Unarmed, it claws for 1d3 at an arm's length. A bow is not shot yet (ranged attacks
//!   are not modelled for any creature): held, it is swung as an improvised 1d4 club, as the
//!   delver's is (SRD 5.1 p. 65).
//! - Its Armor Class is the delver's sum ([`fb_character::explain::armour_class`]) with the
//!   stat block's Dexterity of 14, over what it wears: a worn helm, body armour, gloves, boots
//!   and shield raise it. With nothing worn it keeps the stat block's 13 (the SRD's "armor
//!   scraps"). The delver's swings roll against it (`crate::combat::arm_creatures`), and its own
//!   blows add its weapon's `ToHit` to the stat block's +4.
//! - A shield turns some blows from the front ([`SkeletonKit::blocks`]): the better the shield,
//!   the more.
//! - Bludgeoning hurts it double and poison does nothing (the stat block's vulnerability and
//!   immunity; [`bludgeoning_damage`]).
//! - Struck down it collapses onto its back ([`fb_creatures::pose::skeleton_fallen`]) and, as it
//!   falls, every item it wore and held is set down around it as an ordinary item on the floor
//!   ([`shed_gear`]), once only.
//! - A skeleton past the fog's end is not drawn ([`cull_far`]), and the bones'
//!   thin parts (fingers and toes) are drawn with fewer facets, so a hall of them holds the frame
//!   rate, and they are rolled and fitted side by side when a level is laid out.
//! - Its own sounds ([`BoneSounds`], `tools/synth_bones.py`): bones clatter at each step, rattle
//!   now and then standing about, clack when struck, clash on a shield and fall in a heap.

use bevy::prelude::*;
use fb_creatures::skeleton::SkeletonDims;
use fb_creatures::{DamageType, SkeletonGear, SkeletonState, Species};
use fb_creatures::pose::ArmStance;
use fb_items::outfit::OutfitSet;
use fb_items::{Item, Slot};
use procgen_rng::Mulberry32;

use super::{Bones, Critter};
use crate::combat::feedback::Corpse;
use crate::inventory::underfoot::{FloorChanges, set_down_around};
use crate::inventory::Inventory;
use crate::level::draw_end;
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Obstacles, Player};
use crate::player::blocked_by;
use crate::settings::GameSettings;
use crate::tuning::Tuning;

/// The skeleton's SRD Dexterity score (SRD 5.1 p. 346), for its Armor Class over what it wears.
const DEXTERITY: i32 = 14;
/// A bony claw's damage when it holds no weapon. Our own.
const CLAW: (u32, u32) = (1, 3);
/// An improvised weapon's damage (SRD 5.1 p. 65), for a bow swung as a club.
const IMPROVISED: (u32, u32) = (1, 4);
/// An arm to the wrist and to the fingertips, as shares of stature (D. A. Winter, 4th ed., fig.
/// 4.1), as `combat::Weapon` takes them.
const ARM_TO_WRIST: f32 = 0.332;
const ARM_TO_FINGERTIPS: f32 = 0.44;
/// The least time a skeleton's blow takes to land after its swing starts, seconds: the dagger's
/// 0.9 s swing lands in 0.56 s. Our own floor for a wind-up the eye can read: more than twice
/// the mean simple reaction time to a light, about 0.2 s (R. J. Kosinski, "A Literature Review
/// on Reaction Time", Clemson University, 2013).
pub const MIN_WIND_UP: f32 = 0.5;
/// How far through its swing the blow lands (`fb_creatures::pose::skeleton_armed` throws the arm
/// down at 0.62).
pub const LANDS: f32 = 0.62;
/// A shield's chance to turn a blow, before its armour: and each point of armour adds. The
/// most it ever turns. Our own.
const BLOCK_BASE: f32 = 0.2;
const BLOCK_PER_ARMOUR: f32 = 0.08;
const BLOCK_MOST: f32 = 0.65;
/// How wide in front of it a shield covers, radians each way of where it faces (60 degrees).
const BLOCK_ARC: f32 = std::f32::consts::FRAC_PI_3;
/// How near the player a skeleton must be to be heard rattling idly, world units (15 m).
const RATTLE_RANGE: f32 = 150.0;

/// What a skeleton is, worked out once when it appears.
#[derive(Clone, Debug)]
pub struct SkeletonKit {
    /// The bones' measures and the skeleton's height, crown to soles, world units, as rolled.
    pub dims: SkeletonDims,
    pub height: f32,
    /// How far it covers the floor at rest, world units, as rolled ([`SkeletonState::radius`]).
    pub radius: f32,
    /// The stance of the arms that hold something.
    pub stance: [Option<ArmStance>; 2],
    /// What it wears and holds, as items.
    pub outfit: OutfitSet,
    /// How far from its centre its blow reaches, world units, as rolled.
    pub reach: f32,
    /// The weapon's dice (count and sides), added bonus, and how long a swing takes, seconds.
    pub dice: (u32, u32),
    pub bonus: i32,
    pub swing_time: f32,
    /// Its weapon's `ToHit`, rounded (0 unarmed or with something improvised), added to the stat
    /// block's attack bonus.
    pub to_hit: i32,
    pub armour_class: i32,
    /// Its shield's chance to turn a blow from the front, 0 with none.
    pub block: f32,
    /// What it swings is a mace, a club or a staff, or it has nothing but its bones.
    pub bludgeons: bool,
}

impl SkeletonKit {
    /// A skeleton with nothing worn or held.
    pub fn bare(state: &SkeletonState) -> SkeletonKit {
        SkeletonKit::new(state, OutfitSet { worn: Vec::new(), goods: Vec::new(), ground: Vec::new() }, [None, None])
    }

    /// A skeleton in the gear it rolled.
    pub fn armed(state: &SkeletonState, gear: &SkeletonGear) -> SkeletonKit {
        SkeletonKit::new(state, gear.outfit.clone(), gear.stance)
    }

    /// A skeleton in `outfit`, its arms in `stance`.
    pub fn new(state: &SkeletonState, outfit: OutfitSet, stance: [Option<ArmStance>; 2]) -> SkeletonKit {
        use armory::Family;
        let dims = state.dims();
        let height = dims.body.height;
        let radius = state.radius();
        let weapon = outfit.item(Slot::Mainhand);
        let family = weapon.map(|w| w.shape.family());
        let melee = matches!(family, Some(Family::Sword | Family::Mace | Family::Axe | Family::Club | Family::Spear | Family::Dagger | Family::Staff));
        let (dice, bonus) = match weapon {
            None => (CLAW, 0),
            Some(w) if melee => (w.profile.stats.damage.map_or(IMPROVISED, |d| (d.count as u32, d.sides as u32)), w.profile.stats.damage_bonus.round() as i32),
            Some(_) => (IMPROVISED, 0),
        };
        let length = |w: &Item| w.shape.build().bounds().map_or(0.0, |(lo, hi)| (0..3).map(|i| hi[i] - lo[i]).fold(0.0, f32::max));
        let reach = match weapon {
            Some(w) if melee => ARM_TO_WRIST * height + length(w),
            _ => ARM_TO_FINGERTIPS * height,
        };
        let swing_time = crate::combat::swing_time(family);
        let to_hit = weapon.filter(|_| melee).map_or(0, |w| w.profile.stats.to_hit.round() as i32);
        let armour: Vec<(String, f32)> = outfit.worn.iter().map(|(_, item)| (item.name.clone(), item.profile.stats.armour)).filter(|(_, a)| *a != 0.0).collect();
        let armour_class = if armour.is_empty() {
            Species::Skeleton.stat_block().armour_class
        } else {
            fb_character::explain::armour_class(DEXTERITY, armour).value().round() as i32
        };
        let block = outfit.item(Slot::Offhand).map_or(0.0, |shield| (BLOCK_BASE + BLOCK_PER_ARMOUR * shield.profile.stats.armour).clamp(0.0, BLOCK_MOST));
        let bludgeons = matches!(family, None | Some(Family::Mace | Family::Club | Family::Staff | Family::Bow));
        SkeletonKit { dims, height, radius, stance, outfit, reach, dice, bonus, swing_time, to_hit, armour_class, block, bludgeons }
    }

    /// What its blow is called in the log, by what it swings: a sword slashes, a dagger or a
    /// spear stabs, an axe hacks, a mace, a club, a staff or a bow swung as a club strikes, and
    /// bare bones claw. Never a bite.
    pub fn blow_verb(&self) -> &'static str {
        use armory::Family;
        match self.outfit.item(Slot::Mainhand).map(|w| w.shape.family()) {
            None => "claws",
            Some(Family::Sword) => "slashes",
            Some(Family::Dagger | Family::Spear) => "stabs",
            Some(Family::Axe) => "hacks",
            Some(_) => "strikes",
        }
    }

    /// Seconds from the start of its swing, when its rattle sounds and its arm goes back, to
    /// the blow landing: the wind-up the delver has to see it coming.
    pub fn wind_up(&self) -> f32 {
        LANDS * self.swing_time
    }

    /// One blow's damage: the weapon's dice and bonus, at least 1.
    pub fn roll_damage(&self, rng: &mut Mulberry32) -> u32 {
        let (count, sides) = self.dice;
        let dice: i32 = (0..count).map(|_| rng.int(1, sides.max(1)) as i32).sum();
        (dice + self.bonus).max(1) as u32
    }

    /// Whether its shield turns a blow that came from `from` (world `(x, z)`), the skeleton
    /// standing at `at` facing `facing` (radians about +Y, +Z at 0), given `roll`, 0 to 1.
    pub fn blocks(&self, at: Vec2, facing: f32, from: Vec2, roll: f32) -> bool {
        if self.block <= 0.0 {
            return false;
        }
        let to = from - at;
        if to.length_squared() < 1e-6 {
            return false;
        }
        let forward = Vec2::new(facing.sin(), facing.cos());
        forward.angle_to(to).abs() <= BLOCK_ARC && roll < self.block
    }
}

/// On an entity that is a piece of a skeleton's gear drawn on its bones, so it can be taken off
/// when the gear is shed.
#[derive(Component, Debug)]
pub struct WornGear;

pub use crate::combat::feedback::Looted;

/// Whether a blow of `damage` that is `bludgeoning` hurts a skeleton for more (double: the stat
/// block's vulnerability) or less. Poison does nothing to it (immunity), though no blow of the
/// delver's is poison yet.
pub fn damage_taken(damage: u32, kind: DamageType) -> u32 {
    let block = Species::Skeleton.stat_block();
    if block.immunities.contains(&kind) {
        0
    } else if block.vulnerabilities.contains(&kind) {
        damage * 2
    } else {
        damage
    }
}

/// The damage type of a blow struck with what the delver holds.
pub fn damage_type(bludgeoning: bool) -> DamageType {
    if bludgeoning { DamageType::Bludgeoning } else { DamageType::Piercing }
}

/// The bone clatter of a skeleton, loaded once.
#[derive(Resource, Clone, Debug, Default)]
pub struct BoneSounds {
    pub steps: [Handle<AudioSource>; 3],
    pub rattle: Handle<AudioSource>,
    pub hit: Handle<AudioSource>,
    pub block: Handle<AudioSource>,
    pub collapse: Handle<AudioSource>,
}

fn load_bone_sounds(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    if let Some(assets) = assets {
        commands.insert_resource(BoneSounds {
            steps: [assets.load("audio/bone_step_1.ogg"), assets.load("audio/bone_step_2.ogg"), assets.load("audio/bone_step_3.ogg")],
            rattle: assets.load("audio/bone_rattle.ogg"),
            hit: assets.load("audio/bone_hit.ogg"),
            block: assets.load("audio/bone_block.ogg"),
            collapse: assets.load("audio/bone_collapse.ogg"),
        });
    }
}

pub use crate::audio::play_at;

/// Seconds until a skeleton standing about rattles again.
#[derive(Component, Clone, Debug)]
pub struct Rattle {
    pub left: f32,
    pub rng: Mulberry32,
}

impl Rattle {
    pub fn new(seed: u32) -> Rattle {
        let mut rng = Mulberry32::new(seed ^ 0x8A77_1E00);
        Rattle { left: rng.range_f32(2.0, 8.0), rng }
    }
}

/// Hides each skeleton out of the draw distance ([`draw_end`], the fog's end and a margin) of the delver, and shows it again on coming back.
fn cull_far(tuning: Res<Tuning>, players: Query<&Transform, With<Player>>, mut skeletons: Query<(&Critter, &Transform, &mut Visibility), Without<Player>>) {
    let Some(player) = players.iter().next().map(|t| t.translation) else { return };
    for (critter, at, mut visibility) in &mut skeletons {
        if !critter.spot.species.is_skeleton() {
            continue;
        }
        let wanted = if at.translation.distance(player) > draw_end(&tuning.light) { Visibility::Hidden } else { Visibility::Inherited };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

/// Now and then a living skeleton standing near enough rattles its bones.
fn rattle_idle(
    mut commands: Commands,
    time: Res<Time>,
    tuning: Res<Tuning>,
    settings: Option<Res<GameSettings>>,
    sounds: Option<Res<BoneSounds>>,
    players: Query<&Transform, With<Player>>,
    mut skeletons: Query<(&Transform, &mut Rattle), (With<Critter>, Without<Corpse>, Without<Player>)>,
) {
    let Some(sounds) = sounds else { return };
    let Some(player) = players.iter().next().map(|t| t.translation) else { return };
    let game = settings.as_deref().copied().unwrap_or_default();
    for (at, mut rattle) in &mut skeletons {
        rattle.left -= time.delta_secs();
        if rattle.left > 0.0 {
            continue;
        }
        rattle.left = rattle.rng.range_f32(6.0, 14.0);
        if at.translation.distance(player) <= RATTLE_RANGE {
            play_at(&mut commands, "bones rattle", sounds.rattle.clone(), at.translation, &game, &tuning);
        }
    }
}

/// Sets every item a fallen skeleton wore and held down around where it fell, as the items
/// they are, once, as it starts to collapse: they are taken off its bones and put into the
/// inventory as loose items and set on the floor ([`set_down_around`]), which the floor
/// machinery spawns and records like any dropped item.
///
/// It fires the frame after the skeleton falls, so a skeleton struck down just before the
/// delver leaves has already shed into the floor the record captures (the floor changes it makes
/// are applied before [`crate::record::record_level_left`] reads the floor). It marks the skeleton
/// [`Looted`], and a body brought back from the record is spawned [`Looted`] and bare, so its
/// gear is never set down twice.
#[allow(clippy::too_many_arguments)]
pub fn shed_gear(
    mut commands: Commands,
    tuning: Res<Tuning>,
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Option<Res<Obstacles>>,
    settings: Option<Res<GameSettings>>,
    sounds: Option<Res<BoneSounds>>,
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    bodies: Query<(Entity, &Critter, &Transform), (With<Corpse>, Without<Looted>)>,
    gear: Query<(Entity, &ChildOf), With<WornGear>>,
    bones: Query<&Bones>,
) {
    let obstacles = obstacles.as_deref().cloned().unwrap_or_default();
    let blocked = blocked_by(&level.0, &blocked_cells);
    let game = settings.as_deref().copied().unwrap_or_default();
    for (entity, critter, at) in &bodies {
        let Some(kit) = critter.kit.as_ref().filter(|_| critter.spot.species.is_skeleton()) else { continue };
        commands.entity(entity).insert(Looted);
        if let Some(sounds) = &sounds {
            play_at(&mut commands, "bones fall", sounds.collapse.clone(), at.translation, &game, &tuning);
        }
        // Off the bones.
        let own: Vec<Entity> = bones.get(entity).map(|b| b.0.clone()).unwrap_or_default();
        for (piece, parent) in &gear {
            if own.contains(&parent.parent()) {
                commands.entity(piece).despawn();
            }
        }
        // Onto the floor, round the place it fell.
        let items = kit.outfit.worn.iter().map(|(_, item)| item);
        set_down_around(at.translation.xz(), items, critter.spot.seed, &mut inventory, &mut changes, &blocked, &obstacles.0, tuning.view.floor_item_scale);
    }
}

pub struct SkeletonsPlugin;

impl Plugin for SkeletonsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tuning>()
            .init_resource::<FloorChanges>()
            .add_systems(Startup, load_bone_sounds)
            .add_systems(Update, (rattle_idle, cull_far, shed_gear.after(crate::combat::feedback::collapse).before(crate::inventory::underfoot::apply_floor_changes)).in_set(DelveSystems));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kit_in(seed: u32, gear: &[(Slot, &str)]) -> SkeletonKit {
        let worn = gear
            .iter()
            .map(|&(slot, base)| {
                let item = fb_items::roll_item(base, 40 + seed).unwrap();
                assert!(item.fits(slot), "{base} in {slot:?}");
                (slot, item)
            })
            .collect();
        SkeletonKit::new(&SkeletonState::roll(seed), OutfitSet { worn, goods: Vec::new(), ground: Vec::new() }, [None, None])
    }

    #[test]
    fn a_spear_outreaches_a_dagger_and_a_dagger_a_fist() {
        for seed in 0..12 {
            let spear = kit_in(seed, &[(Slot::Mainhand, "Spear")]);
            let sword = kit_in(seed, &[(Slot::Mainhand, "Sword")]);
            let dagger = kit_in(seed, &[(Slot::Mainhand, "Dagger")]);
            let bare = kit_in(seed, &[]);
            assert!(spear.reach > dagger.reach, "{seed}: spear {} dagger {}", spear.reach, dagger.reach);
            assert!(sword.reach > dagger.reach, "{seed}: sword {} dagger {}", sword.reach, dagger.reach);
            assert!(dagger.reach > bare.reach, "{seed}: dagger {} fist {}", dagger.reach, bare.reach);
        }
    }

    #[test]
    fn the_weapon_sets_the_damage_and_the_pace_of_the_swing() {
        let mut rng = Mulberry32::new(3);
        let bare = kit_in(1, &[]);
        assert_eq!(bare.dice, CLAW);
        assert!((0..200).all(|_| (1..=3).contains(&bare.roll_damage(&mut rng))), "a claw does 1d3");
        let mace = kit_in(1, &[(Slot::Mainhand, "Mace")]);
        let dagger = kit_in(1, &[(Slot::Mainhand, "Dagger")]);
        assert!(mace.swing_time > dagger.swing_time, "a mace is swung slower than a dagger");
        let stats = fb_items::roll_item("Mace", 41).unwrap().profile.stats;
        let dice = stats.damage.unwrap();
        assert_eq!(mace.dice, (dice.count as u32, dice.sides as u32));
        let (lo, hi) = (dice.count as i32 + mace.bonus, (dice.count * dice.sides) as i32 + mace.bonus);
        assert!((0..300).all(|_| (lo.max(1) as u32..=hi.max(1) as u32).contains(&mace.roll_damage(&mut rng))));
        assert!(mace.bludgeons && bare.bludgeons && !dagger.bludgeons);
    }

    #[test]
    fn a_skeleton_names_its_blow_by_its_weapon_and_never_bites() {
        assert_eq!(kit_in(1, &[(Slot::Mainhand, "Sword")]).blow_verb(), "slashes");
        assert_eq!(kit_in(1, &[(Slot::Mainhand, "Mace")]).blow_verb(), "strikes");
        assert_eq!(kit_in(1, &[(Slot::Mainhand, "Club")]).blow_verb(), "strikes");
        assert_eq!(kit_in(1, &[(Slot::Mainhand, "Dagger")]).blow_verb(), "stabs");
        assert_eq!(kit_in(1, &[(Slot::Mainhand, "Spear")]).blow_verb(), "stabs");
        assert_eq!(kit_in(1, &[(Slot::Mainhand, "Hand axe")]).blow_verb(), "hacks");
        assert_eq!(kit_in(1, &[]).blow_verb(), "claws");
    }

    #[test]
    fn every_skeleton_winds_up_for_at_least_half_a_second() {
        for base in ["Dagger", "Sword", "Spear", "Hand axe", "Mace", "Club", "Bow"] {
            let kit = kit_in(2, &[(Slot::Mainhand, base)]);
            assert!(kit.wind_up() >= MIN_WIND_UP, "{base}: {}", kit.wind_up());
        }
        assert!(kit_in(2, &[]).wind_up() >= MIN_WIND_UP);
    }

    #[test]
    fn worn_armour_raises_armour_class_over_the_stat_blocks_13() {
        let bare = kit_in(2, &[(Slot::Mainhand, "Sword")]);
        assert_eq!(bare.armour_class, 13, "armor scraps");
        let helm = kit_in(2, &[(Slot::Head, "Helm"), (Slot::Mainhand, "Sword")]);
        let kitted = kit_in(2, &[(Slot::Head, "Helm"), (Slot::Body, "Mail"), (Slot::Hands, "Gauntlets"), (Slot::Feet, "Boots"), (Slot::Mainhand, "Sword"), (Slot::Offhand, "Shield")]);
        assert!(helm.armour_class != 13, "a helm changes it: {}", helm.armour_class);
        assert!(kitted.armour_class > helm.armour_class, "{} vs {}", kitted.armour_class, helm.armour_class);
        assert!(kitted.armour_class > bare.armour_class + 2, "{} vs {}", kitted.armour_class, bare.armour_class);
        // The delver's own sum, 10 + Dex modifier + each item's armour.
        let armour: f32 = kitted.outfit.worn.iter().map(|(_, i)| i.profile.stats.armour).sum();
        assert_eq!(kitted.armour_class, 12 + armour.round() as i32);
    }

    #[test]
    fn a_shield_turns_blows_from_the_front_only() {
        let shielded = kit_in(4, &[(Slot::Mainhand, "Sword"), (Slot::Offhand, "Shield")]);
        let open = kit_in(4, &[(Slot::Mainhand, "Sword")]);
        assert!(shielded.block > 0.0 && shielded.block <= BLOCK_MOST && open.block == 0.0);
        // Facing +Z (yaw 0) at the origin.
        let (front, behind) = (Vec2::new(0.0, 10.0), Vec2::new(0.0, -10.0));
        assert!(shielded.blocks(Vec2::ZERO, 0.0, front, 0.0), "a low roll is turned");
        assert!(!shielded.blocks(Vec2::ZERO, 0.0, front, 0.99), "a high roll gets through");
        assert!(!shielded.blocks(Vec2::ZERO, 0.0, behind, 0.0), "not from behind");
        assert!(!open.blocks(Vec2::ZERO, 0.0, front, 0.0), "no shield, no block");
        // Turned east, the front is +X.
        assert!(shielded.blocks(Vec2::ZERO, std::f32::consts::FRAC_PI_2, Vec2::new(10.0, 0.0), 0.0));
    }

    #[test]
    fn bones_take_double_from_bludgeoning_and_nothing_from_poison() {
        assert_eq!(damage_taken(5, DamageType::Bludgeoning), 10);
        assert_eq!(damage_taken(5, DamageType::Piercing), 5);
        assert_eq!(damage_taken(5, DamageType::Poison), 0);
        assert_eq!(damage_taken(5, damage_type(true)), 10);
    }

    #[test]
    fn a_kit_is_measured_once_and_matches_the_skeleton() {
        let state = SkeletonState::roll(9);
        let kit = SkeletonKit::bare(&state);
        assert_eq!(kit.height, state.height());
        assert_eq!(kit.radius, state.radius());
        let critter = Critter::new(fb_level::CreatureSpot { x: 0.0, y: 0.0, yaw: 0.0, species: Species::Skeleton, seed: 9, pack: 0 }, fb_creatures::Creature::Skeleton(state));
        assert!(critter.kit.is_some(), "a skeleton always has its kit");
        assert_eq!(critter.radius, kit.radius);
    }
}

/// How a fresh delver fares against the guards of the stone halls: the numbers #354's specs
/// are measured by, from real delvers (`Character::roll`) and the skeletons real levels place
/// with their gear for the depth.
#[cfg(test)]
mod against_a_fresh_delver {
    use super::*;
    use crate::character::{Character, modifier};

    /// The fresh delvers of seeds 1 to 399.
    fn delvers() -> Vec<Character> {
        (1..400).map(Character::roll).collect()
    }

    /// The guards of the first `levels` level seeds at `depth`, and how many each level holds.
    fn guards(depth: u32, levels: u32) -> (Vec<SkeletonKit>, Vec<usize>) {
        let mut kits = Vec::new();
        let mut counts = Vec::new();
        for seed in 1..=levels {
            let spots: Vec<_> = fb_level::generate(seed, depth).creatures.into_iter().filter(|c| c.species.is_skeleton()).collect();
            counts.push(spots.len());
            kits.extend(spots.iter().map(|s| SkeletonKit::new(&SkeletonState::roll(s.seed), fb_items::undead::skeleton_outfit(s.seed, fb_items::Odds::kit(depth)), [None, None])));
        }
        (kits, counts)
    }

    /// The chance a d20 plus `bonus` meets `ac`, a natural 1 always missing and a 20 hitting.
    fn chance(bonus: i32, ac: i32) -> f64 {
        (21 - (ac - bonus)).clamp(1, 19) as f64 / 20.0
    }

    /// The mean blows that land before the delver is down: each delver against a guard in turn,
    /// twenty fights each.
    fn blows_to_down(delvers: &[Character], kits: &[SkeletonKit]) -> f64 {
        let mut rng = Mulberry32::new(5);
        let (mut blows, mut fights) = (0u32, 0u32);
        for (i, delver) in delvers.iter().enumerate() {
            let kit = &kits[i % kits.len()];
            for _ in 0..20 {
                let mut left = crate::combat::first_level_hit_points(delver);
                while left > 0 {
                    left -= kit.roll_damage(&mut rng) as i32;
                    blows += 1;
                }
                fights += 1;
            }
        }
        blows as f64 / fights as f64
    }

    /// The delver's mean chance to hit a guard: the Strength modifier and the proficiency bonus,
    /// with a weapon of no ToHit (most of what the first level gives) against each guard's AC.
    fn delver_hits(delvers: &[Character], kits: &[SkeletonKit]) -> f64 {
        let all: f64 = delvers.iter().flat_map(|d| kits.iter().map(move |k| chance(modifier(d.strength()) + fb_character::combat::PROFICIENCY_BONUS, k.armour_class))).sum();
        all / (delvers.len() * kits.len()) as f64
    }

    #[test]
    fn a_fresh_delver_survives_three_blows_of_a_first_halls_guard_and_hits_half_the_time() {
        let delvers = delvers();
        let (kits, _) = guards(fb_items::undead::FIRST_HALLS, 8);
        let blows = blows_to_down(&delvers, &kits);
        assert!(blows >= 4.0, "down on blow {blows:.2} on average, so not three survived");
        let hits = delver_hits(&delvers, &kits);
        assert!(hits >= 0.5, "hits {:.0}%", hits * 100.0);
    }

    /// Prints the table of #354's numbers at depths 2 and 4.
    #[test]
    #[ignore = "a report: run with --ignored --nocapture"]
    fn report_the_stone_halls() {
        let delvers = delvers();
        let hp: Vec<i32> = delvers.iter().map(crate::combat::first_level_hit_points).collect();
        let mean = |v: &[i32]| v.iter().sum::<i32>() as f64 / v.len() as f64;
        let ac: Vec<i32> = delvers.iter().map(|c| 10 + modifier(c.dexterity())).collect();
        println!("delver: hit points {:.1} (least {}, most {}); bare AC {:.1}", mean(&hp), hp.iter().min().unwrap(), hp.iter().max().unwrap(), mean(&ac));
        for depth in [2, 4] {
            let (kits, counts) = guards(depth, 24);
            let n = kits.len() as f64;
            let mut acs: Vec<i32> = kits.iter().map(|k| k.armour_class).collect();
            acs.sort();
            let damage = kits.iter().map(|k| (k.dice.0 as f64 * (k.dice.1 as f64 + 1.0) / 2.0 + k.bonus as f64).max(1.0)).sum::<f64>() / n;
            let most = kits.iter().map(|k| (k.dice.0 * k.dice.1) as i32 + k.bonus).max().unwrap();
            let hits_delver = delvers.iter().enumerate().map(|(i, d)| chance(4 + kits[i % kits.len()].to_hit, 10 + modifier(d.dexterity()))).sum::<f64>() / delvers.len() as f64;
            println!(
                "depth {depth}: guards a level {:.1} ({}-{}); AC mean {:.1}, median {}, 90th {}; damage a blow {damage:.2} (most {most}); blows to down {:.2}; delver hits {:.0}%; guard hits {:.0}%",
                counts.iter().sum::<usize>() as f64 / counts.len() as f64,
                counts.iter().min().unwrap(),
                counts.iter().max().unwrap(),
                acs.iter().sum::<i32>() as f64 / n,
                acs[acs.len() / 2],
                acs[acs.len() * 9 / 10],
                blows_to_down(&delvers, &kits),
                delver_hits(&delvers, &kits) * 100.0,
                hits_delver * 100.0,
            );
        }
    }
}
