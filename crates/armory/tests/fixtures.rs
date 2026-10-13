//! The example fixtures from the prototype load cleanly. They show the range each family
//! covers; the Rust generators are not required to reproduce them.

use armory::{Family, Fixture};

fn load(f: Family) -> Fixture {
    let path = format!("{}/../../fixtures/armory/{}.json", env!("CARGO_MANIFEST_DIR"), f.key());
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&raw).unwrap()
}

#[test]
fn fixtures_load() {
    // Clubs and axes were never Armory families in the prototype, so they have no fixtures.
    for f in Family::ALL.into_iter().filter(|f| !matches!(f, Family::Club | Family::Axe)) {
        let fx = load(f);
        assert_eq!(fx.family, f);
        assert_eq!(fx.rows.len(), fx.count);
    }
}
