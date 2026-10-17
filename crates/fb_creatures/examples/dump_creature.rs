//! Writes one creature, in its rest pose, as Wavefront OBJ on stdout, one group per bone and
//! part, for a look in any 3D viewer.
//!
//! `cargo run -p fb_creatures --example dump_creature -- <rat|giant_rat|viper|constrictor|skeleton|giant_spider|troll|jackalope> <seed> > rat.obj`

use fb_creatures::{Creature, Species};

fn main() {
    let mut args = std::env::args().skip(1);
    let species = match args.next().as_deref() {
        Some("rat") => Species::Rat,
        Some("giant_rat") => Species::GiantRat,
        Some("viper") => Species::PoisonousSnake,
        Some("constrictor") => Species::ConstrictorSnake,
        Some("skeleton") => Species::Skeleton,
        Some("giant_spider") => Species::GiantSpider,
        Some("troll") => Species::Troll,
        Some("jackalope") => Species::Jackalope,
        _ => {
            eprintln!("usage: dump_creature <rat|giant_rat|viper|constrictor|skeleton|giant_spider|troll|jackalope> [seed]");
            std::process::exit(2);
        }
    };
    let seed = args.next().and_then(|s| s.parse().ok()).unwrap_or(1207);
    let creature = Creature::of(species, seed);
    eprintln!("{creature:?}");
    let rig = creature.rig();
    let world = rig.world(&rig.rest());
    let mut base = 1;
    for (bone, place) in rig.bones.iter().zip(&world) {
        for part in &bone.mesh.parts {
            println!("g {}_{}", bone.name, part.name);
            for &p in &part.positions {
                let [x, y, z] = place.point(p);
                println!("v {x} {y} {z}");
            }
            for &n in &part.normals {
                let [x, y, z] = place.direction(n);
                println!("vn {x} {y} {z}");
            }
            for t in part.indices.chunks(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| i as usize + base);
                println!("f {a}//{a} {b}//{b} {c}//{c}");
            }
            base += part.positions.len();
        }
    }
}
