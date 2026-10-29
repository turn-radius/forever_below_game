// The veil over a node with time left (`TimeLeftMaterial` in `time_left.rs`): grey over the
// share of the time already gone, swept clockwise from twelve o'clock, with a bright line along
// its edge where it is sweeping. Nothing shows at full time; at none the node is covered.

import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct Veil {
    veil: vec4<f32>,
    sweep: vec4<f32>,
    left: f32,
}

@group(1) @binding(0) var<uniform> style: Veil;

const TAU: f32 = 6.2831853;
// The sweeping line's width, in turns at the node's edge.
const LINE: f32 = 0.006;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let spent = clamp(1.0 - style.left, 0.0, 1.0);
    if spent <= 0.0 {
        return vec4(0.0);
    }
    // The uv's y runs down the node, so twelve o'clock is -y and clockwise is toward +x.
    let p = in.uv - vec2(0.5);
    let turn = fract(atan2(p.x, -p.y) / TAU + 1.0);
    // About one pixel, in turns at this radius, so the edge is soft without the seam at
    // twelve o'clock (where `turn` jumps from 1 to 0) blurring.
    let aa = max(max(fwidth(p.x), fwidth(p.y)) / max(length(p), 1e-3) / TAU, 1e-4);
    var covered = 1.0 - smoothstep(spent - aa, spent + aa, turn);
    if spent >= 1.0 {
        covered = 1.0;
    }
    var colour = style.veil.rgb;
    var alpha = style.veil.a * covered;
    if spent < 1.0 {
        let line = 1.0 - smoothstep(LINE, LINE + aa, abs(turn - spent));
        colour = mix(colour, style.sweep.rgb, line);
        alpha = max(alpha, style.sweep.a * line);
    }
    return vec4(colour, alpha);
}
