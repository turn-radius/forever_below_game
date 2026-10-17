//! The range of a bow or crossbow, and of a creature's ranged attack, in feet as the SRD gives
//! it (SRD 5.1 p. 95, "Range"), with the conversion to world units. The rules that read it
//! (`fb_character::combat::ranged_shot`) re-export it.

/// Feet to world units: 1 world unit is 10 cm, so a foot (30.48 cm) is 3.048 units.
pub const FEET_TO_UNITS: f32 = 3.048;

/// A distance in world units as feet.
pub fn units_to_feet(units: f32) -> f32 {
    units / FEET_TO_UNITS
}

/// A ranged weapon's range in feet (SRD 5.1 p. 95, "Range"): shot normally out to `normal_feet`
/// and at disadvantage out to `long_feet`, never beyond.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub normal_feet: f32,
    pub long_feet: f32,
}

impl Range {
    pub const fn new(normal_feet: f32, long_feet: f32) -> Self {
        Range { normal_feet, long_feet }
    }

    /// The band `distance_feet` falls in ([`range_band`]).
    pub fn band(self, distance_feet: f32) -> RangeBand {
        range_band(self, distance_feet)
    }
}

/// Where a target stands against a [`Range`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeBand {
    /// Within normal range: no penalty.
    Normal,
    /// Beyond normal, within long range: disadvantage.
    Long,
    /// Beyond long range: the weapon cannot hit.
    Out,
}

/// The [`RangeBand`] of a target `distance_feet` away: normal out to and including the normal
/// range, long out to and including the long range, and out beyond (SRD 5.1 p. 95).
pub fn range_band(range: Range, distance_feet: f32) -> RangeBand {
    if distance_feet <= range.normal_feet {
        RangeBand::Normal
    } else if distance_feet <= range.long_feet {
        RangeBand::Long
    } else {
        RangeBand::Out
    }
}
