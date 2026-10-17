use fb_items::{roll_item, stats::{Stat, Stats}, tables};

#[test]
fn stat_totals_pinned_across_base_seeds() {
    let test_cases = vec![
        // (base_name, seed, expected_stat_totals). Four changed when the affixes nothing reads
        // were held back: Shirt and Trousers 12345 roll of Wakefulness, Hood 77 Moth-eaten and
        // Torch 123 of Plenty in place of a held-back affix. Every total rose by 1 when Speed (1
        // as usual) joined the stats, and the belts roll among the new slot-bound suffixes.
        ("Shirt", 1, 17.0),
        ("Shirt", 42, 7.0),
        ("Shirt", 12345, 6.85),
        ("Trousers", 1, 17.5),
        ("Trousers", 42, 7.5),
        ("Trousers", 12345, 7.35),
        ("Breeches", 1, 16.5),
        ("Breeches", 99, 31.5),
        ("Breeches", 54321, 8.5),
        ("Hood", 1, 16.5),
        ("Hood", 77, 5.5),
        ("Mail coif", 1, 22.0),
        ("Belt", 1, 5.85),
        ("Belt", 99, 7.0),
        ("Club", 1, 17.0),
        ("Club", 42, 7.0),
        ("Hand axe", 1, 17.0),
        ("Hand axe", 88, 7.0),
        ("Torch", 1, 36.0),
        ("Torch", 123, 35.85),
        ("Lantern", 1, 47.0),
        ("Lantern", 456, 37.0),
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
