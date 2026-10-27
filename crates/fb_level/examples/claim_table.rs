//! Prints each faction's folk, rooms and distinct trades on a few levels:
//! `cargo run -p fb_level --example claim_table -- <depth> <seed>...`.

use fb_faction::Profession;
use std::collections::HashSet;

fn main() {
    let mut args = std::env::args().skip(1);
    let depth: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1);
    let seeds: Vec<u32> = args.filter_map(|a| a.parse().ok()).collect();
    for seed in seeds {
        let level = fb_level::generate(seed, depth);
        let mut line = Vec::new();
        for claim in &level.claims {
            let folk: Vec<_> = level.folk.iter().filter(|f| f.faction == claim.faction).collect();
            let trades: HashSet<Profession> = folk.iter().filter_map(|f| f.trade).map(|t| t.profession).filter(|p| *p != Profession::Guard).collect();
            line.push(format!("{}f/{}r/{}t", folk.len(), claim.rooms.len(), trades.len()));
        }
        println!("depth {depth:>2} seed {seed:>5}: {:>2} folk, {:>2}/{:>2} rooms claimed | {}", level.folk.len(), level.claims.iter().map(|c| c.rooms.len()).sum::<usize>(), level.rooms.len(), line.join("  "));
    }
}
