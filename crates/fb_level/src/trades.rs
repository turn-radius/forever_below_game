//! Who has a trade: the guards and traders of each faction's camp, and the masters the world
//! places on this level.
//!
//! A pure function of the world seed and the level, drawn before the folk are placed from a
//! stream of its own, so it never moves a wall, an item or a light. Each faction camp (all the faction's folk on the level)
//! gets a guard or two and a trade or two, about half the folk in all; trade-intention factions
//! get more traders. Tiers open by depth ([`TradeRules`]) and nobody rolls master: masters come
//! from the world's table ([`fb_faction::World::masters`]) and replace a trader of their
//! faction's camp.

use fb_faction::{FactionSpec, Intention, Masters, Profession, Style, Tier, Trade, TradeRules, World, armour_style};
use procgen_rng::Mulberry32;

use crate::{FolkSpot, Level};

const TRADE_SALT: u32 = 0x7BAD_E5A1;

/// The masters of the world of `world_seed`, placed on the levels of its run.
pub fn masters_of(world_seed: u32, rules: &TradeRules) -> Masters {
    World::new(world_seed).masters(rules)
}

/// The factions that place folk on `level`, in `level.factions` order.
fn held(level: &Level) -> Vec<fb_faction::Faction> {
    level.factions.iter().map(|f| f.id).filter(|id| level.folk.iter().any(|f| f.faction == *id)).collect()
}

/// The faction the world's master of `profession` on `depth` works for: generates the level
/// and reads whose camp the master stands in ([`place_masters`] resolves it). None where no such master is placed.
/// Costs a level, so a caller asks once and keeps the answer.
pub fn master_faction(world_seed: u32, depth: u32, profession: Profession) -> Option<fb_faction::Faction> {
    let master = *masters_of(world_seed, &TradeRules::DEFAULT).on(depth).find(|m| m.profession == profession)?;
    let level = crate::generate_in(world_seed, depth, &crate::LevelParams::for_depth(depth));
    level.folk.iter().find(|f| f.trade == Some(master.trade())).map(|f| f.faction)
}

/// The trades of each claim's camp, in claim order: `plans[k]` holds one entry per member claim
/// `k` places (`strengths[k]`), guards first, then traders, then nobody. Drawn before the folk
/// are placed, so a faction's market and its guards' posts follow from them ([`crate::roles`],
/// [`crate::folk`]).
pub(crate) fn plan(level: &Level, strengths: &[usize], layout_seed: u32) -> Vec<Vec<Option<Trade>>> {
    let rules = TradeRules::DEFAULT;
    let mut rng = Mulberry32::new(layout_seed ^ TRADE_SALT);
    level.claims.iter().zip(strengths).map(|(claim, &size)| camp_trades(level.faction(claim.faction), size, claim.rooms.len(), level.depth, &rules, &mut rng)).collect()
}

/// Puts each master the world places on this level among the folk, once they stand.
pub(crate) fn assign(level: &mut Level, world_seed: u32) {
    place_masters(level, &masters_of(world_seed, &TradeRules::DEFAULT));
}

/// The trades of a camp of `size` folk holding `rooms` rooms, in order: guards first, then
/// traders, then nobody. A claim of [`crate::roles::WORKSHOP_ROOMS`] rooms or more gives each
/// trade a workshop, so its camp keeps no more trades than it has rooms.
fn camp_trades(faction: &FactionSpec, size: usize, rooms: usize, depth: u32, rules: &TradeRules, rng: &mut Mulberry32) -> Vec<Option<Trade>> {
    if size == 0 {
        return Vec::new();
    }
    let extra = if faction.intention == Intention::Trade { rules.trade_faction_extra } else { 0.0 };
    // About half of the camp works: always a guard, and a trader for sure once the camp is four
    // strong. Claims make most camps small, and a trader in every camp of three would leave far
    // fewer than half the folk plain.
    let share = if size >= rules.large_camp { rules.large_camp_share } else { rules.trade_share };
    let wanted = (size as f64 * share + extra + rng.next_f64()).floor() as usize;
    let least = if size >= 4 { 2 } else { 1 };
    let working = wanted.clamp(least.min(size), size);
    let guards = 1 + (size >= 4 && working >= 3 && rng.chance(rules.second_guard_odds)) as usize;
    let working = if rooms >= crate::roles::WORKSHOP_ROOMS { working.min(guards + rooms) } else { working };

    let tiers = rules.tiers_at(depth);
    let mut offered: Vec<(Profession, u32)> = TRADER_WEIGHTS.to_vec();
    let mut out: Vec<Option<Trade>> = Vec::with_capacity(size);
    for k in 0..size {
        if k < guards {
            out.push(Some(Trade::guard()));
        } else if k < working {
            if offered.is_empty() {
                offered = TRADER_WEIGHTS.to_vec();
            }
            let profession = *rng.weighted(&offered);
            offered.retain(|(p, _)| *p != profession);
            let tier = *rng.weighted(&tiers);
            let style = match profession {
                Profession::Armorer => Some(armour_style(faction.armour, rules, rng)),
                Profession::Weaponsmith => Some(*rng.pick(&Style::WEAPONS)),
                _ => None,
            };
            out.push(Some(Trade { profession, tier, style }));
        } else {
            out.push(None);
        }
    }
    out
}

/// How often a trader is each trade.
const TRADER_WEIGHTS: [(Profession, u32); 7] = [
    (Profession::Armorer, 2),
    (Profession::Weaponsmith, 2),
    (Profession::Bowyer, 1),
    (Profession::Tentmaker, 1),
    (Profession::GeneralGoods, 3),
    (Profession::Alchemist, 1),
    (Profession::Enchanter, 1),
];

/// Puts each master placed on this level among the folk of its faction's camp: the faction is
/// `held[slot % held.len()]` among those that place folk here (the next held one if its folk
/// are all masters already). A master takes the place of a
/// trader, or failing that someone with no trade, or a guard if the camp has two.
fn place_masters(level: &mut Level, masters: &Masters) {
    let held = held(level);
    if held.is_empty() {
        return;
    }
    let mut taken: Vec<usize> = Vec::new();
    for master in masters.on(level.depth) {
        // The faction at the slot, or the next one held with a folk still free (two masters
        // may draw the same small camp).
        let camp_of = |faction| -> Vec<usize> { (0..level.folk.len()).filter(|&i| level.folk[i].faction == faction && !taken.contains(&i)).collect() };
        let Some(camp) = (0..held.len()).map(|k| camp_of(held[(master.slot as usize + k) % held.len()])).find(|c| !c.is_empty()) else { continue };
        let guards = camp.iter().filter(|&&i| is_guard(&level.folk[i])).count();
        // A trader of the master's own trade first, then any trader.
        let rank = |i: usize| match level.folk[i].trade {
            Some(t) if t.profession == master.profession => 0,
            Some(t) if t.profession != Profession::Guard => 1,
            None => 2,
            Some(_) if guards > 1 => 3,
            Some(_) => 4,
        };
        let Some(i) = camp.iter().copied().min_by_key(|&i| rank(i)) else { continue };
        let was = level.folk[i].trade.map(|t| t.profession);
        level.folk[i].trade = Some(master.trade());
        // A master who took the last trader of a trade into their workshop makes it their own.
        if let (Some(old), Some(room)) = (was, level.room_at(level.folk[i].x as i32, level.folk[i].y as i32)) {
            let faction = level.folk[i].faction;
            let still = level.folk.iter().any(|f| f.faction == faction && f.trade.is_some_and(|t| t.profession == old));
            let claim = level.claims.iter().find(|c| c.faction == faction).cloned();
            let taken = claim.as_ref().is_some_and(|c| level.workshop_of(c, master.profession).is_some());
            if !still && !taken {
                if let Some(role) = level.roles.get_mut(room).and_then(|r| r.iter_mut().find(|r| **r == crate::RoomRole::Workshop(old))) {
                    *role = crate::RoomRole::Workshop(master.profession);
                }
            }
        }
        taken.push(i);
    }
}

fn is_guard(f: &FolkSpot) -> bool {
    f.trade.is_some_and(|t| t.profession == Profession::Guard)
}

/// Whether `tier` may be met on `depth` by the tiers' rules (masters aside).
pub fn tier_open(tier: Tier, depth: u32, rules: &TradeRules) -> bool {
    match tier {
        Tier::Novice | Tier::Apprentice => true,
        Tier::Expert => depth >= rules.expert_from,
        Tier::Master => depth >= rules.master_from,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LevelParams, generate_in};
    use std::collections::HashSet;

    const WORLDS: std::ops::RangeInclusive<u32> = 1..=32;

    fn level(world: u32, depth: u32) -> Level {
        generate_in(world, depth, &LevelParams::for_depth(depth))
    }

    #[test]
    fn the_same_seed_gives_the_same_trades() {
        for world in [1, 1207, 99] {
            for depth in [1, 4, 8] {
                let (a, b) = (level(world, depth), level(world, depth));
                let trades = |l: &Level| l.folk.iter().map(|f| f.trade).collect::<Vec<_>>();
                assert_eq!(trades(&a), trades(&b));
            }
        }
    }

    #[test]
    fn nobody_is_better_than_an_apprentice_on_levels_one_and_two() {
        for world in WORLDS {
            for depth in 1..=2 {
                for f in level(world, depth).folk {
                    assert!(f.trade.is_none_or(|t| t.tier <= Tier::Apprentice), "world {world} depth {depth}: {f:?}");
                }
            }
        }
    }

    #[test]
    fn no_expert_above_level_three_and_no_master_above_level_six() {
        for world in WORLDS {
            for depth in 1..=5 {
                for f in level(world, depth).folk {
                    let tier = f.trade.map(|t| t.tier);
                    assert!(tier != Some(Tier::Master), "world {world} depth {depth}: {f:?}");
                    assert!(depth >= 3 || tier != Some(Tier::Expert), "world {world} depth {depth}: {f:?}");
                }
            }
        }
    }

    #[test]
    fn masters_are_one_to_a_trade_and_only_where_placed() {
        let rules = TradeRules::DEFAULT;
        let mut seen = 0;
        for world in WORLDS {
            let masters = masters_of(world, &rules);
            for depth in 1..=30 {
                let l = level(world, depth);
                let found: Vec<Trade> = l.folk.iter().filter_map(|f| f.trade).filter(|t| t.tier == Tier::Master).collect();
                let kinds: HashSet<Profession> = found.iter().map(|t| t.profession).collect();
                assert_eq!(kinds.len(), found.len(), "world {world} depth {depth}: two masters of a trade");
                let placed: HashSet<Profession> = masters.on(depth).map(|m| m.profession).collect();
                assert_eq!(kinds, placed, "world {world} depth {depth}");
                seen += found.len();
            }
        }
        assert!(seen > 20, "{seen} masters");
    }

    #[test]
    fn every_placed_master_is_there_with_the_right_faction() {
        let rules = TradeRules::DEFAULT;
        for world in WORLDS {
            for master in masters_of(world, &rules).all() {
                assert!(master.depth >= rules.master_from);
                let l = level(world, master.depth);
                let faction = master_faction(world, master.depth, master.profession).expect("placed");
                let there = l.folk.iter().filter(|f| f.faction == faction && f.trade == Some(master.trade())).count();
                assert_eq!(there, 1, "world {world}: {master:?} is not on level {}", master.depth);
            }
        }
    }

    #[test]
    fn nobody_is_retagged_into_another_camp() {
        // TODO: tighten to faction claims once they land (#541): every folk stands on its faction's ground.
        for world in WORLDS {
            for depth in [6, 7, 8, 9, 10, 13] {
                let l = level(world, depth);
                let mut by_room = std::collections::HashMap::new();
                for f in &l.folk {
                    let room = l.room_at(f.x as i32, f.y as i32);
                    assert_eq!(*by_room.entry(room).or_insert(f.faction), f.faction, "world {world} depth {depth}: two factions in one room");
                }
            }
        }
    }

    #[test]
    fn about_half_the_folk_have_no_trade_and_every_camp_has_a_guard() {
        let (mut folk, mut plain) = (0, 0);
        for world in WORLDS {
            for depth in 1..=5 {
                let l = level(world, depth);
                for spec in &l.factions {
                    let camp: Vec<_> = l.folk.iter().filter(|f| f.faction == spec.id).collect();
                    if camp.is_empty() {
                        continue;
                    }
                    assert!(camp.iter().any(|f| is_guard(f)), "world {world} depth {depth}: {} has no guard", spec.name);
                    if camp.len() >= 4 {
                        assert!(camp.iter().any(|f| f.trade.is_some_and(|t| t.profession != Profession::Guard)), "world {world} depth {depth}: {} has no trader", spec.name);
                    }
                }
                folk += l.folk.len();
                plain += l.folk.iter().filter(|f| f.trade.is_none()).count();
            }
        }
        let share = plain as f64 / folk as f64;
        assert!((0.3..=0.6).contains(&share), "{plain} of {folk} folk have no trade");
    }

    #[test]
    fn armorers_lean_to_their_factions_armour() {
        let (mut own, mut all) = (0, 0);
        for world in WORLDS {
            for depth in 1..=5 {
                let l = level(world, depth);
                for f in &l.folk {
                    if let Some(Trade { profession: Profession::Armorer, style: Some(s), .. }) = f.trade {
                        all += 1;
                        own += (s == Style::of_lean(l.faction(f.faction).armour)) as u32;
                    }
                }
            }
        }
        assert!(all > 10 && own * 10 >= all * 6, "{own} of {all}");
    }
}
