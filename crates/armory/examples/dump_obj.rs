//! Writes a rolled mace as Wavefront OBJ for eyeballing in any 3D viewer, one group per part.
//!
//! `cargo run -p armory --example dump_obj -- <seed> [out.obj]` (default `mace_<seed>.obj`).
//! Prints each part's name, material and triangle count.

use std::fmt::Write as _;

use armory::families::Generator;
use armory::families::mace::Mace;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let path = args.next().unwrap_or_else(|| format!("mace_{seed}.obj"));
    let state = Mace::roll(seed);
    let item = Mace::build(&state);
    println!("seed {seed}: {}", Mace::noun(&state));

    let mut obj = format!("# armory mace, seed {seed}\n");
    let mut base = 1;
    for part in &item.parts {
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
