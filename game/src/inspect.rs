//! Looking at the game's components while it runs (#428), in debug builds only.
//!
//! F12, or `--inspect` on the command line, opens a World Inspector window (egui, through
//! `bevy-inspector-egui`) listing every entity with the components that derive `Reflect`: the
//! player, creatures, folk, lights and the simulation's needs. Editing a value changes it in the
//! game. While the window is open the level has no focus ([`crate::play::level_has_focus`]), so
//! the mouse is free for it and the delver stands still; F12 closes it and takes the mouse back.
//!
//! The inspector is the `inspector` feature, on with `dev` (so on by default), and `main` adds it
//! only with debug assertions, so `cargo run --release` has no inspector. Release builds should
//! still pass `--no-default-features`, which also leaves egui out of the binary. The `Reflect` derives stay in every build, as the
//! types they sit on are small, and [`remote`] mirrors the `fb_character` types, which cannot
//! derive `Reflect` themselves (the generator crates do not depend on Bevy).

use bevy::prelude::*;

/// Whether the inspector window is open. Always present so [`crate::play::level_has_focus`] can
/// read it; only the `inspector` feature ever sets it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InspectorOpen(pub bool);

/// Mirrors of the `fb_character` types the player's components wrap, so those components can
/// derive `Reflect`: the fields are the same as the originals', and Bevy reads and writes the
/// originals through them (`#[reflect(remote = ...)]`).
pub mod remote {
    use bevy::reflect::reflect_remote;

    #[reflect_remote(fb_character::MeterKind)]
    #[derive(Clone, Copy, Debug, Default)]
    pub enum MeterKindDef {
        #[default]
        Hunger,
        Thirst,
        Sleepiness,
    }

    #[reflect_remote(fb_character::Meter)]
    #[derive(Clone, Copy, Debug)]
    pub struct MeterDef {
        pub value: f64,
        pub rate_per_hour: f64,
        pub gear: f64,
        #[reflect(remote = MeterKindDef)]
        pub kind: fb_character::MeterKind,
    }

    #[reflect_remote(fb_character::Needs)]
    #[derive(Clone, Copy, Debug)]
    pub struct NeedsDef {
        #[reflect(remote = MeterDef)]
        pub hunger: fb_character::Meter,
        #[reflect(remote = MeterDef)]
        pub thirst: fb_character::Meter,
        #[reflect(remote = MeterDef)]
        pub sleepiness: fb_character::Meter,
    }

    #[reflect_remote(fb_character::Exhaustion)]
    #[derive(Clone, Copy, Debug)]
    pub struct ExhaustionDef {
        pub level: u32,
    }

    #[reflect_remote(fb_character::WorstBand)]
    #[derive(Clone, Copy, Debug)]
    pub struct WorstBandDef {
        pub hours: f64,
        pub levels: u32,
    }

    #[reflect_remote(fb_character::ExhaustionTimes)]
    #[derive(Clone, Copy, Debug)]
    pub struct ExhaustionTimesDef {
        #[reflect(remote = WorstBandDef)]
        pub starving: fb_character::WorstBand,
        #[reflect(remote = WorstBandDef)]
        pub dehydrated: fb_character::WorstBand,
        #[reflect(remote = WorstBandDef)]
        pub spent: fb_character::WorstBand,
        pub sleep_hours: f64,
        pub slept_recovered: bool,
    }

    #[reflect_remote(fb_character::Stamina)]
    #[derive(Clone, Copy, Debug)]
    pub struct StaminaDef {
        pub value: f64,
        pub winded: bool,
    }

    #[reflect_remote(fb_character::Mana)]
    #[derive(Clone, Copy, Debug)]
    pub struct ManaDef {
        pub points: f64,
        pub max: f64,
    }

    #[reflect_remote(fb_character::Burden)]
    #[derive(Clone, Copy, Debug)]
    pub struct BurdenDef {
        pub load_lb: f64,
        pub capacity_lb: f64,
    }
}

/// Registers the player, creature, folk and simulation components for reflection, which the
/// inspector lists. (`reflect_auto_register` finds them too on most platforms; this does not
/// rely on it.)
pub fn register_gameplay_types(app: &mut App) {
    use crate::{
        character::{DelverMana, DelverNeeds, DelverStamina, DelverVisibility, GearStamina, Load},
        creatures::{Critter, Driven, Idle, Intent, Motion},
        exhaustion::{Exhaustion, ExhaustionTimes},
        folk::{Denizen, FolkHead, Stance, Stride},
        hostility::Hostility,
        play::{Player, PlayerCamera},
        player::{Gait, GearPace, Locomotion, Slowed},
        simulate::Simulated,
    };
    app.register_type::<Player>()
        .register_type::<PlayerCamera>()
        .register_type::<Locomotion>()
        .register_type::<Gait>()
        .register_type::<Slowed>()
        .register_type::<GearPace>()
        .register_type::<Exhaustion>()
        .register_type::<ExhaustionTimes>()
        .register_type::<Load>()
        .register_type::<GearStamina>()
        .register_type::<DelverNeeds>()
        .register_type::<DelverStamina>()
        .register_type::<DelverMana>()
        .register_type::<DelverVisibility>()
        .register_type::<Critter>()
        .register_type::<Intent>()
        .register_type::<Motion>()
        .register_type::<Idle>()
        .register_type::<Driven>()
        .register_type::<Denizen>()
        .register_type::<Stride>()
        .register_type::<Stance>()
        .register_type::<FolkHead>()
        .register_type::<Hostility>()
        .register_type::<Simulated>();
}

#[cfg(all(feature = "inspector", debug_assertions))]
pub use window::InspectPlugin;

#[cfg(all(feature = "inspector", debug_assertions))]
mod window {
    use super::{InspectorOpen, register_gameplay_types};
    use crate::{launch::AppState, play::ScreenOpen, player::set_grab};
    use bevy::{
        camera::{ClearColorConfig, visibility::RenderLayers},
        prelude::*,
        window::{CursorOptions, PrimaryWindow},
    };
    use bevy_inspector_egui::{
        DefaultInspectorConfigPlugin,
        bevy_egui::{
            EguiContext, EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext,
        },
        bevy_inspector, egui,
    };

    /// F12 opens and closes the inspector.
    const TOGGLE: KeyCode = KeyCode::F12;

    /// Adds the inspector window, closed unless `open` (the `--inspect` switch).
    pub struct InspectPlugin {
        pub open: bool,
    }

    impl Plugin for InspectPlugin {
        fn build(&self, app: &mut App) {
            register_gameplay_types(app);
            app.insert_resource(InspectorOpen(self.open))
                // The context goes on a camera of its own, made when the window first opens, as
                // the game's cameras come and go with its views.
                .insert_resource(EguiGlobalSettings {
                    auto_create_primary_context: false,
                    ..default()
                })
                .add_plugins((EguiPlugin::default(), DefaultInspectorConfigPlugin))
                .add_systems(Update, toggle)
                .add_systems(
                    EguiPrimaryContextPass,
                    inspector_window.run_if(|open: Res<InspectorOpen>| open.0),
                );
        }
    }

    /// Opens or closes the window on F12 and frees or retakes the mouse with it (not when a screen
    /// or the menu is open, which hold the mouse themselves). The egui camera
    /// is made the first time it is wanted, draws over everything and clears nothing.
    #[allow(clippy::too_many_arguments)]
    fn toggle(
        keys: Res<ButtonInput<KeyCode>>,
        mut open: ResMut<InspectorOpen>,
        mut was: Local<bool>,
        state: Res<State<AppState>>,
        screen: Option<Res<ScreenOpen>>,
        egui: Query<(), With<PrimaryEguiContext>>,
        mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
        mut commands: Commands,
    ) {
        if keys.just_pressed(TOGGLE) {
            open.0 = !open.0;
        }
        if open.0 == *was {
            return;
        }
        *was = open.0;
        if open.0 {
            if egui.is_empty() {
                commands.spawn((
                    Name::new("Inspector camera"),
                    PrimaryEguiContext,
                    Camera2d,
                    Camera {
                        order: 1000,
                        clear_color: ClearColorConfig::None,
                        ..default()
                    },
                    // Draws nothing of its own: only egui renders to it.
                    RenderLayers::none(),
                ));
            }
            set_grab(&mut cursor, false);
        } else if *state.get() == AppState::Delving && !screen.is_some_and(|s| s.0.is_some()) {
            set_grab(&mut cursor, true);
        }
    }

    fn inspector_window(world: &mut World) {
        let Ok(context) = world
            .query_filtered::<&mut EguiContext, With<PrimaryEguiContext>>()
            .single(world)
        else {
            return;
        };
        let mut context = context.clone();
        egui::Window::new("World Inspector")
            .default_size((360.0, 520.0))
            .show(context.get_mut(), |ui| {
                egui::ScrollArea::both().show(ui, |ui| {
                    bevy_inspector::ui_for_world(world, ui);
                    ui.allocate_space(ui.available_size());
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{character::DelverNeeds, play::Player, player::Locomotion};
    use bevy::ecs::reflect::ReflectComponent;

    /// What the inspector does to a component: find it by its registered type and set a field.
    fn set_field(app: &mut App, entity: Entity, component: &str, field: &str, value: f32) {
        let registry = app.world().resource::<AppTypeRegistry>().clone();
        let registry = registry.read();
        let registration = registry
            .get_with_type_path(component)
            .unwrap_or_else(|| panic!("{component} is registered"));
        let reflect = registration
            .data::<ReflectComponent>()
            .expect("a component")
            .clone();
        let mut entity = app.world_mut().entity_mut(entity);
        let mut data = reflect.reflect_mut(&mut entity).expect("on the entity");
        let bevy::reflect::ReflectMut::Struct(fields) = data.reflect_mut() else {
            panic!("{component} is a struct")
        };
        *fields
            .field_mut(field)
            .unwrap_or_else(|| panic!("{field} is a field"))
            .try_downcast_mut::<f32>()
            .expect("an f32") = value;
    }

    #[test]
    fn the_player_and_a_wrapped_need_can_be_edited_by_reflection() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        register_gameplay_types(&mut app);
        let player = app
            .world_mut()
            .spawn((Player, Locomotion::default(), DelverNeeds::default()))
            .id();
        set_field(
            &mut app,
            player,
            "forever_below::player::Locomotion",
            "yaw",
            1.5,
        );
        assert_eq!(app.world().get::<Locomotion>(player).unwrap().yaw, 1.5);
        // A wrapped `fb_character::Needs` shows its meters through the mirror in [`remote`].
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let needs = registry
            .get_with_type_path("forever_below::character::DelverNeeds")
            .expect("registered");
        assert!(matches!(
            needs.type_info(),
            bevy::reflect::TypeInfo::TupleStruct(_)
        ));
    }
}
