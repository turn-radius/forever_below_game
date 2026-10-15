//! The life, stamina and mana orbs along the bottom left of the view, drawn into the vitals HUD
//! ([`super::vitals`]) so they hide under screens and go with the delve.
//!
//! Each is a glass globe in a gold frame that drains like a flask: red life (the delver's
//! [`Health`]), yellow stamina and blue mana ([`DelverStamina`], [`DelverMana`]). The orb is
//! drawn by one shader ([`OrbMaterial`], `orb.wesl`): the liquid's surface sloshes on its own
//! (from the shader's clock, so nothing is rewritten each frame), and the fill the shader is
//! given eases toward the real value, written only while it is moving. A winded delver's stamina
//! orb darkens.
//!
//! The layout follows the sketch: the large life orb at the bottom left, the small stamina orb
//! above and to the right of it, the visibility eye between the small orbs, the mana orb below
//! and to the right of the eye, and the quickslots running right. Everything is sized in `vmin`
//! (a percent of the view's shorter side) so it scales with the window ([`place`]).

use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::{Shader, ShaderRef, load_shader_library},
};

use super::Health;
use super::vitals::VitalsRoot;
use crate::character::{DelverMana, DelverStamina};
use crate::play::{DelveSystems, Player};
use crate::ui::FRAME_GOLD;

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
        app.add_systems(Update, (spawn_orbs, read_vitals, settle_orbs).chain().in_set(DelveSystems));
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

    /// Where in its slosh the orb starts, so the three do not move together.
    fn phase(self) -> f32 {
        match self {
            OrbKind::Life => 0.0,
            OrbKind::Stamina => 2.1,
            OrbKind::Mana => 4.2,
        }
    }
}

/// Where an orb sits, in `vmin` from the view's bottom left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    /// The left edge.
    pub left: f32,
    /// The bottom edge, up from the bottom of the view.
    pub bottom: f32,
    /// The side of its square.
    pub size: f32,
}

impl Place {
    pub fn right(&self) -> f32 {
        self.left + self.size
    }

    pub fn top(&self) -> f32 {
        self.bottom + self.size
    }
}

/// The space between orbs' neighbours and the view's edge, in `vmin`.
pub const MARGIN: f32 = 2.0;

/// Where an orb sits: life large at the bottom left, stamina small above and to the right of
/// it, mana small below and to the right of the eye that sits between them
/// ([`crate::combat::eye::place`] once it exists), lower than stamina.
pub fn place(kind: OrbKind) -> Place {
    match kind {
        OrbKind::Life => Place { left: MARGIN, bottom: MARGIN, size: 17.0 },
        OrbKind::Stamina => Place { left: MARGIN + 17.5, bottom: MARGIN + 9.5, size: 7.5 },
        OrbKind::Mana => Place { left: MARGIN + 31.0, bottom: MARGIN, size: 7.5 },
    }
}

/// An orb on the HUD.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Orb {
    pub kind: OrbKind,
    /// How full the orb should be, 0 to 1; none until the delver has been read.
    pub target: Option<f32>,
    /// Whether the orb is darkened (a winded delver's stamina).
    pub dim: bool,
    /// Whether the next settle jumps to the target instead of easing (the first reading).
    snap: bool,
}

impl Orb {
    pub fn new(kind: OrbKind) -> Orb {
        Orb { kind, target: None, dim: false, snap: false }
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

/// Gives each new vitals HUD its three orbs.
fn spawn_orbs(mut commands: Commands, roots: Query<Entity, Added<VitalsRoot>>, materials: Option<ResMut<Assets<OrbMaterial>>>) {
    let Some(mut materials) = materials else { return };
    for root in &roots {
        commands.entity(root).with_children(|hud| {
            for kind in OrbKind::ALL {
                let at = place(kind);
                hud.spawn((
                    Name::new(format!("{kind:?} orb")),
                    Orb::new(kind),
                    MaterialNode(materials.add(OrbMaterial { style: OrbStyle::new(kind) })),
                    Node { position_type: PositionType::Absolute, left: vmin(at.left), bottom: vmin(at.bottom), width: vmin(at.size), height: vmin(at.size), ..default() },
                    Pickable::IGNORE,
                ));
            }
        });
    }
}

/// How full each orb should be: life from the delver's hit points, stamina and mana from their
/// pools. Stamina stays full until the delver has one. Orbs are only touched when a value
/// differs.
fn read_vitals(players: Query<(Option<&Health>, Option<&DelverStamina>, Option<&DelverMana>), With<Player>>, mut orbs: Query<&mut Orb>) {
    let Some((health, stamina, mana)) = players.iter().next() else { return };
    for mut orb in &mut orbs {
        let (fill, dim) = match orb.kind {
            OrbKind::Life => (health.map_or(1.0, |h| h.hp.max(0) as f32 / h.max.max(1) as f32), false),
            OrbKind::Stamina => (stamina.map_or(1.0, |s| s.0.value as f32), stamina.is_some_and(|s| !s.0.can_exert())),
            OrbKind::Mana => (mana.map_or(1.0, |m| m.0.fraction() as f32), false),
        };
        let fill = fill.clamp(0.0, 1.0);
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
    if (target - shown).abs() <= SETTLED {
        return target;
    }
    target + (shown - target) * (-EASE_RATE * dt).exp()
}

/// Eases each orb's drawn fill toward its target, and sets its darkening. The material is only
/// written when it must change, so a still orb costs nothing.
fn settle_orbs(time: Res<Time>, mut orbs: Query<(&mut Orb, &MaterialNode<OrbMaterial>)>, materials: Option<ResMut<Assets<OrbMaterial>>>) {
    let Some(mut materials) = materials else { return };
    let dt = time.delta_secs();
    for (mut orb, handle) in &mut orbs {
        let Some(target) = orb.target else { continue };
        let Some(material) = materials.get(&handle.0) else { continue };
        let shown = if orb.snap { target } else { ease(material.style.fill, target, dt) };
        let dim = if orb.dim { DIMMED } else { 0.0 };
        if material.style.fill != shown || material.style.dim != dim {
            if let Some(mut material) = materials.get_mut(&handle.0) {
                material.style.fill = shown;
                material.style.dim = dim;
            }
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
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .insert_state(AppState::Delving)
            .insert_resource(RunDice(procgen_rng::Mulberry32::new(4)))
            .init_resource::<crate::character::Character>()
            .init_resource::<ScreenOpen>()
            .add_plugins((VitalsPlugin, OrbPlugin));
        crate::play::run_delve_systems_while_delving(&mut app);
        let player = app.world_mut().spawn((Player, Transform::default(), Health::full(12))).id();
        let follows = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(crate::carry::ViewModelCamera { follows });
        for _ in 0..4 {
            app.update();
        }
        (app, player)
    }

    fn orb(app: &mut App, kind: OrbKind) -> (Orb, OrbStyle) {
        let mut query = app.world_mut().query::<(&Orb, &MaterialNode<OrbMaterial>)>();
        let (orb, handle) = query.iter(app.world()).find(|(o, _)| o.kind == kind).expect("the orb");
        let style = app.world().resource::<Assets<OrbMaterial>>().get(&handle.0).expect("its material").style;
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
        assert!(texts.iter(app.world()).all(|t| !t.0.starts_with("HP")), "the readout is gone");
    }

    #[test]
    fn a_bite_for_a_quarter_of_max_hit_points_drains_the_life_orb_by_a_quarter() {
        let (mut app, player) = delve();
        app.world_mut().get_mut::<Health>(player).unwrap().hurt(3);
        run(&mut app, 2);
        assert_eq!(orb(&mut app, OrbKind::Life).0.target, Some(0.75));
        run(&mut app, 120);
        assert!((orb(&mut app, OrbKind::Life).1.fill - 0.75).abs() < 1e-3);
        assert_eq!(orb(&mut app, OrbKind::Stamina).1.fill, 1.0, "the others are unmoved");
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
        app.world_mut().get_mut::<DelverStamina>(player).unwrap().0.value = 0.4;
        run(&mut app, 120);
        assert!((orb(&mut app, OrbKind::Stamina).1.fill - 0.4).abs() < 1e-3);
        app.world_mut().get_mut::<DelverStamina>(player).unwrap().0.value = 1.0;
        run(&mut app, 120);
        assert!((orb(&mut app, OrbKind::Stamina).1.fill - 1.0).abs() < 1e-3);
    }

    #[test]
    fn a_winded_delvers_stamina_orb_darkens_and_mana_follows_the_pool() {
        let (mut app, player) = delve();
        app.world_mut().get_mut::<DelverStamina>(player).unwrap().0 = fb_character::Stamina { value: 0.0, winded: true };
        app.world_mut().get_mut::<DelverMana>(player).unwrap().0 = fb_character::Mana { points: 5.0, max: 20.0 };
        run(&mut app, 120);
        assert_eq!(orb(&mut app, OrbKind::Stamina).1.dim, DIMMED);
        assert_eq!(orb(&mut app, OrbKind::Life).1.dim, 0.0);
        assert!((orb(&mut app, OrbKind::Mana).1.fill - 0.25).abs() < 1e-3);
    }

    /// How many times the orb materials have been modified.
    #[derive(Resource, Default)]
    struct Rewrites(usize);

    fn count_rewrites(mut events: MessageReader<AssetEvent<OrbMaterial>>, mut rewrites: ResMut<Rewrites>) {
        rewrites.0 += events.read().filter(|e| matches!(e, AssetEvent::Modified { .. })).count();
    }

    #[test]
    fn a_still_orb_leaves_its_material_alone_and_a_moving_one_writes_it() {
        let (mut app, player) = delve();
        app.init_resource::<Rewrites>().add_systems(Last, count_rewrites);
        run(&mut app, 120);
        let settled = app.world().resource::<Rewrites>().0;
        run(&mut app, 60);
        assert_eq!(app.world().resource::<Rewrites>().0, settled, "nothing rewritten while nothing moves");
        app.world_mut().get_mut::<Health>(player).unwrap().hurt(3);
        run(&mut app, 120);
        assert!(app.world().resource::<Rewrites>().0 > settled, "the draining orb is written");
        let after = app.world().resource::<Rewrites>().0;
        run(&mut app, 60);
        assert_eq!(app.world().resource::<Rewrites>().0, after, "and stops once it has settled");
    }

    #[test]
    fn the_orbs_are_part_of_the_hud_and_hide_under_screens() {
        use crate::carry::{Hud, hide_hud_under_screens};
        let (mut app, _) = delve();
        app.add_systems(Update, hide_hud_under_screens);
        let mut query = app.world_mut().query_filtered::<&ChildOf, With<Orb>>();
        let roots: Vec<Entity> = query.iter(app.world()).map(|c| c.parent()).collect();
        assert_eq!(roots.len(), 3);
        for root in roots {
            assert!(app.world().get::<Hud>(root).is_some());
            assert!(app.world().get::<DespawnOnExit<AppState>>(root).is_some(), "scoped to the delve");
            app.world_mut().resource_mut::<ScreenOpen>().0 = Some(crate::play::Screen::Inventory);
            app.update();
            assert_eq!(*app.world().get::<Visibility>(root).unwrap(), Visibility::Hidden);
            app.world_mut().resource_mut::<ScreenOpen>().0 = None;
            app.update();
            assert_eq!(*app.world().get::<Visibility>(root).unwrap(), Visibility::Inherited);
        }
    }

    #[test]
    fn the_layout_follows_the_sketch_and_fits_the_smallest_window() {
        let (life, stamina, mana) = (place(OrbKind::Life), place(OrbKind::Stamina), place(OrbKind::Mana));
        assert!(life.size > stamina.size * 2.0 && stamina.size == mana.size, "a large life orb and two small ones");
        assert!(stamina.left >= life.right() - 1.0 && stamina.bottom > life.bottom + life.size * 0.4, "stamina is above and to the right of life");
        assert!(mana.left > stamina.right() && mana.bottom < stamina.bottom, "mana is lower, further right: the eye fits between");
        // No overlaps among the three.
        for (a, b) in [(life, stamina), (stamina, mana), (life, mana)] {
            let apart = a.right() <= b.left || b.right() <= a.left || a.top() <= b.bottom || b.top() <= a.bottom;
            assert!(apart, "{a:?} against {b:?}");
        }
        // In a 4:3 window (133 vmin wide, 100 high) and in a square one.
        for orb in [life, stamina, mana] {
            assert!(orb.left >= 0.0 && orb.bottom >= 0.0 && orb.right() <= 100.0 && orb.top() <= 100.0, "{orb:?}");
        }
    }
}
