// The flourish that ends the quickslot bar: a gold rule that tapers in from the left and curls
// at its end into a spiral, like the end of a scroll. Drawn in one pass in units of the node's
// height, so it keeps its shape at any size.

import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct Scroll {
    gold: vec4<f32>,
    gold_dark: vec4<f32>,
}

@group(1) @binding(0) var<uniform> scroll: Scroll;

const TAU: f32 = 6.2831853;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    // Units of the node's height: x from 0 to the width over the height, y from 0 down to 1.
    let p = in.uv * in.size / in.size.y;
    let width = in.size.x / in.size.y;
    let aa = 1.2 / in.size.y;

    // The curl: an Archimedean spiral, one `pitch` further out each turn. Its outer end is at the
    // bottom of the curl (angle a quarter turn), where the rule leaves it going left, so the rule
    // and the spiral share a tangent.
    let pitch = 0.085;
    let outer = pitch * 3.25;
    let centre = vec2<f32>(width - outer - 0.1, 0.4);
    let q = p - centre;
    let r = length(q);
    let angle = atan2(q.y, q.x);
    let arm = abs(fract(r / pitch - angle / TAU + 0.5) - 0.5) * pitch;
    // Thicker where it is outside, thinner as it winds in.
    let reach = clamp(r / outer, 0.0, 1.0);
    let thick = mix(0.009, 0.022, reach);
    var cover = (1.0 - smoothstep(thick - aa, thick + aa, arm)) * (1.0 - smoothstep(outer - aa, outer + aa, r)) * smoothstep(0.03, 0.05, r);
    // A bead at the middle of the curl.
    cover = max(cover, 1.0 - smoothstep(0.034 - aa, 0.034 + aa, r));

    // The rule: along the bottom of the curl, from the left edge, fatter toward the curl.
    let rule_y = centre.y + outer;
    let along = clamp(p.x / max(centre.x, 1e-3), 0.0, 1.0);
    let rule_thick = mix(0.004, 0.022, pow(along, 0.6));
    let rule = (1.0 - smoothstep(rule_thick - aa, rule_thick + aa, abs(p.y - rule_y))) * step(0.0, p.x) * (1.0 - smoothstep(centre.x - aa, centre.x + aa, p.x));
    cover = max(cover, rule);

    // A lit metal wire: brighter toward the top.
    let lit = clamp(1.0 - p.y * 0.9, 0.0, 1.0);
    let colour = mix(scroll.gold_dark.rgb, scroll.gold.rgb, lit);
    return vec4<f32>(colour, cover);
}
