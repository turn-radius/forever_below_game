//! The right leaf of the inventory: the doll inside the runic seal ([`super::seal`]), with the
//! equipment slots round it where `data/slots.json` puts them (the plate's `renderSlots` and
//! `.slot` styles). An empty slot shows its name; a filled one, the item's icon.
//!
//! The doll itself is a placeholder silhouette until `fb_body` draws it. The hook for it is
//! [`DollView`]: a node showing [`DollAssets::doll`], an image that is already a render target.
//! The doll's camera renders into that image (`RenderTarget::Image`) and the screen shows it
//! with nothing else to change; the silhouette is only the image's starting pixels.

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use fb_items::{
    Kind,
    tables::{SlotSide, SlotSize, slot_places},
};

use super::icon::Icons;
use super::items::Inventory;
use super::seal::{Canvas, Glyph, Ink, Seal, SealInks, TURN_SECONDS};
use crate::ui::{BG, BRIGHT, FAINT, GLOW, GOLD, INK, LINE, MUTED, UiFonts, rarity_color};

/// The doll's frame on the page, in logical pixels: the plate's `.dollwrap`, smaller to share
/// the screen with the grids.
pub const FRAME: Vec2 = Vec2::new(600.0, 700.0);
/// The seal's radius in that frame: the plate's `min(W * 0.44, H * 0.42)`.
pub fn seal_radius() -> f32 {
    (FRAME.x * 0.44).min(FRAME.y * 0.42)
}
/// Sizes of big and small slots, in logical pixels: the plate's `clamp` sizes at the frame's
/// scale.
pub const BIG_SLOT: f32 = 80.0;
pub const SMALL_SLOT: f32 = 50.0;
/// The doll's square in the middle of the seal, and its image's pixels per logical pixel.
pub const DOLL_SIZE: f32 = 440.0;
const PIXELS_PER_POINT: f32 = 2.0;
/// Room each rune glyph is set in, in logical pixels.
const GLYPH_BOX: f32 = 20.0;
/// The carried light's glow: its square, how far its centre sits out to the side of the doll's
/// and below it, and its opacity at the centre, falling off with the square of the distance
/// out (our own choice).
const GLOW_SIZE: f32 = 520.0;
const GLOW_OFFSET: Vec2 = Vec2::new(140.0, 40.0);
const GLOW_PEAK: f32 = 0.16;
/// Opacity of the seal's runes over the dark page; its lines are drawn at 0.55 to 0.6
/// ([`super::seal`]).
const RUNE_ALPHA: f32 = 0.5;
/// The seal's own gold glow at its centre, fainter than the plate's 0.2: Bevy blends the UI
/// in linear light, where a wash over a near-black page reads far stronger than CSS's sRGB
/// blend of the same opacity.
const SEAL_GLOW: f32 = 0.07;
/// The seal's bands are shown at this opacity, for the same reason: drawn at 0.55 to 0.6 in
/// the plate, they show at about 40% here.
const SEAL_TINT: f32 = 0.7;

/// Where the doll is drawn: the node showing the doll's render target. `fb_body` points a
/// camera at `image` (`RenderTarget::Image`) to draw the delver there.
#[derive(Component, Clone, Debug)]
pub struct DollView {
    pub image: Handle<Image>,
}

/// An equipment slot round the doll.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotView(pub fb_items::Slot);

/// The carried light's glow beside the doll.
#[derive(Component, Debug)]
pub struct LightGlow;

/// A turning band of the seal: 0 outer, 1 middle, 2 inner.
#[derive(Component, Clone, Copy, Debug)]
pub struct SealBand(pub usize);

/// The seal's three band images and the doll's render target, made once.
#[derive(Resource, Clone, Debug)]
pub struct DollAssets {
    pub bands: [Handle<Image>; 3],
    /// The doll's render target, holding the placeholder silhouette until a camera draws
    /// into it.
    pub doll: Handle<Image>,
    /// The warm glow a carried light casts on its side of the doll.
    pub glow: Handle<Image>,
}

/// The warm glow of a carried light, a canvas [`GLOW_SIZE`] units square.
pub fn light_glow(scale: f32) -> Canvas {
    let mut canvas = Canvas::new((GLOW_SIZE * scale).round() as u32, scale);
    canvas.radial([0.0, 0.0], GLOW_SIZE * 0.5, srgb(GLOW), |t| GLOW_PEAK * (1.0 - t).powi(2));
    canvas
}

/// The side of the frame the carried light is on: the off hand's if it holds a light and the
/// main hand does not, otherwise the main hand's, where the torch is held (`crate::carry`
/// holds it low on the right).
pub fn light_side(inventory: &Inventory) -> SlotSide {
    let lit = |slot| inventory.worn(slot).and_then(|id| inventory.item(id)).is_some_and(|item| item.profile.stats.light > 0.0);
    let hand = if lit(fb_items::Slot::Offhand) && !lit(fb_items::Slot::Mainhand) { fb_items::Slot::Offhand } else { fb_items::Slot::Mainhand };
    slot_places().iter().find(|p| p.slot == hand).map_or(SlotSide::Left, |p| p.side)
}

fn srgb(colour: Color) -> [f32; 3] {
    let c = colour.to_srgba();
    [c.red, c.green, c.blue]
}

fn image_of(canvas: &Canvas) -> Image {
    Image::new(
        Extent3d { width: canvas.size, height: canvas.size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        canvas.to_rgba8(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
}

/// A plain standing figure, faint ink on the page, in a canvas `DOLL_SIZE` units square: a
/// stand-in for the doll, about the plate's framing of a human.
pub fn silhouette(scale: f32) -> Canvas {
    let mut canvas = Canvas::new((DOLL_SIZE * scale).round() as u32, scale);
    let ink = Ink::new(srgb(LINE.mix(&INK, 0.3)), 1.0);
    canvas.disc([0.0, -172.0], 25.0, ink);
    canvas.line([0.0, -150.0], [0.0, -132.0], 18.0, ink);
    canvas.line([-40.0, -112.0], [40.0, -112.0], 36.0, ink);
    canvas.line([-28.0, -100.0], [-22.0, 10.0], 44.0, ink);
    canvas.line([28.0, -100.0], [22.0, 10.0], 44.0, ink);
    canvas.line([0.0, -100.0], [0.0, 12.0], 44.0, ink);
    for side in [-1.0f32, 1.0] {
        canvas.line([side * 58.0, -110.0], [side * 72.0, -20.0], 22.0, ink);
        canvas.line([side * 72.0, -20.0], [side * 80.0, 52.0], 18.0, ink);
        canvas.disc([side * 81.0, 62.0], 11.0, ink);
        canvas.line([side * 20.0, 20.0], [side * 25.0, 110.0], 32.0, ink);
        canvas.line([side * 25.0, 110.0], [side * 27.0, 196.0], 26.0, ink);
        canvas.line([side * 27.0, 196.0], [side * 38.0, 204.0], 16.0, ink);
    }
    canvas.fade(0.16);
    canvas
}

impl FromWorld for DollAssets {
    fn from_world(world: &mut World) -> Self {
        // Gold lines and gold dots alike, faint over the dark page.
        let inks = SealInks { line: srgb(GOLD), gold: srgb(GOLD), page: srgb(BG), glow: SEAL_GLOW };
        let layers = Seal { r: seal_radius() }.layers(PIXELS_PER_POINT, inks);
        let doll = silhouette(PIXELS_PER_POINT);
        let mut doll_image = image_of(&doll);
        // A render target as well, so the doll's camera can draw over the placeholder.
        doll_image.texture_descriptor.usage |= bevy::render::render_resource::TextureUsages::RENDER_ATTACHMENT;
        let glow = image_of(&light_glow(1.0));
        let mut images = world.resource_mut::<Assets<Image>>();
        DollAssets { bands: layers.map(|canvas| images.add(image_of(&canvas))), doll: images.add(doll_image), glow: images.add(glow) }
    }
}

/// Turns the seal's bands, slowly, as the plate's animations do.
pub fn turn_seal(time: Res<Time<Real>>, mut bands: Query<(&SealBand, &mut UiTransform)>) {
    let t = time.elapsed_secs();
    for (band, mut transform) in &mut bands {
        transform.rotation = Rot2::radians(t / TURN_SECONDS[band.0] * std::f32::consts::TAU);
    }
}

/// Builds the doll's leaf: the seal, the doll, and the slots with their labels.
pub fn doll_leaf(parent: &mut ChildSpawnerCommands, assets: &DollAssets, inventory: &Inventory, fonts: &UiFonts, icons: &mut Icons) {
    let seal = Seal { r: seal_radius() };
    let side = seal.r * 2.12;
    let centre = FRAME * 0.5;
    let [outer, middle, inner] = seal.bands([15.0 * 0.7 + 4.0, 12.0 * 0.7 + 4.0, 12.0 * 0.7 + 4.0]);
    let rune_ink = GOLD.with_alpha(RUNE_ALPHA);
    let glyph_inks = [(15.0, rune_ink), (12.0, rune_ink), (12.0, rune_ink)];
    parent.spawn(Node { width: px(FRAME.x), height: px(FRAME.y), flex_shrink: 0.0, ..default() }).with_children(|frame| {
        let glow_x = match light_side(inventory) {
            SlotSide::Left => centre.x - GLOW_OFFSET.x,
            SlotSide::Right => centre.x + GLOW_OFFSET.x,
        };
        frame.spawn((
            LightGlow,
            Node {
                position_type: PositionType::Absolute,
                left: px(glow_x - GLOW_SIZE * 0.5),
                top: px(centre.y + GLOW_OFFSET.y - GLOW_SIZE * 0.5),
                width: px(GLOW_SIZE),
                height: px(GLOW_SIZE),
                ..default()
            },
            ImageNode::new(assets.glow.clone()),
        ));
        for (band, glyphs) in [outer, middle, inner].into_iter().enumerate() {
            frame
                .spawn((
                    SealBand(band),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(centre.x - side * 0.5),
                        top: px(centre.y - side * 0.5),
                        width: px(side),
                        height: px(side),
                        ..default()
                    },
                    UiTransform::IDENTITY,
                    ImageNode::new(assets.bands[band].clone()).with_color(Color::WHITE.with_alpha(SEAL_TINT)),
                ))
                .with_children(|layer| {
                    let (size, colour) = glyph_inks[band];
                    for glyph in glyphs {
                        rune(layer, fonts, &glyph, side, size, colour);
                    }
                    if band == 1 {
                        for (point, text) in seal.star_points().into_iter().zip(seal.medallion_runes()) {
                            rune(layer, fonts, &Glyph { text, at: point, turn: 0.0 }, side, 13.0, GOLD.with_alpha(0.6));
                        }
                    }
                });
        }
        frame.spawn((
            DollView { image: assets.doll.clone() },
            Node {
                position_type: PositionType::Absolute,
                left: px(centre.x - DOLL_SIZE * 0.5),
                top: px(centre.y - DOLL_SIZE * 0.5),
                width: px(DOLL_SIZE),
                height: px(DOLL_SIZE),
                ..default()
            },
            ImageNode::new(assets.doll.clone()),
        ));
        for place in slot_places() {
            let size = match place.size {
                SlotSize::Big => BIG_SLOT,
                SlotSize::Small => SMALL_SLOT,
            };
            let x = px(FRAME.x * place.x_percent / 100.0);
            let worn = inventory.worn(place.slot).and_then(|id| inventory.item(id));
            let (left, right) = match place.side {
                SlotSide::Left => (x, Val::Auto),
                SlotSide::Right => (Val::Auto, x),
            };
            frame
                .spawn((
                    SlotView(place.slot),
                    Node {
                        position_type: PositionType::Absolute,
                        left,
                        right,
                        top: px(FRAME.y * place.y_percent / 100.0),
                        width: px(size),
                        height: px(size),
                        padding: UiRect::all(px(4)),
                        border: UiRect::all(px(1.5)),
                        border_radius: BorderRadius::all(px(8)),
                        ..default()
                    },
                    BackgroundColor(BG.with_alpha(0.8)),
                    // Filled, a bright cream outline; empty, a dim tan one.
                    BorderColor::all(if worn.is_some() { BRIGHT } else { LINE }),
                ))
                .with_children(|slot| {
                    if let Some(item) = worn {
                        let icon = icons.of(item, false);
                        let shape = super::icon::icon_size(item, false).as_vec2();
                        let fit = (size - 11.0) / shape.max_element();
                        slot.spawn(Node { width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() })
                            .with_child((Node { width: px(shape.x * fit), height: px(shape.y * fit), ..default() }, ImageNode::new(icon)));
                        if item.profile.kind == Kind::Gear {
                            slot.spawn((
                                Node { position_type: PositionType::Absolute, right: px(5), top: px(5), width: px(7), height: px(7), border_radius: BorderRadius::MAX, ..default() },
                                BackgroundColor(rarity_color(item.rarity)),
                            ));
                        }
                    }
                    let font = if place.size == SlotSize::Big { 14.0 } else { 12.0 };
                    let (label_left, label_right) = match place.side {
                        SlotSide::Left => (px(0), Val::Auto),
                        SlotSide::Right => (Val::Auto, px(0)),
                    };
                    slot.spawn((
                        Node { position_type: PositionType::Absolute, top: percent(100), left: label_left, right: label_right, margin: UiRect::top(px(2)), ..default() },
                        Text::new(place.label.clone()),
                        UiFonts::style(&fonts.display, font, INK),
                        TextLayout::no_wrap(),
                    ));
                });
        }
        frame.spawn((
            Node { position_type: PositionType::Absolute, right: px(6), bottom: px(4), ..default() },
            Text::new("The doll is to come"),
            UiFonts::style(&fonts.body, 12.0, MUTED.mix(&FAINT, 0.4)),
        ));
    });
}

/// One rune of a band, centred on its place in a layer `side` across, turned to its circle.
fn rune(layer: &mut ChildSpawnerCommands, fonts: &UiFonts, glyph: &Glyph, side: f32, size: f32, colour: Color) {
    layer
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(side * 0.5 + glyph.at[0] - GLYPH_BOX * 0.5),
                top: px(side * 0.5 + glyph.at[1] - GLYPH_BOX * 0.5),
                width: px(GLYPH_BOX),
                height: px(GLYPH_BOX),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            UiTransform::from_rotation(Rot2::radians(glyph.turn)),
        ))
        .with_child((Text::new(glyph.text.to_string()), UiFonts::style(&fonts.rune, size, colour), TextLayout::no_wrap()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seal_fits_its_frame_as_the_plate_sizes_it() {
        assert_eq!(seal_radius(), (600.0f32 * 0.44).min(700.0 * 0.42));
        assert!(seal_radius() * 2.12 <= FRAME.x.max(FRAME.y));
    }

    #[test]
    fn every_slot_has_a_place_on_the_frame() {
        let places = slot_places();
        for slot in [
            fb_items::Slot::Head,
            fb_items::Slot::Neck,
            fb_items::Slot::Shirt,
            fb_items::Slot::Cloak,
            fb_items::Slot::Body,
            fb_items::Slot::Mainhand,
            fb_items::Slot::Offhand,
            fb_items::Slot::Hands,
            fb_items::Slot::Belt,
            fb_items::Slot::Legs,
            fb_items::Slot::Feet,
            fb_items::Slot::Back,
            fb_items::Slot::Ring1,
            fb_items::Slot::Ring2,
            fb_items::Slot::Charm,
        ] {
            let place = places.iter().find(|p| p.slot == slot).unwrap_or_else(|| panic!("{slot:?}"));
            let size = if place.size == SlotSize::Big { BIG_SLOT } else { SMALL_SLOT };
            assert!(FRAME.x * place.x_percent / 100.0 + size <= FRAME.x && FRAME.y * place.y_percent / 100.0 + size <= FRAME.y, "{slot:?} off the frame");
        }
    }

    #[test]
    fn the_glow_is_on_the_side_of_the_hand_holding_a_light() {
        let mut inventory = Inventory::default();
        let main = slot_places().iter().find(|p| p.slot == fb_items::Slot::Mainhand).unwrap().side;
        let off = slot_places().iter().find(|p| p.slot == fb_items::Slot::Offhand).unwrap().side;
        assert_ne!(main, off);
        // The torch is held in the main hand.
        assert_eq!(light_side(&inventory), main);
        let lantern = (1..200).filter_map(|s| fb_items::roll_item("Lantern", s)).find(|i| i.profile.stats.light > 0.0).unwrap();
        let lantern = inventory.add(lantern);
        inventory.wear(lantern, fb_items::Slot::Offhand).unwrap();
        assert_eq!(light_side(&inventory), off);
    }

    #[test]
    fn the_glow_is_warm_in_the_middle_and_gone_at_the_edge() {
        let canvas = light_glow(0.25);
        let c = canvas.size / 2;
        assert!((canvas.alpha(c, c) - GLOW_PEAK).abs() < 0.03, "{}", canvas.alpha(c, c));
        assert!(canvas.alpha(c, c / 2) < canvas.alpha(c, c));
        assert_eq!(canvas.alpha(0, 0), 0.0);
    }

    #[test]
    fn the_placeholder_is_a_faint_figure_in_the_middle() {
        let canvas = silhouette(1.0);
        let c = canvas.size / 2;
        assert!(canvas.alpha(c, c - 40) > 0.1 && canvas.alpha(c, c - 40) <= 0.16 + 1e-4, "the chest");
        assert_eq!(canvas.alpha(5, 5), 0.0);
    }
}
