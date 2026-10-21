//! Preview tool: prints a world's master placements and each folk's trade on levels 1 to 8.
//! `cargo run -p fb_level --example trades -- <world seed> [deepest level]`

use fb_faction::{Profession, TradeRules};
use fb_level::{LevelParams, LevelStyle, generate_in};

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1207);
    let deepest: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(8);
    let world = fb_faction::World::new(seed);
    let masters = fb_level::trades::masters_of(seed, &TradeRules::DEFAULT);
    println!("world {seed}: masters");
    for p in Profession::TRADES {
        let at: Vec<String> = masters
            .of(p)
            .map(|m| {
                let faction = fb_level::trades::master_faction(seed, m.depth, p).map(|f| world.faction(f).name.clone()).unwrap_or_default();
                format!("L{} {}{}", m.depth, faction, m.style.map(|s| format!(" ({})", s.label())).unwrap_or_default())
            })
            .collect();
        println!("  {:<22} {}", p.label(), at.join("; "));
    }
    for depth in 1..=deepest {
        let level = generate_in(seed, depth, &LevelParams::for_style(LevelStyle::for_depth(depth)));
        println!("level {depth}: {} folk", level.folk.len());
        for f in &level.folk {
            let trade = f.trade.map(|t| t.label()).unwrap_or_else(|| "-".into());
            println!("  {:<40} {:?} of {}", trade, f.folk, level.faction(f.faction).name);
        }
    }
}
