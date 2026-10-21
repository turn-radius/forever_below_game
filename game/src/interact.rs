//! What the four action slots do on whatever the delver is aiming at. One lookup,
//! [`verbs_for`], gives a target's verb for each of Primary, Secondary, Tertiary and Special
//! from the target and the delver's state; the prompts and the input both read it, so what the
//! prompt says is what the key does. An action that cannot be done right now is `None`, never
//! greyed. Compare (Alt) is not a slot: [`Verbs::compare`] only says whether the target can be
//! compared, for an "Alt Compare" prompt.
//!
//! | Target | Primary | Secondary | Tertiary | Special |
//! |---|---|---|---|---|
//! | Item on the ground, underfoot or in a container not carried | Take | Equip | | Hang at hip |
//! | Item carried | its use (Eat, Drink, Light, Put out, Refill) | Equip, or Take off if worn | | Hang at hip |
//! | Door | Open or Close | Lock or Unlock, holding its key | Pick the lock, holding lockpicks | |
//! | Container | Open, or Look in once it stands open | Lock or Unlock, holding its key | Pick the lock, holding lockpicks | |
//! | Folk | Talk | Barter | Steal, while hidden | |
//! | Stair | Go down or Go up | | | |
//!
//! Hang at hip sends something that can hang from the belt (a lantern) straight to the hip,
//! where Equip would put it in a hand. It is on Special because Tertiary's key turns a dragged
//! item in the inventory.
//!
//! The core is pure: a [`Target`] and a [`Delver`] in, [`Verbs`] out. [`item_target`] builds an
//! item's target from the real inventory. Not wired yet, because the game has none of it: doors
//! that open (only the entrance door exists), containers in the level, locks, keys, lockpicks
//! and crouching out of sight. Those are plain fields ([`Lock`], [`Delver`]) with no gameplay
//! behind them; whatever adds them fills the fields in. Steal is only offered, nothing steals.
//!
//! The slots are not bound to keys here; [`Slot::action`] maps a [`Slot`] to the keymap's action,
//! and [`key_line`] writes a target's verbs with the keys bound to them: "F Open · R Pick
//! the lock". Aimed at a container within reach, or at a stair with nothing else aimed at
//! (`crate::travel`), [`CrosshairPrompt`] shows that line over the crosshair
//! ([`InteractPlugin`]); a folk's line goes on its card (`crate::folk`). The doors on the edge
//! of a faction's ground have a prompt of their own (`crate::doors`): "F Open", or "Locked", with
//! "G Unlock with key" and "R Pick the lock" for what the delver carries. The entrance never
//! opens for the delver, so it has none.

use bevy::{prelude::*, ui::ComputedUiRenderTargetInfo};
use fb_items::Item;

use crate::carry::ViewModelCamera;
use crate::inventory::{Inventory, ItemId, Location};
use crate::inventory::card::{CARD_GAP, keep_on_screen};
use crate::keymap::{Action, Keymap};
use crate::play::{DelveSystems, ScreenOpen};
use crate::ui::{BG, LINE, MUTED, UiFonts};

/// The four action slots, in the order they are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Primary,
    Secondary,
    Tertiary,
    Special,
}

impl Slot {
    pub const ALL: [Slot; 4] = [Slot::Primary, Slot::Secondary, Slot::Tertiary, Slot::Special];

    /// The slot an action fires; `None` for every other action, Compare included.
    pub fn of(action: Action) -> Option<Slot> {
        match action {
            Action::Primary => Some(Slot::Primary),
            Action::Secondary => Some(Slot::Secondary),
            Action::Tertiary => Some(Slot::Tertiary),
            Action::Special => Some(Slot::Special),
            _ => None,
        }
    }

    /// The action that fires this slot.
    pub fn action(self) -> Action {
        match self {
            Slot::Primary => Action::Primary,
            Slot::Secondary => Action::Secondary,
            Slot::Tertiary => Action::Tertiary,
            Slot::Special => Action::Special,
        }
    }
}

/// What a slot can do to a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Verb {
    Take,
    Equip,
    TakeOff,
    Eat,
    Drink,
    Light,
    PutOut,
    Refill,
    Read,
    HangAtHip,
    Open,
    LookIn,
    Close,
    Pull,
    Lock,
    Unlock,
    PickLock,
    Talk,
    Barter,
    Steal,
    GoDown,
    GoUp,
}

impl Verb {
    /// The name a prompt shows.
    pub fn name(self) -> &'static str {
        match self {
            Verb::Take => "Take",
            Verb::Equip => "Equip",
            Verb::TakeOff => "Take off",
            Verb::Eat => "Eat",
            Verb::Drink => "Drink",
            Verb::Light => "Light",
            Verb::PutOut => "Put out",
            Verb::Refill => "Refill",
            Verb::Read => "Read",
            Verb::HangAtHip => "Hang at hip",
            Verb::Open => "Open",
            Verb::LookIn => "Look in",
            Verb::Close => "Close",
            Verb::Pull => "Pull the lever",
            Verb::Lock => "Lock",
            Verb::Unlock => "Unlock with key",
            Verb::PickLock => "Pick the lock",
            Verb::Talk => "Talk",
            Verb::Barter => "Barter",
            Verb::Steal => "Steal",
            Verb::GoDown => "Go down",
            Verb::GoUp => "Go up",
        }
    }
}

/// A target's verb for each slot, and whether it can be compared.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Verbs {
    pub primary: Option<Verb>,
    pub secondary: Option<Verb>,
    pub tertiary: Option<Verb>,
    pub special: Option<Verb>,
    /// Whether Alt compares it with what is worn: items that can be worn or held.
    pub compare: bool,
}

impl Verbs {
    pub fn get(&self, slot: Slot) -> Option<Verb> {
        match slot {
            Slot::Primary => self.primary,
            Slot::Secondary => self.secondary,
            Slot::Tertiary => self.tertiary,
            Slot::Special => self.special,
        }
    }

    /// The slots that have a verb, with it, in slot order.
    pub fn available(&self) -> Vec<(Slot, Verb)> {
        Slot::ALL.into_iter().filter_map(|slot| Some((slot, self.get(slot)?))).collect()
    }
}

/// The key and verb pairs a prompt or card shows for `verbs`, in order Primary, Secondary,
/// Tertiary, Special, then Compare: ("F", "Take"). A key comes from the keymap, the first input
/// of the slot's action, so a rebind changes it. Verbs that cannot be done, and actions with no
/// key, are left out.
pub fn key_actions(keymap: &Keymap, verbs: &Verbs) -> Vec<(String, &'static str)> {
    let key = |action| keymap.inputs(action).first().map(|input| input.label());
    let mut pairs: Vec<(String, &'static str)> = verbs.available().into_iter().filter_map(|(slot, verb)| Some((key(slot.action())?, verb.name()))).collect();
    if verbs.compare {
        pairs.extend(key(Action::Compare).map(|k| (k, "Compare")));
    }
    pairs
}

/// [`key_actions`] as one line: "F Take \u{b7} G Equip \u{b7} Alt Compare". Empty if there are none.
pub fn key_line(keymap: &Keymap, verbs: &Verbs) -> String {
    key_actions(keymap, verbs).iter().map(|(key, name)| format!("{key} {name}")).collect::<Vec<_>>().join(" \u{b7} ")
}

/// Where an item is, as far as its verbs go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// Lying in the level.
    Ground,
    /// In a bag or on the delver, not worn.
    Carried,
    /// Worn or held.
    Worn,
}

/// A door's or container's lock. The game has no locks yet, so [`Lock::NONE`] is every one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lock {
    /// Whether it has a lock at all.
    pub has_lock: bool,
    pub locked: bool,
}

impl Lock {
    pub const NONE: Lock = Lock { has_lock: false, locked: false };
    pub const LOCKED: Lock = Lock { has_lock: true, locked: true };
    pub const UNLOCKED: Lock = Lock { has_lock: true, locked: false };
}

/// What is aimed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Item {
        place: Place,
        /// What using it does, if anything: Eat, Drink, Light, Put out or Refill.
        uses: Option<Verb>,
        /// Whether it can be worn or held.
        equippable: bool,
        /// Whether it can be hung at the hip now: it hangs there, a belt is worn and it is not
        /// already there.
        hangs: bool,
    },
    Door {
        open: bool,
        lock: Lock,
    },
    Container {
        open: bool,
        lock: Lock,
    },
    Folk,
    /// A wall lever wired to something.
    Switch,
    Stair {
        down: bool,
    },
}

/// What the lookup needs of the delver. Keys, lockpicks and hiding are not in the game yet, so
/// [`Delver::default`] holds none and is in plain sight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delver {
    /// Holding the key to the aimed lock.
    pub has_key: bool,
    pub has_lockpicks: bool,
    /// Crouched out of sight of the aimed folk.
    pub hidden: bool,
}

/// The verbs `target` gives `delver`.
pub fn verbs_for(target: &Target, delver: &Delver) -> Verbs {
    match *target {
        Target::Item { place, uses, equippable, hangs } => {
            let equip = equippable.then_some(Verb::Equip);
            let special = hangs.then_some(Verb::HangAtHip);
            match place {
                Place::Ground => Verbs { primary: Some(Verb::Take), secondary: equip, special, compare: equippable, ..Verbs::default() },
                Place::Carried => Verbs { primary: uses, secondary: equip, special, compare: equippable, ..Verbs::default() },
                Place::Worn => Verbs { primary: uses, secondary: Some(Verb::TakeOff), special, compare: equippable, ..Verbs::default() },
            }
        }
        Target::Door { open, lock } => Verbs { primary: Some(if open { Verb::Close } else { Verb::Open }), ..locks(lock, delver) },
        // A container left standing open is looked in again with the same key.
        Target::Container { open, lock } => Verbs { primary: Some(if open { Verb::LookIn } else { Verb::Open }), ..locks(lock, delver) },
        Target::Switch => Verbs { primary: Some(Verb::Pull), ..Verbs::default() },
        Target::Folk => Verbs { primary: Some(Verb::Talk), secondary: Some(Verb::Barter), tertiary: delver.hidden.then_some(Verb::Steal), ..Verbs::default() },
        Target::Stair { down } => Verbs { primary: Some(if down { Verb::GoDown } else { Verb::GoUp }), ..Verbs::default() },
    }
}

/// The secondary and tertiary verbs of a lock: the key locks or unlocks it, lockpicks open
/// a locked one.
fn locks(lock: Lock, delver: &Delver) -> Verbs {
    if !lock.has_lock {
        return Verbs::default();
    }
    Verbs {
        secondary: delver.has_key.then_some(if lock.locked { Verb::Unlock } else { Verb::Lock }),
        tertiary: (delver.has_lockpicks && lock.locked).then_some(Verb::PickLock),
        ..Verbs::default()
    }
}

/// What using `item` does: eat or drink it, light it or put it out, refill a lantern from it, or
/// read it if it is a book.
pub fn use_of(item: &Item) -> Option<Verb> {
    if item.lesson().is_some() {
        return Some(Verb::Read);
    }
    if let Some(how) = item.consume() {
        return Some(match how {
            fb_items::Consume::Eat => Verb::Eat,
            fb_items::Consume::Drink => Verb::Drink,
        });
    }
    // A waterskin with water in it is drunk from, though drinking leaves it.
    if item.can_sip() {
        return Some(Verb::Drink);
    }
    // A flask of lamp oil fills a lantern; [`item_target`] offers it only while a lantern held
    // or at the hip has room.
    if item.has_oil() {
        return Some(Verb::Refill);
    }
    // Light only a light that would catch: a spent torch or an empty lantern will not.
    item.shape.lit().and_then(|lit| {
        if lit {
            Some(Verb::PutOut)
        } else {
            item.shape.clone().set_lit(true).is_ok().then_some(Verb::Light)
        }
    })
}

/// The target for item `id`, from where the inventory keeps it. An item lying loose is on the
/// ground, aimed at in the level or shown on the inventory's underfoot grid alike, and so is one
/// in a container that is not carried (a chest, a bag set down): both are taken, not used.
/// `None` if the inventory does not know it.
pub fn item_target(inventory: &Inventory, id: ItemId) -> Option<Target> {
    let item = inventory.item(id)?;
    let place = match inventory.location(id)? {
        Location::Worn(_) => Place::Worn,
        Location::In { .. } if inventory.is_carried(id) => Place::Carried,
        Location::In { .. } | Location::Loose => Place::Ground,
    };
    let uses = use_of(item).filter(|&verb| verb != Verb::Refill || inventory.lantern_wants_oil());
    let hangs = inventory.location(id) != Some(Location::Worn(fb_items::Slot::Hip)) && inventory.check_wear(id, fb_items::Slot::Hip).is_ok();
    Some(Target::Item { place, uses, equippable: !item.slots().is_empty(), hangs })
}

/// The line shown over the crosshair for what is aimed at; `None` shows nothing.
#[derive(Resource, Debug, Default, PartialEq)]
pub struct CrosshairPrompt(pub Option<String>);

/// The node that shows the [`CrosshairPrompt`].
#[derive(Component, Debug)]
struct PromptNode;

pub struct InteractPlugin;

impl Plugin for InteractPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CrosshairPrompt>().add_systems(Update, (spawn_prompt, show_prompt.after(crate::props::prompt_aimed_container).after(crate::travel::prompt_stair)).chain().in_set(DelveSystems));
    }
}

/// Spawns the prompt, hidden, once there is a camera drawing over the held torch to draw it
/// (as the item card is).
fn spawn_prompt(mut commands: Commands, nodes: Query<(), With<PromptNode>>, cameras: Query<Entity, With<ViewModelCamera>>) {
    if !nodes.is_empty() {
        return;
    }
    let Some(camera) = cameras.iter().next() else { return };
    commands.spawn((
        Name::new("Crosshair prompt"),
        DespawnOnExit(crate::launch::AppState::Delving),
        PromptNode,
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            padding: UiRect::new(px(12), px(12), px(6), px(7)),
            border: UiRect::all(px(1.5)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(BG),
        BorderColor::all(LINE),
        GlobalZIndex(40),
        Visibility::Hidden,
        Pickable::IGNORE,
        Text::default(),
    ));
}

/// Shows the prompt just above the crosshair (the item card hangs below it) while the level has
/// focus. A prompt whose text changed waits a frame to be measured.
#[allow(clippy::type_complexity)]
fn show_prompt(
    mut commands: Commands,
    prompt: Res<CrosshairPrompt>,
    open: Option<Res<ScreenOpen>>,
    journey: Option<Res<crate::travel::Journey>>,
    fonts: Option<Res<UiFonts>>,
    mut nodes: Query<(Entity, &mut Node, &mut Visibility, &ComputedNode, &ComputedUiRenderTargetInfo), With<PromptNode>>,
    mut shown: Local<Option<String>>,
) {
    let Some(fonts) = fonts else { return };
    let text = prompt.0.clone().filter(|_| !open.is_some_and(|o| o.0.is_some()) && journey.is_none());
    for (entity, mut node, mut visibility, computed, target) in &mut nodes {
        let Some(text) = &text else {
            *visibility = Visibility::Hidden;
            *shown = None;
            continue;
        };
        if shown.as_ref() != Some(text) {
            commands.entity(entity).insert((Text::new(text.clone()), UiFonts::style(&fonts.mono, 13.0, MUTED)));
            *shown = Some(text.clone());
            *visibility = Visibility::Hidden;
            continue;
        }
        let size = computed.size * computed.inverse_scale_factor;
        let screen = target.logical_size();
        let centre = screen * 0.5;
        let at = keep_on_screen(Vec2::new(centre.x - size.x * 0.5, centre.y - size.y - CARD_GAP), size, screen);
        node.left = px(at.x);
        node.top = px(at.y);
        *visibility = Visibility::Inherited;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Input;
    use bevy::input::keyboard::KeyCode;
    use fb_items::roll_item;

    #[test]
    fn the_prompt_names_the_bound_keys() {
        let keymap = Keymap::default();
        let line = |target: Target, delver: Delver| key_line(&keymap, &verbs_for(&target, &delver));
        let strongbox = Target::Container { open: false, lock: Lock::LOCKED };
        assert_eq!(line(strongbox, Delver { has_lockpicks: true, ..Delver::default() }), "F Open \u{b7} R Pick the lock");
        assert_eq!(line(Target::Folk, Delver::default()), "F Talk \u{b7} G Barter");
        assert_eq!(line(Target::Folk, Delver { hidden: true, ..Delver::default() }), "F Talk \u{b7} G Barter \u{b7} R Steal");
        let open = Target::Container { open: true, lock: Lock::NONE };
        assert_eq!(line(open, Delver::default()), "F Look in");
        assert_eq!(line(Target::Stair { down: true }, Delver::default()), "F Go down");
        assert_eq!(line(Target::Stair { down: false }, Delver::default()), "F Go up");
    }

    #[test]
    fn the_prompt_follows_a_rebinding_and_adds_compare() {
        let mut keymap = Keymap::default();
        keymap.bindings.iter_mut().find(|b| b.action == Action::Primary).unwrap().inputs = vec![Input::Key(KeyCode::KeyE)];
        assert_eq!(key_line(&keymap, &verbs_for(&Target::Folk, &Delver::default())), "E Talk \u{b7} G Barter");
        let dagger = roll_item("Dagger", 7).unwrap();
        let on_floor = Target::Item { place: Place::Ground, uses: None, equippable: !dagger.slots().is_empty(), hangs: false };
        assert_eq!(key_line(&Keymap::default(), &verbs_for(&on_floor, &Delver::default())), "F Take \u{b7} G Equip \u{b7} Alt Compare");
    }

    fn target(item: &Item, place: Place) -> Target {
        Target::Item { place, uses: use_of(item), equippable: !item.slots().is_empty(), hangs: false }
    }

    fn carried(base: &str, place: Place) -> Verbs {
        let item = roll_item(base, 7).unwrap_or_else(|| panic!("{base}"));
        verbs_for(&target(&item, place), &Delver::default())
    }

    fn row(v: Verbs) -> (Option<Verb>, Option<Verb>, Option<Verb>, Option<Verb>) {
        (v.primary, v.secondary, v.tertiary, v.special)
    }

    #[test]
    fn a_dagger_on_the_floor_is_taken_or_equipped() {
        let v = carried("Dagger", Place::Ground);
        assert_eq!(row(v), (Some(Verb::Take), Some(Verb::Equip), None, None));
        assert!(v.compare);
        assert_eq!(v.available().len(), 2);
    }

    #[test]
    fn item_target_reads_loose_and_uncarried_items_as_ground() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(roll_item("Pack", 1).unwrap());
        let sack = inventory.add(roll_item("Sack", 1).unwrap());
        let loose = inventory.add(roll_item("Dagger", 7).unwrap());
        let in_sack = inventory.add(roll_item("Dagger", 8).unwrap());
        let at = inventory.grid(sack).unwrap().first_free(inventory.item(in_sack).unwrap().profile.footprint).unwrap();
        inventory.put(in_sack, sack, at).unwrap();
        let place = |inventory: &Inventory, id| match item_target(inventory, id) {
            Some(Target::Item { place, .. }) => place,
            other => panic!("{other:?}"),
        };
        assert_eq!(place(&inventory, loose), Place::Ground);
        assert_eq!(place(&inventory, in_sack), Place::Ground);
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let carried = inventory.add(roll_item("Dagger", 9).unwrap());
        inventory.stow(carried).unwrap();
        assert_eq!(place(&inventory, carried), Place::Carried);
        let worn = inventory.add(roll_item("Ring", 2).unwrap());
        inventory.wear(worn, fb_items::Slot::Ring1).unwrap();
        assert_eq!(place(&inventory, worn), Place::Worn);
    }

    #[test]
    fn a_lantern_hangs_at_the_hip_only_with_a_belt_and_not_already_there() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(roll_item("Pack", 1).unwrap());
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let lantern = inventory.add(roll_item("Lantern", 3).unwrap());
        inventory.stow(lantern).unwrap();
        let verbs = |inventory: &Inventory, id| verbs_for(&item_target(inventory, id).unwrap(), &Delver::default());
        assert_eq!(verbs(&inventory, lantern).special, None, "no belt");
        let belt = inventory.add(roll_item("Belt", 1).unwrap());
        inventory.wear(belt, fb_items::Slot::Belt).unwrap();
        assert_eq!(verbs(&inventory, lantern).special, Some(Verb::HangAtHip));
        assert_eq!(key_line(&Keymap::default(), &verbs(&inventory, lantern)), "F Put out \u{b7} G Equip \u{b7} T Hang at hip \u{b7} Alt Compare");
        inventory.wear(lantern, fb_items::Slot::Offhand).unwrap();
        assert_eq!(verbs(&inventory, lantern).special, Some(Verb::HangAtHip), "from the hand");
        inventory.take_out(lantern);
        inventory.wear(lantern, fb_items::Slot::Hip).unwrap();
        assert_eq!(verbs(&inventory, lantern).special, None, "already there");
        let dagger = inventory.add(roll_item("Dagger", 7).unwrap());
        assert_eq!(verbs(&inventory, dagger).special, None, "a dagger does not hang");
    }

    #[test]
    fn bread_in_a_bag_is_only_eaten() {
        let v = carried("Bread", Place::Carried);
        assert_eq!(row(v), (Some(Verb::Eat), None, None, None));
        assert!(!v.compare);
    }

    #[test]
    fn a_drink_in_a_bag_is_drunk() {
        assert_eq!(carried("Tankard", Place::Carried).primary, Some(Verb::Drink));
    }

    #[test]
    fn a_worn_ring_is_taken_off() {
        let v = carried("Ring", Place::Worn);
        assert_eq!(row(v), (None, Some(Verb::TakeOff), None, None));
        assert!(v.compare);
    }

    #[test]
    fn a_torch_in_a_bag_lights_or_puts_out() {
        let lit = crate::carry::lit_torch(3);
        let v = verbs_for(&target(&lit, Place::Carried), &Delver::default());
        assert_eq!((v.primary, v.secondary), (Some(Verb::PutOut), Some(Verb::Equip)));
        let mut out = lit.clone();
        out.shape.toggle_lit().unwrap();
        assert_eq!(verbs_for(&target(&out, Place::Carried), &Delver::default()).primary, Some(Verb::Light));
    }

    #[test]
    fn a_spent_torch_or_empty_lantern_cannot_be_lit() {
        let mut spent = roll_item("Torch", 3).unwrap();
        let armory::Item::Torch(s) = &mut spent.shape else { panic!("torch") };
        s.burn.state = armory::families::torch::BurnState::Spent;
        assert_eq!(use_of(&spent), None);
        let mut empty = roll_item("Lantern", 3).unwrap();
        let armory::Item::Lantern(s) = &mut empty.shape else { panic!("lantern") };
        s.flame.fuel_left = 0.0;
        s.flame.lit = false;
        assert_eq!(use_of(&empty), None);
        assert_eq!(verbs_for(&target(&spent, Place::Carried), &Delver::default()).primary, None);
    }

    #[test]
    fn a_locked_strongbox_is_picked_with_lockpicks() {
        let strongbox = Target::Container { open: false, lock: Lock::LOCKED };
        let picks = Delver { has_lockpicks: true, ..Delver::default() };
        assert_eq!(row(verbs_for(&strongbox, &picks)), (Some(Verb::Open), None, Some(Verb::PickLock), None));
        let key = Delver { has_key: true, ..Delver::default() };
        assert_eq!(row(verbs_for(&strongbox, &key)), (Some(Verb::Open), Some(Verb::Unlock), None, None));
    }

    #[test]
    fn a_locked_door_with_nothing_is_only_opened() {
        let door = Target::Door { open: false, lock: Lock::LOCKED };
        assert_eq!(row(verbs_for(&door, &Delver::default())), (Some(Verb::Open), None, None, None));
    }

    #[test]
    fn open_doors_close_and_unlocked_ones_lock_with_a_key() {
        let both = Delver { has_key: true, has_lockpicks: true, hidden: false };
        let open = Target::Door { open: true, lock: Lock::UNLOCKED };
        assert_eq!(row(verbs_for(&open, &both)), (Some(Verb::Close), Some(Verb::Lock), None, None));
        let plain = Target::Door { open: false, lock: Lock::NONE };
        assert_eq!(row(verbs_for(&plain, &both)), (Some(Verb::Open), None, None, None));
    }

    #[test]
    fn folk_are_stolen_from_only_while_hidden() {
        let seen = verbs_for(&Target::Folk, &Delver::default());
        assert_eq!(row(seen), (Some(Verb::Talk), Some(Verb::Barter), None, None));
        let hidden = verbs_for(&Target::Folk, &Delver { hidden: true, ..Delver::default() });
        assert_eq!(row(hidden), (Some(Verb::Talk), Some(Verb::Barter), Some(Verb::Steal), None));
        assert!(!hidden.compare);
    }

    #[test]
    fn slots_and_actions_map_both_ways() {
        for slot in Slot::ALL {
            assert_eq!(Slot::of(slot.action()), Some(slot));
        }
        assert_eq!(Slot::of(Action::Compare), None);
        assert_eq!(Slot::of(Action::Sprint), None);
    }

    #[test]
    fn a_floor_dagger_lists_its_keys() {
        let line = |v| key_line(&Keymap::default(), &v);
        assert_eq!(line(carried("Dagger", Place::Ground)), "F Take \u{b7} G Equip \u{b7} Alt Compare");
        assert_eq!(line(carried("Bread", Place::Carried)), "F Eat");
        assert_eq!(line(Verbs::default()), "");
    }

    #[test]
    fn a_rebound_key_changes_the_line() {
        let mut keymap = Keymap::default();
        keymap.bindings.iter_mut().find(|b| b.action == Action::Primary).unwrap().inputs = vec![crate::keymap::Input::Key(KeyCode::KeyE)];
        assert_eq!(key_line(&keymap, &carried("Dagger", Place::Ground)), "E Take \u{b7} G Equip \u{b7} Alt Compare");
    }

    #[test]
    fn an_empty_waterskin_has_no_use() {
        let mut skin = roll_item("Waterskin", 3).unwrap();
        assert_eq!(use_of(&skin), Some(Verb::Drink));
        while skin.sip().is_some() {}
        assert_eq!(use_of(&skin), None);
    }

    #[test]
    fn lamp_oil_refills_only_while_a_held_lantern_has_room() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(roll_item("Pack", 1).unwrap());
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let oil = inventory.add(roll_item(fb_items::LAMP_OIL, 4).unwrap());
        inventory.stow(oil).unwrap();
        let uses = |inventory: &Inventory| match item_target(inventory, oil) {
            Some(Target::Item { uses, .. }) => uses,
            other => panic!("{other:?}"),
        };
        assert_eq!(uses(&inventory), None, "no lantern to fill");
        let mut lantern = roll_item("Lantern", 7).unwrap();
        let capacity = lantern.fuel_capacity().unwrap();
        lantern.shape.refuel(capacity, capacity);
        let lantern = inventory.add(lantern);
        inventory.equip(lantern, fb_items::Slot::Offhand).unwrap();
        assert_eq!(uses(&inventory), None, "the lantern is full");
        inventory.toggle_light(lantern).unwrap();
        inventory.burn_lights(600.0);
        assert_eq!(uses(&inventory), Some(Verb::Refill));
        assert_eq!(key_line(&Keymap::default(), &verbs_for(&item_target(&inventory, oil).unwrap(), &Delver::default())), "F Refill");
    }

    #[test]
    fn verbs_have_names() {
        assert_eq!(Verb::PickLock.name(), "Pick the lock");
        assert_eq!(Verb::PutOut.name(), "Put out");
    }
}
