//! Eating and drinking from the inventory (#228). With the screen open, U eats or drinks the
//! food or drink under the mouse: the item is used up and its nutrition and water come off the
//! delver's hunger and thirst ([`fb_items::Provision`], `docs/items.md`). The inventory keeps no
//! counts, so a stack of bread is several items and one is eaten at a time.
//!
//! [`consume`] does the work and says what it did, so the healing draught adds its cure to it
//! (#235): 2d4 + 2 hit points from the run's dice, a green flash and a soft chime. The delver's needs are the [`DelverNeeds`] on the player; the game clock steps
//! them in #229.

use bevy::prelude::*;
use fb_items::{Consume, Provision};
use procgen_rng::Mulberry32;

use crate::character::DelverNeeds;
use crate::combat::{Health, RunDice};
use crate::settings::GameSettings;
use crate::tuning::Tuning;
use crate::ui::OK;
use crate::inventory::{Hovered, Inventory, ItemId, Toast};
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, Player, Screen, ScreenOpen};

pub struct EatPlugin;

impl Plugin for EatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HealGlow>()
            .add_systems(Startup, load_heal_sound)
            .add_systems(Update, consume_hovered.run_if(inventory_open).in_set(DelveSystems))
            .add_systems(Update, (spawn_heal_glow, fade_heal_glow).chain().in_set(DelveSystems));
    }
}

/// The base of the potion that heals.
pub const HEALING_DRAUGHT: &str = "Healing draught";
/// How long the green flash of a healing draught lasts, seconds. Our own.
pub const HEAL_FLASH_TIME: f32 = 0.33;
/// The green flash's strongest alpha. Our own.
const HEAL_FLASH_ALPHA: f32 = 0.22;

/// What a Healing draught restores: 2d4 + 2 hit points (SRD 5.1 "Potion of Healing"), rolled
/// with the run's dice, and never above the maximum. Returns the hit points restored.
pub fn drink_healing(health: &mut Health, dice: &mut Mulberry32) -> u32 {
    let roll = dice.int(1, 4) + dice.int(1, 4) + 2;
    health.heal(roll)
}

/// Seconds left of the green flash over the view.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct HealGlow(pub f32);

/// The full-view green flash.
#[derive(Component, Debug)]
pub struct HealFlash;

/// The chime a healing draught plays.
#[derive(Resource, Clone, Debug)]
pub struct HealSound(pub Handle<AudioSource>);

fn load_heal_sound(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    if let Some(assets) = assets {
        commands.insert_resource(HealSound(assets.load("audio/heal.ogg")));
    }
}

/// A soft green node over each new view, clear until a draught is drunk.
fn spawn_heal_glow(mut commands: Commands, new: Query<Entity, Added<crate::carry::ViewModelCamera>>) {
    for camera in &new {
        commands.spawn((
            Name::new("HealFlash"),
            HealFlash,
            DespawnOnExit(crate::launch::AppState::Delving),
            UiTargetCamera(camera),
            Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() },
            BackgroundColor(OK.with_alpha(0.0)),
            GlobalZIndex(50),
            Pickable::IGNORE,
        ));
    }
}

fn fade_heal_glow(time: Res<Time<Real>>, mut glow: ResMut<HealGlow>, mut flashes: Query<&mut BackgroundColor, With<HealFlash>>) {
    glow.0 = (glow.0 - time.delta_secs()).max(0.0);
    let alpha = HEAL_FLASH_ALPHA * (glow.0 / HEAL_FLASH_TIME).clamp(0.0, 1.0);
    for mut colour in &mut flashes {
        colour.0 = OK.with_alpha(alpha);
    }
}

fn inventory_open(open: Option<Res<ScreenOpen>>) -> bool {
    open.is_some_and(|o| o.is(Screen::Inventory))
}

/// What was eaten or drunk.
#[derive(Clone, Debug, PartialEq)]
pub struct Consumed {
    pub how: Consume,
    /// The item's own name, for the toast.
    pub name: String,
    pub provision: Provision,
    /// The item's base, so a rule keyed on it (the healing draught's cure) can follow.
    pub base: String,
}

/// Eats or drinks `id`: takes it out of the inventory, wherever it lies, and takes its
/// nutrition and water off `needs`. `None`, and nothing changed, if it is not food or drink.
pub fn consume(inventory: &mut Inventory, needs: &mut fb_character::Needs, id: ItemId) -> Option<Consumed> {
    let item = inventory.item(id)?;
    let (how, provision) = (item.consume()?, item.provision()?);
    let done = Consumed { how, name: item.title().to_string(), provision, base: item.base.clone() };
    inventory.forget(id);
    needs.consume(provision.nutrition, provision.water);
    Some(done)
}

/// "You eat the bread".
pub fn say(done: &Consumed) -> String {
    format!("You {} the {}", done.how.past(), done.name.to_lowercase())
}

/// U in the inventory: eats or drinks the item under the mouse.
fn consume_hovered(
    input: ActionInput,
    hovered: Res<Hovered>,
    mut inventory: ResMut<Inventory>,
    mut needs: Query<&mut DelverNeeds, With<Player>>,
    mut healths: Query<&mut Health, With<Player>>,
    dice: Option<ResMut<RunDice>>,
    mut glow: ResMut<HealGlow>,
    sound: Option<Res<HealSound>>,
    settings: Option<Res<GameSettings>>,
    tuning: Res<Tuning>,
    mut toast: ResMut<Toast>,
    mut commands: Commands,
    time: Option<Res<Time<Real>>>,
) {
    if !input.just_pressed(Action::Consume) {
        return;
    }
    let (Some((id, _)), Ok(mut needs)) = (hovered.0, needs.single_mut()) else { return };
    if let Some(done) = consume(&mut inventory, &mut needs.0, id) {
        let mut line = say(&done);
        if done.base == HEALING_DRAUGHT
            && let (Some(mut dice), Ok(mut health)) = (dice, healths.single_mut())
        {
            let restored = drink_healing(&mut health, &mut dice.0);
            line = format!("{line} (+{restored})");
            glow.0 = HEAL_FLASH_TIME;
            if let Some(sound) = sound {
                let game = settings.as_deref().copied().unwrap_or_default();
                commands.spawn((Name::new("heal"), AudioPlayer::new(sound.0.clone()), crate::audio::sound_effect(&game, &tuning), DespawnOnExit(crate::launch::AppState::Delving)));
            }
        }
        toast.say(line, time.map_or(0.0, |t| t.elapsed_secs()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Keymap;
    use crate::launch::AppState;
    use crate::tuning::Tuning;
    use bevy::input::ButtonInput;
    use bevy::state::app::StatesPlugin;

    fn item(base: &str) -> fb_items::Item {
        fb_items::roll_item(base, 5).unwrap()
    }

    fn hungry(hunger: f64) -> fb_character::Needs {
        let mut needs = fb_character::Needs::default();
        needs.hunger.value = hunger;
        needs
    }

    #[test]
    fn eating_a_loaf_while_hungry_leaves_a_little_hunger() {
        let mut inventory = Inventory::default();
        let loaf = inventory.add(item("Bread"));
        let mut needs = hungry(0.55);
        assert_eq!(needs.hunger.word(), "Hungry");
        let done = consume(&mut inventory, &mut needs, loaf).unwrap();
        assert_eq!(done.how, Consume::Eat);
        assert!((needs.hunger.value - 0.15).abs() < 1e-9, "{}", needs.hunger.value);
        assert!(inventory.item(loaf).is_none());
    }

    #[test]
    fn eating_while_fed_stays_at_zero() {
        let mut inventory = Inventory::default();
        let loaf = inventory.add(item("Bread"));
        let mut needs = fb_character::Needs::default();
        consume(&mut inventory, &mut needs, loaf).unwrap();
        assert_eq!(needs.hunger.value, 0.0);
        assert_eq!(needs.hunger.word(), "Fed");
    }

    #[test]
    fn drinking_a_tankard_removes_it_from_the_grid() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(item("Pack"));
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let tankard = inventory.add(item("Tankard"));
        inventory.stow(tankard).unwrap();
        assert_eq!(inventory.contents_count(pack), 1);
        let mut needs = fb_character::Needs::default();
        needs.thirst.value = 0.8;
        let done = consume(&mut inventory, &mut needs, tankard).unwrap();
        assert_eq!(done.how, Consume::Drink);
        assert!((needs.thirst.value - 0.3).abs() < 1e-9);
        assert_eq!(inventory.contents_count(pack), 0);
        assert!(inventory.item(tankard).is_none());
    }

    #[test]
    fn one_from_a_stack_leaves_the_rest() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(item("Pack"));
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let loaves: Vec<ItemId> = (0..3).map(|_| inventory.add(item("Bread"))).collect();
        for &loaf in &loaves {
            inventory.stow(loaf).unwrap();
        }
        let mut needs = hungry(0.9);
        consume(&mut inventory, &mut needs, loaves[1]).unwrap();
        assert_eq!(inventory.contents_count(pack), 2);
        assert!(inventory.item(loaves[0]).is_some() && inventory.item(loaves[2]).is_some());
    }

    #[test]
    fn a_torch_is_not_eaten_and_a_draught_quenches_nothing() {
        let mut inventory = Inventory::default();
        let torch = inventory.add(item("Torch"));
        let mut needs = hungry(0.5);
        assert!(consume(&mut inventory, &mut needs, torch).is_none());
        assert!(inventory.item(torch).is_some());
        let draught = inventory.add(item("Healing draught"));
        let done = consume(&mut inventory, &mut needs, draught).unwrap();
        assert_eq!((done.base.as_str(), done.how), ("Healing draught", Consume::Drink));
        assert_eq!(needs.hunger.value, 0.5);
        assert!(inventory.item(draught).is_none());
    }

    fn app() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Delving)
            .init_resource::<Tuning>()
            .init_resource::<Keymap>()
            .init_resource::<Inventory>()
            .init_resource::<Hovered>()
            .init_resource::<Toast>()
            .insert_resource(RunDice(Mulberry32::new(11)))
            .insert_resource(HealSound(Handle::default()))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(ScreenOpen(Some(Screen::Inventory)))
            .add_plugins(EatPlugin);
        crate::play::run_delve_systems_while_delving(&mut app);
        let player = app.world_mut().spawn((Player, DelverNeeds(hungry(0.55)))).id();
        app.update();
        app.update();
        (app, player)
    }

    fn press_u(app: &mut App) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyU);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::KeyU);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
    }

    #[test]
    fn u_over_a_loaf_in_the_inventory_eats_it() {
        let (mut app, player) = app();
        let loaf = app.world_mut().resource_mut::<Inventory>().add(item("Bread"));
        let torch = app.world_mut().resource_mut::<Inventory>().add(item("Torch"));
        // Over a torch: nothing happens.
        app.world_mut().resource_mut::<Hovered>().0 = Some((torch, Rect::default()));
        press_u(&mut app);
        assert!(app.world().resource::<Inventory>().item(torch).is_some());
        assert!((app.world().get::<DelverNeeds>(player).unwrap().0.hunger.value - 0.55).abs() < 1e-9);
        // Over the loaf: eaten.
        app.world_mut().resource_mut::<Hovered>().0 = Some((loaf, Rect::default()));
        press_u(&mut app);
        assert!(app.world().resource::<Inventory>().item(loaf).is_none());
        let hunger = app.world().get::<DelverNeeds>(player).unwrap().0.hunger;
        assert!((hunger.value - 0.15).abs() < 1e-9, "{}", hunger.value);
        assert!(app.world().resource::<Toast>().text.starts_with("You eat the "), "{}", app.world().resource::<Toast>().text);
    }

    fn hurt(hp: i32) -> Health {
        Health { hp, max: 12 }
    }

    #[test]
    fn a_draught_at_three_of_twelve_heals_to_between_seven_and_twelve() {
        for seed in 0..200 {
            let mut health = hurt(3);
            drink_healing(&mut health, &mut Mulberry32::new(seed));
            assert!((7..=12).contains(&health.hp), "seed {seed}: {}", health.hp);
        }
    }

    #[test]
    fn a_draught_never_heals_above_the_maximum() {
        for seed in 0..50 {
            let mut health = hurt(11);
            assert_eq!(drink_healing(&mut health, &mut Mulberry32::new(seed)), 1);
            assert_eq!(health.hp, 12);
        }
        let mut full = Health::full(12);
        assert_eq!(drink_healing(&mut full, &mut Mulberry32::new(1)), 0);
        assert_eq!(full.hp, 12);
    }

    #[test]
    fn the_same_dice_give_the_same_roll() {
        let roll = |seed| {
            let mut health = Health { hp: 1, max: 100 };
            drink_healing(&mut health, &mut Mulberry32::new(seed))
        };
        assert_eq!(roll(7), roll(7));
        assert!((4..=10).contains(&roll(7)));
        assert!((0..40).map(roll).collect::<std::collections::HashSet<_>>().len() > 1);
    }

    #[test]
    fn heal_never_lowers_and_reports_what_it_restored() {
        let mut health = hurt(5);
        assert_eq!(health.heal(4), 4);
        assert_eq!(health.heal(100), 3);
        assert_eq!(health.hp, 12);
        let mut over = Health { hp: 15, max: 12 };
        assert_eq!(over.heal(3), 0);
        assert_eq!(over.hp, 15);
    }

    fn glow_colour(app: &mut App) -> Srgba {
        app.world_mut().query_filtered::<&BackgroundColor, With<HealFlash>>().single(app.world()).unwrap().0.to_srgba()
    }

    #[test]
    fn u_over_a_draught_heals_flashes_green_chimes_and_is_used_up() {
        let (mut app, player) = app();
        app.world_mut().entity_mut(player).insert(hurt(3));
        // A view for the flash to sit over.
        app.world_mut().spawn(crate::carry::ViewModelCamera { follows: Entity::PLACEHOLDER });
        app.update();
        let draught = app.world_mut().resource_mut::<Inventory>().add(item("Healing draught"));
        app.world_mut().resource_mut::<Hovered>().0 = Some((draught, Rect::default()));
        press_u(&mut app);
        let hp = app.world().get::<Health>(player).unwrap().hp;
        assert!((7..=12).contains(&hp), "{hp}");
        assert!(app.world().resource::<Inventory>().item(draught).is_none());
        let toast = app.world().resource::<Toast>().text.clone();
        assert!(toast.starts_with("You drink the healing draught (+") && toast.contains(&format!("(+{})", hp - 3)), "{toast}");
        // The sound is at the effects volume.
        let want = crate::audio::effect_volume(&GameSettings::default(), &Tuning::default());
        let playback: Vec<&PlaybackSettings> = app.world_mut().query_filtered::<&PlaybackSettings, With<crate::audio::SoundEffect>>().iter(app.world()).collect();
        assert_eq!(playback.len(), 1);
        assert_eq!(playback[0].volume, bevy::audio::Volume::Linear(want));
        // The flash is green.
        let colour = glow_colour(&mut app);
        assert!(colour.alpha > 0.0 && colour.green > colour.red && colour.green > colour.blue, "{colour:?}");
    }

    #[test]
    fn the_flash_fades_over_its_time() {
        let (mut app, _) = app();
        app.world_mut().spawn(crate::carry::ViewModelCamera { follows: Entity::PLACEHOLDER });
        app.update();
        app.world_mut().resource_mut::<HealGlow>().0 = HEAL_FLASH_TIME;
        app.update();
        let start = glow_colour(&mut app).alpha;
        assert!(start > 0.0);
        app.world_mut().resource_mut::<HealGlow>().0 = HEAL_FLASH_TIME * 0.5;
        app.update();
        let half = glow_colour(&mut app).alpha;
        assert!(half > 0.0 && half < start, "{half} {start}");
        app.world_mut().resource_mut::<HealGlow>().0 = 0.0;
        app.update();
        assert_eq!(glow_colour(&mut app).alpha, 0.0);
    }

    #[test]
    fn a_draught_at_full_health_is_still_used_up_and_a_potion_is_a_mystery() {
        let (mut app, player) = app();
        app.world_mut().entity_mut(player).insert(Health::full(12));
        let draught = app.world_mut().resource_mut::<Inventory>().add(item("Healing draught"));
        app.world_mut().resource_mut::<Hovered>().0 = Some((draught, Rect::default()));
        press_u(&mut app);
        assert!(app.world().resource::<Inventory>().item(draught).is_none());
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 12);
        assert!(app.world().resource::<Toast>().text.ends_with("(+0)"));
        // A plain Potion does not heal.
        app.world_mut().entity_mut(player).insert(hurt(3));
        let potion = app.world_mut().resource_mut::<Inventory>().add(item("Potion"));
        app.world_mut().resource_mut::<Hovered>().0 = Some((potion, Rect::default()));
        press_u(&mut app);
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 3);
    }

    #[test]
    fn u_does_nothing_with_the_inventory_shut() {
        let (mut app, player) = app();
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
        let loaf = app.world_mut().resource_mut::<Inventory>().add(item("Bread"));
        app.world_mut().resource_mut::<Hovered>().0 = Some((loaf, Rect::default()));
        press_u(&mut app);
        assert!(app.world().resource::<Inventory>().item(loaf).is_some());
        assert_eq!(app.world().get::<DelverNeeds>(player).unwrap().0.hunger.value, 0.55);
    }
}
