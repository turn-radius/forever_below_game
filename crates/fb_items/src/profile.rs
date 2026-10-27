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
use crate::range::Range;

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
    /// Where it is worn or held: always for gear, and for a good only if it is a light carried
    /// in the hand ([`held_light_slot`]); `None` for every other good.
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
    /// What a bow or crossbow shoots and how far (SRD 5.1 p. 66).
    pub ranged: Option<Ranged>,
    /// The card's line for goods ("Food · one day").
    pub description: String,
}

/// How many arrows or bolts share one cell.
pub const AMMUNITION_STACK: u8 = 10;

impl Profile {
    /// Sizes a worn bag's grid to its capacity affixes: its plain rows plus `stats.grid_rows`,
    /// never under one row. Call it once after the affixes are applied to the stats; a profile
    /// with no inner grid, or with no capacity change, is left alone.
    pub fn fit_capacity(&mut self) {
        let by = self.stats.grid_rows.round() as i32;
        if let Some(g) = self.holds.as_mut().filter(|_| by != 0) {
            g.rows = (g.rows as i32 + by).clamp(1, u8::MAX as i32) as u8;
        }
    }

    /// The columns and rows a plain bag of this size would open into, when its capacity
    /// affixes made this one differ; `None` for a bag at its plain size and for anything else.
    pub fn plain_grid(&self) -> Option<(u8, u8)> {
        let g = self.holds.as_ref()?;
        let by = self.stats.grid_rows.round() as i32;
        let plain = g.rows as i32 - by;
        (by != 0 && (1..=u8::MAX as i32).contains(&plain)).then_some((g.cols, plain as u8))
    }
}

/// The ammunition a bow or crossbow takes and its range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ranged {
    /// The family it shoots: [`Family::Arrow`] or [`Family::Bolt`].
    pub ammunition: Family,
    pub range: Range,
}

/// The longest and second-longest extents of the shape's mesh, in world units.
pub fn extents(shape: &Item) -> (f32, f32) {
    extents_of(shape.build().bounds())
}

/// [`extents`] from bounds already measured: building the mesh is what costs, so
/// [`profile`] builds it once for both this and [`upright_footprint`].
fn extents_of(bounds: Option<([f32; 3], [f32; 3])>) -> (f32, f32) {
    let Some((lo, hi)) = bounds else { return (0.0, 0.0) };
    let mut e = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    e.sort_by(|a, b| b.total_cmp(a));
    (e[0], e[1])
}

/// Whether a shape of `family` is shown standing as it is built rather than laid along its
/// longest side: worn gear as it is worn (the Armory builds it on the implied mannequin, +Y up
/// and its front toward +Z), and chests, barrels, crates and amphorae as they stand on the
/// floor. Its footprint runs the same way ([`upright_footprint`]), so the cells match the
/// shape drawn upright. Weapons, shields, lights, food and sundries are laid along their
/// longest side.
pub fn stands_upright(family: Family) -> bool {
    matches!(
        family,
        Family::Helm
            | Family::Hood
            | Family::Hat
            | Family::Garment
            | Family::Legs
            | Family::Hands
            | Family::Feet
            | Family::Belt
            | Family::Amulet
            | Family::Ring
            | Family::Charm
            | Family::Bags
            | Family::Chests
    )
}

/// Whether `shape` is shown standing as built: its family stands ([`stands_upright`]), or it has
/// a base it stands on ([`stands_on_base`]: a candle or chamberstick, a bottle, a tankard, a
/// pitched tent). Swords, torches, ropes and the like have none and are laid along their
/// longest side. Its footprint runs the same way.
pub fn shape_stands_upright(shape: &Item) -> bool {
    stands_upright(shape.family()) || stands_on_base(shape)
}

/// Whether `shape` has a base it stands on as built, and so rests on the floor as made rather
/// than laid on its largest face: helms, hats, boots, bags, chests, lanterns, food and drink
/// (the Armory builds each sitting on the table, bottles and tankards upright) and candles.
/// Weapons, shields, torches, clothes, jewellery and the other sundries have none. Our own
/// rule (the plate lays nothing on a floor).
pub fn stands_on_base(shape: &Item) -> bool {
    match shape {
        Item::Sundry(s) => s.article.kind == armory::families::sundry::SundryKind::Candle,
        Item::Tent(s) => s.tent.form == armory::families::tent::TentForm::Pitched,
        Item::Campfire(s) => s.fire.form == armory::families::campfire::FireForm::Set,
        _ => matches!(shape.family(), Family::Helm | Family::Hat | Family::Feet | Family::Bags | Family::Chests | Family::Lantern | Family::Food),
    }
}

/// `footprint` turned, if need be, to run the way `shape` stands as built: more columns than
/// rows for a shape wider (along X) than tall (along Y), more rows for one taller than wide.
/// The cell count stays the same; only which way the cells run follows the shape.
pub fn upright_footprint(footprint: Footprint, shape: &Item) -> Footprint {
    upright_footprint_of(footprint, shape.build().bounds())
}

/// [`upright_footprint`] from bounds already measured.
fn upright_footprint_of(footprint: Footprint, bounds: Option<([f32; 3], [f32; 3])>) -> Footprint {
    let Some((lo, hi)) = bounds else { return footprint };
    let (wide, tall) = (hi[0] - lo[0], hi[1] - lo[1]);
    let turned = Footprint { cols: footprint.rows, rows: footprint.cols };
    if (wide > tall && footprint.cols < footprint.rows) || (tall > wide && footprint.rows < footprint.cols) { turned } else { footprint }
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
    let bounds = shape.build().bounds();
    let (long, wide) = extents_of(bounds);
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
            ranged: None,
            description: String::new(),
        }
    } else if let Some(row) = goods().get(base) {
        Profile {
            kind: Kind::Good,
            slot: held_light_slot(shape),
            tags: if row.food { vec![Tag::Food] } else { vec![] },
            stats: Stats { weight: row.weight, ..Stats::default() },
            footprint: row.footprint,
            packable: true,
            holds: None,
            stack: row.stack,
            ranged: None,
            description: row.description.clone(),
        }
    } else {
        from_shape(base, shape)
    };
    // A potion keeps its goods row's footprint: a large flask stands two cells tall.
    let potion = matches!(shape, Item::Food(s) if s.provision.style == armory::families::food::FoodStyle::Potion) && goods().contains_key(base);
    if !folds(family) && !potion {
        p.footprint = Footprint { cols: cells(wide), rows: cells(long) };
    }
    if stands_upright(family) {
        p.footprint = upright_footprint_of(p.footprint, bounds);
    }
    if matches!(family, Family::Arrow | Family::Bolt) {
        p.footprint = Footprint { cols: 1, rows: 1 };
        p.stack = AMMUNITION_STACK;
    }
    if matches!(family, Family::Staff | Family::Spear | Family::Bow) && long > PACK_LIMIT_UNITS {
        p.packable = false;
    }
    // A tent standing or a fire built is where it was put; rolled up or bundled it goes in a pack.
    if !is_carried_form(shape) {
        p.packable = false;
    }
    p
}

/// Whether `shape` is in the form it is carried in: everything, but a pitched tent or a built fire.
fn is_carried_form(shape: &Item) -> bool {
    match shape {
        Item::Tent(s) => s.tent.form == armory::families::tent::TentForm::Packed,
        Item::Campfire(s) => s.fire.form == armory::families::campfire::FireForm::Packed,
        _ => true,
    }
}

/// Where a good that is a hand-held light is held: the off hand, where `data/bases.json` puts
/// the gear `Torch` and `Lantern`; a candle or chamberstick is held the same way. A spare torch is a torch, so it can be held wherever it is
/// found (pack, chest or floor) without becoming gear: it stays common, plain and described.
/// The plate had it only as a pack good; holding it is this port's own.
pub fn held_light_slot(shape: &Item) -> Option<Slot> {
    let light = match shape {
        Item::Sundry(s) => s.article.kind == armory::families::sundry::SundryKind::Candle,
        _ => matches!(shape.family(), Family::Torch | Family::Lantern),
    };
    light.then_some(Slot::Offhand)
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
        ranged: None,
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
        ranged: None,
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
        Item::Dagger(_) => gear(Slot::Mainhand, &[Weapon, Metal, Blade], weapon(1.0, d(1, 4)), [1, 1]),
        Item::Sword(s) => {
            let bl = s.blade.length;
            let (wt, dmg) = if bl < 5.8 { (2.0, d(1, 6)) } else if bl < 9.0 { (3.0, d(1, 8)) } else { (6.0, d(2, 6)) };
            gear(Slot::Mainhand, &[Weapon, Metal, Blade], weapon(wt, dmg), [1, 1])
        }
        Item::Mace(s) => {
            let big = s.head.radius > 0.5;
            gear(Slot::Mainhand, &[Weapon, Metal, Blunt], weapon(if big { 6.0 } else { 4.0 }, if big { d(1, 8) } else { d(1, 6) }), [1, 1])
        }
        Item::Staff(_) => gear(Slot::Mainhand, &[Weapon, Wood, Blunt], weapon(4.0, d(1, 6)), [1, 1]),
        Item::Spear(s) => {
            let long = s.shaft.length > 18.0;
            gear(Slot::Mainhand, &[Weapon, Wood], weapon(if long { 8.0 } else { 3.0 }, if long { d(1, 10) } else { d(1, 8) }), [1, 1])
        }
        Item::Bow(s) => {
            let long = s.limbs.length > 16.0;
            let range = if long { Range::new(150.0, 600.0) } else { Range::new(80.0, 320.0) };
            Profile { ranged: Some(self::Ranged { ammunition: Family::Arrow, range }), ..gear(Slot::Mainhand, &[Weapon, Wood, Tag::Ranged], weapon(2.0, if long { d(1, 8) } else { d(1, 6) }), [1, 1]) }
        }
        Item::Crossbow(s) => {
            let heavy = s.stock.length > 7.5;
            let range = if heavy { Range::new(100.0, 400.0) } else { Range::new(80.0, 320.0) };
            Profile {
                ranged: Some(self::Ranged { ammunition: Family::Bolt, range }),
                ..gear(Slot::Mainhand, &[Weapon, Wood, Metal, Tag::Ranged], weapon(if heavy { 8.0 } else { 5.0 }, if heavy { d(1, 10) } else { d(1, 8) }), [1, 1])
            }
        }
        Item::Shield(s) => {
            let b = &s.body;
            // The plate had only tower, buckler and the rest; a pavise counts as a tower. The
            // chance to turn a hit aside while the guard is up is our own: the more shield, the
            // more it covers.
            let (ac, wt, block) = match b.shape {
                ShieldShape::Tower | ShieldShape::Pavise => (3.0, 15.0, 0.6),
                ShieldShape::Buckler => (1.0, 2.0, 0.3),
                _ => (2.0, 6.0, 0.45),
            };
            let stuff = if b.material == MaterialKind::Wood { Wood } else { Metal };
            gear(Slot::Offhand, &[Armor, stuff], Stats { block_chance: block, ..worn(wt, ac, 0.0) }, [1, 1])
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
                BagStyle::Satchel => (6, 3, "Satchel", Slot::Satchel, 2.0, [2, 2]),
                BagStyle::Sack => (4, 4, "Sack", Slot::Back, 0.5, [2, 2]),
                BagStyle::BeltPouch => (4, 2, "Belt pouch", Slot::Belt, 1.0, [2, 2]),
            };
            let stuff = if b.fabric == Textile::Leather { Leather } else { Cloth };
            Profile { holds: Some(Grid { cols, rows, label: label.to_string(), accepts: vec![] }), ..gear(slot, &[Container, Bag, Wear, stuff], Stats { weight: wt, ..Stats::default() }, fp) }
        }
        // SRD 5.1 p. 69: an arrow weighs 0.05 lb and a bolt 0.075 lb. Each takes one cell.
        Item::Arrow(_) => good(&[Ammunition], 0.05, [1, 1], "Ammunition · arrow"),
        Item::Bolt(_) => good(&[Ammunition], 0.075, [1, 1], "Ammunition · crossbow bolt"),
        // A quiver or bolt case weighs 1 lb empty and holds 20 (SRD 5.1 p. 67-68), slung in a slot
        // of its own so it never vies with the lantern at the hip or the pack on the back.
        Item::Quiver(_) | Item::BoltCase(_) => {
            let (ammunition, label) = if matches!(shape, Item::Quiver(_)) { (Family::Arrow, "Quiver") } else { (Family::Bolt, "Bolt case") };
            let holds = Grid { cols: 5, rows: 4, label: label.to_string(), accepts: vec![ammunition] };
            Profile { holds: Some(holds), ..gear(Slot::Quiver, &[Container, Wear, Leather], Stats { weight: 1.0, ..Stats::default() }, [1, 3]) }
        }
        Item::Ring(_) => gear(Slot::Ring1, &[Trinket], Stats { weight: 0.05, ..Stats::default() }, [1, 1]),
        Item::Amulet(_) => gear(Slot::Neck, &[Trinket], Stats { weight: 0.1, ..Stats::default() }, [1, 1]),
        Item::Charm(_) => gear(Slot::Charm, &[Trinket], Stats { weight: 0.1, ..Stats::default() }, [1, 1]),
        Item::Chests(s) => {
            let (cols, rows, label) = chest_grid(s.chest.style);
            Profile { packable: false, holds: Some(Grid { cols, rows, label: label.to_string(), accepts: vec![] }), ..good(&[Container], 30.0, [4, 3], "Far too heavy to carry") }
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
                let (shape, p) = rolled(name, seed);
                assert!(p.stats.weight > 0.0, "{name}");
                assert!((1..=9).contains(&p.footprint.cols) && (1..=9).contains(&p.footprint.rows), "{name}: {:?}", p.footprint);
                match p.kind {
                    Kind::Gear => assert!(p.slot.is_some(), "{name}"),
                    Kind::Good => assert_eq!(p.slot, held_light_slot(&shape), "{name}"),
                }
                if p.tags.contains(&Tag::Weapon) {
                    assert!(p.stats.damage.is_some(), "{name}");
                }
            }
        }
    }

    fn worn_gear_runs_its_footprint_the_way_it_stands_check(pairs: std::ops::Range<u32>, base_seeds: &[u32], upright: &[&str], upright_seeds: std::ops::Range<u32>) {
        // A glove stands taller than wide, so its folded 2 by 1 footprint stands 1 by 2; a
        // belt lies wider than tall and keeps 2 by 1.
        for seed in pairs {
            let (_, gloves) = rolled("Gloves", seed);
            assert_eq!((gloves.footprint.cols, gloves.footprint.rows), (1, 2), "{seed}");
            let (_, belt) = rolled("Belt", seed);
            assert_eq!((belt.footprint.cols, belt.footprint.rows), (2, 1), "{seed}");
        }
        for (name, _) in ARMORY_BASES {
            for &seed in base_seeds {
                let (shape, p) = rolled(name, seed);
                if !stands_upright(shape.family()) {
                    continue;
                }
                let (lo, hi) = shape.build().bounds().unwrap();
                let (wide, tall) = (hi[0] - lo[0], hi[1] - lo[1]);
                let fp = p.footprint;
                assert!(!(wide > tall && fp.cols < fp.rows) && !(tall > wide && fp.rows < fp.cols), "{name} {seed}: {fp:?} for {wide} by {tall}");
            }
        }
        // Every style of shoe and boot (turnshoes, poulaines, wraps, ankle boots, sabatons)
        // stands sole down as built, toe to the side, and lies the way it stands.
        for &base in upright {
            for seed in upright_seeds.clone() {
                let (shape, p) = rolled(base, seed);
                let (lo, hi) = shape.build().bounds().unwrap();
                let (wide, tall) = (hi[0] - lo[0], hi[1] - lo[1]);
                let fp = p.footprint;
                assert!(stands_upright(shape.family()), "{base}");
                assert!(!(wide > tall && fp.cols < fp.rows) && !(tall > wide && fp.rows < fp.cols), "{base} {seed}: {fp:?} for {wide} by {tall}");
            }
        }
        assert!(stands_upright(Family::Hood) && stands_upright(Family::Chests));
        assert!(!stands_upright(Family::Sword) && !stands_upright(Family::Shield) && !stands_upright(Family::Torch));
    }

    #[test]
    #[ignore = "slow sweep: gear fitting test, every seed and base; run before merging fitting changes"]
    fn worn_gear_runs_its_footprint_the_way_it_stands() {
        worn_gear_runs_its_footprint_the_way_it_stands_check(1..20, &[1, 2, 3, 1207], &["Shoes", "Boots", "Mail coif", "Hood"], 1..200);
    }

    #[test]
    fn worn_gear_runs_its_footprint_the_way_it_stands_smoke() {
        worn_gear_runs_its_footprint_the_way_it_stands_check(1..3, &[1207], &["Shoes", "Boots", "Mail coif", "Hood"], 1..10);
    }

    #[test]
    fn candles_chambersticks_and_other_based_things_are_drawn_upright() {
        use armory::families::sundry::{Sundry, SundryKind};
        for holder in [false, true] {
            let mut s = Sundry::roll_as(6, SundryKind::Candle);
            s.article.holder = holder;
            let shape = Item::Sundry(s);
            assert!(shape_stands_upright(&shape), "candle holder {holder}");
            // The icon stands it; the cells it takes stay what they were.
            let (long, wide) = extents_of(shape.build().bounds());
            assert_eq!(profile("Candle", &shape).footprint, Footprint { cols: cells(wide), rows: cells(long) });
        }
        for base in ["Lantern", "Wine", "Pack", "Boots"] {
            assert!(shape_stands_upright(&rolled(base, 3).0), "{base}");
        }
        for base in ["Sword", "Torch", "Rope", "Crossbow", "Shield"] {
            assert!(!shape_stands_upright(&rolled(base, 3).0), "{base}");
        }
    }

    /// Footprints as they were before the icon pose and the footprint were told apart: the
    /// row's cells for what folds, else the shape's own cells, turned only for `stands_upright(family)`.
    #[test]
    fn existing_goods_keep_their_footprints() {
        for base in ["Bread", "Cheese", "Waterskin", "Blind cavefish", "Lantern", "Wine", "Tankard", "Candle"] {
            for seed in 1..40 {
                let (shape, p) = rolled(base, seed);
                let bounds = shape.build().bounds();
                let (long, wide) = extents_of(bounds);
                let family = shape.family();
                let row = bases().get(base).map(|r| r.footprint).or_else(|| goods().get(base).map(|r| r.footprint));
                let mut expected = row.unwrap_or(Footprint { cols: 1, rows: 1 });
                if !folds(family) {
                    expected = Footprint { cols: cells(wide), rows: cells(long) };
                }
                if stands_upright(family) {
                    expected = upright_footprint_of(expected, bounds);
                }
                assert_eq!(p.footprint, expected, "{base} {seed}");
            }
        }
        for family in [Family::Food, Family::Lantern, Family::Sundry] {
            assert!(!stands_upright(family), "{family:?}: its footprints were never turned");
        }
    }

    #[test]
    fn a_candle_is_held_in_a_hand_and_a_rope_is_not() {
        for seed in 1..40 {
            let (shape, p) = rolled("Candle", seed);
            assert_eq!(held_light_slot(&shape), Some(Slot::Offhand), "{seed}");
            assert_eq!(p.slot, Some(Slot::Offhand), "{seed}");
        }
        assert_eq!(rolled("Rope", 3).1.slot, None);
    }

    #[test]
    fn things_with_a_base_stand_on_it_and_the_rest_do_not() {
        for base in ["Pack", "Boots", "Lantern", "Candle", "Wine", "Helm"] {
            assert!(stands_on_base(&rolled(base, 3).0), "{base}");
        }
        for base in ["Crossbow", "Bow", "Shield", "Sword", "Torch", "Rope", "Tunic"] {
            assert!(!stands_on_base(&rolled(base, 3).0), "{base}");
        }
    }

    #[test]
    fn every_shield_blocks_and_the_bigger_ones_more() {
        let block = |seed| {
            let (shape, p) = rolled("Shield", seed);
            let Item::Shield(s) = shape else { unreachable!() };
            (s.body.shape, p.stats.block_chance)
        };
        let rolled: Vec<_> = (1..60).map(block).collect();
        for &(shape, chance) in &rolled {
            assert!(chance > 0.0, "{shape:?}");
        }
        let of = |want: ShieldShape| rolled.iter().find(|(s, _)| *s == want).map(|(_, c)| *c);
        let (buckler, tower) = (of(ShieldShape::Buckler).unwrap(), of(ShieldShape::Tower).or(of(ShieldShape::Pavise)).unwrap());
        assert!(buckler < tower);
    }

    #[test]
    fn ammunition_and_its_holders_follow_the_srd() {
        let (arrow, bolt) = (rolled("Arrow", 3).1, rolled("Bolt", 3).1);
        assert_eq!((arrow.stats.weight, bolt.stats.weight), (0.05, 0.075));
        assert!(arrow.tags.contains(&Tag::Ammunition) && bolt.tags.contains(&Tag::Ammunition));
        let (quiver, case) = (rolled("Quiver", 3).1, rolled("Bolt case", 3).1);
        let (q, c) = (quiver.holds.unwrap(), case.holds.unwrap());
        assert!(q.takes(Family::Arrow) && !q.takes(Family::Bolt) && !q.takes(Family::Food));
        assert!(c.takes(Family::Bolt) && !c.takes(Family::Arrow));
        assert!(q.cols as usize * q.rows as usize >= 20 && c.cols as usize * c.rows as usize >= 20);
        assert_eq!((quiver.stats.weight, case.stats.weight), (1.0, 1.0));
        // A bag takes anything.
        assert!(rolled("Pack", 3).1.holds.unwrap().takes(Family::Bolt));
    }

    #[test]
    fn bows_and_crossbows_name_their_ammunition_and_range() {
        let ranged = |base: &str, seed| rolled(base, seed).1.ranged.unwrap();
        let shorts = (1..200).map(|s| ranged("Bow", s)).collect::<Vec<_>>();
        assert!(shorts.iter().all(|r| r.ammunition == Family::Arrow));
        assert!(shorts.iter().any(|r| r.range == Range::new(80.0, 320.0)) && shorts.iter().any(|r| r.range == Range::new(150.0, 600.0)));
        let xbows = (1..200).map(|s| ranged("Crossbow", s)).collect::<Vec<_>>();
        assert!(xbows.iter().all(|r| r.ammunition == Family::Bolt));
        assert!(xbows.iter().any(|r| r.range == Range::new(80.0, 320.0)) && xbows.iter().any(|r| r.range == Range::new(100.0, 400.0)));
        assert!(rolled("Sword", 1).1.ranged.is_none());
    }

    #[test]
    fn plate_stats_carry_over() {
        let (_, club) = rolled("Club", 3);
        assert_eq!((club.slot, club.stats.damage), (Some(Slot::Mainhand), Some(Dice::new(1, 4))));
        assert_eq!(rolled("Lantern", 3).1.stats.light, 30.0);
        let (_, bread) = rolled("Bread", 3);
        assert_eq!((bread.kind, bread.stack), (Kind::Good, 3));
        let (_, spare) = rolled("Spare torch", 3);
        assert_eq!((spare.kind, spare.slot, spare.description.as_str()), (Kind::Good, Some(Slot::Offhand), "Light 20 ft · one hour"));
        let (_, mantle) = rolled("Mantle", 3);
        assert_eq!((mantle.slot, mantle.stats.warmth), (Some(Slot::Cloak), 1.5));
    }

    #[test]
    fn a_carried_tent_is_a_bulky_heavy_roll_that_fits_a_pack() {
        for seed in 1..12 {
            let (shape, p) = rolled("Tent", seed);
            let Item::Tent(t) = &shape else { panic!("{shape:?}") };
            assert_eq!(t.tent.form, armory::families::tent::TentForm::Packed);
            // SRD 5.1 p. 68: a two-person tent weighs 20 lb.
            assert_eq!((p.kind, p.stats.weight, p.packable), (Kind::Good, 20.0, true), "{seed}");
            let cells = p.footprint.cols as u32 * p.footprint.rows as u32;
            assert!((6..=16).contains(&cells), "{seed}: {:?}", p.footprint);
            assert!(!p.description.is_empty());
        }
    }

    #[test]
    fn a_pitched_tent_and_a_built_fire_stay_where_they_were_put() {
        use armory::families::{campfire::{Campfire, FireForm}, tent::{Tent, TentForm, TentStyle}};
        let pitched = Item::Tent(Tent::roll_as(4, TentStyle::Ridge, TentForm::Pitched));
        let p = profile("Tent", &pitched);
        assert!(!p.packable && stands_on_base(&pitched));
        let set = Item::Campfire(Campfire::roll_as(4, FireForm::Set));
        assert!(!profile("Campfire kit", &set).packable && stands_on_base(&set));
        let (packed, p) = rolled("Campfire kit", 4);
        assert!(p.packable && !stands_on_base(&packed));
        assert_eq!((p.kind, p.stats.weight), (Kind::Good, 12.0));
    }
}
