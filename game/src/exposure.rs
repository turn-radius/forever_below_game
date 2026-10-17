//! What in the world keeps acting on the delver, as effect exposures ([`fb_character::effect`]):
//! a held light that smokes stings their eyes. Each cause names its effect, and the effect's
//! `exposure_per_hour` in `data/effects.json` is how fast it brings it on, so the rules (and the
//! lab) set the pace. `crate::simulate::step_the_delver` passes them to the effects' step.

use std::collections::BTreeMap;

use fb_character::effect::{EffectDef, Exposure, Source};
use fb_items::Slot;

use crate::inventory::Inventory;

/// The effect a smoking light brings on.
pub const STINGING_EYES: &str = "stinging_eyes";

/// The affix that makes a light smoke.
const SMOKY: &str = "Smoky";

/// Every cause in `inventory` acting on the delver now, at the rates `defs` give. A cause whose
/// effect `defs` lacks is left out.
pub fn exposures(inventory: &Inventory, defs: &BTreeMap<String, EffectDef>) -> Vec<Exposure> {
    let mut out = vec![];
    let mut push = |id: &str, part: Option<Slot>, source: Source| {
        if let Some(def) = defs.get(id) {
            out.push(Exposure { id: id.to_string(), per_hour: def.exposure_per_hour, part, source });
        }
    };
    for (slot, item) in inventory.equipped() {
        let held = matches!(slot, Slot::Mainhand | Slot::Offhand);
        // A lit light with the Smoky affix, in hand, puts its smoke in the delver's eyes.
        if held && item.shape.lit() == Some(true) && item.affixes.iter().any(|a| a.name == SMOKY) {
            push(STINGING_EYES, None, Source::Item { name: item.name.clone() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A torch rolled with the Smoky affix (or without), lit or not, held in the off hand.
    fn holding(smoky: bool, lit: bool) -> Inventory {
        let seed = (1..20_000)
            .find(|&s| fb_items::roll_item("Torch", s).is_some_and(|i| i.affixes.iter().any(|a| a.name == SMOKY) == smoky && i.shape.clone().set_lit(true).is_ok()))
            .expect("some torch smokes and some does not");
        let mut item = fb_items::roll_item("Torch", seed).unwrap();
        item.shape.set_lit(lit).expect("a fresh torch lights and puts out");
        let mut inventory = Inventory::default();
        let id = inventory.add(item);
        inventory.equip(id, Slot::Offhand).expect("a torch goes in the off hand");
        inventory
    }

    #[test]
    fn a_lit_smoky_light_in_hand_stings_the_eyes_and_nothing_else_does() {
        let defs = fb_character::effect::shipped();
        let stinging = exposures(&holding(true, true), &defs);
        assert_eq!(stinging.len(), 1);
        assert_eq!((stinging[0].id.as_str(), stinging[0].per_hour), (STINGING_EYES, defs[STINGING_EYES].exposure_per_hour));
        assert!(exposures(&holding(true, false), &defs).is_empty(), "a doused smoky torch gives no smoke");
        assert!(exposures(&holding(false, true), &defs).is_empty(), "a clean torch gives no smoke");
    }

    #[test]
    fn holding_a_smoky_torch_brings_on_stinging_eyes_within_a_few_minutes_and_a_clean_one_lets_it_fade() {
        let defs = fb_character::effect::shipped();
        let mut effects = fb_character::effect::Effects::default();
        let smoky = exposures(&holding(true, true), &defs);
        effects.step(&defs, 5.0 / 60.0, &smoky);
        let word = |e: &fb_character::effect::Effects| e.showing(&defs).next().map(|(_, _, _, s)| s.word.clone());
        assert_eq!(word(&effects).as_deref(), Some("Smarting eyes"), "five minutes in");
        effects.step(&defs, 10.0 / 60.0, &smoky);
        assert_eq!(word(&effects).as_deref(), Some("Stinging eyes"), "a quarter of an hour in");
        assert!(effects.sight(&defs) < 0.9, "sight is shorter: {}", effects.sight(&defs));
        let clean = exposures(&holding(false, true), &defs);
        effects.step(&defs, 1.0, &clean);
        assert_eq!(word(&effects), None, "gone within an hour of a clean light");
        assert_eq!(effects.sight(&defs), 1.0);
    }
}
