//! The commission screen: a smith, bowyer or tentmaker works with the delver at the Armory's
//! controls to make a new item to order (`docs/items.md`, "Commissions").
//!
//! The screen is the Forge view's pieces over the level: the item turns on a stage drawn into
//! an image (as the inventory's doll is), beside the Forge's slider panel ([`crate::tweak_panel`])
//! over the fields the maker's tier opens ([`open_controls`]). The delver picks what to make
//! from the families of the trade ([`bases_of`]), then one of the maker's rolls of it, then
//! shapes it. The price follows the base, material, size and ornament of the shaped item
//! ([`fb_items::commission`]) and the maker's tier, and updates as the item is shaped. Confirming
//! puts a [`Commission`] in [`Commissions`]; cancelling (or Esc) closes the screen.
//!
//! Open it with [`open`], or write an [`OpenCommission`] message, as the dialogue panel's
//! Commission topic does. `FB_COMMISSION=<profession>:<tier>[:<style>]` opens it at the start
//! for captures (`FB_COMMISSION=weaponsmith:master:blades`), and `FB_COMMISSION_SET=blade.length=1.4,etch.on=true`
//! then sets fields the way a delver's hand would.
//!
//! Not here yet: taking coins from the delver (there is no wallet) and handing the finished item
//! over (#554). An order only records what was asked for, its price and when it is ready.

use armory::{
    Item as Shape,
    geom::ItemMesh,
    reflect::{Field, FieldKind},
    slider::Slider,
};
use bevy::{
    camera::{Exposure, RenderTarget, ScalingMode, visibility::RenderLayers},
    input::mouse::AccumulatedMouseMotion,
    light::NotShadowCaster,
    prelude::*,
    render::view::Tonemapping,
    window::{CursorOptions, PrimaryWindow},
};
use fb_character::GameTime;
use fb_faction::{Profession, Style, Tier, Trade};
use fb_items::{
    armory_base::roll_shape,
    commission::{FieldClass, classify, material_options, section_label, shaped_copper, size_factor, visible_fields},
    item::Coins,
};
use procgen_rng::Mulberry32;

use crate::game_clock::GameClock;
use crate::launch::AppState;
use crate::light::StudioLight;
use crate::play::{DelveSystems, Screen, ScreenOpen, show_screen};
use crate::render::{ItemAppearance, ItemPalette, ItemSpawner, item_bounds};
use crate::inventory::Icons;
use crate::tweak_panel::{Tweak, TweakPanelPlugin, TweakPanelSystems, chip, panel_body};
use crate::turntable::{PLATE, PLATE_INK, SPIN_SPEED};
use crate::ui::{BG, BRIGHT, CELL, FAINT, INK, LINE, MUTED, UiFonts, node_rect, ui_cursor};

/// Width of the panel on the right, in the interface's logical pixels.
const PANEL_WIDTH: f32 = 430.0;
/// Width of the list of rolls on the left.
const ROLLS_WIDTH: f32 = 290.0;
/// The side of the box a roll's icon is fitted into.
const ICON_BOX: f32 = 64.0;
/// How many rolls of a family a maker offers.
pub const ROLLS: usize = 6;
/// The stage's render layer, beside the icons' (2), the doll's (3) and the held items' (1).
pub const COMMISSION_LAYER: usize = 4;
/// Where the stage sits, far from the level and from the other stages.
const STAGE: Vec3 = Vec3::new(30_000.0, -60_000.0, 30_000.0);
/// The stage image's side in pixels.
const STAGE_PIXELS: u32 = 1280;
/// Share of the stage view the item's longest side fills.
const FILL: f32 = 0.78;
/// What a drag across the stage turns the item by, radians a pixel.
const TURN_PER_PIXEL: f32 = 0.01;

/// What a tier charges over the base price (our own), by the tier the controls are open at.
pub fn tier_charge(tier: Tier) -> f64 {
    match tier {
        Tier::Novice => 1.0,
        Tier::Apprentice => 1.25,
        Tier::Expert => 1.6,
        Tier::Master => 2.5,
    }
}

/// The tier one better, a master staying one.
pub fn tier_up(tier: Tier) -> Tier {
    match tier {
        Tier::Novice => Tier::Apprentice,
        Tier::Apprentice => Tier::Expert,
        Tier::Expert | Tier::Master => Tier::Master,
    }
}

/// The families a trade makes, as the item bases the Armory knows them by: the panel's chips.
/// Empty for a trade that makes nothing to order.
pub fn bases_of(profession: Profession) -> &'static [&'static str] {
    match profession {
        Profession::Armorer => &["Gambeson", "Brigandine", "Mail", "Scale coat", "Cuirass", "Helm", "Hood", "Mail coif", "Gloves", "Gauntlets", "Shoes", "Boots", "Shield"],
        Profession::Weaponsmith => &["Sword", "Dagger", "Mace", "Club", "Hand axe", "Spear", "Staff"],
        Profession::Bowyer => &["Bow", "Crossbow", "Arrow", "Bolt", "Quiver", "Bolt case"],
        Profession::Tentmaker => &["Tent", "Campfire kit"],
        _ => &[],
    }
}

/// Whether `base` is among the things a smith of `style` specialises in: an armorer's armour of
/// that make (with the helm, coif, hands and feet that go with it), a weaponsmith's weapons.
pub fn in_style(style: Style, base: &str) -> bool {
    let bases: &[&str] = match style {
        Style::Padded => &["Gambeson", "Hood", "Gloves", "Shoes"],
        Style::Leather => &["Brigandine", "Hood", "Gloves", "Boots"],
        Style::Mail => &["Mail", "Mail coif"],
        Style::Scale => &["Scale coat"],
        Style::Plate => &["Cuirass", "Helm", "Gauntlets", "Boots", "Shield"],
        Style::Blades => &["Sword", "Dagger"],
        Style::AxesAndClubs => &["Hand axe", "Club"],
        Style::Maces => &["Mace"],
        Style::SpearsAndStaves => &["Spear", "Staff"],
    };
    bases.contains(&base)
}

/// The tier the controls open at for `base` from `trade`: the maker's own, one better in their
/// style.
pub fn controls_tier(trade: &Trade, base: &str) -> Tier {
    match trade.style {
        Some(style) if in_style(style, base) => tier_up(trade.tier),
        _ => trade.tier,
    }
}

/// The middle half of a range: the share of it an expert's hand reaches.
pub fn middle(range: Slider) -> Slider {
    // Both ends on the range's own step grid, so a value at either end is one the slider can hold.
    let quarter = (range.max - range.min) * 0.25;
    let steps = |length: f32| (length / range.step + 1e-3).floor();
    let low = range.min + (quarter / range.step - 1e-3).ceil() * range.step;
    let high = range.min + steps(range.max - quarter - range.min) * range.step;
    range.narrowed(low, high.max(low))
}

/// The fields a maker of `tier` opens on `shape`, in the Armory's order, with the ranges and
/// options the delver may use. Which field is what is [`fb_items::commission::classify`]:
///
/// - novice: none (the rolls are the choice);
/// - apprentice: materials and colours (a material's options are what the part is made of:
///   [`material_options`]; parts that are switched off, and a cloth garment's body material, are
///   not shown);
/// - expert: those, and every shape slider across the middle half of its range
///   ([`middle`]), but no shape choice or part switched on or off;
/// - master: everything but the condition, at full ranges: shape sliders, choices and toggles,
///   etching, gems and inlay.
pub fn open_controls(shape: &Shape, tier: Tier) -> Vec<Field> {
    visible_fields(shape)
        .into_iter()
        .filter_map(|mut field| {
            let class = classify(&field)?;
            let open = match (tier, class) {
                (Tier::Novice, _) => false,
                (Tier::Apprentice | Tier::Expert, FieldClass::Material | FieldClass::Colour) => true,
                (Tier::Apprentice, _) => false,
                (Tier::Expert, FieldClass::Shape) => matches!(field.kind, FieldKind::Slider { .. }),
                (Tier::Expert, FieldClass::Ornament) => false,
                (Tier::Master, _) => true,
            };
            if !open {
                return None;
            }
            let offered = if class == FieldClass::Material { Some(material_options(shape, &field)) } else { None };
            match &mut field.kind {
                FieldKind::Slider { range, .. } if tier == Tier::Expert => *range = middle(*range),
                FieldKind::Choice { options, .. } => {
                    if let Some(offered) = offered {
                        *options = offered;
                    }
                }
                _ => {}
            }
            // Say what each control dresses.
            if let Some(label) = section_label(&field.section) {
                field.section = label.to_string();
            }
            field.name = match field.name.as_str() {
                "color" => "colour".to_string(),
                "color2" => "pattern colour".to_string(),
                "wood" | "leather" | "cord" | "fletching" if field.path.starts_with("palette.") => format!("{} dye", field.name),
                name => name.replace("_color", " colour"),
            };
            Some(field)
        })
        .collect()
}

/// What the controls of `tier` offer, in words: the line under the maker's name.
pub fn controls_summary(tier: Tier) -> &'static str {
    match tier {
        Tier::Novice => "Open: a few rolls to choose from",
        Tier::Apprentice => "Open: rolls, materials and colours",
        Tier::Expert => "Open: rolls, materials, colours and the middle of each shape range",
        Tier::Master => "Open: every range, etching, inlay and gems",
    }
}

/// An order placed with a maker. Collecting it (#554) hands the item over when `ready_at` has
/// come.
#[derive(Clone, Debug, PartialEq)]
pub struct Commission {
    pub maker: String,
    pub faction: String,
    /// The Armory base ("Sword"), the family it was shaped from.
    pub base: String,
    /// The seed of the roll it started as.
    pub seed: u32,
    /// The shape as the delver left it.
    pub shape: Shape,
    /// What it costs, in copper.
    pub price_copper: u64,
    /// The game time it is ready at.
    pub ready_at: GameTime,
}

impl Commission {
    pub fn price(&self) -> Coins {
        Coins::from_copper(self.price_copper)
    }
}

/// The orders the delver has placed, oldest first.
#[derive(Resource, Default, Debug, PartialEq)]
pub struct Commissions(pub Vec<Commission>);

/// A request to open the screen with `maker` (a request from the dialogue panel).
#[derive(Message, Clone, Debug, PartialEq)]
pub struct OpenCommission {
    pub trade: Trade,
    pub maker: String,
    pub faction: String,
}

/// Game hours an order of `price_copper` takes: four, and more the dearer it is (our own).
pub fn ready_hours(price_copper: u64) -> f64 {
    (4.0 + (price_copper as f64).sqrt() * 0.5).min(72.0)
}

/// The commission being shaped. `open` is set while the screen is up.
#[derive(Resource, Default)]
pub struct CommissionScreen {
    open: bool,
    trade: Option<Trade>,
    maker: String,
    faction: String,
    maker_seed: u32,
    base: &'static str,
    roll: usize,
    /// The maker's rolls of the family showing: seed, price and a line about each.
    rolls: Vec<RollRow>,
    shape: Option<Shape>,
    mesh: Option<ItemMesh>,
    /// The shaped item's bounding volume, and a typical roll's, in world units cubed.
    volume: f32,
    reference: f32,
    /// Changes when the panel needs a new shape (a new base or roll).
    structure: u32,
}

/// One of the maker's rolls of a family, as the list offers it.
#[derive(Clone, Debug)]
pub struct RollRow {
    pub seed: u32,
    /// What it costs as rolled, tier's charge included, in copper.
    pub price_copper: u64,
    /// "iron, 112 cm long": its main material and its length.
    pub blurb: String,
    /// The roll as an item, for its icon.
    pub icon: fb_items::Item,
}

/// What the main material of `shape` is called in a blurb.
fn material_word(option: &str) -> &str {
    match option {
        "metal" => "iron",
        "accent" => "bronze",
        other => other,
    }
}

/// A stable seed from a name (FNV-1a).
fn name_seed(name: &str) -> u32 {
    name.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193))
}

/// A new item of `base` rolled from `seed`: no wear.
fn new_shape(base: &str, seed: u32) -> Option<Shape> {
    let mut shape = roll_shape(base, seed)?;
    let mut condition = shape.condition();
    condition.wear = 0.0;
    condition.damage = 0.0;
    condition.broken = false;
    condition.state = armory::cond::ConditionState::Pristine;
    shape.set_condition(condition);
    Some(shape)
}

/// `shape` as a plain common item of `base`, for its icon. Its seed is salted so the icon is
/// cached apart from a found item's of the same base and seed.
fn icon_item(base: &str, seed: u32, shape: Shape) -> Option<fb_items::Item> {
    let mut parts = fb_items::ItemParts::from(fb_items::roll_item(base, seed)?);
    parts.seed ^= 0xC0DE_1C0D;
    parts.shape = shape;
    parts.rarity = fb_items::Rarity::Common;
    parts.affixes.clear();
    fb_items::Item::from_parts(parts)
}

fn volume_of(mesh: &ItemMesh) -> f32 {
    item_bounds(mesh).map_or(0.0, |b| {
        let size = Vec3::from(b.max - b.min);
        size.x * size.y * size.z
    })
}

impl CommissionScreen {
    /// A screen open on `trade`'s first family, or `None` for a trade that makes nothing.
    pub fn begin(trade: Trade, maker: &str, faction: &str) -> Option<CommissionScreen> {
        let base = *bases_of(trade.profession).first()?;
        let mut screen = CommissionScreen {
            open: true,
            trade: Some(trade),
            maker: maker.to_string(),
            faction: faction.to_string(),
            maker_seed: name_seed(maker),
            ..default()
        };
        screen.set_base(base);
        Some(screen)
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn maker(&self) -> &str {
        &self.maker
    }

    pub fn base(&self) -> &'static str {
        self.base
    }

    pub fn roll(&self) -> usize {
        self.roll
    }

    /// The maker's rolls of the family showing.
    pub fn rolls(&self) -> &[RollRow] {
        &self.rolls
    }

    pub fn shape(&self) -> Option<&Shape> {
        self.shape.as_ref()
    }

    pub fn trade(&self) -> Option<Trade> {
        self.trade
    }

    /// The tier the controls are open at for the family showing.
    pub fn tier(&self) -> Tier {
        self.trade.map_or(Tier::Novice, |t| controls_tier(&t, self.base))
    }

    /// Whether the family showing is in the maker's own style.
    pub fn own_style(&self) -> bool {
        self.trade.is_some_and(|t| t.style.is_some_and(|s| in_style(s, self.base)))
    }

    /// The seed of the maker's `index`th roll of `base`: the same each time they are asked.
    pub fn roll_seed(&self, base: &str, index: usize) -> u32 {
        let mut rng = Mulberry32::new(self.maker_seed ^ name_seed(base).rotate_left(11) ^ (index as u32 + 1).wrapping_mul(0x9e37_79b9));
        (rng.next_f64() * 4_294_967_296.0) as u32
    }

    /// Moves to `base`, working out the maker's rolls of it and showing the first.
    pub fn set_base(&mut self, base: &'static str) {
        let Some(trade) = self.trade else { return };
        if !bases_of(trade.profession).contains(&base) {
            return;
        }
        self.base = base;
        let made: Vec<(u32, Shape, ItemMesh)> = (0..ROLLS)
            .filter_map(|i| {
                let seed = self.roll_seed(base, i);
                let shape = new_shape(base, seed)?;
                let mesh = shape.build();
                Some((seed, shape, mesh))
            })
            .collect();
        self.reference = made.iter().map(|(_, _, mesh)| volume_of(mesh)).sum::<f32>() / made.len().max(1) as f32;
        let tier = self.tier();
        self.rolls = made
            .into_iter()
            .filter_map(|(seed, shape, mesh)| {
                let price = shaped_copper(base, &shape, size_factor(volume_of(&mesh), self.reference));
                let materials: Vec<Field> = visible_fields(&shape).into_iter().filter(|f| classify(f) == Some(FieldClass::Material)).collect();
                // What the thing is chiefly made of: its blade, head or body before its fittings.
                let main = ["blade", "head", "cut", "limbs", "tent", "shaft"].iter().find_map(|s| materials.iter().find(|f| f.section == *s)).or(materials.first()).and_then(|f| match &f.kind {
                    FieldKind::Choice { value, .. } => Some(material_word(value).to_string()),
                    _ => None,
                });
                let length = item_bounds(&mesh).map_or(0.0, |b| Vec3::from(b.max - b.min).max_element());
                // A world unit is 10 cm.
                let blurb = match main {
                    Some(main) => format!("{main}, {:.0} cm long", length * 10.0),
                    None => format!("{:.0} cm long", length * 10.0),
                };
                Some(RollRow { seed, price_copper: ((price as f64 * tier_charge(tier)).round() as u64).max(1), blurb, icon: icon_item(base, seed, shape)? })
            })
            .collect();
        self.set_roll(0);
    }

    /// Shows the maker's `index`th roll of the family, dropping the edits.
    pub fn set_roll(&mut self, index: usize) {
        let index = index.min(ROLLS - 1);
        self.roll = index;
        self.shape = new_shape(self.base, self.roll_seed(self.base, index));
        self.normalise();
        self.structure += 1;
        self.remesh();
    }

    /// Brings what the maker's tier opens into what it offers: a rolled material the part is not
    /// made of in this trade becomes the first of its options, and an expert's sliders (which reach
    /// only the middle of their ranges) come inside them.
    fn normalise(&mut self) {
        let Some(shape) = self.shape.as_mut() else { return };
        for field in open_controls(shape, self.trade.map_or(Tier::Novice, |t| controls_tier(&t, self.base))) {
            match field.kind {
                FieldKind::Choice { value, options } if !options.contains(&value) => {
                    if let Some(first) = options.first() {
                        let _ = shape.set_choice(&field.path, first);
                    }
                }
                FieldKind::Slider { value, range } if !range.contains(value) => {
                    let _ = shape.set_slider(&field.path, range.snap(value));
                }
                _ => {}
            }
        }
    }

    fn remesh(&mut self) {
        self.mesh = self.shape.as_ref().map(Shape::build);
        self.volume = self.mesh.as_ref().map_or(0.0, volume_of);
    }

    /// The fields the delver may use now.
    pub fn controls(&self) -> Vec<Field> {
        self.shape.as_ref().map(|s| open_controls(s, self.tier())).unwrap_or_default()
    }

    /// What the shaped item costs in copper: the base price times material, size and ornament,
    /// times what the maker's tier charges.
    pub fn price_copper(&self) -> u64 {
        let Some(shape) = &self.shape else { return 0 };
        let base = shaped_copper(self.base, shape, size_factor(self.volume, self.reference));
        ((base as f64 * tier_charge(self.tier())).round() as u64).max(1)
    }

    pub fn price(&self) -> Coins {
        Coins::from_copper(self.price_copper())
    }

    /// The order as it stands, ready `ready_hours` after `now` (game seconds).
    pub fn order(&self, now: f64) -> Option<Commission> {
        let shape = self.shape.clone()?;
        let price_copper = self.price_copper();
        Some(Commission {
            maker: self.maker.clone(),
            faction: self.faction.clone(),
            base: self.base.to_string(),
            seed: self.roll_seed(self.base, self.roll),
            shape,
            price_copper,
            ready_at: GameTime::from_seconds(now + ready_hours(price_copper) * 3600.0),
        })
    }

    /// Sets the field at `path` to `value`, as a hand would: a number for a slider, an option's
    /// name for a choice, `true` or `false` for a toggle. Only an open control takes it.
    pub fn set_field(&mut self, path: &str, value: &str) -> Result<(), String> {
        let field = self.controls().into_iter().find(|f| f.path == path).ok_or_else(|| format!("{path} is not open to a {}", self.tier().label()))?;
        match field.kind {
            FieldKind::Slider { range, .. } => self.set_slider(path, value.parse::<f32>().map_err(|e| e.to_string())? .clamp(range.min, range.max)),
            FieldKind::Choice { options, .. } => {
                if !options.iter().any(|o| o == value) {
                    return Err(format!("{value} is not one of {options:?}"));
                }
                if let Some(shape) = &mut self.shape {
                    shape.set_choice(path, value)?;
                }
                self.remesh();
            }
            FieldKind::Toggle { .. } => {
                if let Some(shape) = &mut self.shape {
                    shape.set_toggle(path, value == "true")?;
                }
                self.remesh();
            }
            FieldKind::Colour { .. } => {
                let hex = u32::from_str_radix(value.trim_start_matches('#'), 16).map_err(|e| e.to_string())?;
                let rgb = armory::material::Rgb::hex(hex);
                self.set_colour(path, [rgb.0, rgb.1, rgb.2]);
            }
        }
        Ok(())
    }

    fn close(&mut self) {
        self.open = false;
    }
}

impl Tweak for CommissionScreen {
    fn fields(&self) -> Vec<Field> {
        if self.open { self.controls() } else { Vec::new() }
    }

    fn set_slider(&mut self, path: &str, value: f32) {
        let Some(range) = self.controls().into_iter().find(|f| f.path == path).and_then(|f| match f.kind {
            FieldKind::Slider { range, .. } => Some(range),
            _ => None,
        }) else {
            return;
        };
        if let Some(shape) = &mut self.shape {
            let _ = shape.set_slider(path, range.snap(value));
        }
        self.remesh();
    }

    fn cycle(&mut self, path: &str, by: isize) {
        let Some((options, current)) = self.controls().into_iter().find(|f| f.path == path).and_then(|f| match f.kind {
            FieldKind::Choice { options, value } => Some((options, value)),
            _ => None,
        }) else {
            return;
        };
        if options.is_empty() {
            return;
        }
        // A roll can come with a value the maker does not offer; the first click lands on an option.
        let at = options.iter().position(|o| *o == current);
        let next = match at {
            Some(at) => (at as isize + by).rem_euclid(options.len() as isize) as usize,
            None if by >= 0 => 0,
            None => options.len() - 1,
        };
        if let Some(shape) = &mut self.shape {
            let _ = shape.set_choice(path, &options[next]);
        }
        self.remesh();
    }

    fn set_colour(&mut self, path: &str, rgb: [f32; 3]) {
        if !self.controls().iter().any(|f| f.path == path && matches!(f.kind, FieldKind::Colour { .. })) {
            return;
        }
        if let Some(shape) = &mut self.shape {
            let _ = shape.set_colour(path, rgb);
        }
        self.remesh();
    }

    fn flip(&mut self, path: &str) {
        if !self.controls().iter().any(|f| f.path == path) {
            return;
        }
        if let Some(shape) = &mut self.shape {
            let on = shape.toggle(path).unwrap_or(false);
            let _ = shape.set_toggle(path, !on);
        }
        self.remesh();
    }

    fn structure(&self) -> u32 {
        self.structure
    }

    fn slider(&self, path: &str) -> Option<f32> {
        self.shape.as_ref()?.slider(path)
    }
}

/// Opens the screen for `trade`'s `maker` of `faction`, freeing the mouse. Returns whether it
/// opened: a trade that makes nothing to order (a guard, an alchemist) leaves things as they
/// were.
pub fn open(screen: &mut CommissionScreen, open: &mut ScreenOpen, cursor: Option<&mut CursorOptions>, trade: Trade, maker: &str, faction: &str) -> bool {
    let Some(mut begun) = CommissionScreen::begin(trade, maker, faction) else { return false };
    // The panel is built afresh each time the screen opens.
    begun.structure = screen.structure + begun.structure + 1;
    *screen = begun;
    show_screen(open, Some(Screen::Commission), cursor);
    true
}

/// Reads `FB_COMMISSION=<profession>:<tier>[:<style>]`: "weaponsmith:master:blades",
/// "armorer:apprentice", "weaponsmith:expert:axes_and_clubs" (spaces or underscores).
pub fn parse_trade(text: &str) -> Result<Trade, String> {
    let mut parts = text.split(':').map(|p| p.trim().to_lowercase().replace(['_', '-'], " "));
    let profession = parts.next().unwrap_or_default();
    let profession = Profession::TRADES.iter().copied().find(|p| p.label() == profession).ok_or_else(|| format!("no profession {profession:?}"))?;
    let tier = parts.next().unwrap_or_else(|| "novice".to_string());
    let tier = Tier::ALL.iter().copied().find(|t| t.label() == tier).ok_or_else(|| format!("no tier {tier:?}"))?;
    let style = match parts.next() {
        Some(text) if !text.is_empty() => {
            Some(Style::ARMOUR.iter().chain(&Style::WEAPONS).copied().find(|s| s.label() == text).ok_or_else(|| format!("no style {text:?}"))?)
        }
        _ => None,
    };
    Ok(Trade { profession, tier, style })
}

pub struct CommissionPlugin;

impl Plugin for CommissionPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<bevy::app::HierarchyPropagatePlugin<RenderLayers>>() {
            app.add_plugins(bevy::app::HierarchyPropagatePlugin::<RenderLayers>::new(Update));
        }
        app.init_resource::<CommissionScreen>()
            .init_resource::<Commissions>()
            .init_resource::<CommissionStage>()
            .init_resource::<Turn>()
            .add_message::<OpenCommission>()
            .add_plugins(TweakPanelPlugin::<CommissionScreen>::default())
            .add_systems(
                PostStartup,
                open_for_capture.run_if(in_state(AppState::Delving)).run_if(|| std::env::var("FB_COMMISSION").is_ok_and(|v| !v.is_empty())),
            )
            .add_systems(
                Update,
                (
                    (from_talk, open_requests, keep_in_step, press_controls).chain().before(TweakPanelSystems),
                    (build_screen, fill_rolls, show_item, refresh_header, size_stage, turn_item).chain().after(TweakPanelSystems),
                    scroll_for_capture.run_if(|| std::env::var("FB_COMMISSION_SCROLL").is_ok()).after(TweakPanelSystems),
                )
                    .in_set(DelveSystems),
            );
    }
}

/// The stage's render target and camera.
#[derive(Resource)]
struct CommissionStage {
    image: Handle<Image>,
    camera: Option<Entity>,
}

impl FromWorld for CommissionStage {
    fn from_world(world: &mut World) -> Self {
        let mut image = Image::new_target_texture(STAGE_PIXELS, STAGE_PIXELS, bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb, None);
        image.asset_usage = bevy::asset::RenderAssetUsages::RENDER_WORLD | bevy::asset::RenderAssetUsages::MAIN_WORLD;
        CommissionStage { image: world.resource_mut::<Assets<Image>>().add(image), camera: None }
    }
}

/// How far the item has turned, and how big it is for the camera.
#[derive(Resource, Default)]
struct Turn {
    angle: f32,
    size: Vec3,
    centre: Vec3,
    built_for: Option<u32>,
    /// A capture's fixed angle (`FB_COMMISSION_TURN`): the item does not turn.
    hold: Option<f32>,
}

#[derive(Component)]
struct CommissionRoot;

#[derive(Component)]
struct Pivot;

/// The square the stage image fills.
#[derive(Component)]
struct StageFrame;

#[derive(Component)]
struct StageImage;

/// The column of rolls, and the base it was filled for.
#[derive(Component)]
struct RollList(&'static str);

/// What a click on a button of the screen does.
#[derive(Component, Clone, Debug, PartialEq)]
enum Control {
    Base(&'static str),
    Roll(usize),
    Confirm,
    Cancel,
}

/// Text that follows the screen.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum Readout {
    Maker,
    Trade,
    Open,
    Price,
    Ready,
}

/// The talk chose Commission: ask for the screen with the speaker as the maker.
fn from_talk(mut chosen: MessageReader<crate::talk::ServiceChosen>, talk: Res<crate::talk::Talk>, mut requests: MessageWriter<OpenCommission>) {
    for chosen in chosen.read() {
        if chosen.service != crate::talk::Service::Commission {
            continue;
        }
        let Some(speaker) = talk.speaker.as_ref() else { continue };
        let Some(trade) = speaker.trade else { continue };
        requests.write(OpenCommission { trade, maker: speaker.name.clone(), faction: speaker.faction.clone() });
    }
}

fn open_requests(
    mut requests: MessageReader<OpenCommission>,
    mut screen: ResMut<CommissionScreen>,
    mut open_screen: ResMut<ScreenOpen>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
) {
    for request in requests.read() {
        open(&mut screen, &mut open_screen, cursor.as_deref_mut().map(|c| &mut **c), request.trade, &request.maker, &request.faction);
    }
}

/// `FB_COMMISSION`: opens the screen at the start; `FB_COMMISSION_SET` then shapes it.
fn open_for_capture(mut screen: ResMut<CommissionScreen>, mut turn: ResMut<Turn>, mut open_screen: ResMut<ScreenOpen>, mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>) {
    let text = std::env::var("FB_COMMISSION").unwrap_or_default();
    let trade = match parse_trade(&text) {
        Ok(trade) => trade,
        Err(error) => {
            warn!("FB_COMMISSION: {error}");
            return;
        }
    };
    if !open(&mut screen, &mut open_screen, cursor.as_deref_mut().map(|c| &mut **c), trade, "Orla Venn", "the Cinder Hold") {
        warn!("FB_COMMISSION: {} makes nothing to order", trade.profession.label());
        return;
    }
    if let Some(angle) = std::env::var("FB_COMMISSION_TURN").ok().and_then(|v| v.trim().parse::<f32>().ok()) {
        turn.hold = Some(angle);
        turn.angle = angle;
    }
    if let Ok(base) = std::env::var("FB_COMMISSION_BASE") {
        if let Some(&base) = bases_of(trade.profession).iter().find(|b| b.eq_ignore_ascii_case(base.trim())) {
            screen.set_base(base);
        }
    }
    if let Ok(roll) = std::env::var("FB_COMMISSION_ROLL") {
        screen.set_roll(roll.trim().parse().unwrap_or(0));
    }
    for pair in std::env::var("FB_COMMISSION_SET").unwrap_or_default().split(',').filter(|p| !p.is_empty()) {
        match pair.split_once('=') {
            Some((path, value)) => {
                if let Err(error) = screen.set_field(path.trim(), value.trim()) {
                    warn!("FB_COMMISSION_SET: {error}");
                }
            }
            None => warn!("FB_COMMISSION_SET: {pair:?} is not path=value"),
        }
    }
}

/// A screen closed from outside (Esc) is no longer open.
fn keep_in_step(mut screen: ResMut<CommissionScreen>, open: Res<ScreenOpen>) {
    if screen.open && !open.is(Screen::Commission) {
        screen.close();
    }
}

fn clock_now(clock: Option<&GameClock>) -> f64 {
    clock.map_or(0.0, GameClock::game_seconds)
}

/// A click on a button picks a family or a roll, confirms or cancels.
#[allow(clippy::too_many_arguments)]
fn press_controls(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    controls: Query<(&Control, &ComputedNode, &UiGlobalTransform)>,
    mut screen: ResMut<CommissionScreen>,
    mut open: ResMut<ScreenOpen>,
    mut orders: ResMut<Commissions>,
    clock: Option<Res<GameClock>>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
) {
    if !screen.open || !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(point) = window.and_then(|w| ui_cursor(&w, ui_scale.as_deref())) else { return };
    let Some((control, ..)) = controls.iter().find(|(_, node, transform)| node_rect(node, transform).contains(point)) else { return };
    match control.clone() {
        Control::Base(base) => screen.set_base(base),
        Control::Roll(index) => screen.set_roll(index),
        Control::Confirm => {
            // Coins are not taken: the delver has no wallet to spend from yet.
            if let Some(order) = screen.order(clock_now(clock.as_deref())) {
                orders.0.push(order);
            }
            screen.close();
            show_screen(&mut open, None, cursor.as_deref_mut().map(|c| &mut **c));
        }
        Control::Cancel => {
            screen.close();
            show_screen(&mut open, None, cursor.as_deref_mut().map(|c| &mut **c));
        }
    }
}

/// Builds the screen when it opens and takes it down when it closes.
#[allow(clippy::too_many_arguments)]
fn build_screen(
    mut commands: Commands,
    mut screen: ResMut<CommissionScreen>,
    mut stage: ResMut<CommissionStage>,
    fonts: Res<UiFonts>,
    root: Query<Entity, With<CommissionRoot>>,
    pivots: Query<Entity, With<Pivot>>,
    mut cameras: Query<&mut Camera>,
    ui_cameras: Query<Entity, With<crate::carry::ViewModelCamera>>,
    mut turn: ResMut<Turn>,
) {
    let wanted = screen.open;
    let built = root.single().ok();
    if let Some(camera) = stage.camera.and_then(|c| cameras.get_mut(c).ok()).as_mut()
        && camera.is_active != wanted
    {
        camera.is_active = wanted;
    }
    match (wanted, built) {
        (false, Some(root)) => {
            commands.entity(root).despawn();
            for pivot in &pivots {
                commands.entity(pivot).despawn();
            }
            turn.built_for = None;
        }
        (true, None) => {
            // The interface's own camera, so the screen covers the held item.
            let Some(ui_camera) = ui_cameras.iter().next() else { return };
            spawn_ui(&mut commands, &fonts, &screen, &stage, ui_camera);
            // The panel and the readouts fill in from the screen on its next change.
            screen.set_changed();
            if stage.camera.is_none() {
                stage.camera = Some(spawn_stage(&mut commands, &stage));
            }
        }
        _ => {}
    }
}

fn spawn_stage(commands: &mut Commands, stage: &CommissionStage) -> Entity {
    let layer = RenderLayers::layer(COMMISSION_LAYER);
    let exposure = Exposure::INDOOR;
    let ambient = StudioLight::ARMORY;
    commands
        .spawn((
            Name::new("Commission camera"),
            crate::render::ScreenInkExempt,
            DespawnOnExit(AppState::Delving),
            Camera3d::default(),
            Camera { order: -5, clear_color: ClearColorConfig::Custom(PLATE), is_active: true, ..default() },
            RenderTarget::Image(stage.image.clone().into()),
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed { width: 40.0, height: 40.0 },
                near: 0.1,
                far: 4000.0,
                ..OrthographicProjection::default_3d()
            }),
            exposure,
            Tonemapping::Linear,
            AmbientLight { color: ambient.ambient_color, brightness: ambient.ambient / exposure.exposure(), ..default() },
            layer.clone(),
            Transform::from_translation(STAGE + Vec3::new(0.0, 0.0, 400.0)).looking_at(STAGE, Vec3::Y),
        ))
        .with_children(|camera| {
            let (light, at) = ambient.key_light(&exposure);
            camera.spawn((light, at, layer));
        })
        .id()
}

fn spawn_ui(commands: &mut Commands, fonts: &UiFonts, screen: &CommissionScreen, stage: &CommissionStage, ui_camera: Entity) {
    let bases = screen.trade.map_or(&[][..], |t| bases_of(t.profession));
    commands
        .spawn((
            CommissionRoot,
            UiTargetCamera(ui_camera),
            DespawnOnExit(AppState::Delving),
            GlobalZIndex(22),
            Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), ..default() },
            BackgroundColor(PLATE),
        ))
        .with_children(|root| {
            // The maker's rolls of the family, filled in by `fill_rolls`.
            root.spawn((
                RollList(""),
                Node {
                    width: px(ROLLS_WIDTH),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8),
                    padding: UiRect::all(px(12)),
                    border: UiRect::right(px(1)),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                BackgroundColor(BG),
                BorderColor::all(LINE),
            ));
            // The stage: the item on its turntable, drawn into a square as large as fits.
            root.spawn((StageFrame, Node { flex_grow: 1.0, min_width: px(0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, overflow: Overflow::clip(), ..default() }))
                .with_children(|frame| {
                    frame.spawn((StageImage, ImageNode::new(stage.image.clone()), Node { width: px(400), height: px(400), flex_shrink: 0.0, ..default() }));
                    frame.spawn((
                        Text::new("Drag to turn the item"),
                        UiFonts::style(&fonts.body_medium, 17.0, PLATE_INK),
                        Node { position_type: PositionType::Absolute, left: px(20), bottom: px(16), ..default() },
                    ));
                });
            root.spawn((
                Node {
                    width: px(PANEL_WIDTH),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Column,
                    border: UiRect::left(px(1)),
                    ..default()
                },
                BackgroundColor(BG),
                BorderColor::all(LINE),
            ))
            .with_children(|panel| {
                panel
                    .spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(9), padding: UiRect::all(px(14)), border: UiRect::bottom(px(1)), ..default() }, BorderColor::all(FAINT)))
                    .with_children(|header| {
                        header.spawn((Text::new("Commission"), UiFonts::style(&fonts.display_bold, 28.0, INK)));
                        header.spawn((Readout::Maker, Text::new(""), UiFonts::style(&fonts.display, 20.0, BRIGHT)));
                        header.spawn((Readout::Trade, Text::new(""), UiFonts::style(&fonts.body, 17.0, MUTED)));
                        header.spawn((Readout::Open, Text::new(""), UiFonts::style(&fonts.body, 17.0, MUTED)));
                        header.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(5), row_gap: px(5), ..default() }).with_children(|row| {
                            for &base in bases {
                                chip(row, fonts, base, Control::Base(base));
                            }
                        });
                    });
                panel_body(panel);
                panel
                    .spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(10), padding: UiRect::all(px(14)), border: UiRect::top(px(1)), ..default() }, BorderColor::all(FAINT)))
                    .with_children(|footer| {
                        footer.spawn(Node { align_items: AlignItems::Baseline, column_gap: px(10), ..default() }).with_children(|row| {
                            row.spawn((Text::new("Price"), UiFonts::style(&fonts.body, 17.0, MUTED)));
                            row.spawn((Readout::Price, Text::new(""), UiFonts::style(&fonts.mono_bold, 22.0, BRIGHT)));
                            row.spawn((Readout::Ready, Text::new(""), UiFonts::style(&fonts.body, 16.0, MUTED), Node { margin: UiRect::left(px(8)), ..default() }));
                        });
                        footer.spawn(Node { column_gap: px(8), ..default() }).with_children(|row| {
                            chip(row, fonts, "Confirm", Control::Confirm);
                            chip(row, fonts, "Cancel", Control::Cancel);
                        });
                    });
            });
        });
}

/// Fills the column with the maker's rolls of the family showing, each a row with its icon, a
/// line about it and its price, when the family changes.
fn fill_rolls(mut commands: Commands, screen: Res<CommissionScreen>, fonts: Res<UiFonts>, mut lists: Query<(Entity, &mut RollList)>, mut icons: Icons) {
    let Ok((list, mut filled)) = lists.single_mut() else { return };
    if !screen.open || filled.0 == screen.base {
        return;
    }
    filled.0 = screen.base;
    commands.entity(list).despawn_children();
    commands.entity(list).with_children(|list| {
        list.spawn((Text::new(format!("{}: the maker's rolls", screen.base)), UiFonts::style(&fonts.display, 20.0, INK)));
        for (index, roll) in screen.rolls.iter().enumerate() {
            list.spawn((
                Control::Roll(index),
                Node { align_items: AlignItems::Center, column_gap: px(10), padding: UiRect::all(px(8)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(6)), flex_shrink: 0.0, ..default() },
                BackgroundColor(CELL),
                BorderColor::all(LINE),
            ))
            .with_children(|row| {
                let size = crate::inventory::icon::icon_size(&roll.icon, false).as_vec2();
                let fit = (ICON_BOX / size.x).min(ICON_BOX / size.y);
                icons.spawn(row, &roll.icon, false, Node { width: px(size.x * fit), height: px(size.y * fit), flex_shrink: 0.0, ..default() }, 24.0);
                row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }).with_children(|text| {
                    text.spawn((Text::new(format!("Roll {}", index + 1)), UiFonts::style(&fonts.display, 18.0, BRIGHT)));
                    text.spawn((Text::new(roll.blurb.clone()), UiFonts::style(&fonts.body, 15.0, MUTED)));
                    text.spawn((Text::new(Coins::from_copper(roll.price_copper).short()), UiFonts::style(&fonts.mono_bold, 16.0, BRIGHT)));
                });
            });
        }
    });
}

/// Keeps the stage image the largest square that fits what the panel leaves.
fn size_stage(frames: Query<&ComputedNode, With<StageFrame>>, mut images: Query<&mut Node, With<StageImage>>) {
    let Ok(frame) = frames.single() else { return };
    let side = (frame.size * frame.inverse_scale_factor).min_element().floor().max(1.0);
    for mut node in &mut images {
        if node.width != px(side) {
            node.width = px(side);
            node.height = px(side);
        }
    }
}

/// Rebuilds the item on its pivot whenever the shape changes, and frames it when it is new.
fn show_item(
    mut commands: Commands,
    screen: Res<CommissionScreen>,
    mut turn: ResMut<Turn>,
    stage: Res<CommissionStage>,
    old: Query<Entity, With<Pivot>>,
    mut cameras: Query<&mut Projection>,
    mut spawner: ItemSpawner,
    mut shown: Local<Option<(u32, f32)>>,
) {
    if !screen.open {
        *shown = None;
        return;
    }
    let Some(shape) = screen.shape.as_ref() else { return };
    let Some(mesh) = screen.mesh.as_ref() else { return };
    // Rebuild only when the shape changed since the last time: the volume changes with every edit
    // that moves the bounds, the structure with a new roll.
    if !screen.is_changed() && !old.is_empty() {
        return;
    }
    for entity in &old {
        commands.entity(entity).despawn();
    }
    if turn.built_for != Some(screen.structure)
        && let Some(bounds) = item_bounds(mesh)
    {
        turn.size = Vec3::from(bounds.max - bounds.min);
        turn.centre = Vec3::from(bounds.center());
        turn.built_for = Some(screen.structure);
        if let Some(camera) = stage.camera
            && let Ok(mut projection) = cameras.get_mut(camera)
        {
            // The longest side, with the turn's diagonal room, fills the view.
            let side = turn.size.max_element().max(turn.size.length() * 0.5) / FILL;
            *projection = Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed { width: side, height: side },
                near: 0.1,
                far: 4000.0,
                ..OrthographicProjection::default_3d()
            });
        }
    }
    *shown = Some((screen.structure, screen.volume));
    let layer = RenderLayers::layer(COMMISSION_LAYER);
    let appearance = ItemAppearance {
        palette: ItemPalette { materials: shape.palette(), ..ItemPalette::ARMORY },
        condition: shape.condition(),
        seed: shape.seed(),
        etching: shape.etching(),
        ..default()
    };
    let pivot = commands
        .spawn((
            Pivot,
            DespawnOnExit(AppState::Delving),
            Transform::from_translation(STAGE).with_rotation(Quat::from_rotation_y(turn.angle)),
            Visibility::default(),
            bevy::app::Propagate(layer),
            bevy::app::Propagate(NotShadowCaster),
        ))
        .id();
    let centre = turn.centre;
    spawner.spawn(mesh, &appearance).insert((Transform::from_translation(-centre), ChildOf(pivot)));
}

/// Keeps the header's names, lit buttons and price showing the current state.
#[allow(clippy::type_complexity)]
fn refresh_header(
    screen: Res<CommissionScreen>,
    mut readouts: Query<(&Readout, &mut Text)>,
    mut chips: Query<(&Control, &mut BackgroundColor)>,
) {
    if !screen.is_changed() {
        return;
    }
    let Some(trade) = screen.trade else { return };
    let price = screen.price_copper();
    for (readout, mut text) in &mut readouts {
        let line = match readout {
            Readout::Maker => format!("{} · {}", screen.maker, screen.faction),
            Readout::Trade => {
                let own = if screen.own_style() { " · their own style, a tier better" } else { "" };
                format!("{}{own}", trade.label())
            }
            Readout::Open => controls_summary(screen.tier()).to_string(),
            Readout::Price => Coins::from_copper(price).short(),
            Readout::Ready => format!("ready in {:.0} h", ready_hours(price)),
        };
        if text.0 != line {
            text.0 = line;
        }
    }
    for (control, mut background) in &mut chips {
        let on = match control {
            Control::Base(base) => *base == screen.base,
            Control::Roll(index) => *index == screen.roll,
            _ => false,
        };
        background.0 = if on { LINE } else { CELL };
    }
}

/// `FB_COMMISSION_SCROLL=<pixels>`: scrolls the panel down that far a moment after it opens, for
/// captures of the lower sections.
fn scroll_for_capture(mut body: Query<&mut ScrollPosition, With<crate::tweak_panel::PanelBody>>, mut frames: Local<u32>) {
    *frames += 1;
    if *frames == 20
        && let Some(y) = std::env::var("FB_COMMISSION_SCROLL").ok().and_then(|v| v.trim().parse::<f32>().ok())
    {
        for mut position in &mut body {
            position.y = y;
        }
    }
}

/// The item turns slowly; dragging across the stage turns it by hand.
#[allow(clippy::too_many_arguments)]
fn turn_item(
    time: Res<Time<Real>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    frames: Query<(&ComputedNode, &UiGlobalTransform), With<StageFrame>>,
    screen: Res<CommissionScreen>,
    mut turn: ResMut<Turn>,
    mut pivots: Query<&mut Transform, With<Pivot>>,
    mut dragging: Local<bool>,
) {
    if !screen.open {
        return;
    }
    if buttons.just_pressed(MouseButton::Left) {
        let cursor = window.and_then(|w| ui_cursor(&w, ui_scale.as_deref()));
        *dragging = cursor.is_some_and(|c| frames.iter().any(|(n, t)| node_rect(n, t).contains(c)));
    }
    if !buttons.pressed(MouseButton::Left) {
        *dragging = false;
    }
    if let Some(angle) = turn.hold {
        turn.angle = angle;
    } else if *dragging {
        turn.angle += motion.delta.x * TURN_PER_PIXEL;
    } else {
        turn.angle += SPIN_SPEED * time.delta_secs();
    }
    turn.angle = turn.angle.rem_euclid(std::f32::consts::TAU);
    for mut transform in &mut pivots {
        transform.rotation = Quat::from_rotation_y(turn.angle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trade(profession: Profession, tier: Tier, style: Option<Style>) -> Trade {
        Trade { profession, tier, style }
    }

    fn sword() -> Shape {
        roll_shape("Sword", 1207).unwrap()
    }

    fn paths(fields: &[Field]) -> Vec<&str> {
        fields.iter().map(|f| f.path.as_str()).collect()
    }

    #[test]
    fn a_novice_has_no_controls_only_rolls() {
        assert!(open_controls(&sword(), Tier::Novice).is_empty());
    }

    #[test]
    fn an_apprentice_gets_materials_and_colours_but_no_sliders() {
        let fields = open_controls(&sword(), Tier::Apprentice);
        assert!(paths(&fields).contains(&"blade.material"));
        assert!(paths(&fields).contains(&"grip.material"));
        assert!(fields.iter().all(|f| matches!(f.kind, FieldKind::Choice { .. } | FieldKind::Colour { .. })), "{:?}", paths(&fields));
        assert!(fields.iter().any(|f| matches!(f.kind, FieldKind::Colour { .. })), "what is wooden or leather can be dyed");
        assert!(!paths(&fields).contains(&"blade.length"));
        assert!(!paths(&fields).contains(&"etch.pattern"));
        // Only what the maker works in is offered.
        let FieldKind::Choice { options, .. } = &fields.iter().find(|f| f.path == "blade.material").unwrap().kind else { panic!() };
        // A blade is made of metal.
        assert!(options.iter().all(|o| ["metal", "accent", "silver", "gold"].contains(&o.as_str())), "{options:?}");
        assert!(options.len() >= 3);
        // A grip is not.
        let FieldKind::Choice { options, .. } = &fields.iter().find(|f| f.path == "grip.material").unwrap().kind else { panic!() };
        assert!(options.contains(&"leather".to_string()) && !options.contains(&"gold".to_string()), "{options:?}");
    }

    #[test]
    fn an_apprentice_gambeson_has_dyes_and_no_body_material() {
        let item = roll_shape("Gambeson", 3).unwrap();
        let fields = open_controls(&item, Tier::Apprentice);
        assert!(!paths(&fields).contains(&"cut.material"));
        let dye = fields.iter().find(|f| f.path == "cut.cloth.color").expect("the cloth's dye");
        assert!(matches!(dye.kind, FieldKind::Colour { .. }));
        assert_eq!((dye.section.as_str(), dye.name.as_str()), ("Body cloth", "colour"));
        // Parts switched off show nothing.
        let mut bare = item.clone();
        bare.set_toggle("belt.on", false).unwrap();
        assert!(!paths(&open_controls(&bare, Tier::Master)).iter().any(|p| p.starts_with("belt.") && *p != "belt.on"));
    }

    #[test]
    fn a_dye_is_set_from_the_panel() {
        let mut screen = CommissionScreen::begin(trade(Profession::Armorer, Tier::Apprentice, None), "Orla", "Hold").unwrap();
        let before = screen.shape().unwrap().colour("cut.cloth.color").unwrap();
        let wanted = [0.55, 0.16, 0.18];
        assert_ne!(before, wanted);
        screen.set_colour("cut.cloth.color", wanted);
        assert_eq!(screen.shape().unwrap().colour("cut.cloth.color"), Some(wanted));
        // A shape the tier does not open takes no dye either.
        let novice = CommissionScreen::begin(trade(Profession::Armorer, Tier::Novice, None), "Orla", "Hold").unwrap();
        let mut novice = novice;
        novice.set_colour("cut.cloth.color", wanted);
        assert_ne!(novice.shape().unwrap().colour("cut.cloth.color"), Some(wanted));
    }

    #[test]
    fn six_rolls_are_listed_with_prices_and_a_line_each() {
        let screen = CommissionScreen::begin(trade(Profession::Weaponsmith, Tier::Novice, None), "Orla Venn", "Hold").unwrap();
        assert_eq!(screen.rolls().len(), ROLLS);
        for (i, roll) in screen.rolls().iter().enumerate() {
            assert_eq!(roll.seed, screen.roll_seed("Sword", i));
            assert!(roll.price_copper > 0 && roll.blurb.contains("cm long"), "{roll:?}");
        }
        let mut prices: Vec<u64> = screen.rolls().iter().map(|r| r.price_copper).collect();
        prices.sort_unstable();
        prices.dedup();
        assert!(prices.len() > 1, "the rolls are not all one price");
    }

    #[test]
    fn an_expert_roll_starts_inside_what_the_expert_reaches() {
        for seed_name in ["Orla", "Hob", "Tam", "Ysolde"] {
            let screen = CommissionScreen::begin(trade(Profession::Weaponsmith, Tier::Expert, None), seed_name, "Hold").unwrap();
            for field in screen.controls() {
                match field.kind {
                    FieldKind::Slider { value, range } => assert!(range.contains(value), "{} {value} outside {range:?}", field.path),
                    FieldKind::Choice { value, options } => assert!(options.contains(&value), "{} {value} not in {options:?}", field.path),
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn an_expert_adds_the_middle_half_of_the_shape_sliders() {
        let item = sword();
        let full = item.fields();
        let fields = open_controls(&item, Tier::Expert);
        let apprentice = open_controls(&item, Tier::Apprentice);
        assert!(fields.len() > apprentice.len());
        assert!(paths(&fields).contains(&"blade.length"));
        assert!(!paths(&fields).contains(&"guard.style"), "no shape choices");
        assert!(!paths(&fields).contains(&"guard.on"), "no parts on or off");
        assert!(!paths(&fields).contains(&"etch.on") && !paths(&fields).contains(&"pommel.gem_stone"));
        let narrow = |path: &str| match fields.iter().find(|f| f.path == path).unwrap().kind {
            FieldKind::Slider { range, .. } => range,
            _ => panic!(),
        };
        let wide = |path: &str| match full.iter().find(|f| f.path == path).unwrap().kind {
            FieldKind::Slider { range, .. } => range,
            _ => panic!(),
        };
        let (n, w) = (narrow("blade.length"), wide("blade.length"));
        assert!((n.max - n.min - (w.max - w.min) / 2.0).abs() <= 2.0 * w.step, "{n:?} of {w:?}");
        assert!((n.min + n.max - w.min - w.max).abs() <= 2.0 * w.step, "centred");
        assert_eq!(n.step, w.step);
    }

    #[test]
    fn a_master_opens_the_full_ranges_and_the_ornament() {
        let item = sword();
        let fields = open_controls(&item, Tier::Master);
        for path in ["blade.length", "guard.span", "pommel.radius", "guard.style", "guard.on", "etch.on", "etch.pattern", "pommel.gem_stone", "guard.gem", "blade.material"] {
            assert!(paths(&fields).contains(&path), "{path}");
        }
        assert!(!paths(&fields).iter().any(|p| p.starts_with("cond.")));
        let full: Vec<_> = visible_fields(&item).into_iter().filter(|f| classify(f).is_some()).collect();
        assert_eq!(fields.len(), full.len());
        for field in &fields {
            if let FieldKind::Slider { range, .. } = field.kind {
                let FieldKind::Slider { range: whole, .. } = full.iter().find(|f| f.path == field.path).unwrap().kind else { panic!() };
                assert_eq!(range, whole, "{}", field.path);
            }
        }
    }

    #[test]
    fn each_tier_opens_at_least_what_the_one_below_does() {
        let item = sword();
        let counts: Vec<usize> = Tier::ALL.iter().map(|&t| open_controls(&item, t).len()).collect();
        assert!(counts.windows(2).all(|w| w[0] < w[1]), "{counts:?}");
    }

    #[test]
    fn the_smiths_own_style_is_a_tier_better() {
        let smith = trade(Profession::Weaponsmith, Tier::Apprentice, Some(Style::Blades));
        assert_eq!(controls_tier(&smith, "Sword"), Tier::Expert);
        assert_eq!(controls_tier(&smith, "Mace"), Tier::Apprentice);
        let master = trade(Profession::Weaponsmith, Tier::Master, Some(Style::Blades));
        assert_eq!(controls_tier(&master, "Sword"), Tier::Master);
        let armorer = trade(Profession::Armorer, Tier::Novice, Some(Style::Plate));
        assert_eq!(controls_tier(&armorer, "Cuirass"), Tier::Apprentice);
        assert_eq!(controls_tier(&armorer, "Gambeson"), Tier::Novice);
        // A bowyer has no style.
        assert_eq!(controls_tier(&trade(Profession::Bowyer, Tier::Expert, None), "Bow"), Tier::Expert);
        // On the screen: a novice bladesmith's sword is an apprentice's panel.
        let screen = CommissionScreen::begin(trade(Profession::Weaponsmith, Tier::Novice, Some(Style::Blades)), "Orla", "Hold").unwrap();
        assert_eq!(screen.base(), "Sword");
        assert!(screen.own_style());
        assert!(paths(&screen.controls()).contains(&"blade.material"));
        let plain = CommissionScreen::begin(trade(Profession::Weaponsmith, Tier::Novice, None), "Orla", "Hold").unwrap();
        assert!(plain.controls().is_empty());
    }

    #[test]
    fn the_trades_offer_their_families() {
        for profession in [Profession::Armorer, Profession::Weaponsmith, Profession::Bowyer, Profession::Tentmaker] {
            for base in bases_of(profession) {
                assert!(roll_shape(base, 1).is_some(), "{base}");
            }
        }
        assert!(bases_of(Profession::Guard).is_empty());
        assert!(CommissionScreen::begin(Trade::guard(), "Gruk", "Mudfoot").is_none());
        assert_eq!(bases_of(Profession::Tentmaker), ["Tent", "Campfire kit"]);
    }

    #[test]
    fn novice_rolls_are_the_same_every_time_and_differ_between_rolls() {
        let make = || CommissionScreen::begin(trade(Profession::Weaponsmith, Tier::Novice, None), "Orla Venn", "the Cinder Hold").unwrap();
        let (a, b) = (make(), make());
        let seeds: Vec<u32> = (0..ROLLS).map(|i| a.roll_seed("Sword", i)).collect();
        assert_eq!(seeds, (0..ROLLS).map(|i| b.roll_seed("Sword", i)).collect::<Vec<_>>());
        let mut distinct = seeds.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), ROLLS);
        let (mut a, b) = (a, b);
        a.set_roll(3);
        let mut b = b;
        b.set_roll(3);
        assert_eq!(a.shape, b.shape);
        a.set_roll(4);
        assert_ne!(a.shape, b.shape);
        // Another maker offers other rolls.
        let other = CommissionScreen::begin(trade(Profession::Weaponsmith, Tier::Novice, None), "Hob Crane", "the Cinder Hold").unwrap();
        assert_ne!(other.roll_seed("Sword", 0), a.roll_seed("Sword", 0));
    }

    #[test]
    fn the_price_follows_the_material_and_the_size_and_the_tier() {
        let mut screen = CommissionScreen::begin(trade(Profession::Weaponsmith, Tier::Apprentice, None), "Orla", "Hold").unwrap();
        let before = screen.price_copper();
        assert!(before > 0);
        screen.set_field("blade.material", "gold").unwrap();
        let gold = screen.price_copper();
        assert!(gold > before, "{before} -> {gold}");
        screen.set_field("blade.material", "silver").unwrap();
        assert!(screen.price_copper() < gold);
        // An apprentice cannot shape, so the field is refused.
        assert!(screen.set_field("blade.length", "1.2").is_err());
        // A master's longer blade costs more than a shorter one.
        let mut master = CommissionScreen::begin(trade(Profession::Weaponsmith, Tier::Master, None), "Orla", "Hold").unwrap();
        let FieldKind::Slider { range, .. } = master.controls().into_iter().find(|f| f.path == "blade.length").unwrap().kind else { panic!() };
        master.set_field("blade.length", &range.min.to_string()).unwrap();
        let short = master.price_copper();
        master.set_field("blade.length", &range.max.to_string()).unwrap();
        assert!(master.price_copper() > short);
        // The same shape from a dearer tier costs more.
        let novice = CommissionScreen::begin(trade(Profession::Weaponsmith, Tier::Novice, None), "Orla", "Hold").unwrap();
        let expert = CommissionScreen::begin(trade(Profession::Weaponsmith, Tier::Expert, None), "Orla", "Hold").unwrap();
        assert!(expert.price_copper() > novice.price_copper());
    }

    #[test]
    fn dragging_stays_inside_what_the_tier_opens() {
        let mut screen = CommissionScreen::begin(trade(Profession::Weaponsmith, Tier::Expert, None), "Orla", "Hold").unwrap();
        let FieldKind::Slider { range, .. } = screen.controls().into_iter().find(|f| f.path == "blade.length").unwrap().kind else { panic!() };
        screen.set_slider("blade.length", range.max + 10.0);
        assert_eq!(screen.slider("blade.length"), Some(range.max));
        screen.set_slider("blade.length", range.min - 10.0);
        assert_eq!(screen.slider("blade.length"), Some(range.min));
        // Shape choices are not the expert's.
        let before = screen.shape.clone();
        screen.cycle("guard.style", 1);
        screen.flip("guard.on");
        assert_eq!(screen.shape, before);
    }

    #[test]
    fn confirming_makes_an_order_that_keeps_the_shaped_item() {
        let mut screen = CommissionScreen::begin(trade(Profession::Weaponsmith, Tier::Master, Some(Style::Blades)), "Orla Venn", "the Cinder Hold").unwrap();
        screen.set_field("etch.on", "true").unwrap();
        let order = screen.order(1000.0).unwrap();
        assert_eq!((order.maker.as_str(), order.faction.as_str(), order.base.as_str()), ("Orla Venn", "the Cinder Hold", "Sword"));
        assert_eq!(order.shape, *screen.shape().unwrap());
        assert_eq!(order.price_copper, screen.price_copper());
        assert!(order.ready_at.seconds() > 1000.0 + 4.0 * 3600.0);
        assert!(order.price().total_copper() > 0);
        assert!(ready_hours(10_000_000) <= 72.0);
    }

    #[test]
    fn a_makers_gambesons_price_alike() {
        // Cloth coats of one size and make cost about the same, whatever their buckles.
        for maker in ["Orla Venn", "Hob Crane", "Tam"] {
            let screen = CommissionScreen::begin(trade(Profession::Armorer, Tier::Apprentice, None), maker, "Hold").unwrap();
            let prices: Vec<u64> = screen.rolls().iter().map(|r| r.price_copper).collect();
            let (low, high) = (*prices.iter().min().unwrap(), *prices.iter().max().unwrap());
            assert!(high < low * 3, "{maker}: {prices:?}");
        }
    }

    #[test]
    fn the_hook_reads_a_trade() {
        assert_eq!(parse_trade("weaponsmith:master:blades").unwrap(), trade(Profession::Weaponsmith, Tier::Master, Some(Style::Blades)));
        assert_eq!(parse_trade("armorer:apprentice").unwrap(), trade(Profession::Armorer, Tier::Apprentice, None));
        assert_eq!(parse_trade("Weaponsmith:expert:axes_and_clubs").unwrap().style, Some(Style::AxesAndClubs));
        assert_eq!(parse_trade("bowyer").unwrap().tier, Tier::Novice);
        assert!(parse_trade("baker:novice").is_err());
        assert!(parse_trade("armorer:grand").is_err());
        assert!(parse_trade("armorer:novice:blades:x").is_ok());
    }

    #[test]
    fn opening_a_trade_that_makes_nothing_changes_nothing() {
        let mut screen = CommissionScreen::default();
        let mut open_screen = ScreenOpen::default();
        assert!(!open(&mut screen, &mut open_screen, None, Trade::guard(), "Gruk", "Mudfoot"));
        assert_eq!(open_screen.0, None);
        assert!(open(&mut screen, &mut open_screen, None, trade(Profession::Bowyer, Tier::Novice, None), "Tam", "Hold"));
        assert!(open_screen.is(Screen::Commission) && screen.is_open());
    }
}
