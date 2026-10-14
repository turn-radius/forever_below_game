//! What a rolled shape is to the game: its slot, tags, stats, grid footprint and inner grid.
//! The plate's `saMeta` (`reference/forever-below-plate.html`), with the footprint measured
//! from the built mesh (`docs/items.md`, "Stats and footprint").

use crate::dice::Dice;
use crate::footprint::{Footprint, Grid};
use crate::stats::Stats;
use crate::tables::{bases, food_descriptions, goods};
use crate::{Kind, Slot, Tag};
use armory::families::bags::BagStyle;
use armory::families::chests::ChestStyle;
use armory::families::shield::ShieldShape;
use armory::fabric::Textile;
use armory::sections::boot::BootStyle;
use armory::sections::garment::GarmentStyle;
use armory::sections::hand::HandStyle;
use armory::families::helm::HelmStyle;
use armory::{Family, Item, MaterialKind};

/// About 24 cm per inventory cell, in world units (10 cm).
pub const CELL_UNITS: f32 = 2.4;

/// Longest a staff, spear or bow can be and still be packed: six cells.
pub const PACK_LIMIT_UNITS: f32 = 14.4;

/// Cells covering `units`: `clamp(ceil(units / 2.4), 1, 9)`.
pub fn cells(units: f32) -> u8 {
    ((units / CELL_UNITS).ceil() as i32).clamp(1, 9) as u8
}

/// What an item is to the game, before affixes.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub kind: Kind,
    /// Where it is worn or held; `None` for goods.
    pub slot: Option<Slot>,
    pub tags: Vec<Tag>,
    pub stats: Stats,
    pub footprint: Footprint,
    /// Whether it fits in a pack at all. Long staves, spears and bows ride in the hand or on
    /// the back; chests stay where they are.
    pub packable: bool,
    /// The grid a container opens into.
    pub holds: Option<Grid>,
    /// How many share one cell.
    pub stack: u8,
    /// The card's line for goods ("Food · one day").
    pub description: String,
}

/// The longest and second-longest extents of the shape's mesh, in world units.
pub fn extents(shape: &Item) -> (f32, f32) {
    let Some((lo, hi)) = shape.build().bounds() else { return (0.0, 0.0) };
    let mut e = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    e.sort_by(|a, b| b.total_cmp(a));
    (e[0], e[1])
}

/// Families that fold or crumple, so their grid footprint is the plate's folded size rather
/// than their size as modelled on the mannequin.
fn folds(family: Family) -> bool {
    matches!(family, Family::Garment | Family::Hood | Family::Legs | Family::Hands | Family::Belt | Family::Bags)
}

/// The grid a chest, barrel, crate or amphora opens into: columns, rows and its label. The
/// plate gives chests no inner grid (its `saMeta` has none for `chests`), so these sizes are
/// our own: a chest as roomy as a satchel and a half, a crate and a barrel a little less, an
/// amphora's narrow belly a few cells, the small boxes fewer.
pub fn chest_grid(style: ChestStyle) -> (u8, u8, &'static str) {
    match style {
        ChestStyle::Chest => (8, 4, "Chest"),
        ChestStyle::Strongbox => (5, 3, "Strongbox"),
        ChestStyle::Coffer => (4, 2, "Coffer"),
        ChestStyle::Barrel => (4, 5, "Barrel"),
        ChestStyle::Crate => (6, 4, "Crate"),
        ChestStyle::Amphora => (3, 4, "Amphora"),
    }
}

/// The profile of `shape` rolled for base `base`. A base with its own row in
/// `data/bases.json` or `data/goods.json` takes its slot, tags and stats from it; the rest
/// derive them from the shape as `saMeta` does.
pub fn profile(base: &str, shape: &Item) -> Profile {
    let (long, wide) = extents(shape);
    let family = shape.family();
    let mut p = if let Some(row) = bases().get(base) {
        Profile {
            kind: Kind::Gear,
            slot: Some(row.slot),
            tags: row.tags.clone(),
            stats: Stats { weight: row.weight, armour: row.armour as f32, warmth: row.warmth, damage: row.damage, light: row.light, ..Stats::default() },
            footprint: row.footprint,
            packable: true,
            holds: row.holds.clone(),
            stack: 1,
            description: String::new(),
        }
    } else if let Some(row) = goods().get(base) {
        Profile {
            kind: Kind::Good,
            slot: None,
            tags: if row.food { vec![Tag::Food] } else { vec![] },
            stats: Stats { weight: row.weight, ..Stats::default() },
            footprint: row.footprint,
            packable: true,
            holds: None,
            stack: row.stack,
            description: row.description.clone(),
        }
    } else {
        from_shape(base, shape)
    };
    if !folds(family) {
        p.footprint = Footprint { cols: cells(wide), rows: cells(long) };
    }
    if matches!(family, Family::Staff | Family::Spear | Family::Bow) && long > PACK_LIMIT_UNITS {
        p.packable = false;
    }
    p
}

fn gear(slot: Slot, tags: &[Tag], stats: Stats, footprint: [u8; 2]) -> Profile {
    Profile {
        kind: Kind::Gear,
        slot: Some(slot),
        tags: tags.to_vec(),
        stats,
        footprint: footprint.into(),
        packable: true,
        holds: None,
        stack: 1,
        description: String::new(),
    }
}

fn good(tags: &[Tag], weight: f32, footprint: [u8; 2], description: &str) -> Profile {
    Profile {
        kind: Kind::Good,
        slot: None,
        tags: tags.to_vec(),
        stats: Stats { weight, ..Stats::default() },
        footprint: footprint.into(),
        packable: true,
        holds: None,
        stack: 1,
        description: description.to_string(),
    }
}

fn weapon(weight: f32, damage: Dice) -> Stats {
    Stats { weight, damage: Some(damage), ..Stats::default() }
}

fn worn(weight: f32, armour: f32, warmth: f32) -> Stats {
    Stats { weight, armour, warmth, ..Stats::default() }
}

/// `saMeta` for the families the plate's Armory had. Footprints here are the plate's; the
/// caller replaces them with measured ones for rigid shapes.
fn from_shape(base: &str, shape: &Item) -> Profile {
    use Tag::*;
    let d = Dice::new;
    match shape {
        Item::Dagger(_) => gear(Slot::Mainhand, &[Weapon, Metal], weapon(1.0, d(1, 4)), [1, 1]),
        Item::Sword(s) => {
            let bl = s.blade.length;
            let (wt, dmg) = if bl < 5.8 { (2.0, d(1, 6)) } else if bl < 9.0 { (3.0, d(1, 8)) } else { (6.0, d(2, 6)) };
            gear(Slot::Mainhand, &[Weapon, Metal], weapon(wt, dmg), [1, 1])
        }
        Item::Mace(s) => {
            let big = s.head.radius > 0.5;
            gear(Slot::Mainhand, &[Weapon, Metal], weapon(if big { 6.0 } else { 4.0 }, if big { d(1, 8) } else { d(1, 6) }), [1, 1])
        }
        Item::Staff(_) => gear(Slot::Mainhand, &[Weapon, Wood], weapon(4.0, d(1, 6)), [1, 1]),
        Item::Spear(s) => {
            let long = s.shaft.length > 18.0;
            gear(Slot::Mainhand, &[Weapon, Wood], weapon(if long { 8.0 } else { 3.0 }, if long { d(1, 10) } else { d(1, 8) }), [1, 1])
        }
        Item::Bow(s) => gear(Slot::Mainhand, &[Weapon, Wood], weapon(2.0, if s.limbs.length > 16.0 { d(1, 8) } else { d(1, 6) }), [1, 1]),
        Item::Crossbow(s) => {
            let heavy = s.stock.length > 7.5;
            gear(Slot::Mainhand, &[Weapon, Wood, Metal], weapon(if heavy { 8.0 } else { 5.0 }, if heavy { d(1, 10) } else { d(1, 8) }), [1, 1])
        }
        Item::Shield(s) => {
            let b = &s.body;
            // The plate had only tower, buckler and the rest; a pavise counts as a tower.
            let (ac, wt) = match b.shape {
                ShieldShape::Tower | ShieldShape::Pavise => (3.0, 15.0),
                ShieldShape::Buckler => (1.0, 2.0),
                _ => (2.0, 6.0),
            };
            let stuff = if b.material == MaterialKind::Wood { Wood } else { Metal };
            gear(Slot::Offhand, &[Armor, stuff], worn(wt, ac, 0.0), [1, 1])
        }
        Item::Helm(s) => {
            let st = s.shell.style;
            let ac = if matches!(st, HelmStyle::Bascinet | HelmStyle::GreatHelm | HelmStyle::Sallet) { 2.0 } else { 1.0 };
            let wt = match st {
                HelmStyle::GreatHelm => 6.0,
                HelmStyle::Skullcap => 2.0,
                _ => 4.0,
            };
            gear(Slot::Head, &[Metal, Armor, Wear], worn(wt, ac, 0.5), [2, 2])
        }
        Item::Garment(s) => {
            use GarmentStyle as G;
            let st = s.cut.style;
            let slot = match st {
                G::Cloak => Slot::Cloak,
                G::Tunic | G::Robe => Slot::Shirt,
                _ => Slot::Body,
            };
            let (ac, warmth, wt, metal) = match st {
                G::Tunic => (0.0, 1.0, 1.0, false),
                G::Robe => (0.0, 2.0, 3.0, false),
                G::Gambeson => (1.0, 3.0, 8.0, false),
                G::Hauberk => (3.0, 0.0, 20.0, true),
                G::Brigandine => (3.0, 1.0, 16.0, false),
                G::Scale => (4.0, 0.0, 22.0, true),
                G::Cuirass => (5.0, 0.0, 18.0, true),
                G::Cloak => (0.0, 3.0, 3.0, false),
            };
            let mantle = base == "Mantle";
            let mut tags = vec![if metal { Metal } else { Cloth }, Wear];
            if ac > 0.0 {
                tags.push(Armor);
            }
            if metal {
                tags.push(Quiet);
            }
            let stats = worn(if mantle { 1.5 } else { wt }, ac, if mantle { 1.5 } else { warmth });
            let fp = if st == G::Robe || (st == G::Cloak && !mantle) { [2, 4] } else { [2, 3] };
            Profile { tags, ..gear(slot, &[], stats, fp) }
        }
        Item::Hands(s) => {
            let st = s.hand.style;
            let plate = matches!(st, HandStyle::Gauntlet | HandStyle::Hourglass | HandStyle::Mitten);
            let warmth = if matches!(st, HandStyle::Glove | HandStyle::Mitten) { 0.5 } else { 0.0 };
            let tags: &[Tag] = if plate { &[Metal, Wear, Armor] } else { &[Leather, Wear] };
            gear(Slot::Hands, tags, worn(if plate { 3.0 } else { 0.5 }, if plate { 1.0 } else { 0.0 }, warmth), [2, 1])
        }
        Item::Feet(s) => {
            let b = &s.boot;
            let sabaton = b.style == BootStyle::Sabaton;
            let warmth = if b.style == BootStyle::Wrapped { 1.0 } else if b.height > 2.0 { 1.5 } else { 0.5 };
            let wt = if sabaton { 5.0 } else if b.height > 2.0 { 3.0 } else { 1.0 };
            let tags: &[Tag] = if sabaton { &[Metal, Wear, Quiet, Armor] } else { &[Leather, Wear, Quiet] };
            gear(Slot::Feet, tags, worn(wt, if sabaton { 1.0 } else { 0.0 }, warmth), [2, 2])
        }
        Item::Bags(s) => {
            let b = &s.bag;
            let (cols, rows, label, slot, wt, fp) = match b.style {
                BagStyle::Purse | BagStyle::Pouch => return good(&[Container], 0.4, [1, 1], "Coins"),
                BagStyle::Backpack => (8, 5, "Pack", Slot::Back, 4.0, [2, 3]),
                BagStyle::Satchel => (6, 3, "Satchel", Slot::Back, 2.0, [2, 2]),
                BagStyle::Sack => (4, 4, "Sack", Slot::Back, 0.5, [2, 2]),
                BagStyle::BeltPouch => (4, 2, "Belt pouch", Slot::Belt, 1.0, [2, 2]),
            };
            let stuff = if b.fabric == Textile::Leather { Leather } else { Cloth };
            Profile { holds: Some(Grid { cols, rows, label: label.to_string() }), ..gear(slot, &[Container, Wear, stuff], Stats { weight: wt, ..Stats::default() }, fp) }
        }
        Item::Ring(_) => gear(Slot::Ring1, &[Trinket], Stats { weight: 0.05, ..Stats::default() }, [1, 1]),
        Item::Amulet(_) => gear(Slot::Neck, &[Trinket], Stats { weight: 0.1, ..Stats::default() }, [1, 1]),
        Item::Charm(_) => gear(Slot::Charm, &[Trinket], Stats { weight: 0.1, ..Stats::default() }, [1, 1]),
        Item::Chests(s) => {
            let (cols, rows, label) = chest_grid(s.chest.style);
            Profile { packable: false, holds: Some(Grid { cols, rows, label: label.to_string() }), ..good(&[Container], 30.0, [4, 3], "Far too heavy to carry") }
        }
        Item::Food(s) => {
            use armory::families::food::FoodStyle as F;
            let st = s.provision.style;
            let wt = match st {
                F::Haunch | F::Bottle => 3.0,
                F::Tankard | F::Cheese => 2.0,
                F::Pie => 1.5,
                _ => 0.5,
            };
            let desc = food_descriptions().get(&st).map_or("Food", String::as_str);
            good(&[Food], wt, [1, 1], desc)
        }
        // Families the plate drew outside its Armory: every base of theirs has a table row,
        // so this is only reached by a base added without one.
        _ => good(&[], 1.0, [1, 1], ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armory_base::{ARMORY_BASES, roll_shape};

    fn rolled(base: &str, seed: u32) -> (Item, Profile) {
        let shape = roll_shape(base, seed).unwrap();
        let p = profile(base, &shape);
        (shape, p)
    }

    #[test]
    fn cells_follow_the_rule() {
        assert_eq!(cells(0.1), 1);
        assert_eq!(cells(2.4), 1);
        assert_eq!(cells(2.5), 2);
        assert_eq!(cells(40.0), 9);
    }

    #[test]
    fn a_long_spear_cannot_be_packed() {
        let (spear, p) = (1..400).map(|seed| rolled("Spear", seed)).find(|(s, _)| extents(s).0 >= 18.0).expect("no 1.8 m spear in 400 seeds");
        assert!(!p.packable, "{spear:?}");
        assert!(p.footprint.rows >= 8, "{:?}", p.footprint);
        let short = (1..400).map(|seed| rolled("Dagger", seed).1).all(|p| p.packable);
        assert!(short);
    }

    #[test]
    fn bags_have_their_inner_grids() {
        let grid = |base: &str| rolled(base, 7).1.holds.map(|g| (g.cols, g.rows));
        assert_eq!(grid("Pack"), Some((8, 5)));
        assert_eq!(grid("Satchel"), Some((6, 3)));
        assert_eq!(grid("Sack"), Some((4, 4)));
        assert_eq!(grid("Belt"), Some((4, 2)));
        assert_eq!(grid("Purse"), None);
    }

    #[test]
    fn a_daggers_footprint_matches_its_length() {
        for seed in 1..100 {
            let (shape, p) = rolled("Dagger", seed);
            let Item::Dagger(s) = &shape else { panic!() };
            let want = cells(s.shaft.length + s.blade.length) as i32;
            assert!((p.footprint.rows as i32 - want).abs() <= 1, "{seed}: {} rows for {want}", p.footprint.rows);
        }
    }

    #[test]
    fn every_base_has_a_profile() {
        for (name, _) in ARMORY_BASES {
            for seed in [1, 1207] {
                let (_, p) = rolled(name, seed);
                assert!(p.stats.weight > 0.0, "{name}");
                assert!((1..=9).contains(&p.footprint.cols) && (1..=9).contains(&p.footprint.rows), "{name}: {:?}", p.footprint);
                assert_eq!(p.kind == Kind::Gear, p.slot.is_some(), "{name}");
                if p.tags.contains(&Tag::Weapon) {
                    assert!(p.stats.damage.is_some(), "{name}");
                }
            }
        }
    }

    #[test]
    fn plate_stats_carry_over() {
        let (_, club) = rolled("Club", 3);
        assert_eq!((club.slot, club.stats.damage), (Some(Slot::Mainhand), Some(Dice::new(1, 4))));
        assert_eq!(rolled("Lantern", 3).1.stats.light, 30.0);
        let (_, bread) = rolled("Bread", 3);
        assert_eq!((bread.kind, bread.stack), (Kind::Good, 3));
        let (_, mantle) = rolled("Mantle", 3);
        assert_eq!((mantle.slot, mantle.stats.warmth), (Some(Slot::Cloak), 1.5));
    }
}
