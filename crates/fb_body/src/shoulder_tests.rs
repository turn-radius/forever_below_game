//! Specs for cloth over the shoulders: a cowl or hood's cape, and what is worn under it.

use armory::families::garment::Garment;
use armory::families::hood::Hoods;
use armory::geom::vec3::{self, V3};
use armory::sections::garment::GarmentStyle;
use armory::sections::hood::HoodStyle;
use armory::{Family, Item, Part};
use fb_items::Slot;
use std::collections::HashMap;

use crate::{Body, BodySpec, Folk, fit_worn};

/// A cowl (or a hood) with a cape that reaches well down the arms, over a tunic, and with
/// `pauldrons` a hauberk with pauldrons over that.
fn dressed(style: HoodStyle, pauldrons: bool, seed: u32) -> Vec<(Slot, Item)> {
    let mut hood = Hoods::roll_as(seed, style);
    hood.hood.cape = 1.8;
    hood.hood.peak = hood.hood.peak.min(0.3);
    let mut worn = vec![
        (Slot::Shirt, Item::Garment(Garment::roll_as(seed, GarmentStyle::Tunic))),
        (Slot::Legs, Item::roll(Family::Legs, seed).unwrap()),
        (Slot::Head, Item::Hood(hood)),
    ];
    if pauldrons {
        let mut mail = Garment::roll_as(seed + 1, GarmentStyle::Hauberk);
        mail.plates.pauldrons = true;
        worn.push((Slot::Body, Item::Garment(mail)));
    }
    worn
}

/// The highest point of `parts` over the horizontal position `(x, z)`: the farthest triangle
/// a ray straight down from above meets.
fn height_over(parts: &[&Part], x: f32, z: f32) -> Option<f32> {
    let (origin, dir) = ([x, 100.0, z], [0.0, -1.0, 0.0]);
    parts
        .iter()
        .flat_map(|part| part.indices.chunks(3).map(move |t| [t[0], t[1], t[2]].map(|i| part.positions[i as usize])))
        .filter_map(|[a, b, c]| {
            let (e1, e2) = (vec3::sub(b, a), vec3::sub(c, a));
            let h = vec3::cross(dir, e2);
            let det = vec3::dot(e1, h);
            if det.abs() < 1e-9 {
                return None;
            }
            let s = vec3::sub(origin, a);
            let u = vec3::dot(s, h) / det;
            let q = vec3::cross(s, e1);
            let v = vec3::dot(dir, q) / det;
            let t = vec3::dot(e2, q) / det;
            (u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.0).then(|| origin[1] - t)
        })
        .reduce(f32::max)
}

/// The angle in degrees between the two triangles at each edge of `part`, and the edge's middle.
/// Folded seams (the sheet doubling back on itself) are left out.
fn creases(part: &Part) -> Vec<(f32, V3)> {
    let mut edges: HashMap<(u32, u32), Vec<V3>> = HashMap::new();
    for t in part.indices.chunks(3) {
        let tri = [t[0], t[1], t[2]].map(|i| part.positions[i as usize]);
        let n = vec3::normalize_or(vec3::cross(vec3::sub(tri[1], tri[0]), vec3::sub(tri[2], tri[0])), vec3::Y);
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            edges.entry(if a < b { (a, b) } else { (b, a) }).or_default().push(n);
        }
    }
    edges
        .into_iter()
        .filter(|(_, n)| n.len() == 2)
        .map(|((a, b), n)| (vec3::dot(n[0], n[1]).clamp(-1.0, 1.0).acos().to_degrees(), vec3::lerp(part.positions[a as usize], part.positions[b as usize], 0.5)))
        .filter(|(angle, _)| *angle < 150.0)
        .collect()
}

/// What the cloth over one shoulder does: how far worn things stand above it, how sharply its
/// crest turns, and how far it stands off what is under it.
struct Shoulder {
    /// The most any worn layer or the skin under the cloth stands above it.
    poke: f32,
    /// The sharpest crease, in degrees, between neighbouring triangles of the cloth over the
    /// shoulder: 0 is flat, and a fold over a corner reads as a point.
    crease: f32,
    /// Where the worst poke is.
    at: V3,
    /// The most the crest climbs again after it has begun to fall: a ridge.
    ridge: f32,
}

fn measure(body: &Body, worn: &[(Slot, Item)]) -> Shoulder {
    let fit = fit_worn(body, worn);
    let hood = fit.layer(Slot::Head).expect("a hood");
    let cloth: Vec<&Part> = hood.mesh.parts.iter().filter(|p| p.name == "hood").collect();
    let s = body.scale;
    // The crest: the highest the cloth gets at each distance out from the neck, on the
    // right-hand shoulder, from the head's edge out to where the cape ends.
    let head = body.head.half_width.max(0.45 * body.shoulder_half_width) + 0.25 * s;
    let reach = cloth.iter().flat_map(|p| p.positions.iter()).map(|p| p[0]).fold(0.0f32, f32::max);
    let step = 0.18 * s;
    // Over the shoulder itself: from past the middle of the neck's width to the shoulder's tip.
    let over = (0.9 * body.shoulder_half_width).max(1.3 * body.head.half_width + 0.3 * s);
    let crest: Vec<V3> = (0..)
        .map(|i| head + step * i as f32)
        .take_while(|&x| x < reach - 0.5 * step)
        .filter_map(|x| {
            let at = cloth.iter().flat_map(|p| p.positions.iter()).filter(|p| p[0] > x - 0.5 * step && p[0] < x + 0.5 * step && p[1] < body.shoulder_y + 0.2 * body.trunk_length).max_by(|a, b| a[1].total_cmp(&b[1]));
            at.map(|p| [x, p[1], 0.0])
        })
        .collect();
    let worst = cloth
        .iter()
        .flat_map(|p| creases(p))
        .filter(|(_, m)| m[0] > over && m[0] < reach - step && m[1] < body.shoulder_y + 0.7 * s && m[1] > body.shoulder_y - 0.6 * s)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap_or((0.0, [0.0; 3]));
    let crease = worst.0;
    // After the crest's highest point it only falls.
    let top = crest.iter().enumerate().max_by(|a, b| a.1[1].total_cmp(&b.1[1])).map_or(0, |(i, _)| i);
    let ridge = crest[top..].windows(2).map(|w| w[1][1] - w[0][1]).fold(0.0f32, f32::max);
    // What is under the cloth: the skin and every other layer but what is on the head or the
    // back, over the shoulder (outside the neck, on the cloth's shoulders), must not stand
    // above it.
    let mut under: Vec<&Part> = vec![];
    let skin = body.skin_mesh();
    under.extend(skin.parts.iter());
    for l in fit.layers.iter().filter(|l| !matches!(l.slot, Slot::Head | Slot::Back | Slot::Legs | Slot::Feet | Slot::Hands | Slot::Belt)) {
        under.extend(l.mesh.parts.iter().filter(|p| p.name != "collar"));
    }
    let (mut poke, mut at) = (0.0f32, [0.0; 3]);
    // The right shoulder's cap, outside the neck and above the arm's middle, where the cape
    // lies (its front is open on a cowl); every few points of it, to keep the check quick.
    let on_shoulder = |p: &&V3| p[0] > head && p[0] < reach - step && p[1] > body.shoulder_y - 0.6 * s && p[1] < body.shoulder_y + 0.11 * body.trunk_length && p[2].abs() < 0.6 * body.shoulder_half_width.min(2.0);
    let points: Vec<&V3> = under.iter().flat_map(|p| p.positions.iter()).filter(on_shoulder).collect();
    for p in points.iter().step_by((points.len() / 250).max(1)) {
        if let Some(h) = height_over(&cloth, p[0], p[2]) {
            if p[1] - h > poke {
                (poke, at) = (p[1] - h, **p);
            }
        }
    }
    Shoulder { poke, crease, ridge, at }
}

/// The cases the sweeps run: a cowl, then a cowl and a hood over pauldrons.
const CASES: [(HoodStyle, bool); 3] = [(HoodStyle::Cowl, false), (HoodStyle::Cowl, true), (HoodStyle::Hood, true)];

fn cloth_fits_round_the_shoulders_check(folks: &[Folk], seeds: &[u32], kits: &[u32]) {
    let mut failures = vec![];
    for &folk in folks {
        for &seed in seeds {
            let body = Body::new(&BodySpec::roll(folk, seed));
            for &kit in kits {
                for (style, pauldrons) in CASES {
                    let m = measure(&body, &dressed(style, pauldrons, kit));
                    let s = body.scale;
                    let case = format!("{folk:?} {seed} kit {kit} {style:?} pauldrons {pauldrons}");
                    // What is worn on the shoulders does not stand out above the cloth over it
                    // (a flat shelf at the neck's height left yokes and pauldrons showing above
                    // it, and the cloth a wing beside them): within 1.5 cm on a human.
                    if m.poke > 0.15 * s {
                        failures.push(format!("{case}: worn gear stands {:.2} above the cloth at {:.1?}", m.poke, m.at));
                    }
                    // No crease or ridge: neighbouring triangles over the shoulder turn by no
                    // more than 60 degrees, and the crest only falls from its top outward.
                    if m.crease > 60.0 {
                        failures.push(format!("{case}: a {:.0} degree crease over the shoulder", m.crease));
                    }
                    if m.ridge > 0.06 * s {
                        failures.push(format!("{case}: the crest climbs {:.2} again past its top", m.ridge));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore = "slow sweep: body fitting test, every folk and seed; run before merging fitting changes"]
fn cloth_fits_round_the_shoulders() {
    cloth_fits_round_the_shoulders_check(&Folk::ALL, &[1207, 3, 40], &[5, 6, 7]);
}

#[test]
fn cloth_fits_round_the_shoulders_smoke() {
    // The pigfolk and dwarf whose pauldrons stood 0.8 above a cowl, a goblin whose cape top
    // rose past its hood's rim, and the humans in between.
    cloth_fits_round_the_shoulders_check(&[Folk::Human, Folk::Dwarf, Folk::Goblin, Folk::Pigfolk], &[1207], &[5]);
}
