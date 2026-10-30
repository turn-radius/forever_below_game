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
    place_healers(level);
}

const HEALER_SALT: u32 = 0x4EA1_C1E7;

/// Settles each faction's healer, who is always a cleric. Camps plan a healer ([`camp_trades`]), so
/// most have one. Where the planned healer's class ([`FolkSpot::class`]) is not cleric, the trade
/// passes to a plain folk of the camp whose class is, if there is one; else that folk is made a
/// cleric ([`FolkSpot::cleric`]). A camp left with no healer (a master took theirs, or the room
/// had no place for them) makes one of the folk by the same rules: a plain folk first, then a
/// trader whose trade another of the camp keeps, then one who is the camp's only of their trade,
/// then a guard of a camp with two, and only last the camp's one guard. Masters stay as they are.
/// Ties go to the lowest seed, so it is a pure function of the level.
fn place_healers(level: &mut Level) {
    let cleric = Some(fb_faction::ClassKind::Cleric);
    for faction in held(level) {
        let spec = level.faction(faction).clone();
        if spec.is_denizen() {
            continue;
        }
        let camp: Vec<usize> = (0..level.folk.len()).filter(|&i| level.folk[i].faction == faction).collect();
        let is_healer = |f: &FolkSpot| f.trade.is_some_and(|t| t.profession == Profession::Healer);
        let healers: Vec<usize> = camp.iter().copied().filter(|&i| is_healer(&level.folk[i])).collect();
        if let Some(&first) = healers.first() {
            if healers.iter().any(|&i| level.folk[i].class(&spec) == cleric) {
                continue;
            }
            let plain_cleric = camp.iter().copied().filter(|&i| level.folk[i].trade.is_none() && level.folk[i].class(&spec) == cleric).min_by_key(|&i| level.folk[i].seed);
            match plain_cleric {
                Some(j) => {
                    level.folk[j].trade = level.folk[first].trade;
                    level.folk[first].trade = None;
                }
                None => level.folk[first].cleric = true,
            }
            continue;
        }
        let guards = camp.iter().filter(|&&i| is_guard(&level.folk[i])).count();
        let rank = |i: usize| match level.folk[i].trade {
            Some(t) if t.tier == Tier::Master => None,
            None => Some(0),
            Some(t) if t.profession == Profession::Guard => Some(if guards > 1 { 3 } else { 4 }),
            Some(t) => Some(if camp.iter().any(|&j| j != i && level.folk[j].trade.is_some_and(|u| u.profession == t.profession)) { 1 } else { 2 }),
        };
        // Rank first; a cleric class only breaks a tie.
        let Some((_, _, _, i)) = camp.iter().copied().filter_map(|i| rank(i).map(|r| (r, level.folk[i].class(&spec) != cleric, level.folk[i].seed, i))).min() else { continue };
        let tiers = TradeRules::DEFAULT.tiers_at(level.depth);
        let tier = *Mulberry32::new(level.folk[i].seed ^ HEALER_SALT).weighted(&tiers);
        level.folk[i].cleric = level.folk[i].class(&spec) != cleric;
        level.folk[i].trade = Some(Trade { profession: Profession::Healer, tier, style: None });
    }
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

    // Every camp keeps a healer: the first trader after the guards, who needs no workshop. A camp of
    // one is its healer alone.
    let healer = !faction.is_denizen();
    let guards = if size == 1 && healer { 0 } else { guards };
    let working = if healer { working.max((guards + 1).min(size)) } else { working };

    let tiers = rules.tiers_at(depth);
    let mut offered: Vec<(Profession, u32)> = TRADER_WEIGHTS.to_vec();
    let mut out: Vec<Option<Trade>> = Vec::with_capacity(size);
    for k in 0..size {
        if k < guards {
            out.push(Some(Trade::guard()));
        } else if healer && k == guards {
            out.push(Some(Trade { profession: Profession::Healer, tier: *rng.weighted(&tiers), style: None }));
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
        // A trader of the master's own trade first, then any trader. Never a healer, and never the
        // camp's only guard: a camp with no one else to give up passes the master to the next.
        let pick = |camp: Vec<usize>| -> Option<usize> {
            let guards = camp.iter().filter(|&&i| is_guard(&level.folk[i])).count();
            let rank = |i: usize| match level.folk[i].trade {
                Some(t) if t.profession == Profession::Healer => None,
                Some(t) if t.profession == master.profession => Some(0),
                Some(t) if t.profession != Profession::Guard => Some(1),
                None => Some(2),
                Some(_) if guards > 1 => Some(4),
                Some(_) => None,
            };
            camp.iter().copied().filter_map(|i| rank(i).map(|r| (r, i))).min().map(|(_, i)| i)
        };
        let Some(i) = (0..held.len()).find_map(|k| pick(camp_of(held[(master.slot as usize + k) % held.len()]))) else { continue };
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
                    // A camp of one, whose one is its healer, keeps no guard.
                    let lone_healer = camp.len() == 1 && camp[0].trade.is_some_and(|t| t.profession == Profession::Healer);
                    assert!(lone_healer || camp.iter().any(|f| is_guard(f)), "world {world} depth {depth}: {} has no guard", spec.name);
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
    fn every_faction_keeps_a_healer_and_every_healer_is_a_cleric() {
        let (mut camps, mut made) = (0, 0);
        for world in WORLDS {
            for depth in [1, 2, 3, 5, 8, 12] {
                let l = level(world, depth);
                for spec in l.factions.iter().filter(|f| !f.is_denizen()) {
                    let camp: Vec<_> = l.folk.iter().filter(|f| f.faction == spec.id).collect();
                    if camp.is_empty() {
                        continue;
                    }
                    camps += 1;
                    let healers: Vec<_> = camp.iter().filter(|f| f.trade.is_some_and(|t| t.profession == Profession::Healer)).collect();
                    assert!(!healers.is_empty(), "world {world} depth {depth}: {} has no healer", spec.name);
                    made += healers.iter().filter(|f| f.cleric).count();
                }
                for f in &l.folk {
                    let healer = f.trade.is_some_and(|t| t.profession == Profession::Healer);
                    let cleric = f.class(l.faction(f.faction)) == Some(fb_faction::ClassKind::Cleric);
                    assert!(!healer || cleric, "world {world} depth {depth}: a healer who is not a cleric: {f:?}");
                    assert!(healer || !f.cleric, "world {world} depth {depth}: a cleric made for nothing: {f:?}");
                    assert!(!healer || f.trade.is_some_and(|t| t.tier != Tier::Master), "a master healer");
                }
            }
        }
        assert!(camps > 100, "{camps} camps");
        eprintln!("{made} of {camps} camps made a cleric of one of their folk");
    }

    #[test]
    fn a_master_displaces_neither_a_healer_nor_a_camps_only_guard() {
        let rules = TradeRules::DEFAULT;
        let mut checked = 0;
        for world in WORLDS {
            let masters = masters_of(world, &rules);
            for depth in 6..=14 {
                let mut l = level(world, depth);
                let Some(faction) = held(&l).into_iter().find(|f| l.folk.iter().filter(|x| x.faction == *f).count() >= 2) else { continue };
                // Make the camp a guard and a healer alone, then place every master of the level.
                l.folk.retain(|f| f.faction != faction || f.trade.is_some_and(|t| t.profession == Profession::Guard || t.profession == Profession::Healer));
                let mut camp: Vec<usize> = (0..l.folk.len()).filter(|&i| l.folk[i].faction == faction).collect();
                camp.truncate(2);
                if camp.len() < 2 || !is_guard(&l.folk[camp[0]]) == !is_guard(&l.folk[camp[1]]) {
                    continue;
                }
                let before: Vec<_> = camp.iter().map(|&i| l.folk[i].trade).collect();
                place_masters(&mut l, &masters);
                let after: Vec<_> = camp.iter().map(|&i| l.folk[i].trade).collect();
                if masters.on(depth).next().is_some() {
                    checked += 1;
                }
                assert_eq!(before, after, "world {world} depth {depth}: a master displaced the guard or healer");
            }
        }
        assert!(checked > 3, "{checked} camps checked");
    }

    #[test]
    fn no_camp_with_two_folk_who_are_not_masters_loses_its_last_guard() {
        for world in WORLDS {
            for depth in [1, 2, 3, 6, 7, 9, 12] {
                let l = level(world, depth);
                for spec in l.factions.iter().filter(|f| !f.is_denizen()) {
                    let camp: Vec<_> = l.folk.iter().filter(|f| f.faction == spec.id).collect();
                    let others = camp.iter().filter(|f| f.trade.is_none_or(|t| t.tier != Tier::Master)).count();
                    if others >= 2 {
                        assert!(camp.iter().any(|f| is_guard(f)), "world {world} depth {depth}: {} lost its guard", spec.name);
                    }
                }
            }
        }
    }

    #[test]
    fn every_healer_stocks_three_incenses_and_two_censers_and_nobody_else_does() {
        use fb_items::stock::{Line, stock, stock_with};
        let (mut healers, mut others) = (0, 0);
        for world in 1..=6 {
            for depth in [1, 3, 6] {
                let l = level(world, depth);
                for f in l.folk.iter().filter_map(|f| f.trade.map(|t| (f, t))) {
                    let Some(shop) = f.1.shop("Bera", "the Ash Choir") else { continue };
                    if f.1.profession == Profession::Healer {
                        assert_eq!(shop.line, Line::Sacred);
                        let wares = stock(&shop, depth, f.0.seed);
                        let mut kinds: Vec<_> = wares.iter().filter_map(|i| i.incense_kind()).map(|k| k.name()).collect();
                        kinds.sort();
                        kinds.dedup();
                        assert!(kinds.len() >= 3 && wares.iter().filter(|i| i.is_censer()).count() >= 2, "world {world} depth {depth}");
                        assert_eq!(wares, stock(&shop, depth, f.0.seed));
                        let pool = [("heal".to_string(), 3.0), ("cure".to_string(), 2.0)];
                        let scrolls = stock_with(&shop, depth, f.0.seed, &pool).iter().filter(|i| i.prayer().is_some()).count();
                        assert!((1..=2).contains(&scrolls), "world {world} depth {depth}: {scrolls} prayer scrolls");
                        healers += 1;
                    } else {
                        assert_ne!(shop.line, Line::Sacred);
                        others += 1;
                    }
                }
            }
        }
        assert!(healers >= 18 && others > 0, "{healers} healers, {others} others");
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
