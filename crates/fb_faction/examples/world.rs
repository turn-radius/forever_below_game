//! Preview tool: prints a world's pool of factions.
//! `cargo run -p fb_faction --example world -- <world seed> [level seed ...]`
//!
//! With level seeds, also lists the factions each of those levels draws.

use fb_faction::{Preference, World};

fn describe(p: &Preference, world: &World) -> String {
    match p {
        Preference::Creature(k) => format!("{k:?}").to_lowercase(),
        Preference::Class(c) => format!("{c:?}").to_lowercase() + "s",
        Preference::Folk(f) => format!("{f:?}").to_lowercase(),
        Preference::Faction(f) => world.faction(*f).name.clone(),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1207);
    let world = World::new(seed);
    println!("world {seed}: {} factions", world.factions().len());
    for f in world.factions() {
        let list = |ps: &[Preference]| ps.iter().map(|p| describe(p, &world)).collect::<Vec<_>>().join(", ");
        println!(
            "{:>2} {}\n     {:?}, {}, {:?}{}\n     outfits {} and {}, {:?} armour\n     field {:?} {:?} on {:?}, crest {:?} {:?}\n     likes {}\n     hates {}",
            f.id.0,
            f.name,
            f.folk,
            f.alignment.label(),
            f.intention,
            if f.insular { ", insular" } else { "" },
            f.outfits[0].key(),
            f.outfits[1].key(),
            f.armour,
            f.field.division,
            f.field.tincture,
            f.field.tincture2,
            f.field.charge,
            f.field.charge_tincture,
            list(&f.likes),
            list(&f.hates),
        );
    }
    for level in args.filter_map(|s| s.parse::<u32>().ok()) {
        let names: Vec<&str> = world.draw(level).into_iter().map(|id| world.faction(id).name.as_str()).collect();
        println!("level {level}: {}", names.join("; "));
    }
}
