//! Roll helpers shared by the hilted families (swords and daggers): pommel sizes that suit
//! their style, guard terminals and stones, and etching targets for the materials a hilt has.

use procgen_rng::Mulberry32;

use crate::etch::EtchTarget;
use crate::gem::Gemstone;
use crate::material::MaterialKind;
use crate::sections::blade::{Guard, GuardTerminal};
use crate::sections::hafted::{Grip, Pommel, PommelGem, PommelStyle, Wrap};

use MaterialKind::{Bone, Brass, Gold, Iron, Leather, Silver, Wood};

pub(crate) fn is_metal(m: MaterialKind) -> bool {
    matches!(m, Iron | Brass | Gold | Silver)
}

/// Length and radius for a pommel of `style` on a handle of `handle_radius`, scaled by
/// `scale` (1 for a sword, about 0.6 for a dagger).
pub(crate) fn size_pommel(rng: &mut Mulberry32, p: &mut Pommel, handle_radius: f32, scale: f32) {
    use PommelStyle::*;
    let (length, radius) = match p.style {
        // The wheel stands on edge, so its length spans the whole disc and the peen block.
        Wheel => {
            let r = rng.range_f32(0.2, 0.28) * scale;
            (r * 2.0 + 0.1 * scale, r)
        }
        ScentStopper => (rng.range_f32(0.45, 0.7) * scale, rng.range_f32(0.2, 0.3) * scale),
        Ball => (rng.range_f32(0.3, 0.45) * scale, rng.range_f32(0.16, 0.24) * scale),
        Disc => (rng.range_f32(0.18, 0.28) * scale, rng.range_f32(0.24, 0.32) * scale),
        Flared => (rng.range_f32(0.3, 0.45) * scale, rng.range_f32(0.18, 0.26) * scale),
        Ferrule => (rng.range_f32(0.2, 0.35) * scale, handle_radius * rng.range_f32(1.1, 1.3)),
        Spike => (rng.range_f32(0.3, 0.5) * scale, handle_radius * rng.range_f32(1.1, 1.4)),
    };
    p.length = length;
    p.radius = radius.max(handle_radius * 1.15);
    p.facets = match p.style {
        ScentStopper => *rng.pick(&[6, 8, 8, 10, 12]),
        Ferrule => rng.int(8, 16),
        _ => rng.int(16, 28),
    };
}

/// Stones in the pommel: an end stone, or a ring round a face, with probability `odds`.
pub(crate) fn set_pommel_stones(rng: &mut Mulberry32, p: &mut Pommel, odds: f64, stone: Gemstone) {
    let set = !matches!(p.style, PommelStyle::Ferrule | PommelStyle::Spike) && rng.chance(odds);
    p.gem = if !set {
        PommelGem::None
    } else if p.style.has_face() && rng.chance(0.5) {
        PommelGem::Ring
    } else {
        PommelGem::EndStone
    };
    p.gem_count = rng.int(4, 8);
    p.gem_size = match p.gem {
        PommelGem::Ring => rng.range_f32(0.025, 0.045),
        _ => rng.range_f32(0.04, 0.08).min(p.radius * 0.5),
    };
    p.gem_stone = stone;
}

/// A terminal for each arm end, weighted.
pub(crate) fn roll_terminal(rng: &mut Mulberry32, g: &mut Guard, weights: &[(GuardTerminal, u32)], size: (f32, f32)) {
    g.terminal = *rng.weighted(weights);
    g.terminal_size = match g.terminal {
        GuardTerminal::None => g.radius * 1.2,
        _ => rng.range_f32(size.0, size.1),
    };
}

/// A stone in the guard's centre, with probability `odds`.
pub(crate) fn set_guard_stone(rng: &mut Mulberry32, g: &mut Guard, odds: f64, size: (f32, f32), stone: Gemstone) {
    g.gem = rng.chance(odds);
    g.gem_size = rng.range_f32(size.0, size.1);
    g.gem_stone = stone;
}

/// Etching goes on the metals, or now and then on a carved, tooled grip.
pub(crate) fn etch_targets(grip: &Grip) -> Vec<(EtchTarget, u32)> {
    let mut targets = vec![(EtchTarget::Metals, 4)];
    match grip.material {
        Wood => targets.push((EtchTarget::Wood, 1)),
        Bone => targets.push((EtchTarget::Bone, 1)),
        Leather => targets.push((EtchTarget::Leather, 1)),
        _ => {}
    }
    targets
}

/// Cord and wire go on in tight turns, touching: as many turns as the cord's thickness
/// allows. Where that would be more than `max_turns`, the cord thickens up to `max_cord`,
/// and past that the wrap covers less of the grip.
pub(crate) fn close_wind(handle: f32, w: &mut Wrap, max_turns: u32, max_cord: f32) {
    let pitch = |cord: f32| 2.2 * cord;
    let mut run = handle * w.coverage;
    let turns = (run / pitch(w.cord)).round();
    if turns > max_turns as f32 {
        w.turns = max_turns;
        w.cord = (run / (pitch(1.0) * max_turns as f32)).min(max_cord);
        run = pitch(w.cord) * max_turns as f32;
        w.coverage = (run / handle).min(w.coverage);
    } else {
        w.turns = turns.max(2.0) as u32;
    }
}
