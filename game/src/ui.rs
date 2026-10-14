//! The dark ledger's look, shared by the HUD and the inventory screen: a near-black warm page,
//! tan rules, cream titles, muted tan text and a faint gold seal (`docs/rendering.md`,
//! "Palette and UI"). The token names are the plate's (`--bg`, `--cell`, `--line`, `--ink`
//! ...), which its own dark theme keeps; the parchment values, the plate's light theme, are
//! listed in the doc.
//!
//! The fonts are the plate's, from Google Fonts under the SIL Open Font License 1.1, in
//! `game/assets/fonts` with their licences (`game/assets/fonts/README.md`).

use bevy::prelude::*;
use fb_items::Rarity;

const fn hex(rgb: u32) -> Color {
    Color::srgb(((rgb >> 16) & 0xff) as f32 / 255.0, ((rgb >> 8) & 0xff) as f32 / 255.0, (rgb & 0xff) as f32 / 255.0)
}

/// The page: the whole screen, and the hover card (`--bg`).
pub const BG: Color = hex(0x141210);
/// An empty grid cell, a shade above the page (`--cell`).
pub const CELL: Color = hex(0x1c1915);
/// Grid lines, grid outlines and borders, warm tan (`--line`).
pub const LINE: Color = hex(0x5e5040);
/// Titles, in the display face: cream (`--ink`, which the plate's dark theme turns light).
pub const INK: Color = hex(0xe6dcc6);
/// The brightest cream: a filled slot's outline and the values among numbers.
pub const BRIGHT: Color = hex(0xece4d2);
/// Body and help text, muted tan (`--muted`).
pub const MUTED: Color = hex(0xa89a82);
/// Dividers that should barely show (`--faint`).
pub const FAINT: Color = hex(0x332d25);
/// The toast's refusals (the plate's dark `--accent`).
pub const ACCENT: Color = hex(0xc48a52);
/// The seal's lines and runes, drawn at 40 to 60% over the page.
pub const GOLD: Color = hex(0x8a6d3b);
/// The warm glow a carried light casts on its side of the doll.
pub const GLOW: Color = hex(0xd9913f);
/// The portrait's Stygian blue (`--stygian`, `--stygian-2` in the plate's dark theme).
pub const STYGIAN: Color = hex(0x22325c);
pub const STYGIAN_DEEP: Color = hex(0x0b1129);
/// The light in the middle of the portrait's blue (the plate's `.portrait` gradient, `#3a55a0`).
pub const STYGIAN_LIGHT: Color = hex(0x3a55a0);
/// The portrait's frame: the plate's dark-theme `--gold`, brighter than the seal's [`GOLD`].
pub const FRAME_GOLD: Color = hex(0xe3b76a);
/// A drop that fits, and one that does not (the plate's dark `--ok`, `--bad`).
pub const OK: Color = hex(0x86ba80);
pub const BAD: Color = hex(0xea735b);

/// The colour an item's name is written in: the plate's dark-theme rarity colours
/// (`--r-common` to `--r-legend`), its four hues lightened to read on the dark page.
pub fn rarity_color(rarity: Rarity) -> Color {
    match rarity {
        Rarity::Common => hex(0xddd4c1),
        Rarity::Magic => hex(0x8eaeff),
        Rarity::Rare => hex(0xebcc5c),
        Rarity::Legendary => hex(0xf29550),
    }
}

/// The contrast ratio of two colours, from 1 to 21: `(L1 + 0.05) / (L2 + 0.05)` of their
/// relative luminances, lighter over darker, as WCAG 2.1 defines it
/// (<https://www.w3.org/TR/WCAG21/#dfn-contrast-ratio>). Bevy's linear luminance is WCAG's
/// relative luminance: the same sRGB transfer function and Rec. 709 weights
/// (<https://www.w3.org/TR/WCAG21/#dfn-relative-luminance>).
pub fn contrast_ratio(a: Color, b: Color) -> f32 {
    let (la, lb) = (a.to_linear().luminance(), b.to_linear().luminance());
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The plate's four faces, and Plex Mono's bold for values.
#[derive(Resource, Clone, Debug)]
pub struct UiFonts {
    /// Almendra, for names and headings.
    pub display: Handle<Font>,
    /// Alegreya Sans, for running text.
    pub body: Handle<Font>,
    pub body_italic: Handle<Font>,
    pub body_medium: Handle<Font>,
    /// IBM Plex Mono, for numbers.
    pub mono: Handle<Font>,
    /// IBM Plex Mono Bold, for the values among them.
    pub mono_bold: Handle<Font>,
    /// Noto Sans Runic, for the seal.
    pub rune: Handle<Font>,
}

impl FromWorld for UiFonts {
    /// Loads the fonts, or leaves Bevy's default font everywhere when there is no asset server
    /// (in tests).
    fn from_world(world: &mut World) -> Self {
        let load = |path: &'static str| world.get_resource::<AssetServer>().map_or_else(Handle::default, |assets| assets.load(path));
        UiFonts {
            display: load("fonts/Almendra-Regular.ttf"),
            body: load("fonts/AlegreyaSans-Regular.ttf"),
            body_italic: load("fonts/AlegreyaSans-Italic.ttf"),
            body_medium: load("fonts/AlegreyaSans-Medium.ttf"),
            mono: load("fonts/IBMPlexMono-Regular.ttf"),
            mono_bold: load("fonts/IBMPlexMono-Bold.ttf"),
            rune: load("fonts/NotoSansRunic-Regular.ttf"),
        }
    }
}

impl UiFonts {
    /// A text style in `font` at `size` logical pixels.
    pub fn style(font: &Handle<Font>, size: f32, color: Color) -> (TextFont, TextColor) {
        (TextFont::from_font_size(size).with_font(font.clone()), TextColor(color))
    }
}

/// A node's rectangle in logical pixels.
pub fn node_rect(node: &ComputedNode, transform: &UiGlobalTransform) -> Rect {
    Rect::from_center_size(transform.translation * node.inverse_scale_factor, node.size * node.inverse_scale_factor)
}

/// `text` cut into runs of words, each marked whether it is a value: a word with a digit in
/// it ("3", "1d8+1", "2×3", "×0.85"). Spaces stay with the word before them.
pub fn value_runs(text: &str) -> Vec<(String, bool)> {
    let mut runs: Vec<(String, bool)> = Vec::new();
    for word in text.split_inclusive(' ') {
        let value = word.chars().any(|c| c.is_ascii_digit());
        match runs.last_mut() {
            Some((run, was)) if *was == value => run.push_str(word),
            _ => runs.push((word.to_string(), value)),
        }
    }
    runs
}

/// A line of numbers in Plex Mono at `size`: the values bright and bold, the words between
/// them muted.
pub fn numbers_line(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, text: &str, size: f32) {
    parent.spawn((Text::default(), UiFonts::style(&fonts.mono, size, MUTED))).with_children(|line| {
        for (run, value) in value_runs(text) {
            let (font, colour) = if value { (&fonts.mono_bold, BRIGHT) } else { (&fonts.mono, MUTED) };
            line.spawn((TextSpan::new(run), UiFonts::style(font, size, colour)));
        }
    });
}

/// Where the dashes go along a side `length` long: `(start, length)` of each, `dash` long with
/// `gap` between, the run centred so both ends of the side match.
pub fn dash_runs(length: f32, dash: f32, gap: f32) -> Vec<(f32, f32)> {
    if length <= 0.0 || dash <= 0.0 {
        return Vec::new();
    }
    let count = ((length + gap) / (dash + gap)).floor().max(1.0) as usize;
    let dash = dash.min(length);
    let used = count as f32 * dash + (count - 1) as f32 * gap;
    let start = (length - used) * 0.5;
    (0..count).map(|i| (start + i as f32 * (dash + gap), dash)).collect()
}

/// A dashed outline round a `size` box, spawned as absolutely placed children of `parent`
/// (which must be `size`): dashes along the straight sides and a solid quarter arc at each
/// corner of `radius`. Bevy's UI draws only solid borders.
pub fn dashed_outline(parent: &mut ChildSpawnerCommands, size: Vec2, radius: f32, width: f32, dash: f32, gap: f32, colour: Color) {
    let r = radius.min(size.x * 0.5).min(size.y * 0.5);
    let edge = |on: bool, value: Val| if on { value } else { Val::Auto };
    let thick = |on: bool| if on { px(width) } else { px(0) };
    for (left, top) in [(true, true), (false, true), (true, false), (false, false)] {
        let round = |at: bool| if at { CornerRadius::from(px(r)) } else { CornerRadius::default() };
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: edge(left, px(0)),
                right: edge(!left, px(0)),
                top: edge(top, px(0)),
                bottom: edge(!top, px(0)),
                width: px(r),
                height: px(r),
                border: UiRect { left: thick(left), right: thick(!left), top: thick(top), bottom: thick(!top) },
                border_radius: BorderRadius {
                    top_left: round(left && top),
                    top_right: round(!left && top),
                    bottom_left: round(left && !top),
                    bottom_right: round(!left && !top),
                },
                ..default()
            },
            BorderColor::all(colour),
        ));
    }
    let mut dash_node = |left: f32, top: f32, w: f32, h: f32| {
        parent.spawn((
            Node { position_type: PositionType::Absolute, left: px(left), top: px(top), width: px(w), height: px(h), ..default() },
            BackgroundColor(colour),
        ));
    };
    for (start, length) in dash_runs(size.x - 2.0 * r, dash, gap) {
        dash_node(r + start, 0.0, length, width);
        dash_node(r + start, size.y - width, length, width);
    }
    for (start, length) in dash_runs(size.y - 2.0 * r, dash, gap) {
        dash_node(0.0, r + start, width, length);
        dash_node(size.x - width, r + start, width, length);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_is_the_dark_ledger() {
        assert_eq!(BG.to_srgba().to_hex(), "#141210");
        assert_eq!(CELL.to_srgba().to_hex(), "#1C1915");
        assert_eq!(LINE.to_srgba().to_hex(), "#5E5040");
        assert_eq!(INK.to_srgba().to_hex(), "#E6DCC6");
        assert_eq!(MUTED.to_srgba().to_hex(), "#A89A82");
        assert_eq!(rarity_color(Rarity::Legendary).to_srgba().to_hex(), "#F29550");
    }

    /// WCAG 2.1 AA, success criterion 1.4.3: at least 4.5:1 for body text
    /// (<https://www.w3.org/TR/WCAG21/#contrast-minimum>).
    #[test]
    fn text_reads_on_the_page_at_wcag_aa() {
        assert!((contrast_ratio(Color::WHITE, Color::BLACK) - 21.0).abs() < 1e-3);
        assert!((contrast_ratio(MUTED, BG) - contrast_ratio(BG, MUTED)).abs() < 1e-6);
        let body = contrast_ratio(MUTED, BG);
        assert!(body >= 4.5, "body text {body:.2}:1");
        let mut inks = vec![("title", INK), ("value", BRIGHT), ("flaw", BAD), ("toast", ACCENT)];
        inks.extend([Rarity::Common, Rarity::Magic, Rarity::Rare, Rarity::Legendary].map(|r| ("rarity", rarity_color(r))));
        for (what, ink) in inks {
            let ratio = contrast_ratio(ink, BG);
            assert!(ratio >= 4.5, "{what} {} {ratio:.2}:1", ink.to_srgba().to_hex());
        }
    }

    #[test]
    fn values_are_picked_out_of_a_line() {
        let runs = value_runs("Magic \u{b7} Sword \u{b7} 3.5 lb \u{b7} 1d8+1 damage \u{b7} 1\u{d7}4 cells");
        let values: Vec<&str> = runs.iter().filter(|(_, v)| *v).map(|(r, _)| r.trim_end()).collect();
        assert_eq!(values, vec!["3.5", "1d8+1", "1\u{d7}4"]);
        assert_eq!(runs.iter().map(|(r, _)| r.as_str()).collect::<String>(), "Magic \u{b7} Sword \u{b7} 3.5 lb \u{b7} 1d8+1 damage \u{b7} 1\u{d7}4 cells");
        assert_eq!(value_runs("Empty"), vec![("Empty".to_string(), false)]);
    }

    #[test]
    fn dashes_are_centred_along_a_side() {
        let runs = dash_runs(100.0, 6.0, 4.0);
        assert_eq!(runs.len(), 10);
        let last = runs.last().unwrap();
        assert!((runs[0].0 - (100.0 - last.0 - last.1)).abs() < 1e-4, "the same margin at both ends");
        assert!(runs.windows(2).all(|w| (w[1].0 - w[0].0 - 10.0).abs() < 1e-4));
        assert_eq!(dash_runs(3.0, 6.0, 4.0), vec![(0.0, 3.0)]);
        assert!(dash_runs(0.0, 6.0, 4.0).is_empty());
    }
}
