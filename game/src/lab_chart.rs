//! The fight tab's two charts ([`crate::lab_fight::FightSummary`]), drawn into a pixel canvas
//! that the lab shows as an image: their life over the rounds with its median and middle half,
//! and a histogram of the rounds a fight took. Each draws the saved rules' result faint beside
//! the edited one. Plain data in, pixels out, so the same summary draws the same chart and a
//! test can say so.

use crate::lab_fight::{CHART_ROUNDS, FightSummary};
use crate::lab_timeline::{Event, Sample, Timeline};

/// The life chart's size in pixels, and the histogram's.
pub const LIFE_SIZE: (u32, u32) = (960, 360);
pub const HISTOGRAM_SIZE: (u32, u32) = (640, 360);
pub const TIME_SIZE: (u32, u32) = (1200, 420);
/// Room kept clear round the plot, in pixels.
const MARGIN: f32 = 14.0;

type Rgba = [u8; 4];
const BACKGROUND: Rgba = [0x1c, 0x19, 0x15, 255];
const GRID: Rgba = [0x3a, 0x32, 0x26, 255];
const GOLD: Rgba = [0xd9, 0xb4, 0x6a, 255];
const FAINT: Rgba = [0x7d, 0x73, 0x5f, 255];
/// The needs' lines, and the exhaustion step line.
pub const HUNGER: Rgba = [0xc4, 0x8a, 0x52, 255];
pub const THIRST: Rgba = [0x6f, 0x9b, 0xe0, 255];
pub const SLEEP: Rgba = [0x86, 0xba, 0x80, 255];
pub const EXHAUSTION: Rgba = [0xe6, 0xdc, 0xc6, 255];
const DRAW: Rgba = [0xc4, 0x8a, 0x52, 255];
const DEATH: Rgba = [0xea, 0x73, 0x5b, 255];

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
        for (c, &tone) in colour.iter().take(3).enumerate() {
            self.pixels[at + c] = (self.pixels[at + c] as f32 * (1.0 - a) + tone as f32 * a).round() as u8;
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
    /// For the histogram, the share of the width the "draw" column takes at the right.
    pub draw: Option<f32>,
    /// Words to set over the chart at a share of its width: where the run dies.
    pub marks: Vec<(f32, String)>,
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
    Chart { canvas, y_labels: (format!("{}", now.full_life), "0".to_string()), x_labels: ("start".to_string(), format!("round {rounds}")), draw: None, marks: Vec::new() }
}

/// How many fights took each number of rounds, the saved rules' bars faint behind, and a column
/// of its own at the right for the draws.
pub fn histogram_chart(now: &FightSummary, was: Option<&FightSummary>) -> Chart {
    let (w, h) = HISTOGRAM_SIZE;
    let mut canvas = Canvas::new(w, h, BACKGROUND);
    let last = |s: &FightSummary| s.rounds.iter().rposition(|&n| n > 0).unwrap_or(1);
    let shown = last(now).max(was.map_or(0, last)).clamp(6, CHART_ROUNDS);
    let draws = now.draws > 0 || was.is_some_and(|s| s.draws > 0);
    let columns = shown + draws as usize;
    let counts = |s: &FightSummary, column: usize| if column < shown { s.rounds[column + 1] } else { s.draws };
    let tallest = (0..columns).flat_map(|c| [Some(counts(now, c)), was.map(|s| counts(s, c))]).flatten().max().unwrap_or(1).max(1);
    for k in 0..=4 {
        canvas.hline(MARGIN, w as f32 - MARGIN, h as f32 - MARGIN - k as f32 / 4.0 * (h as f32 - 2.0 * MARGIN), GRID);
    }
    let slot = (w as f32 - 2.0 * MARGIN) / columns as f32;
    let top = |n: u32| h as f32 - MARGIN - n as f32 / tallest as f32 * (h as f32 - 2.0 * MARGIN);
    for column in 0..columns {
        let left = MARGIN + column as f32 * slot;
        if let Some(was) = was {
            canvas.rect(left + slot * 0.12, top(counts(was, column)), left + slot * 0.88, h as f32 - MARGIN, [FAINT[0], FAINT[1], FAINT[2], 150]);
        }
        canvas.rect(left + slot * 0.2, top(counts(now, column)), left + slot * 0.8, h as f32 - MARGIN, if column < shown { GOLD } else { DRAW });
    }
    Chart {
        canvas,
        y_labels: (format!("{tallest}"), "0".to_string()),
        x_labels: ("1 round".to_string(), if shown == CHART_ROUNDS { format!("{shown}+") } else { format!("{shown}") }),
        draw: draws.then(|| 1.0 / columns as f32),
        marks: Vec::new(),
    }
}

/// The needs against game time, 0 to 1, with the band edges as faint rules; exhaustion as a
/// step line on its own scale (level 0 to 6 over the same height); the saved rules' run faint
/// and dashed; and a tick at the bottom where something was eaten or drunk.
#[allow(clippy::type_complexity)]
pub fn time_chart(now: &Timeline, was: Option<&Timeline>, edges: &[f64]) -> Chart {
    let (w, h) = TIME_SIZE;
    let mut canvas = Canvas::new(w, h, BACKGROUND);
    let end = now.end().max(was.map_or(0.0, |t| t.end())).max(1.0) as f32;
    let x = |hours: f64| MARGIN + hours as f32 / end * (w as f32 - 2.0 * MARGIN);
    let y = |share: f64| h as f32 - MARGIN - (share as f32).clamp(0.0, 1.0) * (h as f32 - 2.0 * MARGIN);
    for share in std::iter::once(&0.0).chain(edges) {
        canvas.hline(MARGIN, w as f32 - MARGIN, y(*share), GRID);
    }
    let lines: [(Rgba, fn(&Sample) -> f64); 3] = [(HUNGER, |s| s.hunger), (THIRST, |s| s.thirst), (SLEEP, |s| s.sleepiness)];
    let draw = |canvas: &mut Canvas, run: &Timeline, thickness: f32, faint: bool| {
        let tint = |c: Rgba| if faint { [c[0], c[1], c[2], 120] } else { c };
        let dash = faint.then_some((10.0, 8.0));
        for (colour, pick) in lines.iter().copied().chain([(EXHAUSTION, (|s: &Sample| s.exhaustion as f64 / 6.0) as fn(&Sample) -> f64)]) {
            let step = colour == EXHAUSTION;
            for pair in run.samples.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                let (from, to) = ((x(a.hours), y(pick(a))), (x(b.hours), y(pick(b))));
                if step {
                    // Held until the next sample, then up.
                    canvas.line(from, (to.0, from.1), thickness, tint(colour), dash);
                    canvas.line((to.0, from.1), to, thickness, tint(colour), dash);
                } else {
                    canvas.line(from, to, thickness, tint(colour), dash);
                }
            }
        }
    };
    if let Some(was) = was {
        draw(&mut canvas, was, 3.0, true);
    }
    for (hours, event) in &now.events {
        let colour = if *event == Event::Eat { HUNGER } else { THIRST };
        canvas.line((x(*hours), y(0.0)), (x(*hours), y(0.06)), 4.0, colour, None);
    }
    draw(&mut canvas, now, 4.0, false);
    let mut marks = Vec::new();
    if let Some(dies) = crate::lab_timeline::death(now) {
        canvas.line((x(dies), y(0.0)), (x(dies), y(1.0)), 3.0, DEATH, None);
        marks.push((x(dies) / w as f32, "dies".to_string()));
    }
    Chart { canvas, y_labels: ("full".to_string(), "0".to_string()), x_labels: ("start".to_string(), hours_label(end as f64)), draw: None, marks }
}

/// The colours the effects' lines take, in the order the effects began.
pub const EFFECT_COLOURS: [Rgba; 6] = [GOLD, THIRST, DEATH, SLEEP, [0xb5, 0x8c, 0xd9, 255], EXHAUSTION];

/// Each effect's intensity (0 to 1) against game time, in [`EFFECT_COLOURS`] by the order of
/// `ids`: solid where it shows, dotted while it hides (a disease's incubation), the edges of its
/// stages as faint rules in its colour, and the saved rules' run faint and dashed. A tick at the
/// bottom marks where an effect was put on.
pub fn effects_chart(now: &Timeline, was: Option<&Timeline>, ids: &[String], rules: &fb_character::rules::Rules) -> Chart {
    let (w, h) = TIME_SIZE;
    let mut canvas = Canvas::new(w, h, BACKGROUND);
    let end = now.end().max(was.map_or(0.0, |t| t.end())).max(1.0) as f32;
    let x = |hours: f64| MARGIN + hours as f32 / end * (w as f32 - 2.0 * MARGIN);
    let y = |share: f64| h as f32 - MARGIN - (share as f32).clamp(0.0, 1.0) * (h as f32 - 2.0 * MARGIN);
    for share in [0.0, 1.0] {
        canvas.hline(MARGIN, w as f32 - MARGIN, y(share), GRID);
    }
    let colour_of = |i: usize| EFFECT_COLOURS[i % EFFECT_COLOURS.len()];
    for (i, id) in ids.iter().enumerate() {
        let c = colour_of(i);
        for stage in rules.effects.get(id).into_iter().flat_map(|d| &d.stages).filter(|s| s.at > 0.0) {
            canvas.line((MARGIN, y(stage.at)), (w as f32 - MARGIN, y(stage.at)), 1.5, [c[0], c[1], c[2], 90], Some((4.0, 6.0)));
        }
    }
    let draw = |canvas: &mut Canvas, run: &Timeline, thickness: f32, faint: bool| {
        for (i, id) in ids.iter().enumerate() {
            let c = colour_of(i);
            let colour = if faint { [c[0], c[1], c[2], 120] } else { c };
            let at = |trace: &crate::lab_timeline::Trace| trace.iter().find(|(e, _, _)| e == id).map(|(_, v, shows)| (*v, *shows));
            for (pair, traces) in run.samples.windows(2).zip(run.traces.windows(2)) {
                let (Some((a, _)), Some((b, shows))) = (at(&traces[0]), at(&traces[1])) else { continue };
                let dash = if faint { Some((10.0, 8.0)) } else if shows { None } else { Some((3.0, 6.0)) };
                canvas.line((x(pair[0].hours), y(a)), (x(pair[1].hours), y(b)), thickness, colour, dash);
            }
        }
    };
    if let Some(was) = was {
        draw(&mut canvas, was, 3.0, true);
    }
    for (hours, event) in &now.events {
        if *event == Event::Applied {
            canvas.line((x(*hours), y(0.0)), (x(*hours), y(0.06)), 4.0, EXHAUSTION, None);
        }
    }
    draw(&mut canvas, now, 4.0, false);
    Chart { canvas, y_labels: ("full".to_string(), "0".to_string()), x_labels: ("start".to_string(), hours_label(end as f64)), draw: None, marks: Vec::new() }
}

fn hours_label(hours: f64) -> String {
    if hours >= 72.0 { format!("{} days", (hours / 24.0).round()) } else { format!("{} h", hours.round()) }
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
    fn the_effects_chart_draws_each_effect_and_the_saved_rules_faint() {
        use crate::lab_timeline::{Step, effects_seen, run};
        let mut state = LabState::default();
        state.slots.iter_mut().for_each(|s| s.base = None);
        state.depth = Some(4);
        let steps = [Step::Apply("sewer_plague", 0.1), Step::Pass(48.0, fb_character::Activity::Walking)];
        let now = run(&state, &steps);
        let ids = effects_seen(&now);
        assert!(ids.len() >= 2, "plague and the cold: {ids:?}");
        let empty = effects_chart(&run(&state, &[]), None, &[], &state.ruleset.rules);
        let plain = effects_chart(&now, None, &ids, &state.ruleset.rules);
        assert!(painted(&plain.canvas) > painted(&empty.canvas) + 2_000);
        let mut slower = state.clone();
        crate::lab_rules::step(&mut slower.ruleset, "effects/cold/exposure_per_hour", -1);
        let edited = run(&slower, &steps);
        assert_ne!(effects_chart(&edited, Some(&now), &ids, &slower.ruleset.rules).canvas, effects_chart(&edited, None, &ids, &slower.ruleset.rules).canvas);
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
