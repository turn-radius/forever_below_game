//! Preview tool: prints a trader's shelf with values.
//! `cargo run -p fb_faction --example stock -- <profession> <tier> [style] [depth] [seed] [maker] [faction]`
//! for example `-- armorer master mail 8 7 Bera "the Ash Choir"` or `-- general_goods novice - 1`.
//! A profession is `armorer`, `weaponsmith`, `bowyer`, `tentmaker`, `general_goods` or
//! `alchemist`; a tier `novice`, `apprentice`, `expert` or `master`; a style as `Style::label`
//! with underscores (`mail`, `blades`, `axes_and_clubs`), or `-` for none.

use fb_faction::{Profession, Style, Tier, Trade};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize, default: &str| args.get(i).cloned().unwrap_or_else(|| default.to_string());
    let key = |label: &str| label.replace(' ', "_");
    let profession = Profession::TRADES.into_iter().find(|p| key(p.label()) == arg(0, "general_goods") || key(p.label()).starts_with(&arg(0, "general_goods"))).expect("a profession");
    let tier = Tier::ALL.into_iter().find(|t| t.label() == arg(1, "novice")).expect("a tier");
    let wanted = arg(2, "-");
    let style = Style::ARMOUR.into_iter().chain(Style::WEAPONS).find(|s| key(s.label()) == wanted);
    let depth: u32 = arg(3, "1").parse().expect("a depth");
    let seed: u32 = arg(4, "1").parse().expect("a seed");
    let trade = Trade { profession, tier, style };
    let shop = trade.shop(&arg(5, "Bera"), &arg(6, "the Ash Choir")).expect("a trade that sells");
    println!("{} at depth {depth}, seed {seed}", trade.label());
    for item in fb_items::stock::stock(&shop, depth, seed) {
        println!("  {:<34} {:<8} {:>10}   {}", item.title(), format!("{:?}", item.rarity), item.value().short(), item.history.join(" "));
    }
}
