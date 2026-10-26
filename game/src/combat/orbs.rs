//! The life, stamina and mana orbs along the bottom left of the view, drawn into the vitals HUD
//! ([`super::vitals`]) so they hide under screens and go with the delve.
//!
//! Each is a glass globe in a gold frame that drains like a flask: red life (the delver's
//! [`Health`]), yellow stamina and blue mana ([`DelverStamina`], [`DelverMana`]). The orb is
//! drawn by one shader ([`OrbMaterial`], `orb.wesl`): the liquid's surface sloshes on its own
//! (from the shader's clock, so nothing is rewritten each frame), and the fill the shader is
//! given eases toward the real value, written only while it is moving. A winded delver's stamina
//! orb darkens, and a poisoned delver's life turns green ([`POISONED_LIQUIDS`]), easing back to
//! red when the poison wears off.
//!
//! The layout follows the sketch, as one cluster anchored at the bottom left ([`OrbCluster`]):
//! the large life orb, the small stamina orb on its rim up and to the right, the visibility eye
//! on its rim straight right, the mana orb on its rim down and to the right, and the quickslots
//! running right from the eye. Everything is positioned from one pure function
//! ([`orb_cluster_layout`]) in HUD units ([`crate::ui::hud`], hundredths of the reference
//! view's height), so the cluster scales with the window on the shared interface scale.
//!
//! The life orb pulses when the delver is struck: its frame and liquid flash toward a hot red
//! and white and it swells for a moment ([`Pulse`]), harder for a bigger share of their hit
//! points taken, as the flash over the view is ([`super::vitals::flash_strength`]) and as well as
//! it.

use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::{Shader, ShaderRef, load_shader_library},
};

use super::Health;
use super::vitals::{FLASH_CAP, VitalsRoot, flash_strength};
use crate::character::{DelverEffects, DelverMana, DelverStamina};
use crate::play::{DelveSystems, Player};
use crate::ui::{FRAME_GOLD, hud};

const SHADER_PATH: &str = "embedded://forever_below/combat/orb.wesl";

pub struct OrbPlugin;

impl Plugin for OrbPlugin {
    fn build(&self, app: &mut App) {
        // Without a renderer (the logic tests) there are no shaders to load, only the material
        // asset for the systems to write, and with no assets at all (the combat tests) no orbs.
        if app.world().contains_resource::<Assets<Shader>>() {
            load_shader_library!(app, "orb.wesl");
            app.add_plugins(UiMaterialPlugin::<OrbMaterial>::default());
        } else if app.world().contains_resource::<AssetServer>() {
            app.init_asset::<OrbMaterial>();
        }
        app.add_systems(
            Update,
            (spawn_cluster, spawn_orbs, read_vitals, settle_orbs)
                .chain()
                .in_set(DelveSystems),
        );
    }
}

/// Which orb.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OrbKind {
    Life,
    Stamina,
    Mana,
}

impl OrbKind {
    pub const ALL: [OrbKind; 3] = [OrbKind::Life, OrbKind::Stamina, OrbKind::Mana];

    /// The liquid at its surface and at the bottom of the globe, as sRGB.
    fn liquids(self) -> (Color, Color) {
        match self {
            OrbKind::Life => (Color::srgb(0.93, 0.24, 0.19), Color::srgb(0.36, 0.03, 0.05)),
            OrbKind::Stamina => (Color::srgb(0.98, 0.83, 0.28), Color::srgb(0.52, 0.34, 0.04)),
            OrbKind::Mana => (Color::srgb(0.38, 0.64, 1.0), Color::srgb(0.07, 0.14, 0.46)),
        }
    }

    /// The liquid as it is drawn, `poison` of the way (0 to 1) from its own colours to
    /// [`POISONED_LIQUIDS`]. Only the life orb turns.
    pub fn liquids_at(self, poison: f32) -> (LinearRgba, LinearRgba) {
        let (surface, deep) = self.liquids();
        let (surface, deep): (LinearRgba, LinearRgba) = (surface.into(), deep.into());
        if self != OrbKind::Life || poison <= 0.0 {
            return (surface, deep);
        }
        let (green, dark) = POISONED_LIQUIDS;
        let t = poison.clamp(0.0, 1.0);
        (surface.mix(&green.into(), t), deep.mix(&dark.into(), t))
    }

    /// Where in its slosh the orb starts, so the three do not move together.
    fn phase(self) -> f32 {
        match self {
            OrbKind::Life => 0.0,
            OrbKind::Stamina => 2.1,
            OrbKind::Mana => 4.2,
        }
    }
}

/// A poisoned delver's life liquid, at its surface and at the bottom: a venom green. Our own.
pub const POISONED_LIQUIDS: (Color, Color) =
    (Color::srgb(0.46, 0.86, 0.22), Color::srgb(0.07, 0.30, 0.05));
/// How quickly the life orb turns green or back, per second: about a second either way. Our own.
const POISON_RATE: f32 = 3.0;

/// The life orb's diameter HUD units ([`crate::ui::hud`]), which sizes the whole cluster.
pub const LIFE_DIAMETER: f32 = 14.0;
/// The space between the cluster and the view's bottom left corner, HUD units ([`crate::ui::hud`]).
pub const MARGIN: f32 = 2.0;
/// The stamina orb, the eye and the mana orb are this share of the life orb's diameter across.
pub const SMALL_SHARE: f32 = 0.38;
/// How tall the eye's box is, as a share of its width.
pub const EYE_ASPECT: f32 = 0.65;
/// How tall a quickslot is, as a share of the life orb's diameter.
pub const SLOT_SHARE: f32 = 0.38;
/// A status badge's diameter, as a share of the life orb's diameter. Our own.
pub const BADGE_SHARE: f32 = 0.22;

/// An orb: its centre and radius, HUD units ([`crate::ui::hud`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Disc {
    pub centre: Vec2,
    pub radius: f32,
}

impl Disc {
    /// The square the disc fits.
    pub fn square(&self) -> Rect {
        Rect::from_center_half_size(self.centre, Vec2::splat(self.radius))
    }

    /// How far its edge is from `point`: negative inside it.
    pub fn gap_to(&self, point: Vec2) -> f32 {
        self.centre.distance(point) - self.radius
    }
}

/// Where everything in the cluster of orbs, eye and quickslots goes, HUD units ([`crate::ui::hud`]) from the
/// cluster's top left corner, down and right positive ([`orb_cluster_layout`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClusterLayout {
    /// The cluster's size: it bounds everything in it.
    pub size: Vec2,
    pub life: Disc,
    pub stamina: Disc,
    pub mana: Disc,
    pub eye: Rect,
    /// The row of quickslots.
    pub bar: Rect,
    /// A quickslot's side, and the gap between them.
    pub slot: f32,
    pub gap: f32,
    /// The row of status badges, just above the left end of the bar (`crate::status`).
    pub status: Rect,
    /// A status badge's diameter.
    pub badge: f32,
}

/// The cluster for a life orb `d` across, as the sketch has it: the life orb large at the
/// bottom left; the stamina orb with its centre on the life orb's rim at 45 degrees up and to the
/// right; the visibility eye with its centre on the rim straight right; the mana orb with its
/// centre on the rim at 45 degrees down and to the right, each about [`SMALL_SHARE`] of `d`
/// across, so they overlap the life orb's frame; and the quickslot bar starting just right of the
/// eye, level with it, [`SLOT_SHARE`] of `d` tall; and the row of status badges, [`BADGE_SHARE`]
/// of `d` across, sitting just above the bar from its left end. Everything scales with
/// `d`, so it stays a cluster at any window size.
pub fn orb_cluster_layout(d: f32) -> ClusterLayout {
    let r = d * 0.5;
    let small = d * SMALL_SHARE;
    // A point on the life orb's rim at `degrees` above straight right.
    let rim = |degrees: f32| {
        let angle = degrees.to_radians();
        Vec2::new(r + r * angle.cos(), r - r * angle.sin())
    };
    let life = Disc {
        centre: Vec2::splat(r),
        radius: r,
    };
    let stamina = Disc {
        centre: rim(45.0),
        radius: small * 0.5,
    };
    let mana = Disc {
        centre: rim(-45.0),
        radius: small * 0.5,
    };
    let eye_centre = rim(0.0);
    let eye = Rect::from_center_size(eye_centre, Vec2::new(small, small * EYE_ASPECT));
    let slot = d * SLOT_SHARE;
    let gap = slot * 0.12;
    let bar_left = eye.max.x + d * 0.04;
    let slots = crate::quickslots::SLOTS as f32;
    let bar = Rect::new(
        bar_left,
        eye_centre.y - slot * 0.5,
        bar_left + slots * slot + (slots - 1.0) * gap,
        eye_centre.y + slot * 0.5,
    );
    let badge = d * BADGE_SHARE;
    let status = Rect::new(
        bar.min.x,
        bar.min.y - gap - badge,
        bar.max.x,
        bar.min.y - gap,
    );
    // The small orbs poke a little out of the life orb's box, above and below: the cluster is
    // the box round all of it.
    let top = stamina
        .square()
        .min
        .y
        .min(eye.min.y)
        .min(status.min.y)
        .min(0.0);
    let bottom = mana.square().max.y.max(bar.max.y).max(d);
    let by = Vec2::new(0.0, -top);
    let shift = |rect: Rect| Rect {
        min: rect.min + by,
        max: rect.max + by,
    };
    let disc = |disc: Disc| Disc {
        centre: disc.centre + by,
        ..disc
    };
    ClusterLayout {
        size: Vec2::new(bar.max.x, bottom - top),
        life: disc(life),
        stamina: disc(stamina),
        mana: disc(mana),
        eye: shift(eye),
        bar: shift(bar),
        slot,
        gap,
        status: shift(status),
        badge,
    }
}

/// The cluster as the game draws it.
pub fn cluster_layout() -> ClusterLayout {
    orb_cluster_layout(LIFE_DIAMETER)
}

/// The container the orbs, the eye and the quickslot bar are positioned in: anchored at the
/// bottom left of the view and sized by [`cluster_layout`].
#[derive(Component, Debug)]
pub struct OrbCluster;

/// An orb on the HUD.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Orb {
    pub kind: OrbKind,
    /// How full the orb should be, 0 to 1; none until the delver has been read.
    pub target: Option<f32>,
    /// Whether the orb is darkened (a winded delver's stamina).
    pub dim: bool,
    /// Whether the delver is poisoned, which turns the life orb green.
    pub poisoned: bool,
    /// How green the life orb is drawn now, 0 to 1, easing toward `poisoned`.
    pub poison: f32,
    /// Whether the next settle jumps to the target instead of easing (the first reading).
    snap: bool,
    /// The pulse of a blow, on the life orb.
    pub pulse: Pulse,
    /// The hit points last read, to see them fall.
    last_hp: Option<i32>,
}

impl Orb {
    pub fn new(kind: OrbKind) -> Orb {
        Orb {
            kind,
            target: None,
            dim: false,
            poisoned: false,
            poison: 0.0,
            snap: false,
            pulse: Pulse::default(),
            last_hp: None,
        }
    }
}

/// How long the life orb's pulse lasts, seconds. Our own.
pub const PULSE_TIME: f32 = 0.25;
/// How much bigger the life orb swells at the hardest pulse: 1.08 times its size. Our own.
pub const PULSE_SWELL: f32 = 0.08;

/// A blow's pulse on the life orb: how strong it began (a [`flash_strength`]) and how many
/// seconds it has left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pulse {
    pub strength: f32,
    pub left: f32,
}

impl Pulse {
    /// How hot the orb is now, 0 to 1: the blow's strength as a share of the hardest flash,
    /// easing out to nothing over [`PULSE_TIME`].
    pub fn heat(&self) -> f32 {
        let left = (self.left / PULSE_TIME).clamp(0.0, 1.0);
        (self.strength / FLASH_CAP).clamp(0.0, 1.0) * left * left
    }

    /// How much the orb is swollen now: 1 at rest, [`PULSE_SWELL`] more at the hardest.
    pub fn scale(&self) -> f32 {
        1.0 + PULSE_SWELL * self.heat()
    }

    /// A blow of `strength` pulses afresh unless a harder pulse is still showing.
    fn strike(&mut self, strength: f32) {
        if (strength / FLASH_CAP).clamp(0.0, 1.0) >= self.heat() {
            *self = Pulse {
                strength,
                left: PULSE_TIME,
            };
        }
    }
}

/// How the orb is drawn (`orb.wesl`).
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct OrbMaterial {
    #[uniform(0)]
    pub style: OrbStyle,
}

#[derive(ShaderType, Clone, Copy, Debug, PartialEq)]
pub struct OrbStyle {
    pub surface: LinearRgba,
    pub deep: LinearRgba,
    pub gold: LinearRgba,
    pub gold_dark: LinearRgba,
    pub fill: f32,
    pub phase: f32,
    pub slosh: f32,
    pub dim: f32,
    /// How hot the frame and liquid are, from 0 to 1, in a pulse.
    pub pulse: f32,
}

impl OrbStyle {
    /// A full orb of `kind`.
    pub fn new(kind: OrbKind) -> OrbStyle {
        let (surface, deep) = kind.liquids();
        OrbStyle {
            surface: surface.into(),
            deep: deep.into(),
            gold: FRAME_GOLD.into(),
            gold_dark: Color::srgb(0.30, 0.21, 0.08).into(),
            fill: 1.0,
            phase: kind.phase(),
            slosh: 0.035,
            dim: 0.0,
            pulse: 0.0,
        }
    }
}

impl UiMaterial for OrbMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }
}

/// How dark a winded delver's stamina orb goes, 0 to 1.
const DIMMED: f32 = 1.0;
/// How quickly the shown fill closes on the real one, per second.
const EASE_RATE: f32 = 7.0;
/// Closer than this the shown fill is the real one.
const SETTLED: f32 = 0.002;

/// Gives each new vitals HUD the container of the cluster ([`OrbCluster`]).
fn spawn_cluster(mut commands: Commands, roots: Query<Entity, Added<VitalsRoot>>) {
    for root in &roots {
        let size = cluster_layout().size;
        commands.entity(root).with_child((
            Name::new("Orb cluster"),
            OrbCluster,
            Node {
                position_type: PositionType::Absolute,
                left: hud(MARGIN),
                bottom: hud(MARGIN),
                width: hud(size.x),
                height: hud(size.y),
                ..default()
            },
            Pickable::IGNORE,
        ));
    }
}

/// Gives each new cluster its three orbs, positioned from the layout.
fn spawn_orbs(
    mut commands: Commands,
    clusters: Query<Entity, Added<OrbCluster>>,
    materials: Option<ResMut<Assets<OrbMaterial>>>,
) {
    let Some(mut materials) = materials else {
        return;
    };
    let layout = cluster_layout();
    for cluster in &clusters {
        commands.entity(cluster).with_children(|cluster| {
            for (kind, disc) in [
                (OrbKind::Life, layout.life),
                (OrbKind::Stamina, layout.stamina),
                (OrbKind::Mana, layout.mana),
            ] {
                let square = disc.square();
                cluster.spawn((
                    Name::new(format!("{kind:?} orb")),
                    Orb::new(kind),
                    MaterialNode(materials.add(OrbMaterial {
                        style: OrbStyle::new(kind),
                    })),
                    Node {
                        position_type: PositionType::Absolute,
                        left: hud(square.min.x),
                        top: hud(square.min.y),
                        width: hud(disc.radius * 2.0),
                        height: hud(disc.radius * 2.0),
                        ..default()
                    },
                    UiTransform::IDENTITY,
                    Pickable::IGNORE,
                ));
            }
        });
    }
}

/// How full each orb should be: life from the delver's hit points, stamina and mana from their
/// pools. Stamina stays full until the delver has one. Orbs are only touched when a value
/// differs.
#[allow(clippy::type_complexity)]
fn read_vitals(
    players: Query<
        (
            Option<&Health>,
            Option<&DelverStamina>,
            Option<&DelverMana>,
            Option<&DelverEffects>,
        ),
        With<Player>,
    >,
    mut orbs: Query<&mut Orb>,
) {
    let Some((health, stamina, mana, effects)) = players.iter().next() else {
        return;
    };
    let poisoned = effects.is_some_and(|e| e.poisoned_hours().is_some());
    for mut orb in &mut orbs {
        let poisoned = poisoned && orb.kind == OrbKind::Life;
        if orb.poisoned != poisoned {
            orb.poisoned = poisoned;
        }
        let (fill, dim) = match orb.kind {
            OrbKind::Life => (
                health.map_or(1.0, |h| h.hp.max(0) as f32 / h.max.max(1) as f32),
                false,
            ),
            OrbKind::Stamina => (
                stamina.map_or(1.0, |s| s.0.value as f32),
                stamina.is_some_and(|s| !s.0.can_exert()),
            ),
            OrbKind::Mana => (mana.map_or(1.0, |m| m.0.fraction() as f32), false),
        };
        let fill = fill.clamp(0.0, 1.0);
        // Any fall in hit points pulses the life orb, harder for a bigger share of the most.
        if orb.kind == OrbKind::Life
            && let Some(health) = health
        {
            if let Some(last) = orb.last_hp
                && health.hp < last
            {
                orb.pulse
                    .strike(flash_strength(last - health.hp, health.max));
            }
            if orb.last_hp != Some(health.hp) {
                orb.last_hp = Some(health.hp);
            }
        }
        if orb.target != Some(fill) || orb.dim != dim {
            if orb.target.is_none() {
                orb.snap = true;
            }
            orb.target = Some(fill);
            orb.dim = dim;
        }
    }
}

/// The fill that is one step of `dt` seconds nearer `target` than `shown`.
pub fn ease(shown: f32, target: f32, dt: f32) -> f32 {
    ease_at(shown, target, dt, EASE_RATE)
}

/// `shown` one step of `dt` seconds nearer `target`, closing at `rate` per second
/// (exponentially), and on it once closer than [`SETTLED`].
pub fn ease_at(shown: f32, target: f32, dt: f32, rate: f32) -> f32 {
    if (target - shown).abs() <= SETTLED {
        return target;
    }
    target + (shown - target) * (-rate * dt).exp()
}

/// Eases each orb's drawn fill toward its target, and sets its darkening and its pulse (the
/// heat in its frame and liquid, and how far it swells). The material and the orb's scale are
/// only written when they must change, so a still orb costs nothing.
fn settle_orbs(
    time: Res<Time>,
    mut orbs: Query<(&mut Orb, &MaterialNode<OrbMaterial>, &mut UiTransform)>,
    materials: Option<ResMut<Assets<OrbMaterial>>>,
) {
    let Some(mut materials) = materials else {
        return;
    };
    let dt = time.delta_secs();
    for (mut orb, handle, mut transform) in &mut orbs {
        if orb.pulse.left > 0.0 {
            orb.pulse.left = (orb.pulse.left - dt).max(0.0);
        }
        let Some(target) = orb.target else { continue };
        let Some(material) = materials.get(&handle.0) else {
            continue;
        };
        let shown = if orb.snap {
            target
        } else {
            ease(material.style.fill, target, dt)
        };
        let dim = if orb.dim { DIMMED } else { 0.0 };
        let heat = orb.pulse.heat();
        let poison = if orb.snap {
            f32::from(u8::from(orb.poisoned))
        } else {
            ease_at(
                orb.poison,
                f32::from(u8::from(orb.poisoned)),
                dt,
                POISON_RATE,
            )
        };
        let recolour = poison != orb.poison;
        if recolour {
            orb.poison = poison;
        }
        if (material.style.fill != shown
            || material.style.dim != dim
            || material.style.pulse != heat
            || recolour)
            && let Some(mut material) = materials.get_mut(&handle.0)
        {
            material.style.fill = shown;
            material.style.dim = dim;
            material.style.pulse = heat;
            (material.style.surface, material.style.deep) = orb.kind.liquids_at(poison);
        }
        let scale = Vec2::splat(orb.pulse.scale());
        if transform.scale != scale {
            transform.scale = scale;
        }
        if orb.snap {
            orb.snap = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::asset::AssetPlugin;
    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;

    use super::*;
    use crate::combat::vitals::VitalsPlugin;
    use crate::combat::{Health, RunDice};
    use crate::launch::AppState;
    use crate::play::ScreenOpen;

    /// A delve with a view model camera, a delver of 12 hit points and the orbs, stepped a 60th of
    /// a second a frame.
    fn delve() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f32(1.0 / 60.0),
            ))
            .insert_state(AppState::Delving)
            .insert_resource(RunDice(procgen_rng::Mulberry32::new(4)))
            .init_resource::<crate::character::Character>()
            .init_resource::<ScreenOpen>()
            .add_plugins((VitalsPlugin, OrbPlugin));
        crate::play::run_delve_systems_while_delving(&mut app);
        let player = app
            .world_mut()
            .spawn((Player, Transform::default(), Health::full(12)))
            .id();
        let follows = app.world_mut().spawn_empty().id();
        app.world_mut()
            .spawn(crate::carry::ViewModelCamera { follows });
        for _ in 0..4 {
            app.update();
        }
        (app, player)
    }

    fn orb(app: &mut App, kind: OrbKind) -> (Orb, OrbStyle) {
        let mut query = app
            .world_mut()
            .query::<(&Orb, &MaterialNode<OrbMaterial>)>();
        let (orb, handle) = query
            .iter(app.world())
            .find(|(o, _)| o.kind == kind)
            .expect("the orb");
        let style = app
            .world()
            .resource::<Assets<OrbMaterial>>()
            .get(&handle.0)
            .expect("its material")
            .style;
        (*orb, style)
    }

    fn run(app: &mut App, frames: usize) {
        for _ in 0..frames {
            app.update();
        }
    }

    #[test]
    fn the_delve_gets_a_life_stamina_and_mana_orb_and_no_hit_point_readout() {
        let (mut app, _) = delve();
        for kind in OrbKind::ALL {
            let (orb, style) = orb(&mut app, kind);
            assert_eq!(orb.target, Some(1.0), "{kind:?}");
            assert_eq!(style.fill, 1.0, "{kind:?} starts full");
        }
        let mut texts = app.world_mut().query::<&Text>();
        assert!(
            texts.iter(app.world()).all(|t| !t.0.starts_with("HP")),
            "the readout is gone"
        );
    }

    #[test]
    fn a_bite_for_a_quarter_of_max_hit_points_drains_the_life_orb_by_a_quarter() {
        let (mut app, player) = delve();
        app.world_mut().get_mut::<Health>(player).unwrap().hurt(3);
        run(&mut app, 2);
        assert_eq!(orb(&mut app, OrbKind::Life).0.target, Some(0.75));
        run(&mut app, 120);
        assert!((orb(&mut app, OrbKind::Life).1.fill - 0.75).abs() < 1e-3);
        assert_eq!(
            orb(&mut app, OrbKind::Stamina).1.fill,
            1.0,
            "the others are unmoved"
        );
        assert_eq!(orb(&mut app, OrbKind::Mana).1.fill, 1.0);
    }

    #[test]
    fn the_orb_drains_by_easing_rather_than_jumping() {
        let (mut app, player) = delve();
        app.world_mut().get_mut::<Health>(player).unwrap().hurt(6);
        run(&mut app, 3);
        let fill = orb(&mut app, OrbKind::Life).1.fill;
        assert!(fill < 1.0 && fill > 0.55, "on its way down: {fill}");
    }

    #[test]
    fn sprinting_drains_the_stamina_orb_and_it_refills_when_the_delver_stops() {
        let (mut app, player) = delve();
        app.world_mut()
            .get_mut::<DelverStamina>(player)
            .unwrap()
            .0
            .value = 0.4;
        run(&mut app, 120);
        assert!((orb(&mut app, OrbKind::Stamina).1.fill - 0.4).abs() < 1e-3);
        app.world_mut()
            .get_mut::<DelverStamina>(player)
            .unwrap()
            .0
            .value = 1.0;
        run(&mut app, 120);
        assert!((orb(&mut app, OrbKind::Stamina).1.fill - 1.0).abs() < 1e-3);
    }

    #[test]
    fn a_winded_delvers_stamina_orb_darkens_and_mana_follows_the_pool() {
        let (mut app, player) = delve();
        app.world_mut().get_mut::<DelverStamina>(player).unwrap().0 = fb_character::Stamina {
            value: 0.0,
            winded: true,
        };
        app.world_mut().get_mut::<DelverMana>(player).unwrap().0 = fb_character::Mana {
            points: 5.0,
            max: 20.0,
        };
        run(&mut app, 120);
        assert_eq!(orb(&mut app, OrbKind::Stamina).1.dim, DIMMED);
        assert_eq!(orb(&mut app, OrbKind::Life).1.dim, 0.0);
        assert!((orb(&mut app, OrbKind::Mana).1.fill - 0.25).abs() < 1e-3);
    }

    #[test]
    fn a_poisoned_delvers_life_orb_turns_green_and_eases_back_to_red() {
        let (mut app, player) = delve();
        let red = orb(&mut app, OrbKind::Life).1.surface;
        app.world_mut()
            .entity_mut(player)
            .insert(DelverEffects::poisoned_for(
                fb_character::rules::defaults(),
                1.0,
            ));
        run(&mut app, 10);
        let turning = orb(&mut app, OrbKind::Life);
        assert!(
            turning.0.poison > 0.0 && turning.0.poison < 1.0,
            "easing, not jumping: {}",
            turning.0.poison
        );
        run(&mut app, 240);
        let green: LinearRgba = POISONED_LIQUIDS.0.into();
        let (poisoned, style) = orb(&mut app, OrbKind::Life);
        assert_eq!((poisoned.poison, style.surface), (1.0, green));
        assert!(
            style.surface.green > style.surface.red,
            "green: {:?}",
            style.surface
        );
        assert_eq!(
            orb(&mut app, OrbKind::Stamina).1.surface,
            OrbKind::Stamina.liquids_at(0.0).0,
            "only the life orb turns"
        );
        app.world_mut().entity_mut(player).remove::<DelverEffects>();
        run(&mut app, 240);
        let (cured, style) = orb(&mut app, OrbKind::Life);
        assert_eq!(
            (cured.poison, style.surface),
            (0.0, red),
            "back to red when it wears off"
        );
    }

    /// How many times the orb materials have been modified.
    #[derive(Resource, Default)]
    struct Rewrites(usize);

    fn count_rewrites(
        mut events: MessageReader<AssetEvent<OrbMaterial>>,
        mut rewrites: ResMut<Rewrites>,
    ) {
        rewrites.0 += events
            .read()
            .filter(|e| matches!(e, AssetEvent::Modified { .. }))
            .count();
    }

    #[test]
    fn a_still_orb_leaves_its_material_alone_and_a_moving_one_writes_it() {
        let (mut app, player) = delve();
        app.init_resource::<Rewrites>()
            .add_systems(Last, count_rewrites);
        run(&mut app, 120);
        let settled = app.world().resource::<Rewrites>().0;
        run(&mut app, 60);
        assert_eq!(
            app.world().resource::<Rewrites>().0,
            settled,
            "nothing rewritten while nothing moves"
        );
        app.world_mut().get_mut::<Health>(player).unwrap().hurt(3);
        run(&mut app, 120);
        assert!(
            app.world().resource::<Rewrites>().0 > settled,
            "the draining orb is written"
        );
        let after = app.world().resource::<Rewrites>().0;
        run(&mut app, 60);
        assert_eq!(
            app.world().resource::<Rewrites>().0,
            after,
            "and stops once it has settled"
        );
    }

    #[test]
    fn the_orbs_are_part_of_the_hud_cluster_and_hide_under_screens() {
        use crate::carry::{Hud, hide_hud_under_screens};
        let (mut app, _) = delve();
        app.add_systems(Update, hide_hud_under_screens);
        let mut query = app.world_mut().query_filtered::<&ChildOf, With<Orb>>();
        let clusters: Vec<Entity> = query.iter(app.world()).map(|c| c.parent()).collect();
        assert_eq!(clusters.len(), 3);
        assert!(
            clusters
                .iter()
                .all(|&c| c == clusters[0] && app.world().get::<OrbCluster>(c).is_some()),
            "all in the one cluster"
        );
        let hud = app.world().get::<ChildOf>(clusters[0]).unwrap().parent();
        assert!(app.world().get::<Hud>(hud).is_some());
        assert!(
            app.world().get::<DespawnOnExit<AppState>>(hud).is_some(),
            "scoped to the delve"
        );
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(crate::play::Screen::Inventory);
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(hud).unwrap(),
            Visibility::Hidden
        );
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(hud).unwrap(),
            Visibility::Inherited
        );
    }

    #[test]
    fn the_orbs_sit_in_the_cluster_where_the_layout_puts_them() {
        let layout = cluster_layout();
        let (mut app, _) = delve();
        let mut query = app.world_mut().query::<(&Orb, &Node)>();
        for (orb, node) in query.iter(app.world()) {
            let disc = match orb.kind {
                OrbKind::Life => layout.life,
                OrbKind::Stamina => layout.stamina,
                OrbKind::Mana => layout.mana,
            };
            let square = disc.square();
            assert_eq!(
                (node.left, node.top),
                (hud(square.min.x), hud(square.min.y)),
                "{:?}",
                orb.kind
            );
            assert_eq!(
                (node.width, node.height),
                (hud(disc.radius * 2.0), hud(disc.radius * 2.0))
            );
        }
        let mut cluster = app.world_mut().query_filtered::<&Node, With<OrbCluster>>();
        let node = cluster.single(app.world()).unwrap();
        assert_eq!(
            (node.left, node.bottom, node.width, node.height),
            (
                hud(MARGIN),
                hud(MARGIN),
                hud(layout.size.x),
                hud(layout.size.y)
            ),
            "anchored bottom left"
        );
    }

    #[test]
    fn the_small_orbs_sit_on_the_life_orbs_rim_and_the_bar_starts_right_of_the_eye_at_its_height() {
        let d = LIFE_DIAMETER;
        let layout = orb_cluster_layout(d);
        let (life, r) = (layout.life, d * 0.5);
        assert_eq!((life.radius, life.centre.x), (r, r));
        // Each small orb's centre is on the rim, at its angle, and they are 0.38 of the life orb.
        let angle = |disc: Disc| {
            (life.centre.y - disc.centre.y)
                .atan2(disc.centre.x - life.centre.x)
                .to_degrees()
        };
        for (disc, degrees) in [(layout.stamina, 45.0), (layout.mana, -45.0)] {
            assert!(
                (life.centre.distance(disc.centre) - r).abs() < 1e-4,
                "{degrees}: on the rim"
            );
            assert!(
                (angle(disc) - degrees).abs() < 1e-3,
                "{degrees}: at {}",
                angle(disc)
            );
            assert!((disc.radius * 2.0 - 0.38 * d).abs() < 1e-4);
            assert!(
                life.centre.distance(disc.centre) < r + disc.radius,
                "it overlaps the life orb's frame"
            );
        }
        // The eye: 0.38 across, its centre on the rim straight right.
        assert!((layout.eye.width() - 0.38 * d).abs() < 1e-4);
        assert!(
            (layout.eye.center() - Vec2::new(life.centre.x + r, life.centre.y)).length() < 1e-4
        );
        assert!(
            layout.eye.min.x < life.centre.x + r,
            "it overlaps the life orb's frame too"
        );
        // The bar starts just right of the eye, centred on its height, 0.38 of the life orb tall.
        assert!(
            layout.bar.min.x > layout.eye.max.x && layout.bar.min.x - layout.eye.max.x < 0.1 * d,
            "just right of the eye"
        );
        assert!(
            (layout.bar.center().y - layout.eye.center().y).abs() < 1e-4,
            "at the eye's height"
        );
        assert!((layout.bar.height() - 0.38 * d).abs() < 1e-4);
        // Nothing the cluster holds overlaps anything but the life orb.
        let stamina = layout.stamina.square();
        let mana = layout.mana.square();
        for (name, other) in [
            ("eye", layout.eye),
            ("bar", layout.bar),
            ("status row", layout.status),
        ] {
            assert!(
                stamina.intersect(other).is_empty() && mana.intersect(other).is_empty(),
                "{name} clear of the small orbs"
            );
        }
        assert!(stamina.intersect(mana).is_empty());
        assert!(layout.eye.intersect(layout.bar).is_empty());
        // The status row sits just above the bar from its left end, a badge tall.
        assert_eq!(layout.status.min.x, layout.bar.min.x);
        assert!(
            layout.status.max.y < layout.bar.min.y
                && layout.bar.min.y - layout.status.max.y < 0.1 * d,
            "just above the bar"
        );
        assert!((layout.status.height() - layout.badge).abs() < 1e-4);
    }

    #[test]
    fn the_cluster_bounds_everything_and_scales_with_the_life_orb() {
        let layout = orb_cluster_layout(14.0);
        let everything = [
            layout.life.square(),
            layout.stamina.square(),
            layout.mana.square(),
            layout.eye,
            layout.bar,
            layout.status,
        ];
        for rect in everything {
            assert!(
                rect.min.x >= -1e-4
                    && rect.min.y >= -1e-4
                    && rect.max.x <= layout.size.x + 1e-4
                    && rect.max.y <= layout.size.y + 1e-4,
                "{rect:?} in {:?}",
                layout.size
            );
        }
        // Anchored at the corner with the margin, it fits inside 100 units, the height of any
        // window 540 lines or more, and at the half-size floor (5.4 px a unit) still shows a
        // readable orb.
        assert!(MARGIN + layout.size.x <= 100.0 && MARGIN + layout.size.y <= 100.0);
        assert!(layout.life.radius * 2.0 * 5.4 >= 75.0);
        let double = orb_cluster_layout(28.0);
        assert!(
            (double.size - layout.size * 2.0).length() < 1e-3,
            "twice the life orb, twice the cluster"
        );
        assert!((double.eye.center() - layout.eye.center() * 2.0).length() < 1e-3);
    }

    /// A bite of `damage` on a delver of `max` hit points, through the vitals' own systems.
    fn struck_by(damage: u32, max: i32) -> (App, Entity) {
        let (mut app, player) = delve();
        app.world_mut().get_mut::<Health>(player).unwrap().max = max;
        app.world_mut().get_mut::<Health>(player).unwrap().hp = max;
        run(&mut app, 3);
        let attacker = app.world_mut().spawn(Transform::default()).id();
        app.world_mut()
            .write_message(crate::creatures::behaviour::AttackOnDelver {
                attacker,
                species: Some(fb_creatures::Species::Rat),
                damage,
                poison: 0,
                attacker_pos: Vec2::ZERO,
            });
        app.update();
        (app, player)
    }

    fn life(app: &mut App) -> (Orb, OrbStyle, Vec2) {
        let mut query = app
            .world_mut()
            .query::<(&Orb, &MaterialNode<OrbMaterial>, &UiTransform)>();
        let (orb, handle, transform) = query
            .iter(app.world())
            .find(|(o, ..)| o.kind == OrbKind::Life)
            .expect("the life orb");
        let style = app
            .world()
            .resource::<Assets<OrbMaterial>>()
            .get(&handle.0)
            .expect("its material")
            .style;
        (*orb, style, transform.scale)
    }

    #[test]
    fn a_bite_pulses_the_life_orb_and_it_is_gone_after_a_quarter_second() {
        let (mut app, _) = struck_by(3, 12);
        let (struck, style, scale) = life(&mut app);
        assert!(
            struck.pulse.left > 0.0 && struck.pulse.heat() > 0.0,
            "{:?}",
            struck.pulse
        );
        assert!(style.pulse > 0.0, "the frame and liquid are hot");
        assert!(
            scale.x > 1.0 && scale.x <= 1.0 + PULSE_SWELL + 1e-5 && scale.x == scale.y,
            "swollen: {scale}"
        );
        // Easing back.
        run(&mut app, 6);
        let (_, mid, mid_scale) = life(&mut app);
        assert!(
            mid.pulse > 0.0
                && mid.pulse < style.pulse
                && mid_scale.x < scale.x
                && mid_scale.x > 1.0
        );
        run(&mut app, 12);
        let (struck, style, scale) = life(&mut app);
        assert_eq!(
            (struck.pulse.heat(), style.pulse, scale),
            (0.0, 0.0, Vec2::ONE),
            "gone after 0.25 s"
        );
        // The other orbs never pulse.
        for kind in [OrbKind::Stamina, OrbKind::Mana] {
            assert_eq!(orb(&mut app, kind).1.pulse, 0.0);
        }
    }

    #[test]
    fn a_bigger_bite_pulses_harder() {
        let heat = |damage: u32| {
            let (mut app, _) = struck_by(damage, 12);
            let (orb, style, scale) = life(&mut app);
            (orb.pulse.heat(), style.pulse, scale.x)
        };
        let (rat, constrictor) = (heat(1), heat(6));
        assert!(rat.0 > 0.0, "even a scratch pulses");
        assert!(
            constrictor.0 > rat.0 && constrictor.1 > rat.1 && constrictor.2 > rat.2,
            "{rat:?} against {constrictor:?}"
        );
        let swollen = heat(50).2;
        assert!(
            swollen > 1.0 + PULSE_SWELL * 0.8 && swollen <= 1.0 + PULSE_SWELL + 1e-5,
            "the hardest swells to about 1.08: {swollen}"
        );
    }

    #[test]
    fn the_pulse_adds_to_the_flash_over_the_view_and_follows_any_fall_in_hit_points() {
        use crate::combat::vitals::Pain;
        let (mut app, _) = struck_by(3, 12);
        assert!(
            app.world().resource::<Pain>().alpha() > 0.0,
            "the flash over the view still starts"
        );
        assert!(
            life(&mut app).0.pulse.heat() > 0.0,
            "and the orb pulses too"
        );
        // A fall that is not a bite (a trap, say) pulses it as well, and a heal does not.
        let (mut app, player) = delve();
        app.world_mut().get_mut::<Health>(player).unwrap().hp = 6;
        run(&mut app, 3);
        let after_fall = life(&mut app).0.pulse.heat();
        assert!(after_fall > 0.0, "{after_fall}");
        run(&mut app, 30);
        app.world_mut().get_mut::<Health>(player).unwrap().hp = 11;
        run(&mut app, 3);
        assert_eq!(life(&mut app).0.pulse.heat(), 0.0, "healing does not pulse");
    }
}
