//! Outline normals for the inverted-hull ink pass (`docs/rendering.md`, "Ink outline").
//!
//! The hull pushes each vertex out along its outline normal. On faceted parts the shading
//! normals split at every hard edge, so the hull would crack open there. Welding vertices by
//! position and averaging the faces around each position keeps the hull closed.

use std::collections::HashMap;

use super::vec3::{self, V3};

/// How a finished part gets its outline normals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutlineMode {
    /// Weld by position and average the area-weighted face normals ([`welded_outline_normals`]).
    /// Right for anything with hard edges.
    #[default]
    Welded,
    /// Copy the shading normals ([`smooth_outline_normals`]). Right for surfaces that are
    /// smooth everywhere and have no seams that split position from normal.
    Shading,
}

/// Positions rounded to this grid count as the same point when welding.
pub const WELD_EPSILON: f32 = 1e-4;

fn weld_key(p: V3) -> [i64; 3] {
    p.map(|c| (c / WELD_EPSILON).round() as i64)
}

/// One outline normal per vertex: vertices that share a position (rounded to
/// [`WELD_EPSILON`]) share the area-weighted average of the faces around that position.
/// Where the faces cancel out (the two sides of a sheet), the vertex keeps its shading normal.
pub fn welded_outline_normals(positions: &[V3], normals: &[V3], indices: &[u32]) -> Vec<V3> {
    let mut group_of_key: HashMap<[i64; 3], usize> = HashMap::new();
    let group: Vec<usize> = positions
        .iter()
        .map(|&p| {
            let next = group_of_key.len();
            *group_of_key.entry(weld_key(p)).or_insert(next)
        })
        .collect();
    let mut sums = vec![vec3::ZERO; group_of_key.len()];
    let mut weights = vec![0.0f32; group_of_key.len()];
    for tri in indices.as_chunks::<3>().0 {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| i as usize);
        // Unnormalized cross product: its length is twice the triangle's area.
        let face = vec3::cross(vec3::sub(positions[b], positions[a]), vec3::sub(positions[c], positions[a]));
        let weight = vec3::length(face);
        for v in [a, b, c] {
            sums[group[v]] = vec3::add(sums[group[v]], face);
            weights[group[v]] += weight;
        }
    }
    group
        .iter()
        .enumerate()
        .map(|(v, &g)| {
            let shading = normals.get(v).copied().and_then(vec3::try_normalize).unwrap_or(vec3::Y);
            // Faces that cancel (a double-sided sheet) leave only noise; keep the shading normal.
            if vec3::length(sums[g]) > 1e-3 * weights[g] {
                vec3::normalize_or(sums[g], shading)
            } else {
                shading
            }
        })
        .collect()
}

/// Outline normals for an already smooth surface: a copy of the shading normals.
pub fn smooth_outline_normals(normals: &[V3]) -> Vec<V3> {
    normals.to_vec()
}
