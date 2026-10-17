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

/// The effect lice in what the delver wears bring on.
pub const ITCHING: &str = "itching";

/// The affix that puts lice in what is worn.
const LICE: &str = "Lice-ridden";

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
        // Lice in anything worn (not merely held) get into the delver's clothes and hair. The
        // item keeps its lice wherever it lies.
        if !held && item.affixes.iter().any(|a| a.name == LICE) {
            push(ITCHING, None, Source::Item { name: item.name.clone() });
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

    /// A cloak rolled with the Lice-ridden affix, worn.
    fn wearing_lice() -> (Inventory, crate::inventory::items::ItemId) {
        let seed = (1..40_000).find(|&s| fb_items::roll_item("Cloak", s).is_some_and(|i| i.affixes.iter().any(|a| a.name == LICE))).expect("some cloak has lice");
        let mut inventory = Inventory::default();
        let id = inventory.add(fb_items::roll_item("Cloak", seed).unwrap());
        inventory.equip(id, Slot::Cloak).expect("a cloak goes on");
        (inventory, id)
    }

    #[test]
    fn a_lice_ridden_cloak_brings_on_itching_after_a_while_and_it_fades_once_off() {
        let defs = fb_character::effect::shipped();
        let (worn, id) = wearing_lice();
        let lice = exposures(&worn, &defs);
        assert_eq!(lice.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec![ITCHING]);
        let mut effects = fb_character::effect::Effects::default();
        let word = |e: &fb_character::effect::Effects| e.showing(&defs).next().map(|(_, _, _, s)| s.word.clone());
        effects.step(&defs, 0.25, &lice);
        assert_eq!(word(&effects), None, "not yet, a quarter hour in");
        effects.step(&defs, 1.5, &lice);
        assert_eq!(word(&effects).as_deref(), Some("Itching"), "within two hours");
        assert!(effects.noticed(&defs) > 0.0, "easier to notice");
        // Taken off, the cloak keeps its lice; the itch fades over a few hours.
        let mut off = worn.clone();
        off.take_out(id);
        assert!(exposures(&off, &defs).is_empty());
        assert!(off.item(id).unwrap().affixes.iter().any(|a| a.name == LICE), "the cloak keeps its lice");
        effects.step(&defs, 1.0, &[]);
        assert!(word(&effects).is_some(), "still itching an hour after");
        effects.step(&defs, 3.0, &[]);
        assert_eq!(word(&effects), None, "gone a few hours after");
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
