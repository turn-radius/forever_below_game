//! Screen-space ink outlines, a try against the inverted hull (`outline.rs`, #434). A
//! fullscreen pass (`screen_ink.wesl`) after the lit image draws [`ARMORY_INK`] where the depth
//! or normal prepass jumps. [`InkStyle`] picks between the two at runtime: on `Screen` the hulls
//! are hidden and every 3D camera gets a depth and normal prepass and a [`ScreenInk`] pass.
//!
//! The pass is built on Bevy 0.20's fullscreen post-process pattern
//! (`bevy_core_pipeline::fullscreen_material`, `bevy_post_process::effect_stack`), with the
//! prepass textures added to its bind group.

use bevy::{
    core_pipeline::{
        Core3d, Core3dSystems, FullscreenShader,
        prepass::{DepthPrepass, NormalPrepass, ViewPrepassTextures},
        tonemapping::tonemapping,
    },
    ecs::schedule::IntoScheduleConfigs,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        camera::ExtractedCamera,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_resource::{
            BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedRenderPipelineId, ColorTargetState, ColorWrites,
            DynamicUniformBuffer, FragmentState, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
            RenderPipelineDescriptor, ShaderStages, ShaderType, SpecializedRenderPipeline, SpecializedRenderPipelines, TextureFormat,
            TextureSampleType,
            binding_types::{texture_2d, texture_2d_multisampled, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer},
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        view::{ExtractedView, Msaa, ViewTarget},
    },
    shader::{Shader, ShaderDefVal},
};

use super::{ARMORY_INK, InkOutline};

/// Which ink the game draws: the inverted hulls of [`OutlineMaterial`](super::OutlineMaterial),
/// or one screen-space pass over the lit image.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InkStyle {
    #[default]
    Hull,
    Screen,
}

impl InkStyle {
    /// `FB_INK=screen` or `FB_INK=hull` picks one for this run, over the player's setting.
    pub fn from_env() -> Option<InkStyle> {
        Self::parse(&std::env::var("FB_INK").ok()?)
    }

    pub fn parse(value: &str) -> Option<InkStyle> {
        match value.trim().to_ascii_lowercase().as_str() {
            "hull" => Some(InkStyle::Hull),
            "screen" => Some(InkStyle::Screen),
            _ => None,
        }
    }
}

/// The screen-space ink pass for this camera. [`apply_ink_style`] adds and removes it with the
/// prepasses it reads.
#[derive(Component, ExtractComponent, Clone, Copy, Debug, PartialEq)]
#[extract_app(RenderApp)]
pub struct ScreenInk {
    pub color: LinearRgba,
    /// Line width in pixels on a 1080-pixel-tall viewport; scales with the viewport, as the
    /// hull's does.
    pub width_px: f32,
    /// Whether this plugin added the camera's [`DepthPrepass`], and so removes it again.
    pub added_depth_prepass: bool,
}

impl ScreenInk {
    /// The same 2 px of Armory ink as the hull's default.
    pub fn new() -> Self {
        ScreenInk { color: ARMORY_INK.into(), width_px: 2.0, added_depth_prepass: false }
    }
}

impl Default for ScreenInk {
    fn default() -> Self {
        Self::new()
    }
}

/// How far a pixel's depth may fall short of its neighbours' average, as a fraction of its own
/// (a perspective view), before it is ink.
const DEPTH_THRESHOLD: f32 = 0.1;
/// The same in world units (an orthographic view: a held item's icon, the turntable).
const ORTHO_DEPTH_THRESHOLD: f32 = 0.1;
/// `1 - dot` of two neighbouring normals that makes an edge, about 45 degrees.
const NORMAL_THRESHOLD: f32 = 0.3;

/// Adds the switch, the systems that apply it, and the render pass.
pub(super) fn add(app: &mut App) {
    let style = InkStyle::from_env().unwrap_or_default();
    app.insert_resource(style)
        .add_plugins(ExtractComponentPlugin::<ScreenInk>::default())
        .add_systems(Update, (apply_ink_style, hide_new_hulls, add_ink_to_new_cameras).chain());
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    render_app
        .init_resource::<SpecializedRenderPipelines<ScreenInkPipeline>>()
        .init_resource::<ScreenInkUniforms>()
        .add_systems(RenderStartup, init_pipeline)
        .add_systems(Render, (prepare_pipelines, prepare_uniforms).in_set(RenderSystems::Prepare))
        .add_systems(Core3d, screen_ink.in_set(Core3dSystems::PostProcess).before(tonemapping));
}

/// When the style changes, shows or hides every hull and adds or removes the screen pass on
/// every 3D camera.
fn apply_ink_style(
    style: Res<InkStyle>,
    mut commands: Commands,
    mut hulls: Query<&mut Visibility, With<InkOutline>>,
    cameras: Query<(Entity, Option<&ScreenInk>, Has<DepthPrepass>), With<Camera3d>>,
) {
    if !style.is_changed() {
        return;
    }
    let screen = *style == InkStyle::Screen;
    for mut visibility in &mut hulls {
        visibility.set_if_neq(if screen { Visibility::Hidden } else { Visibility::Inherited });
    }
    for (camera, ink, has_depth_prepass) in &cameras {
        set_camera_ink(&mut commands, camera, ink, has_depth_prepass, screen);
    }
}

/// A hull spawned while the screen style is on stays hidden.
fn hide_new_hulls(style: Res<InkStyle>, mut hulls: Query<&mut Visibility, Added<InkOutline>>) {
    if *style == InkStyle::Screen {
        for mut visibility in &mut hulls {
            *visibility = Visibility::Hidden;
        }
    }
}

/// A camera spawned while the screen style is on gets the pass.
fn add_ink_to_new_cameras(
    style: Res<InkStyle>,
    mut commands: Commands,
    cameras: Query<(Entity, Option<&ScreenInk>, Has<DepthPrepass>), Added<Camera3d>>,
) {
    if *style == InkStyle::Screen {
        for (camera, ink, has_depth_prepass) in &cameras {
            set_camera_ink(&mut commands, camera, ink, has_depth_prepass, true);
        }
    }
}

/// Puts the screen pass and the prepasses it reads on `camera`, or takes them off. A depth
/// prepass something else added is left alone.
fn set_camera_ink(commands: &mut Commands, camera: Entity, ink: Option<&ScreenInk>, has_depth_prepass: bool, screen: bool) {
    let mut camera = commands.entity(camera);
    if screen {
        if ink.is_none() {
            camera.insert((ScreenInk { added_depth_prepass: !has_depth_prepass, ..default() }, NormalPrepass));
            if !has_depth_prepass {
                camera.insert(DepthPrepass);
            }
        }
    } else if let Some(ink) = ink {
        camera.remove::<(ScreenInk, NormalPrepass)>();
        if ink.added_depth_prepass {
            camera.remove::<DepthPrepass>();
        }
    }
}

#[derive(Resource)]
struct ScreenInkPipeline {
    layout: BindGroupLayoutDescriptor,
    layout_multisampled: BindGroupLayoutDescriptor,
    fullscreen_shader: FullscreenShader,
    shader: Handle<Shader>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ScreenInkPipelineKey {
    target_format: TextureFormat,
    multisampled: bool,
}

/// What `screen_ink.wesl` reads (`InkParams`).
#[derive(ShaderType, Clone, Copy)]
struct ScreenInkUniform {
    color: Vec4,
    width: f32,
    ortho_scale: f32,
    depth_threshold: f32,
    normal_threshold: f32,
}

#[derive(Resource, Default)]
struct ScreenInkUniforms(DynamicUniformBuffer<ScreenInkUniform>);

#[derive(Component)]
struct ScreenInkOffset(u32);

#[derive(Component)]
struct ScreenInkPipelineId(CachedRenderPipelineId);

fn init_pipeline(mut commands: Commands, fullscreen_shader: Res<FullscreenShader>, asset_server: Res<AssetServer>) {
    let layout_for = |multisampled: bool| {
        BindGroupLayoutDescriptor::new(
            "screen ink bind group layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    texture_2d(TextureSampleType::Float { filterable: false }),
                    uniform_buffer::<ScreenInkUniform>(true),
                    if multisampled { texture_depth_2d_multisampled() } else { texture_depth_2d() },
                    if multisampled {
                        texture_2d_multisampled(TextureSampleType::Float { filterable: false })
                    } else {
                        texture_2d(TextureSampleType::Float { filterable: false })
                    },
                ),
            ),
        )
    };
    commands.insert_resource(ScreenInkPipeline {
        layout: layout_for(false),
        layout_multisampled: layout_for(true),
        fullscreen_shader: fullscreen_shader.clone(),
        shader: asset_server.load("embedded://forever_below/render/screen_ink.wesl"),
    });
}

impl SpecializedRenderPipeline for ScreenInkPipeline {
    type Key = ScreenInkPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        RenderPipelineDescriptor {
            label: Some("screen ink".into()),
            layout: vec![if key.multisampled { self.layout_multisampled.clone() } else { self.layout.clone() }],
            vertex: self.fullscreen_shader.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs: if key.multisampled { vec![ShaderDefVal::Bool("MULTISAMPLED".into(), true)] } else { vec![] },
                targets: vec![Some(ColorTargetState { format: key.target_format, blend: None, write_mask: ColorWrites::ALL })],
                ..default()
            }),
            ..default()
        }
    }
}

fn prepare_pipelines(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<ScreenInkPipeline>>,
    pipeline: Res<ScreenInkPipeline>,
    views: Query<(Entity, &ExtractedView, &Msaa), (With<ScreenInk>, With<ExtractedCamera>)>,
) {
    for (view, extracted, msaa) in &views {
        let key = ScreenInkPipelineKey { target_format: extracted.target_format, multisampled: msaa.samples() > 1 };
        let id = pipelines.specialize(&pipeline_cache, &pipeline, key);
        commands.entity(view).insert(ScreenInkPipelineId(id));
    }
}

fn prepare_uniforms(
    mut commands: Commands,
    mut uniforms: ResMut<ScreenInkUniforms>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    views: Query<(Entity, &ScreenInk, &ExtractedView)>,
) {
    uniforms.0.clear();
    for (view, ink, extracted) in &views {
        let clip_from_view = extracted.clip_from_view;
        let orthographic = clip_from_view.w_axis.w == 1.0;
        let uniform = ScreenInkUniform {
            color: Vec4::from_array(ink.color.to_f32_array()),
            width: ink.width_px * extracted.viewport.w as f32 / 1080.0,
            ortho_scale: if orthographic { clip_from_view.z_axis.z.abs() } else { 0.0 },
            depth_threshold: if orthographic { ORTHO_DEPTH_THRESHOLD } else { DEPTH_THRESHOLD },
            normal_threshold: NORMAL_THRESHOLD,
        };
        let offset = uniforms.0.push(&uniform);
        commands.entity(view).insert(ScreenInkOffset(offset));
    }
    uniforms.0.write_buffer(&device, &queue);
}

fn screen_ink(
    view: ViewQuery<(&ViewTarget, &ScreenInkPipelineId, &ScreenInkOffset, &ViewPrepassTextures, &Msaa), With<ScreenInk>>,
    pipeline_cache: Res<PipelineCache>,
    pipeline: Res<ScreenInkPipeline>,
    uniforms: Res<ScreenInkUniforms>,
    mut ctx: RenderContext,
) {
    let (view_target, pipeline_id, offset, prepass, msaa) = view.into_inner();
    let Some(render_pipeline) = pipeline_cache.get_render_pipeline(pipeline_id.0) else { return };
    let Some(uniform) = uniforms.0.binding() else { return };
    let (Some(depth), Some(normal)) = (prepass.depth_only_view(), prepass.normal_view()) else { return };

    let post_process = view_target.post_process_write();
    let layout = if msaa.samples() > 1 { &pipeline.layout_multisampled } else { &pipeline.layout };
    let bind_group = ctx.render_device().create_bind_group(
        Some("screen ink bind group"),
        &pipeline_cache.get_bind_group_layout(layout),
        &BindGroupEntries::sequential((post_process.source, uniform, depth, normal)),
    );
    let pass_descriptor = RenderPassDescriptor {
        label: Some("screen ink"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: post_process.destination,
            depth_slice: None,
            resolve_target: None,
            ops: Operations::default(),
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    };
    let mut render_pass = ctx.begin_tracked_render_pass(pass_descriptor);
    render_pass.set_render_pipeline(render_pipeline);
    render_pass.set_bind_group(0, &bind_group, &[offset.0]);
    render_pass.draw(0..3, 0..1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(style: InkStyle) -> App {
        let mut app = App::new();
        app.insert_resource(style)
            .add_systems(Update, (apply_ink_style, hide_new_hulls, add_ink_to_new_cameras).chain());
        app
    }

    #[test]
    fn the_style_parses_from_its_names() {
        assert_eq!(InkStyle::parse("screen"), Some(InkStyle::Screen));
        assert_eq!(InkStyle::parse(" Hull "), Some(InkStyle::Hull));
        assert_eq!(InkStyle::parse("neon"), None);
        assert_eq!(InkStyle::default(), InkStyle::Hull);
    }

    #[test]
    fn the_screen_style_hides_the_hulls_and_the_hull_style_brings_them_back() {
        let mut app = app(InkStyle::Hull);
        let hull = app.world_mut().spawn((InkOutline, Visibility::default())).id();
        app.update();
        assert_eq!(*app.world().get::<Visibility>(hull).unwrap(), Visibility::Inherited);
        *app.world_mut().resource_mut::<InkStyle>() = InkStyle::Screen;
        app.update();
        assert_eq!(*app.world().get::<Visibility>(hull).unwrap(), Visibility::Hidden);
        let late = app.world_mut().spawn((InkOutline, Visibility::default())).id();
        app.update();
        assert_eq!(*app.world().get::<Visibility>(late).unwrap(), Visibility::Hidden, "a hull spawned later is hidden too");
        *app.world_mut().resource_mut::<InkStyle>() = InkStyle::Hull;
        app.update();
        for hull in [hull, late] {
            assert_eq!(*app.world().get::<Visibility>(hull).unwrap(), Visibility::Inherited);
        }
    }

    #[test]
    fn cameras_get_the_pass_and_its_prepasses_only_on_the_screen_style() {
        let mut app = app(InkStyle::Hull);
        let bare = app.world_mut().spawn(Camera3d::default()).id();
        let with_depth = app.world_mut().spawn((Camera3d::default(), DepthPrepass)).id();
        app.update();
        assert!(app.world().get::<ScreenInk>(bare).is_none());

        *app.world_mut().resource_mut::<InkStyle>() = InkStyle::Screen;
        let late = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        for camera in [bare, with_depth, late] {
            assert!(app.world().get::<ScreenInk>(camera).is_some());
            assert!(app.world().get::<NormalPrepass>(camera).is_some());
            assert!(app.world().get::<DepthPrepass>(camera).is_some());
        }

        *app.world_mut().resource_mut::<InkStyle>() = InkStyle::Hull;
        app.update();
        for camera in [bare, with_depth, late] {
            assert!(app.world().get::<ScreenInk>(camera).is_none());
            assert!(app.world().get::<NormalPrepass>(camera).is_none());
        }
        assert!(app.world().get::<DepthPrepass>(bare).is_none(), "the depth prepass the pass added goes with it");
        assert!(app.world().get::<DepthPrepass>(with_depth).is_some(), "a depth prepass something else added stays");
    }
}
