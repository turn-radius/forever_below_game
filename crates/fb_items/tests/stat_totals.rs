use fb_items::{Odds, roll_item_with, stats::{Stat, Stats}, tables};

#[test]
fn stat_totals_pinned_across_base_seeds() {
    let test_cases = vec![
        // (base_name, seed, expected_stat_totals), rolled eight levels down, since the first
        // level now rolls nearly everything common. Four changed when the affixes nothing reads
        // were held back, and changed back when disease made disease risk read again: Shirt and
        // Trousers 12345 roll of Lesser Warding, Hood 77 Lice-ridden and Torch 123 Gleaming and
        // of the Ox. Every total rose by 1 when Speed (1 as usual) joined the stats, and the belts
        // roll among the new slot-bound suffixes. Shirt 12345, Trousers 12345 and Breeches 54321 changed
        // when the rolls moved eight levels down: they come up rare there. Breeches 99 (Mildewed)
        // and Belt 99 (Feeble) changed when flaws could roll on magic items. Nine changed when the
        // comfort suffixes took slots (Shirt 1 and 12345, Trousers 1, Breeches 1 and 99, Belt 1, Club 1,
        // Hand axe 1, Torch 123): the pool a draw lands in is smaller, so those seeds roll another affix.
        ("Shirt", 1, 9.0),
        ("Shirt", 42, 7.0),
        ("Shirt", 12345, 8.6),
        ("Trousers", 1, 9.5),
        ("Trousers", 42, 7.5),
        ("Trousers", 12345, 7.1),
        ("Breeches", 1, 8.5),
        ("Breeches", 99, 9.0),
        ("Breeches", 54321, 10.5),
        ("Hood", 1, 16.5),
        ("Hood", 77, 7.0),
        ("Mail coif", 1, 22.0),
        ("Belt", 1, 8.0),
        ("Belt", 99, 8.0),
        ("Club", 1, 8.0),
        ("Club", 42, 7.0),
        ("Hand axe", 1, 8.0),
        ("Hand axe", 88, 7.0),
        ("Torch", 1, 36.0),
        ("Torch", 123, 36.0),
        ("Lantern", 1, 47.0),
        ("Lantern", 456, 37.0),
    ];

    for (base, seed, expected_total) in &test_cases {
        let item = roll_item_with(base, *seed, Odds::loot(8)).unwrap_or_else(|| panic!("Failed to roll item: {}", base));

        // Sum all stat values for comparison
        let mut total = 0.0;
        for stat in Stat::ALL {
            total += item.profile.stats.get(stat).abs(); // abs() to handle both positive and negative values
        }

        // Allow small floating point differences (tolerance of 0.0001)
        assert!(
            (total - expected_total).abs() < 0.0001,
            "Stat totals mismatch for {} seed={}: expected {:.4}, got {:.4}",
            base, seed, expected_total, total
        );
    }
}

#[test]
fn rate_multipliers_stack_multiplicatively() {
    // Two "of Plenty" affixes should give hunger_rate 0.85 * 0.85 = 0.7225
    // Two "of Wakefulness" affixes should give sleepiness_rate 0.85 * 0.85 = 0.7225
    // This verifies that Scale modifiers stack multiplicatively.

    let affixes = tables::affixes();

    // Find "of Plenty" and apply it twice to hunger_rate
    let of_plenty = affixes
        .iter()
        .find(|a| a.name == "of Plenty")
        .unwrap_or_else(|| panic!("Could not find 'of Plenty' affix"));

    let mut stats = Stats::default();
    for modifier in &of_plenty.fx {
        modifier.apply_to_stats(&mut stats);
    }
    for modifier in &of_plenty.fx {
        modifier.apply_to_stats(&mut stats);
    }

    assert!(
        (stats.hunger_rate - 0.7225).abs() < 1e-6,
        "Two 'of Plenty' should give hunger_rate 0.7225, got {}", stats.hunger_rate
    );

    // Find "of Wakefulness" and apply it twice to sleepiness_rate
    let of_wakefulness = affixes
        .iter()
        .find(|a| a.name == "of Wakefulness")
        .unwrap_or_else(|| panic!("Could not find 'of Wakefulness' affix"));

    let mut stats = Stats::default();
    for modifier in &of_wakefulness.fx {
        modifier.apply_to_stats(&mut stats);
    }
    for modifier in &of_wakefulness.fx {
        modifier.apply_to_stats(&mut stats);
    }

    assert!(
        (stats.sleepiness_rate - 0.7225).abs() < 1e-6,
        "Two 'of Wakefulness' should give sleepiness_rate 0.7225, got {}", stats.sleepiness_rate
    );
}
