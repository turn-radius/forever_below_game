//! Lighting and putting out torches and lanterns ([`armory::kindle`]). In the level, L acts on
//! the light carried: the main hand's, else the off hand's, else the lantern at the hip; with
//! Shift, the hip lantern alone. With the inventory open, L acts on the light under the mouse,
//! wherever it lies. The toast says what happened, or that a spent torch will not light.

use armory::kindle::Unlit;
use bevy::prelude::*;
use fb_items::Slot;

use crate::inventory::{Hovered, Inventory, ItemId, Toast};
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, Screen, ScreenOpen, level_has_focus};

pub struct KindlePlugin;

impl Plugin for KindlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (light_carried.run_if(level_has_focus), light_hovered.run_if(inventory_open)).in_set(DelveSystems));
    }
}

fn inventory_open(open: Option<Res<ScreenOpen>>) -> bool {
    open.is_some_and(|o| o.is(Screen::Inventory))
}

/// The carried light L acts on: the main hand's torch or lantern, else the off hand's, else
/// the hip lantern; the hip lantern alone with `hip` (Shift). Anything else in a hand is
/// passed over.
pub fn light_to_toggle(inventory: &Inventory, hip: bool) -> Option<ItemId> {
    let slots: &[Slot] = if hip { &[Slot::Hip] } else { &[Slot::Mainhand, Slot::Offhand, Slot::Hip] };
    slots.iter().filter_map(|&slot| inventory.worn(slot)).find(|&id| inventory.item(id).is_some_and(|item| item.shape.lit().is_some()))
}

/// Lights `id` or puts it out, and says so.
pub fn toggle_and_say(inventory: &mut Inventory, id: ItemId) -> String {
    let what = inventory.item(id).map_or("light".to_string(), |item| item.shape.family().key().to_string());
    match inventory.toggle_light(id) {
        Ok(true) => format!("You light the {what}"),
        Ok(false) => format!("You put out the {what}"),
        Err(Unlit::Spent) => format!("The {what} is spent and will not light"),
        Err(Unlit::NotALight) => format!("The {what} cannot be lit"),
    }
}

fn now(time: Option<Res<Time<Real>>>) -> f32 {
    time.map_or(0.0, |t| t.elapsed_secs())
}

/// L in the level: the carried light ([`light_to_toggle`]), Shift+L the hip lantern.
fn light_carried(input: ActionInput, mut inventory: ResMut<Inventory>, mut toast: ResMut<Toast>, time: Option<Res<Time<Real>>>) {
    if !input.just_pressed(Action::Light) {
        return;
    }
    let hip = input.pressed(Action::Hurry);
    match light_to_toggle(&inventory, hip) {
        Some(id) => {
            let said = toggle_and_say(&mut inventory, id);
            toast.say(said, now(time));
        }
        None => toast.say(if hip { "No lantern hangs at your hip" } else { "You carry no torch or lantern" }, now(time)),
    }
}

/// L in the inventory: the torch or lantern under the mouse, wherever it lies.
fn light_hovered(input: ActionInput, hovered: Res<Hovered>, mut inventory: ResMut<Inventory>, mut toast: ResMut<Toast>, time: Option<Res<Time<Real>>>) {
    if !input.just_pressed(Action::Light) {
        return;
    }
    let Some((id, _)) = hovered.0 else { return };
    if inventory.item(id).is_some_and(|item| item.shape.lit().is_some()) {
        let said = toggle_and_say(&mut inventory, id);
        toast.say(said, now(time));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        equip(&mut inventory, fb_items::roll_item("Belt", 1).unwrap(), Slot::Belt);
        let hip = equip(&mut inventory, lantern(true), Slot::Hip);
        assert_eq!(light_to_toggle(&inventory, false), Some(hip), "only the hip lantern");
        let hand = equip(&mut inventory, torch(BurnState::Burning), Slot::Offhand);
        assert_eq!(light_to_toggle(&inventory, false), Some(hand), "the hand first");
        assert_eq!(light_to_toggle(&inventory, true), Some(hip), "Shift: the hip");
    }

    #[test]
    fn l_puts_a_held_torch_out_and_lights_it_again() {
        let mut inventory = Inventory::default();
        let id = equip(&mut inventory, torch(BurnState::Burning), Slot::Offhand);
        assert_eq!(toggle_and_say(&mut inventory, id), "You put out the torch");
        assert!(matches!(&inventory.item(id).unwrap().shape, armory::Item::Torch(s) if s.burn.state == BurnState::Doused));
        assert!(crate::carry::item_light(&inventory.item(id).unwrap().shape).is_none());
        assert_eq!(toggle_and_say(&mut inventory, id), "You light the torch");
        assert_eq!(lit(&inventory, id), Some(true));
    }

    #[test]
    fn a_spent_torch_refuses() {
        let mut inventory = Inventory::default();
        let id = equip(&mut inventory, torch(BurnState::Spent), Slot::Offhand);
        assert_eq!(toggle_and_say(&mut inventory, id), "The torch is spent and will not light");
        assert_eq!(lit(&inventory, id), Some(false));
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
}
