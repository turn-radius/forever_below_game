// Ink outline for `OutlineMaterial`: an inverted hull. The pipeline culls front faces, so
// only the back of the pushed-out shell shows around the silhouette. Each vertex moves along
// its outline normal (smoothed across hard edges, see `withOutlineNormals` in
// `reference/spline_armory_engine.js`) by a width set in screen pixels, so the line reads
// the same at any distance. See `docs/rendering.md` ("Ink outline").

import bevy_pbr::render::{
    mesh_functions::{get_world_from_local, mesh_normal_local_to_world},
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
};

struct OutlineUniform {
    color: vec4<f32>,
    // Line width in pixels on a 1080-pixel-tall viewport; scales with the viewport.
    width_px: f32,
    // Cap on the push in world units, so distant and small things get thinner lines.
    max_width: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> outline: OutlineUniform;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) outline_normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
}

const REFERENCE_VIEWPORT_HEIGHT: f32 = 1080.0;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    let world_from_local = get_world_from_local(vertex.instance_index);
    let world_position = (world_from_local * vec4<f32>(vertex.position, 1.0)).xyz;
    let normal = normalize(mesh_normal_local_to_world(vertex.outline_normal, vertex.instance_index));

    // World size of one pixel at this vertex's depth. clip_from_view[1][1] is
    // 1 / tan(fov_y / 2) for a perspective camera and 2 / height for an orthographic one.
    let viewport_height = view.viewport.w;
    var pixel = 2.0 / (view.clip_from_view[1][1] * viewport_height);
    if view.clip_from_view[3][3] != 1.0 {
        let depth = -(view.view_from_world * vec4<f32>(world_position, 1.0)).z;
        pixel = pixel * max(depth, 0.0);
    }
    let width_px = outline.width_px * viewport_height / REFERENCE_VIEWPORT_HEIGHT;
    let width = min(width_px * pixel, outline.max_width);

    var out: VertexOutput;
    out.clip_position = position_world_to_clip(world_position + normal * width);
    return out;
}

@fragment
fn fragment() -> @location(0) vec4<f32> {
    return outline.color;
}
