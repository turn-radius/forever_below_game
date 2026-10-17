//! Prints a level as a text map: `cargo run -p fb_level --example print_level -- <seed>`.
//! `#` rock, `.` floor, `+` door, `E` entrance, `@` start, `i` item, `*` light, and folk by
//! their first letter in capitals: `H` human, `D` dwarf, `N` gnome, `G` goblin, `L` lizardfolk,
//! `P` pigfolk. Props: containers `c` chest, `b` barrel, `x` crate, `a` amphora (`s` strongbox,
//! `f` coffer), and scenery `%` rubble, `~` bones, `o` pillar. Creatures: `r` rat, `R` giant
//! rat, `v` poisonous snake, `S` constrictor snake, `k` skeleton, `T` troll.

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
        },
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
                })
                .collect()
        })
        .collect();
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
        rows[folk.y as usize][folk.x as usize] = folk_mark(folk.folk);
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
    for row in rows {
        println!("{}", row.into_iter().collect::<String>());
    }
    for item in &level.items {
        println!("{:?} seed {} at ({:.1}, {:.1})", item.family, item.seed, item.x, item.y);
    }
    for folk in &level.folk {
        println!(
            "{} of {}, seed {}, at ({:.1}, {:.1}) facing {:.0} degrees",
            folk.folk.race().label,
            folk.faction.name(),
            folk.seed,
            folk.x,
            folk.y,
            folk.yaw.to_degrees()
        );
    }
    for prop in &level.props {
        let what = match prop.prop {
            Prop::Container { style, seed, loot } => format!("{style:?} seed {seed}, loot {loot}"),
            Prop::Scenery { kind, seed } => format!("{kind:?} seed {seed}"),
        };
        println!("{what} at ({:.1}, {:.1}) facing {:.0} degrees", prop.x, prop.y, prop.yaw.to_degrees());
    }
    for creature in &level.creatures {
        let block = creature.species.stat_block();
        println!("{} of pack {}, seed {}, at ({:.1}, {:.1}) facing {:.0} degrees", block.name, creature.pack, creature.seed, creature.x, creature.y, creature.yaw.to_degrees());
    }
}
