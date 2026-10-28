//! Condition: how worn, rusted, rotted and damaged an item is. Shared by every family.
//! From the prototype's `cond` section, `BASE.cond`, `COND_STATES` and `condWord` in
//! `reference/spline_armory_engine.js`; the shader side is in `docs/rendering.md`
//! ("Wear, rust and grime").
//!
//! "Forge new" makes items pristine. Wear comes from what happens to an item after it is
//! made, which is `fb_items`' job.

use crate::slider::sliders;
use crate::material::MaterialKind;
use serde::{Deserialize, Serialize};

/// The named condition presets. `Custom` means the sliders were set by hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionState {
    Custom,
    Pristine,
    Serviceable,
    BattleWorn,
    Neglected,
    /// Dug out of a grave or a ruin.
    GraveFind,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub state: ConditionState,
    /// Overall wear, 0..=1.
    pub wear: f32,
    /// How much of the wear is rust on iron: 0 for none (plain wear only dulls, stains and nicks
    /// it), 1 for rust to the full extent of the wear. Raised by things that rust iron: the
    /// Rusted affix, grave goods.
    pub rust: f32,
    /// Per-material multipliers on `wear`.
    pub patina: f32,
    pub rot: f32,
    pub fray: f32,
    /// Nicks, chips and losses, 0..=1.
    pub damage: f32,
    /// Snapped-off point (bladed families only).
    pub broken: bool,
    /// Scale of the wear pattern.
    pub scale: f32,
}

sliders! {
    ConditionLimits for Condition {
        wear: 0.0, 1.0, 0.01;
        rust: 0.0, 1.5, 0.01;
        patina: 0.0, 1.5, 0.01;
        rot: 0.0, 1.5, 0.01;
        fray: 0.0, 1.5, 0.01;
        damage: 0.0, 1.0, 0.01;
        scale: 0.5, 4.0, 0.05;
    }
}

impl Condition {
    /// How strongly a material shows wear: overall wear times that material's multiplier,
    /// capped at 1.2 (`decayAmt` in `reference/spline_armory_engine.js`).
    pub fn decay(&self, kind: MaterialKind) -> f32 {
        let multiplier = match kind {
            // Iron's wear is dulling, stain and nicks; its rust is `rust_decay`.
            MaterialKind::Iron => 1.0,
            MaterialKind::Brass | MaterialKind::Gold | MaterialKind::Silver => self.patina,
            MaterialKind::Wood | MaterialKind::Bone | MaterialKind::Stone | MaterialKind::Food => self.rot,
            MaterialKind::Leather | MaterialKind::Cord | MaterialKind::Cloth => self.fray,
            MaterialKind::Gem | MaterialKind::Glass | MaterialKind::Paint | MaterialKind::Skin | MaterialKind::Hair => 1.0,
        };
        (self.wear * multiplier).clamp(0.0, 1.2)
    }

    /// How much rust iron shows: wear times `rust`, capped at 1.2. Zero for every other material,
    /// and for iron unless `rust` is raised.
    pub fn rust_decay(&self, kind: MaterialKind) -> f32 {
        match kind {
            MaterialKind::Iron => (self.wear * self.rust).clamp(0.0, 1.2),
            _ => 0.0,
        }
    }

    /// The sliders for a named state (`COND_STATES`). `Custom` gives the pristine values.
    pub fn of(state: ConditionState) -> Self {
        let (wear, damage, rust, patina, rot, fray) = match state {
            ConditionState::Custom | ConditionState::Pristine => (0.0, 0.0, 0.0, 1.0, 1.0, 1.0),
            ConditionState::Serviceable => (0.25, 0.15, 0.0, 1.0, 1.0, 1.0),
            ConditionState::BattleWorn => (0.45, 0.55, 0.0, 1.0, 0.8, 1.2),
            ConditionState::Neglected => (0.75, 0.3, 0.0, 1.2, 1.25, 1.0),
            ConditionState::GraveFind => (1.0, 0.8, 1.4, 1.4, 1.4, 1.5),
        };
        Condition { state, wear, rust, patina, rot, fray, damage, broken: false, scale: 1.6 }
    }

    /// The word the item card uses for overall wear (`condWord`).
    pub fn word(&self) -> &'static str {
        match self.wear {
            w if w < 0.1 => "Pristine",
            w if w < 0.3 => "Serviceable",
            w if w < 0.55 => "Battle-worn",
            w if w < 0.8 => "Neglected",
            _ => "Ruinous",
        }
    }
}

impl Default for Condition {
    fn default() -> Self {
        Condition::of(ConditionState::Pristine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decay_uses_the_material_multiplier() {
        let condition = Condition { wear: 0.5, rust: 2.0, rot: 0.5, ..Condition::default() };
        assert_eq!(condition.decay(MaterialKind::Iron), 0.5);
        assert_eq!(condition.rust_decay(MaterialKind::Iron), 1.0);
        assert_eq!(condition.decay(MaterialKind::Wood), 0.25);
        assert_eq!(condition.rust_decay(MaterialKind::Wood), 0.0);
        let ruinous = Condition { wear: 1.0, rust: 1.5, ..Condition::default() };
        assert_eq!(ruinous.rust_decay(MaterialKind::Iron), 1.2);
    }

    #[test]
    fn plain_wear_never_rusts_iron() {
        for state in [ConditionState::Pristine, ConditionState::Serviceable, ConditionState::BattleWorn, ConditionState::Neglected] {
            let condition = Condition::of(state);
            assert_eq!(condition.rust, 0.0, "{state:?}");
            for kind in MaterialKind::SELECTABLE {
                assert_eq!(condition.rust_decay(kind), 0.0, "{state:?} {kind:?}");
            }
        }
        let worn = Condition::of(ConditionState::BattleWorn);
        assert_eq!(worn.decay(MaterialKind::Iron), worn.wear);
        assert!(Condition::of(ConditionState::GraveFind).rust_decay(MaterialKind::Iron) > 1.0);
    }

    #[test]
    fn pristine_items_have_no_decay() {
        for kind in MaterialKind::SELECTABLE {
            assert_eq!(Condition::default().decay(kind), 0.0);
        }
    }
}
