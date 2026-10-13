//! Geometry shared by every family. Families build parts from these primitives:
//!
//! - [`MeshBuilder`]: append, merge and transform geometry, then finish into a [`Part`].
//! - [`lathe`]: revolve a radial profile (hafts, grips, pommels, bowls).
//! - [`sweep`] / [`tube`]: a cross-section along a path on rotation-minimizing frames
//!   (cords, wraps, prongs, curls, bow strings, rings), with [`CatmullRom`] and [`helix`] paths.
//! - [`extrude`]: a 2D outline with holes as a hard-edged slab (flanges, blades, shield faces).
//! - [`parametric_surface`] / [`stitch_grid`]: a sampled (u, v) surface (cloth, garments).
//! - [`shapes`]: spheres, spikes, frustums, boxes, tori.
//! - [`outline`]: the welded outline normals the ink pass needs (`docs/rendering.md`).

pub mod builder;
pub mod grid;
pub mod lathe;
pub mod mesh;
pub mod outline;
pub mod polygon;
pub mod shapes;
pub mod spline;
pub mod sweep;
pub mod vec3;

#[cfg(test)]
mod tests;

pub use builder::MeshBuilder;
pub use grid::{GridOptions, Shading, parametric_surface, stitch_grid};
pub use lathe::{LatheOptions, lathe, lathe_fn};
pub use mesh::{ItemMesh, Part};
pub use outline::{OutlineMode, smooth_outline_normals, welded_outline_normals};
pub use polygon::{extrude, signed_area, triangulate};
pub use shapes::{cuboid, cylinder, frustum, icosphere, spike, torus, uv_sphere};
pub use spline::{CatmullRom, Frame, helix, polyline_length, resample_by_length, rotation_minimizing_frames};
pub use sweep::{SweepOptions, regular_polygon, sweep, tube};
pub use vec3::{Mat3, V3};
