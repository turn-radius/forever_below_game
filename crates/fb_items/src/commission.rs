//! What a commissioned item is made of and what it costs, from its Armory shape alone. The
//! commission screen (`game/src/commission.rs`) sorts an item's fields into [`FieldClass`]es to
//! decide which a tier opens, and prices the shaped item with [`shaped_copper`]. See
//! `docs/items.md`, "Commissions".

use armory::Item as Shape;
use std::collections::HashSet;

use armory::reflect::{Field, FieldKind};

use crate::value::base_copper;

/// What a field of the Armory shape is for, which sets the tier that opens it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FieldClass {
    /// What a part is made of: any choice named `material`, `metal`, `plate`, `sole`, `buckle`,
    /// `binding`, `trim`, `fittings`, `fabric`, `canvas`, `stuff`, `vessel`, `cover` or `paper`.
    Material,
    /// A colour: every dye ([`FieldKind::Colour`]: a cloth's colour and pattern colour, a
    /// `*_color`, the palette's wood, leather, cord and fletching) and the heraldic `tincture`s.
    Colour,
    /// The form: every other slider, choice and toggle (lengths, widths, curves, styles, parts
    /// switched on or off).
    Shape,
    /// What is added to a made thing: the `etch` section, gems (`gem*` fields, the `gem` and
    /// `stone` sections, `stones`), `inlay`, and the heraldry of a shield's `field`
    /// (division and charge).
    Ornament,
}

/// The material choices by name (besides any name ending in `material`).
const MATERIAL_NAMES: [&str; 12] = ["metal", "plate", "sole", "buckle", "binding", "trim", "fittings", "fabric", "canvas", "stuff", "vessel", "cover"];

/// The class of a field, or `None` for one a commission never opens: the condition (`cond.*`),
/// which a new thing has none of.
pub fn classify(field: &Field) -> Option<FieldClass> {
    let root = field.path.split('.').next().unwrap_or_default();
    let name = field.name.as_str();
    if root == "cond" {
        return None;
    }
    if matches!(field.kind, FieldKind::Colour { .. }) {
        return Some(FieldClass::Colour);
    }
    let ornament = matches!(root, "etch" | "gem" | "stone" | "setting")
        || matches!(name, "inlay" | "stone" | "stones" | "side_stones" | "device" | "paper")
        || name.starts_with("gem")
        || (root == "field" && !is_colour(name));
    if ornament {
        return Some(FieldClass::Ornament);
    }
    if matches!(field.kind, FieldKind::Choice { .. }) {
        if is_colour(name) {
            return Some(FieldClass::Colour);
        }
        if name.ends_with("material") || MATERIAL_NAMES.contains(&name) {
            return Some(FieldClass::Material);
        }
    }
    Some(FieldClass::Shape)
}

fn is_colour(name: &str) -> bool {
    name.contains("tincture") || name.contains("color")
}

/// The fields of `shape` that show on the made thing: not the palette's tints of what it is not
/// made of, not those of a part that is switched off
/// (a section whose `on` toggle is false, its own toggle apart), not a gem's size, count or stone
/// while the gem toggle is off, and not what a garment's style leaves out: the body `material` of
/// a cloth garment (it is cloth) and the `cloth` of an armoured one.
pub fn visible_fields(shape: &Shape) -> Vec<Field> {
    let fields = shape.fields();
    let off: HashSet<String> = fields.iter().filter(|f| f.name == "on" && matches!(f.kind, FieldKind::Toggle { value: false })).map(|f| f.section.clone()).collect();
    let gem_off: HashSet<String> = fields.iter().filter(|f| f.name == "gem" && matches!(f.kind, FieldKind::Toggle { value: false })).map(|f| f.section.clone()).collect();
    let armour = shape.choice("cut.style").map(|style| matches!(style.as_str(), "hauberk" | "scale" | "cuirass"));
    // The palette's tints show only when something is made of what they dye.
    let made_of = |option: &str| fields.iter().any(|f| matches!(&f.kind, FieldKind::Choice { value, .. } if value == option) && classify(f) == Some(FieldClass::Material) && !off.contains(&f.section));
    let fletched = fields.iter().any(|f| f.name.starts_with("fletch") && !f.path.starts_with("palette."));
    let hidden = |f: &Field| {
        if f.name == "on" {
            return false;
        }
        if f.path.starts_with("palette.") {
            return match f.name.as_str() {
                "wood" => !made_of("wood"),
                "leather" => !made_of("leather"),
                "cord" => !made_of("string"),
                _ => !fletched,
            };
        }
        let mut prefix = String::new();
        for part in f.section.split('.') {
            if !prefix.is_empty() {
                prefix.push('.');
            }
            prefix.push_str(part);
            if off.contains(prefix.as_str()) {
                return true;
            }
        }
        (f.name.starts_with("gem_") && gem_off.contains(f.section.as_str()))
            || (armour == Some(false) && f.path == "cut.material")
            || (armour == Some(true) && f.path.starts_with("cut.cloth."))
    };
    fields.iter().filter(|f| !hidden(f)).cloned().collect()
}

/// What a section of the shape dresses, as the panel's heading says it ("Body cloth", "Trim cloth"):
/// `None` keeps the section's own name.
pub fn section_label(section: &str) -> Option<&'static str> {
    Some(match section {
        "cut" => "Body",
        "cut.cloth" => "Body cloth",
        "hem" => "Hem and trim",
        "hem.fabric" => "Trim cloth",
        "belt" => "Belt",
        "pouch" => "Pouch",
        "pouch.cloth" => "Pouch cloth",
        "hood.cloth" => "Hood cloth",
        "hat.cloth" => "Hat cloth",
        "hat.band_cloth" => "Hat band cloth",
        "legwear" => "Legs",
        "legwear.cloth" => "Leg cloth",
        "boot" => "Boots",
        "bag.cloth" => "Bag cloth",
        "chest.glaze" => "Glaze",
        "palette" => "Dyes",
        _ => return None,
    })
}

const METALS: [&str; 4] = ["metal", "accent", "silver", "gold"];

/// The options of the material choice `field` a maker offers for what it dresses: metals for a
/// blade, a head, a buckle or a suit of mail (and leather for scales), leather, cloth, cord and
/// wood for a grip, a wrap or a belt, wood, bone and iron for a shaft, and [`MADE_OF`] where no
/// rule is given. In the field's own order.
pub fn material_options(shape: &Shape, field: &Field) -> Vec<String> {
    let FieldKind::Choice { options, .. } = &field.kind else { return Vec::new() };
    if !options.iter().any(|o| MADE_OF.contains(&o.as_str())) {
        return options.clone();
    }
    let section = field.section.rsplit('.').next().unwrap_or_default();
    let allowed: Vec<&str> = match (section, field.name.as_str()) {
        ("cut", _) if shape.choice("cut.style").as_deref() == Some("scale") => vec!["metal", "accent", "silver", "gold", "leather"],
        ("cut", _) => METALS.to_vec(),
        ("belt", "material") => vec!["leather", "cloth", "string"],
        (_, "buckle" | "fittings" | "plate") => METALS.to_vec(),
        (_, "sole") => vec!["leather", "wood"],
        (_, "binding") => vec!["leather", "string", "cloth"],
        ("blade" | "head" | "spikes" | "flanges" | "prongs" | "boss" | "socket" | "plates" | "aventail" | "visor" | "cheeks" | "crest", _) => METALS.to_vec(),
        ("guard" | "pommel" | "finial", _) => vec!["metal", "accent", "silver", "gold", "bone"],
        ("grip" | "wrap" | "handle", _) => vec!["leather", "cloth", "string", "wood"],
        ("shaft" | "stock" | "limbs" | "riser", _) => vec!["wood", "bone", "metal"],
        _ => MADE_OF.to_vec(),
    };
    let kept: Vec<String> = options.iter().filter(|o| allowed.contains(&o.as_str())).cloned().collect();
    if kept.is_empty() { options.iter().filter(|o| MADE_OF.contains(&o.as_str())).cloned().collect() } else { kept }
}

/// The materials a commission offers, in the Armory's order: the selectable kinds and cloth.
/// A part's other options (food, skin, hair, glass, cut stones, paint) are not made to order.
pub const MADE_OF: [&str; 10] = ["metal", "accent", "gold", "silver", "wood", "bone", "stone", "leather", "string", "cloth"];

/// What a material option multiplies the price by (our own): iron is the measure, gold twelve
/// times it, string and stone the cheapest. Options that are not materials (a fabric's weave)
/// leave it as it was.
pub fn material_factor(option: &str) -> f64 {
    match option {
        "metal" => 1.0,
        "accent" => 1.3,
        "gold" => 12.0,
        "silver" => 5.0,
        "wood" => 0.6,
        "bone" => 0.5,
        "stone" => 0.4,
        "leather" => 0.8,
        "string" => 0.3,
        "cloth" => 0.5,
        "paint" => 0.7,
        _ => 1.0,
    }
}

/// What the size of the shaped item multiplies the price by (our own): the square root of its
/// bounds' volume over `reference`'s (a typical roll of the base), between half and two and a
/// half times.
pub fn size_factor(volume: f32, reference: f32) -> f64 {
    if reference <= 0.0 || volume <= 0.0 {
        return 1.0;
    }
    f64::from(volume / reference).sqrt().clamp(0.5, 2.5)
}

/// How much a material choice counts towards the price (our own): what the thing chiefly is
/// (a blade, a head, a body, limbs, a shaft, a tent's canvas) counts 3, small fittings (a buckle,
/// trim) a tenth, a grip, wrap, belt, guard or pommel a half, and the rest 1.
pub fn material_weight(field: &Field) -> f64 {
    let section = field.section.rsplit('.').next().unwrap_or_default();
    match (section, field.name.as_str()) {
        (_, "buckle" | "fittings" | "trim" | "binding") => 0.1,
        ("blade" | "head" | "cut" | "limbs" | "shaft" | "tent" | "body" | "stock" | "boot" | "legwear" | "hood" | "hat", _) => 3.0,
        ("grip" | "wrap" | "belt" | "guard" | "pommel" | "handle", _) => 0.5,
        _ => 1.0,
    }
}

/// What the visible material choices multiply the price by: 1 plus each one's [`material_factor`]
/// over iron, weighted by [`material_weight`], over the total weight or a main part's 3, whichever
/// is more. So a thing made all of one material takes its factor, and a gold buckle on a cloth coat
/// (which has no main material to speak of) is a little dearer, not twelve times.
pub fn shape_material_factor(shape: &Shape) -> f64 {
    let (mut over, mut weights) = (0.0, 0.0);
    for f in visible_fields(shape).iter().filter(|f| classify(f) == Some(FieldClass::Material)) {
        if let FieldKind::Choice { value, .. } = &f.kind {
            let w = material_weight(f);
            over += (material_factor(value) - 1.0) * w;
            weights += w;
        }
    }
    (1.0 + over / weights.max(3.0)).max(0.1)
}

/// What ornament multiplies the price by (our own): 1.2 for etching, 1.25 for each gem or inlay
/// switched on.
pub fn ornament_factor(shape: &Shape) -> f64 {
    visible_fields(shape)
        .iter()
        .filter(|f| classify(f) == Some(FieldClass::Ornament))
        .filter_map(|f| match &f.kind {
            FieldKind::Toggle { value: true } if f.path == "etch.on" => Some(1.2),
            FieldKind::Toggle { value: true } if f.name.starts_with("gem") || f.name == "inlay" => Some(1.25),
            _ => None,
        })
        .product()
}

/// What base `base` costs shaped as `shape` and `size` ([`size_factor`]), new and common, in
/// copper and at least 1: the base's price times material, size and ornament. The tier's
/// charge is the maker's, on top.
pub fn shaped_copper(base: &str, shape: &Shape, size: f64) -> u64 {
    let worth = f64::from(base_copper(base).unwrap_or(1)) * shape_material_factor(shape) * size * ornament_factor(shape);
    (worth.round() as u64).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armory_base::roll_shape;

    fn sword() -> Shape {
        roll_shape("Sword", 1207).unwrap()
    }

    fn class_of(shape: &Shape, path: &str) -> Option<FieldClass> {
        classify(&shape.fields().into_iter().find(|f| f.path == path).unwrap_or_else(|| panic!("no {path}")))
    }

    #[test]
    fn a_sword_sorts_into_material_shape_and_ornament() {
        let s = sword();
        assert_eq!(class_of(&s, "blade.material"), Some(FieldClass::Material));
        assert_eq!(class_of(&s, "grip.material"), Some(FieldClass::Material));
        assert_eq!(class_of(&s, "blade.length"), Some(FieldClass::Shape));
        assert_eq!(class_of(&s, "guard.style"), Some(FieldClass::Shape));
        assert_eq!(class_of(&s, "guard.on"), Some(FieldClass::Shape));
        assert_eq!(class_of(&s, "etch.pattern"), Some(FieldClass::Ornament));
        assert_eq!(class_of(&s, "pommel.gem_stone"), Some(FieldClass::Ornament));
        assert_eq!(class_of(&s, "guard.gem"), Some(FieldClass::Ornament));
        assert_eq!(class_of(&s, "cond.wear"), None);
    }

    #[test]
    fn every_ported_family_has_shape_and_a_dye_to_commission() {
        for &family in armory::Item::PORTED {
            let item = armory::Item::roll(family, 7).unwrap();
            let classes: Vec<_> = item.fields().iter().filter_map(classify).collect();
            assert!(classes.contains(&FieldClass::Shape), "{family:?} has no shape");
            assert!(classes.contains(&FieldClass::Colour), "{family:?} has no dye");
        }
        // Cloth and leather things have a dye of their own, not only the palette's.
        for family in [armory::Family::Garment, armory::Family::Hood, armory::Family::Hat, armory::Family::Belt, armory::Family::Legs, armory::Family::Tent, armory::Family::Bags] {
            let item = armory::Item::roll(family, 7).unwrap();
            assert!(item.fields().iter().any(|f| matches!(f.kind, FieldKind::Colour { .. }) && !f.path.starts_with("palette.")), "{family:?}");
        }
    }

    #[test]
    fn a_gold_buckle_on_an_absent_belt_costs_nothing() {
        let mut s = (0..200).filter_map(|n| roll_shape("Tunic", n)).find(|s| s.toggle("belt.on") == Some(false)).expect("a tunic with no belt");
        let before = shaped_copper("Tunic", &s, 1.0);
        s.set_choice("belt.buckle", "gold").unwrap();
        s.set_choice("belt.material", "gold").unwrap();
        assert_eq!(shaped_copper("Tunic", &s, 1.0), before);
        assert!(!visible_fields(&s).iter().any(|f| f.section == "belt" && f.name != "on"));
        s.set_toggle("belt.on", true).unwrap();
        assert!(shaped_copper("Tunic", &s, 1.0) > before);
    }

    #[test]
    fn a_gold_buckle_is_a_little_dearer_not_a_fortune() {
        let mut coat = (0..200).filter_map(|n| roll_shape("Gambeson", n)).find(|s| s.toggle("belt.on") == Some(true)).unwrap();
        coat.set_choice("belt.buckle", "metal").unwrap();
        let iron = shaped_copper("Gambeson", &coat, 1.0);
        coat.set_choice("belt.buckle", "gold").unwrap();
        let gold = shaped_copper("Gambeson", &coat, 1.0);
        assert!(gold > iron && (gold as f64) < iron as f64 * 1.5, "{iron} -> {gold}");
    }

    #[test]
    fn a_gambeson_has_no_fletching_dye() {
        for n in 0..20 {
            let coat = roll_shape("Gambeson", n).unwrap();
            assert!(!visible_fields(&coat).iter().any(|f| f.path == "palette.fletching"), "{n}");
        }
    }

    #[test]
    fn a_dye_shows_only_for_what_the_thing_is_made_of() {
        let cuirass = roll_shape("Cuirass", 3).unwrap();
        assert!(!visible_fields(&cuirass).iter().any(|f| f.path == "palette.fletching"));
        let arrow = roll_shape("Arrow", 3).unwrap();
        assert!(visible_fields(&arrow).iter().any(|f| f.path == "palette.fletching"));
    }

    #[test]
    fn a_cloth_garment_has_no_body_material_and_mail_offers_metals() {
        let gambeson = roll_shape("Gambeson", 3).unwrap();
        assert!(!visible_fields(&gambeson).iter().any(|f| f.path == "cut.material"));
        assert!(visible_fields(&gambeson).iter().any(|f| f.path == "cut.cloth.color"));
        let mail = roll_shape("Mail", 3).unwrap();
        let fields = visible_fields(&mail);
        let body = fields.iter().find(|f| f.path == "cut.material").unwrap();
        assert!(material_options(&mail, body).iter().all(|o| ["metal", "accent", "silver", "gold"].contains(&o.as_str())));
        assert!(!fields.iter().any(|f| f.path.starts_with("cut.cloth.")));
        let scale = roll_shape("Scale coat", 3).unwrap();
        let body = visible_fields(&scale).into_iter().find(|f| f.path == "cut.material").unwrap();
        assert!(material_options(&scale, &body).contains(&"leather".to_string()));
    }

    #[test]
    fn a_material_changes_the_price_and_gold_costs_most() {
        let mut s = sword();
        let _ = s.set_toggle("etch.on", false);
        for gem in ["pommel.gem", "guard.gem"] {
            let _ = s.set_toggle(gem, false);
        }
        let size = 1.0;
        let iron = {
            for f in s.fields().iter().filter(|f| classify(f) == Some(FieldClass::Material)) {
                let _ = s.set_choice(&f.path, "metal");
            }
            shaped_copper("Sword", &s, size)
        };
        assert_eq!(iron, 1500, "iron is the measure");
        for part in ["blade", "guard", "pommel"] {
            s.set_choice(&format!("{part}.material"), "gold").unwrap();
        }
        assert!(shaped_copper("Sword", &s, size) > iron * 2);
        s.set_choice("blade.material", "wood").unwrap();
        assert!(shaped_copper("Sword", &s, size) != iron);
    }

    #[test]
    fn size_and_ornament_raise_the_price() {
        let s = sword();
        assert!(size_factor(8.0, 1.0) > size_factor(1.0, 1.0));
        assert_eq!(size_factor(0.0, 1.0), 1.0);
        assert!(size_factor(1e9, 1.0) <= 2.5);
        let mut etched = s.clone();
        etched.set_toggle("etch.on", true).unwrap();
        let mut plain = s;
        plain.set_toggle("etch.on", false).unwrap();
        assert!(ornament_factor(&etched) > ornament_factor(&plain));
    }
}
