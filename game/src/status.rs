//! The status badges: a row of small round badges just above the left end of the quickslot bar,
//! one for each condition the delver is under right now ([`fb_character::conditions`]), so a
//! hungry, winded or overladen delver can see it without opening the inventory.
//!
//! - Each badge is a disc with an icon for its kind ([`icon_alpha`], drawn here as a small
//!   distance-field picture), coloured by its [`Severity`] ([`severity_colour`]), worst first.
//! - With nothing wrong the row is empty and takes no room.
//! - Hovering a badge, while the mouse is free, shows its name and what it does above the row
//!   ("Winded: Cannot sprint or swing hard until stamina is back to 20%").
//! - The row is placed from the orb cluster's layout in `vmin` ([`cluster_layout`]), so it scales
//!   with the window, and it is part of the HUD, so it hides under screens.
//!
//! - The character sheet lists the same conditions with the same badges
//!   (`crate::inventory::sheet::update_conditions`, [`badge`], [`ConditionIcons`]), which is
//!   where the player reads them while the mouse is locked.
//!
//! `FB_CONDITIONS` holds the delver under conditions for captures, and `FB_CONDITION_HOVER`
//! shows one badge's card ([`DebugConditions`]).

use bevy::{
    asset::RenderAssetUsages,
    platform::collections::HashMap,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    text::FontSize,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use fb_character::LoadBand;
use fb_character::conditions::{Circumstances, Condition, ConditionKind, Severity, conditions};

use crate::character::{Character, DelverEffects, DelverNeeds, DelverStamina, Load, modifier};
use crate::combat::orbs::{OrbCluster, cluster_layout};
use crate::combat::Health;
use crate::combat::vitals::Constricted;
use crate::exhaustion::Exhaustion;
use crate::play::{DelveSystems, Player};
use crate::player::Slowed;
use crate::ui::{BG, INK, LINE, UiFonts};

pub struct StatusPlugin;

impl Plugin for StatusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShownConditions>().init_resource::<ConditionIcons>().insert_resource(DebugConditions::from_env()).add_systems(
            Update,
            (spawn_row, hold_debug_conditions, read_conditions, draw_badges, show_card).chain().in_set(DelveSystems),
        );
    }
}

/// How many pixels across a badge's icon is drawn.
pub const ICON_PX: u32 = 64;

/// The colour of a badge of `severity`: a calm cream for a nudge, amber, orange, and red for a
/// danger to the run; a soft green for one doing good. Our own.
pub fn severity_colour(severity: Severity) -> Color {
    match severity {
        Severity::Good => Color::srgb(0.46, 0.74, 0.42),
        Severity::Mild => Color::srgb(0.86, 0.80, 0.66),
        Severity::Moderate => Color::srgb(0.88, 0.66, 0.26),
        Severity::Serious => Color::srgb(0.92, 0.45, 0.20),
        Severity::Grave => Color::srgb(0.90, 0.20, 0.17),
    }
}

/// The conditions the badges show now. Rewritten only when they change.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct ShownConditions(pub Vec<Condition>);

/// The row of badges in the orb cluster.
#[derive(Component, Debug)]
pub struct StatusRow;

/// A badge, showing the `n`th of the [`ShownConditions`].
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusBadge(pub usize);

/// The card over the row that names the hovered badge and says what it does.
#[derive(Component, Debug)]
pub struct StatusCard;

/// The conditions the delver is under, from their simulation.
#[allow(clippy::type_complexity)]
fn read_conditions(
    character: Option<Res<Character>>,
    tuning: Option<Res<crate::tuning::Tuning>>,
    rules: Option<Res<crate::rules::GameRules>>,
    players: Query<(&DelverNeeds, &Exhaustion, &DelverStamina, &Load, Has<Constricted>, Option<&Slowed>, Option<&DelverEffects>, Option<&Health>), With<Player>>,
    mut shown: ResMut<ShownConditions>,
) {
    let Some((needs, exhaustion, stamina, load, held, slowed, effects, health)) = players.iter().next() else { return };
    let mending = tuning.as_deref().map_or(fb_character::recovery::MINUTES_PER_HIT_POINT, |t| t.recovery.minutes_per_hit_point as f64);
    let now = Circumstances {
        needs: needs.0,
        exhaustion: exhaustion.0,
        stamina: stamina.0,
        load: load.0,
        con_modifier: character.map_or(0, |c| modifier(c.constitution())),
        held_pace: held.then(|| slowed.map_or(1.0, |s| s.0 as f64)),
        wounded_mending: health.filter(|h| h.hp < h.max && !h.is_down()).map(|_| mending),
    };
    let none = fb_character::effect::Effects::default();
    let list = conditions(&now, effects.map_or(&none, |e| &e.0), &crate::rules::GameRules::of(&rules).effects);
    if shown.0 != list {
        shown.0 = list;
    }
}

/// Gives each new orb cluster its row of badges and the card above it, placed from the layout.
fn spawn_row(mut commands: Commands, clusters: Query<Entity, Added<OrbCluster>>, fonts: Option<Res<UiFonts>>) {
    let layout = cluster_layout();
    let font = fonts.map_or_else(TextFont::default, |f| TextFont::default().with_font(f.body.clone()));
    for cluster in &clusters {
        commands.entity(cluster).with_children(|cluster| {
            cluster
                .spawn((
                    Name::new("Status badges"),
                    StatusRow,
                    Node {
                        position_type: PositionType::Absolute,
                        left: vmin(layout.status.min.x),
                        top: vmin(layout.status.min.y),
                        height: vmin(layout.badge),
                        flex_direction: FlexDirection::Row,
                        column_gap: vmin(layout.gap),
                        display: Display::None,
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_child((
                    Name::new("Status card"),
                    StatusCard,
                    Node {
                        position_type: PositionType::Absolute,
                        bottom: vmin(layout.badge + layout.gap),
                        left: vmin(0.0),
                        padding: UiRect::axes(vmin(0.9), vmin(0.5)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(3)),
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(BG.with_alpha(0.92)),
                    BorderColor::all(LINE),
                    Text::default(),
                    font.clone().with_font_size(FontSize::VMin(1.9)),
                    TextColor(INK),
                    TextLayout::no_wrap(),
                    Pickable::IGNORE,
                    GlobalZIndex(5),
                ));
        });
    }
}

/// Redraws the badges when the conditions change: one per condition, worst first, and the row
/// hidden when there are none.
fn draw_badges(
    mut commands: Commands,
    shown: Res<ShownConditions>,
    mut rows: Query<(Entity, &mut Node, Ref<StatusRow>), Without<StatusBadge>>,
    badges: Query<Entity, With<StatusBadge>>,
    mut images: Option<ResMut<Assets<Image>>>,
    mut icons: ResMut<ConditionIcons>,
) {
    if !shown.is_changed() && !rows.iter().any(|(_, _, row)| row.is_added()) {
        return;
    }
    for badge in &badges {
        commands.entity(badge).despawn();
    }
    let layout = cluster_layout();
    for (row, mut node, _) in &mut rows {
        let display = if shown.0.is_empty() { Display::None } else { Display::Flex };
        if node.display != display {
            node.display = display;
        }
        commands.entity(row).with_children(|row| {
            for (n, condition) in shown.0.iter().enumerate() {
                let icon = icons.get(condition.kind, images.as_deref_mut());
                badge(row, condition, vmin(layout.badge), vmin(layout.badge * 0.07), icon).insert(StatusBadge(n));
            }
        });
    }
}

/// The badge icons, drawn once a kind and shared by the HUD row and the character sheet.
#[derive(Resource, Debug, Default)]
pub struct ConditionIcons(HashMap<ConditionKind, Handle<Image>>);

impl ConditionIcons {
    /// The icon for `kind`, drawn into `images` the first time it is asked for. None without
    /// images (the logic tests).
    pub fn get(&mut self, kind: ConditionKind, images: Option<&mut Assets<Image>>) -> Option<Handle<Image>> {
        if let Some(icon) = self.0.get(&kind) {
            return Some(icon.clone());
        }
        let icon = images?.add(icon_image(kind));
        self.0.insert(kind, icon.clone());
        Some(icon)
    }
}

/// Spawns one condition's badge under `parent`: a disc `size` across with a `rim` in its
/// severity's colour and its icon (if drawn) tinted to match.
pub fn badge<'a>(parent: &'a mut ChildSpawnerCommands, condition: &Condition, size: Val, rim: Val, icon: Option<Handle<Image>>) -> EntityCommands<'a> {
    let colour = severity_colour(condition.severity);
    let mut badge = parent.spawn((
        Name::new(format!("{} badge", condition.name)),
        Node {
            width: size,
            height: size,
            flex_shrink: 0.0,
            border: UiRect::all(rim),
            border_radius: BorderRadius::MAX,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(colour.darker(0.55).with_alpha(0.92)),
        BorderColor::all(colour),
        Pickable::IGNORE,
    ));
    if let Some(icon) = icon {
        badge.with_child((ImageNode { image: icon, color: colour.lighter(0.12), ..default() }, Node { width: percent(74), height: percent(74), ..default() }, Pickable::IGNORE));
    }
    badge
}

/// The badge under the mouse, while the mouse is free (or the one `FB_CONDITION_HOVER` names),
/// and its card: the condition's name and what it does, over the badge.
#[allow(clippy::type_complexity)]
fn show_card(
    debug: Res<DebugConditions>,
    shown: Res<ShownConditions>,
    windows: Query<(&Window, &CursorOptions), With<PrimaryWindow>>,
    badges: Query<(&StatusBadge, &ComputedNode, &UiGlobalTransform, &InheritedVisibility)>,
    mut cards: Query<(&mut Node, &mut Text, &mut TextColor), With<StatusCard>>,
) {
    let cursor = windows.iter().next().filter(|(_, options)| options.grab_mode == CursorGrabMode::None).and_then(|(window, _)| window.physical_cursor_position());
    let over = debug.hover.or_else(|| {
        let point = cursor?;
        badges.iter().find(|(_, node, transform, visible)| visible.get() && node.contains_point(**transform, point)).map(|(badge, ..)| badge.0)
    });
    let layout = cluster_layout();
    let condition = over.and_then(|n| shown.0.get(n).map(|c| (n, c)));
    for (mut node, mut text, mut colour) in &mut cards {
        let Some((n, condition)) = condition else {
            if node.display != Display::None {
                node.display = Display::None;
            }
            continue;
        };
        let line = format!("{}: {}", condition.name, condition.effect);
        if text.0 != line {
            text.0 = line;
        }
        colour.set_if_neq(TextColor(severity_colour(condition.severity).lighter(0.2)));
        let left = vmin(n as f32 * (layout.badge + layout.gap));
        if node.display != Display::Flex || node.left != left {
            node.display = Display::Flex;
            node.left = left;
        }
    }
}

/// Conditions to hold the delver under, for captures: `FB_CONDITIONS` lists them, comma
/// separated, as `hunger:<0 to 1>`, `thirst:<0 to 1>`, `sleep:<0 to 1>`, `exhaustion:<level>`,
/// `winded`, `load:<light|burdened|encumbered|overloaded>` and `poisoned` (for an hour) or
/// `poisoned:<minutes>`; `FB_CONDITION_HOVER=<n>` shows the
/// card of the `n`th badge.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct DebugConditions {
    pub hunger: Option<f64>,
    pub thirst: Option<f64>,
    pub sleepiness: Option<f64>,
    pub exhaustion: Option<u32>,
    pub winded: bool,
    pub load: Option<LoadBand>,
    /// Game hours of poison to hold the delver at.
    pub poisoned: Option<f64>,
    pub hover: Option<usize>,
}

impl DebugConditions {
    /// Reads `FB_CONDITIONS` and `FB_CONDITION_HOVER`.
    pub fn from_env() -> DebugConditions {
        let mut debug = std::env::var("FB_CONDITIONS").map(|list| DebugConditions::parse(&list)).unwrap_or_default();
        debug.hover = std::env::var("FB_CONDITION_HOVER").ok().and_then(|n| n.trim().parse().ok());
        debug
    }

    /// The conditions in a list such as `hunger:0.8,winded,load:encumbered`. Unknown entries
    /// are left out.
    pub fn parse(list: &str) -> DebugConditions {
        let mut debug = DebugConditions::default();
        for entry in list.split(',').map(str::trim) {
            let (name, value) = entry.split_once(':').unwrap_or((entry, ""));
            let share = value.trim().parse::<f64>().ok().map(|v| v.clamp(0.0, 1.0));
            match name.trim().to_ascii_lowercase().as_str() {
                "hunger" => debug.hunger = share,
                "thirst" => debug.thirst = share,
                "sleep" | "sleepiness" => debug.sleepiness = share,
                "exhaustion" => debug.exhaustion = value.trim().parse().ok(),
                "winded" => debug.winded = true,
                "poisoned" | "poison" => debug.poisoned = Some(value.trim().parse::<f64>().map_or(fb_character::poison::VENOM_POISONED_HOURS, |minutes| minutes.max(0.0) / 60.0)),
                "load" => {
                    debug.load = match value.trim().to_ascii_lowercase().as_str() {
                        "light" => Some(LoadBand::Light),
                        "burdened" => Some(LoadBand::Burdened),
                        "encumbered" => Some(LoadBand::Encumbered),
                        "overloaded" => Some(LoadBand::Overloaded),
                        _ => None,
                    }
                }
                _ => {}
            }
        }
        debug
    }

    fn holds_anything(&self) -> bool {
        self.hunger.is_some() || self.thirst.is_some() || self.sleepiness.is_some() || self.exhaustion.is_some() || self.winded || self.load.is_some() || self.poisoned.is_some()
    }
}

/// Holds the delver under the [`DebugConditions`] every frame, so the simulation cannot clear
/// them before a capture.
#[allow(clippy::type_complexity)]
fn hold_debug_conditions(
    mut commands: Commands,
    debug: Res<DebugConditions>,
    mut players: Query<(Entity, &mut DelverNeeds, &mut Exhaustion, &mut DelverStamina, &mut Load, Option<&DelverEffects>), With<Player>>,
) {
    if !debug.holds_anything() {
        return;
    }
    for (player, mut needs, mut exhaustion, mut stamina, mut load, poison) in &mut players {
        if let Some(hours) = debug.poisoned {
            let held = DelverEffects::poisoned_for(fb_character::rules::defaults(), hours);
            if poison != Some(&held) {
                commands.entity(player).insert(held);
            }
        }
        let mut held = needs.0;
        for (value, meter) in [(debug.hunger, &mut held.hunger), (debug.thirst, &mut held.thirst), (debug.sleepiness, &mut held.sleepiness)] {
            if let Some(value) = value {
                meter.value = value;
            }
        }
        needs.set_if_neq(DelverNeeds(held));
        if let Some(level) = debug.exhaustion {
            exhaustion.set_if_neq(Exhaustion(fb_character::Exhaustion::new(level)));
        }
        if debug.winded {
            stamina.set_if_neq(DelverStamina(fb_character::Stamina { value: 0.05, winded: true }));
        }
        if let Some(band) = debug.load {
            load.set_if_neq(Load(band));
        }
    }
}

/// A badge's icon as an image: white, with the picture in its alpha ([`icon_alpha`]), for the
/// badge to tint.
fn icon_image(kind: ConditionKind) -> Image {
    let alpha = icon_alpha(kind, ICON_PX);
    let rgba = alpha.iter().flat_map(|&a| [255, 255, 255, a]).collect();
    Image::new(
        Extent3d { width: ICON_PX, height: ICON_PX, depth_or_array_layers: 1 },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Distance to a circle of radius `r` about `c`: negative inside.
fn circle(p: Vec2, c: Vec2, r: f32) -> f32 {
    p.distance(c) - r
}

/// Distance to a ring of radius `r` about `c`, `width` across.
fn ring(p: Vec2, c: Vec2, r: f32, width: f32) -> f32 {
    (p.distance(c) - r).abs() - width * 0.5
}

/// Distance to the segment from `a` to `b`, thickened by `r`.
fn capsule(p: Vec2, a: Vec2, b: Vec2, r: f32) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    p.distance(a + ab * t) - r
}

/// Distance to a convex polygon with its corners in clockwise order on screen (y down): exact
/// inside and near the edges, which is all an antialiased edge needs.
fn polygon(p: Vec2, corners: &[Vec2]) -> f32 {
    let mut d = f32::MIN;
    for (i, &a) in corners.iter().enumerate() {
        let b = corners[(i + 1) % corners.len()];
        let edge = (b - a).normalize();
        // Outward normal, for clockwise corners with y down.
        let normal = Vec2::new(edge.y, -edge.x);
        d = d.max((p - a).dot(normal));
    }
    d
}

/// The signed distance from `p` (from -1 to 1 across, y down) to the icon for `kind`. The
/// shapes are our own: a loaf, a drop, a crescent moon, an hourglass, gusts of wind, a weight,
/// two links of a chain, a bubbling flask and a cross for mending.
pub fn icon_distance(kind: ConditionKind, p: Vec2) -> f32 {
    let v = Vec2::new;
    match kind {
        ConditionKind::Hunger => {
            let dome = circle(p, v(0.0, 0.25), 0.78).max(p.y - 0.5);
            let scores = [-0.38, 0.0, 0.38].into_iter().map(|x| capsule(p, v(x - 0.1, -0.25), v(x + 0.1, 0.1), 0.07)).fold(f32::MAX, f32::min);
            dome.max(-scores)
        }
        ConditionKind::Thirst => {
            let (centre, r, tip) = (v(0.0, 0.25), 0.5, v(0.0, -0.85));
            let angle = (r / centre.distance(tip)).asin();
            let (s, c) = angle.sin_cos();
            let cone = ((p - tip).dot(v(c, -s))).max((p - tip).dot(v(-c, -s))).max(p.y - centre.y);
            circle(p, centre, r).min(cone)
        }
        ConditionKind::Sleepiness => circle(p, v(0.0, 0.0), 0.75).max(-circle(p, v(0.38, -0.28), 0.62)),
        ConditionKind::Exhaustion => {
            let top = polygon(p, &[v(-0.5, -0.68), v(0.5, -0.68), v(0.0, 0.02)]);
            let bottom = polygon(p, &[v(0.0, -0.02), v(0.5, 0.68), v(-0.5, 0.68)]);
            let caps = capsule(p, v(-0.6, -0.8), v(0.6, -0.8), 0.08).min(capsule(p, v(-0.6, 0.8), v(0.6, 0.8), 0.08));
            top.min(bottom).min(caps)
        }
        ConditionKind::Winded => {
            let lines = capsule(p, v(-0.8, -0.42), v(0.35, -0.42), 0.09).min(capsule(p, v(-0.55, 0.0), v(0.55, 0.0), 0.09)).min(capsule(p, v(-0.8, 0.42), v(0.15, 0.42), 0.09));
            let curl = ring(p, v(0.35, -0.62), 0.2, 0.17).max(-(p.x - 0.35));
            let curl_low = ring(p, v(0.55, -0.2), 0.2, 0.17).max(-(p.x - 0.55));
            lines.min(curl).min(curl_low)
        }
        ConditionKind::Load => {
            let body = polygon(p, &[v(-0.42, -0.2), v(0.42, -0.2), v(0.72, 0.78), v(-0.72, 0.78)]);
            body.min(ring(p, v(0.0, -0.48), 0.3, 0.17))
        }
        ConditionKind::Held => ring(p, v(-0.32, 0.0), 0.42, 0.2).min(ring(p, v(0.32, 0.0), 0.42, 0.2)),
        ConditionKind::Poisoned => {
            let flask = circle(p, v(0.0, 0.28), 0.52).min(capsule(p, v(0.0, -0.2), v(0.0, -0.6), 0.17)).min(capsule(p, v(-0.25, -0.7), v(0.25, -0.7), 0.09));
            let bubbles = circle(p, v(-0.16, 0.36), 0.13).min(circle(p, v(0.2, 0.16), 0.09)).min(circle(p, v(0.08, 0.5), 0.07));
            flask.max(-bubbles)
        }
        ConditionKind::Recovering => capsule(p, v(0.0, -0.62), v(0.0, 0.62), 0.2).min(capsule(p, v(-0.62, 0.0), v(0.62, 0.0), 0.2)),
        // Any other effect: a diamond with an open eye in it, until it has an icon of its own.
        ConditionKind::Effect => polygon(p, &[v(0.0, -0.72), v(0.62, 0.0), v(0.0, 0.72), v(-0.62, 0.0)]).max(-circle(p, v(0.0, 0.0), 0.2)),
    }
}

/// The icon for `kind`, `size` pixels square, as one alpha byte a pixel, rows from the top: the
/// shape of [`icon_distance`] with its edge antialiased over a pixel.
pub fn icon_alpha(kind: ConditionKind, size: u32) -> Vec<u8> {
    let pixel = 2.0 / size as f32;
    (0..size * size)
        .map(|i| {
            let (x, y) = (i % size, i / size);
            let p = Vec2::new((x as f32 + 0.5) * pixel - 1.0, (y as f32 + 0.5) * pixel - 1.0);
            let cover = (0.5 - icon_distance(kind, p) / pixel).clamp(0.0, 1.0);
            (cover * 255.0).round() as u8
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use bevy::state::app::StatesPlugin;

    use super::*;
    use crate::launch::AppState;

    const KINDS: [ConditionKind; 10] = [
        ConditionKind::Hunger,
        ConditionKind::Thirst,
        ConditionKind::Sleepiness,
        ConditionKind::Exhaustion,
        ConditionKind::Winded,
        ConditionKind::Load,
        ConditionKind::Held,
        ConditionKind::Poisoned,
        ConditionKind::Recovering,
        ConditionKind::Effect,
    ];

    #[test]
    fn every_icon_is_a_shape_inside_its_square() {
        for kind in KINDS {
            let alpha = icon_alpha(kind, 32);
            let filled = alpha.iter().filter(|&&a| a > 127).count();
            assert!(filled > 32 * 32 / 12 && filled < 32 * 32 * 2 / 3, "{kind:?}: {filled} of 1024 filled");
            let edge = (0..32).flat_map(|i| [alpha[i], alpha[31 * 32 + i], alpha[i * 32], alpha[i * 32 + 31]]);
            assert!(edge.into_iter().all(|a| a == 0), "{kind:?} keeps off the badge's rim");
        }
    }

    #[test]
    fn worse_is_redder() {
        let red_share = |s| {
            let c = severity_colour(s).to_srgba();
            c.red - c.green
        };
        assert!(red_share(Severity::Good) < red_share(Severity::Mild));
        assert!(red_share(Severity::Mild) < red_share(Severity::Moderate));
        assert!(red_share(Severity::Moderate) < red_share(Severity::Serious));
        assert!(red_share(Severity::Serious) < red_share(Severity::Grave));
    }

    #[test]
    fn the_debug_list_reads_each_condition() {
        let debug = DebugConditions::parse("hunger:0.8, winded,load:Encumbered,exhaustion:2,thirst:3,poisoned:30,bogus");
        assert_eq!(
            debug,
            DebugConditions { hunger: Some(0.8), thirst: Some(1.0), exhaustion: Some(2), winded: true, load: Some(LoadBand::Encumbered), poisoned: Some(0.5), ..DebugConditions::default() }
        );
        assert_eq!(DebugConditions::parse("poisoned").poisoned, Some(1.0), "an hour, as venom gives");
        assert!(!DebugConditions::parse("").holds_anything());
    }

    fn delve() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin)).insert_state(AppState::Delving).init_resource::<Character>().add_plugins(StatusPlugin);
        app.insert_resource(DebugConditions::default());
        crate::play::run_delve_systems_while_delving(&mut app);
        let player = app.world_mut().spawn((Player, DelverNeeds::default(), Exhaustion::default(), DelverStamina::default(), Load::default())).id();
        app.world_mut().spawn((OrbCluster, Node::default()));
        app.update();
        (app, player)
    }

    fn badges(app: &mut App) -> Vec<String> {
        let mut shown = app.world_mut().query::<(&StatusBadge, &Name)>();
        let mut names: Vec<(usize, String)> = shown.iter(app.world()).map(|(b, n)| (b.0, n.to_string())).collect();
        names.sort();
        names.into_iter().map(|(_, n)| n).collect()
    }

    fn row_display(app: &mut App) -> Display {
        let mut rows = app.world_mut().query_filtered::<&Node, With<StatusRow>>();
        rows.single(app.world()).unwrap().display
    }

    #[test]
    fn a_fresh_delver_has_an_empty_row_that_takes_no_room() {
        let (mut app, _) = delve();
        assert_eq!(badges(&mut app), Vec::<String>::new());
        assert_eq!(row_display(&mut app), Display::None);
    }

    #[test]
    fn venom_shows_a_poisoned_badge_with_the_minutes_left_until_it_wears_off() {
        let (mut app, player) = delve();
        app.world_mut().entity_mut(player).insert(DelverEffects::poisoned_for(fb_character::rules::defaults(), 0.5));
        app.update();
        app.update();
        assert_eq!(badges(&mut app), vec!["Poisoned badge".to_string()]);
        assert_eq!(app.world().resource::<ShownConditions>().0[0].effect, "Attacking at disadvantage; wears off in 30 minutes");
        app.world_mut().entity_mut(player).remove::<DelverEffects>();
        app.update();
        app.update();
        assert_eq!(badges(&mut app), Vec::<String>::new());
    }

    #[test]
    fn badges_come_and_go_with_the_conditions() {
        let (mut app, player) = delve();
        app.world_mut().get_mut::<DelverStamina>(player).unwrap().0.spend(1.0);
        app.world_mut().get_mut::<DelverNeeds>(player).unwrap().0.hunger.value = 0.8;
        app.update();
        app.update();
        assert_eq!(badges(&mut app), vec!["Winded badge".to_string(), "Famished badge".to_string()]);
        assert_eq!(row_display(&mut app), Display::Flex);
        app.world_mut().get_mut::<DelverStamina>(player).unwrap().0 = fb_character::Stamina::default();
        app.update();
        app.update();
        assert_eq!(badges(&mut app), vec!["Famished badge".to_string()], "winded clears as stamina returns");
        app.world_mut().insert_resource(DebugConditions { hover: Some(0), ..DebugConditions::default() });
        app.update();
        let mut cards = app.world_mut().query_filtered::<(&Node, &Text), With<StatusCard>>();
        let (node, text) = cards.single(app.world()).unwrap();
        assert_eq!((node.display, text.0.as_str()), (Display::Flex, "Famished: Sleep no longer eases exhaustion"));
    }
}
