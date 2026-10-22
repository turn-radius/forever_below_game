//! SPIKE (`FB_LOOK=pulp`): an alternate set of body proportions, heroic and "painted" rather than
//! doll-like. Plain data; the game sets it once at startup with [`set_look`] and every
//! [`crate::Body`] built afterwards reads it. Unset, [`Look::NOW`] is every multiplier 1 and every
//! addition 0, so bodies are bit-for-bit what they were.

use std::sync::OnceLock;

use crate::folk::Folk;

/// Multipliers (1 = unchanged) and additions (0 = unchanged) on a body's proportions, for a
/// folk that takes the look in full (a human). Other folk take a share of it ([`Look::share`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Head size (radius), width of the head.
    pub head: f32,
    pub head_width: f32,
    /// Added to the hip's share of height (longer legs).
    pub leg: f32,
    pub shoulders: f32,
    pub chest: f32,
    pub waist: f32,
    pub neck: f32,
    /// Extra swell in the deltoid, forearm and calf profiles.
    pub deltoid: f32,
    pub forearm: f32,
    pub calf: f32,
    /// Face: added to the folk's jaw narrowing and chin jut; brow ridge and cheekbone relief
    /// multipliers; eye size.
    pub jaw: f32,
    pub chin: f32,
    pub brow: f32,
    pub cheek: f32,
    pub eye: f32,
}

impl Look {
    pub const NOW: Look = Look { head: 1.0, head_width: 1.0, leg: 0.0, shoulders: 1.0, chest: 1.0, waist: 1.0, neck: 1.0, deltoid: 1.0, forearm: 1.0, calf: 1.0, jaw: 0.0, chin: 0.0, brow: 1.0, cheek: 1.0, eye: 1.0 };

    /// 1980s pulp fantasy: about eight heads tall, broad chest, narrow waist, long legs,
    /// defined limbs, a squarer jaw, a heavier brow, smaller eyes.
    pub const PULP: Look = Look {
        head: 0.90,
        head_width: 0.94,
        leg: 0.035,
        shoulders: 1.14,
        chest: 1.08,
        waist: 0.88,
        neck: 1.12,
        deltoid: 1.14,
        forearm: 1.12,
        calf: 1.16,
        jaw: -0.18,
        chin: 0.14,
        brow: 1.5,
        cheek: 2.2,
        eye: 0.8,
    };

    /// How much of the look a folk takes: dwarves stay stocky, gnomes small, the rest part-way.
    pub fn share(folk: Folk) -> f32 {
        match folk {
            Folk::Human => 1.0,
            Folk::Dwarf => 0.45,
            Folk::Gnome => 0.4,
            Folk::Goblin => 0.6,
            Folk::Lizardfolk => 0.6,
            Folk::Pigfolk => 0.45,
        }
    }

    /// This look at `k` of its strength (1 exactly leaves it as it is, 0 gives [`Look::NOW`]).
    pub fn at(self, k: f32) -> Look {
        let m = |v: f32| 1.0 + (v - 1.0) * k;
        let a = |v: f32| v * k;
        Look {
            head: m(self.head), head_width: m(self.head_width), leg: a(self.leg), shoulders: m(self.shoulders), chest: m(self.chest), waist: m(self.waist),
            neck: m(self.neck), deltoid: m(self.deltoid), forearm: m(self.forearm), calf: m(self.calf), jaw: a(self.jaw), chin: a(self.chin),
            brow: m(self.brow), cheek: m(self.cheek), eye: m(self.eye),
        }
    }

    pub fn for_folk(self, folk: Folk) -> Look {
        self.at(Look::share(folk))
    }
}

static LOOK: OnceLock<Look> = OnceLock::new();

/// Sets the look every body is built with. Call once at startup; later calls are ignored.
pub fn set_look(look: Look) {
    let _ = LOOK.set(look);
}

/// The look in force (unset: [`Look::NOW`]).
pub fn look() -> Look {
    LOOK.get().copied().unwrap_or(Look::NOW)
}

/// The look in force, at `folk`'s share of it.
pub fn look_for(folk: Folk) -> Look {
    look().for_folk(folk)
}
