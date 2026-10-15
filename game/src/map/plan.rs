//! The level drawn as an inked plan on dark paper, as the map shows it ([`draw_plan`]): the
//! floor of each seen cell washed a shade lighter than the page, rock beside it hatched, walls
//! as ink lines that wander a little as a hand draws them, doorways as gaps with a door drawn
//! across, the entrance marked, and small symbols for seen props and lights. Cells not seen
//! are left blank. Pure: the same level and the same seen cells always draw the same picture.
//!
//! Pixels are in the image's own space: `x` east, `y` south, `px` pixels to a cell, with a
//! margin of [`MARGIN`] cells round the level.

use armory::build::scenery::Scenery;
use fb_level::props::Prop;
use fb_level::sight::Explored;
use fb_level::{Cell, Level, LightKind};

/// Cells of blank page round the level.
pub const MARGIN: f32 = 1.5;

/// The ledger's colours, as linear-free sRGB triples (`crate::ui`).
const WASH: [f32; 3] = [0.20, 0.176, 0.145];
const INK: [f32; 3] = [0.902, 0.863, 0.776];
const HATCH: [f32; 3] = [0.369, 0.314, 0.251];
const ACCENT: [f32; 3] = [0.769, 0.541, 0.322];
const GLOW: [f32; 3] = [0.851, 0.569, 0.247];
const CAP: [f32; 3] = [0.525, 0.729, 0.502];

/// An RGBA picture being drawn, each pixel premultiplied by its alpha.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanCanvas {
    pub width: u32,
    pub height: u32,
    pixels: Vec<[f32; 4]>,
}

impl PlanCanvas {
    pub fn new(width: u32, height: u32) -> Self {
        PlanCanvas { width, height, pixels: vec![[0.0; 4]; (width * height) as usize] }
    }

    /// Lays `rgb` at `alpha` over pixel `(x, y)`.
    fn blend(&mut self, x: i32, y: i32, rgb: [f32; 3], alpha: f32) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 || alpha <= 0.0 {
            return;
        }
        let p = &mut self.pixels[(y as u32 * self.width + x as u32) as usize];
        let a = alpha.min(1.0);
        for (c, &v) in p.iter_mut().zip(&rgb) {
            *c = v * a + *c * (1.0 - a);
        }
        p[3] = a + p[3] * (1.0 - a);
    }

    /// The opacity of pixel `(x, y)`.
    pub fn alpha(&self, x: u32, y: u32) -> f32 {
        self.pixels[(y * self.width + x) as usize][3]
    }

    /// Fills the rectangle from `a` to `b`.
    pub fn rect(&mut self, a: [f32; 2], b: [f32; 2], rgb: [f32; 3], alpha: f32) {
        for y in a[1].floor() as i32..b[1].ceil() as i32 {
            for x in a[0].floor() as i32..b[0].ceil() as i32 {
                // Coverage of the pixel by the rectangle, for soft edges.
                let cx = ((x as f32 + 1.0).min(b[0]) - (x as f32).max(a[0])).clamp(0.0, 1.0);
                let cy = ((y as f32 + 1.0).min(b[1]) - (y as f32).max(a[1])).clamp(0.0, 1.0);
                self.blend(x, y, rgb, alpha * cx * cy);
            }
        }
    }

    /// A line from `a` to `b`, `width` pixels wide with round ends, antialiased.
    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, rgb: [f32; 3], alpha: f32) {
        let r = width * 0.5;
        let (lo_x, hi_x) = (a[0].min(b[0]) - r - 1.0, a[0].max(b[0]) + r + 1.0);
        let (lo_y, hi_y) = (a[1].min(b[1]) - r - 1.0, a[1].max(b[1]) + r + 1.0);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = (dx * dx + dy * dy).max(1e-6);
        for y in lo_y.floor() as i32..=hi_y.ceil() as i32 {
            for x in lo_x.floor() as i32..=hi_x.ceil() as i32 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let t = (((px - a[0]) * dx + (py - a[1]) * dy) / len2).clamp(0.0, 1.0);
                let d = (px - (a[0] + t * dx)).hypot(py - (a[1] + t * dy));
                let cover = (r + 0.5 - d).clamp(0.0, 1.0);
                if cover > 0.0 {
                    self.blend(x, y, rgb, alpha * cover);
                }
            }
        }
    }

    /// A filled circle.
    pub fn disc(&mut self, c: [f32; 2], r: f32, rgb: [f32; 3], alpha: f32) {
        for y in (c[1] - r - 1.0).floor() as i32..=(c[1] + r + 1.0).ceil() as i32 {
            for x in (c[0] - r - 1.0).floor() as i32..=(c[0] + r + 1.0).ceil() as i32 {
                let d = (x as f32 + 0.5 - c[0]).hypot(y as f32 + 0.5 - c[1]);
                self.blend(x, y, rgb, alpha * (r + 0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// A circle's outline.
    pub fn ring(&mut self, c: [f32; 2], r: f32, width: f32, rgb: [f32; 3], alpha: f32) {
        for y in (c[1] - r - width).floor() as i32..=(c[1] + r + width).ceil() as i32 {
            for x in (c[0] - r - width).floor() as i32..=(c[0] + r + width).ceil() as i32 {
                let d = ((x as f32 + 0.5 - c[0]).hypot(y as f32 + 0.5 - c[1]) - r).abs();
                self.blend(x, y, rgb, alpha * (width * 0.5 + 0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// The picture as 8-bit sRGB with straight alpha, for an image.
    pub fn to_rgba8(&self) -> Vec<u8> {
        self.pixels
            .iter()
            .flat_map(|p| {
                let a = p[3];
                let un = |c: f32| if a > 0.0 { (c / a).clamp(0.0, 1.0) } else { 0.0 };
                [un(p[0]), un(p[1]), un(p[2]), a.clamp(0.0, 1.0)].map(|v| (v * 255.0).round() as u8)
            })
            .collect()
    }
}

/// A small, steady wobble for the point `(x, y)` on the grid, -1..1 on each axis: where a hand
/// would have drawn it, so lines meeting at a corner meet at the same wobbled point.
fn wobble(x: f32, y: f32, seed: u32) -> (f32, f32) {
    let h = |k: f32| ((x * 127.1 + y * 311.7 + k + seed as f32 * 0.013).sin() * 43_758.547).rem_euclid(1.0);
    (h(0.0) * 2.0 - 1.0, h(57.3) * 2.0 - 1.0)
}

/// How a level is laid on the picture: pixels to a cell, and where cell `(0, 0)`'s corner is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanFrame {
    pub px: f32,
    pub origin: [f32; 2],
}

impl PlanFrame {
    /// The frame for `level` at `px` pixels to a cell, with a margin round it.
    pub fn of(px: f32) -> Self {
        PlanFrame { px, origin: [MARGIN * px, MARGIN * px] }
    }

    /// The picture's size for `level`.
    pub fn size(&self, level: &Level) -> (u32, u32) {
        let side = |cells: u32| ((cells as f32 + 2.0 * MARGIN) * self.px).ceil() as u32;
        (side(level.width), side(level.height))
    }

    /// Where the grid point `(x, y)` (in cells) lands on the picture.
    pub fn at(&self, x: f32, y: f32) -> [f32; 2] {
        [self.origin[0] + x * self.px, self.origin[1] + y * self.px]
    }
}

/// `level` as far as `explored` has seen it, `px` pixels to a cell.
pub fn draw_plan(level: &Level, explored: &Explored, px: f32) -> PlanCanvas {
    let frame = PlanFrame::of(px);
    let (w, h) = frame.size(level);
    let mut canvas = PlanCanvas::new(w, h);
    let seen = |x: i32, y: i32| explored.is_seen(x, y) && level.is_open(x, y);
    let ink = (px * 0.085).max(1.2);
    let jitter = px * 0.045;
    let wobbled = |x: f32, y: f32| {
        let (wx, wy) = wobble(x, y, level.seed);
        let p = frame.at(x, y);
        [p[0] + wx * jitter, p[1] + wy * jitter]
    };

    // The floor's wash, and hatching on the rock beside it.
    for y in 0..level.height as i32 {
        for x in 0..level.width as i32 {
            if seen(x, y) {
                canvas.rect(frame.at(x as f32, y as f32), frame.at(x as f32 + 1.0, y as f32 + 1.0), WASH, 0.9);
                continue;
            }
            let beside = (-1..=1).any(|dy| (-1..=1).any(|dx| seen(x + dx, y + dy)));
            if beside && !level.is_open(x, y) {
                hatch(&mut canvas, &frame, x, y, level.seed);
            }
        }
    }

    // Walls: each side of a seen cell that faces rock, in three wandering strokes.
    for y in 0..level.height as i32 {
        for x in 0..level.width as i32 {
            if !seen(x, y) {
                continue;
            }
            let (fx, fy) = (x as f32, y as f32);
            let sides = [((0, -1), (fx, fy), (fx + 1.0, fy)), ((0, 1), (fx, fy + 1.0), (fx + 1.0, fy + 1.0)), ((-1, 0), (fx, fy), (fx, fy + 1.0)), ((1, 0), (fx + 1.0, fy), (fx + 1.0, fy + 1.0))];
            for ((dx, dy), a, b) in sides {
                if level.is_open(x + dx, y + dy) {
                    continue;
                }
                let points: Vec<[f32; 2]> = (0..=3).map(|k| k as f32 / 3.0).map(|t| wobbled(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)).collect();
                for pair in points.windows(2) {
                    canvas.line(pair[0], pair[1], ink, INK, 0.92);
                }
            }
            if level.cell(x, y) == Cell::Door {
                door(&mut canvas, &frame, level, x, y, ink);
            }
        }
    }

    // The entrance: a heavy bar on the outer edge and an arrow pointing in.
    let e = level.entrance;
    if seen(e.x as i32, e.y as i32) {
        let (dx, dy) = e.inward.step();
        let c = [e.x as f32 + 0.5, e.y as f32 + 0.5];
        let back = [c[0] - dx as f32 * 0.5, c[1] - dy as f32 * 0.5];
        let side = [dy as f32 * 0.45, dx as f32 * 0.45];
        canvas.line(frame.at(back[0] - side[0], back[1] - side[1]), frame.at(back[0] + side[0], back[1] + side[1]), ink * 2.2, ACCENT, 1.0);
        let tip = [c[0] + dx as f32 * 0.3, c[1] + dy as f32 * 0.3];
        let tail = [c[0] - dx as f32 * 0.25, c[1] - dy as f32 * 0.25];
        canvas.line(frame.at(tail[0], tail[1]), frame.at(tip[0], tip[1]), ink, ACCENT, 1.0);
        for s in [1.0, -1.0] {
            let wing = [tip[0] - dx as f32 * 0.2 + side[0] * 0.4 * s, tip[1] - dy as f32 * 0.2 + side[1] * 0.4 * s];
            canvas.line(frame.at(tip[0], tip[1]), frame.at(wing[0], wing[1]), ink, ACCENT, 1.0);
        }
    }

    // Seen props and lights.
    for spot in &level.props {
        if !seen(spot.x.floor() as i32, spot.y.floor() as i32) {
            continue;
        }
        let c = frame.at(spot.x, spot.y);
        match spot.prop {
            Prop::Container { .. } => {
                let r = px * 0.2;
                canvas.rect([c[0] - r, c[1] - r * 0.7], [c[0] + r, c[1] + r * 0.7], INK, 0.85);
                canvas.rect([c[0] - r + ink, c[1] - r * 0.7 + ink], [c[0] + r - ink, c[1] + r * 0.7 - ink], WASH, 1.0);
            }
            Prop::Scenery { kind: Scenery::Pillar, .. } => canvas.disc(c, px * 0.2, INK, 0.9),
            Prop::Scenery { kind: Scenery::Rubble, .. } => {
                for k in 0..4 {
                    let (wx, wy) = wobble(spot.x + k as f32, spot.y, 7);
                    canvas.disc([c[0] + wx * px * 0.18, c[1] + wy * px * 0.18], px * 0.06, INK, 0.75);
                }
            }
            Prop::Scenery { kind: Scenery::Bones, .. } => {
                canvas.line([c[0] - px * 0.15, c[1] - px * 0.15], [c[0] + px * 0.15, c[1] + px * 0.15], ink, INK, 0.75);
                canvas.line([c[0] - px * 0.15, c[1] + px * 0.15], [c[0] + px * 0.15, c[1] - px * 0.15], ink, INK, 0.75);
            }
        }
    }
    for light in &level.lights {
        if !seen(light.x.floor() as i32, light.y.floor() as i32) {
            continue;
        }
        let c = frame.at(light.x, light.y);
        match light.kind {
            LightKind::Brazier => {
                canvas.ring(c, px * 0.2, ink, GLOW, 0.95);
                canvas.disc(c, px * 0.08, GLOW, 1.0);
            }
            LightKind::Glowcaps => {
                for k in 0..3 {
                    let (wx, wy) = wobble(light.x, light.y + k as f32, 11);
                    canvas.disc([c[0] + wx * px * 0.12, c[1] + wy * px * 0.12], px * 0.05, CAP, 0.95);
                }
            }
        }
    }
    canvas
}

/// Diagonal hatching across a rock cell, as a plan shades solid ground.
fn hatch(canvas: &mut PlanCanvas, frame: &PlanFrame, x: i32, y: i32, seed: u32) {
    let (fx, fy) = (x as f32, y as f32);
    for k in 1..4 {
        let t = k as f32 / 4.0;
        let (wx, _) = wobble(fx + t, fy, seed ^ 5);
        let shift = wx * 0.05;
        // A stroke from the left or top edge down to the bottom or right edge, at 45 degrees.
        let (a, b) = ([fx, fy + t * 2.0 + shift], [fx + t * 2.0 + shift, fy]);
        let clip = |p: [f32; 2]| [p[0].clamp(fx, fx + 1.0), p[1].clamp(fy, fy + 1.0)];
        let (a, b) = (clip(a), clip(b));
        canvas.line(frame.at(a[0], a[1]), frame.at(b[0], b[1]), frame.px * 0.04 + 0.6, HATCH, 0.7);
    }
}

/// A door across the doorway at `(x, y)`: a narrow leaf across the passage, its frame drawn by
/// the walls either side.
fn door(canvas: &mut PlanCanvas, frame: &PlanFrame, level: &Level, x: i32, y: i32, ink: f32) {
    let across_x = level.is_open(x - 1, y) || level.is_open(x + 1, y);
    let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
    let (a, b) = if across_x { ((cx - 0.08, cy - 0.42), (cx + 0.08, cy + 0.42)) } else { ((cx - 0.42, cy - 0.08), (cx + 0.42, cy + 0.08)) };
    let (pa, pb) = (frame.at(a.0, a.1), frame.at(b.0, b.1));
    canvas.rect(pa, pb, INK, 0.9);
    canvas.rect([pa[0] + ink * 0.6, pa[1] + ink * 0.6], [pb[0] - ink * 0.6, pb[1] - ink * 0.6], WASH, 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unseen_cells_stay_blank_and_seen_ones_are_drawn() {
        let level = fb_level::generate(1207);
        let px = 16.0;
        let frame = PlanFrame::of(px);
        let blank = draw_plan(&level, &Explored::new(&level), px);
        assert!((0..blank.height).all(|y| (0..blank.width).all(|x| blank.alpha(x, y) == 0.0)), "nothing seen, nothing drawn");

        let all = draw_plan(&level, &Explored::all(&level), px);
        assert_eq!((all.width, all.height), frame.size(&level));
        let room = level.rooms[0];
        let middle = frame.at(room.x as f32 + room.w as f32 * 0.5, room.y as f32 + room.h as f32 * 0.5);
        assert!(all.alpha(middle[0] as u32, middle[1] as u32) > 0.8, "a room's floor is washed");
        // Its west wall is inked a little brighter than the wash.
        let wall = frame.at(room.x as f32, room.y as f32 + room.h as f32 * 0.5);
        assert!(all.alpha(wall[0] as u32, wall[1] as u32) > 0.8);
    }

    #[test]
    fn the_same_sight_draws_the_same_plan() {
        let level = fb_level::generate(7);
        let explored = Explored::all(&level);
        assert_eq!(draw_plan(&level, &explored, 12.0), draw_plan(&level, &explored, 12.0));
    }

    #[test]
    fn a_wobble_is_small_and_shared_by_lines_meeting_at_a_point() {
        for (x, y) in [(0.0, 0.0), (3.0, 7.0), (12.5, 1.0)] {
            let (wx, wy) = wobble(x, y, 3);
            assert!(wx.abs() <= 1.0 && wy.abs() <= 1.0);
            assert_eq!(wobble(x, y, 3), (wx, wy));
        }
    }
}
