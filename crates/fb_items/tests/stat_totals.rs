use fb_items::{roll_item, stats::{Stat, Stats}, tables};

#[test]
fn stat_totals_pinned_across_base_seeds() {
    let test_cases = vec![
        // (base_name, seed, expected_stat_totals)
        ("Shirt", 1, 16.0),
        ("Shirt", 42, 6.0),
        ("Shirt", 12345, 5.5),
        ("Trousers", 1, 16.5),
        ("Trousers", 42, 6.5),
        ("Trousers", 12345, 6.0),
        ("Breeches", 1, 15.5),
        ("Breeches", 99, 30.5),
        ("Breeches", 54321, 7.5),
        ("Hood", 1, 15.5),
        ("Hood", 77, 6.0),
        ("Mail coif", 1, 21.0),
        ("Belt", 1, 15.0),
        ("Belt", 99, 30.0),
        ("Club", 1, 16.0),
        ("Club", 42, 6.0),
        ("Hand axe", 1, 16.0),
        ("Hand axe", 88, 6.0),
        ("Torch", 1, 35.0),
        ("Torch", 123, 37.0),
        ("Lantern", 1, 46.0),
        ("Lantern", 456, 36.0),
    ];

    for (base, seed, expected_total) in &test_cases {
        let item = roll_item(base, *seed).unwrap_or_else(|| panic!("Failed to roll item: {}", base));

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
