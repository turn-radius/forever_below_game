// Etching painted on a part's base colour after its pattern and before wear: engraving on
// metal, carving on wood and bone, tooling on leather, embroidery on cloth. Colours in and out
// are display (sRGB) values.
//
// Port of `etch2()` and the `uEtch` lines of `DECAY_BODY` in
// `reference/spline_armory_engine.js`. Ids are `armory::etch::EtchPattern::shader_id()`. The
// pattern is projected three ways in item space and blended by the item-space normal, so it is
// fixed to the item as it turns. See `docs/rendering.md`.

import super::decay::{fbm, value_noise};

const ETCH_NONE: u32 = 0u;
const ETCH_SCROLLWORK: u32 = 1u;
const ETCH_BORDER_BANDS: u32 = 2u;
const ETCH_KNOTWORK: u32 = 3u;
const ETCH_CHEVRONS: u32 = 4u;
const ETCH_DIAPER: u32 = 5u;
const ETCH_RUNES: u32 = 6u;
const PI: f32 = 3.14159265359;

// What the shader needs of one part's etching (`Etch` in `toon.rs`).
struct Etch {
    id: u32,
    color: vec3<f32>,
    scale: f32,
    weight: f32,
    coverage: f32,
    inlaid: u32,
    darken: f32,
    stitched: u32,
}

// GLSL's `mod`: always between 0 and `y`.
fn etch_mod(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

// `h2` in the prototype.
fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

// Distance from `p` to the segment `a`-`b`.
fn segment_distance(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

// A line of half width `w` at distance `d`, softened at its edge (`lineM`).
fn line_mask(d: f32, w: f32) -> f32 {
    return 1.0 - smoothstep(w, w * 1.7 + 0.01, d);
}

// One flat projection of the pattern, 0..1 where a line is cut.
fn etch_plane(p: vec2<f32>, id: u32, w: f32, seed: f32) -> f32 {
    if id == ETCH_SCROLLWORK {
        // Contours of a little noise: lines that curl round each other.
        let n = value_noise(vec3(p * 0.9, 3.1)) + 0.5 * value_noise(vec3(p * 1.9, 7.3));
        return line_mask(abs(fract(n * 4.0) - 0.5), w * 0.8);
    }
    if id == ETCH_BORDER_BANDS {
        // Pairs of rules with a zigzag between, a band every four units.
        let y = etch_mod(p.y, 4.0);
        let zig = 0.7 + 0.35 * (abs(fract(p.x * 0.8) - 0.5) * 2.0 - 0.5);
        var m = max(line_mask(abs(y - 0.15), w), line_mask(abs(y - 1.25), w));
        m = max(m, line_mask(abs(y - zig), w));
        return select(0.0, m, y < 1.5);
    }
    if id == ETCH_KNOTWORK {
        // Two waves crossing back and forth.
        let a = abs(fract(p.y * 0.5 + 0.22 * sin(p.x * PI)) - 0.5);
        let b = abs(fract(p.y * 0.5 - 0.22 * sin(p.x * PI)) - 0.5);
        return max(line_mask(a, w * 0.5), line_mask(b, w * 0.5));
    }
    if id == ETCH_CHEVRONS {
        return line_mask(abs(fract(p.y * 0.5 + abs(fract(p.x * 0.5) - 0.5)) - 0.5), w * 0.5);
    }
    if id == ETCH_DIAPER {
        // A diagonal lattice with a dot in each diamond.
        let a = abs(fract((p.x + p.y) * 0.5) - 0.5);
        let b = abs(fract((p.x - p.y) * 0.5) - 0.5);
        let c = fract(vec2(p.x + p.y, p.x - p.y) * 0.5) - 0.5;
        return max(max(line_mask(a, w * 0.5), line_mask(b, w * 0.5)), line_mask(max(length(c) - 0.08, 0.0), w * 0.6));
    }
    if id == ETCH_RUNES {
        // A column of runes, each a stave with seeded twigs.
        if abs(p.x) > 0.55 {
            return 0.0;
        }
        let cell = floor(vec2(p.x + 0.5, p.y / 1.4));
        let q = vec2(fract(p.x + 0.5), fract(p.y / 1.4) * 1.4);
        let h = hash2(cell + vec2(seed));
        var d = segment_distance(q, vec2(0.5, 0.15), vec2(0.5, 1.25));
        if etch_mod(floor(h * 2.0), 2.0) > 0.5 {
            d = min(d, segment_distance(q, vec2(0.5, 1.1), vec2(0.85, 0.8)));
        }
        if etch_mod(floor(h * 4.0), 2.0) > 0.5 {
            d = min(d, segment_distance(q, vec2(0.5, 1.1), vec2(0.15, 0.8)));
        }
        if etch_mod(floor(h * 8.0), 2.0) > 0.5 {
            d = min(d, segment_distance(q, vec2(0.5, 0.7), vec2(0.85, 0.4)));
        }
        if etch_mod(floor(h * 16.0), 2.0) > 0.5 {
            d = min(d, segment_distance(q, vec2(0.5, 0.6), vec2(0.15, 0.9)));
        }
        return line_mask(d, w * 0.6);
    }
    return 0.0;
}

// Works `etch` into `color`. `item_position` and `item_normal` are in item space; the decay
// arguments wear the lines away with the surface, as the prototype does.
fn apply_etch(
    color: vec3<f32>,
    item_position: vec3<f32>,
    item_normal: vec3<f32>,
    etch: Etch,
    decay_amount: f32,
    decay_scale: f32,
    seed: f32,
) -> vec3<f32> {
    if etch.id == ETCH_NONE {
        return color;
    }
    // Three projections blended by the normal, sharpened so each face takes mostly one.
    var weights = pow(abs(item_normal), vec3(4.0));
    weights = weights / (weights.x + weights.y + weights.z + 1e-5);
    let q = item_position * etch.scale;
    var mask = etch_plane(q.zy, etch.id, etch.weight, seed) * weights.x
        + etch_plane(q.xz, etch.id, etch.weight, seed) * weights.y
        + etch_plane(q.xy, etch.id, etch.weight, seed) * weights.z;
    if etch.coverage < 0.99 {
        let field = value_noise(item_position * etch.scale * 0.15 + vec3(9.0));
        mask = mask * smoothstep(1.0 - etch.coverage - 0.06, 1.0 - etch.coverage + 0.06, field);
    }
    // Wear rubs the lines out where the surface is worst.
    let a = clamp(decay_amount, 0.0, 1.2);
    let p = item_position * decay_scale + vec3(seed);
    let n2 = fbm(p * 3.3 + vec3(4.1));
    mask = mask * clamp(1.0 - a * 0.8 * smoothstep(0.3, 0.8, n2), 0.0, 1.0);

    var line = select(color * etch.darken, mix(color, etch.color, 0.9), etch.inlaid != 0u);
    if etch.stitched != 0u {
        // Thread laid in slanted stitches: each stitch catches the light along its length and
        // falls into shadow where it meets the next (our own, for embroidery).
        let stitch = fract((q.x + q.y + q.z) * 6.0);
        line = line * (0.8 + 0.3 * smoothstep(0.0, 0.5, stitch) * (1.0 - smoothstep(0.5, 1.0, stitch)));
    }
    return mix(color, line, mask);
}
