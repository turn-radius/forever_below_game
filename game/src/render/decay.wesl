// Procedural wear painted on a part's base colour before lighting. Colours in and out are
// display (sRGB) values, as in the prototype.
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
const DECAY_SKIN: u32 = 13u;
// Cave rock and layered cave rock (ours): stone that also breaks into facets or beds
// (`rock_shade`, `strata_shade`), weathered as type 9 is.
const DECAY_ROCK: u32 = 14u;
const DECAY_STRATA: u32 = 15u;
const DECAY_MARBLE: u32 = 16u;
const DECAY_QUARTZ: u32 = 17u;
const DECAY_LIMESTONE: u32 = 18u;

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

// How much of a detail survives on screen: `frequency` is its lattice cells per unit of the
// pattern and `cell` the units one pixel spans (`fwidth`), so `frequency * cell` is lattice
// cells per pixel. Kept whole while a lattice cell spans four pixels or more, gone by two
// (the Nyquist limit), so fine noise fades to its mean instead of shimmering as the view moves.
// Band limiting by the pixel footprint is Inigo Quilez's ("Band limited synthesis",
// https://iquilezles.org/articles/bandlimiting/); the 2 to 4 pixel window is our own.
fn detail(frequency: f32, cell: f32) -> f32 {
    return 1.0 - smoothstep(0.25, 0.5, frequency * cell);
}

// Value noise at `p`, faded to its mean (0.5) where a pixel spans `cell` units or more of `p`.
fn filtered_noise(p: vec3<f32>, cell: f32) -> f32 {
    return mix(0.5, value_noise(p), detail(1.0, cell));
}

// Four octaves of value noise, normalised to roughly 0..1. Each octave fades to its mean once
// it is finer than a pixel spans (`cell`, in units of `p_in`; 0 keeps every octave).
fn fbm(p_in: vec3<f32>, cell: f32) -> f32 {
    var p = p_in;
    var amplitude = 0.5;
    var frequency = 1.0;
    var sum = 0.0;
    for (var k = 0; k < 4; k = k + 1) {
        // An octave faded out entirely is its mean: skip sampling it, which far stone does
        // for most of its octaves.
        let keep = detail(frequency, cell);
        if keep > 0.0 {
            sum = sum + amplitude * mix(0.5, value_noise(p), keep);
        } else {
            sum = sum + amplitude * 0.5;
        }
        p = p * 2.03 + vec3(1.7, 9.2, 3.1);
        amplitude = amplitude * 0.5;
        frequency = frequency * 2.03;
    }
    return sum / 0.94;
}

// Two fbm samples blended into one patch field, rescaled to zero mean and unit spread
// (measured: `0.75 n + 0.35 n2` has mean 0.548, standard deviation 0.094).
fn patch_field(n: f32, n2: f32) -> f32 {
    return (n * 0.75 + n2 * 0.35 - 0.548) / 0.094;
}

// A 0..1 mask over about `coverage` of the surface: `field` (from `patch_field`) thresholded
// at the logistic estimate of its `1 - coverage` quantile, blurred by `softness` (in standard
// deviations) at the edges. No coverage, no mask.
fn patches(field: f32, coverage: f32, softness: f32) -> f32 {
    if coverage <= 0.0 {
        return 0.0;
    }
    let c = clamp(coverage, 0.001, 0.999);
    let threshold = log((1.0 - c) / c) / 1.702;
    return smoothstep(threshold - softness * 0.5, threshold + softness * 0.5, field);
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3(0.3, 0.59, 0.11));
}

// Returns `color` worn by `amount` (0 = as made, up to 1.2 = ruinous) for material
// `decay_type`. `item_position` is in item space; `scale` sets the size of the pattern and
// `seed` shifts it per item. `pixel` is how many item units one pixel spans
// (`length(fwidth(item_position))`): detail finer than that fades out (`detail`).
fn apply_decay(
    color: vec3<f32>,
    item_position: vec3<f32>,
    pixel: f32,
    decay_type: u32,
    amount: f32,
    rust_amount: f32,
    scale: f32,
    seed: f32,
) -> vec3<f32> {
    var c = color;
    // Units of the pattern (`p` below) one pixel spans.
    let cell = pixel * scale;
    let a = clamp(amount, 0.0, 1.2);

    // Wood grain is part of the material, not wear: drawn even on new wood.
    var grain = 0.0;
    if decay_type == DECAY_WOOD {
        let q = vec3(item_position.x * 22.0, item_position.y * 1.2, item_position.z * 22.0);
        grain = fbm(q * scale * 0.6 + vec3(seed), cell * 22.0 * 0.6);
        c = c * (0.92 + 0.16 * grain);
    }

    if decay_type == DECAY_NONE || a <= 0.0 {
        return c;
    }

    // How far along the decay is: 0 as made, 1 at the ruinous end (a grave find).
    let t = a / 1.2;
    let p = item_position * scale + vec3(seed);
    let n = fbm(p, cell);
    let n2 = fbm(p * 3.3 + vec3(4.1), cell * 3.3);

    if decay_type == DECAY_IRON {
        // Wear dulls and stains iron; only `rust_amount` brings rust blooms and pits. Even a
        // grave find keeps about half its surface bare.
        let tr = clamp(rust_amount, 0.0, 1.2) / 1.2;
        c = mix(c, c * vec3(0.8, 0.79, 0.78), t * 0.7);
        // Blooms come from a coarser sample than `n`, so they spread a few centimetres.
        let blooms = patch_field(fbm(p * 0.5 + vec3(5.2), cell * 0.5), n2);
        let bloom = patches(blooms, tr * 0.5, 0.12);
        // Wear alone stains it grey; the warm stain ringing a bloom is rust's.
        let grime = patches(blooms, t * 0.45, 0.5);
        c = mix(c, c * vec3(0.78, 0.77, 0.76), grime * 0.45);
        let stain = patches(blooms, tr * 0.68, 0.5);
        c = mix(c, c * vec3(0.8, 0.7, 0.62), stain * 0.55);
        // Rust in muted browns, laid over the iron rather than replacing it, so a rusted piece
        // still reads as iron under the toon bands.
        let crust = filtered_noise(p * 7.0 + vec3(2.3), cell * 7.0);
        let rust = mix(vec3(0.3, 0.18, 0.12), vec3(0.46, 0.29, 0.18), smoothstep(0.3, 0.75, crust * 0.6 + n2 * 0.4));
        c = mix(c, rust, bloom * 0.7);
        // Pits: dark specks, thickest in the blooms.
        let pit = smoothstep(0.0, 0.03, filtered_noise(p * 15.0 + vec3(7.3), cell * 15.0) - mix(0.97, 0.88, tr) + 0.1 * bloom);
        c = mix(c, vec3(0.1, 0.06, 0.04), pit * smoothstep(0.1, 0.3, tr) * 0.85);
    } else if decay_type == DECAY_BRASS {
        // Tarnish darkening the whole surface, then crusts of verdigris in patches.
        c = mix(c, c * vec3(0.66, 0.56, 0.4), t * 0.8);
        let field = patch_field(fbm(p * 0.6 + vec3(8.4), cell * 0.6), n2);
        let crust = patches(field, t * 0.25, 0.12);
        // A darker green ring where the crust thins out.
        let rim = patches(field, t * 0.36, 0.4) * (1.0 - crust);
        c = mix(c, c * vec3(0.55, 0.68, 0.58), rim * 0.7);
        let verdigris = mix(vec3(0.3, 0.55, 0.47), vec3(0.45, 0.66, 0.56), smoothstep(0.35, 0.65, filtered_noise(p * 6.0, cell * 6.0)));
        c = mix(c, verdigris, crust);
    } else if decay_type == DECAY_WOOD {
        // Silvering, rot, moss.
        c = mix(c, vec3(luminance(c)) * vec3(0.95, 0.95, 0.92) * 1.15, a * 0.65);
        c = c * (1.0 - a * 0.25 * smoothstep(0.55, 0.75, grain));
        let rot = patches(patch_field(n, n2), t * 0.2, 0.2);
        c = mix(c, vec3(0.17, 0.12, 0.08), rot);
        let moss = patches(patch_field(n2, fbm(p * 0.7 + vec3(3.9), cell * 0.7)), 0.2 * smoothstep(0.4, 0.75, a), 0.15);
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
        let grime = patches(patch_field(n, n2), t * 0.3, 0.3);
        c = mix(c, vec3(0.24, 0.19, 0.12), grime * 0.7);
    } else if decay_type == DECAY_SILVER {
        // Tarnish, warm toning, black patches.
        c = mix(c, c * vec3(0.74, 0.72, 0.7), a * 0.6);
        c = mix(c, c * vec3(0.92, 0.8, 0.62), smoothstep(0.4, 0.7, n2) * a * 0.45);
        let blotch = patches(patch_field(n, n2), t * 0.35, 0.15);
        c = mix(c, vec3(0.16, 0.15, 0.16), blotch * 0.75);
    } else if decay_type == DECAY_BONE {
        // Yellowing, stains, hairline cracks.
        c = mix(c, c * vec3(1.0, 0.86, 0.64), a * 0.75);
        let stain = smoothstep(1.0 - a * 0.7, 1.06 - a * 0.7, n * 0.6 + n2 * 0.5) * min(1.0, a * 1.4);
        c = mix(c, vec3(0.42, 0.3, 0.18), stain * 0.8);
        let crack = 1.0 - smoothstep(0.0, 0.012 * a + 0.001, abs(n2 - 0.5));
        c = c * (1.0 - 0.6 * crack * step(0.25, a));
    } else if decay_type == DECAY_STONE || decay_type == DECAY_ROCK || decay_type == DECAY_STRATA || decay_type == DECAY_MARBLE || decay_type == DECAY_QUARTZ || decay_type == DECAY_LIMESTONE {
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
    } else if decay_type == DECAY_SKIN {
        // Dirt on a folk's skin and hair (`amount` is the character's dirt): a dull grey-brown
        // film, then smudges that pool low on the body, as the plate's body dirt does. Item
        // space is body space here, feet at y = 0 and 10 cm units, so `low` fades out up
        // the legs (our own falloff).
        c = mix(c, c * vec3(0.84, 0.8, 0.74), a * 0.45);
        let low = 1.0 - smoothstep(0.0, 9.0, item_position.y);
        let smudge = patches(patch_field(n, n2), t * (0.18 + 0.5 * low), 0.35);
        c = mix(c, c * vec3(0.5, 0.43, 0.35), smudge * 0.75);
    } else {
        // Food: browning, going stale, mould.
        c = mix(c, c * vec3(0.8, 0.7, 0.56), a * 0.5);
        let mould = smoothstep(0.86 - a * 0.3, 0.9 - a * 0.3, n2 * 0.6 + n * 0.5) * step(0.3, a);
        c = mix(c, mix(vec3(0.55, 0.62, 0.45), vec3(0.86, 0.88, 0.82), n2), mould);
    }
    return c;
}

// A 3D hash into three channels, 0..1 each.
fn hash33(c: vec3<f32>) -> vec3<f32> {
    return vec3(hash3(c), hash3(c + vec3(31.7, 12.3, 5.1)), hash3(c + vec3(7.9, 41.1, 23.3)));
}

// The nearest two points of a jittered lattice (one point in each unit cell) to `p`: their
// distances and the cells they sit in.
struct Cells {
    f1: f32,
    f2: f32,
    id1: vec3<f32>,
    id2: vec3<f32>,
}

fn nearest_cells(p: vec3<f32>) -> Cells {
    let base = floor(p);
    var out = Cells(8.0, 8.0, vec3(0.0), vec3(0.0));
    for (var z = -1; z <= 1; z = z + 1) {
        for (var y = -1; y <= 1; y = y + 1) {
            for (var x = -1; x <= 1; x = x + 1) {
                let id = base + vec3(f32(x), f32(y), f32(z));
                let d = distance(p, id + 0.1 + 0.8 * hash33(id));
                if d < out.f1 {
                    out.f2 = out.f1;
                    out.id2 = out.id1;
                    out.f1 = d;
                    out.id1 = id;
                } else if d < out.f2 {
                    out.f2 = d;
                    out.id2 = id;
                }
            }
        }
    }
    return out;
}

// How rock shades differently from plain stone: a shading normal broken into facets or beds,
// and a tone per facet or bed with dark fissures or seams in it.
struct RockShade {
    normal: vec3<f32>,
    tone: vec3<f32>,
}

// Gradient of value noise at `q` by forward differences `e` apart: four samples.
fn noise_gradient(q: vec3<f32>, e: f32) -> vec3<f32> {
    let v = value_noise(q);
    return vec3(
        value_noise(q + vec3(e, 0.0, 0.0)) - v,
        value_noise(q + vec3(0.0, e, 0.0)) - v,
        value_noise(q + vec3(0.0, 0.0, e)) - v,
    ) / e;
}

// Craggy cave rock (ours), for hewn walls only, in world units of 10 cm: one voronoi pass of
// facets 50 to 80 cm across, each tilted its own way (gently, so the toon bands step from facet
// to facet without the wall turning busy) and a few dark narrow fissures along facet borders.
// Only the walls of hewn quarters take it; their floors and ceilings are plain stone. Everything
// fades to plain stone as it shrinks on screen: a facet is whole at 66 pixels across and gone
// by 33, a fissure at about three pixels wide and gone by one, and past that distance the
// voronoi is skipped altogether, which is most of a view.
fn rock_shade(position: vec3<f32>, normal: vec3<f32>, pixel: f32, seed: f32) -> RockShade {
    let flat_amount = smoothstep(0.5, 0.8, abs(normal.y));
    let size = mix(6.5, 3.0, flat_amount);
    var n = normal;
    var tone = 1.0;
    var fissure = 0.0;

    let facets = 1.0 - smoothstep(0.015, 0.03, pixel / size);
    if facets > 0.0 {
        // Warped a little, so facet borders wander instead of running straight.
        var p = position / size + vec3(seed * 1.3);
        p = p + (value_noise(p * 1.3) - 0.5) * 0.3;
        let cells = nearest_cells(p);
        let id = cells.id1;
        let tilt_dir = hash33(id) * 2.0 - 1.0;
        let tilt = tilt_dir - normal * dot(tilt_dir, normal);
        let lean = 0.4 + 0.8 * hash3(id + vec3(5.5));
        n = normalize(normal + tilt * lean * mix(1.0, 0.35, flat_amount) * facets);

        // Fissures run along the border between two facets, on about one border in twenty, and
        // broken along their length.
        let border = (cells.f2 - cells.f1) * size;
        let keeps = step(0.95, hash3(cells.id1 + cells.id2 + vec3(3.3)));
        let broken = smoothstep(0.35, 0.55, value_noise(position * 0.45 + vec3(seed)));
        fissure = (1.0 - smoothstep(0.05, 0.2, border)) * keeps * broken * (1.0 - 0.7 * flat_amount) * (1.0 - smoothstep(0.1, 0.25, pixel));
        // A facet's own tone, and a little less light on one leaning away.
        tone = mix(1.0, 0.97 + 0.06 * hash3(id + vec3(9.1)), facets);
        tone = tone * mix(1.0, mix(0.86, 1.02, smoothstep(0.55, 1.0, dot(n, normal))), facets);
    }

    // A faint bump rides over the facets.
    let coarse = detail(0.9, pixel);
    if coarse > 0.0 {
        let bump = noise_gradient(position * 0.9 + vec3(seed), 0.15) * 0.09 * coarse;
        n = normalize(n + bump - normal * dot(bump, normal));
    }
    return RockShade(n, vec3(tone * (1.0 - 0.6 * fissure)));
}

// Layered cave rock (ours): horizontal beds 30 to 60 cm thick that wobble with height, in
// alternating tones, each topped by a ledge (the normal tilts up at its top and down at its
// foot) with a dark seam between beds. Fades to plain stone as a bed shrinks under about six
// pixels.
fn strata_shade(position: vec3<f32>, normal: vec3<f32>, pixel: f32, seed: f32) -> RockShade {
    let wobble = value_noise(vec3(position.x * 0.06, position.y * 0.1, position.z * 0.06) + vec3(seed)) - 0.5;
    let wander = value_noise(vec3(position.x * 0.17, 0.0, position.z * 0.17) + vec3(seed + 5.0)) - 0.5;
    let u = position.y / 3.8 + wobble * 1.6 + wander * 0.9;
    let bed = floor(u);
    let f = fract(u);
    let beds = 1.0 - smoothstep(0.35, 0.75, pixel);
    let wall = 1.0 - abs(normal.y);

    let ledge = smoothstep(0.76, 0.96, f) - (1.0 - smoothstep(0.03, 0.24, f));
    let grain = (value_noise(vec3(position.x * 1.2, position.y * 6.0, position.z * 1.2) + vec3(seed)) - 0.5) * 0.35 * detail(6.0, pixel);
    let n = normalize(normal + vec3(0.0, (ledge * 1.5 * beds + grain) * wall, 0.0));

    let h = hash3(vec3(bed, seed, 2.0));
    let warm = mix(vec3(1.0, 0.98, 0.94), vec3(0.95, 0.98, 1.02), hash3(vec3(bed, 7.0, seed)));
    let seam = (1.0 - smoothstep(0.008, 0.035, f)) * (1.0 - smoothstep(0.12, 0.3, pixel));
    let streak = 0.9 + 0.2 * value_noise(vec3(position.x * 0.5, position.y * 7.0, position.z * 0.5) + vec3(seed + 11.0));
    let lip = 1.0 + 0.18 * smoothstep(0.8, 0.95, f) * (1.0 - smoothstep(0.97, 1.0, f));
    let tone = mix(vec3(1.0), warm * (0.82 + 0.3 * h) * mix(1.0, streak, detail(7.0, pixel)) * lip, beds) * (1.0 - 0.55 * seam);
    return RockShade(n, tone);
}


// Marble (ours): pale stone threaded with grey veins that wander in long curves, a few fine
// hairlines beside them, and a faint soft mottling. The normal is left alone; marble is smooth.
fn marble_shade(position: vec3<f32>, normal: vec3<f32>, pixel: f32, seed: f32) -> RockShade {
    let warp = vec3(value_noise(position * 0.035 + vec3(seed)) - 0.5, value_noise(position * 0.035 + vec3(seed + 9.0)) - 0.5, value_noise(position * 0.035 + vec3(seed + 3.0)) - 0.5) * 5.0;
    let p = position + warp * 3.0;
    let ridge = abs(value_noise(p * 0.045 + vec3(seed + 21.0)) - 0.5) * 2.0;
    let vein = (1.0 - smoothstep(0.0, 0.06, ridge)) * (1.0 - smoothstep(0.3, 0.9, pixel));
    let hair = (1.0 - smoothstep(0.0, 0.035, abs(value_noise(p * 0.12 + vec3(seed + 5.0)) - 0.5) * 2.0)) * (1.0 - smoothstep(0.1, 0.4, pixel));
    let mottle = 0.94 + 0.1 * value_noise(position * 0.08 + vec3(seed + 2.0));
    let tone = mix(vec3(1.0), vec3(0.66, 0.69, 0.76), vein * 0.8 + hair * 0.35) * mottle;
    return RockShade(normal, tone);
}

// Limestone (ours): the layered cave rock's beds and ledges, but toned in creams, honey and soft
// greys only, gently banded, with no warm and cool stripes.
fn limestone_shade(position: vec3<f32>, normal: vec3<f32>, pixel: f32, seed: f32) -> RockShade {
    let layered = strata_shade(position, normal, pixel, seed);
    let wobble = value_noise(vec3(position.x * 0.05, position.y * 0.09, position.z * 0.05) + vec3(seed)) - 0.5;
    let band = 0.5 + 0.5 * sin(position.y * 0.55 + wobble * 5.0);
    let tone = mix(vec3(0.97, 0.95, 0.9), vec3(1.0, 0.96, 0.86), band) * (0.93 + 0.1 * value_noise(position * 0.05 + vec3(seed + 4.0)));
    return RockShade(layered.normal, tone * mix(1.0, 0.9, 1.0 - smoothstep(0.0, 1.0, layered.tone.x)));
}

// Quartz rock (ours): pale, faintly glassy stone whose surface is a mass of tiny facets, each
// catching the light its own way: the normal is jittered per fine cell, and a few cells glint.
// Fades to plain with distance so it never shimmers as noise.
fn quartz_shade(position: vec3<f32>, normal: vec3<f32>, pixel: f32, seed: f32) -> RockShade {
    let near = 1.0 - smoothstep(0.08, 0.4, pixel);
    // Fine and sparse: a faint grain in the shading, and now and then a pin-prick of light a
    // couple of centimetres across, rounded and never square, gone from a distance.
    let q = position * 7.0;
    let cell = floor(q);
    let j = vec3(hash3(cell + vec3(seed)), hash3(cell + vec3(seed + 3.7)), hash3(cell + vec3(seed + 8.1))) - vec3(0.5);
    let n = normalize(normal + j * 0.05 * near);
    let spot = length(fract(q) - vec3(0.5 + 0.3 * (hash3(cell + vec3(seed + 6.0)) - 0.5)));
    let round = 1.0 - smoothstep(0.04, 0.16, spot);
    let glint = step(0.992, hash3(cell + vec3(seed + 1.3))) * round * near;
    let cloud = 0.95 + 0.08 * value_noise(position * 0.06 + vec3(seed));
    return RockShade(n, vec3(cloud + 0.45 * glint));
}

// Lichen on stone (ours) where `damp` (0..1, carried per vertex) is above a few percent: small,
// sparse crustose crusts in tones close to the stone's own, only a little greener and greyer, so
// they read as faint growth you find by looking and never as a pattern across a room. They
// gather in a few clusters and favour where stone is shut in or cut: block edges and joints and
// corners (`crease`, 0..1, from the baked corner shading and the change in the surface normal),
// the foot of a wall (to about 1.2 m) and up-facing ledges, and are far sparser on open faces
// and overhead. Each crust has a slightly paler rim and a slightly darker heart. Far off the
// crusts fade out, and most surfaces (damp near zero) cost nothing. `position` is in world
// units of 10 cm and sampled in the world, so every column and door of a shape grows its own;
// `height` is the surface's height above the floor.
fn apply_lichen(color: vec3<f32>, position: vec3<f32>, height: f32, normal: vec3<f32>, pixel: f32, damp: f32, seed: f32, crease: f32) -> vec3<f32> {
    // Faint damp and far-off stone grow nothing worth drawing.
    if damp < 0.04 || pixel > 0.3 {
        return color;
    }
    let wet = clamp(damp, 0.0, 1.0);
    let up = smoothstep(0.3, 0.85, normal.y);
    let down = smoothstep(0.3, 0.85, -normal.y);
    let foot = 1.0 - smoothstep(4.0, 12.0, height);
    let favour = clamp(0.2 + 0.7 * foot + 0.5 * up + 1.1 * crease, 0.0, 1.4) * (1.0 - 0.8 * down);

    // Clusters: where lichen grows at all, two octaves.
    let k = 0.08;
    let q = position * k + vec3(seed * 0.7);
    let cell = pixel * k;
    let field = 0.65 * filtered_noise(q, cell) + 0.35 * filtered_noise(q * 2.3 + vec3(8.1), cell * 2.3);
    let cluster = smoothstep(0.58 - 0.25 * wet, 0.72 - 0.25 * wet, field);
    let cover = clamp((0.02 + 0.14 * wet) * favour, 0.0, 0.4) * cluster;
    if cover <= 0.002 {
        return color;
    }

    // Crusts: small and merged a little, two octaves, the threshold set by the share to cover.
    let kk = 0.5;
    let p = position * kk + vec3(seed * 0.3 + 13.0);
    let ce = pixel * kk;
    let n = 0.65 * filtered_noise(p, ce) + 0.35 * filtered_noise(p * 2.7 + vec3(4.1), ce * 2.7);
    let threshold = mix(0.74, 0.54, clamp(cover * 2.5, 0.0, 1.0));
    let edge = clamp(pixel * 1.2, 0.03, 0.12);
    let crust = smoothstep(threshold, threshold + edge, n) * (1.0 - smoothstep(0.12, 0.3, pixel));
    if crust <= 0.0 {
        return color;
    }
    // Close to the stone: a little greener, a little greyer, a hair paler at the rim and darker
    // in the heart.
    let heart = smoothstep(threshold + edge, threshold + edge + 0.1, n);
    let tone = color * vec3(0.9, 0.98, 0.88) * (1.03 - 0.07 * heart);
    return mix(color, tone, crust * 0.65);
}
