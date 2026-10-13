//! Dense triangulation of a 2D polygon with holes, for surfaces that bend afterwards.
//!
//! [`super::triangulate`] uses only the outline's own points, so a large face comes out as a
//! fan of long thin triangles. Projected onto a dome or a dished shield, those triangles cut
//! chords through the curve. [`tessellate`] fills the interior with a regular grid of cells
//! instead, and ear clipping only stitches the narrow band between the grid and the outline.
//! Every edge then stays within about one cell, wherever the polygon is later mapped.

use std::collections::BTreeMap;

use super::polygon::{P2, in_triangle, point_in_polygon, signed_area, turn};

/// A polygon filled with small triangles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tessellation {
    pub points: Vec<P2>,
    /// Counter-clockwise triangles over `points`.
    pub triangles: Vec<[u32; 3]>,
    /// The resampled outline (counter-clockwise), then each resampled hole (clockwise), as
    /// indices into `points`: walking any loop, the solid is on the left.
    pub boundary: Vec<Vec<u32>>,
}

/// Fills `outline` minus `holes` with triangles about one `cell` across.
///
/// Grid lines run through `origin` every `cell[0]` in x and `cell[1]` in y, so a caller can
/// line them up with creases in whatever surface the result is projected onto. Outline and
/// hole edges are split so no piece is longer than a cell. Either winding is accepted. Holes
/// must lie inside the outline and not touch it or each other.
pub fn tessellate(outline: &[P2], holes: &[Vec<P2>], cell: [f32; 2], origin: P2) -> Tessellation {
    let cell = [cell[0].abs().max(1e-5), cell[1].abs().max(1e-5)];
    let to_grid = |p: P2| [(p[0] - origin[0]) / cell[0], (p[1] - origin[1]) / cell[1]];
    let from_grid = |q: P2| [origin[0] + q[0] * cell[0], origin[1] + q[1] * cell[1]];

    let mut out = Tessellation::default();
    let outline = resample(&oriented(&outline.iter().map(|&p| to_grid(p)).collect::<Vec<_>>(), true));
    if outline.len() < 3 {
        return out;
    }
    let holes: Vec<Vec<P2>> = holes
        .iter()
        .map(|h| resample(&oriented(&h.iter().map(|&p| to_grid(p)).collect::<Vec<_>>(), false)))
        .filter(|h| h.len() >= 3)
        .collect();

    // Boundary points first, so the boundary loops index straight into `points`.
    let mut points: Vec<P2> = Vec::new();
    let mut loop_ids: Vec<Vec<u32>> = Vec::new();
    for ring in std::iter::once(&outline).chain(&holes) {
        let start = points.len() as u32;
        points.extend_from_slice(ring);
        loop_ids.push((start..points.len() as u32).collect());
    }

    // Cells clear of every boundary edge by half a cell, then inside by a scanline.
    let (lo, hi) = points.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), q| {
        ([lo[0].min(q[0]), lo[1].min(q[1])], [hi[0].max(q[0]), hi[1].max(q[1])])
    });
    let (i0, j0) = (lo[0].floor() as i32 - 1, lo[1].floor() as i32 - 1);
    let (ni, nj) = ((hi[0].ceil() as i32 + 1 - i0) as usize, (hi[1].ceil() as i32 + 1 - j0) as usize);
    let mut blocked = vec![false; ni * nj];
    let rings: Vec<&Vec<P2>> = std::iter::once(&outline).chain(&holes).collect();
    for ring in &rings {
        for k in 0..ring.len() {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            let (x0, x1) = (a[0].min(b[0]) - 0.5, a[0].max(b[0]) + 0.5);
            let (y0, y1) = (a[1].min(b[1]) - 0.5, a[1].max(b[1]) + 0.5);
            for j in ((y0.floor() as i32 - j0).max(0))..((y1.ceil() as i32 - j0).min(nj as i32)) {
                for i in ((x0.floor() as i32 - i0).max(0))..((x1.ceil() as i32 - i0).min(ni as i32)) {
                    blocked[j as usize * ni + i as usize] = true;
                }
            }
        }
    }
    let mut inside = vec![false; ni * nj];
    for j in 0..nj {
        let y = (j as i32 + j0) as f32 + 0.5;
        let mut xs: Vec<f32> = Vec::new();
        for ring in &rings {
            for k in 0..ring.len() {
                let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
                if (a[1] > y) != (b[1] > y) {
                    xs.push(a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]));
                }
            }
        }
        xs.sort_by(f32::total_cmp);
        for pair in xs.as_chunks::<2>().0 {
            for i in 0..ni {
                let x = (i as i32 + i0) as f32 + 0.5;
                if x > pair[0] && x < pair[1] && !blocked[j * ni + i] {
                    inside[j * ni + i] = true;
                }
            }
        }
    }
    let at = |inside: &[bool], i: i32, j: i32| i >= 0 && j >= 0 && (i as usize) < ni && (j as usize) < nj && inside[j as usize * ni + i as usize];

    // Cells that meet only at a corner would make a boundary loop touch itself: drop one.
    loop {
        let mut changed = false;
        for j in 0..nj as i32 - 1 {
            for i in 0..ni as i32 - 1 {
                let (a, b, c, d) = (at(&inside, i, j), at(&inside, i + 1, j), at(&inside, i, j + 1), at(&inside, i + 1, j + 1));
                if (a && d && !b && !c) || (b && c && !a && !d) {
                    let (di, dj) = if a && d { (i + 1, j + 1) } else { (i + 1, j) };
                    inside[dj as usize * ni + di as usize] = false;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    // Grid vertices and the cells' own triangles.
    let mut grid_ids: BTreeMap<(i32, i32), u32> = BTreeMap::new();
    let mut grid_id = |points: &mut Vec<P2>, i: i32, j: i32| -> u32 {
        *grid_ids.entry((i, j)).or_insert_with(|| {
            points.push([i as f32, j as f32]);
            points.len() as u32 - 1
        })
    };
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    // Directed boundary edges of the cell region, region on the left: start -> end.
    let mut edges: BTreeMap<(i32, i32), (i32, i32)> = BTreeMap::new();
    for j in 0..nj as i32 {
        for i in 0..ni as i32 {
            if !at(&inside, i, j) {
                continue;
            }
            let (gi, gj) = (i + i0, j + j0);
            let a = grid_id(&mut points, gi, gj);
            let b = grid_id(&mut points, gi + 1, gj);
            let c = grid_id(&mut points, gi + 1, gj + 1);
            let d = grid_id(&mut points, gi, gj + 1);
            triangles.push([a, b, c]);
            triangles.push([a, c, d]);
            if !at(&inside, i, j - 1) {
                edges.insert((gi, gj), (gi + 1, gj));
            }
            if !at(&inside, i + 1, j) {
                edges.insert((gi + 1, gj), (gi + 1, gj + 1));
            }
            if !at(&inside, i, j + 1) {
                edges.insert((gi + 1, gj + 1), (gi, gj + 1));
            }
            if !at(&inside, i - 1, j) {
                edges.insert((gi, gj + 1), (gi, gj));
            }
        }
    }

    // Chain the edges into loops: outer loops run counter-clockwise, loops round gaps clockwise.
    let mut cell_loops: Vec<Vec<u32>> = Vec::new();
    while let Some((&start, _)) = edges.iter().next() {
        let mut ids = Vec::new();
        let mut v = start;
        while let Some(next) = edges.remove(&v) {
            ids.push(grid_ids[&v]);
            v = next;
        }
        if ids.len() >= 3 {
            cell_loops.push(ids);
        }
    }
    let coords = |ids: &[u32], points: &[P2]| ids.iter().map(|&k| points[k as usize]).collect::<Vec<P2>>();
    let (outer, gaps): (Vec<Vec<u32>>, Vec<Vec<u32>>) =
        cell_loops.into_iter().partition(|ids| signed_area(&coords(ids, &points)) > 0.0);

    // Band regions: the outline round everything, and each gap in the cells round what it holds.
    let gap_polys: Vec<Vec<P2>> = gaps.iter().map(|g| coords(g, &points)).collect();
    let smallest_gap_holding = |p: P2| -> Option<usize> {
        (0..gaps.len())
            .filter(|&g| point_in_polygon(p, &gap_polys[g]))
            .min_by(|&a, &b| signed_area(&gap_polys[a]).abs().total_cmp(&signed_area(&gap_polys[b]).abs()))
    };
    let mut region_holes: Vec<Vec<Vec<u32>>> = vec![Vec::new(); gaps.len() + 1];
    for ids in loop_ids.iter().skip(1).chain(&outer) {
        let p = points[ids[0] as usize];
        let region = smallest_gap_holding(p).map_or(0, |g| g + 1);
        region_holes[region].push(ids.clone());
    }
    let regions = std::iter::once(&loop_ids[0]).chain(&gaps).zip(region_holes);
    for (ring, inner) in regions {
        let ring_pts = coords(ring, &points);
        let inner_pts: Vec<Vec<P2>> = inner.iter().map(|h| coords(h, &points)).collect();
        let lookup: Vec<u32> = ring.iter().chain(inner.iter().flatten()).copied().collect();
        for t in fill_band(&ring_pts, &inner_pts) {
            triangles.push(t.map(|k| lookup[k as usize]));
        }
    }

    out.points = points.into_iter().map(from_grid).collect();
    out.triangles = triangles;
    out.boundary = loop_ids;
    out
}

/// [`super::triangulate`] for the band between the outline and the cells, clipping the ear
/// with the shortest new edge each time so the triangles stay small and even.
fn fill_band(outline: &[P2], holes: &[Vec<P2>]) -> Vec<[u32; 3]> {
    let mut pts: Vec<P2> = outline.to_vec();
    let mut ring: Vec<u32> = (0..outline.len() as u32).collect();
    if signed_area(outline) < 0.0 {
        ring.reverse();
    }
    let mut hole_rings: Vec<Vec<u32>> = Vec::new();
    for hole in holes {
        let base = pts.len() as u32;
        pts.extend_from_slice(hole);
        let mut h: Vec<u32> = (base..base + hole.len() as u32).collect();
        if signed_area(hole) > 0.0 {
            h.reverse();
        }
        hole_rings.push(h);
    }
    let max_x = |h: &Vec<u32>| h.iter().map(|&i| pts[i as usize][0]).fold(f32::MIN, f32::max);
    hole_rings.sort_by(|a, b| max_x(b).total_cmp(&max_x(a)));
    for h in &hole_rings {
        ring = bridge(&ring, h, &pts);
    }

    let n = ring.len();
    let p = |node: usize| pts[ring[node] as usize];
    let mut prev: Vec<usize> = (0..n).map(|k| (k + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|k| (k + 1) % n).collect();
    let mut alive = vec![true; n];
    let ear = |node: usize, prev: &[usize], next: &[usize], alive: &[bool]| -> Option<f32> {
        let (a, b, c) = (p(prev[node]), p(node), p(next[node]));
        if turn(a, b, c) <= 1e-5 {
            return None;
        }
        let blocked = (0..n).any(|k| {
            let q = p(k);
            alive[k] && q != a && q != b && q != c && in_triangle(q, a, b, c)
        });
        (!blocked).then(|| (a[0] - c[0]).powi(2) + (a[1] - c[1]).powi(2))
    };
    let mut score: Vec<Option<f32>> = (0..n).map(|k| ear(k, &prev, &next, &alive)).collect();
    let mut tris = Vec::with_capacity(n);
    let mut left = n;
    while left > 3 {
        let best = (0..n).filter(|&k| alive[k]).filter_map(|k| score[k].map(|s| (k, s))).min_by(|x, y| x.1.total_cmp(&y.1));
        let node = match best {
            Some((k, _)) => k,
            None => {
                // Ears unblocked by earlier clips: look again, then force one if still stuck.
                for k in (0..n).filter(|&k| alive[k]) {
                    score[k] = ear(k, &prev, &next, &alive);
                }
                match (0..n).filter(|&k| alive[k]).filter_map(|k| score[k].map(|s| (k, s))).min_by(|x, y| x.1.total_cmp(&y.1)) {
                    Some((k, _)) => k,
                    None => {
                        let k = (0..n).find(|&k| alive[k]).unwrap_or(0);
                        alive[k] = false;
                        let (a, c) = (prev[k], next[k]);
                        next[a] = c;
                        prev[c] = a;
                        left -= 1;
                        continue;
                    }
                }
            }
        };
        let (a, c) = (prev[node], next[node]);
        tris.push([ring[a], ring[node], ring[c]]);
        alive[node] = false;
        next[a] = c;
        prev[c] = a;
        left -= 1;
        score[a] = ear(a, &prev, &next, &alive);
        score[c] = ear(c, &prev, &next, &alive);
    }
    if left == 3 {
        let k = (0..n).find(|&k| alive[k]).unwrap_or(0);
        let (a, c) = (prev[k], next[k]);
        if turn(p(a), p(k), p(c)) > 1e-5 {
            tris.push([ring[a], ring[k], ring[c]]);
        }
    }
    tris
}

/// Whether segments `ab` and `cd` cross or touch.
fn segments_meet(a: P2, b: P2, c: P2, d: P2) -> bool {
    let o = |p: P2, q: P2, r: P2| (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
    let (d1, d2, d3, d4) = (o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return true;
    }
    let on = |p: P2, q: P2, r: P2, side: f32| {
        side.abs() <= 1e-9 && r[0] >= p[0].min(q[0]) - 1e-9 && r[0] <= p[0].max(q[0]) + 1e-9 && r[1] >= p[1].min(q[1]) - 1e-9 && r[1] <= p[1].max(q[1]) + 1e-9
    };
    on(c, d, a, d1) || on(c, d, b, d2) || on(a, b, c, d3) || on(a, b, d, d4)
}

/// Whether direction `v -> m` leaves vertex `v` into the solid, which lies on the left of
/// `prev -> v -> next`.
fn in_cone(prev: P2, v: P2, next: P2, m: P2) -> bool {
    let (left_of_in, left_of_out) = (turn(prev, v, m) > 0.0, turn(v, next, m) > 0.0);
    if turn(prev, v, next) >= 0.0 { left_of_in && left_of_out } else { left_of_in || left_of_out }
}

/// Joins a clockwise `hole` into the counter-clockwise `ring` along the shortest edge that
/// crosses nothing: unlike a ray-cast bridge, this stays sound when points line up on a grid.
fn bridge(ring: &[u32], hole: &[u32], pts: &[P2]) -> Vec<u32> {
    let p = |i: u32| pts[i as usize];
    let edges: Vec<(P2, P2)> = [ring, hole]
        .iter()
        .flat_map(|r| (0..r.len()).map(move |k| (p(r[k]), p(r[(k + 1) % r.len()]))))
        .collect();
    let mut pairs: Vec<(f32, usize, usize)> = Vec::with_capacity(hole.len() * ring.len());
    for (hk, &hm) in hole.iter().enumerate() {
        for (rk, &rv) in ring.iter().enumerate() {
            let (m, v) = (p(hm), p(rv));
            pairs.push(((v[0] - m[0]).powi(2) + (v[1] - m[1]).powi(2), hk, rk));
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let best = pairs.into_iter().find(|&(d, hk, rk)| {
        let (m, v) = (p(hole[hk]), p(ring[rk]));
        let (hprev, hnext) = (p(hole[(hk + hole.len() - 1) % hole.len()]), p(hole[(hk + 1) % hole.len()]));
        let (rprev, rnext) = (p(ring[(rk + ring.len() - 1) % ring.len()]), p(ring[(rk + 1) % ring.len()]));
        d > 1e-12
            && in_cone(rprev, v, rnext, m)
            && in_cone(hprev, m, hnext, v)
            && !edges.iter().any(|&(a, b)| !(a == v || b == v || a == m || b == m) && segments_meet(m, v, a, b))
    });
    let Some((_, hk, rk)) = best else { return ring.to_vec() };
    let mut out = Vec::with_capacity(ring.len() + hole.len() + 2);
    out.extend_from_slice(&ring[..=rk]);
    out.extend((0..=hole.len()).map(|k| hole[(hk + k) % hole.len()]));
    out.push(ring[rk]);
    out.extend_from_slice(&ring[rk + 1..]);
    out
}

/// The ring in the wanted winding, with repeated points (and a repeated closing point) removed.
fn oriented(ring: &[P2], ccw: bool) -> Vec<P2> {
    let mut out: Vec<P2> = Vec::with_capacity(ring.len());
    for &p in ring {
        if out.last().is_none_or(|q: &P2| (p[0] - q[0]).hypot(p[1] - q[1]) > 1e-5) {
            out.push(p);
        }
    }
    while out.len() > 1 && {
        let (a, b) = (out[0], out[out.len() - 1]);
        (a[0] - b[0]).hypot(a[1] - b[1]) <= 1e-5
    } {
        out.pop();
    }
    if (signed_area(&out) > 0.0) != ccw {
        out.reverse();
    }
    out
}

/// Splits every edge of a closed ring into pieces no longer than one grid cell.
fn resample(ring: &[P2]) -> Vec<P2> {
    let mut out = Vec::with_capacity(ring.len() * 2);
    for k in 0..ring.len() {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        let pieces = ((b[0] - a[0]).hypot(b[1] - a[1]) / 0.9).ceil().max(1.0) as usize;
        for s in 0..pieces {
            let t = s as f32 / pieces as f32;
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(t: &Tessellation) -> f32 {
        t.triangles
            .iter()
            .map(|&[a, b, c]| {
                let (a, b, c) = (t.points[a as usize], t.points[b as usize], t.points[c as usize]);
                0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]))
            })
            .sum()
    }

    fn assert_ccw(t: &Tessellation) {
        for &[a, b, c] in &t.triangles {
            let (a, b, c) = (t.points[a as usize], t.points[b as usize], t.points[c as usize]);
            let x = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            assert!(x > 0.0, "{x} {a:?} {b:?} {c:?}");
        }
    }

    fn square(c: P2, r: f32) -> Vec<P2> {
        vec![[c[0] - r, c[1] - r], [c[0] + r, c[1] - r], [c[0] + r, c[1] + r], [c[0] - r, c[1] + r]]
    }

    #[test]
    fn fills_a_disc_with_small_triangles() {
        let disc: Vec<P2> = (0..64).map(|k| {
            let (s, c) = (std::f32::consts::TAU * k as f32 / 64.0).sin_cos();
            [3.0 * c, 3.0 * s]
        }).collect();
        let t = tessellate(&disc, &[], [0.2, 0.2], [0.0, 0.0]);
        assert_ccw(&t);
        assert!((area(&t) - signed_area(&disc)).abs() < 1e-3, "{} vs {}", area(&t), signed_area(&disc));
        for &[a, b, c] in &t.triangles {
            for (p, q) in [(a, b), (b, c), (c, a)] {
                let (p, q) = (t.points[p as usize], t.points[q as usize]);
                let l = (p[0] - q[0]).hypot(p[1] - q[1]); assert!(l < 0.75, "long edge {l} {p:?} {q:?}");
            }
        }
        assert_eq!(t.boundary.len(), 1);
    }

    #[test]
    fn holes_are_cut_out() {
        let outer = square([0.0, 0.0], 2.0);
        let holes = vec![square([-0.8, 0.0], 0.4), square([1.0, 1.0], 0.3)];
        let t = tessellate(&outer, &holes, [0.1, 0.15], [0.03, 0.0]);
        assert_ccw(&t);
        let expected = 16.0 - 0.64 - 0.36;
        assert!((area(&t) - expected).abs() < 1e-3, "{}", area(&t));
        assert_eq!(t.boundary.len(), 3);
        assert!(signed_area(&t.boundary[1].iter().map(|&k| t.points[k as usize]).collect::<Vec<_>>()) < 0.0);
    }

    #[test]
    fn narrow_and_concave_outlines_still_fill() {
        let comb = vec![[0.0, 0.0], [5.0, 0.0], [5.0, 3.0], [4.0, 3.0], [4.0, 0.5], [3.0, 0.5], [3.0, 3.0], [0.0, 3.0]];
        let t = tessellate(&comb, &[], [0.25, 0.25], [0.0, 0.0]);
        assert_ccw(&t);
        assert!((area(&t) - signed_area(&comb)).abs() < 1e-3, "{} {}", area(&t), signed_area(&comb));
        let strip = vec![[0.0, 0.0], [4.0, 0.0], [4.0, 0.1], [0.0, 0.1]];
        let t = tessellate(&strip, &[], [0.3, 0.3], [0.0, 0.0]);
        assert!((area(&t) - 0.4).abs() < 1e-4);
    }

    #[test]
    fn area_is_kept_across_cell_sizes_and_offsets() {
        let star: Vec<P2> = (0..14).map(|k| {
            let r = if k % 2 == 0 { 2.5 } else { 1.4 };
            let (s, c) = (std::f32::consts::TAU * k as f32 / 14.0).sin_cos();
            [r * c, r * s]
        }).collect();
        let holes = vec![square([0.0, 0.0], 0.5), vec![[1.0, 0.1], [1.3, 0.2], [1.05, 0.4]]];
        let expected = signed_area(&star) - 1.0 - signed_area(&holes[1]).abs();
        for k in 0..40 {
            let size = 0.05 + 0.02 * k as f32;
            let cell = [size, size * (0.7 + 0.03 * k as f32)];
            let t = tessellate(&star, &holes, cell, [0.013 * k as f32, -0.021 * k as f32]);
            assert_ccw(&t);
            for &[a, b, c] in &t.triangles {
                let (a, b, c) = (t.points[a as usize], t.points[b as usize], t.points[c as usize]);
                let m = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0];
                assert!(point_in_polygon(m, &star) && !holes.iter().any(|h| point_in_polygon(m, h)), "k {k} stray triangle {a:?} {b:?} {c:?}");
            }
            assert!((area(&t) - expected).abs() < 2e-3, "cell {cell:?}: {} vs {expected}", area(&t));
        }
    }
}
