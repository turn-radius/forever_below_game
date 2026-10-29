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
        // Every total rose by 1 again when attack rate (1 as usual) joined the stats, and by 2 more
        // with stamina refill and cost. Trousers 12345 and Breeches 99 now roll another affix, since of
        // Endurance joined the leg slot's pool. Club 1, Hand axe 1, Torch 1 and 123 and Lantern 1
        // now roll another affix, since of Thieves came back into the pools when lockpicking was read again.
        // Of Thieves now rolls only on gloves, rings, amulets, charms and belts, and a rare or
        // legendary slot with nothing that fits takes the other kind, so Club 1, Hand axe 1, Torch 1
        // and 123 and Lantern 1 roll other affixes (a lantern now has prefixes to fall back on). Torch 1 and 123 and Lantern 1 changed again when lights got
        // higher tiers of Gleaming and Long-burning and the suffixes of the Ember, the Furnace and Patience.
        ("Shirt", 1, 12.0),
        ("Shirt", 42, 10.0),
        ("Shirt", 12345, 11.6),
        ("Trousers", 1, 12.5),
        ("Trousers", 42, 10.5),
        ("Trousers", 12345, 9.7),
        ("Breeches", 1, 11.5),
        ("Breeches", 99, 9.8),
        ("Breeches", 54321, 13.5),
        ("Hood", 1, 19.5),
        ("Hood", 77, 10.0),
        ("Mail coif", 1, 25.0),
        ("Belt", 1, 11.0),
        ("Belt", 99, 11.0),
        ("Club", 1, 11.0),
        ("Club", 42, 10.0),
        ("Hand axe", 1, 11.0),
        ("Hand axe", 88, 10.0),
        ("Torch", 1, 31.0),
        ("Torch", 123, 53.0),
        ("Lantern", 1, 42.0),
        ("Lantern", 456, 40.0),
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
