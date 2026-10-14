//! Writes one folk's body as Wavefront OBJ on stdout, one group per part, for a look in any
//! 3D viewer before the doll view exists.
//!
//! `cargo run -p fb_body --example dump_body -- <folk> <seed> > body.obj`

use fb_body::{Body, BodySpec, Folk};

fn main() {
    let mut args = std::env::args().skip(1);
    let folk = args.next().and_then(|f| Folk::parse(&f)).unwrap_or_else(|| {
        eprintln!("usage: dump_body <human|dwarf|gnome|goblin|lizard|pig> [seed]");
        std::process::exit(2);
    });
    let seed = args.next().and_then(|s| s.parse().ok()).unwrap_or(1207);
    let spec = BodySpec::roll(folk, seed);
    eprintln!("{spec:?}");
    let mesh = Body::new(&spec).mesh();
    let mut base = 1;
    for part in &mesh.parts {
        println!("g {}", part.name);
        for p in &part.positions {
            println!("v {} {} {}", p[0], p[1], p[2]);
        }
        for n in &part.normals {
            println!("vn {} {} {}", n[0], n[1], n[2]);
        }
        for t in part.indices.chunks(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| i as usize + base);
            println!("f {a}//{a} {b}//{b} {c}//{c}");
        }
        base += part.positions.len();
    }
}
