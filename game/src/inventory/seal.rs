//! The runic seal behind the doll, drawn as the plate's `renderRing` draws it
//! (`reference/forever-below-plate.html`): a seven-pointed star in a heptagon, medallions at
//! its points, rings of ticks and dots, and three bands of runes, the outer and inner bands
//! turning one way and the middle the other.
//!
//! Bevy's UI draws no vector paths, so the lines are rasterised here into three images, one
//! per turning band ([`Seal::layers`]), plain Rust and anti-aliased by each pixel's distance
//! to the stroke. The runes are text, one glyph per node, set round their circles by
//! [`band_glyphs`] in Noto Sans Runic, and turn with their band.

use procgen_rng::Mulberry32;

/// The plate's rune row (`renderRing`).
pub const RUNES: &str = "ᚠᚢᚦᚨᚱᚲᚷᚹᚺᚾᛁᛃᛇᛈᛉᛊᛏᛒᛖᛗᛚᛜᛞᛟ";

/// Seconds for each band to turn once, and which way (the plate's `fbspin` animations
/// `.sa`, `.sb` reversed, `.sc`).
pub const TURN_SECONDS: [f32; 3] = [300.0, -420.0, 200.0];

/// An RGBA image being drawn, straight alpha, colours in display (sRGB) values 0 to 1.
#[derive(Clone, Debug)]
pub struct Canvas {
    pub size: u32,
    /// Pixels per seal unit.
    pub scale: f32,
    pixels: Vec<[f32; 4]>,
}

/// A colour and its opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    pub rgb: [f32; 3],
    pub alpha: f32,
}

impl Ink {
    pub fn new(rgb: [f32; 3], alpha: f32) -> Self {
        Ink { rgb, alpha }
    }
}

impl Canvas {
    /// A clear canvas `size` pixels square, `scale` pixels to a seal unit, with the seal's
    /// centre in its middle.
    pub fn new(size: u32, scale: f32) -> Self {
        Canvas { size, scale, pixels: vec![[0.0; 4]; (size * size) as usize] }
    }

    /// The pixel position of seal point `p` (seal units from the centre, y down).
    fn to_pixels(&self, p: [f32; 2]) -> [f32; 2] {
        let c = self.size as f32 * 0.5;
        [c + p[0] * self.scale, c + p[1] * self.scale]
    }

    /// Lays `ink` over pixel `(x, y)` at `coverage`, source over.
    fn blend(&mut self, x: i32, y: i32, ink: Ink, coverage: f32) {
        if x < 0 || y < 0 || x >= self.size as i32 || y >= self.size as i32 || coverage <= 0.0 {
            return;
        }
        let a = (ink.alpha * coverage).min(1.0);
        let px = &mut self.pixels[(y as u32 * self.size + x as u32) as usize];
        let out = a + px[3] * (1.0 - a);
        if out <= 0.0 {
            return;
        }
        for i in 0..3 {
            px[i] = (ink.rgb[i] * a + px[i] * px[3] * (1.0 - a)) / out;
        }
        px[3] = out;
    }

    /// Draws a ring of radius `r` and line `width` (seal units) round the centre. `keep`
    /// says, by angle (radians, clockwise from +x), which parts of it are drawn.
    pub fn ring(&mut self, centre: [f32; 2], r: f32, width: f32, ink: Ink, keep: impl Fn(f32) -> bool) {
        let [cx, cy] = self.to_pixels(centre);
        let (r, half) = (r * self.scale, (width * self.scale * 0.5).max(0.5));
        let reach = r + half + 1.0;
        for y in (cy - reach).floor() as i32..=(cy + reach).ceil() as i32 {
            let dy = y as f32 + 0.5 - cy;
            if dy.abs() > reach {
                continue;
            }
            let outer = (reach * reach - dy * dy).max(0.0).sqrt();
            let inner_sq = (r - half - 1.0).max(0.0).powi(2) - dy * dy;
            let inner = if inner_sq > 0.0 { inner_sq.sqrt() } else { 0.0 };
            for side in [-1.0f32, 1.0] {
                let (a, b) = (cx + side * inner, cx + side * outer);
                for x in a.min(b).floor() as i32..=a.max(b).ceil() as i32 {
                    let dx = x as f32 + 0.5 - cx;
                    // Each half row once: the left half takes x < cx, the right the rest.
                    if (side < 0.0) != (dx < 0.0) {
                        continue;
                    }
                    let d = (dx * dx + dy * dy).sqrt();
                    let coverage = (half + 0.5 - (d - r).abs()).clamp(0.0, 1.0);
                    if coverage > 0.0 && keep(dy.atan2(dx)) {
                        self.blend(x, y, ink, coverage);
                    }
                }
            }
        }
    }

    /// Fills a disc of radius `r` at `centre`.
    pub fn disc(&mut self, centre: [f32; 2], r: f32, ink: Ink) {
        let [cx, cy] = self.to_pixels(centre);
        let r = r * self.scale;
        for y in (cy - r - 1.0).floor() as i32..=(cy + r + 1.0).ceil() as i32 {
            for x in (cx - r - 1.0).floor() as i32..=(cx + r + 1.0).ceil() as i32 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                self.blend(x, y, ink, (r + 0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// Draws a straight line of `width` from `a` to `b`, with round ends.
    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, ink: Ink) {
        let (a, b) = (self.to_pixels(a), self.to_pixels(b));
        let half = (width * self.scale * 0.5).max(0.5);
        let reach = half + 1.0;
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length_sq = (dx * dx + dy * dy).max(1e-6);
        let length = length_sq.sqrt();
        let (nx, ny) = (-dy / length, dx / length);
        let (x_lo, x_hi) = (a[0].min(b[0]) - reach, a[0].max(b[0]) + reach);
        for y in (a[1].min(b[1]) - reach).floor() as i32..=(a[1].max(b[1]) + reach).ceil() as i32 {
            let py = y as f32 + 0.5;
            // Where this row crosses the band either side of the line, within the ends.
            let (lo, hi) = if nx.abs() > 1e-4 {
                let x1 = a[0] + (reach - ny * (py - a[1])) / nx;
                let x2 = a[0] + (-reach - ny * (py - a[1])) / nx;
                (x1.min(x2).max(x_lo), x1.max(x2).min(x_hi))
            } else {
                (x_lo, x_hi)
            };
            for x in lo.floor() as i32..=hi.ceil() as i32 {
                let px = x as f32 + 0.5;
                let t = (((px - a[0]) * dx + (py - a[1]) * dy) / length_sq).clamp(0.0, 1.0);
                let d = ((px - a[0] - t * dx).powi(2) + (py - a[1] - t * dy).powi(2)).sqrt();
                self.blend(x, y, ink, (half + 0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// A radial gradient of `rgb` round `centre` out to `r`, its opacity `at(t)` at `t` of the
    /// way out (0 to 1, falling): concentric discs from the outside in, each adding what the
    /// gradient gains there.
    pub fn radial(&mut self, centre: [f32; 2], r: f32, rgb: [f32; 3], at: impl Fn(f32) -> f32) {
        let steps = 48;
        let mut below = 0.0;
        for i in (0..steps).rev() {
            let t = i as f32 / steps as f32;
            let want = at(t);
            let add = ((want - below) / (1.0 - below)).max(0.0);
            self.disc(centre, r * (t + 1.0 / steps as f32), Ink::new(rgb, add));
            below = want;
        }
    }

    /// Draws a closed or open path through `points`.
    pub fn path(&mut self, points: &[[f32; 2]], width: f32, ink: Ink) {
        for pair in points.windows(2) {
            self.line(pair[0], pair[1], width, ink);
        }
    }

    /// The pixels as 8-bit sRGB RGBA, straight alpha, row by row from the top.
    pub fn to_rgba8(&self) -> Vec<u8> {
        self.pixels.iter().flat_map(|p| [p[0], p[1], p[2], p[3]].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)).collect()
    }

    /// Scales every pixel's opacity by `factor`, so overlapping shapes drawn solid can be
    /// faded as one.
    pub fn fade(&mut self, factor: f32) {
        for p in &mut self.pixels {
            p[3] *= factor;
        }
    }

    /// The opacity at pixel `(x, y)`.
    pub fn alpha(&self, x: u32, y: u32) -> f32 {
        self.pixels[(y * self.size + x) as usize][3]
    }
}

/// The seal's colours: lines (drawn at 55%), gold, and the page behind the medallions; and
/// the gold glow's opacity at the centre (the plate's 0.2).
#[derive(Clone, Copy, Debug)]
pub struct SealInks {
    pub line: [f32; 3],
    pub gold: [f32; 3],
    pub page: [f32; 3],
    pub glow: f32,
}

/// The seal at radius `r` (the plate's `R`, its outer ring), in seal units.
#[derive(Clone, Copy, Debug)]
pub struct Seal {
    pub r: f32,
}

/// One rune of a band, set round its circle: its character, where its centre goes (seal
/// units from the centre) and its turn (radians, clockwise).
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub text: char,
    pub at: [f32; 2],
    pub turn: f32,
}

/// The plate's rune bands (`band(n, sep)`): `n` runes from `rng`, every `sep`th a spaced
/// dot instead.
pub fn band_text(rng: &mut Mulberry32, n: usize, sep: usize) -> String {
    let runes: Vec<char> = RUNES.chars().collect();
    let mut text = String::new();
    for i in 0..n {
        if i % sep == sep - 1 {
            text.push_str(" \u{b7} ");
        } else {
            text.push(runes[(rng.next_f64() * runes.len() as f64) as usize]);
        }
    }
    text
}

/// `text` set along a circle of radius `r` as SVG's `textPath` sets it on the plate's circles:
/// from the left of the circle clockwise, letters upright away from the centre, or from the
/// right anticlockwise (`reversed`), upright toward it. Each letter takes `advance` units
/// (spaces a third of that) and the text stops where the circle closes.
pub fn band_glyphs(text: &str, r: f32, advance: f32, reversed: bool) -> Vec<Glyph> {
    let circumference = std::f32::consts::TAU * r;
    let mut s = 0.0;
    let mut glyphs = Vec::new();
    for c in text.chars() {
        let width = if c == ' ' { advance / 3.0 } else { advance };
        if s + width > circumference {
            break;
        }
        let along = s + width * 0.5;
        s += width;
        if c == ' ' {
            continue;
        }
        let (angle, turn) = if reversed {
            let angle = -along / r;
            (angle, angle - std::f32::consts::FRAC_PI_2)
        } else {
            let angle = std::f32::consts::PI + along / r;
            (angle, angle + std::f32::consts::FRAC_PI_2)
        };
        glyphs.push(Glyph { text: c, at: [angle.cos() * r, angle.sin() * r], turn });
    }
    glyphs
}

/// A point at radius `r` and angle `a`.
fn polar(r: f32, a: f32) -> [f32; 2] {
    [a.cos() * r, a.sin() * r]
}

impl Seal {
    /// The seven points of the star (the plate's `V`), the first at the top.
    pub fn star_points(&self) -> [[f32; 2]; 7] {
        std::array::from_fn(|i| polar(self.r * 0.7, -std::f32::consts::FRAC_PI_2 + i as f32 / 7.0 * std::f32::consts::TAU))
    }

    /// The radius of the medallions at the star's points.
    pub fn medallion_r(&self) -> f32 {
        self.r * 0.062
    }

    /// The inner band's radius (the plate's `ri`).
    pub fn inner_r(&self) -> f32 {
        self.r * 0.48
    }

    /// The soft gold glow behind everything (the plate's `#rg` gradient, `.breathe`): 0.2 at
    /// the centre, 0.07 at 55%, none at the edge, scaled to `inks.glow` at the centre.
    pub fn draw_glow(&self, canvas: &mut Canvas, inks: SealInks) {
        let k = inks.glow / 0.2;
        let at = |t: f32| k * if t < 0.55 { 0.2 + (0.07 - 0.2) * t / 0.55 } else { 0.07 * (1.0 - (t - 0.55) / 0.45) };
        canvas.radial([0.0, 0.0], self.r * 1.04, inks.gold, at);
    }

    /// The outer band (the plate's `.sa`): its rings, ticks and dots.
    pub fn draw_outer(&self, canvas: &mut Canvas, inks: SealInks) {
        let r = self.r;
        let line = Ink::new(inks.line, 0.55);
        canvas.ring([0.0, 0.0], r, 1.6, line, |_| true);
        canvas.ring([0.0, 0.0], r - 4.0, 0.7, line, |_| true);
        for i in 0..180 {
            let a = i as f32 / 180.0 * std::f32::consts::TAU;
            let l = if i % 15 == 0 { 11.0 } else if i % 5 == 0 { 7.0 } else { 3.5 };
            canvas.line(polar(r - 3.0, a), polar(r - 3.0 - l, a), 0.9, line);
        }
        canvas.ring([0.0, 0.0], r - 38.0, 1.0, line, |_| true);
        for i in 0..56 {
            let size = if i % 4 == 0 { 1.8 } else { 1.0 };
            canvas.disc(polar(r * 0.89, i as f32 / 56.0 * std::f32::consts::TAU), size, Ink::new(inks.gold, 0.55));
        }
    }

    /// The middle band (`.sb`): the star, its heptagon, crescents between its points and
    /// medallions at them.
    pub fn draw_middle(&self, canvas: &mut Canvas, inks: SealInks) {
        let r = self.r;
        let line = Ink::new(inks.line, 0.55);
        let gold = Ink::new(inks.gold, 0.6);
        canvas.ring([0.0, 0.0], r * 0.76, 1.0, gold, |_| true);
        // Dashed 2 on, 5 off along the circle.
        let dashed = r * 0.86;
        canvas.ring([0.0, 0.0], dashed, 0.8, line, |a| (a.rem_euclid(std::f32::consts::TAU) * dashed).rem_euclid(7.0) < 2.0);
        let v = self.star_points();
        let star: Vec<[f32; 2]> = (0..=7).map(|i| v[(i * 3) % 7]).collect();
        let star2: Vec<[f32; 2]> = (0..=7).map(|i| v[(i * 2) % 7]).collect();
        let heptagon: Vec<[f32; 2]> = (0..=7).map(|i| v[i % 7]).collect();
        // The glow line, blurred in the plate (`feGaussianBlur` 2.4): wide faint strokes.
        for (width, alpha) in [(10.0, 0.05), (7.0, 0.08), (5.0, 0.12)] {
            canvas.path(&star, width, Ink::new(inks.gold, alpha));
        }
        canvas.path(&star, 1.3, gold);
        canvas.path(&star2, 0.8, line);
        canvas.path(&heptagon, 0.9, line);
        for i in 0..7 {
            let a = -std::f32::consts::FRAC_PI_2 + (i as f32 + 0.5) / 7.0 * std::f32::consts::TAU;
            // Open toward the rim: the arc leaves out 1.6 rad either side of outward.
            canvas.ring(polar(r * 0.585, a), r * 0.05, 1.0, gold, |b| {
                let off = (b - a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                off.abs() > 1.6
            });
        }
        let m = self.medallion_r();
        for p in v {
            canvas.disc(p, m, Ink::new(inks.page, 1.0));
            canvas.ring(p, m, 1.2, gold, |_| true);
            canvas.ring(p, m * 0.72, 0.8, line, |_| true);
        }
    }

    /// The inner band (`.sc`): two rings.
    pub fn draw_inner(&self, canvas: &mut Canvas, inks: SealInks) {
        let ri = self.inner_r();
        canvas.ring([0.0, 0.0], ri, 1.1, Ink::new(inks.gold, 0.6), |_| true);
        canvas.ring([0.0, 0.0], ri - 28.0, 0.8, Ink::new(inks.line, 0.55), |_| true);
    }

    /// The three turning bands, outer, middle and inner, each on a canvas `scale` pixels to
    /// a unit, the glow under the outer band.
    pub fn layers(&self, scale: f32, inks: SealInks) -> [Canvas; 3] {
        let size = (self.r * 2.12 * scale).ceil() as u32;
        let mut outer = Canvas::new(size, scale);
        self.draw_glow(&mut outer, inks);
        self.draw_outer(&mut outer, inks);
        let mut middle = Canvas::new(size, scale);
        self.draw_middle(&mut middle, inks);
        let mut inner = Canvas::new(size, scale);
        self.draw_inner(&mut inner, inks);
        [outer, middle, inner]
    }

    /// The runes of the three bands, outer, middle and inner, as the plate draws them from
    /// `mulberry32(7)`: 260 in nines round `R - 26`, 200 in sevens backwards round `0.82 R`,
    /// 120 in sixes round `ri - 14`, each cut off where its circle closes. `advances` are
    /// each band's letter advance in units.
    pub fn bands(&self, advances: [f32; 3]) -> [Vec<Glyph>; 3] {
        let mut rng = Mulberry32::new(7);
        let outer = band_text(&mut rng, 260, 9);
        let middle = band_text(&mut rng, 200, 7);
        let inner = band_text(&mut rng, 120, 6);
        [
            band_glyphs(&outer, self.r - 26.0, advances[0], false),
            band_glyphs(&middle, self.r * 0.82, advances[1], true),
            band_glyphs(&inner, self.inner_r() - 14.0, advances[2], false),
        ]
    }

    /// The rune in each medallion (the plate's `runes[(i*5+3) % len]`).
    pub fn medallion_runes(&self) -> [char; 7] {
        let runes: Vec<char> = RUNES.chars().collect();
        std::array::from_fn(|i| runes[(i * 5 + 3) % runes.len()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INKS: SealInks = SealInks { line: [0.5, 0.46, 0.4], gold: [0.66, 0.47, 0.18], page: [0.95, 0.93, 0.88], glow: 0.2 };

    #[test]
    fn rings_and_lines_land_where_they_are_drawn() {
        let mut canvas = Canvas::new(101, 1.0);
        canvas.ring([0.0, 0.0], 30.0, 2.0, Ink::new([1.0; 3], 1.0), |_| true);
        // On the ring, inside it and outside it.
        assert!(canvas.alpha(50 + 30, 50) > 0.9);
        assert!(canvas.alpha(50, 50 - 30) > 0.9);
        assert_eq!(canvas.alpha(50, 50), 0.0);
        assert_eq!(canvas.alpha(50 + 36, 50), 0.0);
        let mut canvas = Canvas::new(101, 1.0);
        canvas.line([-40.0, -40.0], [40.0, 40.0], 2.0, Ink::new([1.0; 3], 1.0));
        assert!(canvas.alpha(60, 60) > 0.9 && canvas.alpha(30, 30) > 0.9);
        assert_eq!(canvas.alpha(70, 30), 0.0);
        // A ring keeps only the arcs asked for.
        let mut canvas = Canvas::new(101, 1.0);
        canvas.ring([0.0, 0.0], 30.0, 2.0, Ink::new([1.0; 3], 1.0), |a| a.abs() < 1.0);
        assert!(canvas.alpha(80, 50) > 0.9 && canvas.alpha(20, 50) == 0.0);
    }

    #[test]
    fn the_seal_matches_the_plate_layout() {
        let seal = Seal { r: 264.0 };
        let v = seal.star_points();
        assert!(v[0][0].abs() < 1e-3 && (v[0][1] + 0.7 * 264.0).abs() < 1e-3, "the first point is at the top");
        let [outer, middle, inner] = seal.layers(1.0, INKS);
        let c = outer.size / 2;
        // The outer ring at R, the medallion at the top point, the inner ring at 0.48 R.
        assert!((263..=265).map(|d| outer.alpha(c + d, c)).fold(0.0, f32::max) > 0.3);
        assert!(middle.alpha(c, (c as f32 - 0.7 * 264.0) as u32) > 0.9);
        let ri = (0.48 * 264.0f32) as u32;
        assert!((ri - 1..=ri + 1).map(|d| inner.alpha(c + d, c)).fold(0.0, f32::max) > 0.3);
        assert!(inner.alpha(c, c) == 0.0);
    }

    #[test]
    fn rune_bands_follow_the_plate() {
        let mut rng = Mulberry32::new(7);
        let band = band_text(&mut rng, 9, 9);
        assert_eq!(band.chars().count(), 8 + 3, "eight runes then a spaced dot");
        assert!(band.ends_with(" \u{b7} "));
        let seal = Seal { r: 264.0 };
        let [outer, middle, inner] = seal.bands([14.0, 12.0, 12.0]);
        for (glyphs, r) in [(&outer, 238.0), (&middle, 264.0 * 0.82), (&inner, 264.0 * 0.48 - 14.0)] {
            assert!(glyphs.len() > 30);
            for g in glyphs.iter() {
                assert!(((g.at[0].powi(2) + g.at[1].powi(2)).sqrt() - r).abs() < 1e-2);
            }
        }
        // The outer band starts at the left and runs clockwise over the top, upright outward.
        let first = &outer[0];
        assert!(first.at[0] < -200.0 && first.at[1] < 0.0, "{:?}", first.at);
        assert!((first.turn - (std::f32::consts::PI + std::f32::consts::FRAC_PI_2)).abs() < 0.1);
        assert_eq!(seal.medallion_runes()[0], RUNES.chars().nth(3).unwrap());
    }
}
