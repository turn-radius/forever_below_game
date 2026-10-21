//! Tents and campfire kits: deterministic, drawable, within their sliders, and the right size
//! in each form.

use armory::families::Generator;
use armory::families::campfire::{self, Campfire, CampfireState, FireForm, Rig};
use armory::families::tent::{self, Tent, TentForm, TentState, TentStyle};
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=40;

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

fn size(mesh: &ItemMesh) -> [f32; 3] {
    let (lo, hi) = mesh.bounds().expect("a mesh");
    [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]
}

fn drawable(mesh: &ItemMesh, what: &str) {
    assert!(!mesh.parts.is_empty(), "{what}: no parts");
    for p in &mesh.parts {
        p.validate().unwrap_or_else(|e| panic!("{what}: {e}"));
        assert!(!p.indices.is_empty(), "{what}: {} has no triangles", p.name);
        assert!(p.material.is_some(), "{what}: {} has no material", p.name);
    }
    let (lo, _) = mesh.bounds().unwrap();
    assert!(lo[1].abs() < 1e-3, "{what}: rests {} off the ground", lo[1]);
}

fn tents() -> impl Iterator<Item = (TentState, ItemMesh)> {
    SEEDS.flat_map(|seed| {
        [TentStyle::Ridge, TentStyle::Bell].into_iter().flat_map(move |style| {
            [TentForm::Pitched, TentForm::Packed].into_iter().map(move |form| {
                let s = Tent::roll_as(seed, style, form);
                let mesh = Tent::build(&s);
                (s, mesh)
            })
        })
    })
}

#[test]
fn same_seed_same_tent_and_mesh() {
    for seed in [1, 9, 1207] {
        assert_eq!(Tent::roll(seed), Tent::roll(seed));
        let s = Tent::roll(seed);
        assert_eq!(Tent::build(&s), Tent::build(&s));
        assert_eq!(Tent::roll_as(seed, s.tent.style, s.tent.form), s);
    }
    assert_ne!(Tent::roll(1), Tent::roll(2));
}

#[test]
fn every_tent_part_is_drawable_in_both_forms_and_styles() {
    for (s, mesh) in tents() {
        drawable(&mesh, &format!("tent {} {:?} {:?}", s.seed, s.tent.style, s.tent.form));
    }
}

#[test]
fn tent_sliders_hold() {
    for seed in 1..=300 {
        let s = Tent::roll(seed);
        let bad = s.out_of_range(&tent::LIMITS);
        assert!(bad.is_empty(), "seed {seed}: {bad:?}");
    }
    let mut s = Tent::roll(3);
    s.tent.height = 99.0;
    s.tent.guys = 40;
    s.tent.door_open = -1.0;
    s.fit(&tent::LIMITS);
    assert_eq!((s.tent.height, s.tent.guys, s.tent.door_open), (14.0, 8, 0.0));
    assert!(s.out_of_range(&tent::LIMITS).is_empty());
}

#[test]
fn a_pitched_tent_is_ten_to_fourteen_units_tall() {
    for (s, mesh) in tents().filter(|(s, _)| s.tent.form == TentForm::Pitched) {
        let h = size(&mesh)[1];
        assert!((s.tent.height..=s.tent.height + 1.2).contains(&h), "seed {} {:?}: {h} tall for a {} pole", s.seed, s.tent.style, s.tent.height);
        assert!((10.0..=15.3).contains(&h), "{h}");
    }
}

#[test]
fn the_extremes_of_every_slider_still_build() {
    for style in [TentStyle::Ridge, TentStyle::Bell] {
        for (lo, door, guys, pegs, patches) in [(true, 0.0, 0, 0, 0), (false, 1.0, 8, 4, 6)] {
            let mut s = Tent::roll_as(5, style, TentForm::Pitched);
            let t = &mut s.tent;
            let l = tent::LIMITS.tent;
            (t.width, t.length, t.height) = if lo { (l.width.min, l.length.min, l.height.min) } else { (l.width.max, l.length.max, l.height.max) };
            (t.door_open, t.guys, t.pegs, t.patches) = (door, guys, pegs, patches);
            for form in [TentForm::Pitched, TentForm::Packed] {
                s.tent.form = form;
                drawable(&Tent::build(&s), &format!("{style:?} {form:?} lo={lo}"));
            }
        }
    }
}

#[test]
fn the_pitched_forms_have_their_poles_flaps_guys_and_pegs() {
    for style in [TentStyle::Ridge, TentStyle::Bell] {
        let mut s = Tent::roll_as(7, style, TentForm::Pitched);
        (s.tent.guys, s.tent.pegs, s.tent.patches) = (4, 2, 3);
        let mesh = Tent::build(&s);
        for name in ["canvas", "flaps", "groundsheet", "poles", "guys", "pegs", "patches", "stitches"] {
            assert!(part(&mesh, name).is_some(), "{style:?} has no {name}");
        }
        (s.tent.guys, s.tent.pegs, s.tent.patches) = (0, 0, 0);
        let bare = Tent::build(&s);
        for name in ["guys", "pegs", "patches", "stitches"] {
            assert!(part(&bare, name).is_none(), "{style:?} still has {name}");
        }
    }
}

#[test]
fn opening_the_door_moves_the_flaps() {
    let mut s = Tent::roll_as(2, TentStyle::Ridge, TentForm::Pitched);
    s.tent.door_open = 0.0;
    let shut = Tent::build(&s);
    s.tent.door_open = 1.0;
    let open = Tent::build(&s);
    assert_ne!(part(&shut, "flaps"), part(&open, "flaps"));
    assert_eq!(part(&shut, "canvas"), part(&open, "canvas"));
}

#[test]
fn a_packed_tent_is_a_strapped_roll_with_poles_and_fits_a_pack() {
    for (s, mesh) in tents().filter(|(s, _)| s.tent.form == TentForm::Packed) {
        for name in ["roll", "poles", "straps", "pegs"] {
            assert!(part(&mesh, name).is_some(), "seed {} has no {name}", s.seed);
        }
        assert!(part(&mesh, "flaps").is_none() && part(&mesh, "guys").is_none());
        let [long, deep, tall] = size(&mesh);
        assert!(long > 6.0 && long < 14.4, "seed {}: {long} long", s.seed);
        assert!(deep < 6.0 && tall < 6.0, "seed {}: {deep} by {tall}", s.seed);
    }
}

#[test]
fn a_bigger_tent_rolls_fatter() {
    let (mut small, mut big) = (Tent::roll_as(4, TentStyle::Ridge, TentForm::Packed), Tent::roll_as(4, TentStyle::Ridge, TentForm::Packed));
    (small.tent.width, small.tent.length, small.tent.height) = (11.0, 13.0, 10.0);
    (big.tent.width, big.tent.length, big.tent.height) = (18.0, 21.0, 14.0);
    assert!(tent::packed_size(&big.tent).0 > tent::packed_size(&small.tent).0 * 1.3);
}

#[test]
fn every_canvas_builds() {
    for canvas in tent::Canvas::ALL {
        let mut s = Tent::roll_as(11, TentStyle::Ridge, TentForm::Pitched);
        s.tent.canvas = canvas;
        let mesh = Tent::build(&s);
        assert_eq!(part(&mesh, "canvas").unwrap().material, Some(canvas.material()));
    }
}

fn fires() -> impl Iterator<Item = (CampfireState, ItemMesh)> {
    SEEDS.flat_map(|seed| {
        [FireForm::Set, FireForm::Packed].into_iter().map(move |form| {
            let s = Campfire::roll_as(seed, form);
            let mesh = Campfire::build(&s);
            (s, mesh)
        })
    })
}

#[test]
fn same_seed_same_fire_and_mesh() {
    for seed in [1, 9, 1207] {
        assert_eq!(Campfire::roll(seed), Campfire::roll(seed));
        let s = Campfire::roll(seed);
        assert_eq!(Campfire::build(&s), Campfire::build(&s));
        assert_eq!(Campfire::roll_as(seed, s.fire.form), s);
    }
    assert_ne!(Campfire::roll(1), Campfire::roll(2));
}

#[test]
fn every_fire_part_is_drawable() {
    for (s, mesh) in fires() {
        drawable(&mesh, &format!("fire {} {:?}", s.seed, s.fire.form));
    }
}

#[test]
fn fire_sliders_hold() {
    for seed in 1..=300 {
        let s = Campfire::roll(seed);
        let bad = s.out_of_range(&campfire::LIMITS);
        assert!(bad.is_empty(), "seed {seed}: {bad:?}");
    }
    let mut s = Campfire::roll(3);
    s.fire.stones = 99;
    s.fire.kindling = 99;
    s.fire.size = 0.1;
    s.fit(&campfire::LIMITS);
    assert_eq!((s.fire.stones, s.fire.kindling, s.fire.size), (14, 10, 0.8));
}

#[test]
fn a_set_fire_has_its_stones_wood_and_rig() {
    for rig in [Rig::Bare, Rig::Hook, Rig::Tripod] {
        let mut s = Campfire::roll_as(6, FireForm::Set);
        (s.fire.rig, s.fire.pot, s.fire.lit, s.fire.logs, s.fire.kindling) = (rig, true, true, 4, 5);
        let mesh = Campfire::build(&s);
        for name in ["ash", "stones", "logs", "kindling", "flame"] {
            assert!(part(&mesh, name).is_some(), "{rig:?} has no {name}");
        }
        assert_eq!(part(&mesh, "rig").is_some(), rig != Rig::Bare, "{rig:?}");
        assert_eq!(part(&mesh, "pot").is_some(), rig != Rig::Bare, "{rig:?}: a pot hangs only from a rig");
        assert_eq!(part(&mesh, "chain").is_some(), rig != Rig::Bare);
        s.fire.pot = false;
        assert!(part(&Campfire::build(&s), "pot").is_none());
        s.fire.lit = false;
        assert!(part(&Campfire::build(&s), "flame").is_none());
    }
}

#[test]
fn a_tripod_stands_taller_than_the_fire_and_more_logs_make_a_taller_stack() {
    let mut s = Campfire::roll_as(8, FireForm::Set);
    (s.fire.rig, s.fire.pot, s.fire.lit, s.fire.size) = (Rig::Tripod, true, false, 1.0);
    let h = size(&Campfire::build(&s))[1];
    assert!((6.5..9.5).contains(&h), "{h}");
    (s.fire.rig, s.fire.kindling, s.fire.logs) = (Rig::Bare, 0, 2);
    let low = size(&Campfire::build(&s))[1];
    s.fire.logs = 8;
    assert!(size(&Campfire::build(&s))[1] > low);
}

#[test]
fn a_lit_set_fire_gives_light_and_a_packed_kit_does_not() {
    let mut s = Campfire::roll_as(8, FireForm::Set);
    s.fire.lit = true;
    assert!(Campfire::light(&s).is_some_and(|l| l.radius > 100.0));
    s.fire.lit = false;
    assert!(Campfire::light(&s).is_none());
    let mut packed = Campfire::roll_as(8, FireForm::Packed);
    packed.fire.lit = true;
    assert!(Campfire::light(&packed).is_none());
}

#[test]
fn a_packed_kit_is_a_tied_bundle_that_fits_a_pack() {
    for (s, mesh) in fires().filter(|(s, _)| s.fire.form == FireForm::Packed) {
        for name in ["sticks", "ties"] {
            assert!(part(&mesh, name).is_some(), "seed {} has no {name}", s.seed);
        }
        let [long, deep, tall] = size(&mesh);
        assert!(long < 14.4 && deep < 6.0 && tall < 6.0, "seed {}: {long} x {deep} x {tall}", s.seed);
    }
}

fn near(p: &Part, at: [f32; 3], tol: f32) -> bool {
    p.positions.iter().any(|v| (0..3).map(|i| (v[i] - at[i]).powi(2)).sum::<f32>().sqrt() <= tol)
}

#[test]
fn every_guy_is_taut_steep_and_ends_at_a_peg_head() {
    for seed in SEEDS {
        for style in [TentStyle::Ridge, TentStyle::Bell] {
            let mut s = Tent::roll_as(seed, style, TentForm::Pitched);
            (s.tent.guys, s.tent.pegs) = (8, 2);
            let mesh = Tent::build(&s);
            let (guys, pegs) = (part(&mesh, "guys").unwrap(), part(&mesh, "pegs").unwrap());
            let stakes = tent::stakes(&s.tent);
            assert_eq!(stakes.len(), 8);
            for st in stakes {
                assert!(near(guys, st.head, 0.15), "seed {seed} {style:?}: guy misses its peg head");
                assert!(near(pegs, st.head, 0.15), "seed {seed} {style:?}: no peg head at {:?}", st.head);
                assert!(st.descent() > 25f32.to_radians(), "seed {seed} {style:?}: {} degrees", st.descent().to_degrees());
                assert!(st.base[1].abs() < 1e-4);
            }
        }
    }
}

#[test]
fn a_bell_tents_guys_keep_clear_of_the_door() {
    for seed in SEEDS {
        let mut s = Tent::roll_as(seed, TentStyle::Bell, TentForm::Pitched);
        s.tent.guys = 8;
        for st in tent::stakes(&s.tent) {
            let a = (-st.from[2]).atan2(st.from[0]);
            assert!(a.abs() > 0.25, "seed {seed}: a guy at {a} rad is in the doorway");
        }
    }
}

#[test]
fn in_form_round_trips_and_changes_only_the_form() {
    let packed = Tent::roll_as(12, TentStyle::Ridge, TentForm::Packed);
    let pitched = Tent::in_form(&packed, TentForm::Pitched);
    assert_eq!(pitched.tent.form, TentForm::Pitched);
    let mut same = pitched.clone();
    same.tent.form = TentForm::Packed;
    assert_eq!(same, packed);
    assert_eq!(Tent::in_form(&pitched, TentForm::Packed), packed);
}
