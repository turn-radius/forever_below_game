//! Prints the economy by depth: the coins a delver finds on a level (loose purses and the
//! purses in containers) beside what a master's rare sword and rare mail cost.
//! `cargo run -p fb_level --example coins_by_depth -- [levels per depth]`
//! The table goes in `docs/items.md`, "Value".

use fb_items::{Coins, Odds, Rarity, roll_contents, roll_item_with, roll_loot};
use fb_level::{Prop, generate};

fn median(v: &mut [u64]) -> u64 {
    v.sort_unstable();
    v.get(v.len() / 2).copied().unwrap_or(0)
}

fn fmt(cp: u64) -> String {
    Coins::from_copper(cp).short()
}

fn main() {
    let levels: u32 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(40);
    println!("{:>5} {:>12} {:>12} {:>12} {:>14} {:>14}", "depth", "mean coins", "median", "cumulative", "rare sword", "rare mail");
    let mut cumulative = 0.0;
    for depth in 1..=10u32 {
        let odds = Odds::loot(depth);
        let mut found: Vec<u64> = Vec::new();
        for seed in 1..=levels {
            let level = generate(seed + depth * 1000, depth);
            let mut cp: u64 = level.items.iter().filter_map(|s| roll_loot(s.family, s.seed, odds)).filter_map(|i| i.coins).map(|c| c.total_copper()).sum();
            for prop in &level.props {
                if let Prop::Container { style, loot, .. } = prop.prop {
                    cp += roll_contents(style, loot, odds).iter().filter_map(|i| i.coins).map(|c| c.total_copper()).sum::<u64>();
                }
            }
            found.push(cp);
        }
        let mean = found.iter().sum::<u64>() as f64 / found.len() as f64;
        cumulative += mean;
        // A master's rare piece rolls three levels deeper than the level, as the shelf does.
        let master = Odds { depth: depth + 3, cap: Rarity::Rare };
        let price = |base: &str| {
            let mut v: Vec<u64> = (0..1500u32).filter_map(|s| roll_item_with(base, s, master)).filter(|i| i.rarity == Rarity::Rare).take(60).map(|i| i.value_copper()).collect();
            median(&mut v)
        };
        println!("{depth:>5} {:>12} {:>12} {:>12} {:>14} {:>14}", fmt(mean as u64), fmt(median(&mut found)), fmt(cumulative as u64), fmt(price("Sword")), fmt(price("Mail")));
    }
}
