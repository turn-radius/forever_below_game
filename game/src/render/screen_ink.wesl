// Screen-space ink: one fullscreen pass over the lit image that draws the Armory's ink where
// the depth or the normal prepass jumps. A try at replacing the inverted hull (`outline.wesl`);
// see `docs/rendering.md` ("Screen-space ink").
//
// Depth: the prepass depth is reversed, so it is 1 / z for a perspective view and linear in z
// for an orthographic one. Either way it is linear in screen space across a flat surface, so
// the second difference across a pixel and its two neighbours is zero on a plane however
// steeply the plane is seen, and it is positive only where this pixel is farther than its
// surroundings: the far side of a silhouette and the bottom of a crease. Ink falls on that side
// only, as the hull's does, rather than on both sides of the edge.
//
// Normals: ink where a neighbour half a line away faces another way. Only between surfaces at
// about the same depth, since the normal prepass holds nothing where nothing was drawn.

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

struct InkParams {
    color: vec4<f32>,
    // Line width in pixels of this view.
    width: f32,
    // |dDepth / dz| of an orthographic view; 0 for a perspective one.
    ortho_scale: f32,
    // Perspective: how far (as a fraction of this pixel's own depth) the depth may fall short
    // of its neighbours' average. Orthographic: in world units.
    depth_threshold: f32,
    // 1 - dot of two normals that makes an edge.
    normal_threshold: f32,
}

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var<uniform> ink: InkParams;
@if(MULTISAMPLED)
@group(0) @binding(2) var depth_texture: texture_depth_multisampled_2d;
@else
@group(0) @binding(2) var depth_texture: texture_depth_2d;
@if(MULTISAMPLED)
@group(0) @binding(3) var normal_texture: texture_multisampled_2d<f32>;
@else
@group(0) @binding(3) var normal_texture: texture_2d<f32>;

fn at(p: vec2<i32>) -> vec2<i32> {
    return clamp(p, vec2<i32>(0), vec2<i32>(textureDimensions(depth_texture)) - vec2<i32>(1));
}

fn depth_at(p: vec2<i32>) -> f32 {
    return textureLoad(depth_texture, at(p), 0);
}

fn normal_at(p: vec2<i32>) -> vec3<f32> {
    return normalize(textureLoad(normal_texture, at(p), 0).xyz * 2.0 - 1.0);
}

fn depth_edge(p: vec2<i32>, step: i32, here: f32) -> bool {
    var edge = false;
    for (var axis = 0; axis < 2; axis += 1) {
        var offset = vec2<i32>(step, 0);
        if axis == 1 {
            offset = vec2<i32>(0, step);
        }
        let second_difference = depth_at(p + offset) + depth_at(p - offset) - 2.0 * here;
        if ink.ortho_scale > 0.0 {
            edge = edge || second_difference / ink.ortho_scale > ink.depth_threshold;
        } else {
            edge = edge || second_difference > ink.depth_threshold * here + 1e-7;
        }
    }
    return edge;
}

fn normal_edge(p: vec2<i32>, step: i32, here: f32) -> bool {
    if here <= 0.0 {
        return false;
    }
    let normal = normal_at(p);
    var edge = false;
    for (var i = 0; i < 4; i += 1) {
        var offset = vec2<i32>(step, 0);
        if i == 1 {
            offset = vec2<i32>(-step, 0);
        } else if i == 2 {
            offset = vec2<i32>(0, step);
        } else if i == 3 {
            offset = vec2<i32>(0, -step);
        }
        let there = depth_at(p + offset);
        // Within a fifth of its own depth, so a silhouette is the depth test's to ink.
        if there > 0.0 && abs(there - here) < 0.2 * here {
            edge = edge || 1.0 - dot(normal, normal_at(p + offset)) > ink.normal_threshold;
        }
    }
    return edge;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let p = vec2<i32>(in.position.xy);
    let source = textureLoad(source_texture, p, 0);
    let here = depth_at(p);
    let line = max(i32(round(ink.width)), 1);
    if depth_edge(p, line, here) || normal_edge(p, max(line / 2, 1), here) {
        return vec4<f32>(ink.color.rgb, source.a);
    }
    return source;
}
