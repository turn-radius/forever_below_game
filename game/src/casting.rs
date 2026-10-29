//! Casting in the game: the prayers and spells every caster shares ([`fb_character::casting`]).
//!
//! A caster writes prayers from terms ([`Spellbook`], on the delver as [`DelverSpellbook`] and
//! saved with them); a cleric starts with Divine Light written. The Pray key ([`Action::Pray`],
//! C) casts the chosen prayer at the chosen tier; the tier key ([`Action::CastTier`], V) steps
//! the tier 1, 2, 3 and round again. A quickslot can hold a written prayer as well as an item
//! ([`AbilitySlots`]): the slot's key then casts it at the chosen tier. A cleric starts with
//! Divine Light in the first free slot; in the inventory the slot's key with the mouse over
//! nothing binds the chosen prayer to that slot.
//!
//! Every cast goes through a [`CastRequest`] message, which anything can send. [`cast_requests`]
//! checks the caster and the terms, spends the mana from [`DelverMana`] and either sends a
//! [`Casted`] message, which is what a prayer's effect reacts to (#747 gives the light term its
//! light, reading `cast.written.effect`, `cast.range` and `cast.duration`), or says on screen
//! why it failed. The HUD shows the chosen prayer, its tier and its cost above the quickslot bar.

use bevy::prelude::*;
use fb_character::Casting;
use fb_character::casting::{Cast, CastError, MAX_TIER, Spellbook, cast_with, points};

use crate::character::{Character, DelverMana};
use crate::combat::orbs::{OrbCluster, cluster_layout};
use crate::inventory::{Hovered, Toast};
use crate::keymap::{Action, ActionInput, Keymap};
use crate::launch::AppState;
use crate::play::{DelveSystems, Player, inventory_open, level_has_focus};
use crate::quickslots::{Quickslots, SLOTS};
use crate::rules::GameRules;
use crate::ui::{BRIGHT, UiFonts, hud};

/// What the delver has written and can write with.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct DelverSpellbook(pub Spellbook);

pub struct CastingPlugin;

impl Plugin for CastingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AbilitySlots>()
            .init_resource::<Chosen>()
            .add_message::<CastRequest>()
            .add_message::<Casted>()
            .add_systems(OnExit(AppState::Delving), forget_the_abilities)
            .add_systems(
                Update,
                (
                    start_the_spellbook,
                    give_clerics_their_prayer,
                    items_displace_abilities.after(crate::quickslots::bind_hovered),
                    (step_tier, pray_key, ability_slot_key).run_if(level_has_focus),
                    bind_ability_hovered.run_if(inventory_open),
                    debug_cast,
                    cast_requests,
                    spawn_hud,
                    show_chosen,
                    show_slot_labels,
                )
                    .chain()
                    .in_set(DelveSystems),
            );
    }
}

/// Asks for a cast of the written prayer or spell called `name`. Sent by the Pray key and the
/// quickslots, and by anything else that casts.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct CastRequest {
    pub name: String,
    pub tier: u8,
}

/// A cast that went through, its mana spent. The prayer's effect reacts to this: the light term's
/// light is filled in on it.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct Casted(pub Cast);

/// The prayer (by name) and tier the Pray key and the quickslots cast.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct Chosen {
    pub name: String,
    pub tier: u8,
}

impl Default for Chosen {
    fn default() -> Self {
        Chosen { name: "Divine Light".into(), tier: 1 }
    }
}

/// The written prayer held by each quickslot, by name. A slot holds an item or a prayer, never both.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct AbilitySlots(pub [Option<String>; SLOTS]);

impl AbilitySlots {
    /// The prayer in `slot`.
    pub fn get(&self, slot: usize) -> Option<&str> {
        self.0.get(slot).and_then(|s| s.as_deref())
    }

    /// Holds `name` in `slot` alone: it leaves any other slot.
    pub fn bind(&mut self, slot: usize, name: &str) {
        if slot >= SLOTS {
            return;
        }
        for bound in &mut self.0 {
            if bound.as_deref() == Some(name) {
                *bound = None;
            }
        }
        self.0[slot] = Some(name.to_string());
    }
}

/// A new caster's spellbook starts as their class's: the known terms, and for a cleric Divine
/// Light. A spellbook already filled (a delve resumed) is left alone.
fn start_the_spellbook(mut players: Query<&mut DelverSpellbook>, character: Option<Res<Character>>, rules: Res<GameRules>, mut chosen: ResMut<Chosen>) {
    let Some(character) = character else { return };
    for mut book in &mut players {
        if book.0 == Spellbook::default() {
            let start = Spellbook::starting_with(&rules, character.class.casting());
            if start != Spellbook::default() {
                book.0 = start;
            }
        }
        // The chosen prayer is one the book holds.
        if book.0.find(&chosen.name).is_none() {
            chosen.name = book.0.written.first().map_or_else(String::new, |w| w.name.clone());
        }
    }
}

fn forget_the_abilities(mut slots: ResMut<AbilitySlots>, mut chosen: ResMut<Chosen>) {
    *slots = AbilitySlots::default();
    *chosen = Chosen::default();
}

/// A new cleric holds Divine Light in the first slot with nothing in it.
fn give_clerics_their_prayer(new: Query<(), Added<Player>>, character: Option<Res<Character>>, items: Res<Quickslots>, mut slots: ResMut<AbilitySlots>) {
    if new.is_empty() || character.is_none_or(|c| c.class.casting() != Casting::Divine) || slots.0.iter().any(Option::is_some) {
        return;
    }
    if let Some(free) = (0..SLOTS).find(|&s| items.get(s).is_none()) {
        slots.bind(free, "Divine Light");
    }
}

/// An item bound to a slot takes it from an ability.
fn items_displace_abilities(items: Res<Quickslots>, mut slots: ResMut<AbilitySlots>) {
    if !items.is_changed() {
        return;
    }
    for slot in 0..SLOTS {
        if items.get(slot).is_some() && slots.0[slot].is_some() {
            slots.0[slot] = None;
        }
    }
}

/// The tier key steps the chosen tier and says what it costs.
fn step_tier(input: ActionInput, mut chosen: ResMut<Chosen>, rules: Res<GameRules>, players: Query<&DelverSpellbook, With<Player>>, mut toast: ResMut<Toast>, time: Option<Res<Time<Real>>>) {
    if !input.just_pressed(Action::CastTier) {
        return;
    }
    chosen.tier = chosen.tier % MAX_TIER + 1;
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    let preview = players.iter().next().and_then(|b| b.0.find(&chosen.name)).map(|w| w.preview_with(&rules, chosen.tier));
    if let Some(p) = preview {
        toast.say(format!("{} tier {}: {} mana, {} ft", chosen.name, chosen.tier, points(p.cost), points(p.range)), now);
    }
}

fn pray_key(input: ActionInput, chosen: Res<Chosen>, mut requests: MessageWriter<CastRequest>) {
    if input.just_pressed(Action::Pray) {
        requests.write(CastRequest { name: chosen.name.clone(), tier: chosen.tier });
    }
}

/// A slot's key casts the prayer in it at the chosen tier.
fn ability_slot_key(input: ActionInput, slots: Res<AbilitySlots>, chosen: Res<Chosen>, mut requests: MessageWriter<CastRequest>) {
    let Some(slot) = input.just_pressed_index(Action::Quickslot) else { return };
    if let Some(name) = slots.get(slot) {
        requests.write(CastRequest { name: name.to_string(), tier: chosen.tier });
    }
}

/// In the inventory a slot's key over no item binds the chosen prayer to the slot.
fn bind_ability_hovered(
    input: ActionInput,
    hovered: Res<Hovered>,
    chosen: Res<Chosen>,
    players: Query<&DelverSpellbook, With<Player>>,
    mut slots: ResMut<AbilitySlots>,
    items: Res<Quickslots>,
    mut toast: ResMut<Toast>,
    time: Option<Res<Time<Real>>>,
) {
    let Some(slot) = input.just_pressed_index(Action::Quickslot) else { return };
    if hovered.0.is_some() || !players.iter().next().is_some_and(|b| b.0.find(&chosen.name).is_some()) {
        return;
    }
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    if items.get(slot).is_some() {
        toast.say(format!("Slot {} already holds an item", crate::quickslots::number(slot)), now);
        return;
    }
    slots.bind(slot, &chosen.name);
    toast.say(format!("{} is bound to slot {}", chosen.name, crate::quickslots::number(slot)), now);
}

/// Does each request: spends the mana and sends [`Casted`], or says why not.
fn cast_requests(
    mut requests: MessageReader<CastRequest>,
    mut done: MessageWriter<Casted>,
    character: Option<Res<Character>>,
    rules: Res<GameRules>,
    mut players: Query<(&mut DelverMana, &DelverSpellbook), With<Player>>,
    mut toast: ResMut<Toast>,
    time: Option<Res<Time<Real>>>,
) {
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    for request in requests.read() {
        let Some((mut mana, book)) = players.iter_mut().next() else { continue };
        if character.as_deref().is_some_and(|c| c.class.casting() == Casting::Martial) {
            toast.say(CastError::WrongCaster { needs: Casting::Martial }.say(&request.name), now);
            continue;
        }
        let Some(written) = book.0.find(&request.name) else {
            toast.say(format!("You have not written {}", request.name), now);
            continue;
        };
        let casting = character.as_deref().map_or(Casting::Martial, |c| c.class.casting());
        let mut pool = mana.0;
        match cast_with(&rules, &mut pool, casting, &book.0.unlocked, written, request.tier) {
            Ok(cast) => {
                mana.0 = pool;
                toast.say(format!("You cast {}, tier {}: {} mana", cast.written.name, cast.tier, points(cast.cost)), now);
                done.write(Casted(cast));
            }
            Err(why) => {
                toast.say(why.say(&request.name), now);
            }
        }
    }
}

/// `FB_PRAY=1` casts once, a moment into the delve; `FB_PRAY_TIER=2` chooses the tier first and
/// `FB_PRAY_DRAIN=1` empties the pool to show a failed cast. For captures.
fn debug_cast(mut frames: Local<u32>, mut chosen: ResMut<Chosen>, mut requests: MessageWriter<CastRequest>, mut players: Query<&mut DelverMana, With<Player>>) {
    *frames += 1;
    if *frames == 60 {
        if let Some(tier) = std::env::var("FB_PRAY_TIER").ok().and_then(|v| v.trim().parse::<u8>().ok()) {
            chosen.tier = tier.clamp(1, MAX_TIER);
        }
        if std::env::var("FB_PRAY_DRAIN").is_ok_and(|v| !v.is_empty())
            && let Some(mut mana) = players.iter_mut().next()
        {
            mana.0.points = 2.0;
        }
    }
    if *frames == 90 && std::env::var("FB_PRAY").is_ok_and(|v| !v.is_empty()) {
        requests.write(CastRequest { name: chosen.name.clone(), tier: chosen.tier });
    }
}

/// The line over the quickslot bar naming the chosen ability.
#[derive(Component)]
struct ChosenLine;

/// The ability's name and tier inside a slot.
#[derive(Component, Clone, Copy, Debug)]
pub struct AbilityLabel(pub usize);

fn spawn_hud(mut commands: Commands, clusters: Query<Entity, Added<OrbCluster>>, fonts: Option<Res<UiFonts>>) {
    let Some(fonts) = fonts else { return };
    let layout = cluster_layout();
    for cluster in &clusters {
        commands.entity(cluster).with_children(|cluster| {
            cluster.spawn((
                ChosenLine,
                Text::new(""),
                UiFonts::style(&fonts.body, 16.0, BRIGHT),
                Node { position_type: PositionType::Absolute, left: hud(layout.bar.min.x), top: hud(layout.bar.min.y - layout.slot * 0.95), ..default() },
                Pickable::IGNORE,
            ));
        });
    }
}

/// "Divine Light, tier 2, 6 mana, 40 ft (C pray, V tier)" for a caster; nothing for anyone else.
fn show_chosen(chosen: Res<Chosen>, rules: Res<GameRules>, keymap: Res<Keymap>, players: Query<&DelverSpellbook, With<Player>>, mut lines: Query<&mut Text, With<ChosenLine>>) {
    let text = match players.iter().next().and_then(|b| b.0.find(&chosen.name)) {
        Some(w) => {
            let p = w.preview_with(&rules, chosen.tier);
            format!("{}, tier {}, {} mana, {} ft  ({} pray, {} tier)", w.name, chosen.tier, points(p.cost), points(p.range), keymap.label(Action::Pray), keymap.label(Action::CastTier))
        }
        None => String::new(),
    };
    for mut line in &mut lines {
        if line.0 != text {
            line.0 = text.clone();
        }
    }
}

/// Each slot holding a prayer names it and the tier it would cast at.
fn show_slot_labels(slots: Res<AbilitySlots>, chosen: Res<Chosen>, mut labels: Query<(&AbilityLabel, &mut Text)>) {
    for (label, mut text) in &mut labels {
        let want = slots.get(label.0).map_or_else(String::new, |name| format!("{}
{}", name.split(' ').next_back().unwrap_or(name), chosen.tier));
        if text.0 != want {
            text.0 = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::state::app::StatesPlugin;

    use super::*;
    use crate::play::{ScreenOpen, run_delve_systems_while_delving};

    fn delve(class_seed_cleric: bool, mana: f64) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Delving)
            .init_resource::<Keymap>()
            .init_resource::<Quickslots>()
            .init_resource::<Hovered>()
            .init_resource::<Toast>()
            .init_resource::<ScreenOpen>()
            .init_resource::<GameRules>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_plugins(CastingPlugin);
        run_delve_systems_while_delving(&mut app);
        let mut character = Character::roll(1);
        character.class = if class_seed_cleric { crate::character::Class::Cleric } else { crate::character::Class::Fighter };
        app.insert_resource(character);
        let player = app.world_mut().spawn((Player, DelverSpellbook::default(), DelverMana(fb_character::Mana { points: mana, max: 10.0 }))).id();
        app.update();
        (app, player)
    }

    fn pool(app: &App, player: Entity) -> f64 {
        app.world().get::<DelverMana>(player).unwrap().0.points
    }

    fn casted(app: &mut App) -> Vec<Cast> {
        app.world_mut().resource_mut::<Messages<Casted>>().drain().map(|c| c.0).collect()
    }

    fn press(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
    }

    #[test]
    fn a_tier_two_prayer_with_ten_mana_spends_six_and_leaves_four() {
        let (mut app, player) = delve(true, 10.0);
        app.world_mut().resource_mut::<Chosen>().tier = 2;
        press(&mut app, KeyCode::KeyC);
        assert_eq!(pool(&app, player), 4.0);
        let casts = casted(&mut app);
        assert_eq!(casts.len(), 1);
        assert_eq!((casts[0].written.name.as_str(), casts[0].tier, casts[0].cost), ("Divine Light", 2, 6.0));
        assert!(app.world().resource::<Toast>().text.contains("tier 2"));
    }

    #[test]
    fn too_little_mana_fails_and_says_so() {
        let (mut app, player) = delve(true, 5.0);
        app.world_mut().resource_mut::<Chosen>().tier = 2;
        press(&mut app, KeyCode::KeyC);
        assert_eq!(pool(&app, player), 5.0, "nothing spent");
        assert!(casted(&mut app).is_empty());
        assert!(app.world().resource::<Toast>().text.starts_with("Not enough mana"));
    }

    #[test]
    fn only_clerics_pray() {
        let (mut app, player) = delve(false, 10.0);
        press(&mut app, KeyCode::KeyC);
        assert_eq!(pool(&app, player), 10.0);
        assert!(casted(&mut app).is_empty());
        assert!(app.world().resource::<Toast>().text.starts_with("You cannot cast"));
    }

    #[test]
    fn the_tier_key_steps_one_to_three_and_round() {
        let (mut app, _) = delve(true, 10.0);
        let mut seen = vec![];
        for _ in 0..4 {
            press(&mut app, KeyCode::KeyV);
            seen.push(app.world().resource::<Chosen>().tier);
        }
        assert_eq!(seen, vec![2, 3, 1, 2]);
        assert!(app.world().resource::<Toast>().text.contains("6 mana"));
    }

    #[test]
    fn a_cleric_holds_divine_light_in_a_slot_and_its_key_casts_it() {
        let (mut app, player) = delve(true, 10.0);
        assert_eq!(app.world().resource::<AbilitySlots>().get(0), Some("Divine Light"));
        press(&mut app, KeyCode::Digit1);
        assert_eq!(pool(&app, player), 7.0);
        assert_eq!(casted(&mut app).len(), 1);
    }

    #[test]
    fn a_fighter_gets_no_prayer_in_a_slot() {
        let (app, _) = delve(false, 10.0);
        assert_eq!(app.world().resource::<AbilitySlots>().get(0), None);
    }

    #[test]
    fn an_item_bound_to_the_slot_takes_it_from_the_ability() {
        let (mut app, _) = delve(true, 10.0);
        app.world_mut().resource_mut::<Quickslots>().bind(0, crate::inventory::ItemId(9));
        app.update();
        assert_eq!(app.world().resource::<AbilitySlots>().get(0), None);
    }

    #[test]
    fn a_slot_holds_an_item_or_a_prayer_and_binding_refuses_to_clear_an_item() {
        let (mut app, _) = delve(true, 10.0);
        app.world_mut().resource_mut::<Quickslots>().bind(2, crate::inventory::ItemId(9));
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(crate::play::Screen::Inventory);
        press(&mut app, KeyCode::Digit3);
        assert_eq!(app.world().resource::<AbilitySlots>().get(2), None, "refused over an item");
        assert_eq!(app.world().resource::<Quickslots>().get(2), Some(crate::inventory::ItemId(9)));
        assert!(app.world().resource::<Toast>().text.contains("already holds an item"));
        press(&mut app, KeyCode::Digit4);
        assert_eq!(app.world().resource::<AbilitySlots>().get(3), Some("Divine Light"));
        assert_eq!(app.world().resource::<AbilitySlots>().get(0), None, "a prayer sits in one slot only");
    }

    #[test]
    fn the_chosen_prayer_follows_the_book_and_a_late_character_fills_it() {
        let (mut app, player) = delve(false, 10.0);
        assert_eq!(app.world().resource::<Chosen>().name, "", "a fighter chooses nothing");
        app.world_mut().resource_mut::<Character>().class = crate::character::Class::Cleric;
        app.update();
        assert_eq!(app.world().get::<DelverSpellbook>(player).unwrap().0.written.len(), 1);
        assert_eq!(app.world().resource::<Chosen>().name, "Divine Light");
    }
}
