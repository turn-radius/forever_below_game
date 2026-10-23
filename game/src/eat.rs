//! Eating and drinking from the inventory (#228). With the screen open, the item under the mouse
//! is the target of [`act_on_hovered`], which reads its verbs from [`crate::interact`]: F (Primary)
//! uses it, G (Secondary) equips it or takes it off. F eats or drinks the food or drink: the item
//! is used up and its nutrition and water come off the delver's hunger and thirst
//! ([`fb_items::Provision`], `docs/items.md`). The inventory keeps no counts, so a stack of bread is several items and one is eaten at a time. A waterskin stays: each
//! F is a swallow ([`fb_items::SWALLOW`]) out of its fill, until it is empty.
//!
//! [`consume`] does the work and says what it did, so the healing draught adds its cure to it
//! (#235): 2d4 + 2 hit points from the run's dice, a green flash and a soft chime. An Antidote (a potion of the antidote's brew) ends the
//! venom's Poisoned ([`fb_character::poison`]). The delver's needs are the [`DelverNeeds`] on the
//! player; the game clock steps them in #229.

use bevy::{ecs::system::SystemParam, prelude::*};
use armory::families::food::Brew;
use fb_items::{Consume, Provision};
use procgen_rng::Mulberry32;

use crate::character::{Character, DelverEffects, DelverNeeds, Reading};
use crate::combat::{Health, RunDice};
use crate::settings::GameSettings;
use crate::tuning::Tuning;
use crate::ui::OK;
use crate::inventory::{FloorChanges, Hovered, Inventory, ItemId, Toast};
use crate::carry::FloorItem;
use crate::interact::{self, Slot, Verb};
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, Player, inventory_open};

pub struct EatPlugin;

impl Plugin for EatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HealGlow>()
            .add_systems(Startup, load_heal_sound)
            .add_systems(Update, drink_nearest_healing.run_if(crate::play::level_has_focus).in_set(DelveSystems))
            .add_systems(Update, act_on_hovered.run_if(inventory_open).run_if(crate::barter::not_bartering).in_set(DelveSystems))
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

/// What was eaten or drunk.
#[derive(Clone, Debug, PartialEq)]
pub struct Consumed {
    pub how: Consume,
    /// The item's own name, for the toast.
    pub name: String,
    pub provision: Provision,
    /// The item's base, so a rule keyed on it (the healing draught's cure) can follow.
    pub base: String,
    /// A swallow from a waterskin that stays, not an item used up.
    pub sipped: bool,
    /// What a potion held, so a rule keyed on it (the antidote's cure) can follow.
    pub brew: Option<Brew>,
    /// What an elixir gives, so the delver can be given it.
    pub elixir: Option<fb_items::Elixir>,
}

/// Eats or drinks `id`: takes it out of the inventory, wherever it lies, and takes its
/// nutrition and water off `needs`. A waterskin is drunk from instead: a swallow comes out of its
/// fill and it stays. `None`, and nothing changed, if it is not food or drink.
pub fn consume(inventory: &mut Inventory, needs: &mut fb_character::Needs, id: ItemId) -> Option<Consumed> {
    let item = inventory.item(id)?;
    if item.can_sip() {
        let (name, base) = (item.title().to_string(), item.base.clone());
        let provision = inventory.sip(id)?;
        needs.consume(provision.nutrition, provision.water);
        return Some(Consumed { how: Consume::Drink, name, provision, base, sipped: true, brew: None, elixir: None });
    }
    let (how, provision) = (item.consume()?, item.provision()?);
    let done = Consumed { how, name: item.title().to_string(), provision, base: item.base.clone(), sipped: false, brew: item.brew(), elixir: item.elixir() };
    inventory.forget(id);
    needs.consume(provision.nutrition, provision.water);
    Some(done)
}

/// "You eat the bread".
pub fn say(done: &Consumed) -> String {
    if done.sipped {
        return format!("You drink from the {}", done.name.to_lowercase());
    }
    format!("You {} the {}", done.how.past(), done.name.to_lowercase())
}

/// The effect an Antidote ends: the venom's.
pub const CURED_BY_ANTIDOTE: &str = "poisoned";
/// The effect the Elixir of Embers gives.
const WARMED: &str = "warmed";
/// The chills that warmth from within lifts.
const LIFTED_BY_WARMTH: [&str; 2] = ["cold", "chilled_blood"];

/// What eating needs from the world: the delver's needs, hit points and effects, the run's dice,
/// the green flash and the chime. [`Eating::eat`] is what F in the inventory and a quickslot share.
#[derive(SystemParam)]
pub struct Eating<'w, 's> {
    commands: Commands<'w, 's>,
    needs: Query<'w, 's, &'static mut DelverNeeds, With<Player>>,
    healths: Query<'w, 's, &'static mut Health, With<Player>>,
    effects: Query<'w, 's, (Entity, Option<&'static mut DelverEffects>), With<Player>>,
    rules: Option<Res<'w, crate::rules::GameRules>>,
    dice: Option<ResMut<'w, RunDice>>,
    glow: ResMut<'w, HealGlow>,
    sound: Option<Res<'w, HealSound>>,
    settings: Option<Res<'w, GameSettings>>,
    tuning: Res<'w, Tuning>,
    character: Option<ResMut<'w, Character>>,
    page: Option<ResMut<'w, crate::reading::OpenPage>>,
}

impl Eating<'_, '_> {
    /// Whether the delver is at full hit points.
    pub fn unhurt(&self) -> bool {
        self.healths.single().is_ok_and(|h| h.hp >= h.max)
    }

    /// Eats or drinks `id` ([`consume`]), and if it is a healing draught heals the delver, flashes
    /// the view green and chimes; an Antidote ends their poisoning. Returns what the toast says; `None`, and nothing changed, if it
    /// is not food or drink, or there is no delver.
    pub fn eat(&mut self, inventory: &mut Inventory, id: ItemId) -> Option<String> {
        let mut needs = self.needs.single_mut().ok()?;
        let done = consume(inventory, &mut needs.0, id)?;
        let mut line = say(&done);
        if done.base == HEALING_DRAUGHT
            && let (Some(dice), Ok(mut health)) = (self.dice.as_mut(), self.healths.single_mut())
        {
            let restored = drink_healing(&mut health, &mut dice.0);
            line = format!("{line} (+{restored})");
            self.glow.0 = HEAL_FLASH_TIME;
            if let Some(sound) = &self.sound {
                let game = self.settings.as_deref().copied().unwrap_or_default();
                self.commands.spawn((Name::new("heal"), AudioPlayer::new(sound.0.clone()), crate::audio::sound_effect(&game, &self.tuning), DespawnOnExit(crate::launch::AppState::Delving)));
            }
        }
        if done.brew == Some(Brew::Antidote)
            && let Ok((player, Some(mut effects))) = self.effects.single_mut()
            && effects.0.get(CURED_BY_ANTIDOTE, None).is_some()
        {
            effects.0.remove(CURED_BY_ANTIDOTE, None);
            // The component goes with the last effect, as the clock takes it off.
            if effects.0.active.is_empty() {
                self.commands.entity(player).remove::<DelverEffects>();
            }
            line = format!("{line}: the venom is gone");
        }
        if let Some(elixir) = done.elixir
            && let Ok((player, effects)) = self.effects.single_mut()
        {
            let defs = &crate::rules::GameRules::of(&self.rules).effects;
            let source = fb_character::effect::Source::Consumed { what: done.name.clone() };
            match effects {
                Some(mut effects) => {
                    // Warmth from within drives the cold out at once.
                    if elixir.effect == WARMED {
                        for cold in LIFTED_BY_WARMTH {
                            effects.0.remove(cold, None);
                        }
                    }
                    effects.0.add(defs, elixir.effect, elixir.intensity, None, source);
                }
                None => {
                    let mut fresh = fb_character::effect::Effects::default();
                    fresh.add(defs, elixir.effect, elixir.intensity, None, source);
                    self.commands.entity(player).insert(DelverEffects(fresh));
                }
            }
            if let Some(def) = defs.get(elixir.effect) {
                line = format!("{line}: {}", def.name.to_lowercase());
            }
        }
        Some(line)
    }

    /// Reads `id`: a note's words open on a page ([`crate::reading`]), and a book teaches
    /// ([`Character::read`]), the score it teaches rising for good, once per delver. Returns what
    /// the toast says; `None` if there is nothing to read on it or no delver to read a book.
    pub fn read(&mut self, inventory: &Inventory, id: ItemId) -> Option<String> {
        let book = inventory.item(id)?;
        if let (Some(words), Some(page)) = (&book.words, self.page.as_mut()) {
            page.open(book.title(), words.clone());
            return Some(format!("You read the {}", book.title().to_lowercase()));
        }
        let character = self.character.as_mut()?;
        book.lesson()?;
        let name = book.title().to_lowercase();
        // Only a lesson learned changes the delver, so a book read before marks nothing.
        if character.books_read.contains(&book.seed) {
            return Some(format!("The {name} has nothing more to teach you"));
        }
        Some(match character.read(book) {
            Reading::Learned { stat, score } => format!("You study the {name}. Your {} rises to {score}", fb_items::lesson::long_name(stat)),
            Reading::AlreadyRead | Reading::NothingToLearn => format!("The {name} has nothing more to teach you"),
        })
    }
}

/// The healing draught nearest to hand among what is carried, never one lying underfoot or in a
/// container not carried: a quickslot's first, then the belt's grid, then the pack's, then the
/// bags inside those, breadth first.
pub fn nearest_healing(inventory: &Inventory, slots: Option<&crate::quickslots::Quickslots>) -> Option<ItemId> {
    use fb_items::Slot;
    let is_draught = |id: ItemId| inventory.is_carried(id) && inventory.item(id).is_some_and(|i| i.base == HEALING_DRAUGHT);
    if let Some(id) = slots.and_then(|s| s.0.iter().flatten().copied().find(|&id| is_draught(id))) {
        return Some(id);
    }
    let order = [Slot::Belt, Slot::Hip, Slot::Back, Slot::Quiver];
    let mut level: Vec<ItemId> = order.iter().filter_map(|&slot| inventory.worn(slot)).collect();
    while !level.is_empty() {
        let mut inside = Vec::new();
        for container in level {
            for entry in inventory.grid(container).map(|g| g.entries()).into_iter().flatten() {
                if is_draught(entry.id) {
                    return Some(entry.id);
                }
                inside.push(entry.id);
            }
        }
        level = inside;
    }
    None
}

/// H on the level: drinks the nearest healing draught carried ([`nearest_healing`]) as the
/// inventory's drink does ([`Eating::eat`]). Says so if none is carried, and at full hit points
/// keeps the draught.
fn drink_nearest_healing(
    input: ActionInput,
    mut slots: Option<ResMut<crate::quickslots::Quickslots>>,
    mut inventory: ResMut<Inventory>,
    mut eating: Eating,
    mut floor_changes: ResMut<FloorChanges>,
    mut toast: ResMut<Toast>,
    time: Option<Res<Time<Real>>>,
) {
    if !input.just_pressed(Action::DrinkHealing) {
        return;
    }
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    let Some(id) = nearest_healing(&inventory, slots.as_deref()) else {
        toast.say("You have no healing draught.", now);
        return;
    };
    if eating.unhurt() {
        toast.say("You are unhurt.", now);
        return;
    }
    // From a quickslot, drink as the key does, so the slot moves on to the next draught.
    let slot = slots.as_deref().and_then(|s| s.slot_of(id));
    if let (Some(slots), Some(slot)) = (slots.as_mut(), slot) {
        let line = crate::quickslots::use_slot(slot, slots, &mut inventory, &mut eating, &mut floor_changes);
        toast.say(line, now);
    } else if let Some(line) = eating.eat(&mut inventory, id) {
        toast.say(line, now);
    }
}

/// F and G in the inventory, on the item under the mouse. Its verbs come from
/// [`interact::verbs_for`], the same lookup the prompts read. F does its use: Eat and Drink
/// ([`Eating::eat`]), Light and Put out ([`crate::kindle::toggle_and_say`]), Refill from lamp oil
/// ([`crate::kindle::refill_and_say`]), Read a book ([`Eating::read`]), or Take for an item
/// underfoot or in a container not carried, into the first bag with room ([`crate::inventory::take_up`]).
/// G equips it in the slot it is made for, or takes it off if worn, back to the pack or underfoot
/// if that is full ([`crate::inventory::quick`], the double-click). An item lying underfoot is
/// equipped as it is from the level ([`crate::inventory::equip_from_floor`]): what it displaces
/// goes into a bag, or is set down where the item lay. T, Hang at hip, does the same into the hip
/// ([`crate::inventory::equip`], [`crate::inventory::equip_from_floor_in`]). An item with no such
/// verb does nothing.
#[allow(clippy::too_many_arguments)]
fn act_on_hovered(
    input: ActionInput,
    hovered: Res<Hovered>,
    mut inventory: ResMut<Inventory>,
    mut eating: Eating,
    mut floor_changes: ResMut<FloorChanges>,
    mut toast: ResMut<Toast>,
    floor: Query<&FloorItem>,
    time: Option<Res<Time<Real>>>,
) {
    let primary = input.just_pressed(Action::Primary);
    let special = input.just_pressed(Action::Special);
    if !primary && !special && !input.just_pressed(Action::Secondary) {
        return;
    }
    let Some((id, _)) = hovered.0 else { return };
    let Some(target) = interact::item_target(&inventory, id) else { return };
    let verbs = interact::verbs_for(&target, &interact::Delver::default());
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    if primary {
        match verbs.get(Slot::Primary) {
            Some(Verb::Eat | Verb::Drink) => {
                if let Some(line) = eating.eat(&mut inventory, id) {
                    toast.say(line, now);
                }
            }
            Some(Verb::Light | Verb::PutOut) => {
                let said = crate::kindle::toggle_and_say(&mut inventory, id);
                toast.say(said, now);
            }
            Some(Verb::Refill) => {
                let said = crate::kindle::refill_and_say(&mut inventory, id);
                toast.say(said, now);
            }
            Some(Verb::Read) => {
                if let Some(line) = eating.read(&inventory, id) {
                    toast.say(line, now);
                }
            }
            Some(Verb::Take) => match crate::inventory::take_up(&mut inventory, id) {
                Ok(changes) => floor_changes.0.extend(changes),
                Err(refusal) => toast.say(refusal.reason(), now),
            },
            _ => {}
        }
    } else if special {
        if verbs.get(Slot::Special) != Some(Verb::HangAtHip) {
            return;
        }
        let hung = if inventory.location(id) == Some(crate::inventory::Location::Loose) {
            let (at, yaw) = floor.iter().find(|f| f.id == id).map_or((Vec2::ZERO, 0.0), |f| (Vec2::new(f.spot.x, f.spot.y), f.spot.yaw));
            crate::inventory::equip_from_floor_in(&mut inventory, id, fb_items::Slot::Hip, at, yaw).map(|equipped| equipped.changes)
        } else {
            crate::inventory::equip(&mut inventory, id, fb_items::Slot::Hip)
        };
        match hung {
            Ok(changes) => floor_changes.0.extend(changes),
            Err(refusal) => toast.say(refusal.reason(), now),
        }
    } else if verbs.get(Slot::Secondary) == Some(Verb::Equip)
        && inventory.location(id) == Some(crate::inventory::Location::Loose)
    {
        let (at, yaw) = floor.iter().find(|f| f.id == id).map_or((Vec2::ZERO, 0.0), |f| (Vec2::new(f.spot.x, f.spot.y), f.spot.yaw));
        match crate::inventory::equip_from_floor(&mut inventory, id, at, yaw) {
            Ok(equipped) => {
                toast.say(equipped.message(&inventory, id), now);
                floor_changes.0.extend(equipped.changes);
            }
            Err(refusal) => toast.say(refusal.reason(), now),
        }
    } else if matches!(verbs.get(Slot::Secondary), Some(Verb::Equip | Verb::TakeOff)) {
        match crate::inventory::quick(&mut inventory, id) {
            Ok(changes) => floor_changes.0.extend(changes),
            Err(refusal) => toast.say(refusal.reason(), now),
        }
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
    use crate::play::{Screen, ScreenOpen};

    fn item(base: &str) -> fb_items::Item {
        fb_items::roll_item(base, 5).unwrap()
    }

    fn item_seeded(base: &str) -> impl Fn(u32) -> fb_items::Item + '_ {
        move |seed| fb_items::roll_item(base, seed).unwrap()
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
            .init_resource::<FloorChanges>()
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

    fn press_key(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(key);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
    }

    fn press_u(app: &mut App) {
        press_key(app, KeyCode::KeyF);
    }

    /// `base` in the pack, wearing one first if need be; an item added loose lies underfoot.
    fn carry(app: &mut App, base: &str) -> ItemId {
        stow(app, item(base))
    }

    /// `thing` in the pack, wearing one first if need be.
    fn stow(app: &mut App, thing: fb_items::Item) -> ItemId {
        let mut inventory = app.world_mut().resource_mut::<Inventory>();
        if inventory.worn(fb_items::Slot::Back).is_none() {
            let pack = inventory.add(item("Pack"));
            inventory.wear(pack, fb_items::Slot::Back).unwrap();
        }
        let id = inventory.add(thing);
        inventory.stow(id).unwrap();
        id
    }

    fn hover(app: &mut App, id: ItemId) {
        app.world_mut().resource_mut::<Hovered>().0 = Some((id, Rect::default()));
    }

    #[test]
    fn f_over_a_book_teaches_its_score_once() {
        let (mut app, _) = app();
        app.insert_resource(Character::roll(3));
        let book = carry(&mut app, "Book");
        let stat = item("Book").lesson().unwrap();
        let score = |app: &App| {
            let a = app.world().resource::<Character>().scores();
            [a.strength, a.dexterity, a.constitution, a.intelligence, a.wisdom][fb_items::lesson::LESSONS.iter().position(|&s| s == stat).unwrap()]
        };
        let before = score(&app);
        hover(&mut app, book);
        press_u(&mut app);
        assert_eq!(score(&app), before + 1);
        let said = app.world().resource::<Toast>().text.clone();
        assert!(said.contains(fb_items::lesson::long_name(stat)) && said.contains(&format!("rises to {}", before + 1)), "{said}");
        assert!(app.world().resource::<Inventory>().item(book).is_some(), "the book is kept");
        press_u(&mut app);
        assert_eq!(score(&app), before + 1, "a book teaches a delver once");
        assert!(app.world().resource::<Toast>().text.ends_with("has nothing more to teach you"));
    }

    #[test]
    fn f_over_a_scroll_or_note_does_nothing() {
        let (mut app, _) = app();
        app.insert_resource(Character::roll(3));
        for base in ["Scroll", "Note"] {
            let id = carry(&mut app, base);
            hover(&mut app, id);
            press_u(&mut app);
            assert_eq!(app.world().resource::<Character>().learned, fb_character::AbilityBonus::default(), "{base}");
            assert!(app.world().resource::<Inventory>().item(id).is_some(), "{base}");
        }
    }

    #[test]
    fn g_over_a_ring_puts_it_on_a_free_finger_and_takes_it_off_again() {
        let (mut app, _) = app();
        let (pack, ring) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let pack = inventory.add(item("Pack"));
            inventory.wear(pack, fb_items::Slot::Back).unwrap();
            let ring = inventory.add(item("Ring"));
            inventory.stow(ring).unwrap();
            (pack, ring)
        };
        hover(&mut app, ring);
        press_key(&mut app, KeyCode::KeyG);
        let worn = app.world().resource::<Inventory>().location(ring);
        assert!(matches!(worn, Some(crate::inventory::Location::Worn(fb_items::Slot::Ring1 | fb_items::Slot::Ring2))), "{worn:?}");
        press_key(&mut app, KeyCode::KeyG);
        let back = app.world().resource::<Inventory>().location(ring);
        assert!(matches!(back, Some(crate::inventory::Location::In { container, .. }) if container == pack), "{back:?}");
    }

    #[test]
    fn g_over_a_worn_item_with_a_full_pack_sets_it_down_underfoot() {
        let (mut app, _) = app();
        let ring = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let ring = inventory.add(item("Ring"));
            inventory.wear(ring, fb_items::Slot::Ring1).unwrap();
            ring
        };
        hover(&mut app, ring);
        press_key(&mut app, KeyCode::KeyG);
        assert_eq!(app.world().resource::<FloorChanges>().0, vec![crate::inventory::FloorChange::SetDown(ring, 0.0)]);
        assert_eq!(app.world().resource::<Inventory>().location(ring), Some(crate::inventory::Location::Loose));
    }

    #[test]
    fn g_over_a_good_that_cannot_be_worn_does_nothing() {
        let (mut app, _) = app();
        let bread = app.world_mut().resource_mut::<Inventory>().add(item("Bread"));
        let before = app.world().resource::<Inventory>().location(bread);
        hover(&mut app, bread);
        press_key(&mut app, KeyCode::KeyG);
        assert_eq!(app.world().resource::<Inventory>().location(bread), before);
        assert!(app.world().resource::<FloorChanges>().0.is_empty());
    }

    #[test]
    fn f_over_a_waterskin_drinks_a_swallow_and_keeps_the_skin() {
        let (mut app, player) = app();
        app.world_mut().get_mut::<DelverNeeds>(player).unwrap().0.thirst.value = 0.8;
        let skin = carry(&mut app, "Waterskin");
        hover(&mut app, skin);
        press_u(&mut app);
        let thirst = app.world().get::<DelverNeeds>(player).unwrap().0.thirst.value;
        assert!((thirst - 0.55).abs() < 1e-6, "{thirst}");
        assert!(app.world().resource::<Inventory>().item(skin).is_some());
        assert_eq!(app.world().resource::<Toast>().text, "You drink from the waterskin");
    }

    #[test]
    fn f_over_an_unlit_torch_lights_it() {
        let (mut app, _) = app();
        let torch = carry(&mut app, "Spare torch");
        hover(&mut app, torch);
        press_u(&mut app);
        assert_eq!(app.world().resource::<Inventory>().item(torch).unwrap().shape.lit(), Some(true));
        press_u(&mut app);
        assert_eq!(app.world().resource::<Inventory>().item(torch).unwrap().shape.lit(), Some(false));
    }

    #[test]
    fn f_over_a_tankard_quenches_thirst() {
        let (mut app, player) = app();
        app.world_mut().get_mut::<DelverNeeds>(player).unwrap().0.thirst.value = 0.8;
        let tankard = carry(&mut app, "Tankard");
        hover(&mut app, tankard);
        press_u(&mut app);
        assert!((app.world().get::<DelverNeeds>(player).unwrap().0.thirst.value - 0.3).abs() < 1e-6);
    }

    #[test]
    fn u_over_a_loaf_in_the_inventory_eats_it() {
        let (mut app, player) = app();
        let loaf = carry(&mut app, "Bread");
        let torch = carry(&mut app, "Torch");
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

    fn lying(app: &mut App, base: &str) -> ItemId {
        app.world_mut().resource_mut::<Inventory>().add(item(base))
    }

    #[test]
    fn f_over_a_dagger_underfoot_takes_it_into_the_pack() {
        let (mut app, _) = app();
        let bread = carry(&mut app, "Bread");
        let pack = app.world().resource::<Inventory>().location(bread);
        let dagger = lying(&mut app, "Dagger");
        hover(&mut app, dagger);
        press_u(&mut app);
        let taken = app.world().resource::<Inventory>().location(dagger);
        assert!(matches!((taken, pack), (Some(crate::inventory::Location::In { container, .. }), Some(crate::inventory::Location::In { container: bag, .. })) if container == bag), "{taken:?}");
        assert_eq!(app.world().resource::<FloorChanges>().0, vec![crate::inventory::FloorChange::Lifted(dagger)]);
    }

    #[test]
    fn f_over_underfoot_bread_takes_it_rather_than_eating_it() {
        let (mut app, player) = app();
        carry(&mut app, "Torch");
        let bread = lying(&mut app, "Bread");
        hover(&mut app, bread);
        press_u(&mut app);
        assert!(matches!(app.world().resource::<Inventory>().location(bread), Some(crate::inventory::Location::In { .. })));
        assert!((app.world().get::<DelverNeeds>(player).unwrap().0.hunger.value - 0.55).abs() < 1e-9);
    }

    #[test]
    fn f_with_no_room_leaves_the_item_underfoot_and_says_why() {
        let (mut app, _) = app();
        let dagger = lying(&mut app, "Dagger");
        hover(&mut app, dagger);
        press_u(&mut app);
        assert_eq!(app.world().resource::<Inventory>().location(dagger), Some(crate::inventory::Location::Loose));
        assert!(!app.world().resource::<Toast>().text.is_empty());
    }

    #[test]
    fn f_over_an_item_in_a_set_down_bag_takes_it_into_the_pack() {
        let (mut app, _) = app();
        carry(&mut app, "Torch");
        let (sack, dagger) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let sack = inventory.add(item("Sack"));
            let dagger = inventory.add(item("Dagger"));
            let at = inventory.grid(sack).and_then(|g| g.first_free(inventory.item(dagger).unwrap().profile.footprint)).unwrap();
            inventory.put(dagger, sack, at).unwrap();
            (sack, dagger)
        };
        hover(&mut app, dagger);
        press_u(&mut app);
        let taken = app.world().resource::<Inventory>().location(dagger);
        assert!(matches!(taken, Some(crate::inventory::Location::In { container, .. }) if container != sack), "{taken:?}");
    }

    #[test]
    fn g_over_a_helmet_underfoot_wears_it_and_sends_the_old_one_to_the_pack() {
        let (mut app, _) = app();
        carry(&mut app, "Torch");
        let (old, new) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let old = inventory.add(item("Helm"));
            inventory.wear(old, fb_items::Slot::Head).unwrap();
            (old, inventory.add(item("Helm")))
        };
        hover(&mut app, new);
        press_key(&mut app, KeyCode::KeyG);
        let inventory = app.world().resource::<Inventory>();
        assert_eq!(inventory.location(new), Some(crate::inventory::Location::Worn(fb_items::Slot::Head)));
        assert!(matches!(inventory.location(old), Some(crate::inventory::Location::In { .. })), "{:?}", inventory.location(old));
        assert!(app.world().resource::<Toast>().text.starts_with("Wearing "), "{}", app.world().resource::<Toast>().text);
    }

    #[test]
    fn g_over_a_helmet_underfoot_with_a_full_pack_sets_the_old_one_down_where_it_lay() {
        let (mut app, _) = app();
        let (old, new) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let old = inventory.add(item("Helm"));
            inventory.wear(old, fb_items::Slot::Head).unwrap();
            (old, inventory.add(item("Helm")))
        };
        let shape = app.world().resource::<Inventory>().item(new).unwrap();
        let spot = fb_level::ItemSpot { x: 3.0, y: 4.0, yaw: 0.5, family: shape.shape.family(), seed: shape.seed };
        app.world_mut().spawn(FloorItem { id: new, spot, min: Vec3::ZERO, max: Vec3::ONE });
        hover(&mut app, new);
        press_key(&mut app, KeyCode::KeyG);
        let changes = app.world().resource::<FloorChanges>().0.clone();
        assert!(changes.contains(&crate::inventory::FloorChange::SetDownAt(old, Vec2::new(3.0, 4.0), 0.5)), "{changes:?}");
        assert!(changes.contains(&crate::inventory::FloorChange::Lifted(new)), "{changes:?}");
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
        let draught = carry(&mut app, "Healing draught");
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
        let draught = carry(&mut app, "Healing draught");
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
    fn an_antidote_ends_the_venom_and_leaves_other_effects() {
        let (mut app, player) = app();
        let antidote = (0..200).map(item_seeded("Potion")).find(|p| p.brew() == Some(Brew::Antidote)).unwrap();
        let mut effects = DelverEffects::poisoned_for(fb_character::rules::defaults(), 1.0);
        effects.0.add(&fb_character::rules::defaults().effects, "stinging_eyes", 0.8, None, fb_character::effect::Source::Surroundings);
        app.world_mut().entity_mut(player).insert(effects);
        let id = stow(&mut app, antidote.clone());
        hover(&mut app, id);
        press_u(&mut app);
        let left = app.world().get::<DelverEffects>(player).expect("their eyes still sting");
        assert!(left.poisoned_hours().is_none() && left.0.get("stinging_eyes", None).is_some());
        assert_eq!(app.world().resource::<Toast>().text, "You drink the antidote: the venom is gone");
        // With the venom the only effect, the component goes too.
        app.world_mut().entity_mut(player).insert(DelverEffects::poisoned_for(fb_character::rules::defaults(), 1.0));
        let id = stow(&mut app, antidote);
        hover(&mut app, id);
        press_u(&mut app);
        assert!(app.world().get::<DelverEffects>(player).is_none());
    }

    #[test]
    fn each_elixir_gives_its_effect_to_the_delver() {
        let defs = &fb_character::rules::defaults().effects;
        for (base, elixir) in fb_items::ELIXIRS {
            assert!(defs.contains_key(elixir.effect), "{base}");
            let (mut app, player) = app();
            let id = carry(&mut app, base);
            hover(&mut app, id);
            press_u(&mut app);
            assert!(app.world().resource::<Inventory>().item(id).is_none(), "{base} is used up");
            let effects = app.world().get::<DelverEffects>(player).expect(base);
            assert!(effects.0.get(elixir.effect, None).is_some(), "{base}");
        }
        // The Hare quickens the pace; Vigil slows sleepiness; Fasting slows hunger.
        let (mut app, player) = app();
        let id = carry(&mut app, "Elixir of the Hare");
        hover(&mut app, id);
        press_u(&mut app);
        let effects = &app.world().get::<DelverEffects>(player).unwrap().0;
        assert!(effects.pace(defs) > 1.2);
        assert_eq!(app.world().resource::<Toast>().text, "You drink the elixir of the hare: swift");
        for (base, meter) in [("Elixir of Vigil", fb_character::MeterKind::Sleepiness), ("Elixir of Fasting", fb_character::MeterKind::Hunger)] {
            let (mut drinker, player) = self::app();
            let id = carry(&mut drinker, base);
            hover(&mut drinker, id);
            press_u(&mut drinker);
            let effects = &drinker.world().get::<DelverEffects>(player).unwrap().0;
            assert!(effects.need_rate(defs, meter) < 1.0, "{base}");
        }
    }

    #[test]
    fn the_elixir_of_embers_drives_out_the_cold() {
        let defs = &fb_character::rules::defaults().effects;
        let (mut app, player) = app();
        let mut effects = fb_character::effect::Effects::default();
        effects.add(defs, "cold", 1.0, None, fb_character::effect::Source::Consumed { what: "test".into() });
        app.world_mut().entity_mut(player).insert(DelverEffects(effects));
        let id = carry(&mut app, "Elixir of Embers");
        hover(&mut app, id);
        press_u(&mut app);
        let effects = &app.world().get::<DelverEffects>(player).unwrap().0;
        assert!(effects.get("cold", None).is_none());
        assert!(effects.get(WARMED, None).is_some());
    }

    #[test]
    fn an_elixir_joins_the_effects_already_on_the_delver() {
        let (mut app, player) = app();
        app.world_mut().entity_mut(player).insert(DelverEffects::poisoned_for(fb_character::rules::defaults(), 1.0));
        let id = carry(&mut app, "Elixir of Vigil");
        hover(&mut app, id);
        press_u(&mut app);
        let effects = app.world().get::<DelverEffects>(player).unwrap();
        assert!(effects.poisoned_hours().is_some() && effects.0.get("wakeful", None).is_some());
    }

    #[test]
    fn a_healing_potion_does_not_cure_the_venom() {
        let (mut app, player) = app();
        app.world_mut().entity_mut(player).insert(DelverEffects::poisoned_for(fb_character::rules::defaults(), 1.0));
        let draught = carry(&mut app, "Healing draught");
        hover(&mut app, draught);
        press_u(&mut app);
        assert!(app.world().get::<DelverEffects>(player).is_some_and(|e| e.poisoned_hours().is_some()));
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

    fn on_the_level(app: &mut App) {
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
    }

    /// A sack stowed in the pack with a healing draught inside.
    fn draught_in_a_pouch(app: &mut App) -> (ItemId, ItemId) {
        let sack = stow(app, item("Sack"));
        let mut inventory = app.world_mut().resource_mut::<Inventory>();
        let draught = inventory.add(item(HEALING_DRAUGHT));
        let at = inventory.grid(sack).and_then(|g| g.first_free(inventory.item(draught).unwrap().profile.footprint)).unwrap();
        inventory.put(draught, sack, at).unwrap();
        (sack, draught)
    }

    #[test]
    fn h_drinks_a_draught_from_a_pouch_in_the_pack() {
        let (mut app, player) = app();
        on_the_level(&mut app);
        app.world_mut().entity_mut(player).insert(hurt(3));
        app.world_mut().spawn(crate::carry::ViewModelCamera { follows: Entity::PLACEHOLDER });
        let (sack, draught) = draught_in_a_pouch(&mut app);
        press_key(&mut app, KeyCode::KeyH);
        let hp = app.world().get::<Health>(player).unwrap().hp;
        assert!((7..=12).contains(&hp), "{hp}");
        let inventory = app.world().resource::<Inventory>();
        assert!(inventory.item(draught).is_none());
        assert_eq!(inventory.contents_count(sack), 0);
        assert!(app.world().resource::<Toast>().text.starts_with("You drink the healing draught (+"));
        assert!(app.world().resource::<HealGlow>().0 > 0.0);
        let chimes = app.world_mut().query_filtered::<(), With<crate::audio::SoundEffect>>().iter(app.world()).count();
        assert_eq!(chimes, 1);
    }

    #[test]
    fn h_drinks_the_belt_draught_before_the_pack_one() {
        let (mut app, player) = app();
        on_the_level(&mut app);
        app.world_mut().entity_mut(player).insert(hurt(3));
        let (_, in_pouch) = draught_in_a_pouch(&mut app);
        let in_pack = stow(&mut app, item(HEALING_DRAUGHT));
        let on_belt = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let belt = inventory.add(item("Belt"));
            inventory.wear(belt, fb_items::Slot::Belt).unwrap();
            let draught = inventory.add(item(HEALING_DRAUGHT));
            let at = inventory.grid(belt).and_then(|g| g.first_free(inventory.item(draught).unwrap().profile.footprint)).unwrap();
            inventory.put(draught, belt, at).unwrap();
            draught
        };
        press_key(&mut app, KeyCode::KeyH);
        let inventory = app.world().resource::<Inventory>();
        assert!(inventory.item(on_belt).is_none());
        assert!(inventory.item(in_pack).is_some() && inventory.item(in_pouch).is_some());
        // The pack's own draught comes before the one in the pouch.
        press_key(&mut app, KeyCode::KeyH);
        let inventory = app.world().resource::<Inventory>();
        assert!(inventory.item(in_pack).is_none() && inventory.item(in_pouch).is_some());
    }

    #[test]
    fn h_with_no_draught_says_so_and_changes_nothing() {
        let (mut app, player) = app();
        on_the_level(&mut app);
        app.world_mut().entity_mut(player).insert(hurt(3));
        let loaf = carry(&mut app, "Bread");
        press_key(&mut app, KeyCode::KeyH);
        assert_eq!(app.world().resource::<Toast>().text, "You have no healing draught.");
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 3);
        assert!(app.world().resource::<Inventory>().item(loaf).is_some());
        assert_eq!(app.world().resource::<HealGlow>().0, 0.0);
    }

    #[test]
    fn h_at_full_hit_points_keeps_the_draught() {
        let (mut app, player) = app();
        on_the_level(&mut app);
        app.world_mut().entity_mut(player).insert(hurt(12));
        let draught = carry(&mut app, HEALING_DRAUGHT);
        press_key(&mut app, KeyCode::KeyH);
        assert_eq!(app.world().resource::<Toast>().text, "You are unhurt.");
        assert!(app.world().resource::<Inventory>().item(draught).is_some());
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 12);
    }

    #[test]
    fn h_ignores_a_draught_on_the_floor_or_in_a_bag_underfoot() {
        let (mut app, player) = app();
        on_the_level(&mut app);
        app.world_mut().entity_mut(player).insert(hurt(3));
        carry(&mut app, "Torch");
        let (lying, boxed) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let lying = inventory.add(item(HEALING_DRAUGHT));
            let sack = inventory.add(item("Sack"));
            let boxed = inventory.add(item(HEALING_DRAUGHT));
            let at = inventory.grid(sack).and_then(|g| g.first_free(inventory.item(boxed).unwrap().profile.footprint)).unwrap();
            inventory.put(boxed, sack, at).unwrap();
            (lying, boxed)
        };
        press_key(&mut app, KeyCode::KeyH);
        assert_eq!(app.world().resource::<Toast>().text, "You have no healing draught.");
        let inventory = app.world().resource::<Inventory>();
        assert!(inventory.item(lying).is_some() && inventory.item(boxed).is_some());
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 3);
    }

    #[test]
    fn h_does_nothing_on_the_inventory_or_talk_screens() {
        let (mut app, player) = app();
        app.world_mut().entity_mut(player).insert(hurt(3));
        let draught = carry(&mut app, HEALING_DRAUGHT);
        for screen in [Some(Screen::Inventory), Some(Screen::Talk)] {
            app.world_mut().resource_mut::<ScreenOpen>().0 = screen;
            press_key(&mut app, KeyCode::KeyH);
            assert!(app.world().resource::<Inventory>().item(draught).is_some(), "{screen:?}");
            assert_eq!(app.world().get::<Health>(player).unwrap().hp, 3);
            assert!(app.world().resource::<Toast>().text.is_empty());
        }
    }

    #[test]
    fn drink_healing_is_bound_to_h_and_nothing_else_is() {
        let keymap = Keymap::default();
        assert_eq!(keymap.inputs(Action::DrinkHealing), &[crate::keymap::Input::Key(KeyCode::KeyH)]);
        let clashes = keymap.bindings.iter().filter(|b| b.action != Action::DrinkHealing && b.inputs.contains(&crate::keymap::Input::Key(KeyCode::KeyH))).count();
        assert_eq!(clashes, 0);
    }

    #[test]
    fn h_drinks_the_slotted_draught_and_the_slot_takes_the_next() {
        let (mut app, player) = app();
        on_the_level(&mut app);
        app.world_mut().init_resource::<crate::quickslots::Quickslots>();
        app.world_mut().entity_mut(player).insert(hurt(3));
        let a = stow(&mut app, item(HEALING_DRAUGHT));
        let c = stow(&mut app, item(HEALING_DRAUGHT));
        let b = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let belt = inventory.add(item("Belt"));
            inventory.wear(belt, fb_items::Slot::Belt).unwrap();
            let draught = inventory.add(item(HEALING_DRAUGHT));
            let at = inventory.grid(belt).and_then(|g| g.first_free(inventory.item(draught).unwrap().profile.footprint)).unwrap();
            inventory.put(draught, belt, at).unwrap();
            draught
        };
        app.world_mut().resource_mut::<crate::quickslots::Quickslots>().bind(2, a);
        press_key(&mut app, KeyCode::KeyH);
        let inventory = app.world().resource::<Inventory>();
        assert!(inventory.item(a).is_none() && inventory.item(b).is_some() && inventory.item(c).is_some());
        let held = app.world().resource::<crate::quickslots::Quickslots>().get(2);
        assert!(held.is_some() && held != Some(a), "{held:?}");
    }

    #[test]
    fn h_ignores_a_slotted_draught_set_down_underfoot() {
        let (mut app, player) = app();
        on_the_level(&mut app);
        app.world_mut().init_resource::<crate::quickslots::Quickslots>();
        app.world_mut().entity_mut(player).insert(hurt(3));
        carry(&mut app, "Torch");
        let lying = app.world_mut().resource_mut::<Inventory>().add(item(HEALING_DRAUGHT));
        app.world_mut().resource_mut::<crate::quickslots::Quickslots>().bind(0, lying);
        press_key(&mut app, KeyCode::KeyH);
        assert_eq!(app.world().resource::<Toast>().text, "You have no healing draught.");
        assert!(app.world().resource::<Inventory>().item(lying).is_some());
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 3);
    }
}
