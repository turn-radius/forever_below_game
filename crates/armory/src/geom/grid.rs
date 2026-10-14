//! Stitching a grid of points into a surface. Lathes, sweeps and parametric surfaces all
//! come down to rows of points; this is where their triangles and normals are made.
//!
//! The grid is `points[row][column]`. Columns run **around** (u) and rows run **along** (v).
//! The front of the surface faces `du x dv`: with u to the right and v up, the front faces you.

use std::collections::HashMap;

use super::builder::MeshBuilder;
use super::vec3::{self, V3};

/// Where a surface's shading normals are smooth and where its facets show hard edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shading {
    /// Normals blend across the edges between columns (round, not faceted, around).
    pub smooth_around: bool,
    /// Normals blend across the edges between rows (no hard rings along).
    pub smooth_along: bool,
}

impl Shading {
    /// Smooth everywhere (a turned handle, a grip, a sphere).
    pub const SMOOTH: Shading = Shading { smooth_around: true, smooth_along: true };
    /// Every facet flat (a cut gem, a chiselled stone head).
    pub const FLAT: Shading = Shading { smooth_around: false, smooth_along: false };
    /// Hard edges around, smooth along: flat sides that curve lengthwise (an octagonal haft
    /// with a swelling, a flanged head).
    pub const FACETED: Shading = Shading { smooth_around: false, smooth_along: true };
    /// Smooth around, hard edges along: round bands with crisp steps (turned rings, mouldings).
    pub const BANDED: Shading = Shading { smooth_around: true, smooth_along: false };
}

impl Default for Shading {
    fn default() -> Self {
        Shading::SMOOTH
    }
}

/// How [`stitch_grid`] closes and shades a grid.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridOptions {
    /// The last column joins back to the first (a tube or a lathe).
    pub wrap_around: bool,
    /// The last row joins back to the first (a torus, a closed loop).
    pub wrap_along: bool,
    pub shading: Shading,
    /// The v texture coordinate of each row; by default the mean distance along the rows,
    /// normalized to 0..=1.
    pub row_v: Option<Vec<f32>>,
}

/// The front-facing, area-weighted normal of one grid cell (twice its area long).
fn cell_normal(points: &[Vec<V3>], r0: usize, r1: usize, c0: usize, c1: usize) -> V3 {
    let p00 = points[r0][c0];
    let p01 = points[r0][c1];
    let p10 = points[r1][c0];
    let p11 = points[r1][c1];
    // (p01 - p10) x (p11 - p00) = 2 (du x dv) for a parallelogram, and stays valid when one
    // edge has collapsed to a pole.
    vec3::cross(vec3::sub(p01, p10), vec3::sub(p11, p00))
}

fn is_collapsed(row: &[V3]) -> bool {
    let first = row[0];
    row.iter().all(|&p| vec3::distance(p, first) < 1e-6)
}

/// The default v coordinate per row: mean arc length along the columns, normalized.
fn default_row_v(points: &[Vec<V3>], wrap_along: bool) -> Vec<f32> {
    let rows = points.len();
    let cols = points[0].len();
    let mut v = vec![0.0f32; rows + usize::from(wrap_along)];
    for r in 1..v.len() {
        let (a, b) = (&points[r - 1], &points[r % rows]);
        let mean = (0..cols).map(|c| vec3::distance(a[c], b[c])).sum::<f32>() / cols as f32;
        v[r] = v[r - 1] + mean;
    }
    let total = *v.last().unwrap_or(&0.0);
    if total > 1e-9 {
        for x in &mut v {
            *x /= total;
        }
    } else {
        let n = (v.len() - 1).max(1) as f32;
        for (i, x) in v.iter_mut().enumerate() {
            *x = i as f32 / n;
        }
    }
    v
}

/// Turns `points[row][column]` into triangles with normals and uvs per [`GridOptions`].
///
/// Hard edges get split vertices; smooth ones share them. Cells whose corners have collapsed
/// (the rows at a lathe's poles) lose their degenerate triangles, and a fully collapsed row
/// under smooth-around shading takes the average of every cell around it, so poles shade
/// cleanly with no NaN. Every row must have the same number of points.
pub fn stitch_grid(points: &[Vec<V3>], options: &GridOptions) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    let rows = points.len();
    if rows == 0 {
        return out;
    }
    let cols = points[0].len();
    assert!(points.iter().all(|r| r.len() == cols), "stitch_grid: rows must have equal length");
    if cols < 2 || rows < 2 && !options.wrap_along {
        return out;
    }
    let shading = options.shading;
    let cell_cols = if options.wrap_around { cols } else { cols - 1 };
    let cell_rows = if options.wrap_along { rows } else { rows - 1 };
    let row_v = options.row_v.clone().unwrap_or_else(|| default_row_v(points, options.wrap_along));
    let col_u = |c: usize| c as f32 / cell_cols as f32;
    let collapsed: Vec<bool> = points.iter().map(|r| is_collapsed(r)).collect();

    let cell = |i: usize, j: usize| cell_normal(points, i, (i + 1) % rows, j, (j + 1) % cols);
    // The cells on either side of grid line `k`, given `n` cells and `wrap`.
    let neighbours = |k: usize, n: usize, wrap: bool| -> Vec<usize> {
        let mut v = Vec::with_capacity(2);
        if k > 0 {
            v.push(k - 1);
        } else if wrap {
            v.push(n - 1);
        }
        if k < n {
            v.push(k);
        } else if wrap {
            v.push(0);
        }
        v.dedup();
        v
    };

    // Shading normal of corner (row line a, column line b) as seen from cell (i, j).
    let corner_normal = |a: usize, b: usize, i: usize, j: usize| -> V3 {
        let cell_rows_used = if shading.smooth_along { neighbours(a, cell_rows, options.wrap_along) } else { vec![i] };
        let cell_cols_used: Vec<usize> = if !shading.smooth_around {
            vec![j]
        } else if collapsed[a % rows] {
            (0..cell_cols).collect()
        } else {
            neighbours(b, cell_cols, options.wrap_around)
        };
        let mut sum = vec3::ZERO;
        for &ci in &cell_rows_used {
            for &cj in &cell_cols_used {
                sum = vec3::add(sum, cell(ci, cj));
            }
        }
        vec3::try_normalize(sum).or_else(|| vec3::try_normalize(cell(i, j))).unwrap_or(vec3::Y)
    };

    // Vertex identity: row line, column line, plus the cell when that direction is hard.
    let mut ids: HashMap<(usize, usize, usize, usize), u32> = HashMap::new();
    let mut vertex = |out: &mut MeshBuilder, a: usize, b: usize, i: usize, j: usize| -> u32 {
        let key = (a, b, if shading.smooth_along { usize::MAX } else { i }, if shading.smooth_around { usize::MAX } else { j });
        *ids.entry(key).or_insert_with(|| out.push_vertex(points[a % rows][b % cols], corner_normal(a, b, i, j), [col_u(b), row_v[a]]))
    };

    for i in 0..cell_rows {
        for j in 0..cell_cols {
            let (a0, a1, b0, b1) = (i, i + 1, j, j + 1);
            let p = |a: usize, b: usize| points[a % rows][b % cols];
            for (x, y, z) in [((a0, b0), (a0, b1), (a1, b0)), ((a1, b0), (a0, b1), (a1, b1))] {
                let face = vec3::cross(vec3::sub(p(y.0, y.1), p(x.0, x.1)), vec3::sub(p(z.0, z.1), p(x.0, x.1)));
                if vec3::length_squared(face) < 1e-14 {
                    continue;
                }
                let ia = vertex(&mut out, x.0, x.1, i, j);
                let ib = vertex(&mut out, y.0, y.1, i, j);
                let ic = vertex(&mut out, z.0, z.1, i, j);
                out.push_triangle(ia, ib, ic);
            }
        }
    }
    out
}

/// Samples a parametric surface `f(u, v)` over `0..=1` squared on a grid and stitches it.
/// With `wrap_around`, u stops short of 1 and the last column joins the first (likewise v
/// with `wrap_along`). Front faces point along `df/du x df/dv`.
pub fn parametric_surface(u_segments: usize, v_segments: usize, options: &GridOptions, f: impl Fn(f32, f32) -> V3) -> MeshBuilder {
    let u_segments = u_segments.max(1);
    let v_segments = v_segments.max(1);
    let cols = if options.wrap_around { u_segments } else { u_segments + 1 };
    let rows = if options.wrap_along { v_segments } else { v_segments + 1 };
    let points: Vec<Vec<V3>> = (0..rows)
        .map(|r| {
            let v = r as f32 / v_segments as f32;
            (0..cols).map(|c| f(c as f32 / u_segments as f32, v)).collect()
        })
        .collect();
    let mut opts = options.clone();
    if opts.row_v.is_none() {
        opts.row_v = Some((0..=v_segments).map(|r| r as f32 / v_segments as f32).collect());
    }
    stitch_grid(&points, &opts)
}
