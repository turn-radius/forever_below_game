//! Folk met in the level: each [`FolkSpot`] drawn as a dressed `fb_body` figure, standing on
//! the floor at its spot, lit by the level's lights.
//!
//! - [`Person::of`] says who stands at a spot: their body ([`BodySpec::roll`]), a name from
//!   `data/syl.json`, and an outfit from their faction's leaning ([`fb_level::FACTIONS`])
//!   rolled through [`fb_items::outfit::Outfit::roll`] (which gives a bare-headed gnome a
//!   pointed hat, as the plate does). Everyone holds something: a weapon, a light or both.
//! - The shared [`Figure::dress`] (`crate::figure`, as the doll uses) fits it all to the body:
//!   held gear first, posing each arm, then what is worn. The pieces that turn with the head
//!   (head, hair, a helm or hat) go under a pivot at the neck.
//! - Each item is drawn with its own look (palette, condition, etching) by the
//!   [`ItemSpawner`], in the house style: toon bands, ink and decay. Skin carries the person's
//!   dirt. A lit torch or lantern in hand lights the room round them.
//! - Standing, they breathe and now and then shift their weight ([`Idle`]): the whole figure
//!   moves a little about its feet, since there is no skeleton. A player within
//!   [`LOOK_RANGE`] draws their head round ([`look_at`]), turning about the neck; under a hood,
//!   whose cape lies on the shoulders, the body turns a little instead.
//! - Each stands in a circle on the floor that the player cannot walk into
//!   ([`BlocksTheWay`], gathered into `play::Obstacles`).
//! - Looked at within reach, a card under the crosshair says who they are ([`folk_card_text`]):
//!   name, folk and faction, and what they wear and hold, item names in their rarity's colour,
//!   styled as the item card.
//!
//! Figures are built once per spot, in parallel, and kept in [`FolkFigures`], so the same
//! level shows the same people without building them again.

use std::sync::Arc;

use bevy::{app::Propagate, light::NotShadowCaster, platform::collections::HashMap, prelude::*, tasks::ComputeTaskPool, ui::ComputedUiRenderTargetInfo};
use fb_body::BodySpec;
use fb_items::{Item, Rarity, Slot, roll_item};
use fb_level::{FolkSpot, Level};
use procgen_rng::Mulberry32;

use crate::carry::{AimBuffer, Aimed, REACH, ViewModelCamera, given_light, nearest_aimed};
use crate::figure::Figure;
use crate::inventory::card::{CARD_WIDTH, place_under};
use crate::level::EXPOSURE;
use crate::play::{BlocksTheWay, CurrentLevel, DelveSystems, PlayerCamera, ScreenOpen};
use crate::ui::{BG, FAINT, INK, LINE, MUTED, UiFonts, rarity_color};
use crate::render::ItemSpawner;

pub struct FolkPlugin;

impl Plugin for FolkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FolkFigures>()
            .init_resource::<AimedFolk>()
            .init_resource::<UiFonts>()
            .add_systems(
                Update,
                (
                    spawn_folk.run_if(resource_exists_and_changed::<CurrentLevel>),
                    idle_folk,
                    turn_heads,
                    (aim_at_folk, spawn_folk_card, show_folk_card).chain(),
                )
                    .in_set(DelveSystems),
            );
    }
}

/// Salts for a person's name and dirt, drawn apart from their body and outfit. Our own.
const NAME_SALT: u32 = 0x4E41_4D45;
const DIRT_SALT: u32 = 0xD127_0001;

/// How far above a held flame its light hangs, in world units (10 cm).
const HELD_LIGHT_LIFT: f32 = 1.0;

/// Who stands at a spot.
#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    pub spot: FolkSpot,
    pub name: String,
    pub spec: BodySpec,
    /// The key of the outfit they dress from (`data/outfits.json`).
    pub outfit: &'static str,
    /// Everything worn and held, by slot.
    pub gear: Vec<(Slot, Item)>,
    /// How dirty their skin is, 0 clean to about 0.85 (the plate's `C.dirt`).
    pub dirt: f32,
}

impl Person {
    /// The person at `spot`: a pure function of it.
    pub fn of(spot: &FolkSpot) -> Person {
        let faction = spot.faction.row();
        let mut rng = Mulberry32::new(spot.seed ^ NAME_SALT);
        let name = fb_items::naming::folk_name(&mut rng, spot.folk.key());
        let outfit = *rng.pick(faction.outfits);
        let mut gear = fb_items::outfit::outfit(outfit).map_or_else(Vec::new, |o| o.roll(spot.seed, spot.folk.key()).worn);
        let has = |gear: &[(Slot, Item)], slot| gear.iter().any(|(s, _)| *s == slot);
        // Nobody stands about empty-handed: a torch if the outfit gave nothing to hold.
        if !has(&gear, Slot::Mainhand) && !has(&gear, Slot::Offhand) {
            gear.extend(roll_item("Torch", spot.seed.wrapping_add(2)).map(|torch| (Slot::Mainhand, torch)));
        }
        // The plate's `rollChar`: scavengers are filthy, the freshly fallen grubby, the rest
        // a little dirty.
        let mut dirt_rng = Mulberry32::new(spot.seed ^ DIRT_SALT);
        let dirt = match outfit {
            "scavenger" => dirt_rng.range_f32(0.55, 0.85),
            "fallen" => dirt_rng.range_f32(0.3, 0.5),
            _ => dirt_rng.range_f32(0.08, 0.38),
        };
        Person { spot: *spot, name, spec: BodySpec::roll(spot.folk, spot.seed), outfit, gear, dirt }
    }

    /// What they wear, innermost slots first as listed.
    pub fn worn(&self) -> impl Iterator<Item = &(Slot, Item)> {
        self.gear.iter().filter(|(slot, _)| !matches!(slot, Slot::Mainhand | Slot::Offhand))
    }

    /// What they hold.
    pub fn held(&self) -> impl Iterator<Item = &(Slot, Item)> {
        self.gear.iter().filter(|(slot, _)| matches!(slot, Slot::Mainhand | Slot::Offhand))
    }

    pub fn item(&self, slot: Slot) -> Option<&Item> {
        self.gear.iter().find(|(s, _)| *s == slot).map(|(_, item)| item)
    }
}

/// The circle on the floor someone in `figure` stands in, which the player cannot walk into:
/// their shoulders and a layer or two of clothes. Our own allowance.
pub fn footprint(figure: &Figure) -> f32 {
    figure.body.shoulder_half_width * 1.2 + 0.3
}

/// Figures already built, by spot seed, so a level drawn again reuses them.
#[derive(Resource, Default)]
pub struct FolkFigures(HashMap<u32, Arc<(Person, Figure)>>);

impl FolkFigures {
    /// The person and figure for every spot in `spots`, building (in parallel) those not yet
    /// built.
    pub fn for_spots(&mut self, spots: &[FolkSpot]) -> Vec<Arc<(Person, Figure)>> {
        let missing: Vec<FolkSpot> = spots.iter().filter(|s| !self.0.get(&s.seed).is_some_and(|built| built.0.spot == **s)).copied().collect();
        let built = ComputeTaskPool::get_or_init(Default::default).scope(|scope| {
            for spot in &missing {
                scope.spawn(async move {
                    let person = Person::of(spot);
                    let figure = Figure::dress(&person.spec, &person.gear, person.dirt);
                    (person, figure)
                });
            }
        });
        for pair in built {
            self.0.insert(pair.0.spot.seed, Arc::new(pair));
        }
        spots.iter().filter_map(|s| self.0.get(&s.seed).cloned()).collect()
    }
}

/// A person standing in the level: the root of their figure, on the floor at their spot.
#[derive(Component, Clone, Debug)]
#[require(DespawnOnExit<crate::launch::AppState> = DespawnOnExit(crate::launch::AppState::Delving), crate::simulate::Simulated)]
pub struct Denizen {
    pub person: Person,
    /// The circle on the floor they stand in ([`footprint`]).
    pub radius: f32,
    /// The figure's bounds in its root's space, for looking at them.
    pub min: Vec3,
    pub max: Vec3,
}

/// The figure under its [`Denizen`] root, moving as someone standing does ([`Idle`]).
#[derive(Component, Clone, Copy, Debug)]
pub struct Stance {
    pub idle: Idle,
    /// How far the whole body has turned toward someone near, yaw radians: only when the head
    /// cannot turn on its own (under a hood).
    pub turn: f32,
}

/// The part of a figure that turns with the head, pivoting at the neck: where it looks now as
/// `(yaw, pitch)` radians, the yaw turning the face (+Z) toward +X, the pitch up.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FolkHead {
    pub look: Vec2,
}

/// Breaths per second at rest: 15 a minute, inside the 12 to 18 the Cleveland Clinic gives for
/// a resting adult (<https://my.clevelandclinic.org/health/articles/10881-vital-signs>), each
/// person a little faster or slower.
const BREATHS_PER_SECOND: f32 = 0.25;
/// How far a breath lifts and fills the figure, as a share of its height and girth. Our own
/// tuning: enough to see close up, never enough to read as bobbing.
const BREATH_LIFT: f32 = 0.006;
const BREATH_FILL: f32 = 0.004;
/// Weight shifts from foot to foot: about one every ten seconds, leaning up to this many
/// radians over the standing foot and moving the hips this far across. Our own tuning.
const SHIFTS_PER_SECOND: f32 = 0.05;
const SHIFT_LEAN: f32 = 0.012;
const SHIFT_SWAY: f32 = 0.18;

/// How someone stands still: breathing, and now and then shifting their weight. Each person's
/// rates and phases are their own, from their seed, so a group never moves in step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Idle {
    pub breath_rate: f32,
    pub breath_phase: f32,
    pub shift_rate: f32,
    pub shift_phase: f32,
}

/// Salt for a person's idle stream. Our own.
const IDLE_SALT: u32 = 0x1D1E_0B0D;

impl Idle {
    pub fn of(seed: u32) -> Idle {
        let mut rng = Mulberry32::new(seed ^ IDLE_SALT);
        Idle {
            breath_rate: BREATHS_PER_SECOND * rng.range_f32(0.85, 1.15),
            breath_phase: rng.range_f32(0.0, std::f32::consts::TAU),
            shift_rate: SHIFTS_PER_SECOND * rng.range_f32(0.7, 1.3),
            shift_phase: rng.range_f32(0.0, std::f32::consts::TAU),
        }
    }

    /// The figure's transform at `t` seconds, about its feet: a breath lifts and fills it, and
    /// a weight shift leans it over one foot and moves it across. The shift dwells on each foot
    /// (a sine through a soft clamp) rather than swaying evenly.
    pub fn at(&self, t: f32) -> Transform {
        let tau = std::f32::consts::TAU;
        let breath = (tau * self.breath_rate * t + self.breath_phase).sin() * 0.5 + 0.5;
        let shift = ((tau * self.shift_rate * t + self.shift_phase).sin() * 2.0).clamp(-1.0, 1.0);
        Transform {
            translation: Vec3::new(shift * SHIFT_SWAY, 0.0, 0.0),
            rotation: Quat::from_rotation_z(-shift * SHIFT_LEAN),
            scale: Vec3::new(1.0 + breath * BREATH_FILL, 1.0 + breath * BREATH_LIFT, 1.0 + breath * BREATH_FILL),
        }
    }
}

/// How near the player must come before folk turn to look, in world units (4 m).
pub const LOOK_RANGE: f32 = 40.0;
/// How far a head turns to either side, and tilts up or down, radians (70 and 25 degrees).
/// Our own limits, short of what a neck can do.
const HEAD_YAW_MAX: f32 = 70.0 * std::f32::consts::PI / 180.0;
const HEAD_PITCH_MAX: f32 = 25.0 * std::f32::consts::PI / 180.0;
/// Past this angle off the way they face, someone does not turn to look at all.
const NOTICE_ANGLE: f32 = 110.0 * std::f32::consts::PI / 180.0;
/// How quickly a head turns to its target, per second ([`StableInterpolate`]).
const HEAD_TURN_RATE: f32 = 4.0;
/// Under a hood the whole body turns instead, this share of the way, at most this far.
const BODY_TURN_SHARE: f32 = 0.35;
const BODY_TURN_MAX: f32 = 25.0 * std::f32::consts::PI / 180.0;

/// Where a head at `head` (in its figure's root space, the face +Z) looks to see an eye at
/// `eye` (the same space): `(yaw, pitch)` radians, yaw turning toward +X and pitch up, held to
/// the neck's limits. `None` if the eye is out of [`LOOK_RANGE`] or behind them.
pub fn look_at(head: Vec3, eye: Vec3) -> Option<Vec2> {
    let to = eye - head;
    let across = Vec2::new(to.x, to.z).length();
    if !(1e-3..=LOOK_RANGE).contains(&across) {
        return None;
    }
    let yaw = to.x.atan2(to.z);
    if yaw.abs() > NOTICE_ANGLE {
        return None;
    }
    let pitch = to.y.atan2(across);
    Some(Vec2::new(yaw.clamp(-HEAD_YAW_MAX, HEAD_YAW_MAX), pitch.clamp(-HEAD_PITCH_MAX, HEAD_PITCH_MAX)))
}

/// The head's rotation about the neck for a look of `(yaw, pitch)`: turned, then tipped (a
/// rotation about +X tips the face down, so the pitch goes in negated).
pub fn head_rotation(look: Vec2) -> Quat {
    Quat::from_euler(EulerRot::YXZ, look.x, -look.y, 0.0)
}

/// The world transform of someone standing at `spot`: on the floor, facing the spot's yaw.
/// The body faces +Z and [`FolkSpot::yaw`] is a camera's yaw (0 faces -Z), so it turns a half
/// turn more.
pub fn stand_at(spot: &FolkSpot) -> Transform {
    let at = Vec3::from_array(Level::world(spot.x, spot.y));
    Transform::from_translation(at).with_rotation(Quat::from_rotation_y(spot.yaw + std::f32::consts::PI))
}

/// Spawns every folk spot of the level once, and again only if the spots change.
pub(crate) fn spawn_folk(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    mut figures: ResMut<FolkFigures>,
    old: Query<Entity, With<Denizen>>,
    mut spawner: ItemSpawner,
    mut shown: Local<Option<Vec<FolkSpot>>>,
) {
    // Again only if the spots change, or every folk was cleared away (a new delve).
    if shown.as_deref() == Some(&level.0.folk[..]) && !old.is_empty() {
        return;
    }
    for entity in &old {
        commands.entity(entity).despawn();
    }
    for built in figures.for_spots(&level.0.folk) {
        let (person, figure) = &*built;
        spawn_denizen(&mut commands, &mut spawner, person, figure);
    }
    *shown = Some(level.0.folk.clone());
}

/// Spawns one person: a root at the spot, a [`Stance`] under it carrying the figure's pieces,
/// and the pieces that turn with the head under a pivot at the neck.
pub fn spawn_denizen(commands: &mut Commands, spawner: &mut ItemSpawner, person: &Person, figure: &Figure) -> Entity {
    let (min, max) = figure.bounds().unwrap_or((Vec3::ZERO, Vec3::ZERO));
    let root = commands
        .spawn((
            Name::new(person.name.clone()),
            Denizen { person: person.clone(), radius: footprint(figure), min, max },
            BlocksTheWay(fb_level::FloorShape::Round(footprint(figure))),
            stand_at(&person.spot),
            Visibility::default(),
        ))
        .id();
    let stance = commands.spawn((Stance { idle: Idle::of(person.spot.seed), turn: 0.0 }, Transform::default(), Visibility::default(), ChildOf(root))).id();
    let head = figure
        .pieces
        .iter()
        .any(|p| p.on_head)
        .then(|| commands.spawn((FolkHead::default(), Transform::from_translation(figure.neck), Visibility::default(), ChildOf(stance))).id());
    for piece in figure.pieces.iter().filter(|p| !p.mesh.parts.is_empty()) {
        let mut entity = spawner.spawn(&piece.mesh, &piece.appearance);
        entity.insert(Name::new(piece.name.clone()));
        match head.filter(|_| piece.on_head) {
            Some(head) => entity.insert((Transform::from_translation(-figure.neck), ChildOf(head))),
            None => entity.insert(ChildOf(stance)),
        };
        if piece.lit {
            entity.insert(Propagate(NotShadowCaster));
        }
    }
    if let Some(light) = &figure.light {
        // A little above the flame, so the hand and the stone under it do not burn white.
        // Lit as tuned once it is spawned (`carry::relight_given_lights`).
        commands.spawn((given_light(light.anchor, EXPOSURE, &crate::tuning::LightTuning::default()), Transform::from_translation(light.at + Vec3::Y * HELD_LIGHT_LIFT), ChildOf(stance)));
    }
    root
}

/// Breathes and shifts every figure's weight ([`Idle::at`]), turned toward the player as far
/// as [`Stance::turn`] says.
pub(crate) fn idle_folk(time: Res<Time>, mut stances: Query<(&Stance, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (stance, mut transform) in &mut stances {
        let idle = stance.idle.at(t);
        *transform = Transform { rotation: Quat::from_rotation_y(stance.turn) * idle.rotation, ..idle };
    }
}

/// Turns heads toward a player within [`LOOK_RANGE`] and back once they leave; under a hood,
/// the whole body turns a little instead.
fn turn_heads(
    time: Res<Time>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    folk: Query<(&GlobalTransform, &Children), With<Denizen>>,
    mut stances: Query<(&mut Stance, &Children)>,
    mut heads: Query<(&mut FolkHead, &mut Transform)>,
) {
    let eye = eyes.iter().next().map(GlobalTransform::translation);
    let dt = time.delta_secs();
    for (root, children) in &folk {
        let from_world = root.affine().inverse();
        let eye = eye.map(|eye| from_world.transform_point3(eye));
        for &child in children {
            let Ok((mut stance, parts)) = stances.get_mut(child) else { continue };
            match parts.iter().find(|&e| heads.contains(e)) {
                Some(head) => {
                    let Ok((mut head, mut transform)) = heads.get_mut(head) else { continue };
                    let target = eye.and_then(|eye| look_at(transform.translation, eye)).unwrap_or(Vec2::ZERO);
                    head.look.smooth_nudge(&target, HEAD_TURN_RATE, dt);
                    transform.rotation = head_rotation(head.look);
                }
                None => {
                    let target = eye.and_then(|eye| look_at(Vec3::ZERO, eye)).map_or(0.0, |look| (look.x * BODY_TURN_SHARE).clamp(-BODY_TURN_MAX, BODY_TURN_MAX));
                    stance.turn.smooth_nudge(&target, HEAD_TURN_RATE, dt);
                }
            }
        }
    }
}

/// What the card says about someone looked at.
#[derive(Clone, Debug, PartialEq)]
pub struct FolkCardText {
    pub name: String,
    /// Their folk and faction: "Dwarf of the Ash Choir".
    pub line: String,
    /// What they wear and what they hold, each item's name and rarity.
    pub wears: Vec<(String, Rarity)>,
    pub holds: Vec<(String, Rarity)>,
}

/// The card for `person`: their name, folk and faction (named as `data/factions.json` names
/// it), and what they wear and hold.
pub fn folk_card_text(person: &Person) -> FolkCardText {
    let names = |gear: &mut dyn Iterator<Item = &(Slot, Item)>| gear.map(|(_, item)| (item.title().to_string(), item.rarity)).collect();
    FolkCardText {
        name: person.name.clone(),
        line: format!("{} of {}", person.spot.folk.race().label, person.spot.faction.name()),
        wears: names(&mut person.worn()),
        holds: names(&mut person.held()),
    }
}

/// The folk under the crosshair and in reach ([`REACH`]), if any.
#[derive(Resource, Debug, Default)]
pub struct AimedFolk(pub Option<Entity>);

/// The card over the level for the folk looked at: one on the screen, styled as the item card
/// (`crate::inventory::card`).
#[derive(Component, Debug)]
pub struct FolkCard;

/// Picks the folk under the crosshair within reach, by their figure's bounds, with the same
/// buffer round the crosshair as items (`crate::carry::nearest_aimed`).
fn aim_at_folk(
    tuning: Res<crate::tuning::Tuning>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    folk: Query<(Entity, &GlobalTransform, &Denizen)>,
    mut aimed: ResMut<AimedFolk>,
) {
    let hit = eyes.iter().next().and_then(|eye| {
        let targets = folk.iter().map(|(entity, at, denizen)| (entity, at.compute_transform(), denizen.min, denizen.max));
        nearest_aimed(eye.translation(), eye.forward().as_vec3(), REACH, AimBuffer::of(&tuning), targets).map(|(entity, _)| entity)
    });
    if aimed.0 != hit {
        aimed.0 = hit;
    }
}

/// Spawns the folk card, hidden, once there is a camera drawing over the held torch to draw
/// it (as the item card is).
fn spawn_folk_card(mut commands: Commands, cards: Query<(), With<FolkCard>>, cameras: Query<Entity, With<ViewModelCamera>>) {
    if !cards.is_empty() {
        return;
    }
    let Some(camera) = cameras.iter().next() else { return };
    commands.spawn((
        Name::new("Folk card"),
        DespawnOnExit(crate::launch::AppState::Delving),
        FolkCard,
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            width: px(CARD_WIDTH),
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            padding: UiRect::new(px(14), px(14), px(10), px(12)),
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

/// Fills the card: the name in the display face, folk and faction under it, then what they
/// wear and hold, each item's name in its rarity's colour.
fn fill_folk_card(commands: &mut Commands, card: Entity, text: &FolkCardText, fonts: &UiFonts) {
    commands.entity(card).despawn_children().with_children(|card| {
        card.spawn((Text::new(text.name.clone()), UiFonts::style(&fonts.display, 22.0, INK)));
        card.spawn((Text::new(text.line.clone()), UiFonts::style(&fonts.body_italic, 15.0, MUTED)));
        for (heading, items) in [("Wears", &text.wears), ("Holds", &text.holds)] {
            if items.is_empty() {
                continue;
            }
            card.spawn((Node { margin: UiRect::top(px(4)), ..default() }, Text::new(heading), UiFonts::style(&fonts.mono, 12.0, MUTED)));
            card.spawn((Text::default(), UiFonts::style(&fonts.body, 14.0, INK))).with_children(|line| {
                for (i, (name, rarity)) in items.iter().enumerate() {
                    if i > 0 {
                        line.spawn((TextSpan::new(" \u{b7} "), UiFonts::style(&fonts.body, 14.0, FAINT.mix(&MUTED, 0.5))));
                    }
                    line.spawn((TextSpan::new(name.clone()), UiFonts::style(&fonts.body, 14.0, rarity_color(*rarity))));
                }
            });
        }
    });
}

/// Shows the card for the folk looked at, under the crosshair, while the level has focus and
/// no item card is up. A card filled this frame waits a frame to be measured.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn show_folk_card(
    mut commands: Commands,
    open: Option<Res<ScreenOpen>>,
    aimed: Res<AimedFolk>,
    aimed_item: Option<Res<Aimed>>,
    folk: Query<&Denizen>,
    fonts: Res<UiFonts>,
    mut cards: Query<(Entity, &mut Node, &mut Visibility, &ComputedNode, &ComputedUiRenderTargetInfo), With<FolkCard>>,
    mut shown: Local<Option<Entity>>,
) {
    let screen_open = open.is_some_and(|o| o.0.is_some());
    let item_card = aimed_item.is_some_and(|a| a.0.is_some());
    let subject = aimed.0.filter(|_| !screen_open && !item_card).and_then(|e| folk.get(e).ok().map(|d| (e, d)));
    for (card, mut node, mut visibility, computed, target) in &mut cards {
        let Some((entity, denizen)) = subject else {
            *visibility = Visibility::Hidden;
            *shown = None;
            continue;
        };
        if *shown != Some(entity) {
            fill_folk_card(&mut commands, card, &folk_card_text(&denizen.person), &fonts);
            *shown = Some(entity);
            *visibility = Visibility::Hidden;
            continue;
        }
        let size = computed.size * computed.inverse_scale_factor;
        let screen = target.logical_size();
        let at = place_under(screen * 0.5, size, screen);
        node.left = px(at.x);
        node.top = px(at.y);
        *visibility = Visibility::Inherited;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folk_1207() -> Vec<FolkSpot> {
        fb_level::generate(1207).folk
    }

    #[test]
    fn the_same_spot_is_the_same_person() {
        for spot in folk_1207() {
            assert_eq!(Person::of(&spot), Person::of(&spot));
        }
    }

    #[test]
    fn everyone_on_1207_is_dressed_and_holds_something() {
        for spot in folk_1207() {
            let person = Person::of(&spot);
            let figure = Figure::dress(&person.spec, &person.gear, person.dirt);
            let names: Vec<&str> = figure.pieces.iter().map(|p| p.name.as_str()).collect();
            let holds = person.held().filter(|(_, item)| names.contains(&item.title())).count();
            assert!(holds > 0, "{} holds nothing: {names:?}", person.name);
            assert!(person.worn().any(|(_, item)| names.contains(&item.title())), "{} wears nothing: {names:?}", person.name);
            assert!(figure.pieces.iter().any(|p| p.on_head), "{}: nothing turns with the head", person.name);
            // Standing on the floor, about as tall as their folk.
            let (lo, hi) = figure.bounds().unwrap();
            assert!(lo.y > -0.5 && lo.y < 0.5, "{}: lowest point {}", person.name, lo.y);
            assert!(hi.y > spot.folk.race().h * 0.85, "{}: {} tall", person.name, hi.y);
        }
    }

    #[test]
    fn faction_outfits_name_real_outfits() {
        for row in fb_level::FACTIONS {
            for key in row.outfits {
                assert!(fb_items::outfit::outfit(key).is_some(), "{}: {key}", row.name);
            }
        }
    }

    #[test]
    fn figures_are_built_once_per_spot() {
        let spots = folk_1207();
        let mut figures = FolkFigures::default();
        let first = figures.for_spots(&spots[..2]);
        let again = figures.for_spots(&spots[..2]);
        assert!(first.iter().zip(&again).all(|(a, b)| Arc::ptr_eq(a, b)));
        assert_eq!(first[0].0.spot, spots[0]);
    }

    #[test]
    fn folk_stand_on_their_spot_facing_its_yaw() {
        let spot = folk_1207()[0];
        let t = stand_at(&spot);
        assert_eq!(t.translation, Vec3::new(spot.x * fb_level::CELL, 0.0, spot.y * fb_level::CELL));
        // The body's face (+Z) points the way the spot faces.
        let face = t.rotation * Vec3::Z;
        let (dx, dy) = spot.facing();
        assert!(face.abs_diff_eq(Vec3::new(dx, 0.0, dy), 1e-5), "{face} vs ({dx}, {dy})");
    }

    #[test]
    fn idle_motion_is_subtle_and_each_persons_own() {
        let idles: Vec<Idle> = folk_1207().iter().map(|s| Idle::of(s.seed)).collect();
        for (i, idle) in idles.iter().enumerate() {
            let frames: Vec<Transform> = (0..600).map(|k| idle.at(k as f32 * 0.1)).collect();
            // It moves...
            let lift = frames.iter().map(|t| t.scale.y).fold(f32::MIN, f32::max) - frames.iter().map(|t| t.scale.y).fold(f32::MAX, f32::min);
            let sway = frames.iter().map(|t| t.translation.x).fold(f32::MIN, f32::max) - frames.iter().map(|t| t.translation.x).fold(f32::MAX, f32::min);
            assert!(lift > 0.003 && sway > 0.1, "{i}: lift {lift} sway {sway}");
            // ...but only a little: under 2 cm across and a degree of lean, the feet staying put.
            for t in &frames {
                assert!(t.translation.length() <= 0.2 && t.translation.y == 0.0, "{t:?}");
                assert!(t.rotation.angle_between(Quat::IDENTITY) < 1f32.to_radians());
                assert!((t.scale - Vec3::ONE).abs().max_element() < 0.01);
            }
            for other in &idles[i + 1..] {
                assert_ne!(idle, other);
                assert!((1..50).any(|k| idle.at(k as f32 * 0.37) != other.at(k as f32 * 0.37)), "two folk move in step");
            }
        }
    }

    #[test]
    fn heads_look_toward_someone_near_in_front() {
        let head = Vec3::new(0.0, 14.0, 0.0);
        // Ahead and to the right (+X), at eye height: turns right, looks a little up.
        let look = look_at(head, Vec3::new(10.0, 16.0, 20.0)).unwrap();
        assert!(look.x > 0.3 && look.x < 0.6 && look.y > 0.0, "{look}");
        // Far off, or right behind: no look.
        assert_eq!(look_at(head, Vec3::new(0.0, 16.0, 60.0)), None);
        assert_eq!(look_at(head, Vec3::new(0.0, 16.0, -20.0)), None);
        // Off to the side the turn is held to the neck's limit.
        let side = look_at(head, Vec3::new(20.0, 14.0, -2.0)).unwrap();
        assert!((side.x - HEAD_YAW_MAX).abs() < 1e-5, "{side}");
        // A head turned right by a positive yaw: its face points toward +X.
        assert!((head_rotation(Vec2::new(0.5, 0.0)) * Vec3::Z).x > 0.4);
        assert!((head_rotation(Vec2::new(0.0, 0.3)) * Vec3::Z).y > 0.2, "a positive pitch looks up");
    }

    /// An app with one stand-in figure (no meshes) facing +Z from the origin, and a camera.
    fn looking_app(eye: Vec3) -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(50)))
            .init_resource::<crate::play::Obstacles>()
            .add_systems(Update, turn_heads)
            .add_systems(PostUpdate, crate::play::gather_obstacles.after(TransformSystems::Propagate));
        let person = Person::of(&folk_1207()[0]);
        let denizen = Denizen { person, radius: 2.5, min: Vec3::new(-2.0, 0.0, -2.0), max: Vec3::new(2.0, 17.0, 2.0) };
        let root = app.world_mut().spawn((denizen, BlocksTheWay(fb_level::FloorShape::Round(2.5)), Transform::default())).id();
        let stance = app.world_mut().spawn((Stance { idle: Idle::of(1), turn: 0.0 }, Transform::default(), ChildOf(root))).id();
        let head = app.world_mut().spawn((FolkHead::default(), Transform::from_xyz(0.0, 14.0, 0.0), ChildOf(stance))).id();
        let camera = app.world_mut().spawn((PlayerCamera, Transform::from_translation(eye))).id();
        (app, head, camera)
    }

    fn settle(app: &mut App) {
        // Four and a half seconds at 20 frames a second.
        for _ in 0..90 {
            app.update();
        }
    }

    #[test]
    fn heads_turn_toward_a_near_player_and_back() {
        let (mut app, head, camera) = looking_app(Vec3::new(15.0, 16.0, 25.0));
        settle(&mut app);
        let look = app.world().get::<FolkHead>(head).unwrap().look;
        assert!(look.x > 0.1, "turned toward the player: {look}");
        let face = app.world().get::<Transform>(head).unwrap().rotation * Vec3::Z;
        assert!(face.x > 0.1, "{face}");
        // The player walks off: the head comes back round.
        app.world_mut().get_mut::<Transform>(camera).unwrap().translation = Vec3::new(200.0, 16.0, 0.0);
        settle(&mut app);
        let look = app.world().get::<FolkHead>(head).unwrap().look;
        assert!(look.length() < 0.05, "back to straight ahead: {look}");
    }

    #[test]
    fn the_card_names_the_person_their_faction_and_their_gear() {
        for spot in folk_1207() {
            let person = Person::of(&spot);
            let card = folk_card_text(&person);
            assert_eq!(card.name, person.name);
            let faction = &fb_items::tables::factions()[spot.faction.0 as usize];
            assert!(card.line.ends_with(faction.as_str()), "{}", card.line);
            assert!(card.line.starts_with(spot.folk.race().label.as_str()), "{}", card.line);
            assert_eq!(card.wears.len() + card.holds.len(), person.gear.len());
            assert!(!card.holds.is_empty());
            for (name, rarity) in card.wears.iter().chain(&card.holds) {
                assert!(person.gear.iter().any(|(_, item)| item.title() == name && item.rarity == *rarity), "{name}");
            }
        }
    }

    #[test]
    fn the_card_shows_only_within_reach() {
        let (mut app, _, camera) = looking_app(Vec3::new(0.0, 16.0, 30.0));
        app.init_resource::<AimedFolk>().init_resource::<crate::tuning::Tuning>().add_systems(Update, aim_at_folk);
        let aim = |app: &mut App, eye: Vec3| {
            *app.world_mut().get_mut::<Transform>(camera).unwrap() = Transform::from_translation(eye).looking_at(Vec3::new(0.0, 12.0, 0.0), Vec3::Y);
            app.update();
            app.update();
            app.world().resource::<AimedFolk>().0
        };
        // A metre and a half off, looking at them: in reach.
        assert!(aim(&mut app, Vec3::new(0.0, 16.0, 15.0)).is_some());
        // Four metres off, looking at them: out of reach.
        assert_eq!(aim(&mut app, Vec3::new(0.0, 16.0, 40.0)), None);
        // Near, but looking away.
        *app.world_mut().get_mut::<Transform>(camera).unwrap() = Transform::from_xyz(0.0, 16.0, 15.0).looking_at(Vec3::new(0.0, 16.0, 40.0), Vec3::Y);
        app.update();
        app.update();
        assert_eq!(app.world().resource::<AimedFolk>().0, None);
    }

    #[test]
    fn folk_block_the_way_where_they_stand() {
        let (mut app, _, _) = looking_app(Vec3::new(0.0, 16.0, 30.0));
        app.update();
        app.update();
        assert_eq!(app.world().resource::<crate::play::Obstacles>().0, vec![crate::play::Obstacle::Circle { centre: Vec2::ZERO, radius: 2.5 }]);
        // And on 1207 every folk stands in a circle wide enough to bump into.
        for spot in folk_1207().iter().take(2) {
            let person = Person::of(spot);
            let radius = footprint(&Figure::dress(&person.spec, &person.gear, person.dirt));
            assert!(radius > 1.5 && radius < 5.0, "{radius}");
        }
    }

    /// Most of a part's surface, by area, that may wind against its normals: smooth normals
    /// averaged across a fold or a seam lean past a few thin faces, but an inverted part is
    /// nearly all of it. Our own allowance: on seeds 1 to 16 and 1207 the worst part winds
    /// 8.5% of its area against, and the worst ink hull 7.1%.
    const MOST_WOUND_AGAINST: f32 = 0.25;

    /// The share of a part's surface, by area, whose winding faces against its shading normals
    /// (`Part::indices` are counter-clockwise front faces), and the same for its outline normals.
    fn wound_against(part: &armory::Part) -> (f32, f32) {
        let (mut total, mut shading, mut outline) = (0.0, 0.0, 0.0);
        let at = |i: u32| Vec3::from_array(part.positions[i as usize]);
        let sum = |set: &[[f32; 3]], tri: &[u32]| tri.iter().map(|&i| Vec3::from_array(set[i as usize])).sum::<Vec3>();
        for tri in part.indices.as_chunks::<3>().0 {
            let face = (at(tri[1]) - at(tri[0])).cross(at(tri[2]) - at(tri[0]));
            let area = face.length() * 0.5;
            total += area;
            if face.dot(sum(&part.normals, tri)) <= 0.0 {
                shading += area;
            }
            if face.dot(sum(&part.outline_normals, tri)) <= 0.0 {
                outline += area;
            }
        }
        if total > 0.0 { (shading / total, outline / total) } else { (0.0, 0.0) }
    }

    fn every_fitted_folk_part_faces_out_and_is_coloured_check(seeds: impl IntoIterator<Item = u32>, people: usize) {
        // Seeds 1 to 16 and 1207: every person standing about, dressed as drawn. A part wound
        // inside out (its front faces turned in, or its ink hull pushed in over it), with broken
        // buffers, or with a black or non-finite colour would draw as unlit solid black (#151).
        for seed in seeds {
            for spot in fb_level::generate(seed).folk.into_iter().take(people) {
                let person = Person::of(&spot);
                let figure = Figure::dress(&person.spec, &person.gear, person.dirt);
                for piece in &figure.pieces {
                    for part in &piece.mesh.parts {
                        let at = format!("seed {seed}, {} ({:?}): {} / {}", person.name, spot.folk, piece.name, part.name);
                        part.validate().unwrap_or_else(|e| panic!("{at}: {e}"));
                        let (shading, outline) = wound_against(part);
                        assert!(shading < MOST_WOUND_AGAINST, "{at}: {:.1}% of it winds against its normals", shading * 100.0);
                        assert!(outline < MOST_WOUND_AGAINST, "{at}: {:.1}% of its ink hull pushes inward", outline * 100.0);
                        let colour = piece.appearance.material_for(part).base.base_color.to_linear();
                        let lum = colour.red * 0.2126 + colour.green * 0.7152 + colour.blue * 0.0722;
                        assert!(lum.is_finite() && lum > 0.002, "{at}: drawn {colour:?}");
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "slow sweep: folk body fitting test, seeds 1 to 16 and 1207; run before merging fitting changes"]
    fn every_fitted_folk_part_faces_out_and_is_coloured() {
        every_fitted_folk_part_faces_out_and_is_coloured_check((1..=16).chain([1207]), usize::MAX);
    }

    #[test]
    fn every_fitted_folk_part_faces_out_and_is_coloured_smoke() {
        every_fitted_folk_part_faces_out_and_is_coloured_check([1207], 1);
    }
}
