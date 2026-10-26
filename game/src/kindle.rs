//! Lighting and putting out torches and lanterns ([`armory::kindle`]). In the level, L acts on
//! the light carried: the main hand's, else the off hand's, else the lantern at the hip; with
//! Shift, the hip lantern alone. With the inventory open, L acts on the light under the mouse,
//! wherever it lies. The toast says what happened, or that a spent torch will not light.
//! A flask of lamp oil, used from the inventory or a quickslot, fills the lanterns held or at
//! the hip ([`refill_and_say`]), so an empty one lights again.

use armory::kindle::Unlit;
use bevy::prelude::*;
use fb_items::Slot;

use crate::game_clock::{GameClock, advance_game_clock};
use crate::inventory::{Hovered, Inventory, ItemId, Toast};
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, inventory_open, level_has_focus};

pub struct KindlePlugin;

impl Plugin for KindlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            burn_fuel.after(advance_game_clock).in_set(DelveSystems),
        );
        app.add_systems(
            Update,
            (
                light_carried.run_if(level_has_focus),
                light_hovered
                    .run_if(inventory_open)
                    .run_if(crate::barter::not_bartering),
            )
                .in_set(DelveSystems),
        );
    }
}

/// The carried light L acts on: the main hand's torch or lantern, else the off hand's, else
/// the hip lantern; the hip lantern alone with `hip` (Shift). Anything else in a hand is
/// passed over.
pub fn light_to_toggle(inventory: &Inventory, hip: bool) -> Option<ItemId> {
    let slots: &[Slot] = if hip {
        &[Slot::Hip]
    } else {
        &[Slot::Mainhand, Slot::Offhand, Slot::Hip]
    };
    slots
        .iter()
        .filter_map(|&slot| inventory.worn(slot))
        .find(|&id| {
            inventory
                .item(id)
                .is_some_and(|item| item.shape.lit().is_some())
        })
}

/// Lights `id` or puts it out, and says so.
pub fn toggle_and_say(inventory: &mut Inventory, id: ItemId) -> String {
    let what = inventory.item(id).map_or("light".to_string(), |item| {
        item.shape.family().key().to_string()
    });
    match inventory.toggle_light(id) {
        Ok(true) => format!("You light the {what}"),
        Ok(false) => format!("You put out the {what}"),
        Err(Unlit::Spent) => format!("The {what} is spent and will not light"),
        Err(Unlit::Empty) => format!("The {what} is out of fuel and will not light"),
        Err(Unlit::NotALight) => format!("The {what} cannot be lit"),
    }
}

/// Fills the lanterns held or at the hip from the lamp-oil flask `flask`
/// ([`Inventory::refill_lanterns`]), and says so: that they are filled, that the flask ran out
/// first, or that there is no lantern to fill or room in it.
pub fn refill_and_say(inventory: &mut Inventory, flask: ItemId) -> String {
    let lanterns = inventory.equipped_lanterns();
    let what = if lanterns.len() > 1 {
        "lanterns"
    } else {
        "lantern"
    };
    if lanterns.is_empty() {
        return "You hold no lantern to fill".to_string();
    }
    if !inventory.item(flask).is_some_and(fb_items::Item::has_oil) {
        return "The flask is empty".to_string();
    }
    let filled = inventory.refill_lanterns(flask);
    let what_filled = if filled.len() > 1 {
        "lanterns"
    } else {
        "lantern"
    };
    if filled.is_empty() {
        format!(
            "Your {what} {} already full",
            if lanterns.len() > 1 { "are" } else { "is" }
        )
    } else if inventory.lantern_wants_oil() {
        format!("You pour the last of the oil into the {what_filled}")
    } else {
        format!("You fill the {what_filled}")
    }
}

/// Burns every lit light's fuel by the game time that passed since the last step, wherever
/// the light is.
fn burn_fuel(
    clock: Res<GameClock>,
    mut inventory: ResMut<Inventory>,
    mut last: Local<f64>,
    mut jumps: Local<u32>,
) {
    let now = clock.game_seconds();
    // A clock set at a stroke (a delve resumed) burns nothing for the time it skipped.
    let jumped = std::mem::replace(&mut *jumps, clock.jumps()) != clock.jumps();
    let passed = if jumped { 0.0 } else { now - *last };
    *last = now;
    if passed > 0.0 {
        // Burning is not a change anyone sees until a card's minute, a state or a held torch's
        // char moves, so only then is the inventory marked changed.
        if inventory
            .bypass_change_detection()
            .burn_lights(passed as f32)
        {
            inventory.set_changed();
        }
    }
}

fn now(time: Option<Res<Time<Real>>>) -> f32 {
    time.map_or(0.0, |t| t.elapsed_secs())
}

/// L in the level: the carried light ([`light_to_toggle`]), Shift+L the hip lantern.
fn light_carried(
    input: ActionInput,
    mut inventory: ResMut<Inventory>,
    mut toast: ResMut<Toast>,
    time: Option<Res<Time<Real>>>,
) {
    if !input.just_pressed(Action::Light) {
        return;
    }
    let hip = input.pressed(Action::Sprint);
    match light_to_toggle(&inventory, hip) {
        Some(id) => {
            let said = toggle_and_say(&mut inventory, id);
            toast.say(said, now(time));
        }
        None => toast.say(
            if hip {
                "No lantern hangs at your hip"
            } else {
                "You carry no torch or lantern"
            },
            now(time),
        ),
    }
}

/// L in the inventory: the torch or lantern under the mouse, wherever it lies.
fn light_hovered(
    input: ActionInput,
    hovered: Res<Hovered>,
    mut inventory: ResMut<Inventory>,
    mut toast: ResMut<Toast>,
    time: Option<Res<Time<Real>>>,
) {
    if !input.just_pressed(Action::Light) {
        return;
    }
    let Some((id, _)) = hovered.0 else { return };
    if inventory
        .item(id)
        .is_some_and(|item| item.shape.lit().is_some())
    {
        let said = toggle_and_say(&mut inventory, id);
        toast.say(said, now(time));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::{Screen, ScreenOpen};
    use armory::families::torch::BurnState;

    fn torch(state: BurnState) -> fb_items::Item {
        let mut item = fb_items::roll_item("Torch", 7).unwrap();
        if let armory::Item::Torch(s) = &mut item.shape {
            s.burn.state = state;
        }
        item
    }

    fn lantern(lit: bool) -> fb_items::Item {
        let mut item = fb_items::roll_item("Lantern", 7).unwrap();
        item.shape.set_lit(lit).unwrap();
        item
    }

    fn equip(inventory: &mut Inventory, item: fb_items::Item, slot: Slot) -> ItemId {
        let id = inventory.add(item);
        inventory.equip(id, slot).unwrap();
        id
    }

    fn lit(inventory: &Inventory, id: ItemId) -> Option<bool> {
        inventory.item(id).and_then(|i| i.shape.lit())
    }

    #[test]
    fn l_acts_on_the_hand_first_then_the_hip() {
        let mut inventory = Inventory::default();
        equip(
            &mut inventory,
            fb_items::roll_item("Belt", 1).unwrap(),
            Slot::Belt,
        );
        let hip = equip(&mut inventory, lantern(true), Slot::Hip);
        assert_eq!(
            light_to_toggle(&inventory, false),
            Some(hip),
            "only the hip lantern"
        );
        let hand = equip(&mut inventory, torch(BurnState::Burning), Slot::Offhand);
        assert_eq!(
            light_to_toggle(&inventory, false),
            Some(hand),
            "the hand first"
        );
        assert_eq!(
            light_to_toggle(&inventory, true),
            Some(hip),
            "Shift: the hip"
        );
    }

    #[test]
    fn l_puts_a_held_torch_out_and_lights_it_again() {
        let mut inventory = Inventory::default();
        let id = equip(&mut inventory, torch(BurnState::Burning), Slot::Offhand);
        assert_eq!(toggle_and_say(&mut inventory, id), "You put out the torch");
        assert!(
            matches!(&inventory.item(id).unwrap().shape, armory::Item::Torch(s) if s.burn.state == BurnState::Doused)
        );
        assert!(crate::carry::item_light(&inventory.item(id).unwrap().shape).is_none());
        assert_eq!(toggle_and_say(&mut inventory, id), "You light the torch");
        assert_eq!(lit(&inventory, id), Some(true));
    }

    #[test]
    fn a_spent_torch_refuses() {
        let mut inventory = Inventory::default();
        let id = equip(&mut inventory, torch(BurnState::Spent), Slot::Offhand);
        assert_eq!(
            toggle_and_say(&mut inventory, id),
            "The torch is spent and will not light"
        );
        assert_eq!(lit(&inventory, id), Some(false));
    }

    /// A flask of lamp oil holding `fill` of a full lantern.
    fn oil(fill: f32) -> fb_items::Item {
        let mut flask = fb_items::roll_item(fb_items::LAMP_OIL, 7).unwrap();
        if let armory::Item::Food(s) = &mut flask.shape {
            s.provision.fill = fill;
        }
        flask
    }

    /// A lantern with `share` of a full one's oil, out.
    fn lantern_at(share: f32) -> fb_items::Item {
        let mut item = lantern(true);
        item.burn(1.0e9);
        let capacity = item.fuel_capacity().unwrap();
        item.shape.refuel(capacity * share, capacity);
        item
    }

    fn share(inventory: &Inventory, id: ItemId) -> f32 {
        let item = inventory.item(id).unwrap();
        item.fuel_left().unwrap() / item.fuel_capacity().unwrap()
    }

    #[test]
    fn lamp_oil_fills_an_empty_held_lantern_so_it_lights_again() {
        let mut inventory = Inventory::default();
        let id = equip(&mut inventory, lantern_at(0.0), Slot::Offhand);
        assert_eq!(
            toggle_and_say(&mut inventory, id),
            "The lantern is out of fuel and will not light"
        );
        let flask = inventory.add(oil(1.0));
        assert_eq!(
            refill_and_say(&mut inventory, flask),
            "You fill the lantern"
        );
        assert!((share(&inventory, id) - 1.0).abs() < 1e-6);
        assert!(!inventory.item(flask).unwrap().has_oil());
        assert_eq!(toggle_and_say(&mut inventory, id), "You light the lantern");
        assert_eq!(refill_and_say(&mut inventory, flask), "The flask is empty");
    }

    #[test]
    fn lamp_oil_fills_the_hand_first_then_the_hip_until_it_runs_out() {
        let mut inventory = Inventory::default();
        equip(
            &mut inventory,
            fb_items::roll_item("Belt", 1).unwrap(),
            Slot::Belt,
        );
        let hip = equip(&mut inventory, lantern_at(0.0), Slot::Hip);
        let hand = equip(&mut inventory, lantern_at(0.5), Slot::Mainhand);
        let flask = inventory.add(oil(0.8));
        assert_eq!(
            refill_and_say(&mut inventory, flask),
            "You pour the last of the oil into the lanterns"
        );
        assert!(
            (share(&inventory, hand) - 1.0).abs() < 1e-4,
            "the hand first"
        );
        assert!(
            (share(&inventory, hip) - 0.3).abs() < 1e-4,
            "the hip takes the rest"
        );
        assert!(!inventory.item(flask).unwrap().has_oil());
    }

    #[test]
    fn lamp_oil_needs_a_lantern_with_room() {
        let mut inventory = Inventory::default();
        let flask = inventory.add(oil(1.0));
        assert_eq!(
            refill_and_say(&mut inventory, flask),
            "You hold no lantern to fill"
        );
        let bagged = inventory.add(lantern_at(0.0));
        assert_eq!(
            refill_and_say(&mut inventory, flask),
            "You hold no lantern to fill",
            "a lantern on the floor or in a bag is not filled"
        );
        assert_eq!(share(&inventory, bagged), 0.0);
        equip(&mut inventory, lantern_at(1.0), Slot::Offhand);
        assert_eq!(
            refill_and_say(&mut inventory, flask),
            "Your lantern is already full"
        );
        assert!(inventory.item(flask).unwrap().has_oil());
    }

    #[test]
    fn a_bagged_lantern_lights_and_keeps_its_state() {
        let mut inventory = Inventory::default();
        let id = inventory.add(lantern(false));
        assert_eq!(toggle_and_say(&mut inventory, id), "You light the lantern");
        // Its state is the item's own, so it goes wherever the item goes.
        let carried = inventory.item(id).unwrap().clone();
        let mut elsewhere = Inventory::default();
        let again = elsewhere.add(carried);
        assert_eq!(lit(&elsewhere, again), Some(true));
    }

    mod fuel {
        use std::time::Duration;

        use bevy::state::app::StatesPlugin;

        use super::*;
        use crate::game_clock::GameClockPlugin;
        use crate::launch::AppState;
        use crate::play::run_delve_systems_while_delving;
        use crate::tuning::Tuning;

        fn app() -> App {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, StatesPlugin))
                .insert_state(AppState::Delving)
                .init_resource::<Tuning>()
                .init_resource::<ScreenOpen>()
                .init_resource::<Inventory>()
                .add_plugins(GameClockPlugin)
                .add_systems(
                    FixedUpdate,
                    burn_fuel.after(advance_game_clock).in_set(DelveSystems),
                );
            run_delve_systems_while_delving(&mut app);
            app.update();
            app.update();
            app
        }

        /// Runs `seconds` of fixed time, one second per step; at the default ratio of 12, five
        /// minutes of it are a game hour.
        fn run(app: &mut App, seconds: u32) {
            for _ in 0..seconds {
                app.world_mut()
                    .resource_mut::<Time<Fixed>>()
                    .advance_by(Duration::from_secs(1));
                app.world_mut().run_schedule(FixedUpdate);
            }
        }

        fn fresh_torch(burn_time: f32) -> fb_items::Item {
            let mut item = fb_items::roll_item("Spare torch", 3).unwrap();
            item.profile.stats.burn_time = burn_time;
            item.shape.scale_fuel(burn_time);
            item
        }

        fn light(app: &mut App, item: fb_items::Item, lit: bool) -> ItemId {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let id = inventory.add(item);
            inventory.toggle_light(id).unwrap();
            if !lit {
                inventory.toggle_light(id).unwrap();
            }
            id
        }

        fn left(app: &App, id: ItemId) -> f32 {
            app.world()
                .resource::<Inventory>()
                .item(id)
                .unwrap()
                .fuel_left()
                .unwrap()
        }

        fn spent(app: &App, id: ItemId) -> bool {
            matches!(&app.world().resource::<Inventory>().item(id).unwrap().shape, armory::Item::Torch(s) if s.burn.state == BurnState::Spent)
        }

        /// A clock set at a stroke, as a resumed delve sets it, burns nothing for the time it skipped:
        /// a torch lit on the floor is as long as it was.
        #[test]
        fn a_clock_set_at_a_stroke_burns_no_fuel_for_the_time_it_skipped() {
            let mut app = app();
            let id = light(&mut app, fresh_torch(1.0), true);
            run(&mut app, 2);
            let before = left(&app, id);
            app.world_mut()
                .resource_mut::<crate::game_clock::GameClock>()
                .jump_to(fb_character::GameTime::from_seconds(30.0 * 3600.0));
            run(&mut app, 1);
            assert!(
                before - left(&app, id) < 30.0,
                "{before} then {}",
                left(&app, id)
            );
            assert!(!spent(&app, id));
            run(&mut app, 100);
            assert!(left(&app, id) < before - 100.0, "and it burns on after");
        }

        #[test]
        fn a_plain_torch_is_spent_after_a_game_hour() {
            let mut app = app();
            let id = light(&mut app, fresh_torch(1.0), true);
            run(&mut app, 290);
            assert!(!spent(&app, id), "ten game minutes left");
            run(&mut app, 12);
            assert!(spent(&app, id));
            assert_eq!(left(&app, id), 0.0);
        }

        #[test]
        fn a_long_burning_torch_burns_for_two_game_hours() {
            let mut app = app();
            let id = light(&mut app, fresh_torch(2.0), true);
            run(&mut app, 300);
            assert!(!spent(&app, id), "still burning at an hour");
            assert!(
                (left(&app, id) - 3_600.0).abs() < 60.0,
                "{}",
                left(&app, id)
            );
            run(&mut app, 305);
            assert!(spent(&app, id));
        }

        #[test]
        fn dousing_at_twenty_minutes_and_relighting_leaves_forty() {
            let mut app = app();
            let id = light(&mut app, fresh_torch(1.0), true);
            run(&mut app, 100);
            app.world_mut()
                .resource_mut::<Inventory>()
                .toggle_light(id)
                .unwrap();
            run(&mut app, 300);
            app.world_mut()
                .resource_mut::<Inventory>()
                .toggle_light(id)
                .unwrap();
            let minutes = left(&app, id) / 60.0;
            assert!((minutes - 40.0).abs() < 0.5, "{minutes}");
        }

        #[test]
        fn a_lit_torch_on_the_floor_burns_and_a_doused_one_in_a_bag_does_not() {
            let mut app = app();
            let floor = light(&mut app, fresh_torch(1.0), true);
            let bagged = light(&mut app, fresh_torch(1.0), false);
            {
                let mut inventory = app.world_mut().resource_mut::<Inventory>();
                let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
                inventory.wear(pack, Slot::Back).unwrap();
                inventory.stow(bagged).unwrap();
                assert!(matches!(
                    inventory.location(bagged),
                    Some(crate::inventory::Location::In { .. })
                ));
            }
            run(&mut app, 150);
            assert!(
                (left(&app, floor) - 1_800.0).abs() < 60.0,
                "{}",
                left(&app, floor)
            );
            assert_eq!(left(&app, bagged), 3_600.0);
        }

        #[test]
        fn burning_marks_the_inventory_changed_only_when_something_shown_changes() {
            let mut app = app();
            let changed = |app: &mut App| app.world_mut().resource_ref::<Inventory>().is_changed();
            let settle = |app: &mut App| {
                app.world_mut().clear_trackers();
            };
            run(&mut app, 5);
            settle(&mut app);
            run(&mut app, 1);
            assert!(!changed(&mut app), "nothing lit");
            let id = light(&mut app, fresh_torch(1.0), true);
            settle(&mut app);
            // One step is 12 game seconds: the minute (and the char bucket) hold for a few.
            run(&mut app, 1);
            assert!(!changed(&mut app), "a lit torch within the same minute");
            let minute = |app: &App| (left(app, id) / 60.0).round() as u32;
            let start = minute(&app);
            let mut crossed = false;
            for _ in 0..20 {
                settle(&mut app);
                run(&mut app, 1);
                let now = minute(&app);
                if now != start {
                    assert!(changed(&mut app), "crossed a minute");
                    crossed = true;
                    break;
                }
                assert!(!changed(&mut app));
            }
            assert!(crossed);
            // Burning the rest down: going spent is a shown change.
            settle(&mut app);
            run(&mut app, 400);
            assert!(spent(&app, id));
        }

        #[test]
        fn going_spent_marks_the_inventory_changed() {
            let mut app = app();
            let id = light(&mut app, fresh_torch(1.0), true);
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            assert!(
                inventory.bypass_change_detection().burn_lights(1.0e6),
                "spent is a shown change"
            );
            assert!(spent(&app, id));
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            assert!(
                !inventory.bypass_change_detection().burn_lights(10.0),
                "nothing lit now"
            );
        }

        #[test]
        fn a_held_candle_lights_burns_down_and_goes_out_with_l() {
            use fb_items::Slot;
            let mut inventory = Inventory::default();
            let mut candle = (1..200)
                .filter_map(|s| {
                    fb_items::roll_item("Chamberstick", s)
                        .or_else(|| fb_items::roll_item("Candle", s))
                })
                .find(|i| matches!(&i.shape, armory::Item::Sundry(s) if s.article.holder))
                .unwrap();
            if let armory::Item::Sundry(s) = &mut candle.shape {
                s.article.lit = true;
                s.article.burnt = 0.0;
            }
            let id = inventory.add(candle);
            for hand in [Slot::Offhand, Slot::Mainhand] {
                assert!(inventory.item(id).unwrap().fits(hand));
                inventory.equip(id, hand).unwrap();
                assert_eq!(
                    crate::carry::carried_lights(&inventory),
                    vec![(hand, id)],
                    "held lit, it lights"
                );
            }
            let full = inventory.item(id).unwrap().fuel_left().unwrap();
            assert!(inventory.burn_lights(600.0));
            assert!(
                inventory.item(id).unwrap().fuel_left().unwrap() < full,
                "burns down while held"
            );
            assert_eq!(inventory.toggle_light(id), Ok(false));
            assert!(
                crate::carry::carried_lights(&inventory).is_empty(),
                "out, it gives no light"
            );
            let left = inventory.item(id).unwrap().fuel_left();
            inventory.burn_lights(600.0);
            assert_eq!(inventory.item(id).unwrap().fuel_left(), left);
        }

        #[test]
        fn an_open_screen_burns_nothing() {
            let mut app = app();
            let id = light(&mut app, fresh_torch(1.0), true);
            app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
            run(&mut app, 300);
            assert_eq!(left(&app, id), 3_600.0);
        }

        #[test]
        fn an_empty_lantern_will_not_light() {
            let mut inventory = Inventory::default();
            let mut item = fb_items::roll_item("Lantern", 7).unwrap();
            item.shape.set_lit(true).unwrap();
            item.burn(1.0e9);
            let id = inventory.add(item);
            assert_eq!(
                toggle_and_say(&mut inventory, id),
                "The lantern is out of fuel and will not light"
            );
        }
    }
}
