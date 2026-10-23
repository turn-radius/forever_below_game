//! Kills move standing (#547). Each creature or folk the delver brings to 0 hit points
//! ([`Slain`]) is weighed by every faction of the world: one that hates the kind thinks better of
//! the delver, one that likes it worse, each kill of a kind counting for less than the last
//! ([`fb_faction::Standings::on_kill`]). Killing a folk also tells their own faction, and those
//! that like or hate their faction, folk or dress. Striking a member is `crate::hostility`'s.
//!
//! `FB_KILLS=spider:6,rat:3` (dev) applies kills at the start of a delve, so captures can show
//! the factions section after a run of them; it also marks the factions that care as met.

use bevy::prelude::*;
use fb_creatures::Species;
use fb_faction::{CreatureKind, Outfit, Victim};

use crate::combat::feedback::Slain;
use crate::creatures::Critter;
use crate::folk::Denizen;
use crate::launch::AppState;
use crate::play::DelveSystems;
use crate::record::WorldRecord;
use crate::travel::{Abyss, begin_the_run};

pub struct KillsPlugin;

impl Plugin for KillsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Slain>().add_systems(Update, kills_move_standing.in_set(DelveSystems)).add_systems(OnEnter(AppState::Delving), apply_dev_kills.after(begin_the_run));
    }
}

/// The kind of creature factions know a species as.
pub fn kind_of(species: Species) -> CreatureKind {
    match species {
        Species::Rat | Species::GiantRat => CreatureKind::Rats,
        Species::PoisonousSnake | Species::ConstrictorSnake => CreatureKind::Snakes,
        Species::Skeleton => CreatureKind::Undead,
        Species::GiantSpider => CreatureKind::Spiders,
        Species::Troll => CreatureKind::Trolls,
        Species::Jackalope => CreatureKind::Jackalopes,
        Species::GiantCentipede => CreatureKind::Centipedes,
        Species::Bat | Species::GiantBat => CreatureKind::Bats,
    }
}

/// Reads a dev kind word ("spider", "rats") as a kind.
fn kind_from_word(word: &str) -> Option<CreatureKind> {
    let word = word.trim().to_ascii_lowercase();
    let word = word.trim_end_matches('s');
    CreatureKind::ALL.into_iter().find(|k| format!("{k:?}").to_ascii_lowercase().trim_end_matches('s') == word)
}

/// What a folk is to the factions that weigh their death: their faction, folk and class.
pub fn victim_of(denizen: &Denizen) -> Victim {
    let person = &denizen.person;
    let class = [Outfit::Fighter, Outfit::Ranger, Outfit::Mage, Outfit::Cleric, Outfit::Scavenger].into_iter().find(|o| o.key() == person.outfit).and_then(Outfit::class);
    Victim::Member { faction: person.faction.id, folk: Some(person.spec.folk), class }
}

/// Weighs each of the delver's kills this frame against every faction.
pub fn kills_move_standing(mut slain: MessageReader<Slain>, abyss: Option<Res<Abyss>>, mut record: ResMut<WorldRecord>, creatures: Query<&Critter>, folk: Query<&Denizen>) {
    let victims: Vec<Victim> = slain
        .read()
        .filter(|s| !s.by_ally)
        .filter_map(|s| match (creatures.get(s.target), folk.get(s.target)) {
            (Ok(critter), _) => Some(Victim::Creature(kind_of(critter.creature.species()))),
            (_, Ok(denizen)) => Some(victim_of(denizen)),
            _ => None,
        })
        .collect();
    let Some(abyss) = abyss else { return };
    if victims.is_empty() {
        return;
    }
    weigh(&fb_faction::World::new(abyss.run_seed), &mut record.standings, &victims);
}

/// Every faction of `world` weighs each of `victims` in turn.
pub fn weigh(world: &fb_faction::World, standings: &mut fb_faction::Standings, victims: &[Victim]) {
    for victim in victims {
        standings.on_kill(world, *victim);
    }
}

/// `FB_KILLS`: kills to apply at the start of a delve, as `kind:count` pairs.
fn apply_dev_kills(abyss: Option<Res<Abyss>>, record: Option<ResMut<WorldRecord>>) {
    let (Ok(spec), Some(abyss), Some(mut record)) = (std::env::var("FB_KILLS"), abyss, record) else { return };
    let world = fb_faction::World::new(abyss.run_seed);
    for pair in spec.split(',') {
        let Some((word, count)) = pair.split_once(':') else { continue };
        let (Some(kind), Ok(count)) = (kind_from_word(word), count.trim().parse::<u32>()) else { continue };
        for _ in 0..count {
            record.standings.on_kill(&world, Victim::Creature(kind));
        }
        for f in world.factions() {
            let thing = fb_faction::Preference::Creature(kind);
            if f.likes.contains(&thing) || f.hates.contains(&thing) {
                record.standings.meet(f.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_species_has_a_kind_and_words_read() {
        assert_eq!(kind_of(Species::GiantRat), CreatureKind::Rats);
        assert_eq!(kind_of(Species::Skeleton), CreatureKind::Undead);
        assert_eq!(kind_from_word("spider"), Some(CreatureKind::Spiders));
        assert_eq!(kind_from_word("Rats"), Some(CreatureKind::Rats));
        assert_eq!(kind_from_word("dragon"), None);
    }

    #[test]
    fn weighing_a_spider_kill_and_a_member_kill_moves_the_factions_that_care() {
        let spiders = fb_faction::Preference::Creature(CreatureKind::Spiders);
        let world = (0..500).map(fb_faction::World::new).find(|w| w.factions().iter().any(|f| f.likes.contains(&spiders))).unwrap();
        let liker = world.factions().iter().find(|f| f.likes.contains(&spiders)).unwrap().id;
        let mut standings = fb_faction::Standings::default();
        weigh(&world, &mut standings, &[Victim::Creature(CreatureKind::Spiders)]);
        assert!(standings.get(liker).value() < 0 && !standings.has_met(liker), "they hear of it, without meeting the delver");
        let member = world.factions().iter().map(|f| f.id).find(|f| *f != liker).unwrap();
        let before = standings.get(member).value();
        weigh(&world, &mut standings, &[Victim::Member { faction: member, folk: Some(world.faction(member).folk), class: None }]);
        assert!(standings.get(member).value() <= before - 40 + 10 && standings.has_met(member));
    }

    #[test]
    fn a_kill_by_an_allied_folk_moves_no_standing_and_the_delvers_does() {
        let level = fb_level::generate(1207, 1);
        let spot = level.creatures[0];
        let kind = kind_of(spot.species);
        let thing = fb_faction::Preference::Creature(kind);
        let run_seed = (0..500).find(|s| fb_faction::World::new(*s).factions().iter().any(|f| f.likes.contains(&thing) || f.hates.contains(&thing))).expect("a world whose factions care");
        let standings_after = |by_ally: bool| {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins).add_message::<Slain>().init_resource::<WorldRecord>().insert_resource(Abyss { run_seed, ..Default::default() }).add_systems(Update, kills_move_standing);
            let target = app.world_mut().spawn(Critter::new(spot, spot.creature())).id();
            app.world_mut().write_message(Slain { target, by_ally });
            app.update();
            app.world().resource::<WorldRecord>().standings.clone()
        };
        assert_eq!(standings_after(true), fb_faction::Standings::default(), "an ally's kill counted");
        assert_ne!(standings_after(false), fb_faction::Standings::default(), "the delver's kill did not");
    }
}
