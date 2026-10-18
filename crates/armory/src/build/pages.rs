//! A page block: a stack of separate leaves, each a thin sheet with its own edge, so the
//! fore-edge, head and tail show fine ridges and the leaves sit slightly uneven. Books build
//! theirs here; the leaves follow any surface the caller gives them (flat when closed, dipping
//! to the gutter and fanned when open).
//!
//! The block is built in book space: the spine runs along X, the leaves spread across Z
//! (`side` picks +Z or -Z from the spine at z = 0) and stack up Y.

use crate::geom::{GridOptions, MeshBuilder, Shading, V3, thick_sheet, value_noise3};

/// The measurements of a stack of leaves.
#[derive(Clone, Copy, Debug)]
pub struct LeafStack {
    pub count: usize,
    /// How thick each leaf is.
    pub thickness: f32,
    /// Half the leaves' length along the spine.
    pub half_len: f32,
    /// Where the leaves start, measured from the spine across the page.
    pub u0: f32,
    /// Samples along the leaf's width, and along its length.
    pub cols: usize,
    pub rows: usize,
    /// 0..=1: how unevenly the leaves sit (edges trimmed short, a little height).
    pub uneven: f32,
    pub seed: u32,
}

/// The stack as meshes: for leaf `i`, `reach(i)` is how far from the spine it runs (before it
/// is trimmed) and `shape(i, u, x)` is the height of its top face at distance `u` from the
/// spine and `x` along it. Faces look up; the leaf is `thickness` thick below its top.
pub fn leaf_stack(spec: &LeafStack, side: f32, reach: &dyn Fn(usize) -> f32, shape: &dyn Fn(usize, f32, f32) -> f32) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    let cols = spec.cols.max(2);
    let rows = spec.rows.max(2);
    for i in 0..spec.count {
        let f = i as f32;
        let n = |k: f32| value_noise3([f * 0.83 + k, k * 1.7, 0.4], spec.seed ^ 0x000b_00c5);
        let trim_len = spec.uneven * 0.012 * n(0.0);
        let bite = if n(7.0) > 0.82 { 0.02 } else { 0.0 };
        let trim_u = spec.uneven * (0.022 * n(3.0) + bite);
        let dy = spec.uneven * 0.0015 * (n(5.0) - 0.5);
        let half = (spec.half_len - trim_len).max(0.05);
        let u1 = (reach(i) - trim_u).max(spec.u0 + 0.05);
        let mut grid: Vec<Vec<V3>> = (0..rows)
            .map(|r| {
                let x = -half + 2.0 * half * r as f32 / (rows - 1) as f32;
                (0..cols)
                    .map(|c| {
                        let u = spec.u0 + (u1 - spec.u0) * c as f32 / (cols - 1) as f32;
                        [x, shape(i, u, x) + dy, side * u]
                    })
                    .collect()
            })
            .collect();
        // Columns must run toward +Z for the leaf's face to look up.
        if side < 0.0 {
            grid.iter_mut().for_each(|row| row.reverse());
        }
        out.append(&thick_sheet(&grid, &GridOptions { shading: Shading::SMOOTH, ..Default::default() }, spec.thickness));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    #[test]
    fn leaves_face_up_on_both_sides_and_are_drawable() {
        let spec = LeafStack { count: 6, thickness: 0.012, half_len: 0.8, u0: 0.0, cols: 4, rows: 3, uneven: 1.0, seed: 3 };
        for side in [1.0, -1.0] {
            let m = leaf_stack(&spec, side, &|_| 1.0, &|i, _, _| 0.1 + 0.015 * i as f32);
            let top = m.positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
            let up = m.positions.iter().zip(&m.normals).filter(|(p, _)| p[1] > top - 0.03);
            assert!(up.clone().any(|(_, n)| n[1] > 0.9), "top leaf faces down");
            m.into_part("pages", OutlineMode::Welded).validate().unwrap();
        }
    }
}
