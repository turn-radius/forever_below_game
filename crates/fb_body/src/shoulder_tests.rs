//! Specs for cloth over the shoulders: a cowl or hood's cape, and what is worn under it.

use armory::families::garment::Garment;
use armory::families::hood::Hoods;
use armory::geom::vec3::{self, V3};
use armory::sections::garment::{GarmentStyle, Sleeves};
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
/// A fold (the sheet doubling back on itself) reads as the sharpest crease there is. Only the
/// surface that can be seen counts: the faces that look up or out from the body's axis (a
/// sheet's thickness offsets its inner face, which folds where the sheet turns tighter than it is
/// thick, out of sight).
fn creases(part: &Part) -> Vec<(f32, V3)> {
    let mut edges: HashMap<(u32, u32), Vec<V3>> = HashMap::new();
    for t in part.indices.chunks(3) {
        let tri = [t[0], t[1], t[2]].map(|i| part.positions[i as usize]);
        let n = vec3::normalize_or(vec3::cross(vec3::sub(tri[1], tri[0]), vec3::sub(tri[2], tri[0])), vec3::Y);
        let c = vec3::scale(vec3::add(vec3::add(tri[0], tri[1]), tri[2]), 1.0 / 3.0);
        if vec3::dot(n, [c[0], 3.0, c[2]]) <= 0.0 {
            continue;
        }
        // A sliver (three points nearly in a line) has no normal worth the name.
        let longest = (0..3).map(|k| vec3::distance(tri[k], tri[(k + 1) % 3])).fold(0.0f32, f32::max);
        if vec3::length(vec3::cross(vec3::sub(tri[1], tri[0]), vec3::sub(tri[2], tri[0]))) < 0.12 * longest * longest {
            continue;
        }
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            edges.entry(if a < b { (a, b) } else { (b, a) }).or_default().push(n);
        }
    }
    edges
        .into_iter()
        .filter(|(_, n)| n.len() == 2)
        .map(|((a, b), n)| (vec3::dot(n[0], n[1]).clamp(-1.0, 1.0).acos().to_degrees(), vec3::lerp(part.positions[a as usize], part.positions[b as usize], 0.5)))
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
    assert!(!cloth.is_empty(), "the hood has no \"hood\" part");
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
    assert!(crest.len() >= 3, "the crest has only {} samples", crest.len());
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
    let (mut tested, mut missed) = (0, 0);
    for p in points.iter().step_by((points.len() / 250).max(1)) {
        match height_over(&cloth, p[0], p[2]) {
            Some(h) => {
                tested += 1;
                if p[1] - h > poke {
                    (poke, at) = (p[1] - h, **p);
                }
            }
            None => missed += 1,
        }
    }
    // The check must see the cloth: most of the shoulder's points have it over them (a
    // cowl's open front is the rest).
    assert!(tested >= 20 && tested >= 2 * missed, "only {tested} of {} points of the shoulder lie under the cloth", tested + missed);
    Shoulder { poke, crease, ridge, at }
}

/// The cases the sweeps run: a cowl, then a cowl and a hood over pauldrons.
const CASES: [(HoodStyle, bool); 3] = [(HoodStyle::Cowl, false), (HoodStyle::Cowl, true), (HoodStyle::Hood, true)];

fn cloth_fits_round_the_shoulders_check(folks: &[Folk], seeds: &[u32], kits: &[u32]) {
    let mut failures = vec![];
    let (mut measured, mut unmeasured) = (0, 0);
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
                // A cloak or mantle: over a robe, over pauldrons, and the mage's own kit.
                let mage = fb_items::outfit::outfit("mage").expect("the mage outfit");
                let kits: [(String, Vec<(Slot, Item)>); 3] = [
                    ("cloak".to_string(), cloaked(GarmentStyle::Cloak, false, kit)),
                    ("cloak over pauldrons".to_string(), cloaked(GarmentStyle::Cloak, true, kit)),
                    ("mage kit".to_string(), mage.roll(kit, folk.key()).worn.into_iter().map(|(slot, i)| (slot, i.shape)).collect()),
                ];
                for (name, worn) in kits.iter().filter(|(_, worn)| worn.iter().any(|(slot, _)| *slot == Slot::Cloak)) {
                    let Some((turn, sag)) = cloak_outline(&body, worn) else {
                        unmeasured += 1;
                        continue;
                    };
                    measured += 1;
                    let case = format!("{folk:?} {seed} kit {kit} {name}");
                    // The outline from the collar over the shoulder is taut and round: it
                    // does not dip between the collar and the shoulder (a flat shelf with a
                    // peak at the arm sagged by up to 0.6 of the wearer's scale), and turns
                    // by under 58 degrees between neighbouring stretches (a square corner).
                    if sag > 0.12 * body.scale {
                        failures.push(format!("{case}: the outline sags {:.2} between the collar and the shoulder", sag / body.scale));
                    }
                    if turn > 58.0 {
                        failures.push(format!("{case}: the outline turns {turn:.0} degrees at the shoulder"));
                    }
                }
                // A cloak cut as short as it goes keeps its hem close to what is under it: within
                // half the wearer's scale (it stood 0.2 to 0.35 out with the shoulders drawn taut).
                let (hem, beneath) = hem_reach(&body, kit);
                if hem > beneath + 0.5 * body.scale {
                    failures.push(format!("{folk:?} {seed} kit {kit}: a short cloak's hem flares {:.2} out from what is under it", hem - beneath));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // The cloak outlines are really measured: at most as many have no roof to judge as have one
    // (a collar that stops at the jaw leaves a small head little roof between it and the arm).
    assert!(measured > 0 && unmeasured <= measured, "{measured} cloak outlines measured, {unmeasured} with no roof");
}

#[test]
#[ignore = "slow sweep: body fitting test, every folk and seed; run before merging fitting changes"]
fn cloth_fits_round_the_shoulders() {
    cloth_fits_round_the_shoulders_check(&Folk::ALL, &[1207, 3, 40], &[5, 6, 7]);
}

#[test]
fn cloth_fits_round_the_shoulders_smoke() {
    // A goblin whose cloak sagged by two thirds of its scale between collar and shoulder, and a
    // pigfolk whose pauldrons stood 0.8 above a cowl; the sweep has the rest (about 7 s here).
    cloth_fits_round_the_shoulders_check(&[Folk::Goblin, Folk::Pigfolk], &[37], &[7]);
}


/// A mage's robe with a mantle or a cloak over it (and pauldrons under it on a hauberk with
/// `pauldrons`), as the doll wears them.
fn cloaked(style: GarmentStyle, pauldrons: bool, seed: u32) -> Vec<(Slot, Item)> {
    let mut robe = Garment::roll_as(seed, GarmentStyle::Robe);
    robe.cut.sleeves = Sleeves::Long;
    robe.cut.sleeve_flare = 1.0;
    let mut worn = vec![
        (Slot::Shirt, Item::Garment(robe)),
        (Slot::Cloak, Item::Garment(Garment::roll_as(seed + 1, style))),
        (Slot::Legs, Item::roll(Family::Legs, seed).unwrap()),
    ];
    if pauldrons {
        let mut mail = Garment::roll_as(seed + 2, GarmentStyle::Hauberk);
        mail.plates.pauldrons = true;
        worn.push((Slot::Body, Item::Garment(mail)));
    }
    worn
}

/// How the outline of a cloak over the shoulders, seen from the front, goes from the collar
/// out over the shoulder and down the arm: the sharpest turn in degrees between neighbouring
/// stretches of it (a flat top running out from the neck to a square corner turns by a great
/// deal at the corner), and the deepest the outline sags below the straight line between two
/// of its points (a dip between the collar and the shoulder, with the shoulder a peak).
fn cloak_outline(body: &Body, worn: &[(Slot, Item)]) -> Option<(f32, f32)> {
    let fit = fit_worn(body, worn);
    let cloak = fit.layer(Slot::Cloak).expect("a cloak");
    let cloth: Vec<&Part> = cloak.mesh.parts.iter().filter(|p| p.name == "body").collect();
    assert!(!cloth.is_empty(), "the cloak has no \"body\" part");
    let s = body.scale;
    let step = 0.5 * s;
    let top = |x: f32| cloth.iter().flat_map(|p| p.positions.iter()).filter(|p| (p[0].abs() - x).abs() < 0.5 * step).map(|p| p[1]).fold(f32::MIN, f32::max);
    // From the collar's edge (where the outline first drops from the neck's height) down to
    // 1.5 below the shoulder line, where the cloak is falling down the arm.
    let reach = cloth.iter().flat_map(|p| p.positions.iter()).map(|p| p[0].abs()).fold(0.0f32, f32::max);
    let samples: Vec<(f32, f32)> = (0..).map(|i| 0.5 * body.shoulder_half_width + step * i as f32).take_while(|&x| x < reach).map(|x| (x, top(x))).filter(|c| c.1 > -1e6).collect();
    let high = samples.iter().map(|c| c.1).fold(f32::MIN, f32::max);
    let start = samples.iter().position(|c| high - c.1 > 0.02 * s).unwrap_or(0).saturating_sub(1);
    let end = samples[start..].iter().position(|c| c.1 < body.shoulder_y - 0.1 * s).unwrap_or(samples.len() - start);
    let outline: Vec<(f32, f32)> = samples[start..start + end].to_vec();
    // A gnome's or goblin's cowl collar can stand as wide as its shoulders, with no roof between
    // it and the arm to measure: that is no answer, and the check counts how often it happens.
    if outline.len() < 3 {
        return None;
    }
    let heading = |a: (f32, f32), b: (f32, f32)| (b.1 - a.1).atan2(b.0 - a.0).to_degrees();
    let turn = outline.windows(3).map(|w| (heading(w[1], w[2]) - heading(w[0], w[1])).abs()).fold(0.0f32, f32::max);
    // The sag below the chords: the outline against its least concave majorant.
    let mut chain: Vec<(f32, f32)> = vec![];
    for &p in &outline {
        while chain.len() >= 2 {
            let (o, q) = (chain[chain.len() - 2], chain[chain.len() - 1]);
            if (q.0 - o.0) * (p.1 - o.1) - (q.1 - o.1) * (p.0 - o.0) >= 0.0 {
                chain.pop();
            } else {
                break;
            }
        }
        chain.push(p);
    }
    let sag = outline
        .iter()
        .filter_map(|p| chain.windows(2).find(|w| p.0 >= w[0].0 && p.0 <= w[1].0).map(|w| w[0].1 + (w[1].1 - w[0].1) * (p.0 - w[0].0) / (w[1].0 - w[0].0) - p.1))
        .fold(0.0f32, f32::max);
    if sag > 0.12 * s {
        eprintln!("OUTLINE {:?}", outline.iter().map(|c| (format!("{:.2}", c.0), format!("{:.2}", c.1 - body.shoulder_y))).collect::<Vec<_>>());
    }
    Some((turn, sag))
}


/// A cloak cut as short as it goes, over a robe with long sleeves (and not over the legs): its
/// hem as far out from the axis as the cloak reaches, and as far out as whatever is under it
/// (the skin and the robe) reaches there. A hull that ran past the cloak's own rows left the
/// hem flaring out from the arms.
fn hem_reach(body: &Body, seed: u32) -> (f32, f32) {
    let mut worn = cloaked(GarmentStyle::Cloak, false, seed);
    for (slot, item) in worn.iter_mut() {
        if let (Slot::Cloak, Item::Garment(g)) = (&*slot, item) {
            g.cut.length = 0.0;
        }
    }
    let fit = fit_worn(body, &worn);
    let cloak = &fit.layer(Slot::Cloak).expect("a cloak").mesh.parts.iter().find(|p| p.name == "body").expect("a body part").positions;
    let low = cloak.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
    let band = 0.15 * body.scale;
    let radial = |p: &V3| p[0].hypot(p[2]);
    let hem = cloak.iter().filter(|p| p[1] < low + band).map(radial).fold(0.0f32, f32::max);
    let skin = body.skin_mesh();
    let mut under: Vec<&Part> = skin.parts.iter().collect();
    under.extend(fit.layer(Slot::Shirt).expect("a robe").mesh.parts.iter());
    let beneath = under.iter().flat_map(|p| p.positions.iter()).filter(|p| p[1] > low - band && p[1] < low + 3.0 * band).map(radial).fold(0.0f32, f32::max);
    (hem, beneath)
}
