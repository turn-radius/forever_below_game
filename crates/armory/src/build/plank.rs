//! Boards: single planks with eased edges, rows of boards with gaps between them, and round
//! heads made of boards. Crates, barrels, chests, and later doors, shields and tables.

use crate::geom::{MeshBuilder, P2, extrude};

/// A board `length` along X, `width` along Y and `thickness` along Z, centred on the origin.
/// Its four long edges are chamfered by `bevel` (clamped to a third of the thinner side); the
/// ends are sawn square.
pub fn plank(length: f32, width: f32, thickness: f32, bevel: f32) -> MeshBuilder {
    let (t, w) = (thickness * 0.5, width * 0.5);
    let e = bevel.clamp(0.0, thickness.min(width) / 3.0);
    // Cross-section with x through the thickness and y across the width.
    let section: Vec<P2> = if e > 1e-6 {
        vec![[-t + e, -w], [t - e, -w], [t, -w + e], [t, w - e], [t - e, w], [-t + e, w], [-t, w - e], [-t, -w + e]]
    } else {
        vec![[-t, -w], [t, -w], [t, w], [-t, w]]
    };
    let mut board = extrude(&section, &[], length);
    // The extrusion runs along Z with the thickness along X: turn it to run along X.
    board.rotate_y(std::f32::consts::FRAC_PI_2);
    board
}

/// `boards` planks side by side filling `length` (X) by `height` (Y), `gap` apart, each
/// `thickness` thick (Z) and centred on z = 0: a planked wall or lid.
pub fn planked_panel(length: f32, height: f32, boards: u32, gap: f32, thickness: f32, bevel: f32) -> MeshBuilder {
    let n = boards.max(1);
    let width = ((height - gap * (n - 1) as f32) / n as f32).max(1e-3);
    let mut mesh = MeshBuilder::new();
    for i in 0..n {
        let mut board = plank(length, width, thickness, bevel);
        board.translate([0.0, -height * 0.5 + width * 0.5 + i as f32 * (width + gap), 0.0]);
        mesh.append(&board);
    }
    mesh
}

/// A round head of `boards` planks, `radius` across and `thickness` thick, lying flat in the
/// XZ plane about the origin with boards running along X (a barrel head, a round lid).
pub fn planked_disc(radius: f32, boards: u32, gap: f32, thickness: f32) -> MeshBuilder {
    let n = boards.max(1);
    let r = radius.max(1e-4);
    let width = (2.0 * r - gap * (n - 1) as f32) / n as f32;
    let mut mesh = MeshBuilder::new();
    for i in 0..n {
        let z0 = -r + i as f32 * (width + gap);
        let z1 = (z0 + width).min(r);
        let half_x = |z: f32| (r * r - z * z).max(0.0).sqrt();
        // The board's outline in (x, z): its straight sides along the strip and its ends on the circle.
        let steps = 6;
        let mut outline: Vec<P2> = Vec::new();
        for k in 0..=steps {
            let z = z0 + (z1 - z0) * k as f32 / steps as f32;
            outline.push([half_x(z), z]);
        }
        for k in (0..=steps).rev() {
            let z = z0 + (z1 - z0) * k as f32 / steps as f32;
            outline.push([-half_x(z), z]);
        }
        outline.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6);
        if outline.len() < 3 {
            continue;
        }
        let mut board = extrude(&outline, &[], thickness);
        // Outline y becomes world z, the extrusion's z becomes world y.
        board.rotate_x(std::f32::consts::FRAC_PI_2);
        mesh.append(&board);
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    #[test]
    fn boards_have_the_size_asked_for() {
        let (lo, hi) = plank(2.0, 0.4, 0.1, 0.02).bounds().unwrap();
        for (k, half) in [(0, 1.0), (1, 0.2), (2, 0.05)] {
            assert!((hi[k] - half).abs() < 1e-5 && (lo[k] + half).abs() < 1e-5, "axis {k}: {lo:?} {hi:?}");
        }
        let (lo, hi) = planked_panel(3.0, 2.0, 5, 0.03, 0.1, 0.01).bounds().unwrap();
        assert!((hi[1] - 1.0).abs() < 1e-4 && (lo[1] + 1.0).abs() < 1e-4);
        let disc = planked_disc(1.0, 4, 0.02, 0.1);
        let (lo, hi) = disc.bounds().unwrap();
        assert!(hi[0] <= 1.0 + 1e-5 && lo[2] >= -1.0 - 1e-5 && (hi[1] - 0.05).abs() < 1e-5, "{lo:?} {hi:?}");
        for mesh in [plank(2.0, 0.4, 0.1, 0.02), disc] {
            mesh.into_part("boards", OutlineMode::Welded).validate().unwrap();
        }
    }
}
