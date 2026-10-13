// Screen-space sizes for the vertex shaders: line widths are set in pixels on a 1080-pixel-tall
// viewport and scale with the viewport, so they read the same at any window size.

import bevy_pbr::render::mesh_view_bindings::view;

const REFERENCE_VIEWPORT_HEIGHT: f32 = 1080.0;

// World size of one reference pixel (1/1080 of the view height) at `world_position`.
// clip_from_view[1][1] is 1 / tan(fov_y / 2) for a perspective camera and 2 / height for an
// orthographic one; only a perspective pixel grows with depth.
fn reference_pixel(world_position: vec3<f32>) -> f32 {
    var pixel = 2.0 / (view.clip_from_view[1][1] * REFERENCE_VIEWPORT_HEIGHT);
    if view.clip_from_view[3][3] != 1.0 {
        let depth = -(view.view_from_world * vec4<f32>(world_position, 1.0)).z;
        pixel = pixel * max(depth, 0.0);
    }
    return pixel;
}
