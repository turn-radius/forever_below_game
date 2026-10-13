//! Geometry shared by every family. Families build parts from these primitives:
//!
//! - [`MeshBuilder`]: append, merge and transform geometry, then finish into a [`Part`].
//! - [`lathe`]: revolve a radial profile (hafts, grips, pommels, bowls).
//! - [`sweep`] / [`tube`]: a cross-section along a path on rotation-minimizing frames
//!   (cords, wraps, prongs, curls, bow strings, rings), with [`CatmullRom`] and [`helix`] paths.
//! - [`extrude`]: a 2D outline with holes as a hard-edged slab (flanges, blades, shield faces).
//! - [`tessellate`]: a 2D outline with holes filled with small even triangles, ready to bend
//!   onto a surface (see [`crate::build::plate`]).
//! - [`parametric_surface`] / [`stitch_grid`]: a sampled (u, v) surface (cloth, garments).
//! - [`bevelled_slab`]: an outline with holes as a slab with chamfered or rounded rims and
//!   smooth curved walls (pendants, signets, devices, plaques).
//! - [`shapes`]: spheres, spikes, frustums, boxes, tori.
//! - [`value_noise3`]: seeded value noise for roughening surfaces.
//! - [`outline`]: the welded outline normals the ink pass needs (`docs/rendering.md`).
//! - [`lathe_sector`] / [`profile_face`] / [`rind_face`]: turned solids with a wedge cut out,
//!   and the cut faces with a band of rind or crust (cheeses, pies, loaves, fruit).
//! - [`shell_profile`] / [`clip_profile`]: walled vessels and the liquid inside them.
//! - [`carve_hollows`]: spherical hollows pressed into a surface (bites, gouges, dents).

pub mod builder;
pub mod carve;
pub mod grid;
pub mod lathe;
pub mod mesh;
pub mod noise;
pub mod outline;
pub mod polygon;
pub mod section;
pub mod shapes;
pub mod slab;
pub mod spline;
pub mod sweep;
pub mod tessellate;
pub mod vec3;

#[cfg(test)]
mod tests;

pub use builder::MeshBuilder;
pub use carve::{Hollow, carve_hollows};
pub use grid::{GridOptions, Shading, grid_normals, parametric_surface, stitch_grid, thick_sheet};
pub use lathe::{LatheOptions, clip_profile, lathe, lathe_fn, lathe_sector, lathe_warped, profile_height_at, profile_radius_at, shell_inside, shell_profile, spline_profile};
pub use mesh::{ItemMesh, Part};
pub use noise::value_noise3;
pub use outline::{OutlineMode, smooth_outline_normals, welded_outline_normals};
pub use polygon::{P2, distance_to_polygon, extrude, extrude_smooth, extrude_tapered, fillet, offset_ring, point_in_polygon, resample_closed, signed_area, triangulate};
pub use section::{inset_polyline, profile_face, rind_face};
pub use slab::{SlabOptions, bevelled_slab};
pub use shapes::{cuboid, cylinder, frustum, icosphere, spike, torus, uv_sphere};
pub use spline::{CatmullRom, Frame, helix, polyline_length, resample_by_length, rotation_minimizing_frames};
pub use sweep::{SweepOptions, regular_polygon, sweep, sweep_frames, sweep_scaled, tube, tube_varying};
pub use tessellate::{Tessellation, tessellate};
pub use vec3::{Mat3, V3};
