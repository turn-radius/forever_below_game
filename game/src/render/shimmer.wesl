// The shimmer on a legendary item lying on the floor (`Shimmer` in `toon.rs`): a slow, soft
// glow pulse in the shimmer's colour over the whole item, and a few glints drifting over its
// surface, each flaring and fading in a moment. Added to the lit colour as self-light only,
// so it reads in an unlit corner but lights nothing round it, as the plate gave legendary
// metal a faint emissive (`metal.emissive = '#3a2c10'`, `reference/forever-below-plate.html`).
// Glints are hard-edged two-tone discs, a near-white core in a ring of the colour, to keep
// the house's crisp toon steps rather than a soft bloom. The rates and sizes are our own.

import super::decay::{detail, hash3};

const TAU: f32 = 6.28318530718;
// Seconds for one swell and fade of the glow.
const PULSE_PERIOD: f32 = 3.6;
// The glow's trough as a fraction of its peak: it never quite goes out.
const PULSE_FLOOR: f32 = 0.35;
// Glints live in a lattice of cells this many item units across (12 cm), drifting through the
// item this many cells a second, so where a cell meets the surface slides along it.
const GLINT_CELL: f32 = 1.2;
const GLINT_DRIFT: vec3<f32> = vec3(0.05, 0.11, 0.03);
// Share of cells holding a glint, seconds between one glint's flares, the share of that time
// it shows, and its largest radius in cells.
const GLINT_SHARE: f32 = 0.5;
const GLINT_PERIOD: f32 = 2.8;
const GLINT_SHOWN: f32 = 0.3;
const GLINT_RADIUS: f32 = 0.24;
// A glint's brightness against the glow's peak, and how far its core goes toward white.
const GLINT_GAIN: f32 = 4.0;
const GLINT_CORE_WHITE: f32 = 0.8;

// A hard-edged disc of `radius` at distance `d`, its edge softened over `aa` so it does not
// crawl.
fn disc(d: f32, radius: f32, aa: f32) -> f32 {
    return 1.0 - smoothstep(radius - aa, radius + aa, d);
}

// The shimmer's light, linear, to add to the fragment's lit colour. `color` is display
// (sRGB) values, `strength` the glow's peak as a fraction of it (0 for none), `item_position`
// the fragment in item space, `pixel` the item units one pixel spans (`fwidth`), `time` the
// view's time in seconds and `phase` a per-item offset so no two items pulse together.
fn shimmer_light(color_linear: vec3<f32>, strength: f32, item_position: vec3<f32>, pixel: f32, time: f32, phase: f32) -> vec3<f32> {
    if strength <= 0.0 {
        return vec3(0.0);
    }
    let t = time + phase;
    let pulse = PULSE_FLOOR + (1.0 - PULSE_FLOOR) * (0.5 - 0.5 * cos(TAU * t / PULSE_PERIOD));
    var light = color_linear * strength * pulse;

    let q = item_position / GLINT_CELL + GLINT_DRIFT * t;
    let cell = floor(q);
    let local = fract(q);
    let pick = hash3(cell + vec3(17.0, 3.0, 29.0));
    if pick < GLINT_SHARE {
        let centre = 0.3 + 0.4 * vec3(hash3(cell + vec3(5.0, 0.0, 0.0)), hash3(cell + vec3(0.0, 7.0, 0.0)), hash3(cell + vec3(0.0, 0.0, 11.0)));
        // Where this glint is in its own cycle, offset by its cell so glints flare in turn.
        let cycle = fract(t / GLINT_PERIOD + pick / GLINT_SHARE);
        if cycle < GLINT_SHOWN {
            let size = sin(cycle / GLINT_SHOWN * 3.14159265) * GLINT_RADIUS;
            let d = length(local - centre);
            let aa = pixel / GLINT_CELL;
            // Gone once a glint would span too few pixels to stay put.
            let seen = detail(1.0 / (GLINT_CELL * GLINT_RADIUS), pixel);
            let ring = disc(d, size, aa);
            let core = disc(d, size * 0.5, aa);
            let glint = mix(color_linear, mix(color_linear, vec3(1.0), GLINT_CORE_WHITE), core) * ring;
            light += glint * strength * GLINT_GAIN * seen;
        }
    }
    return light;
}
