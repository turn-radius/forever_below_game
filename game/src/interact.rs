//! What the four action slots do on whatever the delver is aiming at. One lookup,
//! [`verbs_for`], gives a target's verb for each of Primary, Secondary, Tertiary and Special
//! from the target and the delver's state; the prompts and the input both read it, so what the
//! prompt says is what the key does. An action that cannot be done right now is `None`, never
//! greyed. Compare (Alt) is not a slot: [`Verbs::compare`] only says whether the target can be
//! compared, for an "Alt Compare" prompt.
//!
//! | Target | Primary | Secondary | Tertiary |
//! |---|---|---|---|
//! | Item on the ground | Take | Equip | |
//! | Item carried | its use (Eat, Drink, Light, Put out) | Equip, or Take off if worn | |
//! | Door | Open or Close | Lock or Unlock, holding its key | Pick the lock, holding lockpicks |
//! | Container | Open | Lock or Unlock, holding its key | Pick the lock, holding lockpicks |
//! | Folk | Talk | Barter | Steal, while hidden |
//!
//! The core is pure: a [`Target`] and a [`Delver`] in, [`Verbs`] out. [`item_target`] builds an
//! item's target from the real inventory. Not wired yet, because the game has none of it: doors
//! that open (only the entrance door exists), containers in the level, locks, keys, lockpicks
//! and crouching out of sight. Those are plain fields ([`Lock`], [`Delver`]) with no gameplay
//! behind them; whatever adds them fills the fields in. Steal is only offered, nothing steals.
//!
//! The slots are not bound to keys here; [`Slot`] is mapped to the keymap's actions where the
//! input reads this.

use fb_items::Item;

use crate::inventory::{Inventory, ItemId, Location};

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
    Open,
    Close,
    Lock,
    Unlock,
    PickLock,
    Talk,
    Barter,
    Steal,
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
            Verb::Open => "Open",
            Verb::Close => "Close",
            Verb::Lock => "Lock",
            Verb::Unlock => "Unlock",
            Verb::PickLock => "Pick the lock",
            Verb::Talk => "Talk",
            Verb::Barter => "Barter",
            Verb::Steal => "Steal",
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
        /// What using it does, if anything: Eat, Drink, Light or Put out.
        uses: Option<Verb>,
        /// Whether it can be worn or held.
        equippable: bool,
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
        Target::Item { place, uses, equippable } => {
            let equip = equippable.then_some(Verb::Equip);
            match place {
                Place::Ground => Verbs { primary: Some(Verb::Take), secondary: equip, compare: equippable, ..Verbs::default() },
                Place::Carried => Verbs { primary: uses, secondary: equip, compare: equippable, ..Verbs::default() },
                Place::Worn => Verbs { primary: uses, secondary: Some(Verb::TakeOff), compare: equippable, ..Verbs::default() },
            }
        }
        Target::Door { open, lock } => Verbs { primary: Some(if open { Verb::Close } else { Verb::Open }), ..locks(lock, delver) },
        Target::Container { open, lock } => Verbs { primary: (!open).then_some(Verb::Open), ..locks(lock, delver) },
        Target::Folk => Verbs { primary: Some(Verb::Talk), secondary: Some(Verb::Barter), tertiary: delver.hidden.then_some(Verb::Steal), ..Verbs::default() },
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

/// What using `item` does: eat or drink it, light it or put it out.
pub fn use_of(item: &Item) -> Option<Verb> {
    if let Some(how) = item.consume() {
        return Some(match how {
            fb_items::Consume::Eat => Verb::Eat,
            fb_items::Consume::Drink => Verb::Drink,
        });
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

/// The target for item `id`, from where the inventory keeps it; `on_floor` if it is one of the
/// level's floor items ([`crate::carry::FloorItem`]). `None` if the inventory does not know it.
pub fn item_target(inventory: &Inventory, id: ItemId, on_floor: bool) -> Option<Target> {
    let item = inventory.item(id)?;
    let place = match inventory.location(id)? {
        Location::Worn(_) => Place::Worn,
        Location::In { .. } => Place::Carried,
        Location::Loose if on_floor => Place::Ground,
        Location::Loose => Place::Carried,
    };
    Some(Target::Item { place, uses: use_of(item), equippable: !item.slots().is_empty() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_items::roll_item;

    fn target(item: &Item, place: Place) -> Target {
        Target::Item { place, uses: use_of(item), equippable: !item.slots().is_empty() }
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
    fn verbs_have_names() {
        assert_eq!(Verb::PickLock.name(), "Pick the lock");
        assert_eq!(Verb::PutOut.name(), "Put out");
    }
}
