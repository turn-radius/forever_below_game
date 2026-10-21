// Blood round the edge of the view through the last gasp: ragged splotches that bloom in from
// every side, pooling in the corners, a few drips running down from the top and drops flung
// just past them. Fixed to the screen so it reads as blood on the eyes, not on the world.
// `spread` sets how far in it reaches and `strength` how strong it shows, which blinks.

import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct Blood {
    // The blood at a splotch's ragged edge, and the near black where it pools.
    colour: vec4<f32>,
    deep: vec4<f32>,
    // How far in from the edge it reaches, in screen heights.
    spread: f32,
    // How strong it shows, 0 to 1; even at its faintest blink it stays plain to see.
    strength: f32,
    // Moves the pattern, so each gasp's blood lies differently.
    seed: f32,
}

@group(1) @binding(0) var<uniform> blood: Blood;

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), u.x), mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var sum = 0.0;
    var amp = 0.55;
    var q = p;
    for (var i = 0; i < 4; i++) {
        sum += noise(q) * amp;
        q = q * 2.03 + vec2<f32>(1.7, 9.2);
        amp *= 0.5;
    }
    return sum;
}

// How far from the nearest edge, in screen heights.
fn edge_distance(p: vec2<f32>, aspect: f32) -> f32 {
    return min(min(p.x, aspect - p.x), min(p.y, 1.0 - p.y));
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    // Screen heights, so the splotches keep their shape on any window.
    let aspect = in.size.x / max(in.size.y, 1.0);
    let p = vec2<f32>(in.uv.x * aspect, in.uv.y);
    let aa = 1.5 / max(in.size.y, 1.0);
    let seed = vec2<f32>(blood.seed * 17.3, blood.seed * 5.1);
    let spread = blood.spread;

    // The corners hold more: blood runs into them from two sides.
    let dx = min(p.x, aspect - p.x);
    let dy = min(p.y, 1.0 - p.y);
    let corner = 1.0 - smoothstep(0.0, 0.45, max(dx, dy));
    let edge = min(dx, dy);

    // Big ragged tongues reaching in, with finer tatters along their rims.
    let tongues = fbm(p * 2.2 + seed);
    let tatters = fbm(p * 9.0 + seed * 1.7);
    let reach = spread * (0.25 + 1.5 * tongues * tongues + 0.3 * tatters + 0.9 * corner);
    let pooled = 1.0 - smoothstep(reach - aa, reach + aa, edge);

    // A few drips running down from the top, each ending in a bead.
    let drip_w = 0.06;
    let column = floor(p.x / drip_w);
    var drips = 0.0;
    for (var i = -1; i <= 1; i++) {
        let c = column + f32(i);
        let h = hash(vec2<f32>(c, 3.7) + seed);
        let x = (c + 0.2 + 0.6 * hash(vec2<f32>(c, 9.1) + seed)) * drip_w;
        let run = spread * (1.2 + 3.0 * hash(vec2<f32>(c, 4.4) + seed)) * step(0.72, h);
        let width = 0.003 + 0.006 * hash(vec2<f32>(c, 6.2) + seed);
        let along = clamp(p.y / max(run, 1e-4), 0.0, 1.0);
        let thread = 1.0 - smoothstep(width * (1.0 - 0.5 * along) - aa, width * (1.0 - 0.5 * along) + aa, abs(p.x - x));
        let bead = 1.0 - smoothstep(width * 1.6 - aa, width * 1.6 + aa, length(p - vec2<f32>(x, run)));
        drips = max(drips, max(thread * step(p.y, run), bead * step(0.001, run)));
    }

    // Drops flung just past the splotches, few and of every size.
    let cell_size = 0.05;
    let cell = floor(p / cell_size);
    var drops = 0.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let c = cell + vec2<f32>(f32(i), f32(j));
            let centre = (c + vec2<f32>(hash(c + 3.1 + seed), hash(c + 7.7 + seed))) * cell_size;
            let from_edge = edge_distance(centre, aspect);
            let band = smoothstep(spread * 0.3, spread * 0.9, from_edge) * (1.0 - smoothstep(spread * 1.4, spread * 2.4, from_edge));
            let size = hash(c + 11.3 + seed);
            let r = cell_size * (0.05 + 0.28 * size * size) * step(0.78, hash(c + seed)) * band;
            drops = max(drops, 1.0 - smoothstep(r - aa, r + aa, length(p - centre)));
        }
    }

    let cover = max(max(pooled, drips), drops);
    // Darkest where it pools deepest, at the very edge, in the corners and the tongues' hearts.
    let depth = clamp(1.0 - edge / max(reach, 1e-4), 0.0, 1.0);
    let grain = fbm(p * 26.0 + seed * 2.0);
    let rgb = mix(blood.colour.rgb, blood.deep.rgb, clamp(depth * 0.9 + corner * 0.3 + grain * 0.25 - 0.15, 0.0, 1.0));
    let strength = clamp(blood.strength, 0.0, 1.0);
    return vec4<f32>(rgb, cover * (0.6 + 0.4 * strength) * step(0.001, strength));
}
