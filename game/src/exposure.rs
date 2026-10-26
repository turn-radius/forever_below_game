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

/// The cold creeping in while worn warmth falls short of the need.
pub const COLD: &str = "cold";
/// What the cold does to the cold-blooded.
pub const CHILLED_BLOOD: &str = "chilled_blood";
/// Lingering by a fire.
pub const WARMED: &str = "warmed";

/// Where the delver is, as the cold and the fire reach them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Surroundings {
    /// Warmth they are short of ([`fb_character::rules::WarmthRules::shortfall`]): positive is
    /// cold, negative warmth to spare.
    pub shortfall: f64,
    /// Whether a fire's warmth reaches them.
    pub by_fire: bool,
    /// Whether they are cold-blooded (lizardfolk), whom the cold also slows and tires.
    pub cold_blooded: bool,
}

/// Every cause in `inventory` and `around` acting on the delver now, at the rates `defs` give.
/// A cause whose effect `defs` lacks is left out.
pub fn exposures(
    inventory: &Inventory,
    defs: &BTreeMap<String, EffectDef>,
    around: &Surroundings,
) -> Vec<Exposure> {
    let mut out = vec![];
    // `scale` times the effect's own exposure rate: the cold's is per point of warmth short.
    let mut push = |id: &str, scale: f64, source: Source| {
        if let Some(def) = defs.get(id) {
            out.push(Exposure {
                id: id.to_string(),
                per_hour: def.exposure_per_hour * scale,
                part: None,
                source,
            });
        }
    };
    // The cold, by how far worn warmth falls short; warmth to spare drives it off faster.
    if around.shortfall != 0.0 {
        push(COLD, around.shortfall, Source::Surroundings);
        if around.cold_blooded {
            push(CHILLED_BLOOD, around.shortfall, Source::Surroundings);
        }
    }
    if around.by_fire {
        push(WARMED, 1.0, Source::Surroundings);
    }
    for (slot, item) in inventory.equipped() {
        let held = matches!(slot, Slot::Mainhand | Slot::Offhand);
        // A lit light with the Smoky affix, in hand, puts its smoke in the delver's eyes.
        if held && item.shape.lit() == Some(true) && item.affixes.iter().any(|a| a.name == SMOKY) {
            push(
                STINGING_EYES,
                1.0,
                Source::Item {
                    name: item.name.clone(),
                },
            );
        }
        // Lice in anything worn (not merely held) get into the delver's clothes and hair. The
        // item keeps its lice wherever it lies.
        if !held && item.affixes.iter().any(|a| a.name == LICE) {
            push(
                ITCHING,
                1.0,
                Source::Item {
                    name: item.name.clone(),
                },
            );
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
            .find(|&s| {
                fb_items::roll_item("Torch", s).is_some_and(|i| {
                    i.affixes.iter().any(|a| a.name == SMOKY) == smoky
                        && i.shape.clone().set_lit(true).is_ok()
                })
            })
            .expect("some torch smokes and some does not");
        let mut item = fb_items::roll_item("Torch", seed).unwrap();
        item.shape
            .set_lit(lit)
            .expect("a fresh torch lights and puts out");
        let mut inventory = Inventory::default();
        let id = inventory.add(item);
        inventory
            .equip(id, Slot::Offhand)
            .expect("a torch goes in the off hand");
        inventory
    }

    #[test]
    fn a_lit_smoky_light_in_hand_stings_the_eyes_and_nothing_else_does() {
        let defs = fb_character::effect::shipped();
        let stinging = exposures(&holding(true, true), &defs, &Surroundings::default());
        assert_eq!(stinging.len(), 1);
        assert_eq!(
            (stinging[0].id.as_str(), stinging[0].per_hour),
            (STINGING_EYES, defs[STINGING_EYES].exposure_per_hour)
        );
        assert!(
            exposures(&holding(true, false), &defs, &Surroundings::default()).is_empty(),
            "a doused smoky torch gives no smoke"
        );
        assert!(
            exposures(&holding(false, true), &defs, &Surroundings::default()).is_empty(),
            "a clean torch gives no smoke"
        );
    }

    fn stage(
        effects: &fb_character::effect::Effects,
        defs: &BTreeMap<String, EffectDef>,
        id: &str,
    ) -> Option<String> {
        effects
            .get(id, None)
            .and_then(|a| a.stage(&defs[id]))
            .map(|(_, s)| s.word.clone())
    }

    /// Steps `hours` an hour at a time in `around`.
    fn live(
        effects: &mut fb_character::effect::Effects,
        defs: &BTreeMap<String, EffectDef>,
        around: &Surroundings,
        hours: u32,
    ) {
        for _ in 0..hours {
            effects.step(defs, 1.0, &exposures(&Inventory::default(), defs, around));
        }
    }

    #[test]
    fn a_shirt_three_levels_down_grows_chilly_then_cold_and_a_cloak_of_the_hearth_stops_it() {
        let (defs, warmth) = (
            fb_character::effect::shipped(),
            fb_character::rules::WarmthRules::default(),
        );
        let in_a_shirt = Surroundings {
            shortfall: warmth.shortfall(6.0, 3, 1.0, 0.0),
            ..Surroundings::default()
        };
        assert_eq!(in_a_shirt.shortfall, 3.0);
        let mut effects = fb_character::effect::Effects::default();
        live(&mut effects, &defs, &in_a_shirt, 1);
        assert_eq!(stage(&effects, &defs, COLD).as_deref(), Some("Chilly"));
        live(&mut effects, &defs, &in_a_shirt, 1);
        assert_eq!(stage(&effects, &defs, COLD).as_deref(), Some("Cold"));
        assert!(
            effects.need_rate(&defs, fb_character::MeterKind::Hunger) > 1.0,
            "the cold makes them hungrier"
        );
        // A cloak of the Hearth (2) brings them to a shortfall of 1, which the cold cannot hold.
        let cloaked = Surroundings {
            shortfall: warmth.shortfall(6.0, 3, 3.0, 0.0),
            ..Surroundings::default()
        };
        live(&mut effects, &defs, &cloaked, 6);
        assert_eq!(stage(&effects, &defs, COLD), None, "warm enough again");
        // On the first level the shirt alone is enough.
        let mut first = fb_character::effect::Effects::default();
        live(
            &mut first,
            &defs,
            &Surroundings {
                shortfall: warmth.shortfall(6.0, 1, 1.0, 0.0),
                ..Surroundings::default()
            },
            12,
        );
        assert!(
            first.active.is_empty(),
            "no cold on the first level in a shirt"
        );
    }

    #[test]
    fn freezing_costs_hit_points_and_a_brazier_warms_and_lifts_the_cold() {
        let (defs, warmth) = (
            fb_character::effect::shipped(),
            fb_character::rules::WarmthRules::default(),
        );
        let mut effects = fb_character::effect::Effects::default();
        let bare = Surroundings {
            shortfall: warmth.shortfall(6.0, 5, 0.0, 0.0),
            ..Surroundings::default()
        };
        assert_eq!(effects.hit_points_per_hour(&defs), 0.0);
        live(&mut effects, &defs, &bare, 3);
        assert_eq!(stage(&effects, &defs, COLD).as_deref(), Some("Freezing"));
        assert!(
            effects.hit_points_per_hour(&defs) > 0.0,
            "freezing wears them down"
        );
        let by_the_brazier = Surroundings {
            shortfall: warmth.shortfall(6.0, 5, 0.0, warmth.brazier),
            by_fire: true,
            ..Surroundings::default()
        };
        live(&mut effects, &defs, &by_the_brazier, 1);
        assert_eq!(stage(&effects, &defs, WARMED).as_deref(), Some("Warmed"));
        assert_eq!(
            stage(&effects, &defs, COLD).as_deref(),
            Some("Chilly"),
            "an hour by the fire lifts most of it"
        );
        live(&mut effects, &defs, &by_the_brazier, 1);
        assert_eq!(stage(&effects, &defs, COLD), None);
    }

    #[test]
    fn a_cold_lizardfolk_walks_slower_at_cold_than_at_chilly() {
        let (defs, warmth) = (
            fb_character::effect::shipped(),
            fb_character::rules::WarmthRules::default(),
        );
        let cold_blooded = Surroundings {
            shortfall: warmth.shortfall(8.0, 1, 1.0, 0.0),
            cold_blooded: true,
            ..Surroundings::default()
        };
        let mut effects = fb_character::effect::Effects::default();
        live(&mut effects, &defs, &cold_blooded, 1);
        assert_eq!(stage(&effects, &defs, COLD).as_deref(), Some("Chilly"));
        let chilly = effects.pace(&defs);
        live(&mut effects, &defs, &cold_blooded, 1);
        assert_eq!(stage(&effects, &defs, COLD).as_deref(), Some("Cold"));
        assert!(
            effects.pace(&defs) < chilly && chilly < 1.0,
            "{chilly} then {}",
            effects.pace(&defs)
        );
        // A human just as cold keeps their pace.
        let mut human = fb_character::effect::Effects::default();
        live(
            &mut human,
            &defs,
            &Surroundings {
                cold_blooded: false,
                ..cold_blooded
            },
            2,
        );
        assert_eq!(human.pace(&defs), 1.0);
    }

    /// A cloak rolled with the Lice-ridden affix, worn.
    fn wearing_lice() -> (Inventory, crate::inventory::items::ItemId) {
        let seed = (1..40_000)
            .find(|&s| {
                fb_items::roll_item("Cloak", s)
                    .is_some_and(|i| i.affixes.iter().any(|a| a.name == LICE))
            })
            .expect("some cloak has lice");
        let mut inventory = Inventory::default();
        let id = inventory.add(fb_items::roll_item("Cloak", seed).unwrap());
        inventory.equip(id, Slot::Cloak).expect("a cloak goes on");
        (inventory, id)
    }

    #[test]
    fn a_lice_ridden_cloak_brings_on_itching_after_a_while_and_it_fades_once_off() {
        let defs = fb_character::effect::shipped();
        let (worn, id) = wearing_lice();
        let lice = exposures(&worn, &defs, &Surroundings::default());
        assert_eq!(
            lice.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec![ITCHING]
        );
        let mut effects = fb_character::effect::Effects::default();
        let word = |e: &fb_character::effect::Effects| {
            e.showing(&defs).next().map(|(_, _, _, s)| s.word.clone())
        };
        effects.step(&defs, 0.25, &lice);
        assert_eq!(word(&effects), None, "not yet, a quarter hour in");
        effects.step(&defs, 1.5, &lice);
        assert_eq!(
            word(&effects).as_deref(),
            Some("Itching"),
            "within two hours"
        );
        assert!(effects.noticed(&defs) > 0.0, "easier to notice");
        // Taken off, the cloak keeps its lice; the itch fades over a few hours.
        let mut off = worn.clone();
        off.take_out(id);
        assert!(exposures(&off, &defs, &Surroundings::default()).is_empty());
        assert!(
            off.item(id).unwrap().affixes.iter().any(|a| a.name == LICE),
            "the cloak keeps its lice"
        );
        effects.step(&defs, 1.0, &[]);
        assert!(word(&effects).is_some(), "still itching an hour after");
        effects.step(&defs, 3.0, &[]);
        assert_eq!(word(&effects), None, "gone a few hours after");
    }

    #[test]
    fn holding_a_smoky_torch_brings_on_stinging_eyes_within_a_few_minutes_and_a_clean_one_lets_it_fade()
     {
        let defs = fb_character::effect::shipped();
        let mut effects = fb_character::effect::Effects::default();
        let smoky = exposures(&holding(true, true), &defs, &Surroundings::default());
        effects.step(&defs, 5.0 / 60.0, &smoky);
        let word = |e: &fb_character::effect::Effects| {
            e.showing(&defs).next().map(|(_, _, _, s)| s.word.clone())
        };
        assert_eq!(
            word(&effects).as_deref(),
            Some("Smarting eyes"),
            "five minutes in"
        );
        effects.step(&defs, 10.0 / 60.0, &smoky);
        assert_eq!(
            word(&effects).as_deref(),
            Some("Stinging eyes"),
            "a quarter of an hour in"
        );
        assert!(
            effects.sight(&defs) < 0.9,
            "sight is shorter: {}",
            effects.sight(&defs)
        );
        let clean = exposures(&holding(false, true), &defs, &Surroundings::default());
        effects.step(&defs, 1.0, &clean);
        assert_eq!(word(&effects), None, "gone within an hour of a clean light");
        assert_eq!(effects.sight(&defs), 1.0);
    }
}
