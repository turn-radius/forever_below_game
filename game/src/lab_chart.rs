//! The fight tab's two charts ([`crate::lab_fight::FightSummary`]), drawn into a pixel canvas
//! that the lab shows as an image: their life over the rounds with its median and middle half,
//! and a histogram of the rounds a fight took. Each draws the saved rules' result faint beside
//! the edited one. Plain data in, pixels out, so the same summary draws the same chart and a
//! test can say so.

use crate::lab_fight::{CHART_ROUNDS, FightSummary};

/// The life chart's size in pixels, and the histogram's.
pub const LIFE_SIZE: (u32, u32) = (960, 360);
pub const HISTOGRAM_SIZE: (u32, u32) = (640, 360);
/// Room kept clear round the plot, in pixels.
const MARGIN: f32 = 14.0;

type Rgba = [u8; 4];
const BACKGROUND: Rgba = [0x1c, 0x19, 0x15, 255];
const GRID: Rgba = [0x3a, 0x32, 0x26, 255];
const GOLD: Rgba = [0xd9, 0xb4, 0x6a, 255];
const FAINT: Rgba = [0x7d, 0x73, 0x5f, 255];

/// A canvas of pixels, drawn on with alpha.
#[derive(Clone, Debug, PartialEq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// Rows of RGBA, top row first.
    pub pixels: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32, fill: Rgba) -> Canvas {
        Canvas { width, height, pixels: fill.iter().copied().cycle().take((width * height * 4) as usize).collect() }
    }

    /// Blends `colour` over a pixel at `coverage` (0 to 1).
    fn blend(&mut self, x: i32, y: i32, colour: Rgba, coverage: f32) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let at = ((y as u32 * self.width + x as u32) * 4) as usize;
        let a = coverage.clamp(0.0, 1.0) * colour[3] as f32 / 255.0;
        for c in 0..3 {
            self.pixels[at + c] = (self.pixels[at + c] as f32 * (1.0 - a) + colour[c] as f32 * a).round() as u8;
        }
    }

    /// A horizontal rule from `x0` to `x1` at `y`.
    pub fn hline(&mut self, x0: f32, x1: f32, y: f32, colour: Rgba) {
        self.line((x0, y), (x1, y), 1.5, colour, None);
    }

    /// A filled rectangle.
    pub fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, colour: Rgba) {
        for y in y0.round() as i32..y1.round() as i32 {
            for x in x0.round() as i32..x1.round() as i32 {
                self.blend(x, y, colour, 1.0);
            }
        }
    }

    /// A line `thickness` wide with round ends, solid or dashed `(on, off)`.
    pub fn line(&mut self, from: (f32, f32), to: (f32, f32), thickness: f32, colour: Rgba, dash: Option<(f32, f32)>) {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = dx.hypot(dy);
        let steps = (length * 2.0).ceil().max(1.0) as u32;
        let r = thickness / 2.0;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            if let Some((on, off)) = dash
                && (t * length) % (on + off) >= on
            {
                continue;
            }
            let (cx, cy) = (from.0 + dx * t, from.1 + dy * t);
            for y in (cy - r - 1.0).floor() as i32..=(cy + r + 1.0).ceil() as i32 {
                for x in (cx - r - 1.0).floor() as i32..=(cx + r + 1.0).ceil() as i32 {
                    let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                    self.blend(x, y, colour, (r + 0.5 - d).clamp(0.0, 1.0) * 0.5);
                }
            }
        }
    }

    /// Fills between two polylines, one point per `x`, a column at a time, at `alpha`.
    pub fn band(&mut self, xs: &[f32], low: &[f32], high: &[f32], colour: Rgba, alpha: f32) {
        for w in 0..xs.len().saturating_sub(1) {
            let (x0, x1) = (xs[w], xs[w + 1]);
            for x in x0.round() as i32..x1.round() as i32 {
                let t = (x as f32 + 0.5 - x0) / (x1 - x0).max(1e-6);
                let (y0, y1) = (low[w] + (low[w + 1] - low[w]) * t, high[w] + (high[w + 1] - high[w]) * t);
                for y in y0.min(y1).round() as i32..y0.max(y1).round().max(y0.min(y1).round() + 1.0) as i32 {
                    self.blend(x, y, colour, alpha);
                }
            }
        }
    }

    /// A number that is the same for the same pixels, for a test.
    pub fn checksum(&self) -> u64 {
        self.pixels.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
    }
}

/// A chart and the words for its edges.
#[derive(Clone, Debug, PartialEq)]
pub struct Chart {
    pub canvas: Canvas,
    /// The top and bottom of the vertical axis.
    pub y_labels: (String, String),
    /// The left and right of the horizontal axis.
    pub x_labels: (String, String),
}

/// Their life over the rounds: the middle half of fights shaded, the median over it, and the
/// saved rules' median dashed and faint.
pub fn life_chart(now: &FightSummary, was: Option<&FightSummary>) -> Chart {
    let (w, h) = LIFE_SIZE;
    let mut canvas = Canvas::new(w, h, BACKGROUND);
    let rounds = now.life.len().max(was.map_or(0, |s| s.life.len())).saturating_sub(1).max(1);
    let full = now.full_life.max(1) as f32;
    let x = |round: usize| MARGIN + round as f32 / rounds as f32 * (w as f32 - 2.0 * MARGIN);
    let y = |life: i32| h as f32 - MARGIN - (life.max(0) as f32 / full).min(1.0) * (h as f32 - 2.0 * MARGIN);
    for k in 0..=4 {
        let gy = y((full * k as f32 / 4.0).round() as i32);
        canvas.hline(MARGIN, w as f32 - MARGIN, gy, GRID);
    }
    let points = |s: &FightSummary, pick: fn(&crate::lab_fight::LifeBand) -> i32| -> (Vec<f32>, Vec<f32>) { (0..s.life.len()).map(|r| (x(r), y(pick(&s.life[r])))).unzip() };
    let (xs, lows) = points(now, |b| b.low);
    let (_, highs) = points(now, |b| b.high);
    canvas.band(&xs, &lows, &highs, GOLD, 0.22);
    let polyline = |canvas: &mut Canvas, s: &FightSummary, thickness: f32, colour: Rgba, dash: Option<(f32, f32)>| {
        let (xs, ys) = points(s, |b| b.median);
        for i in 0..xs.len().saturating_sub(1) {
            canvas.line((xs[i], ys[i]), (xs[i + 1], ys[i + 1]), thickness, colour, dash);
        }
    };
    if let Some(was) = was {
        polyline(&mut canvas, was, 3.0, FAINT, Some((12.0, 9.0)));
    }
    polyline(&mut canvas, now, 5.0, GOLD, None);
    Chart { canvas, y_labels: (format!("{}", now.full_life), "0".to_string()), x_labels: ("start".to_string(), format!("round {rounds}")) }
}

/// How many fights took each number of rounds, the saved rules' bars faint behind.
pub fn histogram_chart(now: &FightSummary, was: Option<&FightSummary>) -> Chart {
    let (w, h) = HISTOGRAM_SIZE;
    let mut canvas = Canvas::new(w, h, BACKGROUND);
    let last = |s: &FightSummary| s.rounds.iter().rposition(|&n| n > 0).unwrap_or(1);
    let shown = last(now).max(was.map_or(0, last)).clamp(6, CHART_ROUNDS);
    let tallest = now.rounds.iter().chain(was.map_or(&[][..], |s| &s.rounds[..])).copied().max().unwrap_or(1).max(1);
    for k in 0..=4 {
        canvas.hline(MARGIN, w as f32 - MARGIN, h as f32 - MARGIN - k as f32 / 4.0 * (h as f32 - 2.0 * MARGIN), GRID);
    }
    let slot = (w as f32 - 2.0 * MARGIN) / shown as f32;
    let top = |n: u32| h as f32 - MARGIN - n as f32 / tallest as f32 * (h as f32 - 2.0 * MARGIN);
    for round in 1..=shown {
        let left = MARGIN + (round - 1) as f32 * slot;
        if let Some(was) = was {
            canvas.rect(left + slot * 0.12, top(was.rounds[round]), left + slot * 0.88, h as f32 - MARGIN, [FAINT[0], FAINT[1], FAINT[2], 150]);
        }
        canvas.rect(left + slot * 0.2, top(now.rounds[round]), left + slot * 0.8, h as f32 - MARGIN, GOLD);
    }
    Chart {
        canvas,
        y_labels: (format!("{tallest}"), "0".to_string()),
        x_labels: ("1 round".to_string(), if shown == CHART_ROUNDS { format!("{shown}+") } else { format!("{shown}") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab_fight::{THOUSAND, fights};
    use crate::lab_view::LabState;

    fn summary(seed: u32) -> FightSummary {
        let state = LabState::default();
        fights(&state, &state.ruleset.rules, seed, THOUSAND)
    }

    fn painted(c: &Canvas) -> usize {
        c.pixels.chunks(4).filter(|p| p[..3] != BACKGROUND[..3]).count()
    }

    #[test]
    fn the_same_summary_draws_the_same_chart_and_another_does_not() {
        let (a, b) = (summary(1), summary(1));
        assert_eq!(life_chart(&a, None), life_chart(&b, None));
        assert_eq!(histogram_chart(&a, None).canvas.checksum(), histogram_chart(&b, None).canvas.checksum());
        assert_ne!(life_chart(&a, None).canvas.checksum(), life_chart(&summary(2), None).canvas.checksum());
    }

    #[test]
    fn the_charts_draw_their_data_and_the_faint_baseline() {
        let now = summary(1);
        let plain = life_chart(&now, None);
        assert!(painted(&plain.canvas) > 5_000);
        let state = LabState::default();
        let mut worse = state.clone();
        crate::lab_rules::set_leaf(&mut worse.ruleset, "creatures/Giant Rat/attack_bonus", crate::lab_rules::Leaf::Int(8));
        let edited = fights(&worse, &worse.ruleset.rules, 1, THOUSAND);
        let both = life_chart(&edited, Some(&now));
        assert_ne!(both.canvas, life_chart(&edited, None).canvas, "the baseline is drawn");
        assert_ne!(histogram_chart(&edited, Some(&now)).canvas, histogram_chart(&edited, None).canvas);
        assert_eq!((plain.y_labels.0.as_str(), plain.y_labels.1.as_str()), ("10", "0"));
    }

    #[test]
    fn a_line_and_a_band_land_where_they_are_put() {
        let mut c = Canvas::new(40, 40, BACKGROUND);
        c.line((5.0, 20.0), (35.0, 20.0), 3.0, GOLD, None);
        let at = |x: u32, y: u32| c.pixels[((y * 40 + x) * 4) as usize];
        assert_ne!(at(20, 20), BACKGROUND[0]);
        assert_eq!(at(20, 5), BACKGROUND[0]);
        let mut d = Canvas::new(40, 40, BACKGROUND);
        d.band(&[0.0, 40.0], &[10.0, 10.0], &[30.0, 30.0], GOLD, 1.0);
        assert_eq!(d.pixels[((20 * 40 + 20) * 4) as usize], GOLD[0]);
        assert_eq!(d.pixels[((5 * 40 + 20) * 4) as usize], BACKGROUND[0]);
    }
}
