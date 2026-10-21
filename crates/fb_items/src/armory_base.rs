//! Which Armory family makes each base, and which of its styles count (the plate's `SAB`).
//!
//! This replaces `data/sab.json`. The extractor serialised the plate's `acc` filters as
//! `s=>arr.includes(s[k][f])`, losing which field and styles they named, and its `tw` tweaks
//! set fields (`food.color2`, `bag.coins`) that the typed Armory states have since renamed or
//! moved. Here each filter is a list of the family's own style enum and each tweak is a
//! function on the typed state, so a renamed field or style fails to compile instead of
//! silently matching nothing.
//!
//! Where the plate rolled and retried (`SA.roll(fam, seed, acc)`, up to 120 seeds), a base
//! here picks one of its styles and rolls it directly with the family's `roll_as`, so every
//! roll lands on the first try and styles the Armory does not store (club and axe
//! archetypes) can still be chosen. See `docs/items.md`.

use armory::families::axe::{Axe, AxeArchetype};
use armory::families::bags::{BagStyle, Bags};
use armory::families::feet::Feet;
use armory::families::food::{Brew, Food, FoodStyle, GLOWCAP};
use armory::families::garment::Garment;
use armory::families::hands::Hands;
use armory::families::hat::Hats;
use armory::families::hood::Hoods;
use armory::families::legs::Legs;
use armory::families::sundry::{Sundry, SundryKind};
use armory::families::torch::BurnState;
use armory::families::writing::{Writing, WritingKind};
use armory::fabric::FabricPattern;
use armory::sections::boot::BootStyle;
use armory::sections::garment::{GarmentStyle, Sleeves};
use armory::sections::hand::HandStyle;
use armory::sections::hat::HatStyle;
use armory::sections::hood::HoodStyle;
use armory::sections::legwear::LegwearStyle;
use armory::{Family, Item, Rgb};
use procgen_rng::Mulberry32;

/// The styles a base accepts from its family. `Any` takes whatever "Forge new" rolls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Accept {
    Any(Family),
    Garment(&'static [GarmentStyle]),
    Hands(&'static [HandStyle]),
    Feet(&'static [BootStyle]),
    Legs(&'static [LegwearStyle]),
    Hood(&'static [HoodStyle]),
    Hat(&'static [HatStyle]),
    Bags(&'static [BagStyle]),
    Food(&'static [FoodStyle]),
    Sundry(&'static [SundryKind]),
    Writing(&'static [WritingKind]),
    /// Axe archetypes are not stored in the state (the noun reads the head itself), so
    /// [`Accept::accepts`] cannot check them; [`Accept::roll`] only rolls these.
    Axe(&'static [AxeArchetype]),
}

impl Accept {
    pub fn family(self) -> Family {
        match self {
            Accept::Any(f) => f,
            Accept::Garment(_) => Family::Garment,
            Accept::Hands(_) => Family::Hands,
            Accept::Feet(_) => Family::Feet,
            Accept::Legs(_) => Family::Legs,
            Accept::Hood(_) => Family::Hood,
            Accept::Hat(_) => Family::Hat,
            Accept::Bags(_) => Family::Bags,
            Accept::Food(_) => Family::Food,
            Accept::Sundry(_) => Family::Sundry,
            Accept::Writing(_) => Family::Writing,
            Accept::Axe(_) => Family::Axe,
        }
    }

    /// Rolls one of the accepted styles, picked uniformly with `rng`, from `seed`.
    pub fn roll(self, seed: u32, rng: &mut Mulberry32) -> Item {
        match self {
            Accept::Any(f) => Item::roll(f, seed).unwrap_or_else(|| panic!("{f:?} is not ported")),
            Accept::Garment(s) => Item::Garment(Garment::roll_as(seed, *rng.pick(s))),
            Accept::Hands(s) => Item::Hands(Hands::roll_as(seed, *rng.pick(s))),
            Accept::Feet(s) => Item::Feet(Feet::roll_as(seed, *rng.pick(s))),
            Accept::Legs(s) => Item::Legs(Legs::roll_as(seed, *rng.pick(s))),
            Accept::Hood(s) => Item::Hood(Hoods::roll_as(seed, *rng.pick(s))),
            Accept::Hat(s) => Item::Hat(Hats::roll_as(seed, *rng.pick(s))),
            Accept::Bags(s) => Item::Bags(Bags::roll_as(seed, *rng.pick(s))),
            Accept::Food(s) => Item::Food(Food::roll_as(seed, *rng.pick(s))),
            Accept::Sundry(s) => Item::Sundry(Sundry::roll_as(seed, *rng.pick(s))),
            Accept::Writing(s) => Item::Writing(Writing::roll_as(seed, *rng.pick(s))),
            Accept::Axe(s) => Item::Axe(Axe::roll_as(seed, *rng.pick(s))),
        }
    }

    /// Whether `item` is of the family and one of the styles (the plate's `acc(state)`).
    pub fn accepts(self, item: &Item) -> bool {
        match (self, item) {
            (Accept::Any(f), item) => item.family() == f,
            (Accept::Garment(s), Item::Garment(st)) => s.contains(&st.cut.style),
            (Accept::Hands(s), Item::Hands(st)) => s.contains(&st.hand.style),
            (Accept::Feet(s), Item::Feet(st)) => s.contains(&st.boot.style),
            (Accept::Legs(s), Item::Legs(st)) => s.contains(&st.legwear.style),
            (Accept::Hood(s), Item::Hood(st)) => s.contains(&st.hood.style),
            (Accept::Hat(s), Item::Hat(st)) => s.contains(&st.hat.style),
            (Accept::Bags(s), Item::Bags(st)) => s.contains(&st.bag.style),
            (Accept::Food(s), Item::Food(st)) => s.contains(&st.provision.style),
            (Accept::Sundry(s), Item::Sundry(st)) => s.contains(&st.article.kind),
            (Accept::Writing(s), Item::Writing(st)) => s.contains(&st.piece.kind),
            (Accept::Axe(_), Item::Axe(_)) => true,
            _ => false,
        }
    }
}

/// Changes a rolled state to fit its base (the plate's `tw`).
pub type Tweak = fn(&mut Item, &mut Mulberry32);

/// How one base is made by the Armory.
#[derive(Clone, Copy, Debug)]
pub struct ArmoryBase {
    pub accept: Accept,
    pub tweak: Option<Tweak>,
    /// Named for the base rather than the Armory's noun: the plate's `Blind cavefish` and
    /// `Healing draught`, and bases the Armory makes from a near style (a Shirt is a short
    /// tunic, but should not be called "Tunic").
    pub named_by_base: bool,
}

impl ArmoryBase {
    const fn of(accept: Accept) -> Self {
        ArmoryBase { accept, tweak: None, named_by_base: false }
    }

    const fn tweaked(self, tweak: Tweak) -> Self {
        ArmoryBase { tweak: Some(tweak), ..self }
    }

    const fn named(self) -> Self {
        ArmoryBase { named_by_base: true, ..self }
    }

    /// The base's shape from `seed`: an accepted style, then the tweak. Pure in `seed`.
    pub fn roll(&self, seed: u32) -> Item {
        let mut rng = Mulberry32::new(seed.wrapping_mul(0x9E37_79B9) ^ 0x5AB0_5AB0);
        let mut item = self.accept.roll(seed, &mut rng);
        if let Some(tweak) = self.tweak {
            tweak(&mut item, &mut rng);
        }
        item
    }
}

/// The Armory base named `name`, from bases, goods or the plate's own Armory list.
pub fn armory_base(name: &str) -> Option<&'static ArmoryBase> {
    ARMORY_BASES.iter().find(|(n, _)| *n == name).map(|(_, b)| b)
}

/// Every base the Armory makes from `family`, in table order.
pub fn bases_of(family: Family) -> impl Iterator<Item = &'static str> {
    ARMORY_BASES.iter().filter(move |(_, b)| b.accept.family() == family).map(|(n, _)| *n)
}

/// A base of `family` chosen from `seed` by [`find_weight`], for loot that names only a family
/// (the level's item spots). Its own stream, so `roll_item(base, seed)` with the same seed is
/// unaffected; the mixing constant is this port's own. `None` for a family no base uses.
pub fn base_for(family: Family, seed: u32) -> Option<&'static str> {
    let bases: Vec<(&str, u32)> = bases_of(family).map(|b| (b, find_weight(b))).filter(|&(_, w)| w > 0).collect();
    (!bases.is_empty()).then(|| *Mulberry32::new(seed ^ 0xBA5E_F00D).weighted(&bases))
}

/// How often base `name` is found among its family's bases, against 1 for most. A book teaches
/// its reader for good ([`crate::Item::lesson`]), so it is found about once for every twenty
/// scrolls and notes (our own). With every weight 1 the pick is the uniform one it always was.
pub fn find_weight(name: &str) -> u32 {
    if held_back(name) {
        return 0;
    }
    match name {
        "Scroll" | "Note" => 10,
        _ => 1,
    }
}

/// Bases nothing in the game uses yet, so none is found loose, in a crate or in a kit (our own).
/// Like the stats nothing reads ([`crate::stats::Stat::in_play`]), they wait for their system:
/// bandages for bandaging. They stay in [`ARMORY_BASES`] and `data/goods.json`, so the forge and
/// [`roll_item`](crate::roll_item) still make them; when the system arrives the base comes off
/// this list.
pub const HELD_BACK: &[&str] = &[
    "Bandages",
    // Camp goods and elixirs come from tentmakers and alchemists, not loose loot.
    "Bedroll",
    "Tinderbox",
    "Elixir of the Hare",
    "Elixir of Vigil",
    "Elixir of Fasting",
    "Elixir of Embers",
];

/// Whether base `name` is held back ([`HELD_BACK`]): [`find_weight`] gives it 0, and crate
/// stock and outfit goods skip it.
pub fn held_back(name: &str) -> bool {
    HELD_BACK.contains(&name)
}

/// The shape of base `name` from `seed`: one of its accepted styles, rolled with the
/// family's `roll_as`, then its tweak (the plate's `SA.roll(fam, seed, acc)` and `tw`).
/// `None` for a name no table knows.
pub fn roll_shape(name: &str, seed: u32) -> Option<Item> {
    armory_base(name).map(|base| base.roll(seed))
}

use Accept as A;

/// Every base the Armory makes, in the plate's `SAB` order with the newer families after.
pub static ARMORY_BASES: &[(&str, ArmoryBase)] = &[
    ("Dagger", ArmoryBase::of(A::Any(Family::Dagger))),
    ("Sword", ArmoryBase::of(A::Any(Family::Sword))),
    ("Mace", ArmoryBase::of(A::Any(Family::Mace))),
    ("Staff", ArmoryBase::of(A::Any(Family::Staff))),
    ("Spear", ArmoryBase::of(A::Any(Family::Spear))),
    ("Bow", ArmoryBase::of(A::Any(Family::Bow))),
    ("Crossbow", ArmoryBase::of(A::Any(Family::Crossbow))),
    ("Shield", ArmoryBase::of(A::Any(Family::Shield))),
    ("Helm", ArmoryBase::of(A::Any(Family::Helm))),
    ("Tunic", ArmoryBase::of(A::Garment(&[GarmentStyle::Tunic]))),
    ("Robe", ArmoryBase::of(A::Garment(&[GarmentStyle::Robe]))),
    ("Gambeson", ArmoryBase::of(A::Garment(&[GarmentStyle::Gambeson]))),
    ("Mail", ArmoryBase::of(A::Garment(&[GarmentStyle::Hauberk]))),
    ("Brigandine", ArmoryBase::of(A::Garment(&[GarmentStyle::Brigandine]))),
    ("Scale coat", ArmoryBase::of(A::Garment(&[GarmentStyle::Scale]))),
    ("Cuirass", ArmoryBase::of(A::Garment(&[GarmentStyle::Cuirass]))),
    ("Cloak", ArmoryBase::of(A::Garment(&[GarmentStyle::Cloak]))),
    ("Mantle", ArmoryBase::of(A::Garment(&[GarmentStyle::Cloak])).tweaked(mantle)),
    ("Gloves", ArmoryBase::of(A::Hands(&[HandStyle::Glove, HandStyle::Mitten, HandStyle::Bracer]))),
    ("Gauntlets", ArmoryBase::of(A::Hands(&[HandStyle::Gauntlet, HandStyle::Hourglass]))),
    ("Shoes", ArmoryBase::of(A::Feet(&[BootStyle::Shoe, BootStyle::Poulaine, BootStyle::Wrapped]))),
    ("Boots", ArmoryBase::of(A::Feet(&[BootStyle::Boot, BootStyle::Sabaton]))),
    ("Pack", ArmoryBase::of(A::Bags(&[BagStyle::Backpack]))),
    ("Satchel", ArmoryBase::of(A::Bags(&[BagStyle::Satchel]))),
    ("Sack", ArmoryBase::of(A::Bags(&[BagStyle::Sack]))),
    ("Purse", ArmoryBase::of(A::Bags(&[BagStyle::Purse, BagStyle::Pouch])).tweaked(purse)),
    ("Ring", ArmoryBase::of(A::Any(Family::Ring))),
    ("Amulet", ArmoryBase::of(A::Any(Family::Amulet))),
    ("Charm", ArmoryBase::of(A::Any(Family::Charm))),
    ("Chest", ArmoryBase::of(A::Any(Family::Chests))),
    ("Bread", ArmoryBase::of(A::Food(&[FoodStyle::Loaf, FoodStyle::Boule]))),
    ("Cheese", ArmoryBase::of(A::Food(&[FoodStyle::Cheese]))),
    ("Smoked fish", ArmoryBase::of(A::Food(&[FoodStyle::Fish]))),
    ("Apple", ArmoryBase::of(A::Food(&[FoodStyle::Apple, FoodStyle::Pear]))),
    ("Meat pie", ArmoryBase::of(A::Food(&[FoodStyle::Pie]))),
    ("Sausages", ArmoryBase::of(A::Food(&[FoodStyle::Sausages]))),
    ("Roast haunch", ArmoryBase::of(A::Food(&[FoodStyle::Haunch]))),
    ("Tankard", ArmoryBase::of(A::Food(&[FoodStyle::Tankard]))),
    ("Wine", ArmoryBase::of(A::Food(&[FoodStyle::Bottle]))),
    ("Potion", ArmoryBase::of(A::Food(&[FoodStyle::Potion]))),
    ("Healing draught", ArmoryBase::of(A::Food(&[FoodStyle::Potion])).tweaked(healing_draught).named()),
    ("Elixir of the Hare", ArmoryBase::of(A::Food(&[FoodStyle::Potion])).tweaked(elixir_of_the_hare).named()),
    ("Elixir of Vigil", ArmoryBase::of(A::Food(&[FoodStyle::Potion])).tweaked(elixir_of_vigil).named()),
    ("Elixir of Fasting", ArmoryBase::of(A::Food(&[FoodStyle::Potion])).tweaked(elixir_of_fasting).named()),
    ("Elixir of Embers", ArmoryBase::of(A::Food(&[FoodStyle::Potion])).tweaked(elixir_of_embers).named()),
    ("Blind cavefish", ArmoryBase::of(A::Food(&[FoodStyle::Fish])).tweaked(blind_cavefish).named()),
    // The plate's own bases (data/bases.json), now Armory families.
    ("Shirt", ArmoryBase::of(A::Garment(&[GarmentStyle::Tunic])).tweaked(shirt).named()),
    ("Trousers", ArmoryBase::of(A::Legs(&[LegwearStyle::Trousers]))),
    ("Breeches", ArmoryBase::of(A::Legs(&[LegwearStyle::Breeches]))),
    ("Leggings", ArmoryBase::of(A::Legs(&[LegwearStyle::Leggings]))),
    ("Leather jerkin", ArmoryBase::of(A::Garment(&[GarmentStyle::Brigandine])).tweaked(leather_jerkin).named()),
    ("Hood", ArmoryBase::of(A::Hood(&[HoodStyle::Hood, HoodStyle::Cowl]))),
    ("Mail coif", ArmoryBase::of(A::Hood(&[HoodStyle::MailCoif]))),
    ("Pointed hat", ArmoryBase::of(A::Hat(&[HatStyle::Pointed]))),
    ("Belt", ArmoryBase::of(A::Any(Family::Belt)).tweaked(belt)),
    ("Club", ArmoryBase::of(A::Any(Family::Club))),
    ("Hand axe", ArmoryBase::of(A::Axe(&[AxeArchetype::Hatchet, AxeArchetype::HandAxe, AxeArchetype::Bearded]))),
    ("Torch", ArmoryBase::of(A::Any(Family::Torch))),
    ("Lantern", ArmoryBase::of(A::Any(Family::Lantern))),
    // Goods (data/goods.json).
    ("Glowcap", ArmoryBase::of(A::Food(&[FoodStyle::Mushroom])).tweaked(glowcap)),
    ("Waterskin", ArmoryBase::of(A::Food(&[FoodStyle::Waterskin])).tweaked(filled)),
    ("Lamp oil", ArmoryBase::of(A::Food(&[FoodStyle::Flask])).tweaked(lamp_oil).named()),
    ("Rope", ArmoryBase::of(A::Sundry(&[SundryKind::Rope]))),
    ("Lockpicks", ArmoryBase::of(A::Sundry(&[SundryKind::Lockpicks]))),
    ("Candle", ArmoryBase::of(A::Sundry(&[SundryKind::Candle]))),
    ("Bandages", ArmoryBase::of(A::Sundry(&[SundryKind::Bandages]))),
    ("Bedroll", ArmoryBase::of(A::Sundry(&[SundryKind::Bedroll]))),
    ("Tinderbox", ArmoryBase::of(A::Sundry(&[SundryKind::Tinderbox]))),
    ("Scroll", ArmoryBase::of(A::Writing(&[WritingKind::Scroll]))),
    ("Note", ArmoryBase::of(A::Writing(&[WritingKind::Note]))),
    ("Book", ArmoryBase::of(A::Writing(&[WritingKind::Book]))),
    ("Spare torch", ArmoryBase::of(A::Any(Family::Torch)).tweaked(spare_torch)),
    // Ammunition and what holds it (SRD 5.1 p. 67-69).
    ("Arrow", ArmoryBase::of(A::Any(Family::Arrow))),
    ("Bolt", ArmoryBase::of(A::Any(Family::Bolt))),
    ("Quiver", ArmoryBase::of(A::Any(Family::Quiver))),
    ("Bolt case", ArmoryBase::of(A::Any(Family::BoltCase))),
];

// ── Tweaks ───────────────────────────────────────────────────────────────────────────────

/// A short cloak: the plate's `s.garment.length=.4+r()*1.2`.
fn mantle(item: &mut Item, r: &mut Mulberry32) {
    if let Item::Garment(s) = item {
        s.cut.length = r.range_f32(0.4, 1.6);
    }
}

/// A purse always holds coins: the plate's `s.bag.coins=true`.
fn purse(item: &mut Item, _: &mut Mulberry32) {
    if let Item::Bags(s) = item {
        s.bag.coins = true;
    }
}

/// Clear glass of the `Food` roll (`GLASS` in `armory::families::food`).
const GLASS: Rgb = Rgb::hex(0xdfe9ee);

/// A red healing brew: the plate's `s.food.color2='#c8323c'`, now the liquid, with the glass
/// tinted toward it as the Armory tints every potion.
fn healing_draught(item: &mut Item, r: &mut Mulberry32) {
    if let Item::Food(s) = item {
        let p = &mut s.provision;
        p.liquid = Brew::Healing.rgb();
        p.outer = mix(GLASS, p.liquid, 0.4);
        p.fill = p.fill.max(r.range_f32(0.6, 0.95));
    }
}

/// An elixir is a full flask of a coloured brew, glass tinted toward it. Our own colours.
fn elixir(item: &mut Item, r: &mut Mulberry32, liquid: u32) {
    if let Item::Food(s) = item {
        let p = &mut s.provision;
        p.liquid = Rgb::hex(liquid);
        p.outer = mix(GLASS, p.liquid, 0.4);
        p.fill = p.fill.max(r.range_f32(0.6, 0.95));
    }
}

fn elixir_of_the_hare(item: &mut Item, r: &mut Mulberry32) {
    elixir(item, r, 0xe27a9a);
}

fn elixir_of_vigil(item: &mut Item, r: &mut Mulberry32) {
    elixir(item, r, 0xb89be0);
}

fn elixir_of_fasting(item: &mut Item, r: &mut Mulberry32) {
    elixir(item, r, 0x9a5a2a);
}

fn elixir_of_embers(item: &mut Item, r: &mut Mulberry32) {
    elixir(item, r, 0xe0612a);
}

/// A pale fish: the plate's `color='#d8d2c4'` (now the skin, `outer`) and `color2='#ece6d8'`
/// (now the fins, `accent`).
fn blind_cavefish(item: &mut Item, _: &mut Mulberry32) {
    if let Item::Food(s) = item {
        s.provision.outer = Rgb::hex(0xd8d2c4);
        s.provision.accent = Rgb::hex(0xece6d8);
    }
}

/// A plain laced shirt worn under everything: a short, unbelted tunic with straight long
/// sleeves, seldom trimmed.
fn shirt(item: &mut Item, r: &mut Mulberry32) {
    if let Item::Garment(s) = item {
        s.cut.length = r.range_f32(1.4, 2.0);
        s.cut.sleeves = Sleeves::Long;
        s.cut.sleeve_flare = r.range_f32(0.0, 0.12);
        s.belt.on = false;
        s.hem.on = r.chance(0.2);
    }
}

/// The plate's leather colours (`data/leather.json`).
const LEATHER: [u32; 5] = [0x5a3a26, 0x4a3226, 0x6b4630, 0x3a2a22, 0x7a5236];

/// A short, sleeveless leather coat, riveted for the plate's "Studded" one time in three.
fn leather_jerkin(item: &mut Item, r: &mut Mulberry32) {
    if let Item::Garment(s) = item {
        s.cut.cloth.color = Rgb::hex(*r.pick(&LEATHER));
        s.cut.cloth.pattern = if r.chance(1.0 / 3.0) { FabricPattern::Rivets } else { FabricPattern::Plain };
    }
}

/// The plate's belt is a container: it always carries its belt pouch.
fn belt(item: &mut Item, _: &mut Mulberry32) {
    if let Item::Belt(s) = item {
        s.fittings.pouch = true;
    }
}

/// A glowcap glows: a cave mushroom that rolled dull gets a glowing cap.
fn glowcap(item: &mut Item, r: &mut Mulberry32) {
    if let Item::Food(s) = item {
        let p = &mut s.provision;
        if p.glow < GLOWCAP {
            // The glowing cap colours of the food roll.
            p.outer = Rgb::hex(*r.pick(&[0x6ad0c8, 0x9ad06a, 0xc8a0e0]));
            p.glow = r.range_f32(0.4, 0.9);
        }
    }
}

/// Goods sold full: "Water · two days".
fn filled(item: &mut Item, r: &mut Mulberry32) {
    if let Item::Food(s) = item {
        s.provision.fill = s.provision.fill.max(r.range_f32(0.6, 1.0));
    }
}

/// A flask of lamp oil, "six hours of lantern light". The oil's colour is this port's own.
fn lamp_oil(item: &mut Item, r: &mut Mulberry32) {
    filled(item, r);
    if let Item::Food(s) = item {
        s.provision.liquid = Rgb::hex(0xc9a24a);
    }
}

/// A spare torch has never been lit.
fn spare_torch(item: &mut Item, _: &mut Mulberry32) {
    if let Item::Torch(s) = item {
        s.burn.state = BurnState::Fresh;
        s.burn.char = 0.0;
        s.burn.fuel_left = armory::kindle::TORCH_SECONDS;
    }
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    Rgb(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::{base_groups, bases, goods};

    #[test]
    fn held_back_bases_are_never_found_loose_but_still_roll_by_name() {
        for base in HELD_BACK {
            assert!(held_back(base) && find_weight(base) == 0, "{base}");
            assert!(armory_base(base).is_some(), "{base} is in ARMORY_BASES");
            assert!(crate::roll_item(base, 1).is_some(), "{base} still rolls by name");
        }
        assert!(!held_back("Rope"));
        let found: Vec<_> = (0..2000).filter_map(|seed| base_for(Family::Sundry, seed)).collect();
        assert!(found.len() == 2000 && found.iter().all(|b| !held_back(b)), "a held-back base came up loose");
        assert!(found.contains(&"Rope"), "the rest of the sundries still come up");
    }

    fn every_named_base() -> Vec<&'static str> {
        let mut names: Vec<&str> = bases().keys().chain(goods().keys()).map(String::as_str).collect();
        names.extend(base_groups().iter().flat_map(|g| g.bases.iter().map(String::as_str)));
        names.sort();
        names.dedup();
        names
    }

    #[test]
    fn books_are_rare_among_writing_finds() {
        let books = (0..1000u32).filter(|&s| base_for(Family::Writing, s) == Some("Book")).count();
        assert!((1..100).contains(&books), "{books} books in 1000 writing finds");
        let scrolls = (0..1000u32).filter(|&s| base_for(Family::Writing, s) == Some("Scroll")).count();
        assert!(scrolls > 300, "{scrolls} scrolls");
    }

    #[test]
    fn every_base_maps_to_a_ported_family() {
        for name in every_named_base() {
            let base = armory_base(name).unwrap_or_else(|| panic!("{name} has no Armory base"));
            assert!(Item::PORTED.contains(&base.accept.family()), "{name}: {:?} is not ported", base.accept.family());
        }
    }

    #[test]
    fn every_table_entry_is_a_known_base() {
        let known = every_named_base();
        for (name, _) in ARMORY_BASES {
            assert!(known.contains(name), "{name} is in no table");
        }
        let mut names: Vec<_> = ARMORY_BASES.iter().map(|(n, _)| *n).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), ARMORY_BASES.len(), "a base is listed twice");
    }

    #[test]
    fn every_filter_accepts_what_it_rolls() {
        for (name, base) in ARMORY_BASES {
            for seed in [1, 2, 3, 1207, 99_991, 765_432] {
                let item = base.roll(seed);
                assert_eq!(item.family(), base.accept.family(), "{name} {seed}");
                assert!(base.accept.accepts(&item), "{name} {seed}: rolled a style it does not accept");
                assert_eq!(item, base.roll(seed), "{name} {seed}: not deterministic");
                assert!(!item.noun().is_empty());
            }
        }
    }

    /// One filter per accepted style, to check each style is reachable.
    fn singles(a: Accept) -> Vec<Accept> {
        use std::slice::from_ref as one;
        match a {
            A::Any(_) => vec![a],
            A::Garment(s) => s.iter().map(|x| A::Garment(one(x))).collect(),
            A::Hands(s) => s.iter().map(|x| A::Hands(one(x))).collect(),
            A::Feet(s) => s.iter().map(|x| A::Feet(one(x))).collect(),
            A::Legs(s) => s.iter().map(|x| A::Legs(one(x))).collect(),
            A::Hood(s) => s.iter().map(|x| A::Hood(one(x))).collect(),
            A::Hat(s) => s.iter().map(|x| A::Hat(one(x))).collect(),
            A::Bags(s) => s.iter().map(|x| A::Bags(one(x))).collect(),
            A::Food(s) => s.iter().map(|x| A::Food(one(x))).collect(),
            A::Sundry(s) => s.iter().map(|x| A::Sundry(one(x))).collect(),
            A::Writing(s) => s.iter().map(|x| A::Writing(one(x))).collect(),
            A::Axe(s) => s.iter().map(|x| A::Axe(one(x))).collect(),
        }
    }

    #[test]
    fn every_style_in_a_filter_rolls() {
        for (name, base) in ARMORY_BASES {
            let rolls: Vec<Item> = (1..120).map(|seed| base.roll(seed)).collect();
            for single in singles(base.accept) {
                assert!(rolls.iter().any(|item| single.accepts(item)), "{name}: {single:?} never rolls");
            }
        }
    }

    #[test]
    fn every_ported_family_has_a_base() {
        // A banner is scenery: the Armory draws it, but no loot base names it.
        for &family in Item::PORTED.iter().filter(|f| **f != Family::Banner) {
            let base = base_for(family, 1207).unwrap_or_else(|| panic!("{family:?} has no base"));
            assert_eq!(armory_base(base).unwrap().accept.family(), family);
            assert_eq!(base_for(family, 1207), Some(base));
        }
        let garments: Vec<_> = (0..200).filter_map(|seed| base_for(Family::Garment, seed)).collect();
        assert!(garments.contains(&"Robe") && garments.contains(&"Shirt"));
    }

    #[test]
    fn each_writing_base_rolls_its_own_kind() {
        for (name, kind) in [("Scroll", WritingKind::Scroll), ("Note", WritingKind::Note), ("Book", WritingKind::Book)] {
            for seed in 1..40 {
                match roll_shape(name, seed) {
                    Some(Item::Writing(s)) => assert_eq!(s.piece.kind, kind, "{name} {seed}"),
                    other => panic!("{name} {seed}: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn boots_roll_only_boots_and_sabatons() {
        for seed in 1..200 {
            match roll_shape("Boots", seed) {
                Some(Item::Feet(s)) => assert!(matches!(s.boot.style, BootStyle::Boot | BootStyle::Sabaton), "{seed}: {:?}", s.boot.style),
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn a_pointed_hat_is_a_hat_with_a_brim() {
        for seed in 1..200 {
            match roll_shape("Pointed hat", seed) {
                Some(Item::Hat(s)) => {
                    assert_eq!(s.hat.style, HatStyle::Pointed);
                    assert!(s.hat.brim > 0.0, "{seed}: no brim");
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn a_mantle_is_a_short_cloak() {
        for seed in 1..200 {
            match roll_shape("Mantle", seed) {
                Some(Item::Garment(s)) => {
                    assert_eq!(s.cut.style, GarmentStyle::Cloak);
                    assert!(s.cut.length < 1.6, "{seed}: {}", s.cut.length);
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn same_base_and_seed_same_shape() {
        for (name, _) in ARMORY_BASES {
            assert_eq!(roll_shape(name, 4242), roll_shape(name, 4242), "{name}");
        }
        assert_ne!(roll_shape("Sword", 1), roll_shape("Sword", 2));
        assert_eq!(roll_shape("Spoon", 1), None);
    }

    #[test]
    fn filters_reject_other_styles() {
        let boots = armory_base("Boots").unwrap();
        let shoes = armory_base("Shoes").unwrap();
        for seed in 1..40 {
            assert!(!shoes.accept.accepts(&boots.roll(seed)));
            assert!(!boots.accept.accepts(&Item::roll(Family::Mace, seed).unwrap()));
        }
    }

    #[test]
    fn tweaks_set_their_fields() {
        for seed in 1..30 {
            match armory_base("Purse").unwrap().roll(seed) {
                Item::Bags(s) => assert!(s.bag.coins),
                other => panic!("{other:?}"),
            }
            match armory_base("Mantle").unwrap().roll(seed) {
                Item::Garment(s) => assert!((0.4..1.6).contains(&s.cut.length)),
                other => panic!("{other:?}"),
            }
            match armory_base("Blind cavefish").unwrap().roll(seed) {
                Item::Food(s) => assert_eq!((s.provision.outer, s.provision.accent), (Rgb::hex(0xd8d2c4), Rgb::hex(0xece6d8))),
                other => panic!("{other:?}"),
            }
            match armory_base("Healing draught").unwrap().roll(seed) {
                Item::Food(s) => assert_eq!(Brew::nearest(s.provision.liquid), Brew::Healing),
                other => panic!("{other:?}"),
            }
            let glowcap = armory_base("Glowcap").unwrap().roll(seed);
            assert_eq!(&glowcap.noun()[..7], "Glowcap");
            match armory_base("Belt").unwrap().roll(seed) {
                Item::Belt(s) => assert!(s.fittings.pouch),
                other => panic!("{other:?}"),
            }
            match armory_base("Spare torch").unwrap().roll(seed) {
                Item::Torch(s) => assert_eq!(s.burn.state, BurnState::Fresh),
                other => panic!("{other:?}"),
            }
            assert!(armory_base("Waterskin").unwrap().roll(seed).noun() == "Waterskin");
        }
    }
}
