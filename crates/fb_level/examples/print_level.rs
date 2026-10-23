//! Prints a level as a text map: `cargo run -p fb_level --example print_level -- <seed>`.
//! `#` rock, `.` floor, `+` door, `E` entrance, `@` start, `i` item, `*` light, and folk by
//! their first letter in capitals: `H` human, `D` dwarf, `N` gnome, `G` goblin, `L` lizardfolk,
//! `P` pigfolk. Props: containers `c` chest, `b` barrel, `x` crate, `a` amphora (`s` strongbox,
//! `f` coffer), and scenery `%` rubble, `~` bones, `o` pillar, `=` bedroll, `u` cooking pot. Creatures: `r` rat, `R` giant
//! rat, `v` poisonous snake, `S` constrictor snake, `k` skeleton, `T` troll, `p` giant spider, `j` jackalope, `c` giant centipede, `b` bat, `B` giant
//! bat. Webs in a spider's nest: `w`. The floor of a faction's claimed rooms is its letter, `W`, `X`,
//! `Y` or `Z` in the order of the faction list, with a legend below it; wild rooms stay `.`.

use armory::build::scenery::Scenery;
use armory::families::chests::ChestStyle;
use fb_body::Folk;
use fb_creatures::Species;
use fb_level::Prop;

fn creature_mark(species: Species) -> char {
    match species {
        Species::Rat => 'r',
        Species::GiantRat => 'R',
        Species::PoisonousSnake => 'v',
        Species::ConstrictorSnake => 'S',
        Species::Skeleton => 'k',
        Species::GiantSpider => 'p',
        Species::Troll => 'T',
        Species::Jackalope => 'j',
        Species::GiantCentipede => 'c',
        Species::Bat => 'b',
        Species::GiantBat => 'B',
    }
}

fn prop_mark(prop: Prop) -> char {
    match prop {
        Prop::Container { style, .. } => match style {
            ChestStyle::Chest => 'c',
            ChestStyle::Barrel => 'b',
            ChestStyle::Crate => 'x',
            ChestStyle::Amphora => 'a',
            ChestStyle::Strongbox => 's',
            ChestStyle::Coffer => 'f',
        },
        Prop::Scenery { kind, .. } => match kind {
            Scenery::Rubble => '%',
            Scenery::Bones => '~',
            Scenery::Pillar => 'o',
            Scenery::Bedroll => '=',
            Scenery::Bunk => 'B',
            Scenery::Throne => 'T',
            Scenery::Altar => 'A',
            Scenery::Bookshelf => 'H',
            Scenery::Table => 't',
            Scenery::WeaponRack => 'W',
        },
        Prop::Campfire { .. } => 'u',
    }
}

fn folk_mark(folk: Folk) -> char {
    match folk {
        Folk::Human => 'H',
        Folk::Dwarf => 'D',
        Folk::Gnome => 'N',
        Folk::Goblin => 'G',
        Folk::Lizardfolk => 'L',
        Folk::Pigfolk => 'P',
        Folk::Minotaur => 'M',
        Folk::Faun => 'F',
        Folk::Cyclops => 'c',
        Folk::WingedDemon => 'W',
        Folk::HornedDemon => 'B',
        Folk::Succubus => 'S',
    }
}

/// A denizen's mark: the first letter of its body, lower case (m, c, f, w, h, s).
fn denizen_mark(body: fb_level::DenizenBody) -> char {
    use fb_level::DenizenBody::*;
    match body {
        Minotaur => 'm',
        Cyclops => 'c',
        Faun => 'f',
        WingedDemon => 'w',
        HornedDemon => 'h',
        Succubus => 's',
    }
}

fn main() {
    let seed = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1207);
    let depth = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let level = fb_level::generate(seed, depth);
    let (sx, sy, facing) = level.start();
    let mut rows: Vec<Vec<char>> = (0..level.height as i32)
        .map(|y| {
            (0..level.width as i32)
                .map(|x| match level.cell(x, y) {
                    fb_level::Cell::Rock => '#',
                    fb_level::Cell::Floor => '.',
                    fb_level::Cell::Door => '+',
                    fb_level::Cell::Column => 'O',
                    fb_level::Cell::Bars => '|',
                    fb_level::Cell::Gate => '=',
                })
                .collect()
        })
        .collect();
    let letter = |k: usize| ['W', 'X', 'Y', 'Z', 'V', 'U'][k % 6];
    for claim in &level.claims {
        let k = level.factions.iter().position(|f| f.id == claim.faction).unwrap_or(0);
        for &r in &claim.rooms {
            let room = level.rooms[r];
            for y in room.y..room.y + room.h {
                for x in room.x..room.x + room.w {
                    if level.cell(x as i32, y as i32) == fb_level::Cell::Floor {
                        rows[y as usize][x as usize] = letter(k);
                    }
                }
            }
        }
    }
    // With `roles` as the third argument, a claim's rooms show their role instead of the
    // faction's letter: `H` hall, `F` hearth, `S` sleeping, `M` market, `T` store (the first
    // role of a room that has several).
    if std::env::args().nth(3).as_deref() == Some("roles") {
        for (r, room) in level.rooms.iter().enumerate() {
            let Some(role) = level.roles_of(r).first() else { continue };
            for y in room.y..room.y + room.h {
                for x in room.x..room.x + room.w {
                    if level.cell(x as i32, y as i32) == fb_level::Cell::Floor {
                        rows[y as usize][x as usize] = role.letter();
                    }
                }
            }
        }
    }
    // With `purposes` as the third argument, rooms show what they were built for: `q` quarters,
    // `c` cell block, `t` throne room, `s` shrine, `l` library, `o` ossuary, `g` guardroom.
    if std::env::args().nth(3).as_deref() == Some("purposes") {
        println!("past: {}", level.past.map_or("none", |p| p.name()));
        for (r, room) in level.rooms.iter().enumerate() {
            let Some(purpose) = level.purpose_of(r) else { continue };
            for y in room.y..room.y + room.h {
                for x in room.x..room.x + room.w {
                    if level.cell(x as i32, y as i32) == fb_level::Cell::Floor {
                        rows[y as usize][x as usize] = purpose.letter();
                    }
                }
            }
        }
    }
    for item in &level.items {
        rows[item.y as usize][item.x as usize] = 'i';
    }
    for light in &level.lights {
        rows[light.y as usize][light.x as usize] = '*';
    }
    for prop in &level.props {
        rows[prop.y as usize][prop.x as usize] = prop_mark(prop.prop);
    }
    for folk in &level.folk {
        rows[folk.y as usize][folk.x as usize] = folk.denizen.map_or_else(|| folk_mark(folk.folk), denizen_mark);
    }
    for web in &level.webs {
        rows[web.y as usize][web.x as usize] = 'w';
    }
    for creature in &level.creatures {
        rows[creature.y as usize][creature.x as usize] = creature_mark(creature.species);
    }
    for stair in &level.stairs {
        rows[stair.y as usize][stair.x as usize] = if stair.kind == fb_level::StairKind::Down { '>' } else { '<' };
    }
    rows[level.entrance.y as usize][level.entrance.x as usize] = 'E';
    rows[sy as usize][sx as usize] = '@';
    println!(
        "seed {seed}: start ({sx}, {sy}) facing {facing:?}, {} rooms, {} items, {} folk, {} props, {} creatures",
        level.rooms.len(),
        level.items.len(),
        level.folk.len(),
        level.props.len(),
        level.creatures.len()
    );
    println!("{} webs", level.webs.len());
    println!("factions: {}", level.factions.iter().map(|f| match f.denizen {
        Some(kind) => format!("{} (denizens: {}, {})", f.name, kind.label(), f.alignment.label()),
        None => format!("{} ({:?}{})", f.name, f.folk, if f.insular { ", insular" } else { "" }),
    }).collect::<Vec<_>>().join("; "));
    for row in rows {
        println!("{}", row.into_iter().collect::<String>());
    }
    for claim in &level.claims {
        let k = level.factions.iter().position(|f| f.id == claim.faction).unwrap_or(0);
        let members = level.folk.iter().filter(|f| f.faction == claim.faction).count();
        let roles: Vec<String> = claim.rooms.iter().map(|&r| format!("{r}@{},{}:{}", level.rooms[r].x, level.rooms[r].y, level.roles_of(r).iter().map(|x| x.letter()).collect::<String>())).collect();
        println!("{} claims {} rooms {:?} for {members} folk: {} ({:?}); roles {}", letter(k), claim.rooms.len(), claim.rooms, level.faction(claim.faction).name, level.faction(claim.faction).intention, roles.join(" "));
    }
    debug_assert!(level.factions.iter().all(|f| level.claims.iter().any(|c| c.faction == f.id)));
    for item in &level.items {
        println!("{:?} seed {} at ({:.1}, {:.1})", item.family, item.seed, item.x, item.y);
    }
    for folk in &level.folk {
        println!(
            "{} of {}, seed {}, at ({:.1}, {:.1}) facing {:.0} degrees",
            folk.denizen.map_or(folk.folk.race().label.to_string(), |body| body.label().to_string()),
            level.faction(folk.faction).name,
            folk.seed,
            folk.x,
            folk.y,
            folk.yaw.to_degrees()
        );
    }
    for prop in &level.props {
        let what = match prop.prop {
            Prop::Container { style, seed, loot } => format!("{style:?} seed {seed}, loot {}", if loot == fb_level::NO_LOOT { "none".to_string() } else { loot.to_string() }),
            Prop::Scenery { kind, seed } => format!("{kind:?} seed {seed}"),
            Prop::Campfire { seed } => format!("Campfire seed {seed}"),
        };
        println!("{what} at ({:.1}, {:.1}) facing {:.0} degrees", prop.x, prop.y, prop.yaw.to_degrees());
    }
    for creature in &level.creatures {
        let block = creature.species.stat_block();
        println!("{} of pack {}, seed {}, at ({:.1}, {:.1}) facing {:.0} degrees", block.name, creature.pack, creature.seed, creature.x, creature.y, creature.yaw.to_degrees());
    }
}
