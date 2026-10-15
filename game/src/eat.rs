//! Eating and drinking from the inventory (#228). With the screen open, U eats or drinks the
//! food or drink under the mouse: the item is used up and its nutrition and water come off the
//! delver's hunger and thirst ([`fb_items::Provision`], `docs/items.md`). The inventory keeps no
//! counts, so a stack of bread is several items and one is eaten at a time.
//!
//! [`consume`] does the work and says what it did, so the healing draught can add its cure to
//! it (#235). The delver's needs are the [`DelverNeeds`] on the player; the game clock steps
//! them in #229.

use bevy::prelude::*;
use fb_items::{Consume, Provision};

use crate::character::DelverNeeds;
use crate::inventory::{Hovered, Inventory, ItemId, Toast};
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, Player, Screen, ScreenOpen};

pub struct EatPlugin;

impl Plugin for EatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, consume_hovered.run_if(inventory_open).in_set(DelveSystems));
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
    mut toast: ResMut<Toast>,
    time: Option<Res<Time<Real>>>,
) {
    if !input.just_pressed(Action::Consume) {
        return;
    }
    let (Some((id, _)), Ok(mut needs)) = (hovered.0, needs.single_mut()) else { return };
    if let Some(done) = consume(&mut inventory, &mut needs.0, id) {
        toast.say(say(&done), time.map_or(0.0, |t| t.elapsed_secs()));
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
