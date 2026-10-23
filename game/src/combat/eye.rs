//! The visibility eye on the life orb's rim, between the stamina and mana orbs: it opens as the delver gets easier to
//! see and closes in the dark.
//!
//! How visible the delver is comes from [`fb_character::visibility`], a pure function of the
//! light on them, how fast they move, their bulk and their Stealth, so creatures can read the
//! same number later. [`watch_the_delver`] works it out from the world: the light falling on the
//! delver is every point light in the level (wall torches, braziers, glowcaps, lit things on the
//! floor) and their own carried light, each at its distance by the toon shader's own falloff
//! ([`crate::light::lit_level`]), with no account taken of walls between. The number is kept
//! on the delver ([`DelverVisibility`]) and written only when it changes. The eye eases toward
//! it and is written only while it moves, as the orbs are ([`super::orbs`]).

use bevy::{
    camera::Exposure,
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::{ShaderRef, load_shader_library, Shader},
};

use super::orbs::{OrbCluster, cluster_layout, ease};
use crate::character::{Character, DelverVisibility, Load};
use crate::inventory::Inventory;
use crate::light::lit_level;
use crate::play::{DelveSystems, Player};
use crate::player::Gait;
use crate::ui::{FRAME_GOLD, INK, hud};

const SHADER_PATH: &str = "embedded://forever_below/combat/eye.wesl";

pub struct EyePlugin;

impl Plugin for EyePlugin {
    fn build(&self, app: &mut App) {
        if app.world().contains_resource::<Assets<Shader>>() {
            load_shader_library!(app, "eye.wesl");
            app.add_plugins(UiMaterialPlugin::<EyeMaterial>::default());
        } else if app.world().contains_resource::<AssetServer>() {
            app.init_asset::<EyeMaterial>();
        }
        app.add_systems(Update, (watch_the_delver, spawn_eye, read_eye, settle_eye).chain().in_set(DelveSystems));
    }
}

/// The eye on the HUD.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Eye {
    /// How open it should be, 0 to 1; none until the delver has been read.
    pub target: Option<f32>,
    snap: bool,
}

/// How the eye is drawn (`eye.wesl`).
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct EyeMaterial {
    #[uniform(0)]
    pub style: EyeStyle,
}

#[derive(ShaderType, Clone, Copy, Debug, PartialEq)]
pub struct EyeStyle {
    pub sclera: LinearRgba,
    pub iris: LinearRgba,
    pub iris_deep: LinearRgba,
    pub gold: LinearRgba,
    pub gold_dark: LinearRgba,
    pub backing: LinearRgba,
    pub open: f32,
}

impl Default for EyeStyle {
    fn default() -> Self {
        EyeStyle {
            sclera: INK.into(),
            iris: Color::srgb(0.82, 0.58, 0.20).into(),
            iris_deep: Color::srgb(0.33, 0.19, 0.05).into(),
            gold: FRAME_GOLD.into(),
            gold_dark: Color::srgb(0.30, 0.21, 0.08).into(),
            backing: Color::srgba(0.03, 0.02, 0.02, 0.55).into(),
            open: 0.0,
        }
    }
}

impl UiMaterial for EyeMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }
}

/// How visible the delver is now: the light on them, their pace, their load and their Stealth
/// ([`fb_character::visibility`]). The light is worked out each frame, but the result only
/// written when it changes, and only to a hundredth, so a still delver in still light marks
/// nothing. Their Stealth is summed from their gear when the character or inventory changes.
#[allow(clippy::type_complexity)]
fn watch_the_delver(
    character: Option<Res<Character>>,
    inventory: Option<Res<Inventory>>,
    rules: Option<Res<crate::rules::GameRules>>,
    lights: Query<(&GlobalTransform, &PointLight)>,
    mut players: Query<(&Transform, Option<&Gait>, &Load, &mut DelverVisibility, Option<&crate::character::DelverEffects>), With<Player>>,
    mut stealth: Local<Option<f64>>,
) {
    let (Some(character), Some(inventory)) = (character, inventory) else { return };
    if stealth.is_none() || character.is_changed() || inventory.is_changed() {
        *stealth = Some(character.gear(&inventory).stealth);
    }
    let stealth = stealth.unwrap_or_default();
    let exposure = Exposure::INDOOR.exposure();
    let rules = crate::rules::GameRules::of(&rules);
    for (at, gait, load, mut seen, effects) in &mut players {
        // The light falls on the body, about half the height of the eye.
        let body = at.translation + Vec3::Y * crate::play::EYE_HEIGHT * 0.5;
        let lit = lit_level(body, lights.iter().map(|(t, l)| (t.translation(), l)), exposure);
        // Effects that draw the eye (an itch that has them scratching) add to it.
        let noticed = effects.map_or(0.0, |e| e.0.noticed(&rules.effects));
        let visible = (fb_character::visibility_with(rules, lit as f64, gait.map_or(0.0, |g| g.pace) as f64, load.0.band_with(rules), stealth) + noticed).min(1.0);
        seen.set_if_neq(DelverVisibility((visible * 100.0).round() as f32 / 100.0));
    }
}

/// Gives each new cluster its eye, on the life orb's rim straight right ([`cluster_layout`]).
fn spawn_eye(mut commands: Commands, clusters: Query<Entity, Added<OrbCluster>>, materials: Option<ResMut<Assets<EyeMaterial>>>) {
    let Some(mut materials) = materials else { return };
    let eye = cluster_layout().eye;
    for cluster in &clusters {
        commands.entity(cluster).with_child((
            Name::new("Visibility eye"),
            Eye { target: None, snap: false },
            MaterialNode(materials.add(EyeMaterial { style: EyeStyle::default() })),
            Node { position_type: PositionType::Absolute, left: hud(eye.min.x), top: hud(eye.min.y), width: hud(eye.width()), height: hud(eye.height()), ..default() },
            Pickable::IGNORE,
        ));
    }
}

/// How open the eye should be: as visible as the delver is.
fn read_eye(players: Query<&DelverVisibility, With<Player>>, mut eyes: Query<&mut Eye>) {
    let Some(seen) = players.iter().next() else { return };
    let open = seen.0.clamp(0.0, 1.0);
    for mut eye in &mut eyes {
        if eye.target != Some(open) {
            if eye.target.is_none() {
                eye.snap = true;
            }
            eye.target = Some(open);
        }
    }
}

/// Eases each eye toward its target, written only while it moves.
fn settle_eye(time: Res<Time>, mut eyes: Query<(&mut Eye, &MaterialNode<EyeMaterial>)>, materials: Option<ResMut<Assets<EyeMaterial>>>) {
    let Some(mut materials) = materials else { return };
    let dt = time.delta_secs();
    for (mut eye, handle) in &mut eyes {
        let Some(target) = eye.target else { continue };
        let Some(material) = materials.get(&handle.0) else { continue };
        let shown = if eye.snap { target } else { ease(material.style.open, target, dt) };
        if material.style.open != shown
            && let Some(mut material) = materials.get_mut(&handle.0)
        {
            material.style.open = shown;
        }
        if eye.snap {
            eye.snap = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::asset::AssetPlugin;
    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;

    use super::*;
    use crate::combat::orbs::OrbPlugin;
    use crate::combat::vitals::VitalsPlugin;
    use crate::combat::{Health, RunDice};
    use crate::launch::AppState;
    use crate::light::Torch;
    use crate::play::ScreenOpen;

    /// A delve with a view model camera and a delver standing still in the dark with 12 hit
    /// points, stepped a 60th of a second a frame.
    fn delve() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .insert_state(AppState::Delving)
            .insert_resource(RunDice(procgen_rng::Mulberry32::new(4)))
            .init_resource::<Character>()
            .init_resource::<Inventory>()
            .init_resource::<ScreenOpen>()
            .add_plugins((VitalsPlugin, OrbPlugin, EyePlugin));
        crate::play::run_delve_systems_while_delving(&mut app);
        let player = app.world_mut().spawn((Player, Transform::default(), Health::full(12))).id();
        let follows = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(crate::carry::ViewModelCamera { follows });
        run(&mut app, 4);
        (app, player)
    }

    fn run(app: &mut App, frames: usize) {
        for _ in 0..frames {
            app.update();
        }
    }

    /// A torch carried at the eye, as the delver's carried light is.
    fn light_a_torch(app: &mut App, player: Entity) -> Entity {
        let at = app.world().get::<Transform>(player).unwrap().translation + Vec3::new(1.5, crate::play::EYE_HEIGHT, -1.5);
        app.world_mut().spawn((Torch::light(1.0, 60.0, &Exposure::INDOOR), GlobalTransform::from_translation(at))).id()
    }

    fn eye(app: &mut App) -> (Eye, EyeStyle) {
        let mut query = app.world_mut().query::<(&Eye, &MaterialNode<EyeMaterial>)>();
        let (eye, handle) = query.single(app.world()).expect("the eye");
        let style = app.world().resource::<Assets<EyeMaterial>>().get(&handle.0).expect("its material").style;
        (*eye, style)
    }

    fn seen(app: &App, player: Entity) -> f32 {
        app.world().get::<DelverVisibility>(player).unwrap().0
    }

    #[test]
    fn standing_still_in_an_unlit_room_the_eye_is_nearly_closed() {
        let (mut app, player) = delve();
        run(&mut app, 120);
        assert!(seen(&app, player) < 0.1, "{}", seen(&app, player));
        assert!(eye(&mut app).1.open < 0.1);
    }

    #[test]
    fn lighting_a_torch_opens_it_wide() {
        let (mut app, player) = delve();
        run(&mut app, 60);
        let shut = eye(&mut app).1.open;
        let torch = light_a_torch(&mut app, player);
        run(&mut app, 120);
        let wide = eye(&mut app).1.open;
        assert!(wide > 0.7 && wide > shut + 0.5, "{shut} then {wide}");
        // Putting it out closes the eye again.
        app.world_mut().entity_mut(torch).despawn();
        run(&mut app, 120);
        assert!(eye(&mut app).1.open < 0.1);
    }

    #[test]
    fn a_stealth_affix_keeps_the_eye_a_step_more_closed_in_the_same_light() {
        let open_with = |stealthy: bool| {
            let (mut app, player) = delve();
            light_a_torch(&mut app, player);
            if stealthy {
                let mut item = fb_items::roll_item("Cloak", 3).unwrap();
                let silent = fb_items::tables::affixes().iter().find(|a| a.name == "Silent").unwrap();
                silent.apply_to_stats(&mut item.profile.stats);
                let mut inventory = app.world_mut().resource_mut::<Inventory>();
                let id = inventory.add(item);
                inventory.wear(id, fb_items::Slot::Cloak).unwrap();
            }
            run(&mut app, 180);
            (seen(&app, player), eye(&mut app).1.open)
        };
        let (plain, plain_open) = open_with(false);
        let (silent, silent_open) = open_with(true);
        assert!((plain - silent - 0.1).abs() < 0.02, "a step of a tenth: {plain} against {silent}");
        assert!(silent_open < plain_open - 0.05, "{silent_open} against {plain_open}");
    }

    #[test]
    fn moving_fast_and_bulky_gear_open_the_eye_further() {
        let (mut app, player) = delve();
        light_a_torch(&mut app, player);
        run(&mut app, 30);
        let still = seen(&app, player);
        app.world_mut().entity_mut(player).insert(Gait { pace: 1.5, ..default() });
        run(&mut app, 30);
        let sprinting = seen(&app, player);
        assert!(sprinting > still + 0.1, "{still} then {sprinting}");
        *app.world_mut().get_mut::<Load>(player).unwrap() = Load::sample(fb_character::LoadBand::Encumbered);
        run(&mut app, 30);
        assert!(seen(&app, player) > sprinting + 0.05);
    }

    #[test]
    fn a_still_delver_in_still_light_rewrites_nothing() {
        let (mut app, player) = delve();
        light_a_torch(&mut app, player);
        run(&mut app, 180);
        app.world_mut().clear_trackers();
        run(&mut app, 3);
        assert!(!app.world().entity(player).get_ref::<DelverVisibility>().unwrap().is_changed());
        let eyes = app.world_mut().query_filtered::<&Eye, Changed<Eye>>().iter(app.world()).count();
        assert_eq!(eyes, 0);
    }

    #[test]
    fn the_eye_is_part_of_the_hud_cluster_and_hides_under_screens() {
        use crate::carry::{Hud, hide_hud_under_screens};
        let (mut app, _) = delve();
        app.add_systems(Update, hide_hud_under_screens);
        let mut query = app.world_mut().query_filtered::<&ChildOf, With<Eye>>();
        let cluster = query.single(app.world()).unwrap().parent();
        assert!(app.world().get::<OrbCluster>(cluster).is_some(), "positioned in the cluster");
        let hud = app.world().get::<ChildOf>(cluster).unwrap().parent();
        assert!(app.world().get::<Hud>(hud).is_some());
        assert!(app.world().get::<DespawnOnExit<AppState>>(hud).is_some(), "scoped to the delve");
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(crate::play::Screen::Inventory);
        app.update();
        assert_eq!(*app.world().get::<Visibility>(hud).unwrap(), Visibility::Hidden);
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
        app.update();
        assert_eq!(*app.world().get::<Visibility>(hud).unwrap(), Visibility::Inherited);
    }

    #[test]
    fn the_eye_sits_where_the_layout_puts_it() {
        let layout = cluster_layout();
        let (mut app, _) = delve();
        let mut query = app.world_mut().query_filtered::<&Node, With<Eye>>();
        let node = query.single(app.world()).unwrap();
        assert_eq!((node.left, node.top), (hud(layout.eye.min.x), hud(layout.eye.min.y)));
        assert_eq!((node.width, node.height), (hud(layout.eye.width()), hud(layout.eye.height())));
    }
}
