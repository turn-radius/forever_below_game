//! Whether a folk will fight the delver, and how a faction turns on them.
//!
//! - Every folk is [`Hostility::Friendly`] until given cause. Nothing a friendly folk does hurts
//!   the delver, and walking into or near them gives no cause.
//! - Cause is a [`Provoked`] message, which anything may send: a blow aimed at a folk does
//!   (`crate::folk_combat`), and catching the delver stealing or a talk gone wrong can later.
//! - A provoked folk turns [`Hostility::Hostile`], and so do the kin who see it happen: the
//!   folk of the same faction in the same room (or, out in a corridor, within [`SIGHT`]) with a
//!   clear line to them (`fb_level::path::line_of_sight`, as creatures notice the delver)
//!   ([`who_turns`]). Other factions stay friendly. "The Ash Choir turn on you" shows low in the
//!   view.
//! - Hostile stays hostile: nothing here turns anyone back, and the record keeps it for the rest
//!   of the run on that level (`crate::record`).
//!
//! Who sees, and the reach of sight in a corridor, are our own.

use bevy::prelude::*;
use fb_level::path::line_of_sight;
use fb_level::{CELL, Faction, Level};

use crate::combat::feedback::Corpse;
use crate::folk::Denizen;
use crate::inventory::Toasts;
use crate::play::{BlockedCells, CurrentLevel, DelveSystems};
use crate::player::blocked_by;

pub struct HostilityPlugin;

impl Plugin for HostilityPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Provoked>().init_resource::<BlockedCells>().add_systems(Update, turn_on_the_delver.in_set(DelveSystems));
    }
}

/// Whether a folk will fight the delver.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hostility {
    #[default]
    Friendly,
    Hostile,
}

impl Hostility {
    pub fn is_hostile(self) -> bool {
        self == Hostility::Hostile
    }
}

/// The delver has given `folk` cause to fight: they, and the kin who see them, turn hostile.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Provoked {
    pub folk: Entity,
}

/// How far a folk out of any room sees a kinsman provoked, world units (8 cells).
pub const SIGHT: f32 = 8.0 * CELL;

/// A folk as [`who_turns`] weighs them: who, of which faction, standing where (world `(x, z)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kin {
    pub entity: Entity,
    pub faction: Faction,
    pub at: Vec2,
}

fn grid(p: Vec2) -> (f32, f32) {
    (p.x / CELL, p.y / CELL)
}

fn room_of(level: &Level, p: Vec2) -> Option<usize> {
    level.room_at((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

/// Who turns on the delver when `provoked` is given cause, of `folk` (everyone living, the
/// provoked among them): the provoked themselves, and each of their faction in the same room
/// (within [`SIGHT`] when the provoked stands in no room) with a clear line to them across
/// `blocked`. A pure function of where everyone stands.
pub fn who_turns(provoked: Entity, folk: &[Kin], level: &Level, blocked: impl Fn(i32, i32) -> bool) -> Vec<Entity> {
    let Some(them) = folk.iter().find(|k| k.entity == provoked) else { return Vec::new() };
    let room = room_of(level, them.at);
    let mut turned = vec![provoked];
    for kin in folk.iter().filter(|k| k.entity != provoked && k.faction == them.faction) {
        let near = match room {
            Some(room) => room_of(level, kin.at) == Some(room),
            None => kin.at.distance(them.at) <= SIGHT,
        };
        if near && line_of_sight(grid(kin.at), grid(them.at), &blocked) {
            turned.push(kin.entity);
        }
    }
    turned
}

/// The line shown when `faction` turns on the delver: "The Ash Choir turn on you".
pub fn turn_line(faction: Faction) -> String {
    let name = faction.name();
    let mut letters = name.chars();
    let capital: String = letters.next().map(|c| c.to_uppercase().chain(letters).collect()).unwrap_or_default();
    format!("{capital} turn on you")
}

/// Turns each provoked folk, and the kin who see it, hostile ([`who_turns`]), and says so once a
/// faction turns.
pub fn turn_on_the_delver(
    mut provoked: MessageReader<Provoked>,
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    mut toasts: Toasts,
    mut folk: Query<(Entity, &Denizen, &Transform, &mut Hostility), Without<Corpse>>,
) {
    let causes: Vec<Entity> = provoked.read().map(|p| p.folk).collect();
    if causes.is_empty() {
        return;
    }
    let blocked = blocked_by(&level.0, &blocked_cells);
    let everyone: Vec<Kin> = folk.iter().map(|(entity, d, t, _)| Kin { entity, faction: d.person.spot.faction, at: t.translation.xz() }).collect();
    for cause in causes {
        let mut newly = false;
        let mut faction = None;
        for entity in who_turns(cause, &everyone, &level.0, &blocked) {
            let Ok((_, denizen, _, mut hostility)) = folk.get_mut(entity) else { continue };
            if !hostility.is_hostile() {
                *hostility = Hostility::Hostile;
                newly = true;
                faction = Some(denizen.person.spot.faction);
                info!("{} of {} turns hostile", denizen.person.name, denizen.person.spot.faction.name());
            }
        }
        if let (true, Some(faction)) = (newly, faction) {
            toasts.add(&turn_line(faction));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_names_the_faction_with_a_capital() {
        let choir = Faction::all().find(|f| f.name() == "the Ash Choir").unwrap();
        assert_eq!(turn_line(choir), "The Ash Choir turn on you");
        let keepers = Faction::all().find(|f| f.name() == "the Lantern-keepers").unwrap();
        assert_eq!(turn_line(keepers), "The Lantern-keepers turn on you");
    }

    #[test]
    fn kin_in_the_room_who_see_it_turn_and_others_do_not() {
        let level = fb_level::generate(1207, 1);
        let open = |x: i32, y: i32| !level.is_open(x, y);
        // The level's folk, as they stand at their spots.
        let folk: Vec<Kin> = level
            .folk
            .iter()
            .enumerate()
            .map(|(i, s)| Kin { entity: Entity::from_raw_u32(i as u32 + 1).unwrap(), faction: s.faction, at: Vec2::new(s.x, s.y) * CELL })
            .collect();
        let first = folk[0];
        let turned = who_turns(first.entity, &folk, &level, open);
        assert_eq!(turned[0], first.entity, "the provoked turn first");
        let room = room_of(&level, first.at);
        for kin in &folk[1..] {
            let same = kin.faction == first.faction && room_of(&level, kin.at) == room && room.is_some();
            assert_eq!(turned.contains(&kin.entity), same && line_of_sight(grid(kin.at), grid(first.at), open), "{kin:?}");
            if kin.faction != first.faction {
                assert!(!turned.contains(&kin.entity), "another faction turned");
            }
        }
        // A group stands together, one faction to a room, so someone else turns with them.
        assert!(turned.len() > 1, "nobody saw it: {turned:?}");
        // Of another faction, standing right beside, they stay friendly.
        let mut stranger = first;
        stranger.entity = Entity::from_raw_u32(999).unwrap();
        stranger.faction = Faction::all().find(|f| *f != first.faction).unwrap();
        stranger.at += Vec2::X;
        let with_stranger: Vec<Kin> = folk.iter().copied().chain([stranger]).collect();
        assert!(!who_turns(first.entity, &with_stranger, &level, open).contains(&stranger.entity));
    }
}
