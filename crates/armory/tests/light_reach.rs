//! How far torches and lanterns reach: about the SRD's 20 ft for a torch and 30 ft for a
//! hooded lantern, and every lantern farther than any torch, whatever its style, fuel or
//! flame.

use armory::build::light::UNITS_PER_FOOT;
use armory::families::Generator;
use armory::families::lantern::{self, Lantern, LanternFuel, LanternState, LanternStyle};
use armory::families::torch::{self, BurnState, Fuel, Torch, TorchState, TorchStyle};

/// A torch's reach in feet, burning with its flame at `flame`.
fn torch_feet(mut s: TorchState, flame: f32) -> f32 {
    s.burn.state = BurnState::Burning;
    s.burn.flame = flame;
    Torch::light(&s).unwrap().radius / UNITS_PER_FOOT
}

/// A lantern's reach in feet, lit with its flame at `flame`.
fn lantern_feet(mut s: LanternState, flame: f32) -> f32 {
    s.flame.lit = true;
    s.flame.flame = flame;
    Lantern::light(&s).unwrap().radius / UNITS_PER_FOOT
}

#[test]
fn every_lantern_reaches_farther_than_any_torch() {
    let torch_flame = [torch::LIMITS.burn.flame.min, torch::LIMITS.burn.flame.max];
    let lantern_flame = [lantern::LIMITS.flame.flame.min, lantern::LIMITS.flame.flame.max];
    let farthest_torch = (0..1000).flat_map(|seed| torch_flame.map(|f| torch_feet(Torch::roll(seed), f))).fold(f32::MIN, f32::max);
    let nearest_lantern = (0..1000).flat_map(|seed| lantern_flame.map(|f| lantern_feet(Lantern::roll(seed), f))).fold(f32::MAX, f32::min);
    assert!(nearest_lantern > farthest_torch, "nearest lantern {nearest_lantern} ft, farthest torch {farthest_torch} ft");
}

#[test]
fn a_horn_oil_lantern_reaches_about_30_ft_and_a_rag_torch_about_20() {
    let horn = (0..400).map(|seed| Lantern::roll_as(seed, LanternStyle::Horn)).find(|s| s.flame.fuel == LanternFuel::Oil).unwrap();
    assert!((lantern_feet(horn.clone(), 1.0) - 30.0).abs() < 1.5, "{}", lantern_feet(horn, 1.0));
    let rag = (0..400).map(|seed| Torch::roll_as(seed, TorchStyle::Rag)).find(|s| s.head.fuel != Fuel::Tallow).unwrap();
    assert!((torch_feet(rag.clone(), 1.0) - 20.0).abs() < 1.0, "{}", torch_feet(rag, 1.0));
}

#[test]
fn style_and_fuel_still_vary_the_reach() {
    let torches: Vec<f32> = (0..300).map(|seed| torch_feet(Torch::roll(seed), 1.0)).collect();
    let lanterns: Vec<f32> = (0..300).map(|seed| lantern_feet(Lantern::roll(seed), 1.0)).collect();
    let spread = |v: &[f32]| v.iter().copied().fold(f32::MIN, f32::max) - v.iter().copied().fold(f32::MAX, f32::min);
    assert!(spread(&torches) > 2.0, "torches spread {}", spread(&torches));
    assert!(spread(&lanterns) > 4.0, "lanterns spread {}", spread(&lanterns));
}
