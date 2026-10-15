//! What the delver has and where each thing is: worn in a slot, lying in a container's grid,
//! or loose on the floor. Plain Rust apart from being a Bevy resource.
//!
//! Every item the game keeps track of has an [`ItemId`] here, items lying in the level
//! included, so a bag keeps its contents wherever it goes (`docs/items.md`, "Containers"): its
//! grid stays under its id whether it is worn, packed in another bag or set down.

use std::collections::{BTreeMap, HashMap};

use bevy::prelude::Resource;
use fb_items::{Item, Slot, footprint::Footprint};

use super::grid::{Grid, Misfit, Placement};

/// One item the inventory keeps track of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub u32);

/// Where an item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    /// On the floor of the level, or anywhere else outside the delver's keeping.
    Loose,
    Worn(Slot),
    /// In a container's grid.
    In { container: ItemId, at: Placement },
}

/// Why an item cannot go where it was put. [`Refusal::reason`] is what the screen says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    NoRoom,
    /// A bag into itself, or into a bag it holds.
    IntoItself,
    /// Too long to pack (a long spear, staff or bow) or too heavy (a chest).
    Unpackable,
    NotAContainer,
    NotWearable,
    WrongSlot,
    /// The hip with no belt to hang from.
    NoBelt,
    Unknown,
}

impl Refusal {
    /// The line the screen shows: the plate's words where it had them (`dropTarget`).
    pub fn reason(self) -> &'static str {
        match self {
            Refusal::NoRoom => "No room there.",
            Refusal::IntoItself => "It cannot go inside itself.",
            Refusal::Unpackable => "Too long or heavy to pack.",
            Refusal::NotAContainer => "That holds nothing.",
            Refusal::NotWearable => "That is not something you can wear.",
            Refusal::WrongSlot => "That does not go there.",
            Refusal::NoBelt => "It hangs from a belt. Wear one first.",
            Refusal::Unknown => "That is not here.",
        }
    }
}

impl From<Misfit> for Refusal {
    fn from(_: Misfit) -> Self {
        Refusal::NoRoom
    }
}

/// Worn slots that carry a container whose grid shows on the inventory screen, in the order
/// they show: the pack or sack on the back, then the belt's pouch (the plate's
/// `containersOf`).
pub const CARRYING_SLOTS: [Slot; 2] = [Slot::Back, Slot::Belt];

/// Where the item an equip displaced went: back where the equipped item came from, or, if it
/// did not fit there, loose underfoot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Displaced {
    Worn(Slot),
    In { container: ItemId, at: Placement },
    Underfoot,
}

/// Everything the delver has, and the items lying in the level.
#[derive(Resource, Clone, Debug, Default)]
pub struct Inventory {
    next: u32,
    items: BTreeMap<ItemId, Item>,
    locations: HashMap<ItemId, Location>,
    /// Each container's grid and what lies in it, wherever the container is.
    grids: HashMap<ItemId, Grid<ItemId>>,
    worn: HashMap<Slot, ItemId>,
}

impl Inventory {
    /// Keeps track of `item`, loose. A container gets its empty grid.
    pub fn add(&mut self, item: Item) -> ItemId {
        let id = ItemId(self.next);
        self.next += 1;
        if let Some(holds) = &item.profile.holds {
            self.grids.insert(id, Grid::new(holds.cols, holds.rows));
        }
        self.items.insert(id, item);
        self.locations.insert(id, Location::Loose);
        id
    }

    /// Stops keeping track of `id` and of everything inside it, wherever it is.
    pub fn forget(&mut self, id: ItemId) {
        self.take_out(id);
        if let Some(grid) = self.grids.remove(&id) {
            for entry in grid.entries() {
                self.locations.insert(entry.id, Location::Loose);
                self.forget(entry.id);
            }
        }
        self.items.remove(&id);
        self.locations.remove(&id);
    }

    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.items.get(&id)
    }

    /// Lights `id` if it is out or puts it out if it is lit, wherever it is: in a hand, at the
    /// hip, in a bag or on the floor ([`armory::Item::toggle_lit`]). Returns whether it is lit
    /// now; a spent torch, or anything that is not a light, is left as it was.
    pub fn toggle_light(&mut self, id: ItemId) -> Result<bool, armory::kindle::Unlit> {
        self.items.get_mut(&id).ok_or(armory::kindle::Unlit::NotALight)?.shape.toggle_lit()
    }

    pub fn location(&self, id: ItemId) -> Option<Location> {
        self.locations.get(&id).copied()
    }

    /// The grid `container` opens into.
    pub fn grid(&self, container: ItemId) -> Option<&Grid<ItemId>> {
        self.grids.get(&container)
    }

    pub fn worn(&self, slot: Slot) -> Option<ItemId> {
        self.worn.get(&slot).copied()
    }

    /// Everything worn or held, with its slot, in the slots' order.
    pub fn equipped(&self) -> Vec<(Slot, &Item)> {
        let mut worn: Vec<(Slot, &Item)> = self.worn.iter().filter_map(|(&slot, &id)| Some((slot, self.item(id)?))).collect();
        worn.sort_by_key(|(slot, _)| *slot as u8);
        worn
    }

    /// The containers whose grids show as carried, in order ([`CARRYING_SLOTS`]).
    pub fn carried_containers(&self) -> Vec<ItemId> {
        CARRYING_SLOTS.iter().filter_map(|&slot| self.worn(slot)).filter(|id| self.grids.contains_key(id)).collect()
    }

    /// Whether `id` is worn, or inside something worn, at any depth.
    pub fn is_carried(&self, id: ItemId) -> bool {
        let mut at = id;
        for _ in 0..64 {
            match self.location(at) {
                Some(Location::Worn(_)) => return true,
                Some(Location::In { container, .. }) => at = container,
                _ => return false,
            }
        }
        false
    }

    /// Whether `holder` is `id` or holds it, at any depth (the plate's `holdsId`).
    pub fn holds(&self, holder: ItemId, id: ItemId) -> bool {
        holder == id || self.grids.get(&holder).is_some_and(|g| g.entries().iter().any(|e| self.holds(e.id, id)))
    }

    /// The footprint of `id`, upright.
    pub fn footprint(&self, id: ItemId) -> Option<Footprint> {
        self.item(id).map(|i| i.profile.footprint)
    }

    /// Whether `id` could go into `container` at `at`, without moving anything.
    pub fn check_put(&self, id: ItemId, container: ItemId, at: Placement) -> Result<(), Refusal> {
        let item = self.item(id).ok_or(Refusal::Unknown)?;
        let grid = self.grids.get(&container).ok_or(Refusal::NotAContainer)?;
        if self.holds(id, container) {
            return Err(Refusal::IntoItself);
        }
        if !item.profile.packable {
            return Err(Refusal::Unpackable);
        }
        let size = super::grid::oriented(item.profile.footprint, at.turned);
        let ignoring = matches!(self.location(id), Some(Location::In { container: c, .. }) if c == container).then_some(id);
        grid.check(size, at.x as i32, at.y as i32, ignoring)?;
        Ok(())
    }

    /// Puts `id` into `container`'s grid at `at`, taking it from wherever it was. Refused, it
    /// stays where it was.
    pub fn put(&mut self, id: ItemId, container: ItemId, at: Placement) -> Result<(), Refusal> {
        self.check_put(id, container, at)?;
        let footprint = self.item(id).ok_or(Refusal::Unknown)?.profile.footprint;
        self.take_out(id);
        self.grids.get_mut(&container).ok_or(Refusal::NotAContainer)?.place(id, footprint, at)?;
        self.locations.insert(id, Location::In { container, at });
        Ok(())
    }

    /// Puts `id` into the first carried container with room for it, at its first free spot.
    /// Returns the container.
    pub fn stow(&mut self, id: ItemId) -> Result<ItemId, Refusal> {
        let item = self.item(id).ok_or(Refusal::Unknown)?;
        if !item.profile.packable {
            return Err(Refusal::Unpackable);
        }
        let footprint = item.profile.footprint;
        for container in self.carried_containers() {
            if self.holds(id, container) {
                continue;
            }
            if let Some(at) = self.grids[&container].first_free(footprint) {
                self.put(id, container, at)?;
                return Ok(container);
            }
        }
        Err(Refusal::NoRoom)
    }

    /// Picks `id` up: a bag goes on, in its slot, if that slot is free; anything else is
    /// stowed ([`Inventory::stow`]). Wearing a picked-up bag is this build's own rule, so a
    /// pack found on the floor becomes a grid at once.
    pub fn pick_up(&mut self, id: ItemId) -> Result<(), Refusal> {
        let item = self.item(id).ok_or(Refusal::Unknown)?;
        if item.profile.holds.is_some()
            && let Some(slot) = item.profile.slot.filter(|s| CARRYING_SLOTS.contains(s))
            && self.worn(slot).is_none()
        {
            return self.wear(id, slot);
        }
        self.stow(id).map(|_| ())
    }

    /// Whether `id` could be worn in `slot`, without moving anything. Something already worn
    /// there is no bar: equipping swaps it out ([`Inventory::equip`]).
    pub fn check_wear(&self, id: ItemId, slot: Slot) -> Result<(), Refusal> {
        let item = self.item(id).ok_or(Refusal::Unknown)?;
        item.profile.slot.ok_or(Refusal::NotWearable)?;
        if !item.fits(slot) {
            return Err(Refusal::WrongSlot);
        }
        if slot == Slot::Hip && self.worn(Slot::Belt).is_none() {
            return Err(Refusal::NoBelt);
        }
        Ok(())
    }

    /// With no belt worn, whatever hung at the hip comes off: into the first carried bag with
    /// room ([`Inventory::stow`]), or else loose, to be set down underfoot. Returns it and where
    /// it went. The user's call; the plate had no hip.
    pub fn unhang(&mut self) -> Option<(ItemId, Displaced)> {
        if self.worn(Slot::Belt).is_some() {
            return None;
        }
        let id = self.worn(Slot::Hip)?;
        self.take_out(id);
        let went = match self.stow(id) {
            Ok(container) => match self.location(id) {
                Some(Location::In { at, .. }) => Displaced::In { container, at },
                _ => Displaced::Underfoot,
            },
            Err(_) => Displaced::Underfoot,
        };
        Some((id, went))
    }

    /// The slot `id` goes in when worn without saying where: the first of its slots
    /// ([`fb_items::Item::slots`]) that is free, or the first if none is. So a ring takes
    /// whichever ring slot is free (the plate's `targetSlot`), a weapon the main hand, or the
    /// off hand if only that is free, and a lantern a free hand, or else the hip if a belt is
    /// worn.
    pub fn target_slot(&self, id: ItemId) -> Option<Slot> {
        let slots = self.item(id)?.slots();
        let free = |slot: &&Slot| self.worn(**slot).is_none_or(|other| other == id) && self.check_wear(id, **slot).is_ok();
        slots.iter().find(free).or(slots.first()).copied()
    }

    /// `slot`, or for a ring dropped on a taken ring slot the other one if that is free, so a
    /// ring fills whichever ring slot is free.
    fn ring_slot(&self, id: ItemId, slot: Slot) -> Slot {
        let free = |s| self.worn(s).is_none_or(|other| other == id);
        match slot {
            Slot::Ring1 | Slot::Ring2 if !free(slot) => {
                let other = if slot == Slot::Ring1 { Slot::Ring2 } else { Slot::Ring1 };
                if self.worn(other).is_none() { other } else { slot }
            }
            _ => slot,
        }
    }

    /// Wears `id` in `slot`, taking it from wherever it was. Refused if something else is
    /// worn there; [`Inventory::equip`] swaps instead.
    pub fn wear(&mut self, id: ItemId, slot: Slot) -> Result<(), Refusal> {
        self.check_wear(id, slot)?;
        match self.worn(slot) {
            Some(other) if other == id => return Ok(()),
            Some(_) => return Err(Refusal::NoRoom),
            None => {}
        }
        self.take_out(id);
        self.worn.insert(slot, id);
        self.locations.insert(id, Location::Worn(slot));
        Ok(())
    }

    /// Equips `id` in `slot` (a ring in whichever ring slot is free), taking it from wherever
    /// it was. Whatever was worn there goes back where `id` came from if it fits (the slot it
    /// was worn in, or its cell in its grid, upright or else turned), or else is left loose to be set down
    /// underfoot (the plate's `commitDrop`). Returns the displaced item and where it went.
    pub fn equip(&mut self, id: ItemId, slot: Slot) -> Result<Option<(ItemId, Displaced)>, Refusal> {
        self.check_wear(id, slot)?;
        let slot = self.ring_slot(id, slot);
        let from = self.location(id);
        let Some(old) = self.worn(slot).filter(|&old| old != id) else {
            self.wear(id, slot)?;
            return Ok(None);
        };
        self.take_out(old);
        self.wear(id, slot)?;
        let back = match from {
            Some(Location::Worn(was)) if self.check_wear(old, was).is_ok() && self.wear(old, was).is_ok() => Displaced::Worn(was),
            // Upright where it fits, else turned, else set down.
            Some(Location::In { container, at }) => [false, true]
                .into_iter()
                .map(|turned| Placement { turned, ..at })
                .find(|&placed| self.put(old, container, placed).is_ok())
                .map_or(Displaced::Underfoot, |at| Displaced::In { container, at }),
            _ => Displaced::Underfoot,
        };
        Ok(Some((old, back)))
    }

    /// Takes `id` from its slot or grid, leaving it loose. What it holds stays in it.
    pub fn take_out(&mut self, id: ItemId) {
        match self.location(id) {
            Some(Location::Worn(slot)) => {
                self.worn.remove(&slot);
            }
            Some(Location::In { container, .. }) => {
                if let Some(grid) = self.grids.get_mut(&container) {
                    grid.remove(id);
                }
            }
            _ => {}
        }
        if self.items.contains_key(&id) {
            self.locations.insert(id, Location::Loose);
        }
    }

    /// How many things lie directly in `container`.
    pub fn contents_count(&self, container: ItemId) -> usize {
        self.grids.get(&container).map_or(0, |g| g.entries().len())
    }

    /// Pounds inside `container`, counted through every bag in it (the plate's
    /// `contentsWeight`).
    pub fn contents_weight(&self, container: ItemId) -> f32 {
        self.grids.get(&container).map_or(0.0, |g| {
            g.entries().iter().map(|e| self.item(e.id).map_or(0.0, |i| i.profile.stats.weight) + self.contents_weight(e.id)).sum()
        })
    }

    /// Every item kept track of, in the order added.
    pub fn ids(&self) -> impl Iterator<Item = ItemId> + '_ {
        self.items.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rolled(base: &str, seed: u32) -> Item {
        fb_items::roll_item(base, seed).unwrap()
    }

    /// A dagger of footprint 1 by 2.
    fn dagger() -> Item {
        (1..500).map(|s| rolled("Dagger", s)).find(|d| d.profile.footprint == Footprint { cols: 1, rows: 2 }).expect("no 1 by 2 dagger")
    }

    fn worn_pack(inventory: &mut Inventory) -> ItemId {
        let pack = inventory.add(rolled("Pack", 7));
        inventory.wear(pack, Slot::Back).unwrap();
        pack
    }

    #[test]
    fn a_backpack_holds_a_dagger_in_any_free_spot() {
        let mut inventory = Inventory::default();
        let pack = worn_pack(&mut inventory);
        assert_eq!(inventory.grid(pack).map(|g| (g.cols, g.rows)), Some((8, 5)));
        let knife = inventory.add(dagger());
        for (x, y) in [(0, 0), (7, 3), (4, 2), (0, 3)] {
            inventory.put(knife, pack, Placement::new(x, y, false)).unwrap();
            assert_eq!(inventory.location(knife), Some(Location::In { container: pack, at: Placement::new(x, y, false) }));
        }
        assert_eq!(inventory.contents_count(pack), 1, "moving keeps one of it");
        assert_eq!(inventory.put(knife, pack, Placement::new(7, 4, false)), Err(Refusal::NoRoom));
        assert!(inventory.is_carried(knife));
    }

    #[test]
    fn an_overlapping_drop_is_refused_and_leaves_things_where_they_were() {
        let mut inventory = Inventory::default();
        let pack = worn_pack(&mut inventory);
        let (a, b) = (inventory.add(dagger()), inventory.add(dagger()));
        inventory.put(a, pack, Placement::new(2, 1, false)).unwrap();
        inventory.put(b, pack, Placement::new(5, 1, false)).unwrap();
        assert_eq!(inventory.put(b, pack, Placement::new(2, 2, false)), Err(Refusal::NoRoom));
        assert_eq!(inventory.location(b), Some(Location::In { container: pack, at: Placement::new(5, 1, false) }));
        // A loose one stays loose.
        let c = inventory.add(dagger());
        assert_eq!(inventory.put(c, pack, Placement::new(2, 1, true)), Err(Refusal::NoRoom));
        assert_eq!(inventory.location(c), Some(Location::Loose));
    }

    #[test]
    fn a_bag_cannot_go_into_itself_or_a_bag_it_holds() {
        let mut inventory = Inventory::default();
        let pack = worn_pack(&mut inventory);
        let sack = inventory.add(rolled("Sack", 3));
        inventory.put(sack, pack, Placement::new(0, 0, false)).unwrap();
        assert_eq!(inventory.put(pack, pack, Placement::new(4, 0, false)), Err(Refusal::IntoItself));
        assert_eq!(inventory.put(pack, sack, Placement::new(0, 0, false)), Err(Refusal::IntoItself));
        assert!(inventory.holds(pack, sack) && !inventory.holds(sack, pack));
        assert_eq!(inventory.location(pack), Some(Location::Worn(Slot::Back)));
    }

    #[test]
    fn a_bag_keeps_its_contents_wherever_it_goes() {
        let mut inventory = Inventory::default();
        let pack = worn_pack(&mut inventory);
        let knife = inventory.add(dagger());
        inventory.put(knife, pack, Placement::new(1, 1, false)).unwrap();
        inventory.take_out(pack);
        assert_eq!(inventory.location(pack), Some(Location::Loose));
        assert!(!inventory.is_carried(knife));
        assert_eq!(inventory.location(knife), Some(Location::In { container: pack, at: Placement::new(1, 1, false) }));
        assert!(inventory.contents_weight(pack) > 0.0);
        inventory.forget(pack);
        assert_eq!((inventory.item(pack), inventory.item(knife)), (None, None));
    }

    #[test]
    fn picking_up_wears_a_bag_then_stows_the_rest() {
        let mut inventory = Inventory::default();
        let belt = inventory.add(rolled("Belt", 2));
        inventory.wear(belt, Slot::Belt).unwrap();
        let pack = inventory.add(rolled("Pack", 7));
        inventory.pick_up(pack).unwrap();
        assert_eq!(inventory.worn(Slot::Back), Some(pack));
        assert_eq!(inventory.carried_containers(), vec![pack, belt]);
        let knife = inventory.add(dagger());
        inventory.pick_up(knife).unwrap();
        assert!(matches!(inventory.location(knife), Some(Location::In { container, .. }) if container == pack));
        let spear = (1..400).map(|s| rolled("Spear", s)).find(|s| !s.profile.packable).unwrap();
        let spear = inventory.add(spear);
        assert_eq!(inventory.pick_up(spear), Err(Refusal::Unpackable));
        assert_eq!(inventory.location(spear), Some(Location::Loose));
    }

    #[test]
    fn rings_fill_either_ring_slot() {
        let mut inventory = Inventory::default();
        let ring = inventory.add(rolled("Ring", 1));
        inventory.wear(ring, Slot::Ring2).unwrap();
        let other = inventory.add(rolled("Ring", 2));
        assert_eq!(inventory.wear(other, Slot::Ring2), Err(Refusal::NoRoom));
        assert_eq!(inventory.wear(other, Slot::Head), Err(Refusal::WrongSlot));
    }

    fn worn_belt(inventory: &mut Inventory) -> ItemId {
        let belt = inventory.add(rolled("Belt", 2));
        inventory.wear(belt, Slot::Belt).unwrap();
        belt
    }

    #[test]
    fn a_ring_fills_ring_1_then_ring_2() {
        let mut inventory = Inventory::default();
        let (a, b, c) = (inventory.add(rolled("Ring", 1)), inventory.add(rolled("Ring", 2)), inventory.add(rolled("Ring", 3)));
        for (ring, slot) in [(a, Slot::Ring1), (b, Slot::Ring2)] {
            let target = inventory.target_slot(ring).unwrap();
            assert_eq!(target, slot);
            assert_eq!(inventory.equip(ring, target), Ok(None));
        }
        assert_eq!((inventory.worn(Slot::Ring1), inventory.worn(Slot::Ring2)), (Some(a), Some(b)));
        // Both full: the first is swapped out, and a ring found on the floor sends it there.
        assert_eq!(inventory.equip(c, inventory.target_slot(c).unwrap()), Ok(Some((a, Displaced::Underfoot))));
        assert_eq!(inventory.location(a), Some(Location::Loose));
        // Dropped on a taken ring slot while the other is free, it takes the free one.
        inventory.take_out(b);
        assert_eq!(inventory.equip(b, Slot::Ring1), Ok(None));
        assert_eq!(inventory.worn(Slot::Ring2), Some(b));
    }

    #[test]
    fn equipping_a_helmet_replaces_the_one_worn() {
        let mut inventory = Inventory::default();
        let belt = worn_belt(&mut inventory);
        let (old, new) = (inventory.add(rolled("Helm", 1)), inventory.add(rolled("Helm", 2)));
        inventory.wear(old, Slot::Head).unwrap();
        let at = Placement::new(1, 0, false);
        inventory.put(new, belt, at).unwrap();
        // The old helm goes back to the new one's cell in the pouch, if it fits there.
        let back = inventory.equip(new, Slot::Head).unwrap();
        assert_eq!(inventory.worn(Slot::Head), Some(new));
        let fits = matches!(back, Some((id, Displaced::In { container, at: placed })) if id == old && container == belt && placed == at);
        let underfoot = back == Some((old, Displaced::Underfoot)) && inventory.location(old) == Some(Location::Loose);
        assert!(fits || underfoot, "{back:?}");
        assert_eq!(inventory.equip(new, Slot::Feet), Err(Refusal::WrongSlot));
    }

    #[test]
    fn a_swap_sends_the_old_item_back_where_the_new_one_came_from() {
        let mut inventory = Inventory::default();
        let pack = worn_pack(&mut inventory);
        let sword = (1..400).map(|s| rolled("Sword", s)).find(|s| s.profile.packable).unwrap();
        let (knife, sword) = (inventory.add(dagger()), inventory.add(sword));
        inventory.wear(knife, Slot::Mainhand).unwrap();
        inventory.stow(sword).unwrap();
        let Some(Location::In { at, .. }) = inventory.location(sword) else { panic!("stowed") };
        let upright = Placement { turned: false, ..at };
        assert_eq!(inventory.equip(sword, Slot::Mainhand), Ok(Some((knife, Displaced::In { container: pack, at: upright }))));
        assert_eq!(inventory.location(knife), Some(Location::In { container: pack, at: upright }));
        // A ring moved from one ring slot onto the other, both full, trades places.
        let (a, b) = (inventory.add(rolled("Ring", 1)), inventory.add(rolled("Ring", 2)));
        inventory.wear(a, Slot::Ring1).unwrap();
        inventory.wear(b, Slot::Ring2).unwrap();
        assert_eq!(inventory.equip(a, Slot::Ring2), Ok(Some((b, Displaced::Worn(Slot::Ring1)))));
        // A pack swapped for a sack it holds cannot go inside it: it is set down.
        let sack = inventory.add(rolled("Sack", 3));
        inventory.stow(sack).unwrap();
        assert_eq!(inventory.equip(sack, Slot::Back), Ok(Some((pack, Displaced::Underfoot))));
        assert_eq!(inventory.carried_containers().first(), Some(&sack));
    }

    /// One of `base` that packs, so it can be stowed and swapped back.
    fn packable(base: &str) -> Item {
        (1..400).map(|s| rolled(base, s)).find(|i| i.profile.packable).unwrap_or_else(|| panic!("no packable {base}"))
    }

    #[test]
    fn every_weapon_family_goes_in_the_off_hand() {
        for base in ["Sword", "Dagger", "Mace", "Club", "Hand axe", "Spear", "Staff", "Bow", "Crossbow"] {
            let mut inventory = Inventory::default();
            let pack = worn_pack(&mut inventory);
            let weapon = inventory.add(packable(base));
            // Dropped on the off hand's slot.
            assert_eq!(inventory.check_wear(weapon, Slot::Offhand), Ok(()), "{base}");
            assert_eq!(inventory.equip(weapon, Slot::Offhand), Ok(None), "{base}");
            assert_eq!(inventory.worn(Slot::Offhand), Some(weapon), "{base}");
            // Without saying where (a double-click, F, the doll): the main hand while it is free,
            // the off hand once only that is free.
            inventory.take_out(weapon);
            inventory.stow(weapon).unwrap();
            assert_eq!(inventory.target_slot(weapon), Some(Slot::Mainhand), "{base}");
            let other = inventory.add(packable("Dagger"));
            inventory.wear(other, Slot::Mainhand).unwrap();
            assert_eq!(inventory.target_slot(weapon), Some(Slot::Offhand), "{base}");
            // Both hands full: the main hand's is swapped out, back into the pack.
            let shield = inventory.add(rolled("Shield", 2));
            inventory.wear(shield, Slot::Offhand).unwrap();
            assert_eq!(inventory.target_slot(weapon), Some(Slot::Mainhand), "{base}");
            assert!(matches!(inventory.equip(weapon, Slot::Mainhand), Ok(Some((id, Displaced::In { container, .. }))) if id == other && container == pack), "{base}");
        }
        // A shield is still only for the off hand, and armour never goes in a hand.
        let mut inventory = Inventory::default();
        let shield = inventory.add(rolled("Shield", 2));
        assert_eq!(inventory.check_wear(shield, Slot::Mainhand), Err(Refusal::WrongSlot));
        let helm = inventory.add(rolled("Helm", 2));
        assert_eq!(inventory.check_wear(helm, Slot::Offhand), Err(Refusal::WrongSlot));
    }

    #[test]
    fn weapons_trade_hands() {
        let mut inventory = Inventory::default();
        let (sword, dagger) = (inventory.add(packable("Sword")), inventory.add(packable("Dagger")));
        inventory.wear(sword, Slot::Mainhand).unwrap();
        inventory.wear(dagger, Slot::Offhand).unwrap();
        // The sword dragged onto the off hand: the dagger goes to the main hand it came from.
        assert_eq!(inventory.equip(sword, Slot::Offhand), Ok(Some((dagger, Displaced::Worn(Slot::Mainhand)))));
        assert_eq!((inventory.worn(Slot::Mainhand), inventory.worn(Slot::Offhand)), (Some(dagger), Some(sword)));
    }

    #[test]
    fn a_lantern_hangs_at_the_hip_only_from_a_belt() {
        let mut inventory = Inventory::default();
        let lantern = inventory.add(rolled("Lantern", 1));
        assert_eq!(inventory.check_wear(lantern, Slot::Hip), Err(Refusal::NoBelt));
        assert_eq!(inventory.wear(lantern, Slot::Hip), Err(Refusal::NoBelt));
        worn_belt(&mut inventory);
        assert_eq!(inventory.equip(lantern, Slot::Hip), Ok(None));
        assert_eq!(inventory.worn(Slot::Hip), Some(lantern));
        assert!(inventory.is_carried(lantern));
        // Nothing but a lantern hangs there.
        let (torch, sword) = (inventory.add(rolled("Torch", 1)), inventory.add(rolled("Sword", 1)));
        assert_eq!(inventory.check_wear(torch, Slot::Hip), Err(Refusal::WrongSlot));
        assert_eq!(inventory.check_wear(sword, Slot::Hip), Err(Refusal::WrongSlot));
        // While the belt is on, nothing comes off.
        assert_eq!(inventory.unhang(), None);
    }

    #[test]
    fn taking_the_belt_off_moves_the_hip_lantern_to_a_bag_or_underfoot() {
        // With a pack on: into the pack.
        let mut inventory = Inventory::default();
        let pack = worn_pack(&mut inventory);
        let belt = worn_belt(&mut inventory);
        let lantern = inventory.add(rolled("Lantern", 1));
        inventory.wear(lantern, Slot::Hip).unwrap();
        inventory.take_out(belt);
        assert!(matches!(inventory.unhang(), Some((id, Displaced::In { container, .. })) if id == lantern && container == pack));
        assert!(matches!(inventory.location(lantern), Some(Location::In { container, .. }) if container == pack));
        assert_eq!(inventory.worn(Slot::Hip), None);
        // With no bag: loose, to be set down.
        let mut inventory = Inventory::default();
        let belt = worn_belt(&mut inventory);
        let lantern = inventory.add(rolled("Lantern", 1));
        inventory.wear(lantern, Slot::Hip).unwrap();
        inventory.take_out(belt);
        assert_eq!(inventory.unhang(), Some((lantern, Displaced::Underfoot)));
        assert_eq!(inventory.location(lantern), Some(Location::Loose));
    }

    #[test]
    fn a_lantern_takes_a_free_hand_then_the_hip() {
        let mut inventory = Inventory::default();
        let lantern = inventory.add(rolled("Lantern", 1));
        assert_eq!(inventory.target_slot(lantern), Some(Slot::Offhand));
        let (sword, dagger) = (inventory.add(packable("Sword")), inventory.add(packable("Dagger")));
        inventory.wear(dagger, Slot::Offhand).unwrap();
        assert_eq!(inventory.target_slot(lantern), Some(Slot::Mainhand));
        inventory.wear(sword, Slot::Mainhand).unwrap();
        // Both hands full and no belt: it swaps out the off hand's.
        assert_eq!(inventory.target_slot(lantern), Some(Slot::Offhand));
        worn_belt(&mut inventory);
        assert_eq!(inventory.target_slot(lantern), Some(Slot::Hip));
        assert_eq!(inventory.equip(lantern, Slot::Hip), Ok(None));
        assert_eq!((inventory.worn(Slot::Mainhand), inventory.worn(Slot::Offhand)), (Some(sword), Some(dagger)));
    }
}
