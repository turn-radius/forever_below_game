//! Soft goods and the surface patterns the shader draws over a part: the textiles bags and
//! sacks are cut from, dyed cloth, woven and printed fabric patterns, quilting, rivets, mail
//! rings, scales and heraldic divisions. Shared by garments, hems and trims, leg cloths,
//! hoods, bags, a chest's glaze, shields and anything else that carries a pattern.
//!
//! From the prototype's `FABRIC` list, the bag's `fabric` select, `PAT_ID` and `clothMat` in
//! `reference/spline_armory_engine.js`. The pattern is state, not geometry: a part carries a
//! [`SurfacePattern`] and the toon shader draws it (`docs/rendering.md`).

use crate::material::{MaterialKind, Rgb};
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

/// The stuff a bag, sack or pouch is cut from (the prototype's bag `fabric` select).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Textile {
    Leather,
    /// Woven wool or linen.
    Cloth,
    /// Coarse jute or hemp sacking.
    Burlap,
}

impl Textile {
    pub const ALL: [Textile; 3] = [Textile::Leather, Textile::Cloth, Textile::Burlap];

    /// The material a part cut from it renders and decays as.
    pub fn material(self) -> MaterialKind {
        match self {
            Textile::Leather => MaterialKind::Leather,
            Textile::Cloth | Textile::Burlap => MaterialKind::Cloth,
        }
    }

    /// How thick a cut panel is, in units (1 unit = 10 cm).
    pub fn thickness(self) -> f32 {
        match self {
            Textile::Leather => 0.035,
            Textile::Cloth => 0.02,
            Textile::Burlap => 0.03,
        }
    }
}

/// A woven, printed, painted or glazed pattern (the prototype's `FABRIC` select).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FabricPattern {
    Plain,
    Stripes,
    Hoops,
    Checks,
    Tartan,
    Lozenges,
    /// Small dots scattered over the cloth ("powdered").
    Powdered,
    Chevrons,
    Quilted,
    Rivets,
}

impl FabricPattern {
    pub const ALL: [FabricPattern; 10] = [
        FabricPattern::Plain, FabricPattern::Stripes, FabricPattern::Hoops, FabricPattern::Checks, FabricPattern::Tartan,
        FabricPattern::Lozenges, FabricPattern::Powdered, FabricPattern::Chevrons, FabricPattern::Quilted, FabricPattern::Rivets,
    ];

    /// What the shader draws for this pattern, or `None` for plain cloth.
    pub fn kind(self) -> Option<PatternKind> {
        Some(match self {
            FabricPattern::Plain => return None,
            FabricPattern::Stripes => PatternKind::Stripes,
            FabricPattern::Hoops => PatternKind::Hoops,
            FabricPattern::Checks => PatternKind::Checks,
            FabricPattern::Tartan => PatternKind::Tartan,
            FabricPattern::Lozenges => PatternKind::Lozenges,
            FabricPattern::Powdered => PatternKind::Powdered,
            FabricPattern::Chevrons => PatternKind::Chevrons,
            FabricPattern::Quilted => PatternKind::Quilted,
            FabricPattern::Rivets => PatternKind::Rivets,
        })
    }
}

/// Every pattern the surface shader draws. Ids follow the prototype's `PAT_ID`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternKind {
    Stripes,
    Hoops,
    Checks,
    Tartan,
    Lozenges,
    Powdered,
    Chevrons,
    Quilted,
    /// Butted or riveted mail rings.
    Mail,
    /// Overlapping scales.
    Scales,
    Rivets,
    /// Heraldic divisions of a painted field, between the part's tint and the pattern colour,
    /// over uv 0..=1 across the field, centred on the fess point ([`crate::heraldry::Division`]).
    PerPale,
    PerFess,
    Quarterly,
    PerBend,
    PerChevron,
    Paly,
    Bendy,
    Checky,
    PerSaltire,
    Barry,
    /// Diamonds of the field's two tinctures ([`crate::heraldry::Division::Lozengy`]); the
    /// cloth pattern is [`PatternKind::Lozenges`].
    Lozengy,
    /// Boards running up the part, `scale` of them across uv 0..=1 (bare shield faces).
    Planks,
}

impl PatternKind {
    /// The shader's pattern id (`PAT_ID` in the prototype).
    pub fn shader_id(self) -> u32 {
        match self {
            PatternKind::Stripes => 1,
            PatternKind::Hoops => 2,
            PatternKind::Checks => 3,
            PatternKind::Tartan => 4,
            PatternKind::Lozenges => 5,
            PatternKind::Powdered => 6,
            PatternKind::Chevrons => 7,
            PatternKind::Quilted => 8,
            PatternKind::Mail => 9,
            PatternKind::Scales => 10,
            PatternKind::Rivets => 11,
            PatternKind::PerPale => 12,
            PatternKind::PerFess => 13,
            PatternKind::Quarterly => 14,
            PatternKind::PerBend => 15,
            PatternKind::PerChevron => 16,
            PatternKind::Paly => 17,
            PatternKind::Bendy => 18,
            PatternKind::Checky => 19,
            // Free in the prototype's table, whose lozengy fields reused the cloth lozenge.
            PatternKind::Lozengy => 20,
            PatternKind::Planks => 21,
            // Not in the prototype's table.
            PatternKind::PerSaltire => 22,
            PatternKind::Barry => 23,
        }
    }

    /// Whether the pattern is laid over the part's uv 0..=1 (a field's divisions, a board's
    /// planks) rather than wrapped round it in item space ([`Wrap`]). The shader tells the two
    /// apart by id: every field pattern's id is [`PatternKind::FIRST_FIELD_ID`] or more.
    pub fn on_field(self) -> bool {
        self.shader_id() >= Self::FIRST_FIELD_ID
    }

    /// Whether each row of the pattern sets its own count of cells round the part from the
    /// girth there ([`Wrap::repeats_around_at`]): mail and scales, whose rows are separate.
    /// Matches `rows_follow_girth` in the game's `pattern.wesl`.
    pub fn rows_follow_girth(self) -> bool {
        matches!(self, PatternKind::Mail | PatternKind::Scales)
    }

    /// The lowest shader id of a field pattern (see [`PatternKind::on_field`]).
    pub const FIRST_FIELD_ID: u32 = 12;

    pub const ALL: [PatternKind; 23] = [
        PatternKind::Stripes, PatternKind::Hoops, PatternKind::Checks, PatternKind::Tartan, PatternKind::Lozenges,
        PatternKind::Powdered, PatternKind::Chevrons, PatternKind::Quilted, PatternKind::Mail, PatternKind::Scales,
        PatternKind::Rivets, PatternKind::PerPale, PatternKind::PerFess, PatternKind::Quarterly, PatternKind::PerBend,
        PatternKind::PerChevron, PatternKind::Paly, PatternKind::Bendy, PatternKind::Checky, PatternKind::PerSaltire,
        PatternKind::Barry, PatternKind::Lozengy, PatternKind::Planks,
    ];
}

/// How a wrapped pattern ([`PatternKind::on_field`] false) is laid on a part, in item space so
/// it stays on the part as it turns or moves: round an axis through the part and along it
/// from the part's lowest point. The renderer and any geometry that must line up with the
/// pattern (a gambeson's quilting puffs) both lay it out with this.
///
/// A single part wraps round a vertical axis through the middle of its bounds: the
/// prototype's cylindrical projection (`uPatProj`, `atan(x, z) * R` in
/// `reference/spline_armory_engine.js`). Our departures:
/// - The number of cells round is rounded to a whole even number, so the pattern closes
///   without a seam at the back (every wrapped pattern repeats within two cells), and the
///   radius is measured from the part rather than passed in.
/// - A part wholly to one side of x = 0 (a sleeve) wraps round its own long axis, so it
///   wears the pattern round itself rather than round the wearer's body. A mirrored pair
///   split down x = 0 (a pair of sleeves, pauldrons, leg cloths) folds its left half onto its
///   right and wraps that the same way.
/// - Mail and scales ([`PatternKind::rows_follow_girth`]) set the count round each row from
///   the part's girth at that row ([`Wrap::profile`]), as real mail is narrowed with
///   decreases, so rings keep their size from a coif's neck to its shoulders.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wrap {
    /// A point on the axis.
    pub origin: [f32; 3],
    /// The axis, a unit vector: rows run along it.
    pub axis: [f32; 3],
    /// A unit vector square to the axis, where the angle round it is zero.
    pub zero: [f32; 3],
    /// The part's lowest point along the axis, where rows start.
    pub base: f32,
    /// Mean distance of the part's vertices from the axis.
    pub radius: f32,
    /// A mirrored pair: points left of x = 0 are folded onto the right before wrapping.
    pub mirrored: bool,
    /// The part's length along the axis.
    pub length: f32,
    /// Mean distance from the axis in [`Wrap::PROFILE`] equal bands from the base up.
    pub profile: [f32; Wrap::PROFILE],
}

impl Wrap {
    /// The wrap for a part with these vertices.
    pub fn of(positions: &[[f32; 3]]) -> Self {
        use crate::geom::vec3;
        let vertical = Wrap {
            origin: [0.0; 3],
            axis: vec3::Y,
            zero: vec3::Z,
            base: 0.0,
            radius: 0.0,
            mirrored: false,
            length: 0.0,
            profile: [0.0; Wrap::PROFILE],
        };
        if positions.is_empty() {
            return vertical;
        }
        let reach = positions.iter().fold(0.0f32, |m, p| m.max(p[0].abs()));
        let gap = positions.iter().fold(f32::MAX, |m, p| m.min(p[0].abs()));
        let both_sides = positions.iter().any(|p| p[0] > 0.0) && positions.iter().any(|p| p[0] < 0.0);
        let mirrored = both_sides && gap > 0.15 * reach;
        let points: Vec<[f32; 3]> = positions.iter().map(|&p| if mirrored { [p[0].abs(), p[1], p[2]] } else { p }).collect();

        // A part off to one side (one sleeve, one half of a pair) wraps round its own length.
        let (origin, axis) = if mirrored || !both_sides {
            let centre = vec3::centroid(&points);
            (centre, long_axis(&points, centre))
        } else {
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            for p in &points {
                lo = [lo[0].min(p[0]), lo[1].min(p[2])];
                hi = [hi[0].max(p[0]), hi[1].max(p[2])];
            }
            ([(lo[0] + hi[0]) * 0.5, 0.0, (lo[1] + hi[1]) * 0.5], vec3::Y)
        };
        // Zero angle faces +z where it can (so a vertical wrap reads as `atan(x, z)`).
        let toward_z = vec3::sub(vec3::Z, vec3::scale(axis, axis[2]));
        let zero = vec3::try_normalize(toward_z).unwrap_or_else(|| vec3::any_perpendicular(axis));
        let mut wrap = Wrap { origin, axis, zero, mirrored, ..vertical };
        let split: Vec<(f32, f32)> = points.iter().map(|&p| wrap.split(p)).map(|(along, r)| (along, vec3::length(r))).collect();
        let top = split.iter().map(|s| s.0).fold(f32::MIN, f32::max);
        wrap.base = split.iter().map(|s| s.0).fold(f32::MAX, f32::min);
        wrap.length = top - wrap.base;
        wrap.radius = split.iter().map(|s| s.1).sum::<f32>() / split.len() as f32;
        let (mut sums, mut counts) = ([0.0f32; Wrap::PROFILE], [0u32; Wrap::PROFILE]);
        for &(along, r) in &split {
            let band = wrap.band(along);
            sums[band] += r;
            counts[band] += 1;
        }
        // A band no vertex falls in takes the nearest band below that has one (or the mean).
        let mut last = wrap.radius;
        for band in 0..Wrap::PROFILE {
            if counts[band] > 0 {
                last = sums[band] / counts[band] as f32;
            }
            wrap.profile[band] = last;
        }
        wrap
    }

    /// Bands in [`Wrap::profile`].
    pub const PROFILE: usize = 16;

    /// The profile band a point `along` the axis falls in.
    fn band(&self, along: f32) -> usize {
        let t = if self.length > 0.0 { (along - self.base) / self.length } else { 0.0 };
        ((t * Self::PROFILE as f32).floor().max(0.0) as usize).min(Self::PROFILE - 1)
    }

    /// Cells round the part `along` the axis for a pattern whose rows each take their own
    /// count ([`PatternKind::rows_follow_girth`]): the girth there times the scale, rounded to
    /// a whole number, at least one.
    pub fn repeats_around_at(&self, scale: f32, along: f32) -> f32 {
        (self.profile[self.band(along)] * scale * std::f32::consts::TAU).round().max(1.0)
    }

    /// `p` folded if mirrored, relative to the axis: its distance along it and the rest.
    fn split(&self, p: [f32; 3]) -> (f32, [f32; 3]) {
        use crate::geom::vec3;
        let p = if self.mirrored { [p[0].abs(), p[1], p[2]] } else { p };
        let q = vec3::sub(p, self.origin);
        let along = vec3::dot(q, self.axis);
        (along, vec3::sub(q, vec3::scale(self.axis, along)))
    }

    /// Pattern cells round the part at `scale` cells per unit: the circumference times the
    /// scale, rounded to a whole even number, at least two.
    pub fn repeats_around(&self, scale: f32) -> f32 {
        2.0 * (self.radius * scale * std::f32::consts::PI).round().max(1.0)
    }

    /// The scale nearest `scale` at which the circumference holds a whole even number of
    /// cells exactly, so a part a little fatter or thinner still rounds to the same count.
    pub fn fitted_scale(&self, scale: f32) -> f32 {
        if self.radius <= 0.0 { scale } else { self.repeats_around(scale) / (std::f32::consts::TAU * self.radius) }
    }

    /// Pattern coordinates of an item-space point at `scale`: cells round the axis (a full
    /// turn is [`Wrap::repeats_around`]) and cells up from the base. Matches `wrap_coords` in
    /// the game's `pattern.wesl`.
    pub fn coords(&self, p: [f32; 3], scale: f32) -> [f32; 2] {
        use crate::geom::vec3;
        let (along, r) = self.split(p);
        let angle = vec3::dot(r, vec3::cross(self.axis, self.zero)).atan2(vec3::dot(r, self.zero));
        [angle / std::f32::consts::TAU * self.repeats_around(scale), (along - self.base) * scale]
    }
}

/// The direction `points` spread furthest along from `centre` (the principal axis of their
/// covariance, by power iteration), turned to point up, or along +x if level.
fn long_axis(points: &[[f32; 3]], centre: [f32; 3]) -> [f32; 3] {
    use crate::geom::vec3;
    let mut c = [[0.0f32; 3]; 3];
    for &p in points {
        let d = vec3::sub(p, centre);
        for i in 0..3 {
            for j in 0..3 {
                c[i][j] += d[i] * d[j];
            }
        }
    }
    let mut v = vec3::normalize_or([0.3, 1.0, 0.2], vec3::Y);
    for _ in 0..48 {
        let w = [vec3::dot(c[0], v), vec3::dot(c[1], v), vec3::dot(c[2], v)];
        match vec3::try_normalize(w) {
            Some(w) => v = w,
            None => return vec3::Y,
        }
    }
    let up = if v[1].abs() > 1e-3 { v[1] } else { v[0] };
    if up < 0.0 { vec3::neg(v) } else { v }
}

/// A pattern as a part carries it: what to draw, in which second colour, at what scale.
/// `color` is ignored by metal patterns (mail, scales), which shade the part's own colour.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfacePattern {
    pub kind: PatternKind,
    pub color: Rgb,
    /// Repeats per unit of surface (the prototype's `patScale` or `ringSize`).
    pub scale: f32,
}

/// One dyed cloth: the prototype's colour, pattern, pattern colour and pattern scale.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fabric {
    pub color: Rgb,
    pub pattern: FabricPattern,
    pub color2: Rgb,
    /// Pattern repeats per unit.
    pub scale: f32,
}

sliders! {
    FabricLimits for Fabric {
        scale: 0.5, 8.0, 0.05;
    }
}

impl Fabric {
    /// Plain cloth of one colour.
    pub const fn plain(color: Rgb) -> Self {
        Fabric { color, pattern: FabricPattern::Plain, color2: color, scale: 2.0 }
    }

    /// The shader pattern for this cloth, if it has one.
    pub fn surface(&self) -> Option<SurfacePattern> {
        self.pattern.kind().map(|kind| SurfacePattern { kind, color: self.color2, scale: self.scale })
    }

    /// The prototype's garment colours (madder, woad, weld and walnut dyes, undyed wool).
    pub const GARMENT_DYES: [Rgb; 8] = [
        Rgb::hex(0x6a7f3a), Rgb::hex(0x2c3e6b), Rgb::hex(0x7a1f24), Rgb::hex(0x4a3a2e),
        Rgb::hex(0xcbbf9f), Rgb::hex(0x5a3d6b), Rgb::hex(0x3f5a5a), Rgb::hex(0x8a5a2b),
    ];
    /// The prototype's pattern colours.
    pub const PATTERN_DYES: [Rgb; 5] = [Rgb::hex(0xe0d4b0), Rgb::hex(0xd4b850), Rgb::hex(0x232323), Rgb::hex(0xa3262a), Rgb::hex(0x2c4e8c)];
    /// The prototype's trim colours.
    pub const TRIM_DYES: [Rgb; 5] = [Rgb::hex(0xc09a3a), Rgb::hex(0xd4b850), Rgb::hex(0x7a2f22), Rgb::hex(0x232323), Rgb::hex(0xe0d4b0)];
}

impl Default for Fabric {
    /// The prototype's `BASE.garment` cloth.
    fn default() -> Self {
        Fabric { color: Rgb::hex(0x6a7f3a), pattern: FabricPattern::Plain, color2: Rgb::hex(0xe0d4b0), scale: 2.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_cloth_has_no_surface_pattern() {
        assert_eq!(Fabric::plain(Rgb::hex(0x123456)).surface(), None);
        let striped = Fabric { pattern: FabricPattern::Stripes, ..Fabric::default() };
        assert_eq!(striped.surface().unwrap().kind.shader_id(), 1);
    }

    #[test]
    fn every_pattern_has_its_own_shader_id() {
        let mut ids: Vec<u32> = PatternKind::ALL.iter().map(|k| k.shader_id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PatternKind::ALL.len());
        assert!(!ids.contains(&0), "0 is no pattern");
    }

    #[test]
    fn field_patterns_are_divisions_and_planks() {
        for kind in PatternKind::ALL {
            let fabric = FabricPattern::ALL.iter().any(|f| f.kind() == Some(kind));
            let metal = matches!(kind, PatternKind::Mail | PatternKind::Scales);
            assert_eq!(kind.on_field(), !fabric && !metal, "{kind:?}");
        }
    }

    /// `n` points round (cx, cz) at radius `r`, at y0 and y1.
    fn tube(cx: f32, cz: f32, r: f32, y0: f32, y1: f32, n: usize) -> Vec<[f32; 3]> {
        (0..n)
            .flat_map(|i| {
                let a = std::f32::consts::TAU * i as f32 / n as f32;
                [[cx + r * a.sin(), y0, cz + r * a.cos()], [cx + r * a.sin(), y1, cz + r * a.cos()]]
            })
            .collect()
    }

    #[test]
    fn wrap_follows_the_part() {
        let wrap = Wrap::of(&tube(1.0, -2.0, 1.5, 3.0, 7.0, 64));
        assert!(!wrap.mirrored);
        assert!((wrap.origin[0] - 1.0).abs() < 1e-3 && (wrap.origin[2] + 2.0).abs() < 1e-3);
        assert_eq!(wrap.axis, [0.0, 1.0, 0.0]);
        assert!((wrap.radius - 1.5).abs() < 1e-3);
        assert_eq!(wrap.base, 3.0);
        // Moving the part moves its pattern with it.
        let moved = Wrap::of(&tube(4.0, 0.0, 1.5, -1.0, 3.0, 64));
        let (a, b) = (wrap.coords([2.5, 4.2, -2.0], 2.0), moved.coords([5.5, 0.2, 0.0], 2.0));
        assert!((a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4, "{a:?} {b:?}");
    }

    #[test]
    fn a_pair_of_sleeves_wraps_each_sleeve_round_itself() {
        // Two sleeves of radius 0.4 hanging down and out from the shoulders, mirrored in x.
        let (down, out) = (0.8f32, 0.6f32);
        let right: Vec<[f32; 3]> = (0..=10)
            .flat_map(|k| {
                let t = k as f32 * 0.3;
                let centre = [1.4 + out * t, 4.0 - down * t, 0.0];
                (0..24).map(move |i| {
                    let a = std::f32::consts::TAU * i as f32 / 24.0;
                    // Round the sleeve's own axis (out, -down, 0): across is (down, out, 0) and z.
                    let (s, c) = a.sin_cos();
                    [centre[0] + 0.4 * c * down, centre[1] + 0.4 * c * out, 0.4 * s]
                })
            })
            .collect();
        let pair: Vec<[f32; 3]> = right.iter().flat_map(|&p| [p, [-p[0], p[1], p[2]]]).collect();
        let wrap = Wrap::of(&pair);
        assert!(wrap.mirrored);
        assert!((wrap.radius - 0.4).abs() < 0.01, "{}", wrap.radius);
        assert!((wrap.axis[0] + out).abs() < 0.01 && (wrap.axis[1] - down).abs() < 0.01, "{:?}", wrap.axis);
        // Each sleeve wears the same pattern, mirrored.
        let p = right[37];
        assert_eq!(wrap.coords(p, 3.0), wrap.coords([-p[0], p[1], p[2]], 3.0));
    }

    #[test]
    fn rows_of_mail_narrow_with_the_part() {
        // A cone, 2 units round at the bottom tapering to 0.5 at the top.
        let cone: Vec<[f32; 3]> = (0..=20)
            .flat_map(|k| {
                let y = k as f32 * 0.2;
                let r = 2.0 - 1.5 * y / 4.0;
                (0..48).map(move |i| {
                    let a = std::f32::consts::TAU * i as f32 / 48.0;
                    [r * a.sin(), y, r * a.cos()]
                })
            })
            .collect();
        let wrap = Wrap::of(&cone);
        assert!((wrap.length - 4.0).abs() < 1e-4);
        let (low, high) = (wrap.repeats_around_at(3.0, 0.1), wrap.repeats_around_at(3.0, 3.9));
        assert_eq!(low, (std::f32::consts::TAU * 1.98 * 3.0).round(), "{:?}", wrap.profile);
        assert!(high < low * 0.35, "{high} {low}");
        assert!(PatternKind::Mail.rows_follow_girth() && !PatternKind::Quilted.rows_follow_girth());
    }

    #[test]
    fn wrapped_patterns_close_round_the_back() {
        let wrap = Wrap::of(&tube(0.0, 0.0, 1.7, 0.0, 1.0, 96));
        for scale in [0.5, 1.3, 2.0, 7.9] {
            let around = wrap.repeats_around(scale);
            assert_eq!(around % 2.0, 0.0);
            // Cells stay near the size asked for.
            assert!((around / (std::f32::consts::TAU * wrap.radius) - scale).abs() <= 1.0 / (std::f32::consts::PI * wrap.radius) + 1e-4);
            // Either side of the seam at the back is the same place in the pattern.
            let left = wrap.coords([-1e-4, 0.5, -1.7], scale)[0];
            let right = wrap.coords([1e-4, 0.5, -1.7], scale)[0];
            assert!(((left - right).abs() - around).abs() < 1e-2, "{left} {right} {around}");
        }
    }

    #[test]
    fn fitted_scale_survives_a_small_change_of_girth() {
        let wrap = Wrap::of(&tube(0.0, 0.0, 1.6, 0.0, 1.0, 96));
        let fitted = wrap.fitted_scale(1.8);
        let around = wrap.repeats_around(fitted);
        assert!((std::f32::consts::TAU * wrap.radius * fitted - around).abs() < 1e-3);
        let fatter = Wrap { radius: wrap.radius * 1.015, ..wrap };
        assert_eq!(fatter.repeats_around(fitted), around);
    }
}
