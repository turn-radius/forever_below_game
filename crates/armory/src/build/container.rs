//! Where a container's contents go: its opening and the space inside, in item space.
//!
//! Containers keep their contents in the game (`CLAUDE.md`, "Game decisions already made"),
//! so the game needs more than a mesh: where to drop an item in, where to draw the contents
//! of an open chest, how much fits. Families that hold things (bags, chests) return a
//! [`ContainerSpace`] from their own `container(&state)` alongside `build`.

use crate::geom::V3;
use crate::geom::vec3;
use serde::{Deserialize, Serialize};

/// The mouth of a container: a flat patch things pass through.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Opening {
    /// Centre of the mouth.
    pub center: V3,
    /// Unit vector pointing out of the container through the mouth.
    pub normal: V3,
    /// Unit vector in the mouth's plane along its first half extent (its width).
    pub across: V3,
    /// Half the mouth's size along `across` and along `normal x across`. Equal for a round mouth.
    pub half_extents: [f32; 2],
    /// Round (a pouch's neck, a barrel's head) rather than rectangular (a chest).
    pub round: bool,
}

impl Opening {
    /// A mouth facing straight up (+Y) with its width along X.
    pub fn upward(center: V3, half_extents: [f32; 2], round: bool) -> Opening {
        Opening { center, normal: vec3::Y, across: vec3::X, half_extents, round }
    }
}

/// The space inside a container.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContainerSpace {
    pub opening: Opening,
    /// Axis-aligned bounds of the usable inside: contents drawn in an open container stay in it.
    pub interior_min: V3,
    pub interior_max: V3,
    /// How much it holds, in litres (1 cubic unit is 1 litre at 10 cm per unit).
    pub litres: f32,
}

impl ContainerSpace {
    /// Size of the interior bounds along each axis.
    pub fn interior_size(&self) -> V3 {
        vec3::sub(self.interior_max, self.interior_min)
    }

    /// Whether `p` lies inside the interior bounds.
    pub fn contains(&self, p: V3) -> bool {
        (0..3).all(|k| p[k] >= self.interior_min[k] && p[k] <= self.interior_max[k])
    }
}

/// The area a closed loop of points encloses, seen from above (its shadow on XZ).
pub fn plan_area(ring: &[V3]) -> f32 {
    let n = ring.len();
    ((0..n).map(|i| ring[i][0] * ring[(i + 1) % n][2] - ring[(i + 1) % n][0] * ring[i][2]).sum::<f32>() * 0.5).abs()
}

/// The volume enclosed by a stack of horizontal loops (the rows of a vessel's wall, bottom to
/// top), by the trapezoid rule over their plan areas. Rows above `top` are left out, and a
/// row straddling it is cut there.
pub fn stacked_volume(rows: &[Vec<V3>], top: f32) -> f32 {
    let layers: Vec<(f32, f32)> = rows.iter().filter(|r| !r.is_empty()).map(|r| (r[0][1], plan_area(r))).collect();
    layers
        .windows(2)
        .map(|w| {
            let ((y0, a0), (y1, a1)) = (w[0], w[1]);
            let y1c = y1.min(top);
            if y1c <= y0 || y1 <= y0 {
                return 0.0;
            }
            let a1c = a0 + (a1 - a0) * (y1c - y0) / (y1 - y0);
            (a0 + a1c) * 0.5 * (y1c - y0)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacked_volume_of_a_box() {
        let square = |y: f32| vec![[-1.0, y, -1.0], [1.0, y, -1.0], [1.0, y, 1.0], [-1.0, y, 1.0]];
        let rows = [square(0.0), square(1.0), square(2.0)];
        assert!((stacked_volume(&rows, 10.0) - 8.0).abs() < 1e-4);
        assert!((stacked_volume(&rows, 1.5) - 6.0).abs() < 1e-4);
    }
}
