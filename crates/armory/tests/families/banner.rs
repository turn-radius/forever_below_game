//! Banners: deterministic per seed, every end shape and hanging builds a drawable mesh, the
//! sliders hold their limits, and a faction's field paints through.

use armory::fabric::PatternKind;
use armory::families::Generator;
use armory::families::banner::{Banner, BannerEnd, BannerMount, BannerState, LIMITS};
use armory::heraldry::{Division, Field, Tincture};
use armory::sections::device::Device;
use armory::{Family, Item, ItemMesh, Part};

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

fn bounds(part: &Part) -> ([f32; 3], [f32; 3]) {
    part.positions.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

fn drawable(state: &BannerState) -> ItemMesh {
    let mesh = Banner::build(state);
    assert!(!mesh.parts.is_empty(), "seed {}: no parts", state.seed);
    for p in &mesh.parts {
        p.validate().unwrap_or_else(|e| panic!("seed {} {:?}: {e}", state.seed, state.cloth.end));
        assert!(!p.indices.is_empty(), "seed {}: {} has no triangles", state.seed, p.name);
        assert!(p.material.is_some(), "seed {}: {} has no material", state.seed, p.name);
        for t in p.indices.chunks(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| p.positions[i as usize]);
            let n = cross(sub(b, a), sub(c, a));
            assert!(dot(n, n) > 1e-14, "seed {}: {} has a degenerate triangle", state.seed, p.name);
        }
    }
    mesh
}

#[test]
fn same_seed_same_banner() {
    for seed in [0, 1, 7, 1207, u32::MAX] {
        let (a, b) = (Banner::roll(seed), Banner::roll(seed));
        assert_eq!(a, b);
        assert_eq!(Banner::noun(&a), Banner::noun(&b));
        assert_eq!(Banner::build(&a), Banner::build(&b), "seed {seed}");
    }
}

#[test]
fn every_end_and_hanging_builds_a_valid_mesh() {
    for mount in [BannerMount::Rod, BannerMount::Pole] {
        for end in BannerEnd::ALL {
            for seed in 1..=6 {
                let mut state = Banner::roll_as(seed, mount, end);
                assert_eq!((state.hanging.mount, state.cloth.end), (mount, end));
                drawable(&state);
                // Every option on at once, and at the deepest cut.
                state.hem.on = true;
                state.fringe.on = true;
                state.cloth.end_depth = LIMITS.cloth.end_depth.max;
                drawable(&state);
                state.cloth.end_depth = LIMITS.cloth.end_depth.min;
                state.hem.width = LIMITS.hem.width.max;
                drawable(&state);
            }
        }
    }
}

#[test]
fn the_limits_hold_at_both_ends() {
    for seed in 1..=64 {
        let state = Banner::roll(seed);
        assert_eq!(state.out_of_range(&LIMITS), Vec::<String>::new(), "seed {seed}");
        assert_eq!(Banner::roll_as(seed, state.hanging.mount, state.cloth.end), state, "roll_as follows roll");
    }
    // Smallest and largest of every slider still build.
    for pick_max in [false, true] {
        let mut state = Banner::roll(3);
        let (c, h, f) = (&LIMITS.cloth, &LIMITS.hanging, &LIMITS.fringe);
        let at = |s: armory::slider::Slider| if pick_max { s.max } else { s.min };
        state.cloth.width = at(c.width);
        state.cloth.length = at(c.length);
        state.cloth.end_depth = at(c.end_depth);
        state.cloth.sag = at(c.sag);
        state.cloth.folds = at(c.folds);
        state.cloth.hang = at(c.hang);
        state.hanging.tabs = at(h.tabs) as u32;
        state.hanging.overhang = at(h.overhang);
        state.hanging.reach = at(h.reach);
        state.hanging.foot = at(h.foot);
        state.fringe.on = true;
        state.fringe.strands = at(f.strands) as u32;
        state.fringe.length = at(f.length);
        assert_eq!(state.out_of_range(&LIMITS), Vec::<String>::new());
        for end in BannerEnd::ALL {
            state.cloth.end = end;
            drawable(&state);
        }
    }
}

#[test]
fn rolls_cover_every_end_and_hanging_at_wall_banner_sizes() {
    let mut ends = std::collections::HashSet::new();
    let mut mounts = std::collections::HashSet::new();
    let mut nouns = std::collections::HashSet::new();
    for seed in 1..=200 {
        let state = Banner::roll(seed);
        ends.insert(state.cloth.end);
        mounts.insert(state.hanging.mount);
        nouns.insert(Banner::noun(&state));
        if state.hanging.mount == BannerMount::Rod {
            assert!((6.0..=12.0).contains(&state.cloth.length), "seed {seed}: wall banner {} tall", state.cloth.length);
        }
    }
    assert_eq!(ends.len(), 4);
    assert_eq!(mounts.len(), 2);
    for noun in ["Banner", "Standard", "Pennant", "Gonfalon"] {
        assert!(nouns.contains(noun), "no {noun} in {nouns:?}");
    }
}

#[test]
fn the_cloth_hangs_below_its_bar_and_is_not_flat() {
    for seed in 1..=24 {
        let mut state = Banner::roll_as(seed, BannerMount::Rod, BannerEnd::Straight);
        state.hem.on = false;
        let mesh = drawable(&state);
        let (lo, hi) = bounds(part(&mesh, "cloth").unwrap());
        assert!(hi[1] <= 0.01 && lo[1] >= -state.cloth.length - 0.01, "seed {seed}: cloth spans {} to {}", lo[1], hi[1]);
        assert!(state.cloth.length - (hi[1] - lo[1]) < 0.7, "seed {seed}: {} tall", hi[1] - lo[1]);
        if state.cloth.folds > 0.2 {
            assert!(hi[2] - lo[2] > 0.1, "seed {seed}: a flat card");
        }
    }
}

#[test]
fn cloth_is_double_sided_and_its_field_uv_spans_the_cloth() {
    let mut state = Banner::roll_as(5, BannerMount::Rod, BannerEnd::Straight);
    state.hem.on = false;
    let mesh = drawable(&state);
    let cloth = part(&mesh, "cloth").unwrap();
    let n = cloth.positions.len() / 2;
    assert_eq!(cloth.positions[..n], cloth.positions[n..], "the back copy sits on the front");
    for (a, b) in cloth.normals[..n].iter().zip(&cloth.normals[n..]) {
        assert!(a[0] * b[0] + a[1] * b[1] + a[2] * b[2] < -0.99, "back normals face the other way");
    }
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for uv in &cloth.uvs {
        lo = [lo[0].min(uv[0]), lo[1].min(uv[1])];
        hi = [hi[0].max(uv[0]), hi[1].max(uv[1])];
    }
    assert!(lo[0] < 0.01 && lo[1] < 0.01 && hi[0] > 0.99 && hi[1] > 0.99, "uv spans {lo:?} to {hi:?}");
}

#[test]
fn the_scale_courts_field_paints_green_and_gold_with_a_tower() {
    let mut state = Banner::roll(11);
    state.field = Field {
        painted: true,
        division: Division::PerPale,
        tincture: Tincture::Vert,
        tincture2: Tincture::Or,
        charge: Device::Tower,
        charge_tincture: Tincture::Argent,
        charge_size: 0.5,
    };
    let mesh = drawable(&state);
    let cloth = part(&mesh, "cloth").unwrap();
    assert_eq!(cloth.tint, Some(Tincture::Vert.rgb()));
    let pattern = cloth.pattern.expect("a divided field has a pattern");
    assert_eq!(pattern.kind, PatternKind::PerPale);
    assert_eq!(pattern.color, Tincture::Or.rgb());
    let charge = part(&mesh, "charge").expect("a tower charge");
    assert_eq!(charge.tint, Some(Tincture::Argent.rgb()));
    // No charge, no part.
    state.field.charge = Device::None;
    assert!(part(&Banner::build(&state), "charge").is_none());
}

#[test]
fn hem_fringe_and_pole_follow_their_toggles() {
    let mut state = Banner::roll_as(9, BannerMount::Pole, BannerEnd::Swallowtail);
    state.hem.on = true;
    state.fringe.on = true;
    let on = drawable(&state);
    for name in ["cloth", "hem", "fringe", "rod", "loops", "finials", "pole", "pole_finial"] {
        assert!(part(&on, name).is_some(), "no {name}");
    }
    state.hem.on = false;
    state.fringe.on = false;
    state.hanging.mount = BannerMount::Rod;
    let off = Banner::build(&state);
    for name in ["hem", "fringe", "pole", "pole_finial"] {
        assert!(part(&off, name).is_none(), "{name} should be gone");
    }
}

#[test]
fn the_forge_view_lists_its_sliders_choices_and_toggles() {
    assert!(Family::Banner.is_ported());
    let mut item = Item::roll(Family::Banner, 7).unwrap();
    assert_eq!(item.family(), Family::Banner);
    let paths: Vec<String> = item.fields().into_iter().map(|f| f.path).collect();
    for path in [
        "cloth.width", "cloth.length", "cloth.end", "cloth.end_depth", "field.division", "field.tincture", "field.charge", "hem.on",
        "hem.width", "fringe.strands", "hanging.mount", "hanging.tabs", "cond.fray",
    ] {
        assert!(paths.iter().any(|p| p == path), "no {path} in {paths:?}");
    }
    assert_eq!(item.choice_options("cloth.end"), ["straight", "forked", "swallowtail", "pointed"]);
    let before = item.build();
    item.set_slider("cloth.width", 8.0).unwrap();
    item.set_choice("cloth.end", "pointed").unwrap();
    assert_ne!(item.build(), before);
    assert_eq!(item.slider("cloth.width"), Some(8.0));
    assert_eq!(item.set_slider("cloth.length", 99.0), Ok(LIMITS.cloth.length.max));
}

type V = [f32; 3];

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Signed distance from `p` to triangle `t` (positive on the side its winding faces).
fn signed_distance(p: V, t: [V; 3]) -> f32 {
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let len = dot(n, n).sqrt();
    let n = [n[0] / len, n[1] / len, n[2] / len];
    let plane = dot(sub(p, t[0]), n);
    let q = [p[0] - n[0] * plane, p[1] - n[1] * plane, p[2] - n[2] * plane];
    // Inside the triangle: the plane distance. Otherwise the nearest edge point.
    let inside = (0..3).all(|i| dot(cross(sub(t[(i + 1) % 3], t[i]), sub(q, t[i])), n) >= 0.0);
    if inside {
        return plane;
    }
    let mut best = f32::MAX;
    for i in 0..3 {
        let (a, b) = (t[i], t[(i + 1) % 3]);
        let ab = sub(b, a);
        let u = (dot(sub(p, a), ab) / dot(ab, ab)).clamp(0.0, 1.0);
        let c = [a[0] + ab[0] * u, a[1] + ab[1] * u, a[2] + ab[2] * u];
        let d = sub(p, c);
        best = best.min(dot(d, d).sqrt());
    }
    best.copysign(plane)
}

#[test]
fn the_charge_is_a_decal_on_the_cloths_front_and_back() {
    for seed in [2, 6, 11, 40] {
        let mut state = Banner::roll_as(seed, BannerMount::Rod, BannerEnd::Straight);
        state.hem.on = false;
        state.cloth.folds = 1.0;
        state.cloth.hang = 1.0;
        state.field.charge = Device::Cross;
        let mesh = drawable(&state);
        let (cloth, charge) = (part(&mesh, "cloth").unwrap(), part(&mesh, "charge").unwrap());
        let half = cloth.indices.len() / 2;
        let tris: Vec<[V; 3]> =
            cloth.indices[..half].chunks(3).map(|t| [t[0], t[1], t[2]].map(|i| cloth.positions[i as usize])).collect();
        let n = charge.positions.len() / 2;
        for (k, &p) in charge.positions.iter().enumerate() {
            // The nearest triangle's signed distance: the front half stands off the front.
            let d = tris.iter().map(|&t| signed_distance(p, t)).min_by(|a, b| a.abs().total_cmp(&b.abs())).unwrap();
            assert!(d.abs() <= 0.05, "seed {seed}: charge vertex {k} is {d} from the cloth");
            assert!((d > 0.0) == (k < n), "seed {seed}: charge vertex {k} is on the wrong side ({d})");
        }
    }
}

#[test]
fn the_decal_covers_the_whole_charge() {
    for device in Device::SIGNS {
        for seed in [4, 5, 9] {
            let mut state = Banner::roll_as(seed, BannerMount::Rod, BannerEnd::Straight);
            state.hem.on = false;
            (state.cloth.folds, state.cloth.hang, state.cloth.sag) = (0.0, 0.0, 0.0);
            state.cloth.width = 5.0;
            state.cloth.length = 12.0;
            state.field.charge = device;
            state.field.charge_size = 0.5;
            let size = 0.5 * 5.0;
            let expected: f32 = armory::build::device::device_shapes(device, seed)
                .iter()
                .map(|(piece, holes)| armory::geom::signed_area(piece).abs() - holes.iter().map(|h| armory::geom::signed_area(h).abs()).sum::<f32>())
                .sum::<f32>()
                * size
                * size;
            let mesh = drawable(&state);
            let charge = part(&mesh, "charge").unwrap();
            // The front half's triangles, projected on the cloth's plane.
            let half = charge.indices.len() / 2;
            let area: f32 = charge.indices[..half]
                .chunks(3)
                .map(|t| {
                    let [a, b, c] = [t[0], t[1], t[2]].map(|i| charge.positions[i as usize]);
                    ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
                })
                .sum();
            assert!((area / expected - 1.0).abs() < 0.02, "{device:?} seed {seed}: decal {area} against charge {expected}");
        }
    }
}

#[test]
fn the_charge_sits_on_the_fess_point_unless_the_foot_is_in_the_way() {
    let mut state = Banner::roll_as(3, BannerMount::Rod, BannerEnd::Straight);
    state.hem.on = false;
    state.field.charge = Device::Cross;
    state.field.charge_size = 0.5;
    let mid = |state: &BannerState| {
        let mesh = Banner::build(state);
        let (lo, hi) = bounds(part(&mesh, "charge").unwrap());
        ((lo[1] + hi[1]) * 0.5, hi[0] - lo[0])
    };
    let (y, width) = mid(&state);
    assert!((y + state.cloth.length * 0.5).abs() < 0.1, "centre {y}");
    assert!(width <= 0.8 * state.cloth.width + 0.01);
    state.cloth.end = BannerEnd::Forked;
    state.cloth.end_depth = 0.8;
    state.field.charge_size = 0.8;
    let (y, _) = mid(&state);
    assert!(y > -state.cloth.length * 0.5, "moved up to clear the notch: {y}");
}
