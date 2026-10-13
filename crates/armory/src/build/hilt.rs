//! A hilted blade: a handle from [`super::hafted`] (grip, wrap, rivets, pommel), a guard from
//! [`super::guard`] and a blade from [`super::blade`], stacked along +Y. Swords and daggers
//! are built this way; anything else with a hilt (a falx, a short glaive) can be too.
//!
//! Item space, as for every hafted item: +Y from the butt, the handle (`shaft`) from y = 0 to
//! its length, then the guard and the blade above it (the prototype's `buildBladed`).

use super::blade::{BladeDamage, BuiltBlade, blade_mesh};
use super::guard::{GuardFit, guard_mesh};
use super::hafted::{Haft, butt_radius, grip_span, grip_mesh, pommel_mesh, rivets_mesh, shaft_bands, wrap_mesh};
use super::parts::Parts;
use crate::cond::Condition;
use crate::material::MaterialKind;
use crate::sections::blade::{Blade, Guard};
use crate::sections::hafted::{Grip, Pommel, Shaft, Wrap};

/// The sections of a hilted blade.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hilt<'a> {
    /// The handle's core (the tang, for a sword). Hidden when the grip covers it.
    pub shaft: &'a Shaft,
    pub grip: &'a Grip,
    pub wrap: &'a Wrap,
    pub pommel: &'a Pommel,
    pub guard: &'a Guard,
    pub blade: &'a Blade,
    /// Damage nicks the blade and can snap its point.
    pub cond: &'a Condition,
    pub seed: u32,
}

/// The finished parts and the blade's measurements.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HiltedBlade {
    pub parts: Parts,
    /// Height of the blade's base above the butt.
    pub blade_base: f32,
    pub blade: BuiltBlade,
}

fn is_metal(m: MaterialKind) -> bool {
    matches!(m, MaterialKind::Iron | MaterialKind::Brass | MaterialKind::Gold | MaterialKind::Silver)
}

/// Builds the handle, guard and blade. Parts: `shaft` (only where the grip leaves it bare),
/// `bands`, `grip`, `wrap`, `rivets`, `pommel`, `pommel_stones`, `guard`, `guard_stones`,
/// `blade`.
pub fn hilted_blade(h: &Hilt) -> HiltedBlade {
    let mut parts = Parts::new();
    let span = grip_span(h.shaft, h.grip);
    let mut haft = Haft::new(h.shaft, h.seed);
    let covered = h.grip.on && span.0 <= 1e-3 && span.1 >= 1.0 - 1e-3;
    if h.grip.on || h.wrap.on {
        haft.clear(span.0, span.1);
    }
    if !covered {
        parts.add("shaft", haft.mesh(), h.shaft.material);
    }
    let avoid: &[(f32, f32)] = if h.grip.on || h.wrap.on { &[span] } else { &[] };
    parts.add("bands", shaft_bands(&haft, h.shaft, avoid), MaterialKind::Iron);
    if h.grip.on {
        parts.add("grip", grip_mesh(&haft, h.grip, span), h.grip.material);
    }
    if h.wrap.on {
        parts.add("wrap", wrap_mesh(&haft, h.grip, span, h.wrap), h.wrap.material);
    }
    let rivet_metal = if is_metal(h.pommel.material) { h.pommel.material } else { MaterialKind::Iron };
    parts.add("rivets", rivets_mesh(&haft, h.grip, span), rivet_metal);
    if h.pommel.on {
        let pommel = pommel_mesh(h.pommel, butt_radius(&haft, h.grip, span)).placed_on(&haft);
        parts.add("pommel", pommel.body, h.pommel.material);
        parts.stone("pommel_stones", pommel.stones, h.pommel.gem_stone, 0.0, false);
    }

    // Guard and blade, built upright on the top of the handle, then set on it.
    let blade = blade_mesh(h.blade, &BladeDamage::from_condition(h.cond, h.seed));
    let mut upper = Parts::new();
    let y0 = -0.03;
    let height = h.guard.height();
    if h.guard.on {
        let grip_radius = haft.radius(1.0) * if h.grip.on { h.grip.thickness } else { 1.0 };
        let fit = GuardFit { blade_width: blade.base_width, blade_thickness: blade.base_thickness, grip_radius };
        let mut guard = guard_mesh(h.guard, &fit);
        for mesh in [&mut guard.body, &mut guard.stones] {
            mesh.translate([0.0, y0 + height * 0.5, 0.0]);
        }
        upper.add("guard", guard.body, h.guard.material);
        upper.stone("guard_stones", guard.stones, h.guard.gem_stone, 0.0, false);
    }
    let blade_y = if h.guard.on { y0 + height - 0.01 } else { y0 };
    let mut blade_mesh = blade.mesh.clone();
    blade_mesh.translate([0.0, blade_y, 0.0]);
    upper.add("blade", blade_mesh, h.blade.material);
    let (top, up) = (haft.point(1.0), haft.tangent(1.0));
    upper.place(top, up);
    parts.extend(upper);
    HiltedBlade { parts, blade_base: top[1] + blade_y, blade }
}
