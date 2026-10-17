// Ink outline for `OutlineMaterial`: an inverted hull. The pipeline culls front faces, so
// only the back of the pushed-out shell shows around the silhouette. Each vertex moves along
// its outline normal (smoothed across hard edges, see `withOutlineNormals` in
// `reference/spline_armory_engine.js`) by a width set in screen pixels, so the line reads
// the same at any distance. See `docs/rendering.md` ("Ink outline").

import bevy_pbr::render::{
    mesh_functions::{get_world_from_local, mesh_normal_local_to_world},
    skinning,
    view_transformations::position_world_to_clip,
};
import super::screen::reference_pixel;

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
    @if(SKINNED)
    @location(6) joint_indices: vec4<u32>,
    @if(SKINNED)
    @location(7) joint_weights: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
@if(SKINNED)
    let world_from_local = skinning::skin_model(vertex.joint_indices, vertex.joint_weights, vertex.instance_index);
@else
    let world_from_local = get_world_from_local(vertex.instance_index);
    let world_position = (world_from_local * vec4<f32>(vertex.position, 1.0)).xyz;
@if(SKINNED)
    // Skinned like a direction, not with Bevy's `skin_normals` (the inverse transpose, for
    // lighting): on a hull it threw ink spikes a unit long off every blended joint.
    let normal = normalize((world_from_local * vec4<f32>(vertex.outline_normal, 0.0)).xyz);
@else
    let normal = normalize(mesh_normal_local_to_world(vertex.outline_normal, vertex.instance_index));

    let width = min(outline.width_px * reference_pixel(world_position), outline.max_width);

    var out: VertexOutput;
    out.clip_position = position_world_to_clip(world_position + normal * width);
    return out;
}

@fragment
fn fragment() -> @location(0) vec4<f32> {
    return outline.color;
}
