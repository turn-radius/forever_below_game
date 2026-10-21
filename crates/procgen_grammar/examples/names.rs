//! Preview tool: prints faction names for every folk and intention over a range of seeds.
//!
//! `cargo run -p procgen_grammar --example names [first_seed] [count]`
//! Defaults to seeds 0..8. Edit `data/faction_names.json` and run again.

use procgen_grammar::faction_name;

const FOLK: [&str; 6] = ["human", "dwarf", "gnome", "goblin", "lizardfolk", "pigfolk"];
const INTENTIONS: [&str; 4] = ["way_out", "trade", "worship", "plunder"];

fn main() {
    let mut args = std::env::args().skip(1);
    let first: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(0);
    let count: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(8);
    for folk in FOLK {
        for intent in INTENTIONS {
            println!("{folk} / {intent}");
            for seed in first..first + count {
                println!("  {seed:>4}  {}", faction_name(folk, intent, seed));
            }
        }
    }
}
