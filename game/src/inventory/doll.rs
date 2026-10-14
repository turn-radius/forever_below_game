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
    Kind, Rarity,
    tables::{SlotSide, SlotSize, slot_places},
};

use super::icon::Icons;
use super::items::Inventory;
use super::seal::{Canvas, Glyph, Ink, Seal, SealInks, TURN_SECONDS};
use crate::ui::{BG, FAINT, GOLD, INK, MUTED, UiFonts, rarity_color};

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

/// Where the doll is drawn: the node showing the doll's render target. `fb_body` points a
/// camera at `image` (`RenderTarget::Image`) to draw the delver there.
#[derive(Component, Clone, Debug)]
pub struct DollView {
    pub image: Handle<Image>,
}

/// An equipment slot round the doll.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotView(pub fb_items::Slot);

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
    let ink = Ink::new(srgb(INK), 1.0);
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
        let inks = SealInks { line: srgb(MUTED), gold: srgb(GOLD), page: srgb(BG) };
        let layers = Seal { r: seal_radius() }.layers(PIXELS_PER_POINT, inks);
        let doll = silhouette(PIXELS_PER_POINT);
        let mut doll_image = image_of(&doll);
        // A render target as well, so the doll's camera can draw over the placeholder.
        doll_image.texture_descriptor.usage |= bevy::render::render_resource::TextureUsages::RENDER_ATTACHMENT;
        let mut images = world.resource_mut::<Assets<Image>>();
        DollAssets { bands: layers.map(|canvas| images.add(image_of(&canvas))), doll: images.add(doll_image) }
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
    let glyph_inks = [(15.0, MUTED.with_alpha(0.62)), (12.0, GOLD.with_alpha(0.7)), (12.0, GOLD.with_alpha(0.7))];
    parent.spawn(Node { width: px(FRAME.x), height: px(FRAME.y), flex_shrink: 0.0, ..default() }).with_children(|frame| {
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
                    ImageNode::new(assets.bands[band].clone()),
                ))
                .with_children(|layer| {
                    let (size, colour) = glyph_inks[band];
                    for glyph in glyphs {
                        rune(layer, fonts, &glyph, side, size, colour);
                    }
                    if band == 1 {
                        for (point, text) in seal.star_points().into_iter().zip(seal.medallion_runes()) {
                            rune(layer, fonts, &Glyph { text, at: point, turn: 0.0 }, side, 13.0, GOLD.with_alpha(0.85));
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
                        border_radius: BorderRadius::all(px(6)),
                        ..default()
                    },
                    BackgroundColor(BG.with_alpha(0.7)),
                    BorderColor::all(if worn.is_some() { INK } else { MUTED.with_alpha(0.55) }),
                ))
                .with_children(|slot| {
                    if let Some(item) = worn {
                        let icon = icons.of(item, false);
                        let shape = super::icon::icon_size(item, false).as_vec2();
                        let fit = (size - 11.0) / shape.max_element();
                        slot.spawn(Node { width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() })
                            .with_child((Node { width: px(shape.x * fit), height: px(shape.y * fit), ..default() }, ImageNode::new(icon)));
                        if item.profile.kind == Kind::Gear && item.rarity != Rarity::Common {
                            slot.spawn((
                                Node { position_type: PositionType::Absolute, right: px(4), top: px(4), width: px(7), height: px(7), border_radius: BorderRadius::MAX, ..default() },
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
                        UiFonts::style(&fonts.display, font, MUTED),
                        TextLayout::no_wrap(),
                    ));
                });
        }
        frame.spawn((
            Node { position_type: PositionType::Absolute, right: px(6), bottom: px(4), ..default() },
            Text::new("The doll is to come"),
            UiFonts::style(&fonts.body, 12.0, FAINT.mix(&MUTED, 0.5)),
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
    fn the_placeholder_is_a_faint_figure_in_the_middle() {
        let canvas = silhouette(1.0);
        let c = canvas.size / 2;
        assert!(canvas.alpha(c, c - 40) > 0.1 && canvas.alpha(c, c - 40) <= 0.16 + 1e-4, "the chest");
        assert_eq!(canvas.alpha(5, 5), 0.0);
    }
}
