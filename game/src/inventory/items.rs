//! What the delver has and where each thing is: worn in a slot, lying in a container's grid,
//! or loose on the floor. Plain Rust apart from being a Bevy resource.
//!
//! Every item the game keeps track of has an [`ItemId`] here, items lying in the level
//! included, so a bag keeps its contents wherever it goes (`docs/items.md`, "Containers"): its
//! grid stays under its id whether it is worn, packed in another bag or set down.

use std::collections::{BTreeMap, HashMap};

use bevy::prelude::Resource;
use fb_items::{Coins, Item, Slot, footprint::Footprint};

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
    /// The container takes only certain things (a quiver takes arrows).
    NotAccepted,
    NotAContainer,
    NotWearable,
    WrongSlot,
    /// The hip with no belt to hang from.
    NoBelt,
    /// The strap slot with no pack worn that has a weapon strap.
    NoStrap,
    /// A quickslot has nothing to do with it.
    NotUsable,
    /// Not carried: a quickslot only takes what the delver has with them.
    NotCarried,
    Unknown,
    /// A trader's shelf takes goods, not coin.
    NotAWare,
    /// A bag with things in it cannot be sold full.
    HoldsThings,
    /// Bartering is going on: only the pack and the trader's shelf are in play.
    Bartering,
}

impl Refusal {
    /// The line the screen shows: the plate's words where it had them (`dropTarget`).
    pub fn reason(self) -> &'static str {
        match self {
            Refusal::NoRoom => "No room there.",
            Refusal::IntoItself => "It cannot go inside itself.",
            Refusal::Unpackable => "Too long or heavy to pack.",
            Refusal::NotAccepted => "That does not go in there.",
            Refusal::NotAContainer => "That holds nothing.",
            Refusal::NotWearable => "That is not something you can wear.",
            Refusal::WrongSlot => "That does not go there.",
            Refusal::NoBelt => "It hangs from a belt. Wear one first.",
            Refusal::NoStrap => "It needs a pack with a weapon strap.",
            Refusal::NotUsable => "A quickslot cannot use that.",
            Refusal::NotCarried => "Pick it up first.",
            Refusal::Unknown => "That is not here.",
            Refusal::NotAWare => "Coin is not a ware.",
            Refusal::HoldsThings => "Empty it first.",
            Refusal::Bartering => "Nothing lies about in the stall.",
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
/// `containersOf`), then the quiver or bolt case.
pub const CARRYING_SLOTS: [Slot; 5] = [Slot::Back, Slot::Satchel, Slot::Belt, Slot::Hip, Slot::Quiver];

/// The minute of fuel a light's card shows ("Fuel 0:42"), rounded as the card rounds it.
fn fuel_minute(item: &Item) -> Option<u32> {
    item.fuel_left().map(|seconds| (seconds.max(0.0) / 60.0).round() as u32)
}

/// How burnt a torch or candle looks, in twentieths of its char or burnt wax, so a held one is refitted about every
/// 0.05 char; 0 for anything else.
pub fn char_bucket(shape: &armory::Item) -> u8 {
    match shape {
        armory::Item::Torch(s) => (s.burn.char * 20.0) as u8,
        armory::Item::Sundry(s) if s.article.kind == armory::families::sundry::SundryKind::Candle => (s.article.burnt * 20.0) as u8,
        _ => 0,
    }
}

/// Where the item an equip displaced went: back where the equipped item came from, or, if it
/// did not fit there, loose underfoot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Displaced {
    Worn(Slot),
    In { container: ItemId, at: Placement },
    /// Folded into a stack of its kind in the container, and gone as an item of its own.
    Folded { container: ItemId },
    Underfoot,
}

/// What [`Inventory::top_up`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TopUp {
    None,
    Some,
    Done,
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
    /// The delver's own coin purse: what they are paid, and what the purses they open hold.
    coins: Coins,
}

impl Inventory {
    /// What is in the delver's coin purse. [`crate::enchanting::wallet`] adds the purses they
    /// carry unopened.
    pub fn coins(&self) -> Coins {
        self.coins
    }

    pub fn set_coins(&mut self, coins: Coins) {
        self.coins = coins;
    }

    /// Puts `coins` in the delver's coin purse.
    pub fn add_coins(&mut self, coins: Coins) {
        self.coins = self.coins + coins;
    }

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

    /// Takes a swallow from the waterskin `id` ([`fb_items::Item::sip`]), wherever it is, and
    /// returns what it quenches.
    pub fn sip(&mut self, id: ItemId) -> Option<fb_items::Provision> {
        self.items.get_mut(&id)?.sip()
    }

    /// The lanterns held or hung at the hip, the main hand's first, then the off hand's, then the
    /// hip's: the ones a flask of lamp oil fills.
    pub fn equipped_lanterns(&self) -> Vec<ItemId> {
        [Slot::Mainhand, Slot::Offhand, Slot::Hip]
            .into_iter()
            .filter_map(|slot| self.worn(slot))
            .filter(|&id| self.item(id).is_some_and(|item| item.oil_room().is_some()))
            .collect()
    }

    /// Whether a lantern held or at the hip has room for more oil.
    pub fn lantern_wants_oil(&self) -> bool {
        self.equipped_lanterns().into_iter().any(|id| self.item(id).and_then(Item::oil_room).is_some_and(|room| room > 0.0))
    }

    /// Pours the lamp-oil flask `flask` into the lanterns held or at the hip, in the order of
    /// [`Inventory::equipped_lanterns`], each as full as the oil goes
    /// ([`fb_items::Item::refill_from`]). Returns the lanterns that took any.
    pub fn refill_lanterns(&mut self, flask: ItemId) -> Vec<ItemId> {
        let Some(mut oil) = self.items.get(&flask).cloned() else { return vec![] };
        let mut filled = vec![];
        for id in self.equipped_lanterns() {
            if let Some(lantern) = self.items.get_mut(&id)
                && lantern.refill_from(&mut oil) > 0.0
            {
                filled.push(id);
            }
        }
        self.items.insert(flask, oil);
        filled
    }

    /// Burns `seconds` of game time off every lit torch and lantern, wherever it is: in a hand,
    /// at the hip, in a bag or on the floor ([`fb_items::Item::burn`]). Returns whether
    /// anything shown changed: a light's fuel minute on its card, its state, or its char bucket
    /// ([`char_bucket`]), so the caller can mark the inventory changed only then.
    pub fn burn_lights(&mut self, seconds: f32) -> bool {
        let mut shown = false;
        for item in self.items.values_mut() {
            if item.shape.lit() != Some(true) {
                continue;
            }
            let before = (item.shape.lit(), fuel_minute(item), char_bucket(&item.shape));
            item.burn(seconds);
            shown |= before != (item.shape.lit(), fuel_minute(item), char_bucket(&item.shape));
        }
        shown
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

    /// Every slot with something in it, and what, in the slots' order.
    pub fn worn_ids(&self) -> Vec<(Slot, ItemId)> {
        let mut worn: Vec<(Slot, ItemId)> = self.worn.iter().map(|(&slot, &id)| (slot, id)).collect();
        worn.sort_by_key(|(slot, _)| *slot as u8);
        worn
    }

    /// Everything worn or held, with its slot, in the slots' order.
    pub fn equipped(&self) -> Vec<(Slot, &Item)> {
        // A weapon on the pack's strap is carried, not held: it gives none of its stats, only its
        // weight ([`Inventory::strapped`]).
        let mut worn: Vec<(Slot, &Item)> = self.worn.iter().filter(|(slot, _)| **slot != Slot::Strap).filter_map(|(&slot, &id)| Some((slot, self.item(id)?))).collect();
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
        if !self.accepts(container, item) {
            return Err(Refusal::NotAccepted);
        }
        let size = super::grid::oriented(item.profile.footprint, at.turned);
        let ignoring = matches!(self.location(id), Some(Location::In { container: c, .. }) if c == container).then_some(id);
        grid.check(size, at.x as i32, at.y as i32, ignoring)?;
        Ok(())
    }

    /// Whether `container` takes `item` by its filter ([`fb_items::footprint::Grid::takes`]).
    pub fn accepts(&self, container: ItemId, item: &Item) -> bool {
        self.item(container).and_then(|c| c.profile.holds.as_ref()).is_none_or(|g| g.takes(item.shape.family()))
    }

    /// Puts `item` in the place of `id`'s, for what an enchanter has changed (its affixes, name
    /// and history): the same base and footprint, so where it lies stays. `false`, and nothing
    /// changes, for an unknown id or another base.
    pub fn replace_item(&mut self, id: ItemId, item: Item) -> bool {
        match self.items.get_mut(&id) {
            Some(old) if old.base == item.base && old.profile.footprint == item.profile.footprint => {
                *old = item;
                true
            }
            _ => false,
        }
    }

    /// Puts `item` in the place of loose `id`, for a camp good that has changed form (a tent
    /// pitched or packed, a bedroll laid out or rolled up): the same base and seed, but a profile
    /// and footprint of its own. Only a loose item changes, since a grid holds it at the footprint
    /// it had. `false`, and nothing changes, for an unknown id, one that is placed, or another item.
    pub fn replace_form(&mut self, id: ItemId, item: Item) -> bool {
        if self.location(id) != Some(Location::Loose) {
            return false;
        }
        match self.items.get_mut(&id) {
            Some(old) if old.base == item.base && old.seed == item.seed => {
                *old = item;
                true
            }
            _ => false,
        }
    }

    /// [`Inventory::replace_form`] for an item that is still in a pack or a slot: takes `id` out
    /// of it and gives it `item`'s form in one step. Nothing changes, and it is `false`, for an
    /// unknown id or another item, so a camp good is never left out of its pack in its old form.
    pub fn set_form_out(&mut self, id: ItemId, item: Item) -> bool {
        if !self.items.get(&id).is_some_and(|old| old.base == item.base && old.seed == item.seed) {
            return false;
        }
        self.take_out(id);
        self.replace_form(id, item)
    }

    /// Mutable access to the shape of `id`, for state that changes in play (the arrow nocked
    /// on a bow, the ammunition a quiver shows). Not for changing what the item is.
    pub fn shape_mut(&mut self, id: ItemId) -> Option<&mut armory::Item> {
        self.items.get_mut(&id).map(|i| &mut i.shape)
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

    /// Whether `id` could go on the trader's `shelf` at `at`: any size of thing, long ones too,
    /// which a pack would not take, so a spear or a bow can be sold and bought.
    pub fn check_shelve(&self, id: ItemId, shelf: ItemId, at: Placement) -> Result<(), Refusal> {
        let item = self.item(id).ok_or(Refusal::Unknown)?;
        let grid = self.grids.get(&shelf).ok_or(Refusal::NotAContainer)?;
        if self.holds(id, shelf) {
            return Err(Refusal::IntoItself);
        }
        let size = super::grid::oriented(item.profile.footprint, at.turned);
        let ignoring = matches!(self.location(id), Some(Location::In { container: c, .. }) if c == shelf).then_some(id);
        grid.check(size, at.x as i32, at.y as i32, ignoring)?;
        Ok(())
    }

    /// Puts `id` on `shelf` at `at` ([`Inventory::check_shelve`]), taking it from wherever it was.
    pub fn shelve_at(&mut self, id: ItemId, shelf: ItemId, at: Placement) -> Result<(), Refusal> {
        self.check_shelve(id, shelf, at)?;
        let footprint = self.item(id).ok_or(Refusal::Unknown)?.profile.footprint;
        self.take_out(id);
        self.grids.get_mut(&shelf).ok_or(Refusal::NotAContainer)?.place(id, footprint, at)?;
        self.locations.insert(id, Location::In { container: shelf, at });
        Ok(())
    }

    /// Puts `id` on `shelf` at its first free spot.
    pub fn shelve(&mut self, id: ItemId, shelf: ItemId) -> Result<Placement, Refusal> {
        let footprint = self.item(id).ok_or(Refusal::Unknown)?.profile.footprint;
        let at = self.grids.get(&shelf).ok_or(Refusal::NotAContainer)?.first_free(footprint).ok_or(Refusal::NoRoom)?;
        self.shelve_at(id, shelf, at)?;
        Ok(at)
    }

    /// Puts `id` into the first carried container with room for it, at its first free spot.
    /// Returns the container. A stack (arrows, bolts) first tops up the stacks of its own kind
    /// already carried, then fills places of its own, ten to a place: 14 arrows make a stack of
    /// 10 and one of 4.
    ///
    /// Contract for callers: on `Ok`, `id` may have been folded into other stacks and forgotten
    /// (`item(id)` is `None`), and `id` may be in a different place than it was or even be
    /// still loose: when some pieces were stowed and the rest found no room, the result is `Ok`
    /// (a partial pickup) and `id` is still loose, holding what is left ([`Inventory::is_partial`]).
    /// `Err` means nothing moved. Use [`Inventory::stow_apart`] to keep `id` itself.
    pub fn stow(&mut self, id: ItemId) -> Result<ItemId, Refusal> {
        let (footprint, containers) = self.stowing(id)?;
        let mut first = None;
        if self.item(id).is_some_and(|i| i.stack_limit() > 1) {
            for &container in &containers {
                if self.holds(id, container) || !self.accepts(container, self.item(id).ok_or(Refusal::Unknown)?) {
                    continue;
                }
                match self.top_up(id, container) {
                    TopUp::Done => return Ok(container),
                    TopUp::Some => {
                        first.get_or_insert(container);
                    }
                    TopUp::None => {}
                }
            }
            // What is left goes in places of its own, a full stack at a time.
            while self.item(id).is_some_and(|i| i.quantity > i.stack_limit()) {
                let limit = self.item(id).map_or(1, |i| i.stack_limit());
                let mut chunk = self.item(id).ok_or(Refusal::Unknown)?.clone();
                chunk.quantity = limit;
                let chunk_id = self.add(chunk);
                match self.stow_whole(chunk_id, footprint, &containers) {
                    Ok(container) => {
                        first.get_or_insert(container);
                        if let Some(rest) = self.items.get_mut(&id) {
                            rest.quantity -= limit;
                        }
                    }
                    Err(refusal) => {
                        self.forget(chunk_id);
                        return first.ok_or(refusal);
                    }
                }
            }
        }
        match self.stow_whole(id, footprint, &containers) {
            Ok(container) => Ok(first.unwrap_or(container)),
            Err(refusal) => first.ok_or(refusal),
        }
    }

    /// Puts `id` itself, whole, in the first carried container with room, never folding it into
    /// another stack: a deal's moves must keep every id, so Cancel can find it
    /// ([`crate::barter`] folds what it bought at settle, [`Inventory::fold_into_stacks`]).
    pub fn stow_apart(&mut self, id: ItemId) -> Result<ItemId, Refusal> {
        let (footprint, containers) = self.stowing(id)?;
        self.stow_whole(id, footprint, &containers)
    }

    /// Whether `id` is still loose with pieces left after a [`Inventory::stow`] said `Ok`.
    pub fn is_partial(&self, id: ItemId) -> bool {
        self.item(id).is_some() && self.location(id) == Some(Location::Loose)
    }

    /// Folds `id`, a carried stack, into the other stacks of its kind in the same bag, as far as
    /// they have room. True when none is left and `id` has been forgotten.
    pub fn fold_into_stacks(&mut self, id: ItemId) -> bool {
        let Some(Location::In { container, .. }) = self.location(id) else { return false };
        if self.item(id).is_none_or(|i| i.stack_limit() <= 1) {
            return false;
        }
        self.top_up(id, container) == TopUp::Done
    }

    /// Moves what `id` has into `onto`, a stack of its kind with room (a drag let go on it).
    /// True when `id` is emptied and forgotten; false when some is left in `id`, which stays
    /// where it is. Refused when nothing could move.
    pub fn merge_onto(&mut self, id: ItemId, onto: ItemId) -> Result<bool, Refusal> {
        let (from, to) = (self.items.get(&id).ok_or(Refusal::Unknown)?, self.items.get(&onto).ok_or(Refusal::Unknown)?);
        if id == onto || !to.can_stack_with(from) || to.stack_room() == 0 {
            return Err(Refusal::NoRoom);
        }
        let moved = to.stack_room().min(from.quantity);
        let left = from.quantity - moved;
        self.items.get_mut(&onto).map(|t| t.quantity += moved);
        if left == 0 {
            self.forget(id);
        } else {
            self.items.get_mut(&id).map(|f| f.quantity = left);
        }
        Ok(left == 0)
    }

    /// A stack in `container` at cell `(x, y)` that `id` could be dropped on and top up.
    pub fn stack_at(&self, id: ItemId, container: ItemId, x: i32, y: i32) -> Option<ItemId> {
        let onto = self.grids.get(&container)?.at_cell(x, y)?.id;
        let (from, to) = (self.item(id)?, self.item(onto)?);
        (onto != id && to.can_stack_with(from) && to.stack_room() > 0 && !self.holds(id, container)).then_some(onto)
    }

    /// The footprint and the carried containers to try for `id`, the ones that name what they
    /// take (a quiver) first.
    fn stowing(&self, id: ItemId) -> Result<(Footprint, Vec<ItemId>), Refusal> {
        let item = self.item(id).ok_or(Refusal::Unknown)?;
        if !item.profile.packable {
            return Err(Refusal::Unpackable);
        }
        let mut containers = self.carried_containers();
        containers.sort_by_key(|c| self.item(*c).and_then(|c| c.profile.holds.as_ref()).is_none_or(|g| g.accepts.is_empty()));
        Ok((item.profile.footprint, containers))
    }

    /// Puts `id` as it is in the first of `containers` with room, at its first free spot.
    fn stow_whole(&mut self, id: ItemId, footprint: Footprint, containers: &[ItemId]) -> Result<ItemId, Refusal> {
        for &container in containers {
            if self.holds(id, container) || !self.accepts(container, self.item(id).ok_or(Refusal::Unknown)?) {
                continue;
            }
            if let Some(at) = self.grids[&container].first_free(footprint) {
                self.put(id, container, at)?;
                return Ok(container);
            }
        }
        Err(Refusal::NoRoom)
    }

    /// Moves what `id` has into the unfilled stacks of its kind lying directly in `container`.
    fn top_up(&mut self, id: ItemId, container: ItemId) -> TopUp {
        let Some(grid) = self.grids.get(&container) else { return TopUp::None };
        let targets: Vec<ItemId> = grid.entries().iter().map(|e| e.id).filter(|&t| t != id).collect();
        let mut result = TopUp::None;
        for target in targets {
            let (Some(from), Some(to)) = (self.items.get(&id), self.items.get(&target)) else { return result };
            if !to.can_stack_with(from) {
                continue;
            }
            let moved = to.stack_room().min(from.quantity);
            if moved == 0 {
                continue;
            }
            let left = from.quantity - moved;
            if let Some(to) = self.items.get_mut(&target) {
                to.quantity += moved;
            }
            result = TopUp::Some;
            if left == 0 {
                self.forget(id);
                return TopUp::Done;
            }
            if let Some(from) = self.items.get_mut(&id) {
                from.quantity = left;
            }
        }
        result
    }

    /// Takes one piece out of the stack `id`: the stack itself if it is one, else a loose piece
    /// of the same kind split from it, the stack left one fewer. `None` for something kept no
    /// longer.
    pub fn take_one(&mut self, id: ItemId) -> Option<ItemId> {
        let item = self.items.get(&id)?;
        if item.quantity <= 1 {
            self.take_out(id);
            return Some(id);
        }
        let mut one = item.clone();
        one.quantity = 1;
        if let Some(stack) = self.items.get_mut(&id) {
            stack.quantity -= 1;
        }
        Some(self.add(one))
    }

    /// How many pieces lie in `container`'s grid, counting each stack's quantity.
    pub fn pieces_in(&self, container: ItemId) -> usize {
        self.grids.get(&container).map_or(0, |g| g.entries().iter().map(|e| self.item(e.id).map_or(0, |i| i.quantity as usize)).sum())
    }

    /// Picks `id` up: a bag goes on, in its slot, if that slot is free; anything else is
    /// stowed ([`Inventory::stow`]). Wearing a picked-up bag is this build's own rule, so a
    /// pack found on the floor becomes a grid at once.
    pub fn pick_up(&mut self, id: ItemId) -> Result<(), Refusal> {
        let item = self.item(id).ok_or(Refusal::Unknown)?;
        if item.profile.holds.is_some()
            && let Some(slot) = item.profile.slot.filter(|s| CARRYING_SLOTS.contains(s))
            && self.worn(slot).is_none()
            && self.wear(id, slot).is_ok()
        {
            return Ok(());
        }
        self.stow(id).map(|_| ())
    }

    /// Whether `id` could be worn in `slot`, without moving anything. Something already worn
    /// there is no bar: equipping swaps it out ([`Inventory::equip`]).
    pub fn check_wear(&self, id: ItemId, slot: Slot) -> Result<(), Refusal> {
        let item = self.item(id).ok_or(Refusal::Unknown)?;
        item.profile.slot.ok_or(Refusal::NotWearable)?;
        // The weapon strap holds one long weapon, and is the worn pack's own.
        if slot == Slot::Strap {
            if !item.strappable() {
                return Err(Refusal::WrongSlot);
            }
            return if self.has_strap() { Ok(()) } else { Err(Refusal::NoStrap) };
        }
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
                None => Displaced::Folded { container },
                _ => Displaced::Underfoot,
            },
            Err(_) => Displaced::Underfoot,
        };
        Some((id, went))
    }

    /// Whether the pack worn on the back was rolled with a weapon strap
    /// ([`fb_items::Accessory::WeaponStrap`]).
    pub fn has_strap(&self) -> bool {
        self.worn(Slot::Back).and_then(|id| self.item(id)).is_some_and(|pack| pack.has_accessory(fb_items::Accessory::WeaponStrap))
    }

    /// With no pack that has a weapon strap worn, the weapon strapped on comes off: into the first
    /// carried bag with room ([`Inventory::stow`]), or else loose, to be set down underfoot (a long
    /// one often cannot be packed). Returns it and where it went.
    pub fn unstrap(&mut self) -> Option<(ItemId, Displaced)> {
        if self.has_strap() {
            return None;
        }
        let id = self.worn(Slot::Strap)?;
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

    /// The weapon strapped to the worn pack.
    pub fn strapped(&self) -> Option<ItemId> {
        self.worn(Slot::Strap)
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
            // Drawn from the strap, the weapon it was swapped for (a mace, a torch) cannot go back on
            // it: it is stowed if a bag has room.
            Some(Location::Worn(Slot::Strap)) => match self.stow(old) {
                Ok(container) => match self.location(old) {
                    Some(Location::In { at, .. }) => Displaced::In { container, at },
                    _ => Displaced::Underfoot,
                },
                Err(_) => Displaced::Underfoot,
            },
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
            g.entries().iter().map(|e| self.item(e.id).map_or(0.0, |i| i.profile.stats.weight * i.quantity as f32) + self.contents_weight(e.id)).sum()
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
        let pack = inventory.add(rolled("Pack", 1));
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

    /// A pack rolled with (or without) a weapon strap.
    fn pack_strapped(strap: bool) -> Item {
        (1..2000).map(|s| rolled("Pack", s)).find(|p| p.has_accessory(fb_items::Accessory::WeaponStrap) == strap).expect("a pack of that kind")
    }

    #[test]
    fn a_weapon_strap_holds_one_long_weapon_outside_the_grid() {
        let mut inventory = Inventory::default();
        let staff = inventory.add(rolled("Staff", 3));
        let dagger = inventory.add(rolled("Dagger", 3));
        assert_eq!(inventory.check_wear(staff, Slot::Strap), Err(Refusal::NoStrap), "no pack");
        let bare = inventory.add(pack_strapped(false));
        inventory.wear(bare, Slot::Back).unwrap();
        assert_eq!(inventory.check_wear(staff, Slot::Strap), Err(Refusal::NoStrap), "a pack with no strap");
        let pack = inventory.add(pack_strapped(true));
        inventory.equip(pack, Slot::Back).unwrap();
        assert_eq!(inventory.check_wear(dagger, Slot::Strap), Err(Refusal::WrongSlot), "not a long weapon");
        assert_eq!(inventory.equip(staff, Slot::Strap), Ok(None));
        assert_eq!(inventory.strapped(), Some(staff));
        assert_eq!(inventory.contents_count(pack), 0, "not in the pack's grid");
        assert!(inventory.is_carried(staff));
        assert!(inventory.equipped().iter().all(|(slot, _)| *slot != Slot::Strap), "it gives no stats, only weight");
        // One only: another long weapon swaps it out.
        let spear = inventory.add(rolled("Spear", 3));
        assert_eq!(inventory.equip(spear, Slot::Strap).map(|d| d.map(|(old, _)| old)), Ok(Some(staff)));
    }

    #[test]
    fn drawing_from_the_strap_stows_what_cannot_go_back_on_it() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(pack_strapped(true));
        inventory.wear(pack, Slot::Back).unwrap();
        let spear = inventory.add(rolled("Spear", 3));
        inventory.wear(spear, Slot::Strap).unwrap();
        let mace = inventory.add(rolled("Dagger", 2));
        inventory.wear(mace, Slot::Mainhand).unwrap();
        // The spear is drawn into the main hand; the dagger cannot go on the strap, so it is packed.
        let swapped = inventory.equip(spear, Slot::Mainhand).unwrap();
        assert!(matches!(swapped, Some((old, Displaced::In { container, .. })) if old == mace && container == pack), "{swapped:?}");
        assert_eq!((inventory.strapped(), inventory.worn(Slot::Mainhand)), (None, Some(spear)));
    }

    #[test]
    fn a_strapped_weapon_comes_off_with_its_pack() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(pack_strapped(true));
        inventory.wear(pack, Slot::Back).unwrap();
        let staff = inventory.add(rolled("Staff", 3));
        inventory.wear(staff, Slot::Strap).unwrap();
        assert_eq!(inventory.unstrap(), None, "the pack is still on");
        let bare = inventory.add(pack_strapped(false));
        inventory.equip(bare, Slot::Back).unwrap();
        let (id, _) = inventory.unstrap().expect("the strap is gone");
        assert_eq!((id, inventory.strapped()), (staff, None));
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

    #[test]
    fn a_quiver_takes_only_arrows_and_a_bolt_case_only_bolts() {
        let mut inventory = Inventory::default();
        let (quiver, case) = (inventory.add(rolled("Quiver", 1)), inventory.add(rolled("Bolt case", 1)));
        let (arrow, bolt, bread) = (inventory.add(rolled("Arrow", 1)), inventory.add(rolled("Bolt", 1)), inventory.add(rolled("Bread", 1)));
        let at = Placement::new(0, 0, false);
        assert_eq!(inventory.put(arrow, quiver, at), Ok(()));
        assert_eq!(inventory.check_put(bolt, quiver, at), Err(Refusal::NotAccepted));
        assert_eq!(inventory.check_put(bread, quiver, at), Err(Refusal::NotAccepted));
        assert_eq!(inventory.check_put(bolt, case, at), Ok(()));
        assert_eq!(inventory.check_put(arrow, case, Placement::new(1, 0, false)), Err(Refusal::NotAccepted));
        // Twenty arrows fit, found by the quiver before a pack.
        inventory.wear(quiver, Slot::Quiver).unwrap();
        for seed in 2..21 {
            let more = inventory.add(rolled("Arrow", seed));
            assert_eq!(inventory.stow(more), Ok(quiver));
        }
        assert_eq!(inventory.contents_count(quiver), 20);
    }

    #[test]
    fn fourteen_arrows_make_a_stack_of_ten_and_one_of_four() {
        let mut inventory = Inventory::default();
        let quiver = inventory.add(rolled("Quiver", 1));
        inventory.wear(quiver, Slot::Quiver).unwrap();
        for _ in 0..14 {
            let arrow = inventory.add(rolled("Arrow", 5));
            assert_eq!(inventory.stow(arrow), Ok(quiver));
        }
        let mut counts: Vec<u32> = inventory.grid(quiver).unwrap().entries().iter().map(|e| inventory.item(e.id).unwrap().quantity).collect();
        counts.sort();
        assert_eq!(counts, vec![4, 10]);
        assert_eq!(inventory.pieces_in(quiver), 14);
        // A bundle of fourteen lying together splits the same way.
        let mut other = Inventory::default();
        let quiver = other.add(rolled("Quiver", 1));
        other.wear(quiver, Slot::Quiver).unwrap();
        let bundle = other.add(rolled("Arrow", 5).with_quantity(14));
        assert_eq!(other.stow(bundle), Ok(quiver));
        assert_eq!((other.contents_count(quiver), other.pieces_in(quiver)), (2, 14));
    }

    #[test]
    fn two_kinds_of_arrow_never_share_a_stack_and_one_comes_off_a_stack_at_a_time() {
        let mut inventory = Inventory::default();
        let quiver = inventory.add(rolled("Quiver", 1));
        inventory.wear(quiver, Slot::Quiver).unwrap();
        for seed in [5, 6, 5, 6] {
            let arrow = inventory.add(rolled("Arrow", seed));
            inventory.stow(arrow).unwrap();
        }
        assert_eq!((inventory.contents_count(quiver), inventory.pieces_in(quiver)), (2, 4));
        let stack = inventory.grid(quiver).unwrap().entries()[0].id;
        let one = inventory.take_one(stack).unwrap();
        assert_ne!(one, stack);
        assert_eq!((inventory.item(one).unwrap().quantity, inventory.item(stack).unwrap().quantity), (1, 1));
        // Put back, it folds into its stack.
        inventory.stow(one).unwrap();
        assert_eq!(inventory.item(stack).unwrap().quantity, 2);
        assert!(inventory.item(one).is_none());
        // The last of a stack comes off whole.
        let last = inventory.take_one(inventory.grid(quiver).unwrap().entries()[1].id).unwrap();
        assert_eq!(inventory.take_one(last), Some(last));
    }

    /// A quiver worn with stacks of `counts` arrows of one kind in it.
    fn quiver_with(counts: &[u32]) -> (Inventory, ItemId, Vec<ItemId>) {
        let mut inventory = Inventory::default();
        let quiver = inventory.add(rolled("Quiver", 1));
        inventory.wear(quiver, Slot::Quiver).unwrap();
        let ids = counts
            .iter()
            .map(|&n| {
                let id = inventory.add(rolled("Arrow", 5).with_quantity(n));
                let at = inventory.grid(quiver).unwrap().first_free(Footprint { cols: 1, rows: 1 }).unwrap();
                inventory.put(id, quiver, at).unwrap();
                id
            })
            .collect();
        (inventory, quiver, ids)
    }

    fn quantity(inventory: &Inventory, id: ItemId) -> Option<u32> {
        inventory.item(id).map(|i| i.quantity)
    }

    #[test]
    fn dropping_a_stack_on_another_tops_it_up_and_keeps_the_rest_in_hand() {
        // 10 onto 4: six move, four stay where they were.
        let (mut inventory, _, ids) = quiver_with(&[4, 10]);
        assert_eq!(inventory.merge_onto(ids[1], ids[0]), Ok(false));
        assert_eq!((quantity(&inventory, ids[0]), quantity(&inventory, ids[1])), (Some(10), Some(4)));
        // 4 onto a full 10: nothing moves.
        assert_eq!(inventory.merge_onto(ids[1], ids[0]), Err(Refusal::NoRoom));
        assert_eq!((quantity(&inventory, ids[0]), quantity(&inventory, ids[1])), (Some(10), Some(4)));
        // 6 onto 3 is 9, the dropped one gone.
        let (mut inventory, quiver, ids) = quiver_with(&[3, 6]);
        assert_eq!(inventory.merge_onto(ids[1], ids[0]), Ok(true));
        assert_eq!((quantity(&inventory, ids[0]), quantity(&inventory, ids[1])), (Some(9), None));
        assert_eq!(inventory.contents_count(quiver), 1);
        // Another kind of arrow never merges.
        let other = inventory.add(rolled("Arrow", 6));
        assert_eq!(inventory.merge_onto(other, ids[0]), Err(Refusal::NoRoom));
        assert!(inventory.stack_at(other, quiver, 0, 0).is_none());
    }

    #[test]
    fn a_stack_is_found_under_the_cell_it_is_dropped_on() {
        let (mut inventory, quiver, ids) = quiver_with(&[4]);
        let hand = inventory.add(rolled("Arrow", 5).with_quantity(3));
        assert_eq!(inventory.stack_at(hand, quiver, 0, 0), Some(ids[0]));
        assert_eq!(inventory.stack_at(hand, quiver, 1, 0), None);
    }

    #[test]
    fn a_pickup_with_no_room_for_the_rest_is_partial_and_the_rest_stays_loose() {
        let mut inventory = Inventory::default();
        let quiver = inventory.add(rolled("Quiver", 1));
        inventory.wear(quiver, Slot::Quiver).unwrap();
        let (cols, rows) = {
            let g = inventory.grid(quiver).unwrap();
            (g.cols, g.rows)
        };
        for k in 0..cols as u32 * rows as u32 - 1 {
            let other = inventory.add(rolled("Arrow", 100 + k));
            inventory.stow(other).unwrap();
        }
        let ours = inventory.add(rolled("Arrow", 5).with_quantity(7));
        inventory.stow(ours).unwrap();
        let more = inventory.add(rolled("Arrow", 5).with_quantity(6));
        assert_eq!(inventory.stow(more), Ok(quiver), "three fit the stack of seven");
        assert_eq!(quantity(&inventory, ours), Some(10));
        assert!(inventory.is_partial(more));
        assert_eq!(quantity(&inventory, more), Some(3));
        let none = inventory.add(rolled("Arrow", 5).with_quantity(2));
        assert_eq!(inventory.stow(none), Err(Refusal::NoRoom));
        assert_eq!(quantity(&inventory, none), Some(2));
    }

    #[test]
    fn a_quiver_is_slung_in_its_own_slot_with_the_hip_and_back_taken_and_no_belt_needed() {
        let mut inventory = Inventory::default();
        let (pack, belt, lantern) = (inventory.add(rolled("Pack", 1)), inventory.add(rolled("Belt", 1)), inventory.add(rolled("Lantern", 1)));
        inventory.wear(pack, Slot::Back).unwrap();
        inventory.wear(belt, Slot::Belt).unwrap();
        inventory.wear(lantern, Slot::Hip).unwrap();
        let quiver = inventory.add(rolled("Quiver", 2));
        assert_eq!(inventory.check_wear(quiver, Slot::Hip), Err(Refusal::WrongSlot));
        assert_eq!(inventory.check_wear(quiver, Slot::Back), Err(Refusal::WrongSlot));
        assert_eq!(inventory.target_slot(quiver), Some(Slot::Quiver));
        inventory.pick_up(quiver).unwrap();
        assert_eq!(inventory.worn(Slot::Quiver), Some(quiver));
        assert!(inventory.carried_containers().contains(&quiver));
        let arrow = inventory.add(rolled("Arrow", 3));
        assert_eq!(inventory.stow(arrow), Ok(quiver), "an arrow picked up goes in the quiver, not the pack");
        // Taking the belt off drops the lantern from the hip but leaves the quiver slung.
        inventory.take_out(belt);
        inventory.unhang();
        assert_eq!(inventory.worn(Slot::Quiver), Some(quiver));
    }

    #[test]
    fn a_satchel_is_worn_beside_a_pack_but_a_second_pack_has_no_room() {
        let mut inventory = Inventory::default();
        let (pack, satchel, sack) = (inventory.add(rolled("Pack", 1)), inventory.add(rolled("Satchel", 1)), inventory.add(rolled("Sack", 1)));
        assert_eq!((inventory.target_slot(pack), inventory.target_slot(satchel), inventory.target_slot(sack)), (Some(Slot::Back), Some(Slot::Satchel), Some(Slot::Back)));
        assert_eq!(inventory.check_wear(satchel, Slot::Back), Err(Refusal::WrongSlot));
        assert_eq!(inventory.check_wear(pack, Slot::Satchel), Err(Refusal::WrongSlot));
        inventory.pick_up(pack).unwrap();
        inventory.pick_up(satchel).unwrap();
        assert_eq!((inventory.worn(Slot::Back), inventory.worn(Slot::Satchel)), (Some(pack), Some(satchel)));
        assert_eq!(inventory.carried_containers(), vec![pack, satchel]);
        // The back is taken: a sack is stowed, not worn, and wearing it would take the pack's place.
        inventory.pick_up(sack).unwrap();
        assert_eq!((inventory.worn(Slot::Back), inventory.worn(Slot::Satchel)), (Some(pack), Some(satchel)));
        assert!(!matches!(inventory.location(sack), Some(Location::Worn(_))));
    }
}
