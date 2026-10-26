//! Hover cards: everything the card says about an item, taken from its `fb_items` item, and
//! where the card goes on screen. The plate's `itemInfoHTML` and `showTip`
//! (`reference/forever-below-plate.html`): name in its rarity's colour, a legendary's title
//! over its old name, a line of what it is and its main numbers, what each affix changes, a
//! container's contents, and a legendary's history. Holding Alt over something that can be
//! worn adds a compare card beside it: what is worn in its place, and what a swap changes.

use bevy::{prelude::*, ui::ComputedUiRenderTargetInfo};
use fb_items::{Coins, Item, Kind, Rarity, Slot, stats::Stats, tables::slot_places};

use super::icon::Icons;
use super::items::{Inventory, ItemId, Location};
use super::{Dragging, Hovered};
use crate::keymap::{Action, ActionInput};
use crate::play::{Screen, ScreenOpen};
use crate::ui::{
    BAD, BG, FAINT, GOLD, INK, LINE, MUTED, OK, UiFonts, coin_spans, numbers_line, rarity_color,
};

/// The price line under a card's title, its coins each in their metal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Price {
    /// What it is worth: "Worth 1 gp 5 sp".
    Worth(Coins),
    /// What a purse holds, opened by a double-click: "Holds 2 gp 3 sp".
    Holds(Coins),
    /// A ware's price while bartering, what it is worth, and whether the delver can pay it.
    Costs {
        price: Coins,
        worth: Coins,
        afford: bool,
    },
    /// What the trader pays for something of the delver's, and what it is worth.
    SellsFor { price: Coins, worth: Coins },
}

impl Price {
    /// The word before the coins, the coins, and the note after them.
    pub fn parts(self) -> (&'static str, Coins, Option<String>) {
        match self {
            Price::Worth(c) => ("Worth", c, None),
            Price::Holds(c) => ("Holds", c, Some("double-click to open".into())),
            Price::Costs { price, worth, .. } => {
                ("Costs", price, Some(format!("worth {}", worth.short())))
            }
            Price::SellsFor { price, worth } => {
                ("Sells for", price, Some(format!("worth {}", worth.short())))
            }
        }
    }

    /// The line as plain text: "Costs 2 gp 3 sp · worth 1 gp 5 sp".
    pub fn line(self) -> String {
        let (word, coins, note) = self.parts();
        match note {
            Some(note) => format!("{word} {} \u{b7} {note}", coins.short()),
            None => format!("{word} {}", coins.short()),
        }
    }
}

/// One affix on the card: what it changes, its name, and whether it is a flaw.
#[derive(Clone, Debug, PartialEq)]
pub struct AffixLine {
    pub text: String,
    pub name: String,
    pub good: bool,
}

/// What a card says.
#[derive(Clone, Debug, PartialEq)]
pub struct CardText {
    /// The title: a legendary's own title, otherwise its name.
    pub title: String,
    pub rarity: Rarity,
    /// A legendary's old name, under its title.
    pub subtitle: Option<String>,
    /// What it is worth, or while bartering what it costs or sells for.
    pub price: Price,
    /// Rarity, what it is, weight, its main numbers and footprint, as one line.
    pub meta: String,
    /// What a container holds.
    pub contents: Option<String>,
    pub affixes: Vec<AffixLine>,
    /// Stats the meta line leaves out that differ from an ordinary thing's.
    pub stats: Vec<String>,
    /// A light's state and what L does to it: "Burning \u{b7} L puts it out".
    pub light: Option<String>,
    /// What food or drink eases, or what a book teaches: "Eases hunger 40%", "Teaches +1 WIS".
    pub consume: Option<String>,
    /// What is written on it: a note's words.
    pub words: Option<String>,
    /// The keys and verbs at the foot, from [`crate::interact::key_actions`]: ("F", "Take").
    /// [`card_text`] leaves it empty; [`show_card`] fills it from the keymap and the item's place.
    pub actions: Vec<(String, &'static str)>,
    /// A legendary's three lines of history, then the item's own ("Made by ...").
    pub history: Vec<String>,
}

/// What a torch or lantern's card says of its flame, and what L would do; `None` for anything
/// that is not a light.
pub fn light_line(item: &armory::Item) -> Option<String> {
    use armory::families::torch::BurnState;
    let fuel = item.fuel_left().map(fuel_text);
    let (state, lit) = match item {
        armory::Item::Torch(s) => (
            match s.burn.state {
                BurnState::Fresh => "Unlit",
                BurnState::Burning => "Burning",
                BurnState::Doused => "Doused",
                BurnState::Spent => return Some("Spent".into()),
            },
            s.burn.state == BurnState::Burning,
        ),
        armory::Item::Lantern(s) if s.flame.fuel_left <= 0.0 => return Some("Empty".into()),
        armory::Item::Lantern(s) => (if s.flame.lit { "Lit" } else { "Unlit" }, s.flame.lit),
        armory::Item::Sundry(s) if item.lit().is_some() => {
            if s.article.burnt >= armory::kindle::CANDLE_BURNT_OUT {
                return Some("Spent".into());
            }
            (
                if s.article.lit { "Burning" } else { "Unlit" },
                s.article.lit,
            )
        }
        _ => return None,
    };
    Some(format!(
        "{state} \u{b7} {} \u{b7} L {}",
        fuel?,
        if lit { "puts it out" } else { "lights it" }
    ))
}

/// How much food or drink eases, "Eases hunger 40%", or what a book teaches, "Teaches +1 WIS".
/// `None` for anything that is not eaten, drunk or read, or eases nothing; the key that uses it
/// is in the card's actions.
pub fn consume_line(item: &Item) -> Option<String> {
    if let Some(teaching) = item.teaching() {
        return Some(format!(
            "{}\nWritten in {}",
            teaching.description(),
            fb_denizens::tongue::tongue_name(teaching.kind)
        ));
    }
    if let Some(stat) = item.lesson() {
        return Some(format!(
            "Teaches {:+} {}",
            fb_items::lesson::LESSON_POINTS,
            fb_items::lesson::short_name(stat)
        ));
    }
    item.consume()?;
    let p = item.provision()?;
    let eases: Vec<String> = [("hunger", p.nutrition), ("thirst", p.water)]
        .into_iter()
        .filter(|(_, by)| *by > 0.0)
        .map(|(meter, by)| format!("{meter} {}%", (by * 100.0).round()))
        .collect();
    (!eases.is_empty()).then(|| format!("Eases {}", eases.join(" \u{b7} ")))
}

/// Burning time left as the card writes it, in game hours and minutes: "Fuel 0:42".
pub fn fuel_text(seconds: f32) -> String {
    let minutes = (seconds.max(0.0) / 60.0).round() as u32;
    format!("Fuel {}:{:02}", minutes / 60, minutes % 60)
}

/// A number as the card writes it: at most two decimals, no trailing zeros.
pub fn number(value: f32) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn signed(value: f32) -> String {
    if value >= 0.0 {
        format!("+{}", number(value))
    } else {
        format!("\u{2212}{}", number(-value))
    }
}

fn capitalised(rarity: Rarity) -> &'static str {
    match rarity {
        Rarity::Common => "Common",
        Rarity::Magic => "Magic",
        Rarity::Rare => "Rare",
        Rarity::Legendary => "Legendary",
    }
}

/// The card for `item`. With the inventory and its id, a container says what it holds.
pub fn card_text(item: &Item, held: Option<(&Inventory, ItemId)>) -> CardText {
    let p = &item.profile;
    let s = &p.stats;
    let cells = format!("{}\u{d7}{} cells", p.footprint.cols, p.footprint.rows);
    let weight = format!("{} lb", number(s.weight));
    let meta: Vec<String> = if p.kind == Kind::Gear {
        let noun = fb_items::naming::noun(&item.base, &item.shape);
        let damage = s.damage.map(|d| {
            let bonus = if s.damage_bonus != 0.0 {
                signed(s.damage_bonus)
            } else {
                String::new()
            };
            format!("{d}{bonus} damage")
        });
        [
            Some(capitalised(item.rarity).to_string()),
            item.legend.is_none().then_some(noun),
            Some(weight),
            (s.armour > 0.0).then(|| format!("armour +{}", number(s.armour))),
            (s.warmth > 0.0).then(|| format!("warmth +{}", number(s.warmth))),
            damage,
            (s.light > 0.0).then(|| format!("light {} ft", number(s.light))),
            Some(cells),
        ]
        .into_iter()
        .flatten()
        .collect()
    } else {
        // A purse's description is its coins, which the price line says.
        let description =
            (!p.description.is_empty() && item.coins.is_none()).then(|| p.description.clone());
        [description, Some(weight), Some(cells)]
            .into_iter()
            .flatten()
            .collect()
    };
    let price = item
        .coins
        .map_or_else(|| Price::Worth(item.value()), Price::Holds);
    let contents = p.holds.as_ref().map(|holds| {
        let inside = format!("{}\u{d7}{} inside", holds.cols, holds.rows);
        match held {
            Some((inventory, id)) if inventory.contents_count(id) > 0 => {
                let n = inventory.contents_count(id);
                format!(
                    "Holds {n} {} \u{b7} {} lb inside \u{b7} {inside}",
                    if n == 1 { "thing" } else { "things" },
                    number(inventory.contents_weight(id))
                )
            }
            _ => format!("Empty \u{b7} {inside}"),
        }
    });
    CardText {
        title: item.title().to_string(),
        rarity: item.rarity,
        subtitle: item.legend.as_ref().map(|l| l.subtitle.clone()),
        price,
        meta: meta.join(" \u{b7} "),
        contents,
        affixes: item
            .affixes
            .iter()
            .map(|a| AffixLine {
                text: a.text.clone(),
                name: a.name.clone(),
                good: a.good,
            })
            .collect(),
        stats: stat_lines(s),
        light: light_line(&item.shape),
        consume: consume_line(item),
        words: item.words.clone(),
        actions: Vec::new(),
        history: item
            .legend
            .as_ref()
            .map_or_else(Vec::new, |l| l.history.to_vec())
            .into_iter()
            .chain(item.history.iter().cloned())
            .collect(),
    }
}

/// What wearing one thing instead of another changes: "+2 armour", and whether that is better.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub text: String,
    pub better: bool,
}

/// What wearing `new` in place of `old` (or of nothing) changes, stat by stat, leaving out
/// what stays the same. A weapon's damage is compared by its average blow.
pub fn changes(new: &Stats, old: Option<&Stats>) -> Vec<Change> {
    let plain = Stats::default();
    let old = old.unwrap_or(&plain);
    let mut lines = Vec::new();
    // `higher` says whether more is better; `unit` follows the number.
    let mut differ = |new: f32, old: f32, name: &str, unit: &str, higher: bool| {
        let by = new - old;
        if by.abs() >= 0.005 {
            lines.push(Change {
                text: format!("{}{unit} {name}", signed(by)),
                better: (by > 0.0) == higher,
            });
        }
    };
    let blow = |s: &Stats| s.damage.map_or(0.0, |d| d.mean() + s.damage_bonus);
    differ(blow(new), blow(old), "damage a blow", "", true);
    differ(new.armour, old.armour, "armour", "", true);
    differ(new.warmth, old.warmth, "warmth", "", true);
    differ(new.light, old.light, "light", " ft", true);
    differ(new.to_hit, old.to_hit, "to hit", "", true);
    differ(
        new.ranged_to_hit,
        old.ranged_to_hit,
        "to hit with missiles",
        "",
        true,
    );
    differ(new.stealth, old.stealth, "stealth", "", true);
    differ(new.lockpicking, old.lockpicking, "lockpicking", "", true);
    differ(new.carry, old.carry, "carry", " lb", true);
    differ(new.darkvision, old.darkvision, "darkvision", " ft", true);
    differ(
        new.heal_on_kill,
        old.heal_on_kill,
        "heal on a kill",
        "",
        true,
    );
    // Rates are multiples of normal, written as the change in that multiple.
    differ(new.burn_time, old.burn_time, "burn time", "\u{d7}", true);
    differ(new.hunger_rate, old.hunger_rate, "hunger", "\u{d7}", false);
    differ(
        new.sleepiness_rate,
        old.sleepiness_rate,
        "sleepiness",
        "\u{d7}",
        false,
    );
    differ(
        new.disease_risk,
        old.disease_risk,
        "disease risk",
        "\u{d7}",
        false,
    );
    differ(new.speed * 100.0, old.speed * 100.0, "speed", "%", true);
    differ(
        new.attack_rate * 100.0,
        old.attack_rate * 100.0,
        "attack speed",
        "%",
        true,
    );
    differ(
        new.stamina_refill * 100.0,
        old.stamina_refill * 100.0,
        "stamina refill",
        "%",
        true,
    );
    differ(
        new.stamina_cost * 100.0,
        old.stamina_cost * 100.0,
        "stamina cost",
        "%",
        false,
    );
    differ(
        new.block_chance * 100.0,
        old.block_chance * 100.0,
        "chance to block",
        "%",
        true,
    );
    differ(new.luck, old.luck, "luck", "", true);
    differ(
        new.break_chance * 100.0,
        old.break_chance * 100.0,
        "chance to break",
        "%",
        false,
    );
    for (new, old, name) in [
        (new.strength, old.strength, "STR"),
        (new.dexterity, old.dexterity, "DEX"),
        (new.constitution, old.constitution, "CON"),
        (new.intelligence, old.intelligence, "INT"),
        (new.wisdom, old.wisdom, "WIS"),
        (new.charisma, old.charisma, "CHA"),
    ] {
        differ(new, old, name, "", true);
    }
    for foe in fb_items::Foe::ALL {
        let (i, kind) = (foe.index(), foe.label().to_lowercase());
        differ(
            new.ward[i] * 100.0,
            old.ward[i] * 100.0,
            &format!("less damage from {kind}"),
            "%",
            true,
        );
        differ(
            new.shun[i] * 100.0,
            old.shun[i] * 100.0,
            &format!("chance {kind} pay no heed"),
            "%",
            true,
        );
        let mean = |s: &Stats| s.bane[i].map_or(0.0, |d| d.mean());
        differ(
            mean(new),
            mean(old),
            &format!("damage against {}", bane_kind(foe)),
            "",
            true,
        );
    }
    differ(new.weight, old.weight, "weight", " lb", false);
    lines
}

/// What wearing `new` in place of `old` changes: its [`changes`], and for a bag or belt the
/// room inside.
pub fn swap_changes(new: &Item, old: Option<&Item>) -> Vec<Change> {
    let mut lines = changes(&new.profile.stats, old.map(|o| &o.profile.stats));
    let room = |item: Option<&Item>| {
        item.and_then(|i| i.profile.holds.as_ref())
            .map_or(0, |g| g.cols as i32 * g.rows as i32)
    };
    let by = room(Some(new)) - room(old);
    if by != 0 {
        lines.insert(
            0,
            Change {
                text: format!("{} cells inside", signed(by as f32)),
                better: by > 0,
            },
        );
    }
    lines
}

/// The slots an item is held up against, in the order Shift steps through them: its own
/// ([`Item::slots`]), and for a torch the hip too, where a lantern may hang. A torch cannot
/// hang there, but what lights the way is worth weighing against it.
fn compare_slots(item: &Item) -> Vec<Slot> {
    let mut slots = item.slots();
    if item.shape.family() == armory::Family::Torch && !slots.contains(&Slot::Hip) {
        slots.push(Slot::Hip);
    }
    slots
}

/// What an item that can be worn is compared with: a slot it fits, and what is worn there.
/// `step` 0 is the first of its slots with something in it, so a sword is held up against the
/// main hand's weapon and a ring against a worn ring; with all its slots empty, the one it
/// would be worn in. Each step on is the next of its slots after that, wrapping round. `None`
/// for goods and for what is already worn.
pub fn compare_with(
    inventory: &Inventory,
    id: ItemId,
    step: usize,
) -> Option<(Slot, Option<ItemId>)> {
    let item = inventory.item(id)?;
    item.profile.slot?;
    if matches!(inventory.location(id), Some(Location::Worn(_))) {
        return None;
    }
    let slots = compare_slots(item);
    let first = slots
        .iter()
        .position(|&slot| inventory.worn(slot).is_some())
        .or_else(|| {
            slots
                .iter()
                .position(|&slot| Some(slot) == inventory.target_slot(id))
        })?;
    let slot = slots[(first + step) % slots.len()];
    Some((slot, inventory.worn(slot)))
}

/// The slot's name as the doll labels it.
fn slot_label(slot: Slot) -> String {
    slot_places()
        .iter()
        .find(|p| p.slot == slot)
        .map_or_else(|| format!("{slot:?}"), |p| p.label.clone())
}

/// The compare card's banner: "Worn \u{b7} Head", or "Nothing worn \u{b7} Head".
pub fn compare_banner(slot: Slot, worn: bool) -> String {
    format!(
        "{} \u{b7} {}",
        if worn { "Worn" } else { "Nothing worn" },
        slot_label(slot)
    )
}

/// "Undead deal 25% less": a ward against `foe` of `fraction`.
pub fn ward_text(foe: fb_items::Foe, fraction: f32) -> String {
    format!(
        "{} deal {}% less",
        foe.label(),
        number((fraction * 100.0).round())
    )
}

/// The kind a bane names: "the undead", "vermin", "spiders".
fn bane_kind(foe: fb_items::Foe) -> String {
    if foe == fb_items::Foe::Undead {
        "the undead".to_string()
    } else {
        foe.label().to_lowercase()
    }
}

/// "+1d6 against the undead": a weapon's extra `dice` against `foe`.
pub fn bane_text(foe: fb_items::Foe, dice: fb_items::dice::Dice) -> String {
    format!("+{dice} against {}", bane_kind(foe))
}

/// "Vermin often pay no heed (50%)": a shun of `chance` for `foe`.
pub fn shun_text(foe: fb_items::Foe, chance: f32) -> String {
    format!(
        "{} often pay no heed ({}%)",
        foe.label(),
        number((chance * 100.0).round())
    )
}

/// The stats the meta line leaves out, where they differ from an ordinary thing's.
pub fn stat_lines(s: &Stats) -> Vec<String> {
    let plain = Stats::default();
    let mut lines = Vec::new();
    let mut bonus = |value: f32, name: &str| {
        if value != 0.0 {
            lines.push(format!("{} {name}", signed(value)));
        }
    };
    bonus(s.to_hit, "to hit");
    bonus(s.ranged_to_hit, "to hit with missiles");
    bonus(s.stealth, "stealth");
    bonus(s.lockpicking, "lockpicking");
    for foe in fb_items::Foe::ALL {
        if s.ward[foe.index()] > 0.0 {
            lines.push(ward_text(foe, s.ward[foe.index()]));
        }
        if s.shun[foe.index()] > 0.0 {
            lines.push(shun_text(foe, s.shun[foe.index()]));
        }
        if let Some(dice) = s.bane[foe.index()] {
            lines.push(bane_text(foe, dice));
        }
    }
    if s.carry != 0.0 {
        lines.push(format!("carries {} lb more", number(s.carry)));
    }
    if s.darkvision > 0.0 {
        lines.push(format!("darkvision {} ft", number(s.darkvision)));
    }
    if s.heal_on_kill > 0.0 {
        lines.push(format!("heals {} on a kill", number(s.heal_on_kill)));
    }
    let mut rate = |value: f32, normal: f32, name: &str| {
        if value != normal {
            lines.push(format!("{name} \u{d7}{}", number(value)));
        }
    };
    rate(s.burn_time, plain.burn_time, "burns");
    rate(s.hunger_rate, plain.hunger_rate, "hunger");
    rate(s.sleepiness_rate, plain.sleepiness_rate, "sleepiness");
    rate(s.disease_risk, plain.disease_risk, "disease risk");
    if s.break_chance > 0.0 {
        lines.push(format!(
            "{}% chance to break on a hard blow",
            number(s.break_chance * 100.0)
        ));
    }
    if s.speed != plain.speed {
        lines.push(format!("{:+}% speed", ((s.speed - 1.0) * 100.0).round()));
    }
    if s.attack_rate != plain.attack_rate {
        lines.push(format!(
            "{:+}% attack speed",
            ((s.attack_rate - 1.0) * 100.0).round()
        ));
    }
    if s.stamina_refill != plain.stamina_refill {
        lines.push(format!(
            "{:+}% stamina refill",
            ((s.stamina_refill - 1.0) * 100.0).round()
        ));
    }
    if s.stamina_cost != plain.stamina_cost {
        lines.push(format!(
            "{:+}% stamina cost",
            ((s.stamina_cost - 1.0) * 100.0).round()
        ));
    }
    if s.block_chance > 0.0 {
        lines.push(format!(
            "{}% chance to block a blow",
            number((s.block_chance * 100.0).round())
        ));
    }
    if s.luck != 0.0 {
        lines.push(format!("{} luck", signed(s.luck)));
    }
    for (value, name) in [
        (s.strength, "STR"),
        (s.dexterity, "DEX"),
        (s.constitution, "CON"),
        (s.intelligence, "INT"),
        (s.wisdom, "WIS"),
        (s.charisma, "CHA"),
    ] {
        if value != 0.0 {
            lines.push(format!("{} {name}", signed(value)));
        }
    }
    lines
}

/// Gap between a card and what it describes, and the least room it leaves at the screen's
/// edges, in logical pixels (the plate's 10 and 8).
pub const CARD_GAP: f32 = 10.0;
pub const CARD_MARGIN: f32 = 8.0;

/// Where a card of `size` goes beside `anchor` on a `screen`: to its right, else its left,
/// else below it (or above, if below is off the screen), always kept on the screen. The
/// plate's `showTip`.
pub fn place_beside(anchor: Rect, size: Vec2, screen: Vec2) -> Vec2 {
    let mut x = anchor.max.x + CARD_GAP;
    let mut y = anchor.min.y;
    if x + size.x > screen.x - CARD_MARGIN {
        x = anchor.min.x - CARD_GAP - size.x;
    }
    if x < CARD_MARGIN {
        x = anchor.min.x;
        y = anchor.max.y + CARD_GAP;
        if y + size.y > screen.y - CARD_MARGIN {
            y = anchor.min.y - CARD_GAP - size.y;
        }
    }
    keep_on_screen(Vec2::new(x, y), size, screen)
}

/// Where a card of `size` goes centred under `point` (the crosshair), kept on the screen.
pub fn place_under(point: Vec2, size: Vec2, screen: Vec2) -> Vec2 {
    keep_on_screen(
        Vec2::new(point.x - size.x * 0.5, point.y + 2.0 * CARD_GAP),
        size,
        screen,
    )
}

/// Where the compare card of `size` goes beside the `card` it compares with: on the side away
/// from the `item` the card describes (the right, under the crosshair), else the other side,
/// kept on the screen.
pub fn place_compare(card: Rect, item: Option<Rect>, size: Vec2, screen: Vec2) -> Vec2 {
    let right = card.max.x + CARD_GAP;
    let left = card.min.x - CARD_GAP - size.x;
    let fits_right = right + size.x <= screen.x - CARD_MARGIN;
    let fits_left = left >= CARD_MARGIN;
    let card_on_left = item.is_some_and(|item| item.center().x > card.center().x);
    let x = match (card_on_left, fits_left, fits_right) {
        (true, true, _) | (false, true, false) => left,
        _ => right,
    };
    keep_on_screen(Vec2::new(x, card.min.y), size, screen)
}

/// `at` moved so a card of `size` there lies inside `screen`, [`CARD_MARGIN`] from its edges
/// where it fits, at the top left where it does not.
pub fn keep_on_screen(at: Vec2, size: Vec2, screen: Vec2) -> Vec2 {
    let max = (screen - size - CARD_MARGIN).max(Vec2::splat(CARD_MARGIN));
    at.clamp(Vec2::splat(CARD_MARGIN), max)
}

/// Width of the card in logical pixels (the plate's `.tip`).
pub const CARD_WIDTH: f32 = 300.0;

/// The hover card: one on the screen, shown for the item under the mouse on the inventory
/// screen, or the item under the crosshair in the level.
#[derive(Component, Debug)]
pub struct ItemCard;

/// The compare card, shown beside the hover card while [`Action::Compare`] is held over
/// something that can be worn: what is worn in its place, and what swapping would change.
#[derive(Component, Debug)]
pub struct CompareCard;

/// The hover card as it stands on the screen: its item, where it is, and the item's own
/// rectangle when it sits beside one. Written by [`show_card`].
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct ShownCard(pub Option<(ItemId, Rect, Option<Rect>)>);

/// Where a card goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CardAnchor {
    /// Beside an item on the screen ([`place_beside`]).
    Beside(Rect),
    /// Under the crosshair ([`place_under`]).
    Crosshair,
}

/// Spawns the card, hidden, once there is a camera drawing over the held torch to draw it.
pub fn spawn_card(
    mut commands: Commands,
    cards: Query<(), With<ItemCard>>,
    cameras: Query<Entity, With<crate::carry::ViewModelCamera>>,
) {
    if !cards.is_empty() {
        return;
    }
    let Some(camera) = cameras.iter().next() else {
        return;
    };
    commands.spawn((card_frame(camera), Name::new("Item card"), ItemCard));
    commands.spawn((card_frame(camera), Name::new("Compare card"), CompareCard));
}

/// A card's frame, hidden, drawn by `camera`.
fn card_frame(camera: Entity) -> impl Bundle {
    (
        DespawnOnExit(crate::launch::AppState::Delving),
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            width: px(CARD_WIDTH),
            flex_direction: FlexDirection::Column,
            border: UiRect::all(px(1.5)),
            border_radius: BorderRadius::all(px(12)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(BG),
        BorderColor::all(LINE),
        BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, 0.55),
            px(0),
            px(14),
            px(0),
            px(34),
        ),
        GlobalZIndex(40),
        Visibility::Hidden,
        Pickable::IGNORE,
    )
}

/// What the compare card adds to a card: a banner over it naming the slot, and what a swap
/// changes under it.
struct Comparison<'a> {
    banner: &'a str,
    /// Names the hovered item, so the changes read as what it gives over what is worn.
    heading: &'a str,
    changes: &'a [Change],
}

/// Fills `card` with what [`card_text`] says, under the item's picture. A compare card puts
/// its banner on top and the swap's changes at the foot, and with nothing worn has only those.
fn fill_card(
    commands: &mut Commands,
    card: Entity,
    shown: Option<(&Item, &CardText)>,
    comparison: Option<Comparison>,
    fonts: &UiFonts,
    icons: &mut Icons,
) {
    commands
        .entity(card)
        .despawn_children()
        .with_children(|card| {
            if let Some(comparison) = &comparison {
                card.spawn((
                    Node {
                        padding: UiRect::new(px(14), px(14), px(8), px(8)),
                        ..default()
                    },
                    BackgroundColor(BG.mix(&GOLD, 0.28)),
                ))
                .with_child((
                    Text::new(comparison.banner.to_string()),
                    UiFonts::style(&fonts.display, 15.0, INK),
                ));
            }
            if let Some((item, text)) = shown {
                fill_item(card, item, text, fonts, icons);
            }
            if let Some(comparison) = &comparison {
                card.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(3),
                        padding: UiRect::new(px(14), px(14), px(10), px(12)),
                        border: UiRect::top(px(1)),
                        ..default()
                    },
                    BorderColor::all(FAINT),
                ))
                .with_children(|list| {
                    list.spawn((
                        Text::new(comparison.heading.to_string()),
                        UiFonts::style(&fonts.body_italic, 13.5, GOLD),
                    ));
                    if comparison.changes.is_empty() {
                        list.spawn((
                            Text::new("No change"),
                            UiFonts::style(&fonts.mono, 13.0, MUTED),
                        ));
                    }
                    for change in comparison.changes {
                        let ink = if change.better { OK } else { BAD };
                        list.spawn((
                            Text::new(change.text.clone()),
                            UiFonts::style(&fonts.mono_bold, 13.0, ink),
                        ));
                    }
                });
            }
        });
}

/// "Costs 2 gp 3 sp", then "worth 1 gp 5 sp" under it: the word in the body face, the coins
/// bold in their metals (red, all of them, for a ware the delver cannot pay for), the note
/// muted on a line of its own so a narrow card never breaks it.
fn price_line(info: &mut ChildSpawnerCommands, fonts: &UiFonts, price: Price) {
    let (word, coins, note) = price.parts();
    let size = 16.0;
    info.spawn(Node {
        flex_direction: FlexDirection::Column,
        ..default()
    })
    .with_children(|column| {
        column
            .spawn((
                Text::new(format!("{word}\u{a0}\u{a0}")),
                UiFonts::style(&fonts.body_medium, size, INK),
            ))
            .with_children(|line| {
                if matches!(price, Price::Costs { afford: false, .. }) {
                    line.spawn((
                        TextSpan::new(coins.short()),
                        UiFonts::style(&fonts.mono_bold, size - 1.0, BAD),
                    ));
                } else {
                    coin_spans(line, fonts, coins, size - 1.0);
                }
            });
        if let Some(note) = note {
            column.spawn((Text::new(note), UiFonts::style(&fonts.mono, 12.0, MUTED)));
        }
    });
}

/// The item's picture and what [`card_text`] says of it.
fn fill_item(
    card: &mut ChildSpawnerCommands,
    item: &Item,
    text: &CardText,
    fonts: &UiFonts,
    icons: &mut Icons,
) {
    let footprint = item.profile.footprint;
    // The plate's `viewH`: the picture's height follows the item's shape, within limits.
    let picture = (CARD_WIDTH * 0.62 * footprint.rows as f32 / footprint.cols.max(1) as f32)
        .clamp(110.0, 230.0);
    let size = super::icon::icon_size(item, false).as_vec2();
    let scale = ((CARD_WIDTH - 40.0) / size.x).min((picture - 20.0) / size.y);
    let colour = rarity_color(text.rarity);
    card.spawn((
        Node {
            height: px(picture),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(BG.mix(&GOLD, 0.12)),
    ))
    .with_children(|frame| {
        icons.spawn(
            frame,
            item,
            false,
            Node {
                width: px(size.x * scale),
                height: px(size.y * scale),
                ..default()
            },
            48.0,
        );
    });
    card.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: px(4),
        padding: UiRect::new(px(14), px(14), px(10), px(12)),
        ..default()
    })
    .with_children(|info| {
        info.spawn((
            Text::new(text.title.clone()),
            UiFonts::style(&fonts.display, 22.0, colour),
        ));
        if let Some(subtitle) = &text.subtitle {
            info.spawn((
                Text::new(subtitle.clone()),
                UiFonts::style(&fonts.body_italic, 15.0, colour),
            ));
        }
        price_line(info, fonts, text.price);
        numbers_line(info, fonts, &text.meta, 12.0);
        if let Some(contents) = &text.contents {
            numbers_line(info, fonts, contents, 12.0);
        }
        for affix in &text.affixes {
            let ink = if affix.good { INK } else { BAD };
            info.spawn((
                Text::new(format!("\u{2022} {} ", affix.text)),
                UiFonts::style(&fonts.body, 14.0, ink),
            ))
            .with_child((
                TextSpan::new(format!("({})", affix.name)),
                UiFonts::style(&fonts.mono, 12.0, MUTED),
            ));
        }
        // The totals, with every affix counted, on one line under them.
        if !text.stats.is_empty() {
            numbers_line(info, fonts, &text.stats.join(" \u{b7} "), 12.0);
        }
        if let Some(light) = &text.light {
            info.spawn((
                Text::new(light.clone()),
                UiFonts::style(&fonts.body_medium, 14.0, INK),
            ));
        }
        if let Some(consume) = &text.consume {
            info.spawn((
                Text::new(consume.clone()),
                UiFonts::style(&fonts.body_medium, 14.0, INK),
            ));
        }
        if let Some(words) = &text.words {
            info.spawn((
                Node {
                    margin: UiRect::vertical(px(4)),
                    padding: UiRect::axes(px(10), px(8)),
                    ..default()
                },
                BackgroundColor(BG.mix(&GOLD, 0.2)),
            ))
            .with_child((
                Text::new(words.clone()),
                UiFonts::style(&fonts.body_italic, 14.5, INK),
            ));
        }
        if !text.actions.is_empty() {
            info.spawn(Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                margin: UiRect::top(px(2)),
                ..default()
            })
            .with_children(|row| {
                for (n, (key, name)) in text.actions.iter().enumerate() {
                    let sep = if n == 0 { "" } else { "\u{b7} " };
                    row.spawn((
                        Text::new(format!("{sep}{key} ")),
                        UiFonts::style(&fonts.mono, 12.0, GOLD),
                    ))
                    .with_child((
                        TextSpan::new(*name),
                        UiFonts::style(&fonts.body_medium, 14.0, INK),
                    ));
                }
            });
        }
        if !text.history.is_empty() {
            info.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(3),
                    margin: UiRect::top(px(4)),
                    padding: UiRect::left(px(10)),
                    border: UiRect::left(px(2)),
                    ..default()
                },
                BorderColor::all(FAINT),
            ))
            .with_children(|history| {
                for line in &text.history {
                    history.spawn((
                        Text::new(line.clone()),
                        UiFonts::style(&fonts.body_italic, 13.5, MUTED),
                    ));
                }
            });
        }
    });
}

/// What the card shows now: the item under the mouse on the screen, unless one is being
/// dragged; in the level, the item under the crosshair.
pub fn card_subject(
    open: bool,
    hovered: Hovered,
    dragging: bool,
    aimed: Option<ItemId>,
) -> Option<(ItemId, CardAnchor)> {
    if open {
        hovered
            .0
            .filter(|_| !dragging)
            .map(|(id, rect)| (id, CardAnchor::Beside(rect)))
    } else {
        aimed.map(|id| (id, CardAnchor::Crosshair))
    }
}

/// `subject` once the mouse has rested on it `delay` seconds (the tuning's `hover_delay`), at
/// `now` seconds: an item hovered on the screen waits that long, counted from when the mouse
/// came onto it (kept in `since`); the item under the crosshair shows at once.
pub fn after_hover_delay(
    subject: Option<(ItemId, CardAnchor)>,
    now: f32,
    delay: f32,
    since: &mut Option<(ItemId, f32)>,
) -> Option<(ItemId, CardAnchor)> {
    let Some((id, CardAnchor::Beside(_))) = subject else {
        *since = None;
        return subject;
    };
    let from = match *since {
        Some((hovered, from)) if hovered == id => from,
        _ => {
            *since = Some((id, now));
            now
        }
    };
    (now - from >= delay).then_some(subject).flatten()
}

/// Shows, fills and places the card. A card filled this frame waits a frame to be measured.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn show_card(
    mut commands: Commands,
    open: Res<ScreenOpen>,
    hovered: Res<Hovered>,
    dragging: Res<Dragging>,
    aimed: Option<Res<crate::carry::Aimed>>,
    floor: Query<&crate::carry::FloorItem>,
    containers: Query<&crate::props::Container>,
    inventory: Res<Inventory>,
    keymap: Option<Res<crate::keymap::Keymap>>,
    fonts: Res<UiFonts>,
    mut icons: Icons,
    mut cards: Query<
        (
            Entity,
            &mut Node,
            &mut Visibility,
            &ComputedNode,
            &ComputedUiRenderTargetInfo,
        ),
        With<ItemCard>,
    >,
    (tuning, time, barter): (
        Option<Res<crate::tuning::Tuning>>,
        Option<Res<Time<Real>>>,
        Option<Res<crate::barter::Barter>>,
    ),
    mut shown: Local<Option<ItemId>>,
    mut hovered_since: Local<Option<(ItemId, f32)>>,
    mut on_screen: ResMut<ShownCard>,
) {
    on_screen.set_if_neq(ShownCard(None));
    let aimed_entity = aimed.and_then(|a| a.0);
    let aimed_item = aimed_entity.and_then(|entity| {
        floor
            .get(entity)
            .map(|f| f.id)
            .or_else(|_| containers.get(entity).map(|c| c.id))
            .ok()
    });
    // A container under the crosshair has its own prompt (`crate::props`), so its card lists no
    // keys. Only the crosshair's card: the crosshair still holds the container after the screen
    // opens on it, and the cards of the items in the screen are theirs.
    let aimed_container = aimed_entity.is_some_and(|entity| containers.contains(entity));
    // No card over the menu.
    let subject = if open.is(Screen::Menu) {
        None
    } else {
        card_subject(
            open.is(Screen::Inventory),
            *hovered,
            dragging.0.is_some_and(|d| d.active),
            aimed_item,
        )
    };
    let delay = tuning.map_or(0.0, |t| t.interface.hover_delay);
    let subject = after_hover_delay(
        subject,
        time.map_or(0.0, |t| t.elapsed_secs()),
        delay,
        &mut hovered_since,
    );
    for (card, mut node, mut visibility, computed, target) in &mut cards {
        let Some((id, anchor)) = subject.filter(|(id, _)| inventory.item(*id).is_some()) else {
            *visibility = Visibility::Hidden;
            *shown = None;
            continue;
        };
        if *shown != Some(id)
            || inventory.is_changed()
            || barter.as_ref().is_some_and(|b| b.is_changed())
            || keymap.as_ref().is_some_and(|k| k.is_changed())
        {
            let item = inventory.item(id).expect("checked above");
            let fallback = crate::keymap::Keymap::default();
            let keymap = keymap.as_deref().unwrap_or(&fallback);
            let mut text = card_text(item, Some((&inventory, id)));
            if let Some(price) = barter.as_deref().and_then(|b| b.tag(&inventory, id)) {
                text.price = price;
            }
            if !(aimed_container && matches!(anchor, CardAnchor::Crosshair))
                && let Some(target) = crate::interact::item_target(&inventory, id)
            {
                let verbs =
                    crate::interact::verbs_for(&target, &crate::interact::Delver::default());
                text.actions = crate::interact::key_actions(keymap, &verbs);
            }
            if inventory.is_carried(id)
                && let Some(key) = keymap.inputs(Action::Drop).first()
            {
                text.actions.push((key.label(), "Drop"));
            }
            fill_card(
                &mut commands,
                card,
                Some((item, &text)),
                None,
                &fonts,
                &mut icons,
            );
            *shown = Some(id);
            *visibility = Visibility::Hidden;
            continue;
        }
        let size = computed.size * computed.inverse_scale_factor;
        let screen = target.logical_size();
        let (at, beside) = match anchor {
            CardAnchor::Beside(rect) => (place_beside(rect, size, screen), Some(rect)),
            CardAnchor::Crosshair => (place_under(screen * 0.5, size, screen), None),
        };
        node.left = px(at.x);
        node.top = px(at.y);
        *visibility = Visibility::Inherited;
        on_screen.set_if_neq(ShownCard(Some((
            id,
            Rect::from_corners(at, at + size),
            beside,
        ))));
    }
}

/// How many times Shift has stepped the comparison of the `hovered` item on, kept in
/// `stepped` with the item it is for. Shift counts only while Alt is held (`comparing`), and
/// another item, or letting go of Alt, starts again.
fn compare_step(
    stepped: &mut (Option<ItemId>, usize),
    hovered: Option<ItemId>,
    comparing: bool,
    shift: bool,
) -> usize {
    if !comparing || stepped.0 != hovered {
        *stepped = (hovered, 0);
    } else if shift {
        stepped.1 += 1;
    }
    stepped.1
}

/// Shows, fills and places the compare card beside the hover card while [`Action::Compare`]
/// is held over something that can be worn ([`compare_with`]), Shift stepping on to the next
/// slot it fits. Like the hover card,
/// one filled this frame waits a frame to be measured.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn show_compare(
    mut commands: Commands,
    input: ActionInput,
    on_screen: Res<ShownCard>,
    inventory: Res<Inventory>,
    fonts: Res<UiFonts>,
    mut icons: Icons,
    mut cards: Query<
        (
            Entity,
            &mut Node,
            &mut Visibility,
            &ComputedNode,
            &ComputedUiRenderTargetInfo,
        ),
        With<CompareCard>,
    >,
    mut shown: Local<Option<(ItemId, Slot, Option<ItemId>)>>,
    // The item the step is for, and how many times Shift has stepped on from its first slot.
    mut stepped: Local<(Option<ItemId>, usize)>,
) {
    let comparing = input.pressed(Action::Compare) || compare_forced();
    let step = compare_step(
        &mut stepped,
        on_screen.0.map(|(id, ..)| id),
        comparing,
        input.just_pressed(Action::Sprint),
    );
    let subject = on_screen
        .0
        .filter(|_| comparing)
        .and_then(|(id, card, beside)| {
            Some((id, compare_with(&inventory, id, step)?, card, beside))
        });
    for (entity, mut node, mut visibility, computed, target) in &mut cards {
        let Some((id, (slot, worn), card, beside)) = subject else {
            *visibility = Visibility::Hidden;
            *shown = None;
            continue;
        };
        if *shown != Some((id, slot, worn)) || inventory.is_changed() {
            let new = inventory.item(id).expect("compare_with found it");
            let old = worn.and_then(|w| inventory.item(w));
            let text = old.map(|old| (old, card_text(old, worn.map(|w| (&*inventory, w)))));
            let banner = compare_banner(slot, old.is_some());
            let changes = swap_changes(new, old);
            let heading = format!("Swapping in the {}", new.title());
            let comparison = Comparison {
                banner: &banner,
                heading: &heading,
                changes: &changes,
            };
            fill_card(
                &mut commands,
                entity,
                text.as_ref().map(|(item, text)| (*item, text)),
                Some(comparison),
                &fonts,
                &mut icons,
            );
            *shown = Some((id, slot, worn));
            *visibility = Visibility::Hidden;
            continue;
        }
        let size = computed.size * computed.inverse_scale_factor;
        let at = place_compare(card, beside, size, target.logical_size());
        node.left = px(at.x);
        node.top = px(at.y);
        *visibility = Visibility::Inherited;
    }
}

/// `FB_COMPARE` holds Alt down for the capture tools, so a still shows the compare card.
fn compare_forced() -> bool {
    std::env::var("FB_COMPARE").is_ok_and(|v| !v.is_empty() && v != "0")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hovered_card_waits_for_the_hover_delay() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let over = |id| Some((ItemId(id), CardAnchor::Beside(rect)));
        let mut since = None;
        assert_eq!(
            after_hover_delay(over(3), 1.0, 0.0, &mut since),
            over(3),
            "no delay: at once"
        );
        let mut since = None;
        assert_eq!(after_hover_delay(over(3), 1.0, 0.5, &mut since), None);
        assert_eq!(after_hover_delay(over(3), 1.4, 0.5, &mut since), None);
        assert_eq!(after_hover_delay(over(3), 1.5, 0.5, &mut since), over(3));
        // Onto another item, the wait starts again.
        assert_eq!(after_hover_delay(over(4), 1.6, 0.5, &mut since), None);
        assert_eq!(after_hover_delay(over(4), 2.2, 0.5, &mut since), over(4));
        // The crosshair's card does not wait.
        let aimed = Some((ItemId(9), CardAnchor::Crosshair));
        assert_eq!(after_hover_delay(aimed, 2.3, 0.5, &mut since), aimed);
    }

    #[test]
    fn every_field_comes_from_the_item() {
        let magic = (1..3_000)
            .filter_map(|s| fb_items::roll_item("Sword", s))
            .find(|i| i.rarity == Rarity::Magic)
            .unwrap();
        let card = card_text(&magic, None);
        assert_eq!(
            (card.title.as_str(), card.rarity, card.subtitle.as_ref()),
            (magic.name.as_str(), Rarity::Magic, None)
        );
        let s = &magic.profile.stats;
        assert!(card.meta.starts_with("Magic \u{b7} "), "{}", card.meta);
        assert!(
            card.meta.contains(&format!("{} lb", number(s.weight))),
            "{}",
            card.meta
        );
        assert!(
            card.meta.contains(&format!("{}", s.damage.unwrap())),
            "{}",
            card.meta
        );
        assert_eq!(card.price, Price::Worth(magic.value()));
        assert!(
            !card.meta.contains(&magic.value().short()),
            "the price has a line of its own: {}",
            card.meta
        );
        let fp = magic.profile.footprint;
        assert!(
            card.meta
                .ends_with(&format!("{}\u{d7}{} cells", fp.cols, fp.rows)),
            "{}",
            card.meta
        );
        assert_eq!(card.affixes.len(), magic.affixes.len());
        for (line, affix) in card.affixes.iter().zip(&magic.affixes) {
            assert_eq!(
                (line.text.as_str(), line.name.as_str(), line.good),
                (affix.text.as_str(), affix.name.as_str(), affix.good)
            );
        }
        assert!(card.history.is_empty());
        assert_eq!(card.words, None);
    }

    #[test]
    fn a_notes_card_shows_its_words() {
        let lore = fb_level::generate(4, 2).lore();
        let note = fb_items::roll_item("Note", 12).unwrap().in_world(&lore);
        assert_eq!(card_text(&note, None).words, note.words);
        assert!(card_text(&note, None).words.is_some());
        assert_eq!(
            crate::interact::use_of(&note),
            Some(crate::interact::Verb::Read)
        );
        assert_eq!(
            crate::interact::use_of(&fb_items::roll_item("Note", 12).unwrap()),
            None,
            "a blank note has nothing to read"
        );
    }

    #[test]
    fn an_items_own_history_follows_a_legends() {
        let mut legend = (1..5_000)
            .filter_map(|s| fb_items::roll_item_with("Sword", s, fb_items::Odds::loot(8)))
            .find(|i| i.rarity == Rarity::Legendary)
            .unwrap();
        legend.history.push("Made by Brenna Holt".into());
        let card = card_text(&legend, None);
        assert_eq!(card.history.len(), 4);
        assert_eq!(card.history[..3], legend.legend.as_ref().unwrap().history);
        assert_eq!(card.history[3], "Made by Brenna Holt");
    }

    #[test]
    fn a_legendary_shows_its_title_old_name_and_history() {
        let legend = (1..5_000)
            .filter_map(|s| fb_items::roll_item_with("Sword", s, fb_items::Odds::loot(8)))
            .find(|i| i.rarity == Rarity::Legendary)
            .unwrap();
        let l = legend.legend.as_ref().unwrap();
        let card = card_text(&legend, None);
        assert_eq!(card.title, l.title);
        assert_eq!(card.subtitle.as_deref(), Some(l.subtitle.as_str()));
        assert_eq!(card.history, l.history.to_vec());
        assert_eq!(card.history.len(), 3);
        assert!(
            !card
                .meta
                .contains(&fb_items::naming::noun(&legend.base, &legend.shape)),
            "the old name already says what it is"
        );
    }

    #[test]
    fn a_purse_says_what_it_holds_and_how_to_open_it() {
        let mut parts = fb_items::ItemParts::from(fb_items::roll_item("Purse", 4).unwrap());
        parts.coins = Some(Coins {
            gp: 2,
            sp: 3,
            cp: 0,
        });
        let purse = fb_items::Item::from_parts(parts).unwrap();
        let card = card_text(&purse, None);
        assert_eq!(
            card.price.line(),
            "Holds 2 gp 3 sp \u{b7} double-click to open"
        );
        assert!(
            !card.meta.contains("gp"),
            "the coins are said once: {}",
            card.meta
        );
    }

    #[test]
    fn goods_and_containers_say_what_they_are() {
        let bread = fb_items::roll_item("Bread", 3).unwrap();
        let card = card_text(&bread, None);
        assert!(
            card.meta.starts_with(&bread.profile.description),
            "{}",
            card.meta
        );
        assert!(card.affixes.is_empty() && card.contents.is_none());
        assert_eq!(
            card.price.line(),
            format!("Worth {}", bread.value().short())
        );

        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        assert_eq!(
            card_text(inventory.item(pack).unwrap(), Some((&inventory, pack)))
                .contents
                .as_deref(),
            Some("Empty \u{b7} 8\u{d7}5 inside")
        );
        let bread = inventory.add(bread);
        inventory
            .put(bread, pack, super::super::grid::Placement::new(0, 0, false))
            .unwrap();
        let contents = card_text(inventory.item(pack).unwrap(), Some((&inventory, pack)))
            .contents
            .unwrap();
        assert!(contents.starts_with("Holds 1 thing \u{b7} "), "{contents}");
    }

    #[test]
    fn stats_list_only_what_differs() {
        assert!(stat_lines(&Stats::default()).is_empty());
        let mut banes = Stats::default();
        banes.bane[fb_items::Foe::Undead.index()] = Some(fb_items::dice::Dice::new(1, 6));
        banes.bane[fb_items::Foe::Beast.index()] = Some(fb_items::dice::Dice::new(1, 4));
        assert_eq!(
            stat_lines(&banes),
            vec!["+1d6 against the undead", "+1d4 against beasts"]
        );
        let lines = changes(&banes, Some(&Stats::default()));
        assert!(
            lines
                .iter()
                .any(|c| c.text.contains("damage against the undead") && c.better)
        );
        let s = Stats {
            stealth: -2.0,
            hunger_rate: 0.85,
            darkvision: 10.0,
            ..Stats::default()
        };
        assert_eq!(
            stat_lines(&s),
            vec!["\u{2212}2 stealth", "darkvision 10 ft", "hunger \u{d7}0.85"]
        );
        assert_eq!(number(3.0), "3");
        assert_eq!(number(0.05), "0.05");
    }

    #[test]
    fn the_card_shows_the_hovered_item_on_the_screen_and_the_aimed_one_in_the_level() {
        let rect = Rect::new(0.0, 0.0, 46.0, 92.0);
        let hovered = Hovered(Some((ItemId(3), rect)));
        assert_eq!(
            card_subject(true, hovered, false, Some(ItemId(9))),
            Some((ItemId(3), CardAnchor::Beside(rect)))
        );
        assert_eq!(
            card_subject(true, hovered, true, None),
            None,
            "not while dragging"
        );
        assert_eq!(
            card_subject(false, hovered, false, Some(ItemId(9))),
            Some((ItemId(9), CardAnchor::Crosshair))
        );
        assert_eq!(card_subject(false, Hovered(None), false, None), None);
    }

    #[test]
    fn a_swap_lists_what_it_changes_and_which_way_is_better() {
        let worn = Stats {
            armour: 2.0,
            weight: 4.0,
            stealth: -1.0,
            ..Stats::default()
        };
        let new = Stats {
            armour: 3.5,
            weight: 6.0,
            stealth: -1.0,
            hunger_rate: 0.9,
            ..Stats::default()
        };
        let text = |c: &[Change]| {
            c.iter()
                .map(|c| (c.text.clone(), c.better))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            text(&changes(&new, Some(&worn))),
            vec![
                ("+1.5 armour".into(), true),
                ("\u{2212}0.1\u{d7} hunger".into(), true),
                ("+2 lb weight".into(), false)
            ],
            "stealth stays the same, so it is left out"
        );
        assert!(changes(&worn, Some(&worn)).is_empty());
        // Against nothing worn, everything it brings is a change.
        assert_eq!(
            text(&changes(&worn, None)),
            vec![
                ("+2 armour".into(), true),
                ("\u{2212}1 stealth".into(), false),
                ("+4 lb weight".into(), false)
            ]
        );
        // Weapons by their average blow: 1d8+1 is 5.5, 1d6 is 3.5.
        let sword = Stats {
            damage: Some(fb_items::dice::Dice::new(1, 8)),
            damage_bonus: 1.0,
            ..Stats::default()
        };
        let knife = Stats {
            damage: Some(fb_items::dice::Dice::new(1, 6)),
            ..Stats::default()
        };
        assert_eq!(
            text(&changes(&sword, Some(&knife))),
            vec![("+2 damage a blow".into(), true)]
        );
        // Speed and block as percentages, and the ability scores by name.
        let boots = Stats {
            speed: 1.1,
            sleepiness_rate: 1.2,
            dexterity: 1.0,
            ..Stats::default()
        };
        let shield = Stats {
            block_chance: 0.15,
            strength: -1.0,
            ..Stats::default()
        };
        assert_eq!(
            text(&changes(&boots, None)),
            vec![
                ("+0.2\u{d7} sleepiness".into(), false),
                ("+10% speed".into(), true),
                ("+1 DEX".into(), true)
            ]
        );
        assert_eq!(
            text(&changes(&shield, None)),
            vec![
                ("+15% chance to block".into(), true),
                ("\u{2212}1 STR".into(), false)
            ]
        );
        // A bag also by the room inside: a pack holds 8x5, a sack less.
        let pack = fb_items::roll_item("Pack", 7).unwrap();
        let sack = fb_items::roll_item("Sack", 7).unwrap();
        let room = |i: &Item| {
            i.profile
                .holds
                .as_ref()
                .map_or(0, |g| g.cols as i32 * g.rows as i32)
        };
        let by = room(&pack) - room(&sack);
        assert!(by > 0);
        assert_eq!(
            swap_changes(&pack, Some(&sack))[0],
            Change {
                text: format!("+{by} cells inside"),
                better: true
            }
        );
        assert_eq!(
            swap_changes(&sack, Some(&pack))[0],
            Change {
                text: format!("\u{2212}{by} cells inside"),
                better: false
            }
        );
    }

    #[test]
    fn changes_are_what_the_new_thing_gives_over_the_old() {
        let worse = Stats {
            armour: 1.0,
            warmth: 1.0,
            light: 20.0,
            to_hit: 1.0,
            carry: 10.0,
            darkvision: 30.0,
            weight: 3.0,
            burn_time: 1.0,
            break_chance: 0.1,
            ..Stats::default()
        };
        let better = Stats {
            armour: 2.0,
            warmth: 2.0,
            light: 30.0,
            to_hit: 2.0,
            carry: 15.0,
            darkvision: 60.0,
            weight: 2.0,
            burn_time: 2.0,
            break_chance: 0.05,
            ..Stats::default()
        };
        let text = |c: &[Change]| {
            c.iter()
                .map(|c| (c.text.clone(), c.better))
                .collect::<Vec<_>>()
        };
        // The better thing over the worse: gains read plus and green, less weight and breakage minus and green.
        assert_eq!(
            text(&changes(&better, Some(&worse))),
            vec![
                ("+1 armour".into(), true),
                ("+1 warmth".into(), true),
                ("+10 ft light".into(), true),
                ("+1 to hit".into(), true),
                ("+5 lb carry".into(), true),
                ("+30 ft darkvision".into(), true),
                ("+1\u{d7} burn time".into(), true),
                ("\u{2212}5% chance to break".into(), true),
                ("\u{2212}1 lb weight".into(), true),
            ]
        );
        // And the other way round, every line flips both sign and colour.
        assert_eq!(
            text(&changes(&worse, Some(&better))),
            vec![
                ("\u{2212}1 armour".into(), false),
                ("\u{2212}1 warmth".into(), false),
                ("\u{2212}10 ft light".into(), false),
                ("\u{2212}1 to hit".into(), false),
                ("\u{2212}5 lb carry".into(), false),
                ("\u{2212}30 ft darkvision".into(), false),
                ("\u{2212}1\u{d7} burn time".into(), false),
                ("+5% chance to break".into(), false),
                ("+1 lb weight".into(), false),
            ]
        );
        // Items: the hovered +2 helmet over the worn +1 one reads "+1 armour", green.
        let mut worn = fb_items::roll_item("Helm", 3).unwrap();
        let mut hovered = worn.clone();
        worn.profile.stats.armour = 1.0;
        hovered.profile.stats.armour = 2.0;
        assert_eq!(
            swap_changes(&hovered, Some(&worn)),
            vec![Change {
                text: "+1 armour".into(),
                better: true
            }]
        );
        assert_eq!(
            swap_changes(&worn, Some(&hovered)),
            vec![Change {
                text: "\u{2212}1 armour".into(),
                better: false
            }]
        );
    }

    #[test]
    fn wearable_things_compare_with_what_is_worn_in_their_place() {
        let mut inventory = Inventory::default();
        let roll = |base| fb_items::roll_item(base, 11).unwrap();
        let helm = inventory.add(roll("Helm"));
        let other_helm = inventory.add(roll("Helm"));
        let bread = inventory.add(roll("Bread"));
        // Nothing on the head yet: compared with the empty slot.
        assert_eq!(compare_with(&inventory, helm, 0), Some((Slot::Head, None)));
        inventory.wear(helm, Slot::Head).unwrap();
        assert_eq!(
            compare_with(&inventory, other_helm, 0),
            Some((Slot::Head, Some(helm)))
        );
        assert_eq!(
            compare_with(&inventory, helm, 0),
            None,
            "what is worn has nothing to compare with"
        );
        assert_eq!(
            compare_with(&inventory, bread, 0),
            None,
            "goods are not worn"
        );
        // A second ring is held up against the worn one, not the empty ring slot.
        let ring = inventory.add(roll("Ring"));
        let other_ring = inventory.add(roll("Ring"));
        inventory.wear(ring, Slot::Ring2).unwrap();
        assert_eq!(
            compare_with(&inventory, other_ring, 0),
            Some((Slot::Ring2, Some(ring)))
        );
        // A weapon against the main hand's.
        let sword = inventory.add(roll("Sword"));
        let mace = inventory.add(roll("Mace"));
        inventory.wear(mace, Slot::Mainhand).unwrap();
        assert_eq!(
            compare_with(&inventory, sword, 0),
            Some((Slot::Mainhand, Some(mace)))
        );
        assert_eq!(
            compare_banner(Slot::Head, true),
            format!("Worn \u{b7} {}", slot_label(Slot::Head))
        );
        assert!(compare_banner(Slot::Head, false).starts_with("Nothing worn \u{b7} "));
    }

    #[test]
    fn shift_steps_the_comparison_on_through_the_slots_an_item_fits() {
        let mut inventory = Inventory::default();
        let roll = |base| fb_items::roll_item(base, 11).unwrap();
        // A torch in the pack, a torch in the off hand and a lantern at the hip.
        let belt = inventory.add(roll("Belt"));
        inventory.wear(belt, Slot::Belt).unwrap();
        let lantern = inventory.add(roll("Lantern"));
        inventory.wear(lantern, Slot::Hip).unwrap();
        let held = inventory.add(roll("Torch"));
        inventory.wear(held, Slot::Offhand).unwrap();
        let spare = inventory.add(roll("Torch"));
        assert_eq!(
            compare_with(&inventory, spare, 0),
            Some((Slot::Offhand, Some(held)))
        );
        // A torch fits either hand, the off hand first, and then the hip.
        assert_eq!(
            compare_with(&inventory, spare, 1),
            Some((Slot::Mainhand, None))
        );
        assert_eq!(
            compare_with(&inventory, spare, 2),
            Some((Slot::Hip, Some(lantern)))
        );
        assert_eq!(
            compare_with(&inventory, spare, 3),
            Some((Slot::Offhand, Some(held))),
            "wraps round"
        );
        // Nothing at the hip: the banner says so.
        inventory.take_out(lantern);
        assert_eq!(compare_with(&inventory, spare, 2), Some((Slot::Hip, None)));
        // Both ring fingers worn: the first ring, then the second.
        let (ring1, ring2, third) = (
            inventory.add(roll("Ring")),
            inventory.add(roll("Ring")),
            inventory.add(roll("Ring")),
        );
        inventory.wear(ring1, Slot::Ring1).unwrap();
        inventory.wear(ring2, Slot::Ring2).unwrap();
        assert_eq!(
            compare_with(&inventory, third, 0),
            Some((Slot::Ring1, Some(ring1)))
        );
        assert_eq!(
            compare_with(&inventory, third, 1),
            Some((Slot::Ring2, Some(ring2)))
        );
        assert_eq!(
            compare_with(&inventory, third, 2),
            Some((Slot::Ring1, Some(ring1)))
        );
        // One slot only: every step shows the head.
        let helm = inventory.add(roll("Helm"));
        let other = inventory.add(roll("Helm"));
        inventory.wear(helm, Slot::Head).unwrap();
        assert_eq!(
            [0, 1, 2].map(|step| compare_with(&inventory, other, step)),
            [Some((Slot::Head, Some(helm))); 3]
        );
    }

    #[test]
    fn shift_with_alt_steps_on_and_a_new_item_starts_again() {
        let mut stepped: (Option<ItemId>, usize) = (None, 0);
        let mut step = |hovered: Option<u32>, comparing, shift| {
            compare_step(&mut stepped, hovered.map(ItemId), comparing, shift)
        };
        assert_eq!(step(Some(1), true, false), 0);
        assert_eq!(step(Some(1), true, true), 1);
        assert_eq!(
            step(Some(1), true, false),
            1,
            "the step stays while Alt is held"
        );
        assert_eq!(step(Some(1), true, true), 2);
        assert_eq!(
            step(Some(2), true, true),
            0,
            "another item starts at its first slot"
        );
        assert_eq!(step(Some(2), false, true), 0, "Shift alone does not step");
        assert_eq!(step(Some(2), true, true), 1);
        assert_eq!(
            step(Some(2), false, false),
            0,
            "letting go of Alt starts again"
        );
    }

    #[test]
    fn the_compare_card_sits_beside_the_card_away_from_the_item() {
        let screen = Vec2::new(1920.0, 1080.0);
        let size = Vec2::new(300.0, 400.0);
        // The card to the item's right: the compare card further right.
        let item = Rect::new(500.0, 300.0, 546.0, 392.0);
        let card = Rect::new(556.0, 300.0, 856.0, 700.0);
        assert_eq!(
            place_compare(card, Some(item), size, screen),
            Vec2::new(866.0, 300.0)
        );
        // The card flipped to the item's left: the compare card further left.
        let item = Rect::new(1700.0, 300.0, 1746.0, 392.0);
        let card = Rect::new(1390.0, 300.0, 1690.0, 700.0);
        assert_eq!(
            place_compare(card, Some(item), size, screen),
            Vec2::new(1080.0, 300.0)
        );
        // Under the crosshair it goes right, or left with no room on the right.
        let card = Rect::new(810.0, 560.0, 1110.0, 960.0);
        assert_eq!(place_compare(card, None, size, screen).x, 1120.0);
        let card = Rect::new(1600.0, 560.0, 1900.0, 960.0);
        assert_eq!(place_compare(card, None, size, screen).x, 1290.0);
        // Never off the screen.
        let at = place_compare(
            Rect::new(8.0, 900.0, 308.0, 1300.0),
            None,
            size,
            Vec2::new(400.0, 1080.0),
        );
        assert!(
            at.x >= CARD_MARGIN && at.y + size.y <= 1080.0 - CARD_MARGIN,
            "{at}"
        );
    }

    #[test]
    fn the_card_stays_on_screen_at_the_edges() {
        let screen = Vec2::new(1920.0, 1080.0);
        let size = Vec2::new(300.0, 400.0);
        let inside = |at: Vec2| {
            at.x >= CARD_MARGIN
                && at.y >= CARD_MARGIN
                && at.x + size.x <= screen.x - CARD_MARGIN
                && at.y + size.y <= screen.y - CARD_MARGIN
        };
        // Room on the right: it goes there, level with the item's top.
        let middle = Rect::new(500.0, 300.0, 546.0, 392.0);
        assert_eq!(
            place_beside(middle, size, screen),
            Vec2::new(546.0 + CARD_GAP, 300.0)
        );
        // At the right edge it flips to the left.
        let right = Rect::new(1850.0, 300.0, 1896.0, 346.0);
        let at = place_beside(right, size, screen);
        assert!(at.x + size.x <= 1850.0 && inside(at), "{at}");
        // At the bottom it rises to stay on screen.
        let bottom = Rect::new(500.0, 1040.0, 546.0, 1076.0);
        assert!(inside(place_beside(bottom, size, screen)));
        // Squeezed on a narrow screen it goes below, or above at the bottom.
        let narrow = Vec2::new(400.0, 1080.0);
        let at = place_beside(Rect::new(50.0, 100.0, 96.0, 146.0), size, narrow);
        assert_eq!(at.y, 146.0 + CARD_GAP);
        let at = place_beside(Rect::new(50.0, 900.0, 96.0, 946.0), size, narrow);
        assert!(at.y + size.y <= 900.0, "{at}");
        // Under the crosshair, and at a corner.
        assert!(inside(place_under(screen * 0.5, size, screen)));
        assert!(inside(place_under(Vec2::new(5.0, 1075.0), size, screen)));
        // Bigger than the screen: pinned to the top left.
        assert_eq!(
            keep_on_screen(Vec2::new(50.0, 50.0), Vec2::new(2000.0, 2000.0), screen),
            Vec2::splat(CARD_MARGIN)
        );
    }

    #[test]
    fn food_and_drink_cards_say_what_they_ease() {
        let line = |base: &str| consume_line(&fb_items::roll_item(base, 3).unwrap());
        assert_eq!(line("Bread").unwrap(), "Eases hunger 40%");
        assert_eq!(line("Tankard").unwrap(), "Eases thirst 50%");
        assert_eq!(line("Healing draught"), None);
        assert_eq!(line("Torch"), None);
    }

    #[test]
    fn the_foot_of_a_card_lists_its_keys() {
        use crate::interact::{Delver, key_line, verbs_for};
        let mut inventory = Inventory::default();
        let dagger = inventory.add(fb_items::roll_item("Dagger", 7).unwrap());
        let bread = inventory.add(fb_items::roll_item("Bread", 3).unwrap());
        let keymap = crate::keymap::Keymap::default();
        let line = |inventory: &Inventory, id| {
            let target = crate::interact::item_target(inventory, id).unwrap();
            key_line(&keymap, &verbs_for(&target, &Delver::default()))
        };
        assert_eq!(
            line(&inventory, dagger),
            "F Take \u{b7} G Equip \u{b7} Alt Compare"
        );
        let pack = inventory.add(fb_items::roll_item("Pack", 1).unwrap());
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        inventory.stow(bread).unwrap();
        assert_eq!(line(&inventory, bread), "F Eat");
    }

    #[test]
    fn a_shown_card_follows_a_rebinding() {
        use crate::keymap::{Action, Input, Keymap};
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Keymap>()
            .init_resource::<ScreenOpen>()
            .init_resource::<Hovered>()
            .init_resource::<Dragging>()
            .init_resource::<ShownCard>()
            .init_resource::<UiFonts>()
            .init_resource::<crate::inventory::icon::IconCache>()
            .init_resource::<Assets<Image>>()
            .insert_resource(Inventory::default())
            .add_systems(Update, show_card);
        let dagger = app
            .world_mut()
            .resource_mut::<Inventory>()
            .add(fb_items::roll_item("Bread", 3).unwrap());
        app.world_mut().spawn((
            ItemCard,
            Node::default(),
            Visibility::Hidden,
            ComputedNode::default(),
            ComputedUiRenderTargetInfo::default(),
        ));
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        app.world_mut().resource_mut::<Hovered>().0 =
            Some((dagger, Rect::new(0.0, 0.0, 10.0, 10.0)));
        let keys = |app: &mut App| {
            let mut texts = app.world_mut().query::<&Text>();
            texts
                .iter(app.world())
                .map(|t| t.0.clone())
                .collect::<Vec<_>>()
        };
        app.update();
        app.update();
        assert!(
            keys(&mut app).iter().any(|t| t == "F "),
            "{:?}",
            keys(&mut app)
        );
        let mut keymap = app.world_mut().resource_mut::<Keymap>();
        keymap
            .bindings
            .iter_mut()
            .find(|b| b.action == Action::Primary)
            .unwrap()
            .inputs = vec![Input::Key(KeyCode::KeyE)];
        app.update();
        app.update();
        let texts = keys(&mut app);
        assert!(texts.iter().any(|t| t == "E "), "{texts:?}");
        assert!(!texts.iter().any(|t| t == "F "), "{texts:?}");
    }

    /// The text of every node in the world, for the card's keys.
    fn card_texts(app: &mut App) -> Vec<String> {
        let mut texts = app.world_mut().query::<&Text>();
        let mut spans = app.world_mut().query::<&TextSpan>();
        let mut all: Vec<String> = texts.iter(app.world()).map(|t| t.0.clone()).collect();
        all.extend(spans.iter(app.world()).map(|t| t.0.clone()));
        all
    }

    /// The inventory open, the crosshair still on the chest it opened, a card showing for `id`.
    fn card_over(inventory: Inventory, ids: impl Fn(&Inventory) -> ItemId) -> Vec<String> {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<crate::keymap::Keymap>()
            .init_resource::<ScreenOpen>()
            .init_resource::<Hovered>()
            .init_resource::<Dragging>()
            .init_resource::<ShownCard>()
            .init_resource::<UiFonts>()
            .init_resource::<crate::inventory::icon::IconCache>()
            .init_resource::<Assets<Image>>()
            .init_resource::<crate::carry::Aimed>()
            .insert_resource(inventory)
            .add_systems(Update, show_card);
        let chest = app
            .world_mut()
            .resource_mut::<Inventory>()
            .add(fb_items::roll_item("Chest", 1).unwrap());
        let prop = app
            .world_mut()
            .spawn(crate::props::Container {
                id: chest,
                opened: true,
            })
            .id();
        app.world_mut().resource_mut::<crate::carry::Aimed>().0 = Some(prop);
        let id = ids(app.world().resource::<Inventory>());
        app.world_mut().spawn((
            ItemCard,
            Node::default(),
            Visibility::Hidden,
            ComputedNode::default(),
            ComputedUiRenderTargetInfo::default(),
        ));
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        app.world_mut().resource_mut::<Hovered>().0 = Some((id, Rect::new(0.0, 0.0, 10.0, 10.0)));
        app.update();
        app.update();
        card_texts(&mut app)
    }

    #[test]
    fn cards_in_the_screen_list_their_keys_though_the_crosshair_holds_a_chest() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 1).unwrap());
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let bread = inventory.add(fb_items::roll_item("Bread", 3).unwrap());
        inventory.stow(bread).unwrap();
        let ring = inventory.add(fb_items::roll_item("Ring", 3).unwrap());
        inventory.wear(ring, fb_items::Slot::Ring1).unwrap();
        let dagger = inventory.add(fb_items::roll_item("Dagger", 7).unwrap());
        let (b, r, d) = (bread, ring, dagger);
        let texts = card_over(inventory.clone(), |_| b);
        assert!(
            texts.iter().any(|t| t == "F ") && texts.iter().any(|t| t == "Eat"),
            "{texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == "\u{b7} X ") && texts.iter().any(|t| t == "Drop"),
            "{texts:?}"
        );
        let texts = card_over(inventory.clone(), |_| r);
        assert!(
            texts.iter().any(|t| t == "G ") && texts.iter().any(|t| t == "Take off"),
            "{texts:?}"
        );
        // Underfoot, the dagger is taken or equipped.
        let texts = card_over(inventory, |_| d);
        for want in ["F ", "Take", "\u{b7} G ", "Equip", "\u{b7} Alt ", "Compare"] {
            assert!(texts.iter().any(|t| t == want), "{want}: {texts:?}");
        }
    }

    #[test]
    fn a_light_card_shows_fuel_left_in_game_time() {
        assert_eq!(fuel_text(42.0 * 60.0), "Fuel 0:42");
        assert_eq!(fuel_text(6.0 * 3_600.0), "Fuel 6:00");
        assert_eq!(fuel_text(3_600.0 + 5.0 * 60.0), "Fuel 1:05");
        let mut torch = fb_items::roll_item("Spare torch", 3).unwrap();
        torch.shape.set_lit(true).unwrap();
        torch.burn(18.0 * 60.0);
        assert_eq!(
            light_line(&torch.shape).unwrap(),
            "Burning \u{b7} Fuel 0:42 \u{b7} L puts it out"
        );
        torch.burn(1.0e6);
        assert_eq!(light_line(&torch.shape).unwrap(), "Spent");
    }
}
