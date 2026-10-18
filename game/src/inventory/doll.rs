//! The right leaf of the inventory: the doll inside the runic seal ([`super::seal`]), with the
//! equipment slots round it where `data/slots.json` puts them (the plate's `renderSlots` and
//! `.slot` styles). An empty slot shows its name; a filled one, the item's icon.
//!
//! The doll is the delver's own body ([`crate::character::Character`]) dressed in what the
//! [`Inventory`] has worn and holding what is in the main and off hand ([`Figure::dress`]),
//! drawn in the house style by its own camera into [`DollAssets::doll`], which the
//! [`DollView`] node shows. As the icons are ([`super::icon`]), it stands on a stage far
//! below the level, on its own render layer ([`DOLL_LAYER`]), lit by a point light far off in
//! the Armory key's direction and the Armory's ambient fill, so no light of the level reaches
//! it. It is redressed whenever what is worn changes, faces the viewer, and turns about its
//! own axis when dragged. A lit thing it holds, or a lit lantern hung at its hip, lights its side
//! of the body with a warm point light at the flame, and its side of the seal with a warm glow.

use bevy::{
    app::Propagate,
    asset::RenderAssetUsages,
    camera::{Exposure, RenderTarget, ScalingMode, visibility::RenderLayers},
    input::mouse::AccumulatedMouseMotion,
    light::NotShadowCaster,
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        view::Tonemapping,
    },
    window::PrimaryWindow,
};
use fb_items::{
    Kind, Slot,
    tables::{SlotSide, SlotSize, slot_places},
};

use std::sync::Arc;

use super::icon::Icons;
use super::items::Inventory;
use super::seal::{Canvas, Glyph, Seal, SealInks, TURN_SECONDS};
use crate::launch::AppState;
use crate::carry::item_light;
use crate::character::Character;
use crate::figure::{Figure, spawn_figure};
use crate::fitting::{FitCache, Fitting, figure_key};
use crate::light::{StudioLight, Torch, point_light, reach_falloff};
use crate::play::{Screen, ScreenOpen};
use crate::render::ItemSpawner;
use crate::ui::{BG, BRIGHT, GLOW, GOLD, INK, LINE, UiFonts, node_rect, rarity_color, ui_cursor};

/// Render layer the doll's camera and the doll are on.
pub const DOLL_LAYER: usize = 3;
/// Where the doll stands: far below the level, and far from the icons' stages and their
/// lights (`super::icon`) and the view model's (`crate::carry::VIEW_MODEL_STAGE`).
pub const DOLL_STAGE: Vec3 = Vec3::new(0.0, -50_000.0, -30_000.0);
/// Room round the turning doll in its image, as a multiple of its height or its turning
/// width, whichever is larger.
const DOLL_PADDING: f32 = 1.04;
/// Radians the doll turns per logical pixel dragged (our own tuning: a drag across the doll's
/// square turns it about once round).
const TURN_PER_PIXEL: f32 = 0.014;
/// The key's strength on the doll before the shader's quarter steps, as for the icons.
const KEY_LEVEL: f32 = 0.6;
/// A held light's strength close up and its reach as a multiple of the body's height (our own
/// tuning: warm on its own arm and side, gone by the far one).
const HELD_LIGHT_PEAK: f32 = 0.55;
const HELD_LIGHT_REACH: f32 = 0.9;

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
    /// The doll's render target, which its camera draws into.
    pub doll: Handle<Image>,
    /// The warm glow a carried light casts on its side of the doll.
    pub glow: Handle<Image>,
    /// The portrait of the doll's head for the header, which its own camera draws into.
    pub portrait: Handle<Image>,
}

/// The portrait's size in the header, in logical pixels (the plate's `.portrait`, 92 by 104),
/// and its image's pixels (the plate's 184 by 208 `snap`).
pub const PORTRAIT: Vec2 = Vec2::new(92.0, 104.0);
const PORTRAIT_PIXELS: UVec2 = UVec2::new(184, 208);
/// The portrait looks at the head from `(.32, .04, 1)` (the plate's `drawPortrait`).
const PORTRAIT_FROM: Vec3 = Vec3::new(0.32, 0.04, 1.0);

/// The warm glow of a carried light, a canvas [`GLOW_SIZE`] units square.
pub fn light_glow(scale: f32) -> Canvas {
    let mut canvas = Canvas::new((GLOW_SIZE * scale).round() as u32, scale);
    canvas.radial([0.0, 0.0], GLOW_SIZE * 0.5, srgb(GLOW), |t| GLOW_PEAK * (1.0 - t).powi(2));
    canvas
}

/// The hand holding a lit thing, the main hand's before the off hand's, if either does.
pub fn lit_hand(inventory: &Inventory) -> Option<Slot> {
    let lit = |slot| inventory.worn(slot).and_then(|id| inventory.item(id)).is_some_and(|item| item_light(&item.shape).is_some());
    [Slot::Mainhand, Slot::Offhand].into_iter().find(|&slot| lit(slot))
}

/// The slot holding the carried light: a hand holding a lit thing ([`lit_hand`]), or else a lit
/// lantern hung at the hip.
pub fn lit_slot(inventory: &Inventory) -> Option<Slot> {
    let lit = |slot| inventory.worn(slot).and_then(|id| inventory.item(id)).is_some_and(|item| item_light(&item.shape).is_some());
    lit_hand(inventory).or_else(|| lit(Slot::Hip).then_some(Slot::Hip))
}

/// The side of the frame the carried light is on: the side its slot is on, if a hand holds a
/// lit thing or a lit lantern hangs at the hip.
pub fn light_side(inventory: &Inventory) -> Option<SlotSide> {
    let slot = lit_slot(inventory)?;
    slot_places().iter().find(|p| p.slot == slot).map(|p| p.side)
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

impl FromWorld for DollAssets {
    fn from_world(world: &mut World) -> Self {
        // Gold lines and gold dots alike, faint over the dark page.
        let inks = SealInks { line: srgb(GOLD), gold: srgb(GOLD), page: srgb(BG), glow: SEAL_GLOW };
        let layers = Seal { r: seal_radius() }.layers(PIXELS_PER_POINT, inks);
        let side = (DOLL_SIZE * PIXELS_PER_POINT).round() as u32;
        let mut doll_image = Image::new_target_texture(side, side, TextureFormat::Rgba8UnormSrgb, None);
        doll_image.asset_usage = RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD;
        let glow = image_of(&light_glow(1.0));
        let mut images = world.resource_mut::<Assets<Image>>();
        let mut portrait = Image::new_target_texture(PORTRAIT_PIXELS.x, PORTRAIT_PIXELS.y, TextureFormat::Rgba8UnormSrgb, None);
        portrait.asset_usage = RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD;
        DollAssets {
            bands: layers.map(|canvas| images.add(image_of(&canvas))),
            doll: images.add(doll_image),
            glow: images.add(glow),
            portrait: images.add(portrait),
        }
    }
}

/// What the doll was dressed for: the key of its body and every worn item, wear included
/// ([`crate::fitting::figure_key`]).
type Dressed = String;

/// The doll's stage: what it was last dressed in, its figure, camera and key light, and how
/// far it is turned.
#[derive(Resource)]
pub struct DollStage {
    /// The delver and each worn item (slot, base and seed) the figure was dressed for.
    dressed: Option<Dressed>,
    figure: Option<Entity>,
    camera: Option<Entity>,
    portrait: Option<Entity>,
    /// Radians the doll is turned from facing the viewer.
    pub turn: f32,
    /// The figure as last dressed.
    pub built: Option<Arc<Figure>>,
    /// An async fitting job running off the main thread, if any.
    fitting: Option<Fitting<Dressed, Arc<Figure>>>,
}

impl Default for DollStage {
    fn default() -> Self {
        DollStage {
            dressed: None,
            figure: None,
            camera: None,
            portrait: None,
            turn: 0.0,
            built: None,
            fitting: None,
        }
    }
}

impl DollStage {
    /// The camera drawing the doll, once there is one.
    pub fn camera(&self) -> Option<Entity> {
        self.camera
    }

    /// The camera drawing the portrait, once there is one.
    pub fn portrait(&self) -> Option<Entity> {
        self.portrait
    }
}

/// The portrait's view of a head centred at `centre` with half height `r`, in body space: the
/// orthographic width and height and the point it looks at. The plate frames the box from
/// `1.55 r` either side and `1.45 r` below to `1.75 r` above a point `0.35 r` under the head's
/// centre (`drawPortrait`), here widened to the image's shape.
pub fn portrait_view(centre: Vec3, r: f32) -> (Vec2, Vec3) {
    let aspect = PORTRAIT_PIXELS.x as f32 / PORTRAIT_PIXELS.y as f32;
    let (width, height) = (3.1 * r, 3.2 * r);
    let height = height.max(width / aspect);
    (Vec2::new(height * aspect, height), centre + Vec3::Y * (-0.35 + (1.75 - 1.45) * 0.5) * r)
}

/// The orthographic view, width and height in world units, that holds a figure of `bounds`
/// turning about its own vertical axis in a square image, and the height it looks at.
pub fn doll_view((min, max): (Vec3, Vec3)) -> (f32, f32) {
    let reach = min.xz().abs().max(max.xz().abs()).length();
    let side = (max.y - min.y).max(2.0 * reach) * DOLL_PADDING;
    (side, (min.y + max.y) * 0.5)
}

/// When the inventory screen opens, check if the equipment changed and start an async fitting
/// task if needed. The old doll stays visible until the new one is ready.
pub fn start_doll_fitting(
    inventory: Res<Inventory>,
    character: Res<Character>,
    open: Res<ScreenOpen>,
    mut stage: ResMut<DollStage>,
    mut cache: ResMut<FitCache>,
) {
    if open.0 != Some(Screen::Inventory) {
        return;
    }
    // Keys spell out every item's state, so they are made only when something may have moved.
    if !(inventory.is_changed() || character.is_changed() || open.is_changed()) {
        return;
    }
    let spec = character.spec();
    let dirt = character.dirt;
    let worn: Vec<(Slot, fb_items::Item)> = inventory.equipped().into_iter().map(|(slot, item)| (slot, item.clone())).collect();
    let key = figure_key(&spec, &worn, dirt);
    if stage.dressed.as_ref() == Some(&key) {
        // Already dressed for this equipment; drop any in-flight fit
        stage.fitting = None;
        return;
    }
    if stage.fitting.as_ref().is_some_and(|f| f.key() == &key) {
        return;
    }
    // Dressed in this a moment ago: no job.
    if let Some(figure) = cache.figure(&key) {
        stage.fitting = Some(Fitting::ready(key, figure));
        return;
    }
    stage.fitting = Some(Fitting::start(key, move || Arc::new(Figure::dress(&spec, &worn, dirt))));
}

/// Poll the fitting task and spawn the figure when it's ready.
pub fn poll_doll_fitting(
    mut commands: Commands,
    assets: Res<DollAssets>,
    mut stage: ResMut<DollStage>,
    mut cache: ResMut<FitCache>,
    mut spawner: ItemSpawner,
) {
    let Some(mut fitting) = stage.fitting.take() else { return };
    if let Some(figure) = fitting.poll() {
        cache.store_figure(fitting.key(), figure.clone());
        stage.dressed = Some(fitting.key().clone());
        spawn_doll_figure(&mut commands, &mut spawner, &assets, &mut stage, &figure);
    } else {
        stage.fitting = Some(fitting);
    }
}

/// Spawn the fitted figure on the doll stage.
fn spawn_doll_figure(
    commands: &mut Commands,
    spawner: &mut ItemSpawner,
    assets: &DollAssets,
    stage: &mut DollStage,
    figure: &Arc<Figure>,
) {
    if let Some(old) = stage.figure.take() {
        commands.entity(old).despawn();
    }
    let Some(bounds) = figure.bounds() else { return };
    let layer = RenderLayers::layer(DOLL_LAYER);
    let exposure = Exposure::INDOOR;
    let root = spawn_figure(commands, spawner, figure);
    commands.entity(root).insert((
        Name::new("Doll"),
        DespawnOnExit(AppState::Delving),
        Transform::from_translation(DOLL_STAGE).with_rotation(Quat::from_rotation_y(stage.turn)),
        Propagate(layer.clone()),
        Propagate(NotShadowCaster),
    ));
    if let Some(light) = figure.light {
        let colour = Color::srgb(light.anchor.color.0, light.anchor.color.1, light.anchor.color.2);
        let lamp = Torch::flickering(point_light(colour, HELD_LIGHT_PEAK, figure.body.height * HELD_LIGHT_REACH, &exposure), light.anchor.flicker);
        commands.entity(root).with_child((lamp, Transform::from_translation(light.at), layer.clone()));
    }
    stage.figure = Some(root);

    // The portrait turns with the doll, so it always sees the face.
    let (head, _) = figure.head();
    let (view, looking_at) = portrait_view(head, figure.body.head.half_height);
    let from = PORTRAIT_FROM.normalize();
    let portrait = commands
        .spawn((
            Name::new("Portrait camera"),
            crate::render::ScreenInkExempt,
            Camera3d::default(),
            Camera { order: -4, clear_color: ClearColorConfig::Custom(Color::NONE), is_active: false, ..default() },
            RenderTarget::Image(assets.portrait.clone().into()),
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed { width: view.x, height: view.y },
                near: 0.1,
                far: 60.0,
                ..OrthographicProjection::default_3d()
            }),
            exposure,
            Tonemapping::Linear,
            AmbientLight { color: StudioLight::ARMORY.ambient_color, brightness: StudioLight::ARMORY.ambient / exposure.exposure(), ..default() },
            layer.clone(),
            Transform::from_translation(looking_at + from * 30.0).looking_at(looking_at, Vec3::Y),
            ChildOf(root),
        ))
        .id();
    stage.portrait = Some(portrait);

    let (side, centre_y) = doll_view(bounds);
    let looking_at = DOLL_STAGE + Vec3::Y * centre_y;
    let depth = 4.0 * side;
    let projection = Projection::Orthographic(OrthographicProjection {
        scaling_mode: ScalingMode::Fixed { width: side, height: side },
        near: 0.1,
        far: 2.0 * depth,
        ..OrthographicProjection::default_3d()
    });
    let at = Transform::from_translation(looking_at + Vec3::Z * depth).looking_at(looking_at, Vec3::Y);
    match stage.camera {
        Some(camera) => {
            commands.entity(camera).insert((projection, at));
        }
        None => {
            // Far off along the Armory key's direction, so its falloff is nearly flat over the
            // doll, as for the icons.
            let distance = 12.0 * side;
            let range = 8.0 * distance;
            let key = point_light(Color::WHITE, KEY_LEVEL / reach_falloff(distance, range), range, &exposure);
            let camera = commands
                .spawn((
                    Name::new("Doll camera"),
                    crate::render::ScreenInkExempt,
                    DespawnOnExit(AppState::Delving),
                    Camera3d::default(),
                    Camera { order: -3, clear_color: ClearColorConfig::Custom(Color::NONE), is_active: false, ..default() },
                    RenderTarget::Image(assets.doll.clone().into()),
                    projection,
                    exposure,
                    Tonemapping::Linear,
                    AmbientLight { color: StudioLight::ARMORY.ambient_color, brightness: StudioLight::ARMORY.ambient / exposure.exposure(), ..default() },
                    layer.clone(),
                    at,
                ))
                .with_child((Name::new("Doll key light"), key, Transform::from_translation(StudioLight::ARMORY.key_from() * distance), layer))
                .id();
            stage.camera = Some(camera);
        }
    }
    stage.built = Some(figure.clone());
}

/// Draws the doll and its portrait only while the inventory is open.
pub fn show_doll(open: Res<ScreenOpen>, stage: Res<DollStage>, mut cameras: Query<&mut Camera>) {
    let shown = open.is(Screen::Inventory);
    for entity in [stage.camera, stage.portrait].into_iter().flatten() {
        if let Ok(mut camera) = cameras.get_mut(entity)
            && camera.is_active != shown
        {
            camera.is_active = shown;
        }
    }
}

/// Turns the doll while the mouse is dragged across it with the button down, unless the press
/// took up an item or landed on a slot.
#[allow(clippy::too_many_arguments)]
pub fn turn_doll(
    open: Res<ScreenOpen>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    views: Query<(&ComputedNode, &UiGlobalTransform), With<DollView>>,
    slots: Query<(&ComputedNode, &UiGlobalTransform), With<SlotView>>,
    dragging: Res<super::Dragging>,
    mut stage: ResMut<DollStage>,
    mut transforms: Query<&mut Transform>,
    mut turning: Local<bool>,
) {
    if !open.is(Screen::Inventory) || !buttons.pressed(MouseButton::Left) {
        *turning = false;
        return;
    }
    if buttons.just_pressed(MouseButton::Left) {
        let cursor = window.and_then(|w| ui_cursor(&w, ui_scale.as_deref()));
        let on = |rect: Rect| cursor.is_some_and(|c| rect.contains(c));
        *turning = dragging.0.is_none() && views.iter().any(|(n, t)| on(node_rect(n, t))) && !slots.iter().any(|(n, t)| on(node_rect(n, t)));
    }
    if !*turning || motion.delta.x == 0.0 {
        return;
    }
    stage.turn = (stage.turn + motion.delta.x * TURN_PER_PIXEL).rem_euclid(std::f32::consts::TAU);
    if let Some(mut transform) = stage.figure.and_then(|f| transforms.get_mut(f).ok()) {
        transform.rotation = Quat::from_rotation_y(stage.turn);
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
        if let Some(side) = light_side(inventory) {
            let glow_x = match side {
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
        }
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
                        let shape = super::icon::icon_size(item, false).as_vec2();
                        let fit = (size - 11.0) / shape.max_element();
                        slot.spawn(Node { width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() })
                            .with_children(|centre| {
                                icons.spawn(centre, item, false, Node { width: px(shape.x * fit), height: px(shape.y * fit), ..default() }, (size * 0.45).min(40.0));
                            });
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
            fb_items::Slot::Hip,
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
        // Nothing lit in hand, no glow.
        assert_eq!(light_side(&inventory), None);
        let lantern = (1..200).filter_map(|s| fb_items::roll_item("Lantern", s)).find(|i| item_light(&i.shape).is_some()).unwrap();
        let lantern = inventory.add(lantern);
        inventory.wear(lantern, fb_items::Slot::Offhand).unwrap();
        assert_eq!(light_side(&inventory), Some(off));
        assert_ne!(Some(main), light_side(&inventory));
        // Hung at the hip, beside the belt, it glows on the hip's side.
        let belt = inventory.add(fb_items::roll_item("Belt", 2).unwrap());
        inventory.wear(belt, fb_items::Slot::Belt).unwrap();
        inventory.take_out(lantern);
        inventory.wear(lantern, fb_items::Slot::Hip).unwrap();
        let hip = slot_places().iter().find(|p| p.slot == fb_items::Slot::Hip).unwrap().side;
        assert_eq!((lit_slot(&inventory), light_side(&inventory)), (Some(fb_items::Slot::Hip), Some(hip)));
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
    fn the_doll_s_view_holds_it_full_height_as_it_turns() {
        // A body 17 tall, its arms and a lantern out to 4 either side.
        let (side, centre) = doll_view((Vec3::new(-4.0, 0.0, -1.0), Vec3::new(3.0, 17.0, 2.0)));
        assert!((17.0..17.0 * 1.1).contains(&side), "{side}");
        assert_eq!(centre, 8.5);
        // A wide shield on a gnome is held by its turning width.
        let (side, _) = doll_view((Vec3::new(-6.0, 0.0, -1.0), Vec3::new(2.0, 10.0, 1.0)));
        assert!(side >= 2.0 * 6.0f32.hypot(1.0), "{side}");
    }

    /// An app that dresses and turns the doll, with no window renderer: the item spawner's
    /// assets, an open inventory, a window with the cursor on the doll's square, and that
    /// square laid out at 100..540 by 100..540.
    fn stage_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Inventory>()
            .init_resource::<Character>()
            .init_resource::<DollStage>()
            .init_resource::<FitCache>()
            .init_resource::<super::super::Dragging>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .insert_resource(ScreenOpen(Some(Screen::Inventory)))
            .init_resource::<Assets<Image>>()
            .init_resource::<DollAssets>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::render::ToonMaterial>>()
            .init_resource::<Assets<crate::render::OutlineMaterial>>()
            .init_resource::<crate::render::OutlineMaterials>()
            .add_systems(Update, (start_doll_fitting, poll_doll_fitting, show_doll, turn_doll).chain());
        let mut window = Window::default();
        window.set_cursor_position(Some(Vec2::new(320.0, 320.0)));
        app.world_mut().spawn((window, PrimaryWindow));
        let image = app.world().resource::<DollAssets>().doll.clone();
        let node = ComputedNode { size: Vec2::splat(440.0), inverse_scale_factor: 1.0, ..ComputedNode::DEFAULT };
        app.world_mut().spawn((DollView { image }, node, UiGlobalTransform::from_translation(Vec2::splat(320.0))));
        app
    }

    /// Wait for the doll fitting to complete, polling until the figure is ready.
    fn wait_for_doll_fitting(app: &mut App) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            app.update();
            let stage = app.world().resource::<DollStage>();
            if stage.figure.is_some() && stage.fitting.is_none() {
                return;
            }
            if std::time::Instant::now() > deadline {
                panic!("Doll fitting did not complete within 60 seconds");
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn worn_names(app: &App) -> Vec<String> {
        app.world().resource::<DollStage>().built.as_ref().unwrap().pieces.iter().map(|p| p.name.clone()).collect()
    }

    #[test]
    fn the_doll_wears_what_is_equipped() {
        let mut app = stage_app();
        wait_for_doll_fitting(&mut app);
        assert_eq!(worn_names(&app), vec!["Body".to_string(), "Head".to_string()], "bare to begin with");
        let helm = fb_items::roll_item("Helm", 5).unwrap();
        let name = helm.title().to_string();
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let id = inventory.add(helm);
            inventory.wear(id, Slot::Head).unwrap();
        }
        wait_for_doll_fitting(&mut app);
        assert_eq!(worn_names(&app), vec!["Body".to_string(), "Head".to_string(), name]);
        let stage = app.world().resource::<DollStage>();
        let camera = app.world().get::<Camera>(stage.camera().unwrap()).unwrap();
        assert!(camera.is_active, "drawn while the inventory is open");
        assert!(matches!(app.world().get::<RenderTarget>(stage.camera().unwrap()), Some(RenderTarget::Image(_))));
    }

    #[test]
    fn the_portrait_is_of_the_doll_s_head() {
        let mut app = stage_app();
        wait_for_doll_fitting(&mut app);
        let stage = app.world().resource::<DollStage>();
        let (portrait, figure) = (stage.portrait().unwrap(), stage.figure.unwrap());
        let (head, r) = { let f = stage.built.as_ref().unwrap(); (f.head().0, f.body.head.half_height) };
        // Drawn into the portrait image, turning with the doll, and looking at its head.
        let target = app.world().get::<RenderTarget>(portrait).unwrap();
        assert!(matches!(target, RenderTarget::Image(i) if i.handle == app.world().resource::<DollAssets>().portrait));
        assert_eq!(app.world().get::<ChildOf>(portrait).map(|c| c.parent()), Some(figure));
        let at = app.world().get::<Transform>(portrait).unwrap();
        let to_head = (head - at.translation).normalize();
        assert!(to_head.dot(at.forward().as_vec3()) > 0.99, "looks at the head");
        let (view, _) = portrait_view(head, r);
        assert!(view.y >= 3.2 * r && view.y < 5.0 * r, "{view}");
    }

    #[test]
    fn swapping_between_two_outfits_a_second_time_starts_no_fit() {
        let mut app = stage_app();
        let (a, b) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            (inventory.add(fb_items::roll_item("Helm", 5).unwrap()), inventory.add(fb_items::roll_item("Helm", 6).unwrap()))
        };
        let wear = |app: &mut App, id| app.world_mut().resource_mut::<Inventory>().equip(id, Slot::Head).unwrap();
        wear(&mut app, a);
        wait_for_doll_fitting(&mut app);
        let first = worn_names(&app);
        wear(&mut app, b);
        wait_for_doll_fitting(&mut app);
        let second = worn_names(&app);
        assert_ne!(first, second);
        let misses = app.world().resource::<FitCache>().misses;
        for (id, names) in [(a, &first), (b, &second), (a, &first)] {
            wear(&mut app, id);
            app.update();
            assert!(app.world().resource::<DollStage>().fitting.is_none(), "no fit started");
            assert_eq!(&worn_names(&app), names);
        }
        let cache = app.world().resource::<FitCache>();
        assert_eq!((cache.misses, cache.hits), (misses, 3));
    }

    #[test]
    fn the_doll_turns_when_dragged() {
        let mut app = stage_app();
        wait_for_doll_fitting(&mut app);
        let figure = app.world().resource::<DollStage>().figure.unwrap();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        app.world_mut().resource_mut::<AccumulatedMouseMotion>().delta = Vec2::new(60.0, 0.0);
        app.update();
        let turn = app.world().resource::<DollStage>().turn;
        assert!((turn - 60.0 * TURN_PER_PIXEL).abs() < 1e-4, "{turn}");
        let rotation = app.world().get::<Transform>(figure).unwrap().rotation;
        assert!(rotation.abs_diff_eq(Quat::from_rotation_y(turn), 1e-5));
        // Let go, and dragging no longer turns it.
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Left);
        app.update();
        app.update();
        assert_eq!(app.world().resource::<DollStage>().turn, turn);
    }

    #[test]
    fn a_sword_equipped_to_the_main_hand_shows_in_the_doll_s_fist() {
        let mut app = stage_app();
        let sword = fb_items::roll_item("Sword", 11).unwrap();
        let name = sword.title().to_string();
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let id = inventory.add(sword);
            super::super::equip(&mut inventory, id, Slot::Mainhand).unwrap();
        }
        wait_for_doll_fitting(&mut app);
        let figure = app.world().resource::<DollStage>().built.clone().unwrap();
        let held = figure.pieces.iter().find(|p| p.name == name).expect("the sword is drawn");
        let fist = Vec3::from_array(figure.body.grip(fb_body::MAIN_HAND).origin);
        let nearest = held.mesh.parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p).distance(fist)).fold(f32::MAX, f32::min);
        assert!(nearest < 0.5, "{nearest}");
    }

    #[test]
    fn opening_inventory_after_equipping_while_closed_shows_new_gear() {
        let mut app = stage_app();
        wait_for_doll_fitting(&mut app);
        // Note initial figure is dressed (empty outfit)
        // Close inventory
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
        // Equip while closed
        let sword = fb_items::roll_item("Sword", 11).unwrap();
        let name = sword.title().to_string();
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let id = inventory.add(sword);
            super::super::equip(&mut inventory, id, Slot::Mainhand).unwrap();
        }
        app.update(); // Should not start fitting a new doll while closed
        {
            let stage = app.world().resource::<DollStage>();
            // Fitting should not be running while closed
            assert!(stage.fitting.is_none(), "no fitting should run while inventory closed");
        }
        // Open inventory
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        wait_for_doll_fitting(&mut app);
        // Check that new gear is shown
        let stage = app.world().resource::<DollStage>();
        let figure = stage.built.as_ref().unwrap();
        let has_sword = figure.pieces.iter().any(|p| p.name == name);
        assert!(has_sword, "new sword should be shown after opening inventory");
    }
}
