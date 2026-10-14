//! The parchment ledger's look, shared by the HUD and the inventory screen: the plate's
//! palette and fonts (`docs/rendering.md`, "Palette and UI"; the tokens at the top of the
//! plate's `<style>`, light theme).
//!
//! The fonts are the plate's, from Google Fonts under the SIL Open Font License 1.1, in
//! `game/assets/fonts` with their licences (`game/assets/fonts/README.md`).

use bevy::prelude::*;
use fb_items::Rarity;

const fn hex(rgb: u32) -> Color {
    Color::srgb(((rgb >> 16) & 0xff) as f32 / 255.0, ((rgb >> 8) & 0xff) as f32 / 255.0, (rgb & 0xff) as f32 / 255.0)
}

/// The page behind everything (`--bg`).
pub const BG: Color = hex(0xf2ede0);
pub const PAPER: Color = hex(0xebe5d5);
/// An empty grid cell (`--cell`).
pub const CELL: Color = hex(0xd7d1c4);
/// Grid lines and borders (`--line`).
pub const LINE: Color = hex(0x3d302a);
pub const INK: Color = hex(0x2b2420);
pub const MUTED: Color = hex(0x7f7568);
pub const FAINT: Color = hex(0xcfc6b3);
pub const ACCENT: Color = hex(0x6f4526);
pub const GOLD: Color = hex(0xa8782e);
/// The portrait's Stygian blue.
pub const STYGIAN: Color = hex(0x22325c);
pub const STYGIAN_DEEP: Color = hex(0x0f1834);
/// A drop that fits, and one that does not (`--ok`, `--bad`).
pub const OK: Color = hex(0x4f7d48);
pub const BAD: Color = hex(0xad3f2c);

/// The colour an item's name is written in.
pub fn rarity_color(rarity: Rarity) -> Color {
    match rarity {
        Rarity::Common => hex(0x4a4239),
        Rarity::Magic => hex(0x2c5bb0),
        Rarity::Rare => hex(0x99700f),
        Rarity::Legendary => hex(0xbb5119),
    }
}

/// The plate's four faces.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_matches_the_plate() {
        assert_eq!(PAPER.to_srgba().to_hex(), "#EBE5D5");
        assert_eq!(CELL.to_srgba().to_hex(), "#D7D1C4");
        assert_eq!(INK.to_srgba().to_hex(), "#2B2420");
        assert_eq!(rarity_color(Rarity::Legendary).to_srgba().to_hex(), "#BB5119");
    }
}
