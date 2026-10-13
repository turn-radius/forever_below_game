//! Writes a rolled item as Wavefront OBJ for eyeballing in any 3D viewer, one group per part.
//!
//! `cargo run -p armory --example dump_obj -- [family] <seed> [out.obj]`: the family defaults
//! to mace (any ported family's key works: `bags`, `chests`...), and the file to
//! `<family>_<seed>.obj`. Prints the noun and each part's name, material and triangle count.

use std::fmt::Write as _;

use armory::{Family, Item};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let family = match args.first().and_then(|a| Family::ALL.into_iter().find(|f| f.key() == a)) {
        Some(f) => {
            args.remove(0);
            f
        }
        None => Family::Mace,
    };
    let mut args = args.into_iter();
    let seed: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let path = args.next().unwrap_or_else(|| format!("{}_{seed}.obj", family.key()));
    let Some(item) = Item::roll(family, seed) else {
        eprintln!("{family:?} is not ported yet");
        std::process::exit(1);
    };
    let mesh = item.build();
    println!("{} seed {seed}: {}", family.key(), item.noun());

    let mut obj = format!("# armory {}, seed {seed}\n", family.key());
    let mut base = 1;
    for part in &mesh.parts {
        println!("  {:<14} {:<22} {:>6} tris", part.name, format!("{:?}", part.material), part.indices.len() / 3);
        writeln!(obj, "g {}", part.name).unwrap();
        for p in &part.positions {
            writeln!(obj, "v {} {} {}", p[0], p[1], p[2]).unwrap();
        }
        for n in &part.normals {
            writeln!(obj, "vn {} {} {}", n[0], n[1], n[2]).unwrap();
        }
        for t in part.indices.as_chunks::<3>().0 {
            let [a, b, c] = [t[0] + base, t[1] + base, t[2] + base];
            writeln!(obj, "f {a}//{a} {b}//{b} {c}//{c}").unwrap();
        }
        base += part.positions.len() as u32;
    }
    std::fs::write(&path, obj).expect("write OBJ");
    println!("wrote {path}");
}
