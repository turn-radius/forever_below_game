//! Prints a level as a text map: `cargo run -p fb_level --example print_level -- <seed>`.
//! `#` rock, `.` floor, `+` door, `E` entrance, `@` start, `i` item, `*` light, and folk by
//! their first letter in capitals: `H` human, `D` dwarf, `N` gnome, `G` goblin, `L` lizardfolk,
//! `P` pigfolk.

use fb_body::Folk;

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
    let level = fb_level::generate(seed);
    let (sx, sy, facing) = level.start();
    let mut rows: Vec<Vec<char>> = (0..level.height as i32)
        .map(|y| {
            (0..level.width as i32)
                .map(|x| match level.cell(x, y) {
                    fb_level::Cell::Rock => '#',
                    fb_level::Cell::Floor => '.',
                    fb_level::Cell::Door => '+',
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
    for folk in &level.folk {
        rows[folk.y as usize][folk.x as usize] = folk_mark(folk.folk);
    }
    rows[level.entrance.y as usize][level.entrance.x as usize] = 'E';
    rows[sy as usize][sx as usize] = '@';
    println!(
        "seed {seed}: start ({sx}, {sy}) facing {facing:?}, {} rooms, {} items, {} folk",
        level.rooms.len(),
        level.items.len(),
        level.folk.len()
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
}
