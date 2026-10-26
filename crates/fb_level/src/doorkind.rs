//! The kind of each claim door and of the switch that works it (#568).
//!
//! A claim door is wood, steel or a portcullis. The kind is rolled per door from the level's
//! seed on a stream of its own (salted by the door's cell), leaning on the level's style (rough
//! rock toward wood and timber grates, stone halls toward steel and iron grates) and on the
//! faction that holds it: plunderers and wardens (the way-out factions) lean to iron, traders
//! and worshippers to wood, and a heavy armour lean tips it further. The claim's main door is
//! rolled like any other, so it is not necessarily the strongest. The switch on the claim's side
//! is rolled to suit the kind: a chain and ring for a portcullis's winch, a lever for a door's bolt,
//! with the other now and then.
//!
//! Everything here is plain data and a pure function of the seed. All the numbers are our own.

use fb_faction::{ArmourLean, Intention};
use procgen_rng::Mulberry32;

use crate::LevelStyle;

/// What a claim door is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum DoorKind {
    /// Planks and iron straps, hung on hinges.
    #[default]
    Wood,
    /// A riveted plate, heavier and harder to pick.
    Steel,
    /// A grate that rises into the lintel rather than swinging. It cannot be picked, and sight
    /// passes through it though movement does not.
    Portcullis,
}

impl DoorKind {
    pub const ALL: [DoorKind; 3] = [DoorKind::Wood, DoorKind::Steel, DoorKind::Portcullis];

    pub fn name(self) -> &'static str {
        match self {
            DoorKind::Wood => "wood",
            DoorKind::Steel => "steel",
            DoorKind::Portcullis => "portcullis",
        }
    }

    /// How much harder than a wooden door's its lock is to pick, or `None` where it cannot be.
    pub fn pick_penalty(self) -> Option<i32> {
        match self {
            DoorKind::Wood => Some(0),
            DoorKind::Steel => Some(STEEL_PICK_PENALTY),
            DoorKind::Portcullis => None,
        }
    }

    /// Whether sight passes through it shut.
    pub fn see_through(self) -> bool {
        self == DoorKind::Portcullis
    }
}

/// What a steel door adds to the pick's difficulty.
pub const STEEL_PICK_PENALTY: i32 = 4;

/// The switch on the claim's side of a door.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SwitchKind {
    /// A slim lever in a short slot, thrown up and down.
    #[default]
    Lever,
    /// A chain with a ring hanging beside the door, pulled down and let spring back.
    Chain,
}

impl SwitchKind {
    pub const ALL: [SwitchKind; 2] = [SwitchKind::Lever, SwitchKind::Chain];

    pub fn name(self) -> &'static str {
        match self {
            SwitchKind::Lever => "lever",
            SwitchKind::Chain => "chain",
        }
    }
}

/// The weights (wood, steel, portcullis) a level's style gives the kinds.
pub fn style_weights(style: LevelStyle) -> [u32; 3] {
    match style {
        LevelStyle::RoughRock | LevelStyle::CrystalCavern => [60, 10, 30],
        LevelStyle::StoneHalls => [20, 40, 40],
    }
}

/// The weights (wood, steel, portcullis) after the faction's lean: what `intention` and `armour`
/// multiply onto the style's. Whole numbers, never zero.
pub fn weights(style: LevelStyle, intention: Intention, armour: ArmourLean) -> [u32; 3] {
    let [wood, steel, grate] = style_weights(style).map(|w| w * 100);
    // Plunderers and wardens to iron, traders and worshippers to wood.
    let [wood, steel, grate] = match intention {
        Intention::Plunder | Intention::WayOut => [wood * 2 / 5, steel * 5 / 2, grate * 3 / 2],
        Intention::Trade | Intention::Worship => [wood * 5 / 2, steel * 2 / 5, grate * 2 / 3],
    };
    match armour {
        ArmourLean::Mail | ArmourLean::Scale | ArmourLean::Plate => [wood * 2 / 3, steel * 3 / 2, grate * 6 / 5],
        ArmourLean::Padded | ArmourLean::Leather => [wood * 3 / 2, steel * 2 / 3, grate * 5 / 6],
    }
    .map(|w| (w / 100).max(1))
}

/// Rolls the kind of a door of a faction with `intention` and `armour` in a level of `style`,
/// from `rng` (the door's own stream).
/// A `shared` door (its far side is another claim's ground, or it has no claim side) has no
/// switch to work a grate from, so it is only ever wood or steel.
pub fn roll_kind(style: LevelStyle, intention: Intention, armour: ArmourLean, shared: bool, rng: &mut Mulberry32) -> DoorKind {
    let [wood, steel, grate] = weights(style, intention, armour);
    let grate = if shared { 0 } else { grate };
    *rng.weighted(&[(DoorKind::Wood, wood), (DoorKind::Steel, steel), (DoorKind::Portcullis, grate)])
}

/// Rolls the switch that suits `kind`: a winch chain for a portcullis, a lever for a door's bolt,
/// the other now and then.
pub fn roll_switch(kind: DoorKind, rng: &mut Mulberry32) -> SwitchKind {
    let chain = match kind {
        DoorKind::Portcullis => 0.85,
        DoorKind::Steel => 0.25,
        DoorKind::Wood => 0.2,
    };
    if rng.chance(chain) { SwitchKind::Chain } else { SwitchKind::Lever }
}

/// The stream of the door in cell `(x, y)` of the level with `seed`.
pub fn door_stream(seed: u32, x: i32, y: i32) -> Mulberry32 {
    Mulberry32::new(seed ^ 0xd00c_4b1d ^ (x as u32).wrapping_mul(0x9e37_79b1) ^ (y as u32).wrapping_mul(0x85eb_ca6b))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTENTIONS: [Intention; 4] = Intention::ALL;
    const ARMOUR: [ArmourLean; 5] = [ArmourLean::Padded, ArmourLean::Leather, ArmourLean::Mail, ArmourLean::Scale, ArmourLean::Plate];

    /// The share (0..1) of `kind` among the weights.
    fn share(w: [u32; 3], kind: DoorKind) -> f64 {
        let i = DoorKind::ALL.iter().position(|&k| k == kind).unwrap();
        w[i] as f64 / w.iter().sum::<u32>() as f64
    }

    #[test]
    fn the_same_stream_gives_the_same_kind() {
        for (x, y) in [(3, 4), (10, 2), (0, 0)] {
            let a = roll_kind(LevelStyle::StoneHalls, Intention::Plunder, ArmourLean::Plate, false, &mut door_stream(7, x, y));
            let b = roll_kind(LevelStyle::StoneHalls, Intention::Plunder, ArmourLean::Plate, false, &mut door_stream(7, x, y));
            assert_eq!(a, b);
        }
    }

    #[test]
    fn rough_rock_leans_to_wood_and_stone_halls_to_iron() {
        for intention in INTENTIONS {
            for armour in ARMOUR {
                let rock = weights(LevelStyle::RoughRock, intention, armour);
                let halls = weights(LevelStyle::StoneHalls, intention, armour);
                assert!(share(rock, DoorKind::Wood) > share(halls, DoorKind::Wood), "{intention:?} {armour:?}");
                assert!(share(halls, DoorKind::Steel) > share(rock, DoorKind::Steel));
            }
        }
    }

    #[test]
    fn plunderers_and_wardens_lean_to_iron_and_traders_and_worshippers_to_wood() {
        for style in [LevelStyle::RoughRock, LevelStyle::StoneHalls] {
            for armour in ARMOUR {
                for iron in [Intention::Plunder, Intention::WayOut] {
                    for wood in [Intention::Trade, Intention::Worship] {
                        let (a, b) = (weights(style, iron, armour), weights(style, wood, armour));
                        assert!(share(a, DoorKind::Steel) > share(b, DoorKind::Steel));
                        assert!(share(b, DoorKind::Wood) > share(a, DoorKind::Wood));
                    }
                }
            }
        }
    }

    #[test]
    fn every_kind_stays_possible_for_every_faction_and_style() {
        for style in [LevelStyle::RoughRock, LevelStyle::StoneHalls] {
            for intention in INTENTIONS {
                for armour in ARMOUR {
                    assert!(weights(style, intention, armour).iter().all(|&w| w >= 1));
                    let mut seen = std::collections::HashSet::new();
                    for cell in 0..400 {
                        seen.insert(roll_kind(style, intention, armour, false, &mut door_stream(3, cell, cell / 7)));
                    }
                    assert_eq!(seen.len(), 3, "{style:?} {intention:?} {armour:?}");
                }
            }
        }
    }

    #[test]
    fn a_shared_door_rolls_only_wood_or_steel() {
        for style in [LevelStyle::RoughRock, LevelStyle::StoneHalls] {
            for intention in INTENTIONS {
                for armour in ARMOUR {
                    let kinds: std::collections::HashSet<DoorKind> = (0..600).map(|c| roll_kind(style, intention, armour, true, &mut door_stream(5, c, c / 5))).collect();
                    assert!(!kinds.contains(&DoorKind::Portcullis), "{style:?} {intention:?} {armour:?}");
                    assert!(kinds.contains(&DoorKind::Wood) && kinds.contains(&DoorKind::Steel));
                }
            }
        }
    }

    #[test]
    fn a_portcullis_is_worked_by_chain_and_a_door_mostly_by_lever() {
        let chains = |kind| (0..400).filter(|&i| roll_switch(kind, &mut door_stream(1, i, 2)) == SwitchKind::Chain).count();
        assert!(chains(DoorKind::Portcullis) > 300);
        assert!(chains(DoorKind::Wood) < 140 && chains(DoorKind::Steel) < 180);
        assert!(chains(DoorKind::Wood) > 20, "a door's chain is rare, not never");
    }

    #[test]
    fn only_steel_is_harder_to_pick_and_a_grate_cannot_be() {
        assert_eq!(DoorKind::Wood.pick_penalty(), Some(0));
        assert_eq!(DoorKind::Steel.pick_penalty(), Some(4));
        assert_eq!(DoorKind::Portcullis.pick_penalty(), None);
        assert!(DoorKind::Portcullis.see_through() && !DoorKind::Wood.see_through() && !DoorKind::Steel.see_through());
    }
}
