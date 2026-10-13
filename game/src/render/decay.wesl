// Procedural wear painted on a part's base colour before lighting.
//
// Port of `DECAY_HEAD` / `DECAY_BODY` in `reference/spline_armory_engine.js`. The noise is
// sampled in item space (the part's local space), so the pattern stays on the object as it
// turns. Type ids are `armory::MaterialKind::decay_type()`; the table is in
// `docs/rendering.md` ("Wear, rust and grime").

const DECAY_NONE: u32 = 0u;
const DECAY_IRON: u32 = 1u;
const DECAY_BRASS: u32 = 2u;
const DECAY_WOOD: u32 = 3u;
const DECAY_LEATHER: u32 = 4u;
const DECAY_GEM: u32 = 5u;
const DECAY_GOLD: u32 = 6u;
const DECAY_SILVER: u32 = 7u;
const DECAY_BONE: u32 = 8u;
const DECAY_STONE: u32 = 9u;
const DECAY_CLOTH: u32 = 10u;
const DECAY_PAINT: u32 = 11u;

fn hash3(p_in: vec3<f32>) -> f32 {
    var p = fract(p_in * 0.3183099 + 0.1);
    p = p * 17.0;
    return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}

// 3D value noise, smoothstep-interpolated lattice hashes.
fn value_noise(x: vec3<f32>) -> f32 {
    let i = floor(x);
    var f = fract(x);
    f = f * f * (3.0 - 2.0 * f);
    let x00 = mix(hash3(i), hash3(i + vec3(1.0, 0.0, 0.0)), f.x);
    let x10 = mix(hash3(i + vec3(0.0, 1.0, 0.0)), hash3(i + vec3(1.0, 1.0, 0.0)), f.x);
    let x01 = mix(hash3(i + vec3(0.0, 0.0, 1.0)), hash3(i + vec3(1.0, 0.0, 1.0)), f.x);
    let x11 = mix(hash3(i + vec3(0.0, 1.0, 1.0)), hash3(i + vec3(1.0, 1.0, 1.0)), f.x);
    return mix(mix(x00, x10, f.y), mix(x01, x11, f.y), f.z);
}

// Four octaves of value noise, normalised to roughly 0..1.
fn fbm(p_in: vec3<f32>) -> f32 {
    var p = p_in;
    var amplitude = 0.5;
    var sum = 0.0;
    for (var k = 0; k < 4; k = k + 1) {
        sum = sum + amplitude * value_noise(p);
        p = p * 2.03 + vec3(1.7, 9.2, 3.1);
        amplitude = amplitude * 0.5;
    }
    return sum / 0.94;
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3(0.3, 0.59, 0.11));
}

// Returns `color` worn by `amount` (0 = as made, up to 1.2 = ruinous) for material
// `decay_type`. `item_position` is in item space; `scale` sets the size of the pattern and
// `seed` shifts it per item.
fn apply_decay(
    color: vec3<f32>,
    item_position: vec3<f32>,
    decay_type: u32,
    amount: f32,
    scale: f32,
    seed: f32,
) -> vec3<f32> {
    var c = color;
    let a = clamp(amount, 0.0, 1.2);

    // Wood grain is part of the material, not wear: drawn even on new wood.
    var grain = 0.0;
    if decay_type == DECAY_WOOD {
        let q = vec3(item_position.x * 22.0, item_position.y * 1.2, item_position.z * 22.0);
        grain = fbm(q * scale * 0.6 + vec3(seed));
        c = c * (0.92 + 0.16 * grain);
    }

    if decay_type == DECAY_NONE || a <= 0.0 {
        return c;
    }

    let p = item_position * scale + vec3(seed);
    let n = fbm(p);
    let n2 = fbm(p * 3.3 + vec3(4.1));

    if decay_type == DECAY_IRON {
        // Dulling, rust blooms, pitting.
        c = mix(c, c * vec3(0.85, 0.82, 0.8), a * 0.6);
        let threshold = 1.0 - a * 0.95;
        let bloom = smoothstep(threshold, threshold + 0.08, n * 0.7 + n2 * 0.45);
        let rust = mix(vec3(0.42, 0.18, 0.08), vec3(0.66, 0.34, 0.14), smoothstep(0.3, 0.8, n2));
        c = mix(c, rust, bloom * min(1.0, a * 1.6));
        c = c * (1.0 - 0.45 * smoothstep(0.86 - a * 0.2, 0.9 - a * 0.2, n2) * smoothstep(0.2, 0.5, a));
    } else if decay_type == DECAY_BRASS {
        // Tarnish, verdigris.
        c = mix(c, c * vec3(0.62, 0.52, 0.38), a * 0.7);
        let threshold = 1.0 - a * 0.85;
        let bloom = smoothstep(threshold, threshold + 0.07, n * 0.6 + n2 * 0.5);
        c = mix(c, mix(vec3(0.30, 0.55, 0.47), vec3(0.45, 0.66, 0.56), n2), bloom * min(1.0, a * 1.5));
    } else if decay_type == DECAY_WOOD {
        // Silvering, rot, moss.
        c = mix(c, vec3(luminance(c)) * vec3(0.95, 0.95, 0.92) * 1.15, a * 0.65);
        c = c * (1.0 - a * 0.25 * smoothstep(0.55, 0.75, grain));
        let threshold = 1.0 - a * 0.8;
        let rot = smoothstep(threshold, threshold + 0.06, n * 0.65 + n2 * 0.4);
        c = mix(c, vec3(0.17, 0.12, 0.08), rot * min(1.0, a * 1.4));
        let moss = smoothstep(0.78 - a * 0.25, 0.82 - a * 0.25, n2) * smoothstep(0.5, 0.9, a);
        c = mix(c, vec3(0.36, 0.42, 0.22), 0.8 * moss);
    } else if decay_type == DECAY_LEATHER {
        // Darkening, scuffs, grime.
        c = mix(c, c * 0.7, a * 0.5);
        let scuff = smoothstep(0.75 - a * 0.3, 0.8 - a * 0.3, n2) * min(1.0, a * 1.3);
        c = mix(c, c * 1.6 + vec3(0.06, 0.05, 0.03), scuff);
        let grime = smoothstep(1.0 - a * 0.7, 1.05 - a * 0.7, n) * min(1.0, a * 2.0);
        c = mix(c, vec3(0.10, 0.08, 0.06), 0.8 * grime);
    } else if decay_type == DECAY_GEM {
        // Clouding, fractures.
        c = mix(c, vec3(luminance(c)) * 0.85, a * 0.6);
        c = mix(c, vec3(0.92), smoothstep(0.88 - a * 0.15, 0.9 - a * 0.15, n2) * a);
    } else if decay_type == DECAY_GOLD {
        // Soft dulling, grime in the hollows.
        c = mix(c, c * vec3(0.86, 0.8, 0.68), a * 0.45);
        let grime = smoothstep(1.0 - a * 0.6, 1.05 - a * 0.6, n * 0.6 + n2 * 0.5) * min(1.0, a * 1.2);
        c = mix(c, vec3(0.24, 0.19, 0.12), grime * 0.7);
    } else if decay_type == DECAY_SILVER {
        // Tarnish, warm toning, black patches.
        c = mix(c, c * vec3(0.74, 0.72, 0.7), a * 0.6);
        c = mix(c, c * vec3(0.92, 0.8, 0.62), smoothstep(0.4, 0.7, n2) * a * 0.45);
        let blotch = smoothstep(1.0 - a * 0.45, 1.06 - a * 0.45, n * 0.6 + n2 * 0.5) * min(1.0, a * 1.5);
        c = mix(c, vec3(0.16, 0.15, 0.16), blotch * 0.75);
    } else if decay_type == DECAY_BONE {
        // Yellowing, stains, hairline cracks.
        c = mix(c, c * vec3(1.0, 0.86, 0.64), a * 0.75);
        let stain = smoothstep(1.0 - a * 0.7, 1.06 - a * 0.7, n * 0.6 + n2 * 0.5) * min(1.0, a * 1.4);
        c = mix(c, vec3(0.42, 0.3, 0.18), stain * 0.8);
        let crack = 1.0 - smoothstep(0.0, 0.012 * a + 0.001, abs(n2 - 0.5));
        c = c * (1.0 - 0.6 * crack * step(0.25, a));
    } else if decay_type == DECAY_STONE {
        // Weathering, lichen.
        c = mix(c, c * 0.78, a * 0.4);
        let lichen = smoothstep(0.8 - a * 0.18, 0.84 - a * 0.18, n2) * min(1.0, a * 1.5);
        c = mix(c, vec3(0.55, 0.6, 0.42), lichen * 0.75);
        let rust_lichen = smoothstep(0.88 - a * 0.1, 0.9 - a * 0.1, n) * min(1.0, a * 1.5);
        c = mix(c, vec3(0.78, 0.52, 0.2), rust_lichen * 0.65);
    } else if decay_type == DECAY_CLOTH {
        // Fading, grime, moth holes (drawn as dark spots).
        c = mix(c, vec3(luminance(c)) * vec3(1.0, 0.97, 0.9) + 0.06, a * 0.4);
        let grime = smoothstep(0.9 - a * 0.5, 1.0 - a * 0.5, n * 0.7 + n2 * 0.4) * a * 0.9;
        c = mix(c, c * vec3(0.62, 0.55, 0.45), grime);
        let hole = smoothstep(0.88 - a * 0.12, 0.9 - a * 0.12, n2) * step(0.35, a);
        c = mix(c, vec3(0.06, 0.05, 0.05), hole);
    } else if decay_type == DECAY_PAINT {
        // Fading, chips showing the boards. The prototype chips even new paint; scaling by
        // `a` keeps a fresh coat whole.
        c = mix(c, c * 0.85 + 0.05, a * 0.35);
        let chip = smoothstep(0.95 - a * 0.45, 0.98 - a * 0.45, n2 * 0.7 + n * 0.45) * min(1.0, a * 4.0);
        c = mix(c, vec3(0.45, 0.31, 0.19) * (0.85 + 0.3 * n), chip);
    } else {
        // Food: browning, going stale, mould.
        c = mix(c, c * vec3(0.8, 0.7, 0.56), a * 0.5);
        let mould = smoothstep(0.86 - a * 0.3, 0.9 - a * 0.3, n2 * 0.6 + n * 0.5) * step(0.3, a);
        c = mix(c, mix(vec3(0.55, 0.62, 0.45), vec3(0.86, 0.88, 0.82), n2), mould);
    }
    return c;
}
