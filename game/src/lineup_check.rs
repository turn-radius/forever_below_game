//! What a faction expects of its members, and whether a member meets it (the lineup view,
//! [`crate::lineup_view`]). All plain data: no Bevy, no drawing.
//!
//! [`expectations`] lists what the faction asks, from its name (sex and classes) and from the
//! spec (colours, armour lean, outfits, folk or insular, alignment, likes and hates).
//! [`check_member`] sets one person, as the level makes them ([`Person::of`]), against the spec
//! and returns every [`Miss`]: no faction colours on their cloth, a kit the faction does not
//! dress in, armour of another lean, the wrong sex or folk, something the faction hates. Whatever
//! the [`FactionSpec`] carries is read here, so a new field of it is one more line in each.

use armory::heraldry::Tincture;
use armory::sections::garment::GarmentStyle;
use armory::sections::hood::HoodStyle;
use fb_faction::{ArmourLean, ClassKind, CreatureKind, FactionSpec, FolkSaid, Garment, Preference, Trim};
use fb_items::{Item, Sex, Slot};
use fb_level::FolkSpot;
use procgen_rng::Mulberry32;

use crate::folk::Person;

/// How many members a lineup shows.
pub const MEMBERS: usize = 8;
/// How deep the level they are met on is, which their gear's rarity is rolled for.
pub const DEPTH: u32 = 1;
/// Salt for a lineup's stream of members, drawn from the world seed and the faction. Our own.
const LINEUP_SALT: u32 = 0x11E0_0687;

/// The `count` members of `faction` a lineup of world `world_seed` shows: spots drawn as the
/// level draws a camp's ([`fb_level::folk`]), a folk from the faction's lean and then a seed,
/// the sex the faction's name gives or their seed rolls. A pure function of the three.
pub fn roll_members(faction: &FactionSpec, world_seed: u32, count: usize) -> Vec<FolkSpot> {
    let mut rng = Mulberry32::new(world_seed.wrapping_mul(0x9E37_79B1) ^ LINEUP_SALT ^ (faction.id.0 as u32).wrapping_mul(0x85EB_CA6B));
    (0..count)
        .map(|_| {
            let folk = faction.member_folk(&mut rng);
            let seed = (rng.next_f64() * 4_294_967_296.0) as u32;
            FolkSpot { x: 0.0, y: 0.0, yaw: 0.0, folk, seed, sex: faction.member_sex(seed), faction: faction.id, trade: None }
        })
        .collect()
}

/// A tincture's heraldic name and what it looks like: "gules (red)".
pub fn tincture_name(tincture: Tincture) -> &'static str {
    match tincture {
        Tincture::Or => "or (gold)",
        Tincture::Argent => "argent (silver)",
        Tincture::Gules => "gules (red)",
        Tincture::Azure => "azure (blue)",
        Tincture::Vert => "vert (green)",
        Tincture::Purpure => "purpure (purple)",
        Tincture::Sable => "sable (black)",
    }
}

/// The heraldic name alone: "gules".
fn heraldic(tincture: Tincture) -> &'static str {
    tincture_name(tincture).split(' ').next().unwrap_or_default()
}

/// A faction's colours as tincture names: "gules and or".
pub fn colours_line(faction: &FactionSpec) -> String {
    let field = &faction.field;
    if field.tincture == field.tincture2 { heraldic(field.tincture).to_string() } else { format!("{} and {}", heraldic(field.tincture), heraldic(field.tincture2)) }
}

fn class_name(class: ClassKind) -> &'static str {
    match class {
        ClassKind::Fighter => "fighter",
        ClassKind::Ranger => "ranger",
        ClassKind::Mage => "mage",
        ClassKind::Cleric => "cleric",
    }
}

fn lean_name(lean: ArmourLean) -> &'static str {
    match lean {
        ArmourLean::Padded => "padded",
        ArmourLean::Leather => "leather",
        ArmourLean::Mail => "mail",
        ArmourLean::Scale => "scale",
        ArmourLean::Plate => "plate",
    }
}

fn creature_name(kind: CreatureKind) -> &'static str {
    match kind {
        CreatureKind::Rats => "rats",
        CreatureKind::Snakes => "snakes",
        CreatureKind::Spiders => "spiders",
        CreatureKind::Undead => "the undead",
        CreatureKind::Bats => "bats",
        CreatureKind::Centipedes => "centipedes",
        CreatureKind::Trolls => "trolls",
        CreatureKind::Jackalopes => "jackalopes",
    }
}

/// What a faction likes or hates, in words; another faction by its name in `world`.
pub fn preference_name(preference: Preference, world: &[FactionSpec]) -> String {
    match preference {
        Preference::Creature(kind) => creature_name(kind).to_string(),
        Preference::Faction(id) => world.iter().find(|f| f.id == id).map_or_else(|| format!("faction {}", id.0), |f| f.name.clone()),
        Preference::Folk(folk) => format!("{} folk", folk.race().label.to_lowercase()),
        Preference::Class(class) => format!("{}s", class_name(class)),
    }
}

fn join(words: impl IntoIterator<Item = String>) -> String {
    let words: Vec<String> = words.into_iter().collect();
    match words.as_slice() {
        [] => "none".into(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// The sex a faction's name sets, in words.
pub fn sex_line(sex: Option<Sex>) -> &'static str {
    match sex {
        Some(Sex::Female) => "women",
        Some(Sex::Male) => "men",
        None => "women and men",
    }
}

/// The classes a faction's name sets: "cleric", "fighter and ranger", or none.
pub fn classes_line(faction: &FactionSpec) -> String {
    if faction.classes.is_empty() { "none named".into() } else { join(faction.classes.iter().map(|&c| class_name(c).to_string())) }
}

/// What the faction expects of its members, as (heading, text) lines. The first lines are what
/// the name says (sex, classes), the rest what the spec carries.
pub fn expectations(faction: &FactionSpec, world: &[FactionSpec]) -> Vec<(&'static str, String)> {
    let outfits = join(faction.outfits.iter().map(|o| o.key().to_string()));
    let folk = faction.folk.race().label.to_lowercase();
    let folk_line = match faction.looks.folk {
        Some(FolkSaid::Mixed) => "every folk, evenly, as its name says".to_string(),
        Some(FolkSaid::Own) => format!("wholly {folk}, as its name says"),
        None if faction.insular => format!("wholly {folk}, and hates other folk"),
        None => format!("mostly {folk} (3 in 4), any other folk besides"),
    };
    let said = if faction.looks.colours.is_empty() { "" } else { ", set by its name" };
    let mut lines = vec![
        ("Sex", sex_line(faction.sex).to_string()),
        ("Classes", classes_line(faction)),
        ("Colours", format!("{}{said}; cloth in these dyes, shields in these arms", colours_line(faction))),
    ];
    if let Some(garment) = faction.looks.garment {
        lines.push(("Garment", match garment {
            Garment::Robe => "a robe as the outer layer".to_string(),
            Garment::RobeOverArmour => format!("{} armour with a robe over it", lean_name(faction.armour)),
        }));
    }
    if let Some(Trim::GoldHem) = faction.looks.trim {
        lines.push(("Trim", "a gold hem, neck and cuffs".to_string()));
    }
    lines.extend([
        ("Outfits", outfits),
        ("Armour", format!("leans {}{}", lean_name(faction.armour), if faction.looks.armour.is_some() { ", as its name says" } else { "" })),
        ("Folk", folk_line),
        ("Alignment", faction.alignment.label().to_string()),
        ("Likes", join(faction.likes.iter().map(|&p| preference_name(p, world)))),
        ("Hates", join(faction.hates.iter().map(|&p| preference_name(p, world)))),
    ]);
    lines
}

/// Which expectation a member misses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    Colours,
    Kit,
    Armour,
    Garment,
    SexOrFolk,
    Hates,
}

impl Rule {
    pub const ALL: [Rule; 6] = [Rule::Colours, Rule::Kit, Rule::Armour, Rule::Garment, Rule::SexOrFolk, Rule::Hates];

    /// The check's name in the panel.
    pub fn label(self) -> &'static str {
        match self {
            Rule::Colours => "faction colours",
            Rule::Kit => "class kit",
            Rule::Armour => "armour lean",
            Rule::Garment => "garment and trim",
            Rule::SexOrFolk => "sex and folk",
            Rule::Hates => "nothing hated",
        }
    }
}

/// One expectation a member misses, and what is wrong, without the name: "no faction colours on
/// her robe".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Miss {
    pub rule: Rule,
    pub problem: String,
}

/// A line for the panel: the member's name, then what is wrong.
pub fn describe(person: &Person, miss: &Miss) -> String {
    format!("{}: {}", person.name, miss.problem)
}

/// Whether `rule` is met by a member with these `misses`.
pub fn passes(misses: &[Miss], rule: Rule) -> bool {
    misses.iter().all(|m| m.rule != rule)
}

fn his(sex: Sex) -> &'static str {
    if sex == Sex::Female { "her" } else { "his" }
}

fn his_list(sex: Sex, things: &str) -> String {
    format!("{} {things}", his(sex))
}

/// The dye of a piece of cloth's main colour.
fn cloth_colour(item: &Item) -> Option<armory::material::Rgb> {
    match &item.shape {
        armory::Item::Garment(g) => Some(g.cut.cloth.color),
        armory::Item::Hood(h) => Some(h.hood.cloth.color),
        _ => None,
    }
}

fn lowercase_base(item: &Item) -> String {
    item.base.to_lowercase()
}

/// Whether `item` is cloth the faction's livery dresses (`fb_items::outfit::Livery`): a garment
/// that is not armour or a leather jerkin, or a hood that is not a mail coif.
fn takes_livery(item: &Item) -> bool {
    match &item.shape {
        armory::Item::Garment(g) => item.base != "Leather jerkin" && !g.cut.style.is_armour(),
        armory::Item::Hood(h) => h.hood.style != HoodStyle::MailCoif,
        _ => false,
    }
}

/// Whether `item`'s cloth or its trim is dyed in either of the faction's tinctures.
fn shows_colours(item: &Item, faction: &FactionSpec) -> bool {
    let dyes = [faction.field.tincture.dye(), faction.field.tincture2.dye()];
    let shown = |colours: [armory::material::Rgb; 2]| colours.iter().any(|c| dyes.contains(c));
    match &item.shape {
        armory::Item::Garment(g) => shown([g.cut.cloth.color, g.cut.cloth.color2]) || (g.hem.on && shown([g.hem.fabric.color, g.hem.fabric.color2])),
        armory::Item::Hood(h) => shown([h.hood.cloth.color, h.hood.cloth.color2]) || (h.hem.on && shown([h.hem.fabric.color, h.hem.fabric.color2])),
        _ => false,
    }
}

/// Whether a piece of cloth that takes a hem has one, in gold.
fn gold_hemmed(item: &Item, gold: armory::material::Rgb) -> bool {
    match &item.shape {
        armory::Item::Garment(g) if matches!(g.cut.style, GarmentStyle::Tunic | GarmentStyle::Cloak | GarmentStyle::Robe) => g.hem.on && g.hem.fabric.color == gold,
        armory::Item::Hood(h) => h.hem.on && h.hem.fabric.color == gold,
        _ => true,
    }
}

/// The lean of a body armour base, if it is one.
fn body_lean(base: &str) -> Option<ArmourLean> {
    Some(match base {
        "Gambeson" => ArmourLean::Padded,
        "Leather jerkin" => ArmourLean::Leather,
        "Mail" => ArmourLean::Mail,
        "Scale coat" => ArmourLean::Scale,
        "Cuirass" | "Brigandine" => ArmourLean::Plate,
        _ => return None,
    })
}

/// Bases any outfit may add beyond its table: the rolled extras, a bow's quiver, and the torch
/// nobody stands without.
const EXTRAS: [&str; 7] = ["Ring", "Amulet", "Charm", "Pointed hat", "Quiver", "Bolt case", "Torch"];

/// Words in an item's name that stand for a kind of creature, whole words only.
fn creature_words(kind: CreatureKind) -> &'static [&'static str] {
    match kind {
        CreatureKind::Rats => &["rat", "rats"],
        CreatureKind::Snakes => &["snake", "snakes", "snakeskin", "serpent"],
        CreatureKind::Spiders => &["spider", "spiders", "spidersilk"],
        CreatureKind::Undead => &["skull", "skulls", "bone", "bones", "skeleton", "grave", "wight", "zombie"],
        CreatureKind::Bats => &["bat", "bats"],
        CreatureKind::Centipedes => &["centipede", "centipedes"],
        CreatureKind::Trolls => &["troll", "trolls"],
        CreatureKind::Jackalopes => &["jackalope", "jackalopes", "antler", "antlers"],
    }
}

fn names_creature(item: &Item, kind: CreatureKind) -> bool {
    let text = format!("{} {}", item.base, item.title()).to_lowercase();
    text.split(|c: char| !c.is_alphabetic()).any(|word| creature_words(kind).contains(&word))
}

/// Every expectation `person` misses of `faction`, in the order of [`Rule::ALL`]. `world` is
/// the pool the faction is in, whose other factions it may hate. A pure function of its inputs.
pub fn check_member(world: &[FactionSpec], person: &Person) -> Vec<Miss> {
    let faction = &person.faction;
    let sex = person.spot.sex;
    let mut misses = Vec::new();
    let mut miss = |rule, problem: String| misses.push(Miss { rule, problem });

    // Colours: the cloth that takes the livery, and a shield's arms.
    // What a robe over the armour covers is not seen, so it is not asked to wear the colours.
    let covered = person.item(Slot::Cloak).is_some_and(|c| c.base == "Robe");
    let cloth: Vec<&(Slot, Item)> = person.gear.iter().filter(|(slot, item)| takes_livery(item) && !(covered && matches!(slot, Slot::Body | Slot::Shirt))).collect();
    if !cloth.is_empty() {
        let names = |pieces: &[&(Slot, Item)]| join(pieces.iter().map(|(_, item)| lowercase_base(item)));
        match faction.looks.colours.first() {
            // A name that gives the colours has every piece of cloth plainly in the first.
            Some(&main) => {
                let off: Vec<&(Slot, Item)> = cloth.iter().copied().filter(|(_, item)| cloth_colour(item) != Some(main.dye())).collect();
                if !off.is_empty() {
                    miss(Rule::Colours, format!("{} {} not in {}, the name's colour", his_list(sex, &names(&off)), if off.len() == 1 { "is" } else { "are" }, heraldic(main)));
                }
            }
            None if !cloth.iter().any(|(_, item)| shows_colours(item, faction)) => miss(Rule::Colours, format!("no faction colours on {} {}", his(sex), names(&cloth))),
            None => {}
        }
    }
    if let Some(shield) = person.gear.iter().find_map(|(_, item)| if let armory::Item::Shield(s) = &item.shape { Some(s) } else { None })
        && shield.field != faction.field
    {
        miss(Rule::Colours, format!("{} shield is not painted with the faction's arms", his(sex)));
    }

    // The kit: one of the faction's outfits, of a class its name allows, made of that outfit's
    // own bases.
    if !faction.outfit_keys().contains(&person.outfit) {
        miss(Rule::Kit, format!("dressed as a {}, which is not one of the faction's outfits ({})", person.outfit, join(faction.outfit_keys().map(String::from))));
    }
    if let Some(class) = class_of(person)
        && !faction.classes.is_empty()
        && !faction.classes.contains(&class)
    {
        miss(Rule::Kit, format!("a {} in a faction of {}s", class_name(class), join(faction.classes.iter().map(|&c| class_name(c).to_string())).replace(" and ", "s and ")));
    }
    if let Some(outfit) = fb_items::outfit::outfit(person.outfit) {
        for (slot, item) in &person.gear {
            let listed = outfit.slots.iter().any(|(s, choice)| s == slot && choice.bases.contains(&item.base));
            // What its name's garment adds to the kit: the robe, and the lean's armour under it.
            let from_name = faction.looks.garment.is_some() && (item.base == "Robe" || (*slot == Slot::Body && body_lean(&item.base).is_some()));
            if !listed && !from_name && !EXTRAS.contains(&item.base.as_str()) {
                miss(Rule::Kit, format!("wears a {} that is not in the {} kit", lowercase_base(item), person.outfit));
            }
        }
    }

    // Armour: body armour of another lean than the faction's.
    if let Some((_, body)) = person.gear.iter().find(|(slot, _)| *slot == Slot::Body)
        && let Some(lean) = body_lean(&body.base)
        && lean != faction.armour
    {
        miss(Rule::Armour, format!("wears {} armour ({}), not the faction's {}", lean_name(lean), lowercase_base(body), lean_name(faction.armour)));
    }

    // Sex and folk.
    if let Some(wanted) = faction.sex
        && wanted != sex
    {
        miss(Rule::SexOrFolk, format!("is {}, in a faction of {}", if sex == Sex::Female { "a woman" } else { "a man" }, sex_line(Some(wanted))));
    }
    if person.spec.woman != (sex == Sex::Female) {
        miss(Rule::SexOrFolk, "has the body of the other sex".to_string());
    }
    if faction.insular && person.spot.folk != faction.folk {
        miss(Rule::SexOrFolk, format!("is {}, in a wholly {} faction", person.spot.folk.race().label.to_lowercase(), faction.folk.race().label.to_lowercase()));
    }
    if person.spec.folk != person.spot.folk {
        miss(Rule::SexOrFolk, "has the body of another folk".to_string());
    }

    // The garment and trim its name gives.
    let robe_in = |slot: Slot| person.item(slot).is_some_and(|i| i.base == "Robe");
    let outer_robe = robe_in(Slot::Cloak) || ((robe_in(Slot::Body) || robe_in(Slot::Shirt)) && person.item(Slot::Cloak).is_none());
    match faction.looks.garment {
        Some(Garment::Robe) if !outer_robe => miss(Rule::Garment, "no robe as the outer layer".to_string()),
        Some(Garment::RobeOverArmour) => {
            let armoured = person.item(Slot::Body).is_some_and(|b| body_lean(&b.base).is_some());
            if !armoured {
                miss(Rule::Garment, "no body armour under the robe".to_string());
            }
            if !robe_in(Slot::Cloak) {
                miss(Rule::Garment, "no robe over the armour".to_string());
            }
        }
        _ => {}
    }
    if faction.looks.trim == Some(Trim::GoldHem) {
        let gold = armory::heraldry::Tincture::Or.dye();
        let bare: Vec<&(Slot, Item)> = cloth.iter().copied().filter(|(_, item)| (shows_colours(item, faction) || !faction.looks.colours.is_empty()) && !gold_hemmed(item, gold)).collect();
        if !bare.is_empty() {
            miss(Rule::Garment, format!("no gold hem on {} {}", his(sex), join(bare.iter().map(|(_, i)| lowercase_base(i)))));
        }
    }
    if faction.looks.folk == Some(FolkSaid::Own) && person.spot.folk != faction.folk {
        miss(Rule::SexOrFolk, format!("is {}, in a faction its name says is wholly {}", person.spot.folk.race().label.to_lowercase(), faction.folk.race().label.to_lowercase()));
    }

    // Hates.
    for &hated in &faction.hates {
        match hated {
            Preference::Folk(folk) if person.spot.folk == folk => miss(Rule::Hates, format!("is {}, whom the faction hates", preference_name(hated, world))),
            Preference::Class(class) if class_of(person) == Some(class) => miss(Rule::Hates, format!("dresses as a {}, which the faction hates", class_name(class))),
            Preference::Creature(kind) => {
                for item in person.gear.iter().map(|(_, item)| item).chain(&person.goods) {
                    if names_creature(item, kind) {
                        miss(Rule::Hates, format!("carries a {}, and the faction hates {}", lowercase_base(item), creature_name(kind)));
                    }
                }
            }
            Preference::Faction(other) => {
                let Some(other) = world.iter().find(|f| f.id == other) else { continue };
                let arms = person.gear.iter().any(|(_, item)| matches!(&item.shape, armory::Item::Shield(s) if s.field == other.field));
                if arms {
                    miss(Rule::Hates, format!("carries the arms of {}, whom the faction hates", other.name));
                }
            }
            _ => {}
        }
    }
    misses.sort_by_key(|m| Rule::ALL.iter().position(|r| *r == m.rule));
    misses
}

/// The faction outfit with this `data/outfits.json` key.
fn outfit_named(key: &str) -> Option<fb_faction::Outfit> {
    use fb_faction::Outfit;
    [Outfit::Fighter, Outfit::Ranger, Outfit::Mage, Outfit::Cleric, Outfit::Scavenger].into_iter().find(|o| o.key() == key)
}

/// The class a person's outfit dresses, if it is one.
fn class_of(person: &Person) -> Option<ClassKind> {
    outfit_named(person.outfit).and_then(fb_faction::Outfit::class)
}

/// The kit a person is dressed in, for a caption: the outfit's label ("Fighter", "Freshly
/// fallen").
pub fn class_label(person: &Person) -> String {
    fb_items::outfit::outfit(person.outfit).map_or_else(|| person.outfit.to_string(), |o| o.label.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_faction::{Faction, World};
    use fb_items::roll_item;

    /// A faction to test against: the first of world 7, set to what each test needs.
    fn faction() -> FactionSpec {
        let mut spec = World::new(7).factions()[0].clone();
        spec.sex = None;
        spec.classes = vec![];
        spec.insular = false;
        spec.hates = vec![];
        spec.armour = ArmourLean::Mail;
        spec.outfits = [fb_faction::Outfit::Mage, fb_faction::Outfit::Cleric];
        spec
    }

    /// A member of `spec` who wears the robe of `outfit`, built as the level builds them.
    fn member(spec: &FactionSpec, seed: u32) -> Person {
        let spot = FolkSpot { x: 0.0, y: 0.0, yaw: 0.0, folk: spec.folk, seed, sex: spec.member_sex(seed), faction: spec.id, trade: None };
        Person::of(&spot, spec, DEPTH)
    }

    /// A person dressed in a plain robe in `colours` (and nothing else cloth), so the colour
    /// check has one piece to look at.
    fn robed(spec: &FactionSpec, colours: Tincture) -> Person {
        let mut person = member(spec, 5);
        person.gear.retain(|(slot, item)| !takes_livery(item) && !matches!(slot, Slot::Body | Slot::Offhand | Slot::Mainhand));
        let mut robe = roll_item("Robe", 3).or_else(|| roll_item("Tunic", 3)).expect("a robe or a tunic is a base");
        if let armory::Item::Garment(g) = &mut robe.shape {
            g.cut.style = armory::sections::garment::GarmentStyle::Robe;
            g.cut.cloth.color = colours.dye();
            g.cut.cloth.color2 = colours.dye();
            g.hem.on = false;
        }
        person.gear.push((Slot::Body, robe));
        person
    }

    #[test]
    fn the_same_world_and_faction_roll_the_same_members() {
        let spec = faction();
        assert_eq!(roll_members(&spec, 113, MEMBERS), roll_members(&spec, 113, MEMBERS));
        assert_ne!(roll_members(&spec, 113, MEMBERS), roll_members(&spec, 114, MEMBERS));
        assert_eq!(roll_members(&spec, 113, MEMBERS).len(), MEMBERS);
        assert!(roll_members(&spec, 113, MEMBERS).iter().all(|s| s.sex == spec.member_sex(s.seed) && s.faction == spec.id));
    }

    #[test]
    fn a_faction_of_one_sex_rolls_only_that_sex() {
        let mut spec = faction();
        spec.sex = Some(Sex::Female);
        assert!(roll_members(&spec, 5, 40).iter().all(|s| s.sex == Sex::Female));
    }

    #[test]
    fn cloth_in_the_factions_dyes_shows_its_colours() {
        let spec = faction();
        let ours = robed(&spec, spec.field.tincture);
        assert!(passes(&check_member(&[spec.clone()], &ours), Rule::Colours));
    }

    #[test]
    fn cloth_in_other_dyes_misses_the_colours_and_names_the_garment() {
        let spec = faction();
        let other = Tincture::ALL.into_iter().find(|t| *t != spec.field.tincture && *t != spec.field.tincture2).unwrap();
        let mut person = robed(&spec, other);
        person.spot.sex = Sex::Female;
        let misses = check_member(&[spec.clone()], &person);
        let miss = misses.iter().find(|m| m.rule == Rule::Colours).expect("flagged");
        assert!(miss.problem.starts_with("no faction colours on her "), "{}", miss.problem);
        assert!(describe(&person, miss).starts_with(&format!("{}: no faction colours", person.name)));
    }

    #[test]
    fn armour_alone_is_not_a_missing_colour() {
        let spec = faction();
        let mut person = member(&spec, 9);
        person.gear.retain(|(_, item)| !takes_livery(item));
        assert!(passes(&check_member(&[spec], &person), Rule::Colours));
    }

    #[test]
    fn a_shield_must_carry_the_factions_arms() {
        let mut spec = faction();
        spec.outfits = [fb_faction::Outfit::Fighter, fb_faction::Outfit::Cleric];
        let mut person = member(&spec, 11);
        let mut shield = roll_item("Shield", 4).unwrap();
        if let armory::Item::Shield(s) = &mut shield.shape {
            s.field = spec.field;
        }
        person.gear.retain(|(slot, _)| *slot != Slot::Offhand);
        person.gear.push((Slot::Offhand, shield.clone()));
        assert!(passes(&check_member(&[spec.clone()], &person), Rule::Colours));
        if let armory::Item::Shield(s) = &mut shield.shape {
            s.field.tincture = if spec.field.tincture == Tincture::Vert { Tincture::Gules } else { Tincture::Vert };
        }
        person.gear.retain(|(slot, _)| *slot != Slot::Offhand);
        person.gear.push((Slot::Offhand, shield));
        assert!(!passes(&check_member(&[spec], &person), Rule::Colours));
    }

    #[test]
    fn armour_of_another_lean_is_flagged_and_of_the_lean_is_not() {
        let mut spec = faction();
        spec.armour = ArmourLean::Leather;
        let mut person = member(&spec, 3);
        person.gear.retain(|(slot, _)| *slot != Slot::Body);
        person.gear.push((Slot::Body, roll_item("Mail", 1).unwrap()));
        let misses = check_member(&[spec.clone()], &person);
        assert!(misses.iter().any(|m| m.rule == Rule::Armour && m.problem.contains("mail") && m.problem.contains("leather")), "{misses:?}");
        person.gear.retain(|(slot, _)| *slot != Slot::Body);
        person.gear.push((Slot::Body, roll_item("Leather jerkin", 1).unwrap()));
        assert!(passes(&check_member(&[spec], &person), Rule::Armour));
    }

    #[test]
    fn a_member_of_the_wrong_sex_or_folk_is_flagged() {
        let mut spec = faction();
        spec.sex = Some(Sex::Female);
        spec.insular = true;
        spec.folk = fb_body::Folk::Dwarf;
        let mut person = member(&spec, 2);
        assert!(passes(&check_member(&[spec.clone()], &person), Rule::SexOrFolk), "{:?}", check_member(&[spec.clone()], &person));
        person.spot.sex = Sex::Male;
        person.spot.folk = fb_body::Folk::Human;
        let misses = check_member(&[spec], &person);
        let problems: Vec<&str> = misses.iter().filter(|m| m.rule == Rule::SexOrFolk).map(|m| m.problem.as_str()).collect();
        assert!(problems.iter().any(|p| p.contains("a man, in a faction of women")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("human, in a wholly dwarf faction")), "{problems:?}");
    }

    #[test]
    fn a_kit_the_faction_does_not_wear_is_flagged() {
        let mut spec = faction();
        spec.classes = vec![ClassKind::Cleric];
        spec.outfits = [fb_faction::Outfit::Cleric, fb_faction::Outfit::Cleric];
        let mut person = member(&spec, 8);
        assert!(passes(&check_member(&[spec.clone()], &person), Rule::Kit), "{:?}", check_member(&[spec.clone()], &person));
        person.outfit = "fighter";
        let misses = check_member(&[spec], &person);
        assert!(misses.iter().any(|m| m.rule == Rule::Kit && m.problem.contains("not one of the faction's outfits")), "{misses:?}");
        assert!(misses.iter().any(|m| m.rule == Rule::Kit && m.problem.contains("fighter in a faction of clerics")), "{misses:?}");
    }

    #[test]
    fn nothing_hated_is_carried() {
        let mut spec = faction();
        spec.hates = vec![Preference::Creature(CreatureKind::Rats), Preference::Folk(fb_body::Folk::Goblin)];
        let mut person = member(&spec, 6);
        person.spot.folk = fb_body::Folk::Human;
        assert!(passes(&check_member(&[spec.clone()], &person), Rule::Hates));
        person.spot.folk = fb_body::Folk::Goblin;
        assert!(!passes(&check_member(&[spec.clone()], &person), Rule::Hates));
        person.spot.folk = fb_body::Folk::Human;
        let mut rat = roll_item("Torch", 1).unwrap();
        rat.base = "Rat tail".into();
        rat.name = "Rat tail".into();
        person.goods.push(rat);
        let misses = check_member(&[spec], &person);
        assert!(misses.iter().any(|m| m.rule == Rule::Hates && m.problem.contains("rat tail")), "{misses:?}");
    }

    #[test]
    fn a_hated_factions_arms_are_flagged() {
        let world = World::new(7).factions().to_vec();
        let mut spec = world[0].clone();
        spec.classes = vec![];
        spec.outfits = [fb_faction::Outfit::Fighter, fb_faction::Outfit::Fighter];
        let other = world[1].clone();
        spec.hates = vec![Preference::Faction(other.id)];
        let mut person = member(&spec, 12);
        let mut shield = roll_item("Shield", 4).unwrap();
        if let armory::Item::Shield(s) = &mut shield.shape {
            s.field = other.field;
        }
        person.gear.retain(|(slot, _)| *slot != Slot::Offhand);
        person.gear.push((Slot::Offhand, shield));
        let misses = check_member(&[spec.clone(), other.clone()], &person);
        assert!(misses.iter().any(|m| m.rule == Rule::Hates && m.problem.contains(&other.name)), "{misses:?}");
        let _ = Faction(0);
    }

    #[test]
    fn expectations_name_the_sex_classes_colours_and_leanings() {
        let mut spec = faction();
        spec.sex = Some(Sex::Female);
        spec.classes = vec![ClassKind::Cleric, ClassKind::Mage];
        spec.field.tincture = Tincture::Sable;
        spec.field.tincture2 = Tincture::Argent;
        let lines = expectations(&spec, &[spec.clone()]);
        let get = |heading: &str| lines.iter().find(|(h, _)| *h == heading).map(|(_, t)| t.as_str()).unwrap();
        assert_eq!(get("Sex"), "women");
        assert_eq!(get("Classes"), "cleric and mage");
        assert!(get("Colours").starts_with("sable and argent"));
        assert_eq!(get("Armour"), "leans mail");
        assert_eq!(colours_line(&spec), "sable and argent");
        let plain = FactionSpec { sex: None, classes: vec![], ..spec };
        let lines = expectations(&plain, &[]);
        assert_eq!(lines.iter().find(|(h, _)| *h == "Sex").unwrap().1, "women and men");
        assert_eq!(lines.iter().find(|(h, _)| *h == "Classes").unwrap().1, "none named");
    }

    #[test]
    fn every_world_faction_can_be_lined_up_and_checked() {
        let world = World::new(113);
        for spec in world.factions() {
            for spot in roll_members(spec, 113, 3) {
                let person = Person::of(&spot, spec, DEPTH);
                let _ = check_member(world.factions(), &person);
            }
        }
    }

    #[test]
    fn a_name_that_gives_the_look_is_met_by_every_member_it_dresses() {
        let (mut named, mut checked) = (0, 0);
        for world in 1..=15 {
            let w = World::new(world);
            for spec in w.factions().iter().filter(|f| !f.looks.is_plain()) {
                named += 1;
                for spot in roll_members(spec, world, 3) {
                    let person = Person::of(&spot, spec, DEPTH);
                    checked += 1;
                    let misses = check_member(w.factions(), &person);
                    let bad: Vec<_> = misses.iter().filter(|m| matches!(m.rule, Rule::Garment | Rule::Kit) || (m.rule == Rule::Colours && !spec.looks.colours.is_empty())).collect();
                    assert!(bad.is_empty(), "world {world} {}: {} {bad:?} looks {:?} gear {:?}", spec.name, person.name, spec.looks, person.gear.iter().map(|(s, i)| format!("{s:?}:{}", i.base)).collect::<Vec<_>>());
                }
            }
        }
        assert!(named > 0 && checked > 0);
    }

    #[test]
    fn a_robe_and_gold_hem_are_expected_and_missing_ones_flagged() {
        let mut spec = faction();
        spec.looks.garment = Some(Garment::Robe);
        spec.looks.trim = Some(Trim::GoldHem);
        spec.looks.colours = vec![spec.field.tincture];
        let lines = expectations(&spec, &[spec.clone()]);
        assert!(lines.iter().any(|(h, t)| *h == "Garment" && t.contains("robe")));
        assert!(lines.iter().any(|(h, _)| *h == "Trim"));
        assert!(lines.iter().any(|(h, t)| *h == "Colours" && t.contains("set by its name")));
        let mut person = member(&spec, 4);
        person.gear.retain(|(slot, _)| !matches!(slot, Slot::Cloak | Slot::Shirt | Slot::Body));
        let misses = check_member(&[spec.clone()], &person);
        assert!(misses.iter().any(|m| m.rule == Rule::Garment && m.problem.contains("no robe")), "{misses:?}");
    }
}
