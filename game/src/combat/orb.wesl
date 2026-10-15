// A vital orb: a glass globe in a gold frame, filled with liquid to `fill` of its height, as the
// life, stamina and mana orbs of the HUD. The liquid has a surface that sloshes gently, a bright
// line along it, and a deep colour at the bottom; the glass has a shadow toward its rim and a
// glint at the top left. The whole thing is drawn in one pass inside the node's square, which
// is why the node must be a square: the orb is the circle that fits it.

import bevy_render::globals::Globals;
import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct Orb {
    // The liquid at its surface and at the bottom of the globe.
    surface: vec4<f32>,
    deep: vec4<f32>,
    // The frame's bright metal and its shadowed metal.
    gold: vec4<f32>,
    gold_dark: vec4<f32>,
    // How full, from 0 to 1.
    fill: f32,
    // Where in its slosh this orb starts, so the three do not move together.
    phase: f32,
    // The surface's wave height, as a share of the globe's radius.
    slosh: f32,
    // From 0 to 1, how far the orb is darkened (a winded delver's stamina).
    dim: f32,
    // From 0 to 1, how hot the frame and liquid are: a blow's pulse flashes them toward red-white.
    pulse: f32,
}

@group(0) @binding(1) var<uniform> globals: Globals;
@group(1) @binding(0) var<uniform> orb: Orb;

const PI: f32 = 3.1415927;
// The globe's radius inside the frame, as a share of the node's half width.
const GLOBE: f32 = 0.84;
// The frame's outer edge.
const RIM: f32 = 0.985;

// A soft step from 0 to 1 over `width` ending at `edge`.
fn edge(x: f32, e: f32, width: f32) -> f32 {
    return smoothstep(e - width, e, x);
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let p = (in.uv - vec2<f32>(0.5)) * 2.0;
    let r = length(p);
    // About one pixel, in the orb's own units.
    let aa = max(max(fwidth(p.x), fwidth(p.y)), 1e-4);

    let silhouette = 1.0 - edge(r, RIM + aa, 2.0 * aa);
    if silhouette <= 0.0 {
        return vec4<f32>(0.0);
    }

    let n = p / max(r, 1e-4);
    // Light from the top left.
    let toward_light = dot(n, normalize(vec2<f32>(-0.55, -0.83)));

    // The frame: a rounded band of metal, bright on the side toward the light, with a dark groove
    // where it meets the glass and a dark line along its outer edge.
    let band = clamp((r - GLOBE) / (RIM - GLOBE), 0.0, 1.0);
    let bevel = sin(band * PI);
    let lit = clamp(0.5 + 0.5 * toward_light, 0.0, 1.0);
    var metal = mix(orb.gold_dark.rgb, orb.gold.rgb, clamp(0.12 + 0.95 * lit * (0.35 + 0.65 * bevel), 0.0, 1.0));
    metal += vec3<f32>(1.0, 0.9, 0.7) * 0.3 * pow(max(toward_light, 0.0), 8.0) * bevel;
    let groove = 1.0 - smoothstep(0.0, 0.09, abs(band - 0.03));
    let outline = smoothstep(0.9, 1.0, band);
    metal = mix(metal, orb.gold_dark.rgb * 0.3, max(groove * 0.85, outline * 0.8));

    // The globe, with the liquid in it.
    let q = p / GLOBE;
    let t = globals.time;
    // The surface is flat at nothing and at full, and ripples between.
    let ripple = orb.slosh * (1.0 - pow(abs(orb.fill * 2.0 - 1.0), 6.0));
    let wave = ripple * (sin(q.x * 2.6 + t * 1.7 + orb.phase) * 0.7 + sin(q.x * 5.1 - t * 2.3 + orb.phase * 1.7) * 0.3);
    // Down is positive y; the globe holds `fill` of its height from the bottom.
    let level = 1.0 - 2.0 * orb.fill + wave;
    let depth = q.y - level;
    let liquid = edge(depth + aa * 2.0, 0.0, aa * 4.0) * step(0.0, orb.fill - 0.0005);

    let sink = clamp(depth / 1.6, 0.0, 1.0);
    var fluid = mix(orb.surface.rgb, orb.deep.rgb, sink);
    // A bright line along the surface, and a lighter band just under it. Only below the surface:
    // above it the exponentials would overflow.
    let under = max(depth, 0.0);
    let lighter = min(orb.surface.rgb * 1.3 + vec3<f32>(0.06), vec3<f32>(1.0));
    fluid = mix(fluid, lighter, 0.7 * exp(-under * 45.0) + 0.15 * exp(-under * 5.0));
    // Slow bands of light moving through the liquid.
    fluid *= 0.93 + 0.07 * sin(q.x * 6.0 + q.y * 3.0 + t * 0.9 + orb.phase);

    // The empty glass: near black, a touch lighter low where the glass thickens.
    var glass = mix(vec3<f32>(0.035, 0.03, 0.028), vec3<f32>(0.075, 0.065, 0.06), clamp(q.y * 0.5 + 0.5, 0.0, 1.0));
    var inside = mix(glass, fluid, liquid);

    // The globe's roundness: dark toward its rim, a little light through the liquid's lower side.
    let rq = clamp(length(q), 0.0, 1.0);
    inside *= 1.0 - 0.6 * smoothstep(0.55, 1.0, rq);
    inside += liquid * orb.surface.rgb * 0.12 * smoothstep(0.4, 1.0, rq) * max(-toward_light, 0.0);

    // The glint: a soft crescent at the top left and a small bright point.
    let glint_at = q - vec2<f32>(-0.36, -0.46);
    let glint = exp(-dot(glint_at * vec2<f32>(1.0, 1.9), glint_at * vec2<f32>(1.0, 1.9)) * 9.0);
    let point_at = q - vec2<f32>(-0.5, -0.58);
    let point = exp(-dot(point_at, point_at) * 90.0);
    inside += vec3<f32>(0.5, 0.52, 0.55) * (0.22 * glint + 0.55 * point);

    inside *= 1.0 - orb.dim * 0.55;

    // A blow's pulse: the liquid and the frame flash toward a hot red and white.
    let hot = vec3<f32>(1.0, 0.62, 0.52);
    inside = mix(inside, hot, orb.pulse * 0.6);
    metal = mix(metal, hot * 1.1, orb.pulse * 0.75);

    let in_globe = 1.0 - edge(r, GLOBE + aa, 2.0 * aa);
    let rgb = mix(metal, inside, in_globe);
    return vec4<f32>(rgb, silhouette);
}
