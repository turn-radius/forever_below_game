//! Hover cards: everything the card says about an item, taken from its `fb_items` item, and
//! where the card goes on screen. The plate's `itemInfoHTML` and `showTip`
//! (`reference/forever-below-plate.html`): name in its rarity's colour, a legendary's title
//! over its old name, a line of what it is and its main numbers, what each affix changes, a
//! container's contents, and a legendary's history.

use bevy::{prelude::*, ui::ComputedUiRenderTargetInfo};
use fb_items::{Item, Kind, Rarity, stats::Stats};

use super::icon::Icons;
use super::items::{Inventory, ItemId};
use super::{Dragging, Hovered};
use crate::play::{Screen, ScreenOpen};
use crate::ui::{BAD, BG, FAINT, GOLD, INK, LINE, MUTED, UiFonts, numbers_line, rarity_color};

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
    /// Rarity, what it is, weight, its main numbers and footprint, as one line.
    pub meta: String,
    /// What a container holds.
    pub contents: Option<String>,
    pub affixes: Vec<AffixLine>,
    /// Stats the meta line leaves out that differ from an ordinary thing's.
    pub stats: Vec<String>,
    /// A legendary's three lines of history.
    pub history: Vec<String>,
}

/// A number as the card writes it: at most two decimals, no trailing zeros.
pub fn number(value: f32) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn signed(value: f32) -> String {
    if value >= 0.0 { format!("+{}", number(value)) } else { format!("\u{2212}{}", number(-value)) }
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
            let bonus = if s.damage_bonus != 0.0 { signed(s.damage_bonus) } else { String::new() };
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
        [(!p.description.is_empty()).then(|| p.description.clone()), Some(weight), Some(cells)].into_iter().flatten().collect()
    };
    let contents = p.holds.as_ref().map(|holds| {
        let inside = format!("{}\u{d7}{} inside", holds.cols, holds.rows);
        match held {
            Some((inventory, id)) if inventory.contents_count(id) > 0 => {
                let n = inventory.contents_count(id);
                format!("Holds {n} {} \u{b7} {} lb inside \u{b7} {inside}", if n == 1 { "thing" } else { "things" }, number(inventory.contents_weight(id)))
            }
            _ => format!("Empty \u{b7} {inside}"),
        }
    });
    CardText {
        title: item.title().to_string(),
        rarity: item.rarity,
        subtitle: item.legend.as_ref().map(|l| l.subtitle.clone()),
        meta: meta.join(" \u{b7} "),
        contents,
        affixes: item.affixes.iter().map(|a| AffixLine { text: a.text.clone(), name: a.name.clone(), good: a.good }).collect(),
        stats: stat_lines(s),
        history: item.legend.as_ref().map_or_else(Vec::new, |l| l.history.to_vec()),
    }
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
    bonus(s.stealth, "stealth");
    bonus(s.lockpicking, "lockpicking");
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
    rate(s.fatigue_rate, plain.fatigue_rate, "fatigue");
    rate(s.disease_risk, plain.disease_risk, "disease risk");
    if s.break_chance > 0.0 {
        lines.push(format!("{}% chance to break on a hard blow", number(s.break_chance * 100.0)));
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
    keep_on_screen(Vec2::new(point.x - size.x * 0.5, point.y + 2.0 * CARD_GAP), size, screen)
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

/// Where a card goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CardAnchor {
    /// Beside an item on the screen ([`place_beside`]).
    Beside(Rect),
    /// Under the crosshair ([`place_under`]).
    Crosshair,
}

/// Spawns the card, hidden, once there is a camera drawing over the held torch to draw it.
pub fn spawn_card(mut commands: Commands, cards: Query<(), With<ItemCard>>, cameras: Query<Entity, With<crate::carry::ViewModelCamera>>) {
    if !cards.is_empty() {
        return;
    }
    let Some(camera) = cameras.iter().next() else { return };
    commands.spawn((
        Name::new("Item card"),
        DespawnOnExit(crate::launch::AppState::Delving),
        ItemCard,
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
        BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.55), px(0), px(14), px(0), px(34)),
        GlobalZIndex(40),
        Visibility::Hidden,
        Pickable::IGNORE,
    ));
}

/// Fills `card` with what [`card_text`] says, under the item's picture.
fn fill_card(commands: &mut Commands, card: Entity, item: &Item, text: &CardText, fonts: &UiFonts, icons: &mut Icons) {
    let footprint = item.profile.footprint;
    // The plate's `viewH`: the picture's height follows the item's shape, within limits.
    let picture = (CARD_WIDTH * 0.62 * footprint.rows as f32 / footprint.cols.max(1) as f32).clamp(110.0, 230.0);
    let size = super::icon::icon_size(item, false).as_vec2();
    let scale = ((CARD_WIDTH - 40.0) / size.x).min((picture - 20.0) / size.y);
    let colour = rarity_color(text.rarity);
    commands.entity(card).despawn_children().with_children(|card| {
        card.spawn((
            Node { height: px(picture), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(BG.mix(&GOLD, 0.12)),
        ))
        .with_children(|frame| {
            icons.spawn(frame, item, false, Node { width: px(size.x * scale), height: px(size.y * scale), ..default() }, 48.0);
        });
        card.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), padding: UiRect::new(px(14), px(14), px(10), px(12)), ..default() })
            .with_children(|info| {
                info.spawn((Text::new(text.title.clone()), UiFonts::style(&fonts.display, 22.0, colour)));
                if let Some(subtitle) = &text.subtitle {
                    info.spawn((Text::new(subtitle.clone()), UiFonts::style(&fonts.body_italic, 15.0, colour)));
                }
                numbers_line(info, fonts, &text.meta, 12.0);
                if let Some(contents) = &text.contents {
                    numbers_line(info, fonts, contents, 12.0);
                }
                for affix in &text.affixes {
                    let ink = if affix.good { INK } else { BAD };
                    info.spawn((Text::new(format!("\u{2022} {} ", affix.text)), UiFonts::style(&fonts.body, 14.0, ink))).with_child((
                        TextSpan::new(format!("({})", affix.name)),
                        UiFonts::style(&fonts.mono, 12.0, MUTED),
                    ));
                }
                // The totals, with every affix counted, on one line under them.
                if !text.stats.is_empty() {
                    numbers_line(info, fonts, &text.stats.join(" \u{b7} "), 12.0);
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
                            history.spawn((Text::new(line.clone()), UiFonts::style(&fonts.body_italic, 13.5, MUTED)));
                        }
                    });
                }
            });
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
        hovered.0.filter(|_| !dragging).map(|(id, rect)| (id, CardAnchor::Beside(rect)))
    } else {
        aimed.map(|id| (id, CardAnchor::Crosshair))
    }
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
    fonts: Res<UiFonts>,
    mut icons: Icons,
    mut cards: Query<(Entity, &mut Node, &mut Visibility, &ComputedNode, &ComputedUiRenderTargetInfo), With<ItemCard>>,
    mut shown: Local<Option<ItemId>>,
) {
    let aimed_item = aimed.and_then(|a| a.0).and_then(|entity| floor.get(entity).map(|f| f.id).or_else(|_| containers.get(entity).map(|c| c.id)).ok());
    // No card over the menu.
    let subject = if open.is(Screen::Menu) { None } else { card_subject(open.is(Screen::Inventory), *hovered, dragging.0.is_some_and(|d| d.active), aimed_item) };
    for (card, mut node, mut visibility, computed, target) in &mut cards {
        let Some((id, anchor)) = subject.filter(|(id, _)| inventory.item(*id).is_some()) else {
            *visibility = Visibility::Hidden;
            *shown = None;
            continue;
        };
        if *shown != Some(id) || inventory.is_changed() {
            let item = inventory.item(id).expect("checked above");
            fill_card(&mut commands, card, item, &card_text(item, Some((&inventory, id))), &fonts, &mut icons);
            *shown = Some(id);
            *visibility = Visibility::Hidden;
            continue;
        }
        let size = computed.size * computed.inverse_scale_factor;
        let screen = target.logical_size();
        let at = match anchor {
            CardAnchor::Beside(rect) => place_beside(rect, size, screen),
            CardAnchor::Crosshair => place_under(screen * 0.5, size, screen),
        };
        node.left = px(at.x);
        node.top = px(at.y);
        *visibility = Visibility::Inherited;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_field_comes_from_the_item() {
        let magic = (1..3_000).filter_map(|s| fb_items::roll_item("Sword", s)).find(|i| i.rarity == Rarity::Magic).unwrap();
        let card = card_text(&magic, None);
        assert_eq!((card.title.as_str(), card.rarity, card.subtitle.as_ref()), (magic.name.as_str(), Rarity::Magic, None));
        let s = &magic.profile.stats;
        assert!(card.meta.starts_with("Magic \u{b7} "), "{}", card.meta);
        assert!(card.meta.contains(&format!("{} lb", number(s.weight))), "{}", card.meta);
        assert!(card.meta.contains(&format!("{}", s.damage.unwrap())), "{}", card.meta);
        let fp = magic.profile.footprint;
        assert!(card.meta.ends_with(&format!("{}\u{d7}{} cells", fp.cols, fp.rows)), "{}", card.meta);
        assert_eq!(card.affixes.len(), magic.affixes.len());
        for (line, affix) in card.affixes.iter().zip(&magic.affixes) {
            assert_eq!((line.text.as_str(), line.name.as_str(), line.good), (affix.text.as_str(), affix.name.as_str(), affix.good));
        }
        assert!(card.history.is_empty());
    }

    #[test]
    fn a_legendary_shows_its_title_old_name_and_history() {
        let legend = (1..5_000).filter_map(|s| fb_items::roll_item("Sword", s)).find(|i| i.rarity == Rarity::Legendary).unwrap();
        let l = legend.legend.as_ref().unwrap();
        let card = card_text(&legend, None);
        assert_eq!(card.title, l.title);
        assert_eq!(card.subtitle.as_deref(), Some(l.subtitle.as_str()));
        assert_eq!(card.history, l.history.to_vec());
        assert_eq!(card.history.len(), 3);
        assert!(!card.meta.contains(&fb_items::naming::noun(&legend.base, &legend.shape)), "the old name already says what it is");
    }

    #[test]
    fn goods_and_containers_say_what_they_are() {
        let bread = fb_items::roll_item("Bread", 3).unwrap();
        let card = card_text(&bread, None);
        assert!(card.meta.starts_with(&bread.profile.description), "{}", card.meta);
        assert!(card.affixes.is_empty() && card.contents.is_none());

        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        assert_eq!(card_text(inventory.item(pack).unwrap(), Some((&inventory, pack))).contents.as_deref(), Some("Empty \u{b7} 8\u{d7}5 inside"));
        let bread = inventory.add(bread);
        inventory.put(bread, pack, super::super::grid::Placement::new(0, 0, false)).unwrap();
        let contents = card_text(inventory.item(pack).unwrap(), Some((&inventory, pack))).contents.unwrap();
        assert!(contents.starts_with("Holds 1 thing \u{b7} "), "{contents}");
    }

    #[test]
    fn stats_list_only_what_differs() {
        assert!(stat_lines(&Stats::default()).is_empty());
        let s = Stats { stealth: -2.0, hunger_rate: 0.85, darkvision: 10.0, ..Stats::default() };
        assert_eq!(stat_lines(&s), vec!["\u{2212}2 stealth", "darkvision 10 ft", "hunger \u{d7}0.85"]);
        assert_eq!(number(3.0), "3");
        assert_eq!(number(0.05), "0.05");
    }

    #[test]
    fn the_card_shows_the_hovered_item_on_the_screen_and_the_aimed_one_in_the_level() {
        let rect = Rect::new(0.0, 0.0, 46.0, 92.0);
        let hovered = Hovered(Some((ItemId(3), rect)));
        assert_eq!(card_subject(true, hovered, false, Some(ItemId(9))), Some((ItemId(3), CardAnchor::Beside(rect))));
        assert_eq!(card_subject(true, hovered, true, None), None, "not while dragging");
        assert_eq!(card_subject(false, hovered, false, Some(ItemId(9))), Some((ItemId(9), CardAnchor::Crosshair)));
        assert_eq!(card_subject(false, Hovered(None), false, None), None);
    }

    #[test]
    fn the_card_stays_on_screen_at_the_edges() {
        let screen = Vec2::new(1920.0, 1080.0);
        let size = Vec2::new(300.0, 400.0);
        let inside = |at: Vec2| {
            at.x >= CARD_MARGIN && at.y >= CARD_MARGIN && at.x + size.x <= screen.x - CARD_MARGIN && at.y + size.y <= screen.y - CARD_MARGIN
        };
        // Room on the right: it goes there, level with the item's top.
        let middle = Rect::new(500.0, 300.0, 546.0, 392.0);
        assert_eq!(place_beside(middle, size, screen), Vec2::new(546.0 + CARD_GAP, 300.0));
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
        assert_eq!(keep_on_screen(Vec2::new(50.0, 50.0), Vec2::new(2000.0, 2000.0), screen), Vec2::splat(CARD_MARGIN));
    }
}
