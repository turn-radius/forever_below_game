//! Bartering with folk: the Barter key (Secondary, G by default) on a folk in reach, or the
//! dialogue panel's Barter topic (`docs/design.md`, "Barter").
//!
//! - A folk of a Hostile or Wary faction, or one who has turned on the delver, will not trade
//!   ("The Mudfoot clan won't trade with you."). Plain folk, guards and enchanters have no shelf:
//!   "<Name> has nothing to sell." A trader of Neutral or better standing opens the inventory
//!   screen with their shelf as a grid of its own beside the delver's pack, and a balance
//!   ([`panel`]).
//! - The shelf is a container the screen holds only while bartering ([`Session::shelf`], shown
//!   through [`crate::inventory::OpenContainer`]). Dragging between it and the pack builds a
//!   [`Deal`]: what moved off the shelf is bought, what moved onto it is sold. Nothing is final
//!   until Accept; Cancel, Esc or leaving the screen puts everything back as it was.
//! - Prices are [`price`] (what the trader asks) and [`offer`] (what they pay), from
//!   `data/barter_rules.json`: our own numbers, tuned by tier and by the faction's standing.
//! - Accept ([`settle`]) is all or nothing. The delver pays from their coin purse, then any
//!   purses they carry unopened ([`crate::enchanting::pay`]), and is paid into their coin purse
//!   ([`Inventory::coins`]); the trader pays from the purse on their shelf
//!   ([`crate::shelf::Shelf::purse`]), so they can run out. What the delver sells stays on the
//!   shelf through a restock. A trade moves the faction's standing a little and meets it.
//!
//! `FB_BARTER=1` presses Barter on the trader nearest the delver a few dozen frames in, for
//! captures (`FB_BARTER_PICK=<n>` on the nth nearest, 0 the first). `FB_BARTER=deal` also gives the delver 40 gp and starts a deal (the first ware to
//! the pack, the first thing they carry to the shelf), and `FB_BARTER=accept` then accepts it.

mod portrait;

pub use portrait::Look;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use bevy::{
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_faction::{Band, Faction, StandingRules, Tier, Trade};
use fb_items::{Coins, Item, Slot};
use serde::Deserialize;

use crate::combat::feedback::Corpse;
use crate::enchanting::{plan_pay, wallet};
use crate::fitting::FitCache;
use crate::folk::{AimedFolk, Denizen};
use crate::folk_routine::Routine;
use crate::hostility::Hostility;
use crate::inventory::card::Price;
use crate::inventory::{Drag, FloorChange, GridView, Inventory, ItemId, Location, OpenContainer, Placement, Refusal, Target, Toasts};
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, Player, Screen, ScreenOpen, show_screen};
use crate::record::WorldRecord;
use crate::shelf::Shelf;
use crate::talk::{Service, ServiceChosen};
use crate::ui::{ACCENT, BAD, BRIGHT, CELL, FAINT, INK, LINE, MUTED, OK, UiFonts, coin_spans, node_rect, ui_cursor};

/// A number for each tier.
#[derive(Clone, Copy, Debug, Deserialize)]
struct ByTier<T> {
    novice: T,
    apprentice: T,
    expert: T,
    master: T,
}

impl<T: Copy> ByTier<T> {
    fn of(&self, tier: Tier) -> T {
        match tier {
            Tier::Novice => self.novice,
            Tier::Apprentice => self.apprentice,
            Tier::Expert => self.expert,
            Tier::Master => self.master,
        }
    }
}

/// A number for each band that trades.
#[derive(Clone, Copy, Debug, Deserialize)]
struct ByBand {
    neutral: f64,
    friendly: f64,
    allied: f64,
}

impl ByBand {
    fn of(&self, band: Band) -> Option<f64> {
        match band {
            Band::Hostile | Band::Wary => None,
            Band::Neutral => Some(self.neutral),
            Band::Friendly => Some(self.friendly),
            Band::Allied => Some(self.allied),
        }
    }
}

/// `data/barter_rules.json`.
#[derive(Clone, Copy, Debug, Deserialize)]
struct Rules {
    /// What a tier's wares cost over their value.
    tier_markup: ByTier<f64>,
    /// What the standing does to what the trader asks: a markup at Neutral, fair at Friendly, a
    /// discount at Allied.
    sell_band: ByBand,
    /// What the standing does to what the trader pays.
    buy_band: ByBand,
    /// The share of an item's value a trader pays for goods of their trade.
    trade_share: f64,
    /// The share of that they pay for anything else: half.
    other_share: f64,
    /// The coin a trader of each tier starts with, in copper.
    purse_copper: ByTier<u64>,
}

static RULES: LazyLock<Rules> =
    LazyLock::new(|| serde_json::from_str(include_str!("../../data/barter_rules.json")).unwrap_or_else(|e| panic!("data/barter_rules.json: {e}")));

/// The coin a trader of `tier` starts with each day, in copper.
pub fn purse_copper(tier: Tier) -> u64 {
    RULES.purse_copper.of(tier)
}

/// Whether `item` is among the goods of `trade`'s line of work.
pub fn is_their_trade(trade: Trade, item: &Item) -> bool {
    trade.shop("", "").is_some_and(|shop| shop.line.bases().contains(&item.base.as_str()))
}

/// What a trader of `trade` asks of a delver in `band` for `item` from their shelf, in copper:
/// its value, times the tier's markup, times the band's. `None` if that band will not trade.
pub fn price(item: &Item, trade: Trade, band: Band) -> Option<u64> {
    let band = RULES.sell_band.of(band)?;
    let worth = item.stack_value_copper() as f64 * RULES.tier_markup.of(trade.tier) * band;
    Some((worth.round() as u64).max(1))
}

/// What a trader of `trade` pays a delver in `band` for `item`, in copper: a fair share of its
/// value for goods of their trade, half that for anything else, scaled by the band. `None` if
/// that band will not trade.
pub fn offer(item: &Item, trade: Trade, band: Band) -> Option<u64> {
    let band = RULES.buy_band.of(band)?;
    let share = if is_their_trade(trade, item) { RULES.trade_share } else { RULES.trade_share * RULES.other_share };
    let worth = item.stack_value_copper() as f64 * share * band;
    Some((worth.round() as u64).max(1))
}

/// What the delver has put in the deal so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Deal {
    /// Wares taken off the shelf.
    pub buying: Vec<ItemId>,
    /// Things put on it.
    pub selling: Vec<ItemId>,
    /// What the trader asks for the wares, in copper.
    pub cost: u64,
    /// What the trader pays for the things, in copper.
    pub offer: u64,
}

impl Deal {
    /// What the trader owes the delver, in copper: negative when the delver owes them.
    pub fn balance(&self) -> i64 {
        self.offer as i64 - self.cost as i64
    }

    pub fn is_empty(&self) -> bool {
        self.buying.is_empty() && self.selling.is_empty()
    }
}

/// A barter in progress.
#[derive(Clone, Debug)]
pub struct Session {
    pub folk: Entity,
    pub name: String,
    /// The faction as it reads in a sentence ("the Mudfoot clan").
    pub faction_name: String,
    pub faction: Faction,
    pub trade: Trade,
    pub band: Band,
    /// The container whose grid is the trader's shelf, in the inventory.
    pub shelf: ItemId,
    /// The shelf's items that the day's roll put out; the rest were sold in.
    rolled: BTreeSet<ItemId>,
    /// Everything on the shelf as the deal began (or was last settled): what a deal may buy.
    stock: BTreeSet<ItemId>,
    /// Where each item the deal has moved since it began (or was last settled) was before its
    /// first move, so Cancel puts back just those and leaves everything else as it is.
    moved: BTreeMap<ItemId, Location>,
    /// The trader's spot seed, which says which folk this is across levels and days.
    pub spot: u32,
    /// The trader's coin as it stood then, in copper.
    pub purse: u64,
    /// What the trader looks like, for the portrait over their shelf.
    pub look: Look,
}

impl Session {
    fn on_shelf(&self, inventory: &Inventory, id: ItemId) -> bool {
        matches!(inventory.location(id), Some(Location::In { container, .. }) if container == self.shelf)
    }

    /// The deal as it stands.
    pub fn deal(&self, inventory: &Inventory) -> Deal {
        let mut deal = Deal::default();
        for &id in &self.stock {
            if !self.on_shelf(inventory, id)
                && inventory.is_carried(id)
                && let Some(item) = inventory.item(id)
            {
                deal.buying.push(id);
                deal.cost += price(item, self.trade, self.band).unwrap_or(0);
            }
        }
        for entry in inventory.grid(self.shelf).map(|g| g.entries()).unwrap_or_default() {
            if self.stock.contains(&entry.id) {
                continue;
            }
            if let Some(item) = inventory.item(entry.id) {
                deal.selling.push(entry.id);
                deal.offer += offer(item, self.trade, self.band).unwrap_or(0);
            }
        }
        deal
    }

    /// The price on `id`'s card and in its cell while bartering: what a ware costs, and whether
    /// the delver can pay it, or what the trader pays for something of the delver's.
    pub fn tag(&self, inventory: &Inventory, id: ItemId) -> Option<Price> {
        let item = inventory.item(id)?;
        if item.coins.is_some() || id == self.shelf {
            return None;
        }
        let worth = item.stack_value();
        if self.stock.contains(&id) {
            return price(item, self.trade, self.band).map(|p| Price::Costs { price: Coins::from_copper(p), worth, afford: wallet(inventory).total_copper() >= p });
        }
        if inventory.is_carried(id) || self.on_shelf(inventory, id) {
            return offer(item, self.trade, self.band).map(|p| Price::SellsFor { price: Coins::from_copper(p), worth });
        }
        None
    }

    /// The deal re-based: the shelf as it is now is the stock, and Cancel returns here.
    fn rebase(&mut self, inventory: &Inventory) {
        self.stock = inventory.grid(self.shelf).map(|g| g.entries().iter().map(|e| e.id).collect()).unwrap_or_default();
        self.rolled.retain(|id| self.stock.contains(id));
        self.moved.clear();
    }

    /// Notes where `id` is, if the deal has not moved it yet, before it moves.
    fn note(&mut self, inventory: &Inventory, id: ItemId) {
        if let Some(at) = inventory.location(id) {
            self.moved.entry(id).or_insert(at);
        }
    }
}

/// How an item dropped while bartering is to be moved.
enum Move {
    Nothing,
    Shelve(Placement),
    Put(ItemId, Placement),
    Wear(Slot),
}

impl Session {
    /// Where, and whether, `drag` may land at `target` while bartering. `None` is for what the
    /// ordinary rules decide (a quickslot).
    fn judge(&self, inventory: &Inventory, drag: &Drag, target: Target) -> Option<Result<Move, Refusal>> {
        let id = drag.id;
        match target {
            Target::Quickslot(_) => return None,
            Target::Page => return Some(Ok(Move::Nothing)),
            Target::Level | Target::Cell { view: GridView::Underfoot, .. } => return Some(Err(Refusal::Bartering)),
            _ => {}
        }
        // Only the pack, what is worn and the shelf are in play: nothing lying about.
        if !(inventory.is_carried(id) || self.on_shelf(inventory, id)) {
            return Some(Err(Refusal::Bartering));
        }
        Some(match target {
            Target::Cell { view: GridView::Container(container), cell } => {
                if container == self.shelf {
                    self.sell_check(inventory, id).and_then(|()| {
                        let place = |at: Placement| inventory.check_shelve(id, container, at).map(|()| at);
                        let at = drag.placement_at(cell).ok_or(Refusal::NoRoom).and_then(place);
                        // Worn things are drawn upright; one that fits only turned goes turned.
                        match at {
                            Err(refusal) if !drag.turned && matches!(inventory.location(id), Some(Location::Worn(_))) => {
                                let mut turned = *drag;
                                turned.turn();
                                turned.placement_at(cell).and_then(|at| place(at).ok()).ok_or(refusal)
                            }
                            other => other,
                        }
                        .map(Move::Shelve)
                    })
                } else if inventory.is_carried(container) {
                    crate::inventory::landing(inventory, drag, container, cell).map(|at| Move::Put(container, at))
                } else {
                    Err(Refusal::Bartering)
                }
            }
            Target::Slot(slot) => self.wear_check(inventory, id, slot).map(|()| Move::Wear(slot)),
            Target::Doll => {
                let slot = inventory.target_slot(id).ok_or(Refusal::NotWearable);
                slot.and_then(|slot| self.wear_check(inventory, id, slot).map(|()| Move::Wear(slot)))
            }
            _ => Err(Refusal::Bartering),
        })
    }

    /// Whether `id` may be put on the shelf: goods, and bags with nothing in them.
    fn sell_check(&self, inventory: &Inventory, id: ItemId) -> Result<(), Refusal> {
        let item = inventory.item(id).ok_or(Refusal::Unknown)?;
        if item.coins.is_some() {
            return Err(Refusal::NotAWare);
        }
        // A pack or belt with a tent tied on or a weapon hung from it is not empty either.
        if inventory.holding(id) > 0 {
            return Err(Refusal::HoldsThings);
        }
        Ok(())
    }

    /// Whether `id` may be worn in `slot` now: only a free one, so nothing is displaced onto the
    /// floor mid-deal.
    fn wear_check(&self, inventory: &Inventory, id: ItemId, slot: Slot) -> Result<(), Refusal> {
        if inventory.worn(slot).is_some_and(|other| other != id) {
            return Err(Refusal::NoRoom);
        }
        inventory.check_wear(id, slot)
    }

    fn make(&self, inventory: &mut Inventory, id: ItemId, how: Move) -> Result<(), Refusal> {
        match how {
            Move::Nothing => Ok(()),
            Move::Shelve(at) => inventory.shelve_at(id, self.shelf, at),
            Move::Put(container, at) => inventory.put(id, container, at),
            Move::Wear(slot) => inventory.wear(id, slot),
        }
    }
}

/// The barter in progress, if any.
#[derive(Resource, Default, Debug)]
pub struct Barter {
    pub session: Option<Session>,
    /// The texture the trader's portrait is drawn into, while the barter lasts.
    pub portrait: Option<Handle<Image>>,
    /// What the last Accept said, and the deal's totals it was said about: it shows only while
    /// the deal is still that one.
    notice: Option<(String, (u64, u64))>,
    /// The game day each folk (by spot seed) last moved the delver's standing by trading.
    traded: BTreeMap<u32, u32>,
}

impl Barter {
    pub fn active(&self) -> bool {
        self.session.is_some()
    }

    /// A barter in progress with nobody behind it, for tests of screens that change while
    /// bartering.
    #[cfg(test)]
    pub fn started_for_test() -> Barter {
        let session = Session {
            folk: Entity::PLACEHOLDER,
            name: String::new(),
            faction_name: String::new(),
            faction: Faction(0),
            trade: Trade::guard(),
            band: Band::Neutral,
            shelf: ItemId(0),
            rolled: BTreeSet::new(),
            stock: BTreeSet::new(),
            moved: BTreeMap::new(),
            spot: 0,
            purse: 0,
            look: Look { spec: fb_body::BodySpec::roll(fb_body::Folk::Human, 1), gear: vec![], dirt: 0.0 },
        };
        Barter { session: Some(session), ..default() }
    }

    /// Whether a drop of `drag` at `target` would work, if bartering decides it.
    pub fn check(&self, inventory: &Inventory, drag: &Drag, target: Target) -> Option<Result<(), Refusal>> {
        let session = self.session.as_ref()?;
        session.judge(inventory, drag, target).map(|r| r.map(|_| ()))
    }

    /// Lets `drag` go at `target`, if bartering decides it.
    pub fn drop_at(&mut self, inventory: &mut Inventory, drag: &Drag, target: Target) -> Option<Result<Vec<FloorChange>, Refusal>> {
        let session = self.session.as_mut()?;
        let how = session.judge(inventory, drag, target)?;
        if how.is_ok() {
            session.note(inventory, drag.id);
        }
        let session = &*session;
        Some(how.and_then(|how| session.make(inventory, drag.id, how)).map(|()| vec![]))
    }

    /// A double-click on `id`: a ware goes to the pack (or the hands, if it is too long for the
    /// pack); the delver's own goes on the shelf. A purse of theirs is opened as at any other
    /// time (`None`, left to [`crate::inventory::quick`]).
    pub fn quick(&mut self, inventory: &mut Inventory, id: ItemId) -> Option<Result<Vec<FloorChange>, Refusal>> {
        let session = self.session.as_mut()?;
        if inventory.is_carried(id) && inventory.item(id).is_some_and(|i| i.coins.is_some()) {
            return None;
        }
        session.note(inventory, id);
        let session = &*session;
        let done = if session.on_shelf(inventory, id) {
            inventory.stow_apart(id).map(|_| ()).or_else(|refusal| match inventory.target_slot(id) {
                Some(slot) if inventory.item(id).is_some_and(|i| i.profile.slot.is_some()) => session.wear_check(inventory, id, slot).and_then(|()| inventory.wear(id, slot)),
                _ => Err(refusal),
            })
        } else if inventory.is_carried(id) {
            session.sell_check(inventory, id).and_then(|()| inventory.shelve(id, session.shelf).map(|_| ()))
        } else {
            Err(Refusal::Bartering)
        };
        Some(done.map(|()| vec![]))
    }

    /// Moves the faction's standing for a trade with the folk at `spot` on game day `day`, and
    /// meets it: once per folk per game day, so trading small things back and forth cannot farm
    /// it. Returns whether it moved.
    pub fn trade_standing(&mut self, standings: &mut fb_faction::Standings, spot: u32, faction: Faction, day: u32) -> bool {
        if self.traded.get(&spot) == Some(&day) {
            standings.meet(faction);
            return false;
        }
        self.traded.insert(spot, day);
        standings.on_trade_with(&StandingRules::default(), faction);
        true
    }

    /// The price for `id`'s card and cell, while bartering.
    pub fn tag(&self, inventory: &Inventory, id: ItemId) -> Option<Price> {
        self.session.as_ref()?.tag(inventory, id)
    }
}

/// Run condition: no barter is going on, so the inventory screen's ordinary hotkeys (eat, wear,
/// light, drop what is under the mouse) are free to act.
pub fn not_bartering(barter: Option<Res<Barter>>) -> bool {
    barter.is_none_or(|b| !b.active())
}

/// A shelf's container for a trader of `name`: `cols` wide and `rows` deep.
fn shelf_container(name: &str, cols: u8, rows: u8) -> Option<Item> {
    let mut chest = fb_items::roll_item("Chest", 0)?;
    let label = format!("{name}'s wares");
    chest.profile.holds = Some(fb_items::footprint::Grid { cols, rows, label, accepts: vec![] });
    Some(chest)
}

/// The most rows a shelf may have, which is a great many wares and sold-in things.
const MAX_SHELF_ROWS: usize = 100;

/// Opens a barter with `person` over `shelf`: sets their wares out on a shelf of their own in
/// `inventory`, sized from what is really on it (a few empty rows over, to put things on). `Err`
/// says why not, with nothing added to the inventory: someone with no trade, or a shelf so
/// crowded it cannot be laid out.
pub fn begin(inventory: &mut Inventory, folk: Entity, person: &crate::folk::Person, band: Band, shelf: &Shelf) -> Result<Session, String> {
    let nothing = || format!("{} has nothing to sell.", person.name);
    let trade = person.spot.trade.ok_or_else(nothing)?;
    let cols = 5u8;
    // Far more than the most rows hold, so that laying it out cannot run away.
    let cells: usize = shelf.all().map(|i| usize::from(i.profile.footprint.cols) * usize::from(i.profile.footprint.rows)).sum();
    if cells > MAX_SHELF_ROWS * usize::from(cols) {
        return Err(format!("{}'s stall is too crowded to trade at.", person.name));
    }
    let laid = crate::inventory::Grid::<usize>::pack(cols, 5, shelf.all().enumerate().map(|(i, item)| (i, item.profile.footprint)));
    let wares = shelf.all().count();
    if laid.entries().len() < wares || usize::from(laid.rows) > MAX_SHELF_ROWS {
        return Err(format!("{}'s stall is too crowded to trade at.", person.name));
    }
    let rows = (usize::from(laid.rows) + 3).min(MAX_SHELF_ROWS + 3) as u8;
    let container = shelf_container(&person.name, cols, rows).ok_or_else(nothing)?;
    let id = inventory.add(container);
    let mut added = vec![];
    let mut rolled = BTreeSet::new();
    for (item, sold) in shelf.wares.iter().map(|i| (i, false)).chain(shelf.sold.iter().map(|i| (i, true))) {
        let ware = inventory.add(item.clone());
        added.push(ware);
        if inventory.shelve(ware, id).is_err() {
            for ware in added {
                inventory.forget(ware);
            }
            inventory.forget(id);
            return Err(format!("{}'s stall is too crowded to trade at.", person.name));
        }
        if !sold {
            rolled.insert(ware);
        }
    }
    let stock = inventory.grid(id).map(|g| g.entries().iter().map(|e| e.id).collect()).unwrap_or_default();
    Ok(Session {
        folk,
        name: person.name.clone(),
        faction_name: person.faction.name.clone(),
        faction: person.faction.id,
        trade,
        band,
        shelf: id,
        rolled,
        stock,
        moved: BTreeMap::new(),
        spot: person.spot.seed,
        purse: shelf.purse,
        look: Look::of(person),
    })
}

/// Cancels: puts back each item the deal moved, to where it was, and takes the shelf down.
/// Everything else in the inventory is left as it is, and the ids it handed out stay used. A
/// spot taken meanwhile gets the next free one, and a thing of the delver's that has nowhere to
/// go is kept out of the shelf's fate.
pub fn end(inventory: &mut Inventory, session: Session) {
    let shelf = session.shelf;
    let moved: Vec<(ItemId, Location)> = session.moved.into_iter().collect();
    for &(id, _) in &moved {
        inventory.take_out(id);
    }
    for (id, was) in moved {
        let back = match was {
            Location::Worn(slot) => inventory.wear(id, slot),
            Location::In { container, at } if container == shelf => inventory.shelve_at(id, shelf, at),
            Location::In { container, at } => inventory.put(id, container, at),
            Location::Loose => Ok(()),
        };
        if back.is_err() {
            let _ = match was {
                Location::In { container, .. } if container == shelf => inventory.shelve(id, shelf).map(|_| ()),
                _ => inventory.stow_apart(id).map(|_| ()),
            };
        }
    }
    // Whatever of the delver's is still on the shelf (nothing, unless something would not go
    // back) comes off before it is taken down with the rest.
    let strays: Vec<ItemId> = inventory.grid(shelf).map(|g| g.entries().iter().map(|e| e.id).filter(|id| !session.stock.contains(id)).collect()).unwrap_or_default();
    for id in strays {
        if inventory.stow_apart(id).is_err() {
            inventory.take_out(id);
        }
    }
    inventory.forget(shelf);
}

/// What a settled deal did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub bought: usize,
    pub sold: usize,
    /// What the trader paid the delver, in copper; negative if the delver paid them.
    pub balance: i64,
}

/// Settles the deal on `session`: the delver pays or is paid, the wares change hands, the shelf
/// is updated. All or nothing: on `Err` (short of coin, the trader short of coin, nothing to settle)
/// nothing has moved. The standing is the caller's to move.
pub fn settle(inventory: &mut Inventory, session: &mut Session, shelf: &mut Shelf) -> Result<Receipt, String> {
    let deal = session.deal(inventory);
    if deal.is_empty() {
        return Err("Nothing to settle.".into());
    }
    let balance = deal.balance();
    let mut paying = None;
    if balance < 0 {
        let owed = balance.unsigned_abs();
        let held = wallet(inventory).total_copper();
        paying = Some(plan_pay(inventory, owed).ok_or_else(|| format!("You are {} short.", Coins::from_copper(owed - held.min(owed)).short()))?);
    } else if balance > 0 && shelf.purse < balance as u64 {
        return Err(format!("{} cannot afford that: {} in their purse.", session.name, Coins::from_copper(shelf.purse).short()));
    }
    if let Some(payment) = paying
        && !payment.make(inventory)
    {
        return Err("Cannot pay.".into());
    }
    // The coin the delver is paid goes into their coin purse.
    if balance > 0 {
        inventory.add_coins(Coins::from_copper(balance as u64));
    }
    // The shelf as it stands is what the trader has now: rolled wares and sold-in ones apart.
    let entries: Vec<ItemId> = inventory.grid(session.shelf).map(|g| g.entries().iter().map(|e| e.id).collect()).unwrap_or_default();
    shelf.wares.clear();
    shelf.sold.clear();
    for id in entries {
        let Some(item) = inventory.item(id).cloned() else { continue };
        if session.rolled.contains(&id) && !deal.buying.contains(&id) {
            shelf.wares.push(item);
        } else {
            shelf.sold.push(item);
        }
    }
    shelf.purse = (shelf.purse as i64 + deal.cost as i64 - deal.offer as i64).max(0) as u64;
    // What was bought folds into the stacks of its kind the delver carries, now that Cancel has
    // nothing left to put back.
    for &id in &deal.buying {
        inventory.fold_into_stacks(id);
    }
    session.purse = shelf.purse;
    session.rebase(inventory);
    Ok(Receipt { bought: deal.buying.len(), sold: deal.selling.len(), balance })
}

/// The first letter of `text` made a capital.
fn capital(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map(|c| c.to_uppercase().chain(letters).collect()).unwrap_or_default()
}

/// What a folk of `band` who may have turned on the delver says to a Barter that will not go
/// ahead, or `None` if they will trade.
pub fn refusal_line(faction_name: &str, band: Band, turned: bool) -> Option<String> {
    (turned || band <= Band::Wary).then(|| format!("{} won't trade with you.", capital(faction_name)))
}

/// A request to barter with a folk, from a capture or a test, as though Barter were pressed on
/// them.
#[derive(Resource, Default, Debug)]
pub struct BarterRequest(pub Option<Entity>);

pub struct BarterPlugin;

impl Plugin for BarterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Barter>().init_resource::<BarterRequest>().init_resource::<portrait::TraderStage>();
        if std::env::var_os("FB_BARTER").is_some() {
            app.add_systems(Update, (barter_for_capture, deal_for_capture.after(open_barter)).in_set(DelveSystems));
        }
        app.add_systems(OnExit(crate::launch::AppState::Delving), leave_the_delve.before(crate::inventory::leave_the_delve));
        app.add_systems(Update, (open_barter, click_barter, close_barter, hover_buttons).chain().in_set(DelveSystems));
        app.add_systems(Update, (portrait::stage_trader.run_if(resource_exists::<FitCache>), portrait::show_trader, portrait::clear_trader).chain().in_set(DelveSystems));
    }
}

/// Leaving the delve ends a barter in progress, so the shelf's chest and wares do not stay in the
/// inventory, and forgets who the delver has traded with.
pub fn leave_the_delve(mut barter: ResMut<Barter>, mut inventory: ResMut<Inventory>, mut showing: ResMut<OpenContainer>, mut stage: ResMut<portrait::TraderStage>) {
    if let Some(session) = barter.session.take() {
        end(&mut inventory, session);
    }
    if showing.0.is_some() {
        showing.0 = None;
    }
    *barter = Barter::default();
    // The trader's figure goes with the delve; forget it so the same trader is dressed afresh.
    *stage = portrait::TraderStage::default();
}

/// For captures (`FB_BARTER=1`): barters with the folk nearest the delver once the level is up.
fn barter_for_capture(
    player: Query<&Transform, With<Player>>,
    folk: Query<(Entity, &Transform), (With<Denizen>, With<Shelf>, Without<Corpse>)>,
    mut request: ResMut<BarterRequest>,
    mut frames: Local<u32>,
) {
    *frames += 1;
    if *frames != 40 {
        return;
    }
    let Some(at) = player.iter().next().map(|t| t.translation) else { return };
    let mut near: Vec<_> = folk.iter().collect();
    near.sort_by(|a, b| a.1.translation.distance(at).total_cmp(&b.1.translation.distance(at)));
    let nth: usize = std::env::var("FB_BARTER_PICK").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    request.0 = near.get(nth).map(|(e, _)| *e);
}

/// For captures (`FB_BARTER=deal` or `accept`): a few frames after the screen opens, gives the
/// delver 40 gp if they carry none, moves the first ware to the pack and the first
/// thing they carry that is not a purse to the shelf; for `accept`, settles it after that.
fn deal_for_capture(
    mut barter: ResMut<Barter>,
    mut inventory: ResMut<Inventory>,
    mut shelves: Query<&mut Shelf>,
    mut record: Option<ResMut<WorldRecord>>,
    clock: Option<Res<crate::game_clock::GameClock>>,
    mut frames: Local<u32>,
) {
    let mode = std::env::var("FB_BARTER").unwrap_or_default();
    if mode != "deal" && mode != "accept" {
        return;
    }
    let Some(session) = barter.session.as_mut() else { return };
    *frames += 1;
    if *frames == 4 {
        // Something to carry things in, and something of the delver's to sell.
        if inventory.worn(Slot::Back).is_none() {
            let pack = inventory.add(fb_items::roll_item("Pack", 2).expect("a pack"));
            let _ = inventory.wear(pack, Slot::Back);
            let blade = inventory.add(fb_items::roll_item("Dagger", 6).expect("a dagger"));
            let _ = inventory.stow(blade);
        }
        if wallet(&inventory).total_copper() == 0 {
            inventory.add_coins(Coins::from_copper(4000));
        }
        let ware = inventory.grid(session.shelf).and_then(|g| g.entries().first().map(|e| e.id));
        if let Some(ware) = ware {
            let _ = inventory.stow(ware).map(|_| ()).or_else(|_| inventory.target_slot(ware).ok_or(Refusal::NotWearable).and_then(|slot| inventory.wear(ware, slot)));
        }
        let own = inventory.ids().find(|&id| !session.stock.contains(&id) && inventory.is_carried(id) && !matches!(inventory.location(id), Some(Location::Worn(_))) && inventory.item(id).is_some_and(|i| i.coins.is_none() && i.profile.holds.is_none()));
        if let Some(own) = own {
            let _ = inventory.shelve(own, session.shelf);
        }
    }
    if *frames == 30 && mode == "accept" {
        let (folk, spot, faction) = (session.folk, session.spot, session.faction);
        if let Ok(mut shelf) = shelves.get_mut(folk) {
            match settle(&mut inventory, session, &mut shelf) {
                Ok(done) => {
                    if let Some(record) = record.as_deref_mut() {
                        barter.trade_standing(&mut record.standings, spot, faction, clock.as_deref().map_or(0, crate::shelf::day_of));
                    }
                    barter.notice = Some((settled_line(&done), (0, 0)));
                }
                Err(why) => barter.notice = Some((why, (0, 0))),
            }
        }
    }
}

/// The Barter key on a folk in reach, the dialogue's Barter topic, or a request: opens the screen
/// or says why not.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn open_barter(
    input: ActionInput,
    aimed: Res<AimedFolk>,
    mut request: ResMut<BarterRequest>,
    mut chosen: MessageReader<ServiceChosen>,
    mut open: ResMut<ScreenOpen>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
    record: Option<Res<WorldRecord>>,
    mut folk: Query<(&Denizen, Option<&Hostility>, Option<&Shelf>, Option<&mut Routine>), Without<Corpse>>,
    mut inventory: ResMut<Inventory>,
    mut barter: ResMut<Barter>,
    mut showing: ResMut<OpenContainer>,
    mut toasts: Toasts,
) {
    let from_talk = chosen.read().find(|c| c.service == Service::Barter).map(|c| c.folk);
    if barter.active() {
        return;
    }
    let from_key = (open.0.is_none() && input.just_pressed(Action::Secondary)).then_some(aimed.0).flatten();
    let target = from_talk.or_else(|| request.0.take()).or(from_key);
    let Some(entity) = target else { return };
    if open.0.is_some() && !open.is(Screen::Talk) {
        return;
    }
    let Ok((denizen, hostility, shelf, _)) = folk.get(entity) else { return };
    let person = &denizen.person;
    let band = record.as_ref().map_or(Band::Neutral, |r| r.standings.band(person.faction.id));
    // A denizen trades only with one who has learned enough of its tongue.
    if let Some(kind) = person.faction.denizen
        && !record.as_ref().is_some_and(|r| r.lexicon().can_barter(kind))
    {
        toasts.say(format!("The {} shakes its head at your words; you don't share enough of its speech to trade.", kind.key()));
        return;
    }
    if let Some(line) = refusal_line(&person.faction.name, band, hostility.is_some_and(|h| h.is_hostile())) {
        toasts.say(line);
        return;
    }
    let Some(shelf) = shelf else {
        toasts.say(format!("{} has nothing to sell.", person.name));
        return;
    };
    let session = match begin(&mut inventory, entity, person, band, shelf) {
        Ok(session) => session,
        Err(why) => {
            toasts.say(why);
            return;
        }
    };
    if let Ok((_, _, _, Some(mut routine))) = folk.get_mut(entity) {
        routine.hold();
    }
    showing.0 = Some(session.shelf);
    barter.session = Some(session);
    barter.notice = None;
    show_screen(&mut open, Some(Screen::Inventory), cursor.as_deref_mut().map(|c| &mut **c));
}

/// What a button on the balance does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Accept,
    Cancel,
}

/// The trader's name over their shelf.
#[derive(Component, Debug)]
pub struct TraderName;

/// A button on the balance.
#[derive(Component, Debug)]
pub struct BarterButton(pub Act);

/// Accept and Cancel under the mouse.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn click_barter(
    input: ActionInput,
    mut open: ResMut<ScreenOpen>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    buttons: Query<(&BarterButton, &ComputedNode, &UiGlobalTransform)>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
    mut barter: ResMut<Barter>,
    mut inventory: ResMut<Inventory>,
    mut record: Option<ResMut<WorldRecord>>,
    mut shelves: Query<&mut Shelf>,
    clock: Option<Res<crate::game_clock::GameClock>>,
    mut toasts: Toasts,
) {
    if !open.is(Screen::Inventory) || !barter.active() || !input.just_pressed(Action::DragItem) {
        return;
    }
    let Some(point) = windows.iter().next().and_then(|w| ui_cursor(w, ui_scale.as_deref())) else { return };
    let Some((button, ..)) = buttons.iter().find(|(_, node, at)| node_rect(node, at).contains(point)) else { return };
    match button.0 {
        Act::Cancel => show_screen(&mut open, None, cursor.as_deref_mut().map(|c| &mut **c)),
        Act::Accept => {
            let Some(session) = barter.session.as_mut() else { return };
            let (spot, faction) = (session.spot, session.faction);
            let Ok(mut shelf) = shelves.get_mut(session.folk) else { return };
            let totals = {
                let deal = session.deal(&inventory);
                (deal.cost, deal.offer)
            };
            match settle(&mut inventory, session, &mut shelf) {
                Ok(done) => {
                    if let Some(record) = record.as_deref_mut() {
                        barter.trade_standing(&mut record.standings, spot, faction, clock.as_deref().map_or(0, crate::shelf::day_of));
                    }
                    let said = settled_line(&done);
                    toasts.say(said.clone());
                    barter.notice = Some((said, (0, 0)));
                }
                Err(why) => barter.notice = Some((why, totals)),
            }
        }
    }
}

/// "Bought 2, sold 1. Paid 4 gp 5 sp."
pub fn settled_line(done: &Receipt) -> String {
    let mut parts = Vec::new();
    if done.bought > 0 {
        parts.push(format!("Bought {}", done.bought));
    }
    if done.sold > 0 {
        parts.push(format!("sold {}", done.sold));
    }
    let coin = Coins::from_copper(done.balance.unsigned_abs()).short();
    let tail = match done.balance {
        0 => String::new(),
        b if b < 0 => format!(" Paid {coin}."),
        _ => format!(" Received {coin}."),
    };
    format!("{}.{tail}", capital(&parts.join(", ")))
}

/// Ends the barter when the screen closes, or the folk falls or turns on the delver: everything
/// not settled goes back.
#[allow(clippy::type_complexity)]
fn close_barter(
    mut open: ResMut<ScreenOpen>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
    mut barter: ResMut<Barter>,
    mut inventory: ResMut<Inventory>,
    mut showing: ResMut<OpenContainer>,
    mut folk: Query<(Option<&Hostility>, Option<&mut Routine>, Has<Corpse>)>,
    mut toasts: Toasts,
) {
    let Some(session) = barter.session.as_ref() else { return };
    let gone = match folk.get(session.folk) {
        Ok((hostility, _, dead)) => dead || hostility.is_some_and(|h| h.is_hostile()),
        Err(_) => true,
    };
    if open.is(Screen::Inventory) && !gone {
        return;
    }
    if gone && open.is(Screen::Inventory) {
        let turned = folk.get(session.folk).is_ok_and(|(hostility, _, dead)| !dead && hostility.is_some_and(|h| h.is_hostile()));
        if turned {
            toasts.say(format!("{} turns on you.", session.name));
        }
        show_screen(&mut open, None, cursor.as_deref_mut().map(|c| &mut **c));
    }
    let Some(session) = barter.session.take() else { return };
    if let Ok((_, Some(mut routine), _)) = folk.get_mut(session.folk) {
        routine.release();
    }
    end(&mut inventory, session);
    showing.0 = None;
    barter.notice = None;
}

/// Lights the button under the mouse.
fn hover_buttons(
    open: Res<ScreenOpen>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    mut buttons: Query<(&ComputedNode, &UiGlobalTransform, &mut BackgroundColor, &mut BorderColor), With<BarterButton>>,
) {
    if !open.is(Screen::Inventory) {
        return;
    }
    let point = windows.iter().next().and_then(|w| ui_cursor(w, ui_scale.as_deref()));
    for (node, at, mut fill, mut border) in &mut buttons {
        let over = point.is_some_and(|p| node_rect(node, at).contains(p));
        let (want_fill, want_border) = if over { (FAINT, ACCENT) } else { (CELL, LINE) };
        if fill.0 != want_fill {
            fill.0 = want_fill;
        }
        if *border != BorderColor::all(want_border) {
            *border = BorderColor::all(want_border);
        }
    }
}

/// The balance under the shelf's grid: who is trading, what is bought and sold, who owes whom,
/// and the Accept and Cancel buttons.
pub fn panel(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, barter: &Barter, inventory: &Inventory, grid_width: f32) {
    let Some(session) = barter.session.as_ref() else { return };
    let deal = session.deal(inventory);
    let coin = |cp: u64| Coins::from_copper(cp).short();
    let mine = wallet(inventory).total_copper();
    let balance = deal.balance();
    let (line, colour) = match balance {
        0 if deal.is_empty() => ("Drag wares to your pack, and your goods to the shelf.".to_string(), MUTED),
        0 => ("Even.".to_string(), INK),
        b if b < 0 && mine < b.unsigned_abs() => (format!("You owe {}. You are {} short.", coin(b.unsigned_abs()), coin(b.unsigned_abs() - mine)), BAD),
        b if b < 0 => (format!("You owe {}.", coin(b.unsigned_abs())), INK),
        b if session.purse < b as u64 => (format!("{} owes you {}, and has only {}.", session.name, coin(b as u64), coin(session.purse)), BAD),
        b => (format!("{} owes you {}.", session.name, coin(b as u64)), OK),
    };
    let band_colour = match session.band {
        Band::Friendly | Band::Allied => OK,
        _ => MUTED,
    };
    parent
        .spawn((
            Node {
                width: px(grid_width),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                padding: UiRect::new(px(14), px(14), px(10), px(12)),
                border: UiRect::all(px(1.0)),
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            BackgroundColor(CELL),
            BorderColor::all(FAINT),
        ))
        .with_children(|panel| {
            panel.spawn(Node { column_gap: px(14), align_items: AlignItems::Center, ..default() }).with_children(|head| {
                if let Some(portrait) = barter.portrait.clone() {
                    crate::inventory::header::portrait_frame(head, portrait, crate::inventory::doll::PORTRAIT);
                }
                head.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(3), ..default() }).with_children(|who| {
                    who.spawn((TraderName, Text::new(session.name.clone()), UiFonts::style(&fonts.display, 28.0, INK)));
                    who.spawn((Text::new(capital(&session.faction_name)), UiFonts::style(&fonts.body, 15.0, MUTED)));
                    who.spawn((Text::default(), UiFonts::style(&fonts.body, 15.0, MUTED))).with_children(|line| {
                        line.spawn((TextSpan::new(format!("{} \u{b7} ", session.trade.label())), UiFonts::style(&fonts.body, 15.0, MUTED)));
                        line.spawn((TextSpan::new(session.band.label()), UiFonts::style(&fonts.body_medium, 15.0, band_colour)));
                    });
                });
            });
            for (word, count, copper) in [("Buying", deal.buying.len(), deal.cost), ("Selling", deal.selling.len(), deal.offer)] {
                panel.spawn((Text::new(format!("{word} {count} \u{b7} ")), UiFonts::style(&fonts.body, 17.0, INK))).with_children(|row| {
                    coin_spans(row, fonts, Coins::from_copper(copper), 16.0);
                });
            }
            panel.spawn((Text::new(line), UiFonts::style(&fonts.body_medium, 18.0, colour)));
            panel.spawn((Text::new("You carry "), UiFonts::style(&fonts.mono, 12.5, MUTED))).with_children(|row| {
                coin_spans(row, fonts, Coins::from_copper(mine), 12.5);
                row.spawn((TextSpan::new(format!(" \u{b7} {} has ", session.name)), UiFonts::style(&fonts.mono, 12.5, MUTED)));
                coin_spans(row, fonts, Coins::from_copper(session.purse), 12.5);
            });
            if let Some((said, for_totals)) = &barter.notice
                && (*for_totals == (0, 0) || *for_totals == (deal.cost, deal.offer))
            {
                panel.spawn((Text::new(said.clone()), UiFonts::style(&fonts.body_italic, 15.0, BRIGHT)));
            }
            panel.spawn(Node { column_gap: px(10), margin: UiRect::top(px(4)), ..default() }).with_children(|row| {
                for (act, label) in [(Act::Accept, "Accept"), (Act::Cancel, "Cancel")] {
                    row.spawn((
                        BarterButton(act),
                        Node {
                            flex_grow: 1.0,
                            justify_content: JustifyContent::Center,
                            padding: UiRect::new(px(14), px(14), px(7), px(8)),
                            border: UiRect::all(px(1.0)),
                            border_radius: BorderRadius::all(px(8)),
                            ..default()
                        },
                        BackgroundColor(CELL),
                        BorderColor::all(LINE),
                    ))
                    .with_children(|button| {
                        button.spawn((Text::new(label), UiFonts::style(&fonts.body, 19.0, INK)));
                    });
                }
            });
        });
}

#[cfg(test)]
mod tests;
