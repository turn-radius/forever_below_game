//! Food "Forge new": deterministic, in range, varied and coherent.

use armory::cond::Condition;
use armory::families::Generator;
use armory::families::food::{Brew, Food, FoodState, FoodStyle, GLOWCAP, LIMITS, Portion, freshness};
use armory::MaterialKind;

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<FoodState> {
    range.map(Food::roll).collect()
}

#[test]
fn same_seed_same_food() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Food::roll(seed), Food::roll(seed));
        assert_eq!(Food::roll(seed).seed, seed);
    }
    assert_ne!(Food::roll(1), Food::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Food::roll(1207);
    let back: FoodState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 977] {
        let s = Food::roll(seed);
        assert_eq!(Food::roll_as(seed, s.provision.style), s, "seed {seed}");
    }
}

#[test]
fn every_slider_within_range() {
    for s in rolls(1..=1024) {
        let bad = s.out_of_range(&LIMITS);
        assert!(bad.is_empty(), "seed {}: {bad:?}", s.seed);
    }
}

#[test]
fn rolls_are_coherent() {
    for s in rolls(1..=1024) {
        let (seed, p) = (s.seed, &s.provision);
        assert!(p.style.portions().contains(&p.portion), "seed {seed}: {:?} cannot be {:?}", p.style, p.portion);
        assert!(p.style.portions()[0] == Portion::Whole);
        if !p.style.is_drink() {
            assert_eq!(p.fill, 0.0, "seed {seed}: food with fill");
            assert!(p.seal.is_none(), "seed {seed}: food with a seal");
        }
        if p.glow > 0.0 {
            assert!(matches!(p.style, FoodStyle::Potion | FoodStyle::Mushroom), "seed {seed}: {:?} glows", p.style);
        }
        if p.style == FoodStyle::Potion {
            assert_eq!(Brew::nearest(p.liquid).rgb(), p.liquid, "seed {seed}: unnamed brew");
        }
        if p.style == FoodStyle::Waterskin {
            assert_eq!(p.vessel, MaterialKind::Leather, "seed {seed}");
        }
        assert_eq!(s.cond, Condition::default(), "seed {seed}: food is forged fresh");
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&FoodState) -> bool| all.iter().any(f);
    for style in FoodStyle::ALL {
        assert!(any(&|s| s.provision.style == style), "no {style:?}");
        for &portion in style.portions() {
            assert!(any(&|s| s.provision.style == style && s.provision.portion == portion), "no {portion:?} {style:?}");
        }
    }
    for brew in Brew::ALL {
        assert!(any(&|s| s.provision.style == FoodStyle::Potion && s.provision.liquid == brew.rgb()), "no {brew:?}");
    }
    for vessel in [MaterialKind::Wood, MaterialKind::Iron, MaterialKind::Silver, MaterialKind::Brass] {
        assert!(any(&|s| s.provision.style == FoodStyle::Tankard && s.provision.vessel == vessel), "no {vessel:?} tankard");
    }
    assert!(any(&|s| s.provision.style == FoodStyle::Flask && s.provision.vessel == MaterialKind::Glass), "no glass flask");
    assert!(any(&|s| s.provision.seal.is_some()), "no wax seals");
    assert!(any(&|s| s.provision.style == FoodStyle::Tankard && !s.provision.is_full()), "no empty tankard");
    assert!(any(&|s| s.provision.style == FoodStyle::Mushroom && s.provision.glow >= GLOWCAP), "no glowcap");
    assert!(any(&|s| s.provision.style == FoodStyle::Mushroom && s.provision.glow < GLOWCAP), "no plain mushroom");
}

fn of(style: FoodStyle, portion: Portion) -> FoodState {
    let mut s = Food::roll_as(5, style);
    s.provision.portion = portion;
    s
}

#[test]
fn nouns() {
    use FoodStyle as F;
    use Portion::*;
    let cases = [
        (F::Loaf, Whole, "Crusty Loaf"),
        (F::Loaf, Cut, "Cut Loaf"),
        (F::Loaf, Piece, "Slice of Bread"),
        (F::Loaf, Bitten, "Bitten Loaf"),
        (F::Boule, Whole, "Round Loaf"),
        (F::Boule, Piece, "Wedge of Bread"),
        (F::Cheese, Whole, "Cheese Wheel"),
        (F::Cheese, Cut, "Cut Cheese"),
        (F::Cheese, Piece, "Wedge of Cheese"),
        (F::Apple, Bitten, "Bitten Apple"),
        (F::Pear, Cut, "Cut Pear"),
        (F::Haunch, Whole, "Roast Haunch"),
        (F::Haunch, Bitten, "Gnawed Haunch"),
        (F::Fish, Whole, "Smoked Fish"),
        (F::Pie, Whole, "Meat Pie"),
        (F::Pie, Piece, "Slice of Pie"),
        // A portion the style cannot come in names (and builds) as whole.
        (F::Fish, Bitten, "Smoked Fish"),
        (F::Cheese, Bitten, "Cheese Wheel"),
    ];
    for (style, portion, name) in cases {
        assert_eq!(Food::noun(&of(style, portion)), name, "{style:?} {portion:?}");
    }

    let mut s = of(F::Tankard, Whole);
    s.provision.fill = 0.8;
    assert_eq!(Food::noun(&s), "Tankard of Ale");
    s.provision.fill = 0.0;
    assert_eq!(Food::noun(&s), "Empty Tankard");

    let mut s = of(F::Potion, Whole);
    s.provision.fill = 0.6;
    for brew in Brew::ALL {
        s.provision.liquid = brew.rgb();
        assert_eq!(Food::noun(&s), brew.name());
    }
    s.provision.liquid = armory::Rgb(0.8, 0.2, 0.25);
    assert_eq!(Food::noun(&s), "Healing Potion", "named for the nearest brew");
    s.provision.fill = 0.0;
    assert_eq!(Food::noun(&s), "Empty Vial");

    let mut s = of(F::Mushroom, Whole);
    s.provision.glow = 0.6;
    s.provision.count = 1;
    assert_eq!(Food::noun(&s), "Glowcap");
    s.provision.count = 3;
    assert_eq!(Food::noun(&s), "Glowcaps");
    s.provision.glow = 0.0;
    assert_eq!(Food::noun(&s), "Cave Mushrooms");

    let mut s = of(F::Flask, Whole);
    s.provision.vessel = MaterialKind::Glass;
    assert_eq!(Food::noun(&s), "Glass Flask");
    s.provision.vessel = MaterialKind::Leather;
    assert_eq!(Food::noun(&s), "Leather Flask");

    let mut s = of(F::Sausages, Whole);
    s.provision.count = 1;
    assert_eq!(Food::noun(&s), "Sausage");
    s.provision.count = 4;
    assert_eq!(Food::noun(&s), "String of Sausages");
}

#[test]
fn freshness_words() {
    let at = |wear| freshness(&Condition { wear, ..Condition::default() });
    assert_eq!(at(0.0), "Fresh");
    assert_eq!(at(0.2), "Day-old");
    assert_eq!(at(0.4), "Stale");
    assert_eq!(at(0.7), "Mouldy");
    assert_eq!(at(0.95), "Rotten");
}

#[test]
fn a_potion_rolls_in_four_sizes_and_the_noun_names_the_size() {
    use armory::families::food::PotionSize;
    let sizes: std::collections::HashSet<_> = (1..=1024).map(|s| Food::roll_as(s, FoodStyle::Potion)).map(|s| s.provision.potion_size()).collect();
    assert_eq!(sizes.len(), 4, "{sizes:?}");
    for size in PotionSize::ALL {
        assert_eq!(PotionSize::of(size.scale()), size);
        let s = Food::roll_potion(5, size, Brew::Mana);
        assert_eq!(Food::noun(&s), format!("{}Mana Potion", size.adjective()));
        assert!(s.out_of_range(&LIMITS).is_empty());
    }
    let top = |size| Food::build(&Food::roll_potion(5, size, Brew::Healing)).bounds().unwrap().1[1];
    assert!(top(PotionSize::Large) > top(PotionSize::Vial) * 1.5, "a large flask stands taller than a vial");
}

#[test]
fn potions_of_one_seed_and_size_differ_only_in_their_liquid() {
    use armory::families::food::PotionSize;
    for seed in [3, 77, 1207] {
        for size in PotionSize::ALL {
            let healing = Food::build(&Food::roll_potion(seed, size, Brew::Healing));
            let mana = Food::build(&Food::roll_potion(seed, size, Brew::Mana));
            assert_eq!(healing.parts.len(), mana.parts.len());
            let mut recoloured = 0;
            for (a, b) in healing.parts.iter().zip(&mana.parts) {
                assert_eq!((a.name, &a.positions, &a.indices), (b.name, &b.positions, &b.indices), "seed {seed}: one bottle");
                recoloured += usize::from(a.tint != b.tint);
            }
            assert!(recoloured >= 2, "the glass and the liquid take the colour");
        }
    }
}

#[test]
fn small_potions_roll_skinny_bottles_and_big_ones_round() {
    use armory::families::food::PotionSize;
    for seed in 1..=200 {
        for size in PotionSize::ALL {
            let s = Food::roll_potion(seed, size, Brew::Healing);
            let (lo, hi) = Food::build(&s).bounds().unwrap();
            let (width, height) = ((hi[0] - lo[0]).max(hi[2] - lo[2]), hi[1] - lo[1]);
            let skinny = matches!(size, PotionSize::Vial | PotionSize::Small);
            assert_eq!(s.provision.skinny(), skinny, "seed {seed} {size:?}");
            // A skinny bottle is about twice as tall as wide; a round one clearly squatter.
            if skinny {
                assert!(height / width > 1.9, "seed {seed} {size:?}: {height} by {width}");
            } else {
                assert!(height / width < 1.9, "seed {seed} {size:?}: {height} by {width}");
            }
        }
    }
}
