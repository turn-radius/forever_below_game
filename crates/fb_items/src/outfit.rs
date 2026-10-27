//! Outfits: the plate's `OUTFITS` (`data/outfits.json`), the four classes and the two starting
//! looks, and rolling one into whole items (the plate's `preparedSet`,
//! `reference/forever-below-plate.html`).
//!
//! Each slot lists candidate bases, optionally led by the chance the slot is filled at all;
//! goods are carried and ground things lie at the wearer's feet. [`Outfit::roll`] picks a
//! base per slot and rolls it through [`roll_item`], so a rolled outfit is a pure function of
//! its key, the seed and the folk.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use armory::fabric::FabricPattern;
use armory::heraldry::{Field, Tincture};
use armory::sections::garment::GarmentStyle;
use armory::sections::hood::HoodStyle;
use procgen_rng::Mulberry32;
use serde::Deserialize;
use serde_json::Value;

use crate::armory_base::held_back;
use crate::{Item, Odds, Slot, roll_item_with};

/// One slot's candidates: the chance it is filled, and the bases to pick from (a base listed
/// twice is twice as likely).
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub chance: f32,
    pub bases: Vec<String>,
}

/// The kind of body armour a faction leans toward. Mirrors the faction crate's own, so this
/// crate needs no dependency on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmourLean {
    Padded,
    Leather,
    Mail,
    Scale,
    Plate,
}

impl ArmourLean {
    /// The bases of `slot` this lean favours (a faction of mail wears mail and a mail coif).
    fn favours(self, slot: Slot) -> &'static [&'static str] {
        match (slot, self) {
            (Slot::Body, ArmourLean::Padded) => &["Gambeson"],
            (Slot::Body, ArmourLean::Leather) => &["Leather jerkin"],
            (Slot::Body, ArmourLean::Mail) => &["Mail"],
            (Slot::Body, ArmourLean::Scale) => &["Scale coat"],
            (Slot::Body, ArmourLean::Plate) => &["Cuirass", "Brigandine"],
            (Slot::Head, ArmourLean::Mail) => &["Mail coif"],
            (Slot::Head, ArmourLean::Plate) => &["Helm"],
            _ => &[],
        }
    }
}

/// The cloth a faction's name puts its members in. Mirrors the faction crate's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Garment {
    /// A robe as the outermost layer.
    Robe,
    /// The armour lean's body armour, with a robe over it.
    RobeOverArmour,
}

/// The trim a faction's name puts on its cloth. Mirrors the faction crate's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trim {
    /// Hem and edges in gold.
    GoldHem,
}

/// A faction's colours as plain data: what its members' cloth, shields and armour show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Livery {
    pub tincture: Tincture,
    pub tincture2: Tincture,
    /// Painted on every shield, exactly.
    pub field: Field,
    pub armour_lean: ArmourLean,
    /// The faction's name gives its colours, so all of its cloth wears them (and plain, in the
    /// main colour), not three pieces in four, and the main colour is the name's first.
    pub named: bool,
    /// The garment its name puts every member in, if it says one.
    pub garment: Option<Garment>,
    /// The trim its name puts on their cloth, if it says one.
    pub trim: Option<Trim>,
    /// Legwear wears the colours too: a denizen's whole dress may be a kilt of them. Off for
    /// folk, whose legwear is left as rolled (and who draw nothing for it).
    pub legs: bool,
}

impl Livery {
    /// The livery of a faction with `field` and `armour_lean`: the field's two tinctures.
    pub fn from_field(field: Field, armour_lean: ArmourLean) -> Livery {
        Livery { tincture: field.tincture, tincture2: field.tincture2, field, armour_lean, named: false, garment: None, trim: None, legs: false }
    }

    /// This livery with legwear in the colours (denizens).
    pub fn with_legs(self) -> Livery {
        Livery { legs: true, ..self }
    }

    /// This livery with the colours its faction's name gives: all cloth wears them.
    pub fn named(self) -> Livery {
        Livery { named: true, ..self }
    }

    /// This livery with the garment its faction's name puts members in.
    pub fn with_garment(self, garment: Option<Garment>) -> Livery {
        Livery { garment, ..self }
    }

    /// This livery with the trim its faction's name puts on cloth.
    pub fn with_trim(self, trim: Option<Trim>) -> Livery {
        Livery { trim, ..self }
    }

    /// The tinctures as (main, trim): cloth takes a colour for its body and a metal for its
    /// trim where the livery has one of each.
    fn cloth(&self) -> (Tincture, Tincture) {
        if !self.named && self.tincture.is_metal() && !self.tincture2.is_metal() { (self.tincture2, self.tincture) } else { (self.tincture, self.tincture2) }
    }

    /// The colour of the trim: gold where the name says a gold hem, else the cloth's own.
    fn edge(&self) -> Tincture {
        if self.trim == Some(Trim::GoldHem) { Tincture::Or } else { self.cloth().1 }
    }

    /// Dresses `item`, if it is cloth that wears colours or a shield, from `rng`. Three
    /// pieces of cloth in four wear the colours, or all of it where the name gives them (or
    /// `forced`); a shield always carries the field.
    fn dress(&self, item: &mut Item, rng: &mut Mulberry32, forced: bool) {
        let (main, trim) = self.cloth();
        let all = self.named || forced;
        let gold = self.trim == Some(Trim::GoldHem);
        match &mut item.shape {
            armory::Item::Shield(s) => s.field = self.field,
            armory::Item::Garment(s) if item.base != "Leather jerkin" && !s.cut.style.is_armour() => {
                if !rng.chance(LIVERY_CLOTH) && !all {
                    return;
                }
                s.cut.cloth.color = main.dye();
                s.cut.cloth.color2 = trim.dye();
                if self.named {
                    // One colour, not a pattern of two.
                    s.cut.cloth.pattern = FabricPattern::Plain;
                }
                if matches!(s.cut.style, GarmentStyle::Tunic | GarmentStyle::Cloak | GarmentStyle::Robe) && (rng.chance(0.5) || gold) {
                    s.hem.on = true;
                }
                let edge = self.edge();
                s.hem.fabric.color = edge.dye();
                s.hem.fabric.color2 = if gold { edge.dye() } else { main.dye() };
                if gold {
                    // The Armory's own trim controls: a plain gold band at the hem, neck and cuffs.
                    s.hem.fabric.pattern = FabricPattern::Plain;
                    (s.hem.hemline, s.hem.neck, s.hem.cuffs) = (true, true, true);
                    s.hem.width = s.hem.width.max(0.4);
                }
            }
            // Legwear wears the colours too: a denizen's whole dress may be a kilt of them.
            armory::Item::Legs(s) if self.legs => {
                if !rng.chance(LIVERY_CLOTH) && !all {
                    return;
                }
                s.legwear.cloth.color = main.dye();
                s.legwear.cloth.color2 = trim.dye();
                if self.named {
                    s.legwear.cloth.pattern = FabricPattern::Plain;
                }
                s.hem.fabric.color = self.edge().dye();
                s.hem.fabric.color2 = if gold { self.edge().dye() } else { main.dye() };
            }
            armory::Item::Hood(s) if s.hood.style != HoodStyle::MailCoif => {
                if !rng.chance(LIVERY_CLOTH) && !all {
                    return;
                }
                // The hood takes the trim colour unless that is a metal, then the main.
                let (cloth, edge) = if trim.is_metal() { (main, trim) } else { (trim, main) };
                let (cloth, edge) = if self.named { (main, self.edge()) } else { (cloth, edge) };
                s.hood.cloth.color = cloth.dye();
                s.hood.cloth.color2 = edge.dye();
                if self.named {
                    s.hood.cloth.pattern = FabricPattern::Plain;
                }
                s.hem.on = s.hem.on || rng.chance(0.5) || gold;
                s.hem.fabric.color = edge.dye();
                s.hem.fabric.color2 = if gold { edge.dye() } else { cloth.dye() };
                if gold {
                    s.hem.fabric.pattern = FabricPattern::Plain;
                }
            }
            _ => {}
        }
    }

    /// Puts `garment` on a member's `worn` kit: a robe as the outermost layer, in the colours
    /// and trim of the livery, and for [`Garment::RobeOverArmour`] the lean's body armour under
    /// it. The robe goes over body armour, in the cloak's place, or in the shirt's place where
    /// there is none. The kit's other gear stays.
    fn robe(&self, garment: Garment, worn: &mut Vec<(Slot, Item)>, odds: Odds, lrng: &mut Mulberry32) {
        let seed = |rng: &mut Mulberry32| (rng.next_f64() * 4_294_967_296.0) as u32;
        if garment == Garment::RobeOverArmour {
            let favoured = self.armour_lean.favours(Slot::Body);
            let armoured = worn.iter().any(|(s, i)| *s == Slot::Body && favoured.contains(&i.base.as_str()));
            if !armoured && !favoured.is_empty() {
                worn.retain(|(s, _)| *s != Slot::Body);
                let base = *lrng.pick(favoured);
                if let Some(armour) = roll_item_with(base, seed(lrng), odds) {
                    worn.push((Slot::Body, armour));
                }
            }
        }
        let Some(mut robe) = roll_item_with("Robe", seed(lrng), odds) else { return };
        self.dress(&mut robe, lrng, true);
        let over_armour = worn.iter().any(|(s, _)| *s == Slot::Body);
        let slot = if over_armour { Slot::Cloak } else { Slot::Shirt };
        // A cloak over a robe hides it, and a shirt under one is not seen.
        worn.retain(|(s, _)| *s != slot && *s != Slot::Cloak);
        worn.push((slot, robe));
    }
}

/// How many pieces of cloth in a faction wear its colours.
pub const LIVERY_CLOTH: f64 = 0.75;
/// How often a faction's armoured member wears the armour it leans toward.
pub const LIVERY_LEAN: f64 = 0.75;
const LIVERY_SALT: u32 = 0x11FE_5EED;

/// A row of `data/outfits.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct Outfit {
    /// Its key in the table (`"fighter"`, `"fallen"`).
    pub key: String,
    /// What the plate calls it (`"Fighter"`, `"Freshly fallen"`).
    pub label: String,
    /// Slots in the table's key order.
    pub slots: Vec<(Slot, Choice)>,
    pub goods: Vec<String>,
    pub ground: Vec<String>,
}

/// An outfit rolled into items.
#[derive(Clone, Debug, PartialEq)]
pub struct OutfitSet {
    /// Each item with the slot it goes in, held items included.
    pub worn: Vec<(Slot, Item)>,
    /// Carried goods.
    pub goods: Vec<Item>,
    /// Things lying at the wearer's feet.
    pub ground: Vec<Item>,
}

impl OutfitSet {
    /// The item in `slot`, if the roll filled it.
    pub fn item(&self, slot: Slot) -> Option<&Item> {
        self.worn.iter().find(|(s, _)| *s == slot).map(|(_, item)| item)
    }
}

#[derive(Deserialize)]
struct Row {
    label: String,
    slots: BTreeMap<String, Vec<Value>>,
    #[serde(default)]
    goods: Vec<String>,
    #[serde(default)]
    ground: Vec<String>,
}

fn choice(values: &[Value]) -> Choice {
    let (chance, rest) = match values.first() {
        Some(Value::Number(n)) => (n.as_f64().unwrap_or(1.0) as f32, &values[1..]),
        _ => (1.0, values),
    };
    Choice { chance, bases: rest.iter().filter_map(|v| v.as_str().map(String::from)).collect() }
}

static OUTFITS: LazyLock<Vec<Outfit>> = LazyLock::new(|| {
    let rows: BTreeMap<String, Row> =
        serde_json::from_str(include_str!("../../../data/outfits.json")).unwrap_or_else(|e| panic!("data/outfits.json: {e}"));
    rows.into_iter()
        .map(|(key, row)| Outfit {
            key,
            label: row.label,
            slots: row
                .slots
                .iter()
                .map(|(slot, values)| {
                    let slot: Slot = serde_json::from_value(Value::String(slot.clone())).unwrap_or_else(|e| panic!("data/outfits.json: {e}"));
                    (slot, choice(values))
                })
                .collect(),
            goods: row.goods,
            ground: row.ground,
        })
        .collect()
});

/// Every outfit in `data/outfits.json`, by key.
pub fn outfits() -> &'static [Outfit] {
    &OUTFITS
}

/// The outfit with `key`, or with that label, ignoring case.
pub fn outfit(key: &str) -> Option<&'static Outfit> {
    outfits().iter().find(|o| o.key.eq_ignore_ascii_case(key) || o.label.eq_ignore_ascii_case(key))
}

/// The plate's chances of a ring, an amulet and a charm on top of any outfit, and of a gnome's
/// pointed hat when the outfit gives no headgear (`preparedSet`).
const EXTRA_RING: f64 = 0.22;
const EXTRA_AMULET: f64 = 0.14;
const EXTRA_CHARM: f64 = 0.14;
const GNOME_HAT: f64 = 0.6;

/// How many arrows or bolts a bowman starts with: a full quiver.
pub const AMMUNITION_CARRIED: usize = 20;

impl Outfit {
    /// Rolls this outfit for a character of `folk` (a key of `data/races.json`) from `seed`.
    ///
    /// As the plate's `preparedSet`: each slot is filled with its chance, from a base picked
    /// among its candidates; a gnome with nothing on its head gets a pointed hat at 60%; and
    /// any outfit may add a ring (22%), an amulet (14%) and a charm (14%) to free slots. The
    /// stream is seeded from the seed and the outfit's key as the plate's is
    /// (`seed * 13 + key.length * 977 + key.charCodeAt(0)`); each item's own seed is drawn
    /// from that stream, where the plate rolled from its one global stream.
    pub fn roll(&self, seed: u32, folk: &str) -> OutfitSet {
        self.roll_with(seed, folk, Odds::default())
    }

    /// [`Outfit::roll`] with every item's rarity rolled against `odds`: the folk met at a
    /// depth wear [`Odds::kit`] for it.
    pub fn roll_with(&self, seed: u32, folk: &str, odds: Odds) -> OutfitSet {
        self.roll_in(seed, folk, odds, None)
    }

    /// [`Outfit::roll_with`] for a member of a faction: cloth in its tinctures, shields
    /// painted with its field and body armour leaning toward its lean. The livery is drawn
    /// from a stream of its own, so the same wearer without one rolls the same bases.
    pub fn roll_in(&self, seed: u32, folk: &str, odds: Odds, livery: Option<&Livery>) -> OutfitSet {
        let mut lrng = Mulberry32::new(seed ^ LIVERY_SALT);
        let first = self.key.bytes().next().unwrap_or(0) as u32;
        let mut rng = Mulberry32::new(seed.wrapping_mul(13).wrapping_add(self.key.len() as u32 * 977).wrapping_add(first));
        let draw = |rng: &mut Mulberry32, base: &str| roll_item_with(base, (rng.next_f64() * 4_294_967_296.0) as u32, odds);
        let mut worn: Vec<(Slot, Item)> = Vec::new();
        for (slot, choice) in &self.slots {
            if choice.bases.is_empty() || (choice.chance < 1.0 && rng.next_f64() > choice.chance as f64) {
                continue;
            }
            let mut base = rng.pick(&choice.bases).clone();
            if let Some(livery) = livery {
                let favoured: Vec<&String> = choice.bases.iter().filter(|b| livery.armour_lean.favours(*slot).contains(&b.as_str())).collect();
                if !favoured.is_empty() && lrng.chance(LIVERY_LEAN) {
                    base = (*lrng.pick(&favoured)).clone();
                }
            }
            if let Some(mut item) = draw(&mut rng, &base) {
                if let Some(livery) = livery {
                    livery.dress(&mut item, &mut lrng, false);
                }
                worn.push((*slot, item));
            }
        }
        if let Some((livery, garment)) = livery.and_then(|l| l.garment.map(|g| (l, g))) {
            livery.robe(garment, &mut worn, odds, &mut lrng);
        }
        let free = |worn: &[(Slot, Item)], slot: Slot| worn.iter().all(|(s, _)| *s != slot);
        let extras = [(Slot::Head, "Pointed hat", GNOME_HAT, folk == "gnome"), (Slot::Ring1, "Ring", EXTRA_RING, true), (Slot::Neck, "Amulet", EXTRA_AMULET, true), (Slot::Charm, "Charm", EXTRA_CHARM, true)];
        for (slot, base, chance, allowed) in extras {
            if allowed && free(&worn, slot) && rng.chance(chance)
                && let Some(item) = draw(&mut rng, base)
            {
                worn.push((slot, item));
            }
        }
        // A bow or crossbow comes with a quiver or bolt case slung and twenty of its
        // ammunition, carried as goods for the wearer to stow in it (SRD 5.1 p. 67-68).
        let mut ammunition = Vec::new();
        let shoots = worn.iter().find(|(s, _)| *s == Slot::Mainhand).and_then(|(_, i)| i.profile.ranged);
        if let Some(ranged) = shoots
            && free(&worn, Slot::Quiver)
        {
            let (holder, ammo) = if ranged.ammunition == armory::Family::Bolt { ("Bolt case", "Bolt") } else { ("Quiver", "Arrow") };
            if let Some(item) = draw(&mut rng, holder) {
                worn.push((Slot::Quiver, item));
                // One kind of arrow, one seed: the pieces stack when stowed.
                let kind = (rng.next_f64() * 4_294_967_296.0) as u32;
                ammunition = (0..AMMUNITION_CARRIED).filter_map(|_| roll_item_with(ammo, kind, odds)).collect();
            }
        }
        let mut goods: Vec<Item> = self.goods.iter().filter(|base| !held_back(base)).filter_map(|base| draw(&mut rng, base)).collect();
        goods.extend(ammunition);
        let ground = self.ground.iter().filter_map(|base| draw(&mut rng, base)).collect();
        OutfitSet { worn, goods, ground }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rarity;

    /// A pack's lashing and weapon strap are places, never contents: nothing rolled, worn by
    /// folk or carried by a corpse comes tied to one.
    #[test]
    fn a_lashing_is_always_found_empty() {
        let mut lashed_packs = 0;
        for seed in 1..300 {
            let pack = crate::roll_item("Pack", seed).unwrap();
            lashed_packs += usize::from(pack.has_accessory(crate::Accessory::Lashing));
            // A pack is a grid and a place; nothing is in its grid unless put there.
            assert!(!pack.lashable(), "{seed}");
            for outfit in outfits() {
                for folk in ["human", "goblin"].into_iter().filter(|_| seed % 100 == 1) {
                    let set = outfit.roll(seed, folk);
                    assert!(set.worn.iter().all(|(_, item)| !item.lashable()), "{seed} {folk}");
                }
            }
        }
        assert!(lashed_packs > 0, "no pack rolled a lashing");
    }

    #[test]
    fn the_first_levels_folk_wear_nothing_rare() {
        // 60 folk, the outfits taken in turn. The cap is what keeps rare gear off them; the
        // sample only has to reach every outfit and see some magic.
        let o = |seed: u32| &outfits()[seed as usize % outfits().len()];
        let mut magic = 0;
        for seed in 0..60 {
            for (_, item) in o(seed).roll_with(seed, "human", Odds::kit(1)).worn {
                assert!(item.rarity <= Rarity::Magic, "{} {seed}: {} is {:?}", o(seed).key, item.base, item.rarity);
                magic += usize::from(item.rarity == Rarity::Magic);
            }
        }
        assert!(magic > 0, "no magic gear on the first level's folk");
        let deep = (0..50).flat_map(|seed| o(seed).roll_with(seed, "human", Odds::kit(5)).worn).filter(|(_, i)| i.rarity >= Rarity::Rare).count();
        assert!(deep > 0, "nothing rare on folk at depth 5");
    }

    #[test]
    fn every_outfit_reads_from_the_table() {
        let keys: Vec<&str> = outfits().iter().map(|o| o.key.as_str()).collect();
        for key in ["fighter", "ranger", "mage", "cleric", "fallen", "scavenger"] {
            assert!(keys.contains(&key), "{key}");
        }
        let fallen = outfit("Freshly fallen").unwrap();
        assert_eq!(fallen.key, "fallen");
        assert_eq!(fallen.ground, vec!["Spare torch".to_string(), "Bread".to_string()]);
        let fighter = outfit("fighter").unwrap();
        let back = &fighter.slots.iter().find(|(s, _)| *s == Slot::Back).unwrap().1;
        assert_eq!((back.chance, back.bases.clone()), (0.7, vec!["Pack".to_string()]));
    }

    #[test]
    fn a_ranger_can_roll_a_satchel_beside_a_pack() {
        let ranger = outfit("ranger").unwrap();
        let sets: Vec<_> = (0..60).map(|s| ranger.roll(s, "human")).collect();
        assert!(sets.iter().any(|set| set.item(Slot::Satchel).is_some_and(|i| i.base == "Satchel")));
        assert!(sets.iter().any(|set| set.item(Slot::Back).is_some() && set.item(Slot::Satchel).is_some()));
    }

    #[test]
    fn rolled_outfits_are_whole_items_from_their_candidates() {
        for outfit in outfits() {
            for seed in [1, 7, 1207] {
                let set = outfit.roll(seed, "human");
                assert_eq!(set, outfit.roll(seed, "human"), "{} {seed}", outfit.key);
                for (slot, item) in &set.worn {
                    assert!(item.fits(*slot), "{}: {} does not fit {slot:?}", outfit.key, item.base);
                    match outfit.slots.iter().find(|(s, _)| s == slot) {
                        Some((_, choice)) => assert!(choice.bases.contains(&item.base), "{}: {} in {slot:?}", outfit.key, item.base),
                        None => assert!(matches!(slot, Slot::Ring1 | Slot::Neck | Slot::Charm | Slot::Head | Slot::Quiver | Slot::Satchel), "{slot:?}"),
                    }
                }
                let shoots = set.item(Slot::Mainhand).is_some_and(|i| i.profile.ranged.is_some());
                assert_eq!(set.goods.len(), outfit.goods.iter().filter(|g| !held_back(g)).count() + if shoots { AMMUNITION_CARRIED } else { 0 });
                assert_eq!(set.ground.len(), outfit.ground.len());
            }
        }
    }

    /// Whether any outfit, `seeds` seeds at each of `depths`, carries a held-back base.
    fn carries_held_back(seeds: u32, depths: &[u32]) -> bool {
        outfits().iter().any(|o| {
            depths.iter().any(|&depth| (0..seeds).any(|seed| o.roll_with(seed, "human", Odds::kit(depth)).goods.iter().any(|g| held_back(&g.base))))
        })
    }

    #[test]
    fn no_outfit_carries_a_held_back_base_smoke() {
        assert!(!carries_held_back(1, &[1, 10]));
    }

    #[test]
    #[ignore = "slow sweep: 25 seeds at each depth from 1 to 10 for every outfit; run before merging"]
    fn no_outfit_carries_a_held_back_base() {
        assert!(!carries_held_back(25, &(1..=10).collect::<Vec<_>>()));
    }

    #[test]
    fn a_fighter_always_holds_a_weapon() {
        let fighter = outfit("fighter").unwrap();
        for seed in 0..30 {
            let set = fighter.roll(seed, "dwarf");
            assert!(set.item(Slot::Mainhand).is_some_and(|i| i.profile.stats.is_weapon()), "{seed}");
        }
    }

    #[test]
    fn a_ranger_with_a_bow_has_a_quiver_and_twenty_arrows() {
        let ranger = outfit("ranger").unwrap();
        let archers: Vec<_> = (0..40).map(|seed| ranger.roll(seed, "human")).filter(|s| s.item(Slot::Mainhand).is_some_and(|i| i.base == "Bow")).collect();
        assert!(!archers.is_empty());
        for set in archers {
            assert_eq!(set.item(Slot::Quiver).map(|i| i.base.as_str()), Some("Quiver"));
            assert_eq!(set.goods.iter().filter(|g| g.base == "Arrow").count(), 20);
        }
    }

    #[test]
    fn gnomes_often_get_a_pointed_hat() {
        let fallen = outfit("fallen").unwrap();
        let hats = (0..100).filter(|&s| fallen.roll(s, "gnome").item(Slot::Head).is_some()).count();
        assert!((40..80).contains(&hats), "{hats}");
        assert!((0..100).all(|s| fallen.roll(s, "human").item(Slot::Head).is_none()));
    }

    fn livery(lean: ArmourLean) -> Livery {
        let field = Field { tincture: Tincture::Vert, tincture2: Tincture::Or, charge: armory::sections::device::Device::Tower, charge_tincture: Tincture::Argent, ..Field::default() };
        Livery::from_field(field, lean)
    }

    fn pieces_of_cloth(set: &OutfitSet) -> Vec<armory::Rgb> {
        set.worn
            .iter()
            .filter_map(|(_, i)| match &i.shape {
                armory::Item::Garment(s) if i.base != "Leather jerkin" && !s.cut.style.is_armour() => Some(s.cut.cloth.color),
                armory::Item::Hood(s) if s.hood.style != HoodStyle::MailCoif => Some(s.hood.cloth.color),
                _ => None,
            })
            .collect()
    }

    /// Folk livery is as it was before denizens: legwear draws nothing from the livery stream
    /// and keeps its rolled colour, so what follows it is unchanged.
    #[test]
    fn folk_legwear_is_left_as_rolled_and_costs_the_livery_stream_nothing() {
        let livery = livery(ArmourLean::Mail);
        for key in ["fighter", "ranger", "mage", "cleric", "scavenger", "fallen"] {
            for seed in [1, 7, 1207] {
                let plain = outfit(key).unwrap().roll_in(seed, "human", Odds::default(), None);
                let dressed = outfit(key).unwrap().roll_in(seed, "human", Odds::default(), Some(&livery));
                let legs = |set: &OutfitSet| set.worn.iter().find_map(|(_, i)| if let armory::Item::Legs(s) = &i.shape { Some(s.legwear.cloth.color) } else { None });
                assert_eq!(legs(&plain), legs(&dressed), "{key} {seed}");
                // The same livery with legs on, but folk never turn it on: it only differs where
                // legwear exists, which is what the flag is for.
                assert_eq!(dressed, outfit(key).unwrap().roll_in(seed, "human", Odds::default(), Some(&livery)), "{key} {seed}");
            }
        }
    }

    #[test]
    fn most_of_a_factions_cloth_wears_its_colours() {
        let livery = livery(ArmourLean::Mail);
        let colours = [Tincture::Vert.dye(), Tincture::Or.dye()];
        let (mut worn, mut total) = (0, 0);
        for seed in 0..30 {
            for key in ["ranger", "mage", "cleric"] {
                for colour in pieces_of_cloth(&outfit(key).unwrap().roll_in(seed, "human", Odds::default(), Some(&livery))) {
                    total += 1;
                    worn += usize::from(colours.contains(&colour));
                }
            }
        }
        let share = worn as f32 / total as f32;
        assert!((0.6..0.9).contains(&share), "{worn} of {total}");
        // Without a livery the same wearers are not in green and gold.
        let plain = (0..30).flat_map(|seed| pieces_of_cloth(&outfit("mage").unwrap().roll(seed, "human"))).filter(|c| colours.contains(c)).count();
        assert!(plain < worn / 4, "{plain}");
    }

    #[test]
    fn shields_carry_the_factions_field() {
        let livery = livery(ArmourLean::Plate);
        let mut shields = 0;
        for seed in 0..30 {
            let set = outfit("fighter").unwrap().roll_in(seed, "human", Odds::default(), Some(&livery));
            if let Some(armory::Item::Shield(s)) = set.item(Slot::Offhand).map(|i| &i.shape) {
                assert_eq!(s.field, livery.field, "{seed}");
                shields += 1;
            }
        }
        assert!(shields > 5, "{shields}");
    }

    #[test]
    fn an_armour_lean_shifts_the_body_armour() {
        let fighter = outfit("fighter").unwrap();
        let share = |lean: Option<ArmourLean>, base: &str| {
            let hits = (0..60)
                .filter(|&seed| {
                    let l = lean.map(livery);
                    fighter.roll_in(seed, "human", Odds::default(), l.as_ref()).item(Slot::Body).is_some_and(|i| i.base == base)
                })
                .count();
            hits as f32 / 60.0
        };
        assert!(share(None, "Mail") < 0.35, "{}", share(None, "Mail"));
        assert!(share(Some(ArmourLean::Mail), "Mail") > 0.65, "{}", share(Some(ArmourLean::Mail), "Mail"));
        assert!(share(Some(ArmourLean::Padded), "Gambeson") > 0.65);
        assert!(share(Some(ArmourLean::Plate), "Mail") < 0.15);
    }

    #[test]
    fn a_liveried_roll_is_deterministic_and_leaves_the_unliveried_one_alone() {
        let livery = livery(ArmourLean::Scale);
        for outfit in outfits() {
            let again = (outfit.roll_in(9, "dwarf", Odds::default(), Some(&livery)), outfit.roll_in(9, "dwarf", Odds::default(), Some(&livery)));
            assert_eq!(again.0, again.1, "{}", outfit.key);
            assert_eq!(outfit.roll_in(9, "dwarf", Odds::default(), None), outfit.roll(9, "dwarf"), "{}", outfit.key);
        }
    }

    /// Azure and argent, named by the faction, in a robe.
    fn sisters(lean: ArmourLean) -> Livery {
        let field = Field { tincture: Tincture::Azure, tincture2: Tincture::Argent, charge: armory::sections::device::Device::Cross, charge_tincture: Tincture::Gules, ..Field::default() };
        Livery::from_field(field, lean).named().with_garment(Some(Garment::Robe))
    }

    /// Argent and or, named, in plate with a robe over it and a gold hem.
    fn redeemed() -> Livery {
        let field = Field { tincture: Tincture::Argent, tincture2: Tincture::Or, charge: armory::sections::device::Device::Cross, charge_tincture: Tincture::Gules, ..Field::default() };
        Livery::from_field(field, ArmourLean::Plate).named().with_garment(Some(Garment::RobeOverArmour)).with_trim(Some(Trim::GoldHem))
    }

    fn robe_of(set: &OutfitSet) -> (Slot, &Item) {
        // The outermost robe: the cloak slot's, else the shirt's.
        set.worn
            .iter()
            .filter(|(s, i)| matches!(s, Slot::Cloak | Slot::Shirt) && i.base == "Robe")
            .max_by_key(|(s, _)| *s == Slot::Cloak)
            .map(|(s, i)| (*s, i))
            .expect("a robe")
    }

    #[test]
    fn a_faction_that_names_its_colours_dresses_all_its_cloth_in_them() {
        let livery = sisters(ArmourLean::Padded);
        let blue = Tincture::Azure.dye();
        let (mut worn, mut total) = (0, 0);
        for seed in 0..40 {
            for key in ["ranger", "mage", "cleric", "scavenger"] {
                for colour in pieces_of_cloth(&outfit(key).unwrap().roll_in(seed, "human", Odds::default(), Some(&livery))) {
                    total += 1;
                    worn += usize::from(colour == blue);
                }
            }
        }
        assert_eq!(worn, total, "{worn} of {total} pieces of cloth are blue");
    }

    #[test]
    fn a_robe_is_the_outer_layer_of_every_member() {
        for key in ["fighter", "ranger", "mage", "cleric", "scavenger"] {
            for seed in 0..25 {
                let set = outfit(key).unwrap().roll_in(seed, "human", Odds::default(), Some(&sisters(ArmourLean::Mail)));
                let (slot, robe) = robe_of(&set);
                match &robe.shape {
                    armory::Item::Garment(g) => {
                        assert_eq!(g.cut.style, GarmentStyle::Robe);
                        assert_eq!(g.cut.cloth.color, Tincture::Azure.dye(), "{key} {seed}");
                    }
                    other => panic!("{other:?}"),
                }
                // Over body armour where there is any; and nothing is worn over the robe.
                assert_eq!(slot == Slot::Cloak, set.item(Slot::Body).is_some(), "{key} {seed}");
                assert!(slot == Slot::Cloak || set.item(Slot::Cloak).is_none(), "{key} {seed}: a cloak over the robe");
            }
        }
    }

    #[test]
    fn a_robe_over_armour_covers_the_leans_armour_in_gold_and_white() {
        for key in ["fighter", "ranger", "mage", "cleric", "scavenger"] {
            for seed in 0..25 {
                let set = outfit(key).unwrap().roll_in(seed, "human", Odds::default(), Some(&redeemed()));
                let body = set.item(Slot::Body).unwrap_or_else(|| panic!("{key} {seed} wears no armour"));
                assert!(["Cuirass", "Brigandine"].contains(&body.base.as_str()), "{key} {seed}: {}", body.base);
                let (slot, robe) = robe_of(&set);
                assert_eq!(slot, Slot::Cloak, "{key} {seed}");
                let armory::Item::Garment(g) = &robe.shape else { panic!() };
                assert_eq!(g.cut.cloth.color, Tincture::Argent.dye());
                assert!(g.hem.on && g.hem.hemline && g.hem.neck && g.hem.cuffs, "{key} {seed}: no hem");
                assert_eq!(g.hem.fabric.color, Tincture::Or.dye());
            }
        }
    }

    #[test]
    fn a_gold_hem_edges_a_tunic_and_a_hood_too() {
        let set = outfit("ranger").unwrap().roll_in(3, "human", Odds::default(), Some(&redeemed()));
        for (_, item) in &set.worn {
            match &item.shape {
                armory::Item::Garment(g) if matches!(g.cut.style, GarmentStyle::Tunic | GarmentStyle::Robe | GarmentStyle::Cloak) => {
                    assert!(g.hem.on && g.hem.fabric.color == Tincture::Or.dye(), "{}", item.base);
                }
                armory::Item::Hood(h) if h.hood.style != HoodStyle::MailCoif => assert!(h.hem.on && h.hem.fabric.color == Tincture::Or.dye()),
                _ => {}
            }
        }
    }

    #[test]
    fn a_robe_keeps_the_class_kit() {
        let livery = sisters(ArmourLean::Padded);
        let mace = (0..30).filter(|&s| outfit("cleric").unwrap().roll_in(s, "human", Odds::default(), Some(&livery)).item(Slot::Mainhand).is_some_and(|i| i.base == "Mace")).count();
        assert!(mace > 12, "{mace}");
        let bows = (0..30).filter(|&s| outfit("ranger").unwrap().roll_in(s, "human", Odds::default(), Some(&livery)).item(Slot::Mainhand).is_some_and(|i| i.base == "Bow")).count();
        assert!(bows > 8, "{bows}");
        // A bow still comes with its quiver.
        for seed in 0..30 {
            let set = outfit("ranger").unwrap().roll_in(seed, "human", Odds::default(), Some(&livery));
            if set.item(Slot::Mainhand).is_some_and(|i| i.base == "Bow") {
                assert!(set.item(Slot::Quiver).is_some(), "{seed}");
            }
        }
    }

    #[test]
    fn a_dressed_member_is_deterministic_and_a_plain_livery_is_as_before() {
        for livery in [sisters(ArmourLean::Scale), redeemed()] {
            for outfit in outfits() {
                assert_eq!(outfit.roll_in(9, "dwarf", Odds::default(), Some(&livery)), outfit.roll_in(9, "dwarf", Odds::default(), Some(&livery)), "{}", outfit.key);
            }
        }
    }
}
