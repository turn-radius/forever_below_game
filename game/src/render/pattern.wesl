// Surface patterns painted on a part's base colour before wear: woven and printed cloth,
// quilting, mail rings, scales and rivets. Colours in and out are display (sRGB) values.
//
// Port of `fab()` and the `uPat` lines of `DECAY_BODY` in `reference/spline_armory_engine.js`.
// Ids are `armory::fabric::PatternKind::shader_id()`. Wrapped patterns are laid round the part
// in item space (`armory::fabric::Wrap`), so they stay on the object as it turns. See
// `docs/rendering.md`.

const TAU: f32 = 6.28318530718;

const PATTERN_NONE: u32 = 0u;
const PATTERN_STRIPES: u32 = 1u;
const PATTERN_HOOPS: u32 = 2u;
const PATTERN_CHECKS: u32 = 3u;
const PATTERN_TARTAN: u32 = 4u;
const PATTERN_LOZENGES: u32 = 5u;
const PATTERN_POWDERED: u32 = 6u;
const PATTERN_CHEVRONS: u32 = 7u;
const PATTERN_QUILTED: u32 = 8u;
const PATTERN_MAIL: u32 = 9u;
const PATTERN_SCALES: u32 = 10u;
const PATTERN_RIVETS: u32 = 11u;
// `PatternKind::FIRST_FIELD_ID`: from here on, patterns lie over the part's uv.
const FIRST_FIELD_PATTERN: u32 = 12u;
const PATTERN_PER_PALE: u32 = 12u;
const PATTERN_PER_FESS: u32 = 13u;
const PATTERN_QUARTERLY: u32 = 14u;
const PATTERN_PER_BEND: u32 = 15u;
const PATTERN_PER_CHEVRON: u32 = 16u;
const PATTERN_PALY: u32 = 17u;
const PATTERN_BENDY: u32 = 18u;
const PATTERN_CHECKY: u32 = 19u;
const PATTERN_LOZENGY: u32 = 20u;
const PATTERN_PLANKS: u32 = 21u;
const PATTERN_PER_SALTIRE: u32 = 22u;
const PATTERN_BARRY: u32 = 23u;

// GLSL's `mod`: always between 0 and `y`. WGSL's `%` keeps the sign of `x`.
fn modulo(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

// Where a wrapped pattern sits on a part: `armory::fabric::Wrap`, with its scale and the
// whole number of cells round.
struct Wrap {
    origin: vec3<f32>,
    axis: vec3<f32>,
    zero: vec3<f32>,
    base: f32,
    scale: f32,
    around: f32,
    mirrored: u32,
    length: f32,
    // `Wrap::profile`: girth in `PROFILE` bands from the base up, four to a vec4.
    profile: array<vec4<f32>, 4>,
}

const PROFILE: f32 = 16.0;

// `Wrap::coords` before the count round is applied: turns round the axis (-0.5..0.5) and
// cells along it from the part's base.
fn wrap_turns(p_in: vec3<f32>, wrap: Wrap) -> vec2<f32> {
    var p = p_in;
    if wrap.mirrored != 0u {
        p.x = abs(p.x);
    }
    let q = p - wrap.origin;
    let along = dot(q, wrap.axis);
    let r = q - wrap.axis * along;
    let angle = atan2(dot(r, cross(wrap.axis, wrap.zero)), dot(r, wrap.zero));
    return vec2(angle / TAU, (along - wrap.base) * wrap.scale);
}

// `PatternKind::rows_follow_girth`: mail and scales, whose rows are separate.
fn rows_follow_girth(id: u32) -> bool {
    return id == PATTERN_MAIL || id == PATTERN_SCALES;
}

// `Wrap::repeats_around_at` for the row of cells `row` (rows are `rows_per_cell` to a cell).
fn repeats_round_row(wrap: Wrap, row: f32, rows_per_cell: f32) -> f32 {
    let along = (row + 0.5) / (rows_per_cell * wrap.scale);
    let t = select(0.0, along / wrap.length, wrap.length > 0.0);
    let band = u32(clamp(floor(t * PROFILE), 0.0, PROFILE - 1.0));
    let girth = wrap.profile[band / 4u][band % 4u];
    return max(round(girth * wrap.scale * TAU), 1.0);
}

// Cell coordinates on the part: `Wrap::coords`, with mail and scales counting cells round
// each row from the girth there.
fn wrap_coords(p: vec3<f32>, wrap: Wrap, id: u32) -> vec2<f32> {
    let turns = wrap_turns(p, wrap);
    var around = wrap.around;
    if rows_follow_girth(id) {
        // Mail packs 1.4 rows to a cell (`wrapped_pattern`).
        let rows_per_cell = select(1.0, 1.4, id == PATTERN_MAIL);
        around = repeats_round_row(wrap, floor(turns.y * rows_per_cell), rows_per_cell);
    }
    return vec2(turns.x * around, turns.y);
}

// Distance to the nearest whole number.
fn seam_distance(x: f32) -> f32 {
    let f = fract(x);
    return min(f, 1.0 - f);
}

// A wrapped pattern at cell coordinates `p`: x is the share of the second colour, y how much
// the surface darkens (a ring's shadow, a stitched seam).
fn wrapped_pattern(p: vec2<f32>, id: u32) -> vec2<f32> {
    if id == PATTERN_STRIPES {
        return vec2(step(0.5, fract(p.x * 0.5)), 0.0);
    }
    if id == PATTERN_HOOPS {
        return vec2(step(0.5, fract(p.y * 0.5)), 0.0);
    }
    if id == PATTERN_CHECKS {
        return vec2(modulo(floor(p.x) + floor(p.y), 2.0), 0.0);
    }
    if id == PATTERN_TARTAN {
        let a = step(0.62, fract(p.x * 0.5));
        let b = step(0.62, fract(p.y * 0.5));
        let t = max(step(0.93, fract(p.x * 1.5 + 0.2)), step(0.93, fract(p.y * 1.5 + 0.2)));
        return vec2(clamp((a + b) * 0.5 + t * 0.5, 0.0, 1.0), 0.0);
    }
    if id == PATTERN_LOZENGES {
        return vec2(modulo(floor((p.x + p.y) * 0.5) + floor((p.x - p.y) * 0.5), 2.0), 0.0);
    }
    if id == PATTERN_POWDERED {
        let c = fract(p + vec2(0.5 * modulo(floor(p.y), 2.0), 0.0)) - 0.5;
        return vec2(1.0 - step(0.17, length(c)), 0.0);
    }
    if id == PATTERN_CHEVRONS {
        return vec2(step(0.5, fract(p.y * 0.5 + abs(fract(p.x * 0.5) - 0.5))), 0.0);
    }
    if id == PATTERN_QUILTED {
        // Stitched seams on whole cells both ways, each puff shading down into them. The
        // prototype stitched channels one way only (`fract(p.x * 0.7)`); a grid matches
        // the gambeson's puffs (`build/garment.rs`).
        let d = min(seam_distance(p.x), seam_distance(p.y));
        let stitch = 1.0 - smoothstep(0.02, 0.05, d);
        let hollow = 1.0 - smoothstep(0.0, 0.5, d);
        return vec2(0.0, stitch * 0.5 + hollow * 0.22);
    }
    if id == PATTERN_MAIL {
        // Rings in rows offset by half a ring: a dark ring line, and the hole showing dark
        // through the middle.
        let q = vec2(p.x + 0.5 * modulo(floor(p.y * 1.4), 2.0), p.y * 1.4);
        let c = fract(q) - 0.5;
        let r = length(c * vec2(1.0, 0.75));
        let ring = (1.0 - smoothstep(0.05, 0.1, abs(r - 0.36))) * 0.55;
        let hole = (1.0 - smoothstep(0.2, 0.27, r)) * 0.7;
        return vec2(0.0, max(ring, hole));
    }
    if id == PATTERN_SCALES {
        // Rows of rounded scales hanging over the row below, each darker towards its root.
        let row = floor(p.y);
        let c = vec2(fract(p.x + 0.5 * modulo(row, 2.0)) - 0.5, fract(p.y));
        let d = length(vec2(c.x, (c.y - 0.9) * 0.9));
        let edge = (1.0 - smoothstep(0.03, 0.08, abs(d - 0.58))) * 0.6 * step(c.y, 0.9);
        return vec2(0.0, max(edge, c.y * 0.18));
    }
    if id == PATTERN_RIVETS {
        let c = fract(p * vec2(1.0, 0.8)) - 0.5;
        return vec2(1.0 - smoothstep(0.1, 0.13, length(c)), 0.0);
    }
    return vec2(0.0);
}

// What a wrapped pattern averages to (estimated from the shapes above), which it fades to
// once its cells are too small to draw cleanly on screen.
fn wrapped_pattern_mean(id: u32) -> vec2<f32> {
    if id == PATTERN_TARTAN {
        return vec2(0.45, 0.0);
    }
    if id == PATTERN_POWDERED {
        return vec2(0.09, 0.0);
    }
    if id == PATTERN_QUILTED {
        return vec2(0.0, 0.1);
    }
    if id == PATTERN_MAIL {
        return vec2(0.0, 0.32);
    }
    if id == PATTERN_SCALES {
        return vec2(0.0, 0.18);
    }
    if id == PATTERN_RIVETS {
        return vec2(0.05, 0.0);
    }
    return vec2(0.5, 0.0);
}

// Odd cells of a run: 1 for odd `floor(x)`, 0 for even.
fn odd(x: f32) -> f32 {
    return modulo(floor(x), 2.0);
}

// A cheap hash of one number to 0..1 (`h2` in the prototype's shader, in one dimension).
fn hash1(x: f32) -> f32 {
    return fract(sin(x * 127.1 + 311.7) * 43758.5453);
}

// A field pattern over the part's uv, which runs 0..=1 across the field with the fess point
// (where the field's lines cross) at 0.5, 0.5, u towards sinister (the viewer's right) and v
// up. `n` is the pattern's scale: repeats for stripes and chequers, boards for planks. x is
// the share of the second tincture, y how much the surface darkens.
//
// From the heraldic branch of `fab()` (`reference/spline_armory_engine.js`), with the first
// tincture in dexter and in chief as blazon reads it (the prototype put the second tincture
// in chief for per fess and per bend), and per saltire, barry and planks' board tones our own.
fn field_pattern(uv: vec2<f32>, id: u32, n: f32) -> vec2<f32> {
    let p = uv - 0.5;
    if id == PATTERN_PER_PALE {
        return vec2(step(0.0, p.x), 0.0);
    }
    if id == PATTERN_PER_FESS {
        return vec2(step(p.y, 0.0), 0.0);
    }
    if id == PATTERN_QUARTERLY {
        // First and fourth quarters (dexter chief, sinister base) in the first tincture.
        return vec2(1.0 - abs(step(0.0, p.x) - step(0.0, p.y)), 0.0);
    }
    if id == PATTERN_PER_BEND {
        // Dexter chief to sinister base; the second tincture below the line.
        return vec2(step(p.x + p.y, 0.0), 0.0);
    }
    if id == PATTERN_PER_CHEVRON {
        return vec2(step(p.y, 0.08 - abs(p.x) * 1.1), 0.0);
    }
    if id == PATTERN_PALY {
        return vec2(odd(uv.x * n), 0.0);
    }
    if id == PATTERN_BENDY {
        return vec2(odd((p.x + p.y + 1.0) * n * 0.5), 0.0);
    }
    if id == PATTERN_CHECKY {
        return vec2(modulo(floor(uv.x * n) + floor(uv.y * n), 2.0), 0.0);
    }
    if id == PATTERN_LOZENGY {
        let q = p * n * 2.0;
        return vec2(modulo(floor((q.x + q.y) * 0.5) + floor((q.x - q.y) * 0.5), 2.0), 0.0);
    }
    if id == PATTERN_PLANKS {
        // A dark seam between boards, and each board a little lighter or darker than the next.
        let seam = 1.0 - smoothstep(0.003, 0.006, seam_distance(uv.x * n) / max(n, 1.0));
        let tone = (hash1(floor(uv.x * n)) - 0.5) * 0.12;
        return vec2(0.0, seam * 0.55 + tone);
    }
    if id == PATTERN_PER_SALTIRE {
        // Chief and base in the first tincture, the flanks in the second.
        return vec2(step(abs(p.y), abs(p.x)), 0.0);
    }
    if id == PATTERN_BARRY {
        // Stripes from the chief down, the first in the first tincture.
        return vec2(odd((1.0 - uv.y) * n), 0.0);
    }
    return vec2(0.0);
}

// Paints pattern `id` over `color`. `item_position` is in item space and `uv` the part's own
// (for field patterns); `cell_size` is how many cells one pixel spans (`fwidth` of the
// pattern coordinates), so fine wrapped patterns fade to their mean instead of shimmering.
// The fade is our own, not the prototype's.
fn apply_pattern(
    color: vec3<f32>,
    item_position: vec3<f32>,
    uv: vec2<f32>,
    cell_size: f32,
    id: u32,
    second: vec3<f32>,
    wrap: Wrap,
) -> vec3<f32> {
    if id == PATTERN_NONE {
        return color;
    }
    if id >= FIRST_FIELD_PATTERN {
        let f = field_pattern(uv, id, wrap.scale);
        return mix(color, second, f.x) * (1.0 - f.y);
    }
    let p = wrap_coords(item_position, wrap, id);
    let sharp = 1.0 - smoothstep(0.25, 0.6, cell_size);
    let f = mix(wrapped_pattern_mean(id), wrapped_pattern(p, id), sharp);
    return mix(color, second, f.x) * (1.0 - f.y);
}
