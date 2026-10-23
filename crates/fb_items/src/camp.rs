//! Camp goods: a bedroll and a tent, which a delver sets down on the floor to sleep in and packs
//! up again (`docs/items.md`, "Camp goods").
//!
//! Setting one down changes its form, not what it is: a bedroll is unrolled
//! ([`armory::families::sundry::Article::laid`]) and a tent pitched
//! ([`armory::families::tent::TentForm::Pitched`]). It stays the same item, with its seed,
//! rarity, affixes, history and wear, and the Armory draws it in its set-down form. The profile is
//! worked out again for the new form ([`Item::from_parts`]), so a pitched tent no longer fits a
//! pack and a packed one does.

use armory::families::sundry::SundryKind;
use armory::families::tent::TentForm;

use crate::item::{Item, ItemParts};
use crate::naming::{item_name, noun};

/// What kind of camp good a thing is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Camp {
    Bedroll,
    Tent,
}

impl Camp {
    /// What `shape` is, if it is a camp good in either form.
    pub fn of(shape: &armory::Item) -> Option<Camp> {
        match shape {
            armory::Item::Sundry(s) if s.article.kind == SundryKind::Bedroll => Some(Camp::Bedroll),
            armory::Item::Tent(_) => Some(Camp::Tent),
            _ => None,
        }
    }

    /// The word for setting it down: a bedroll is laid out, a tent pitched.
    pub fn set_verb(self) -> &'static str {
        match self {
            Camp::Bedroll => "Lay out",
            Camp::Tent => "Pitch",
        }
    }

    /// The word for taking it up.
    pub fn pack_verb(self) -> &'static str {
        "Pack up"
    }
}

/// Whether `shape` is a camp good in its set-down form: a bedroll laid out or a tent pitched.
pub fn is_set(shape: &armory::Item) -> bool {
    match shape {
        armory::Item::Sundry(s) => s.article.kind == SundryKind::Bedroll && s.article.laid,
        armory::Item::Tent(s) => s.tent.form == TentForm::Pitched,
        _ => false,
    }
}

impl Item {
    /// What camp good this is, in either form.
    pub fn camp(&self) -> Option<Camp> {
        Camp::of(&self.shape)
    }

    /// Whether it is a camp good lying set down, to be slept in.
    pub fn is_set_camp(&self) -> bool {
        is_set(&self.shape)
    }
}

/// Half the floor the canvas of a pitched tent covers, along its item x and z: a ridge tent's
/// length along the ridge and its width across the eaves, a bell tent's diameter. Guy lines and
/// pegs reach past it, and are not in the way. `None` for anything but a tent.
pub fn canvas_half(shape: &armory::Item) -> Option<(f32, f32)> {
    use armory::families::tent::TentStyle;
    let armory::Item::Tent(s) = shape else { return None };
    let t = &s.tent;
    Some(match t.style {
        TentStyle::Ridge => (t.length * 0.5, t.width * 0.5),
        TentStyle::Bell => (t.width * 0.5, t.width * 0.5),
    })
}

/// `item` in its set-down form: a bedroll laid out, a tent pitched. `None` for anything else, and
/// for a camp good that is set down already.
pub fn set_up(item: &Item) -> Option<Item> {
    if item.camp().is_none() || item.is_set_camp() {
        return None;
    }
    in_form(item, true)
}

/// `item` in its carried form: a bedroll rolled and strapped, a tent packed. `None` for anything
/// else, and for a camp good that is carried already.
pub fn pack_up(item: &Item) -> Option<Item> {
    if item.camp().is_none() || !item.is_set_camp() {
        return None;
    }
    in_form(item, false)
}

/// `item` with its shape in the set-down (`set`) or carried form, and nothing else changed but
/// what follows: its name if it was the shape's plain noun, and its profile.
fn in_form(item: &Item, set: bool) -> Option<Item> {
    let mut parts = ItemParts::from(item.clone());
    match &mut parts.shape {
        armory::Item::Sundry(s) => s.article.laid = set,
        armory::Item::Tent(s) => s.tent.form = if set { TentForm::Pitched } else { TentForm::Packed },
        _ => return None,
    }
    // A name that was only the shape's noun follows it ("Rolled Ridge Tent" becomes "Ridge Tent");
    // one that was given a maker's or an enchanter's word keeps it.
    let was = item_name(&noun(&item.base, &item.shape), &item.affixes);
    if parts.name == was {
        parts.name = item_name(&noun(&parts.base, &parts.shape), &parts.affixes);
    }
    Item::from_parts(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Odds, roll_item, roll_item_with};

    fn tent(seed: u32) -> Item {
        roll_item("Tent", seed).unwrap()
    }

    #[test]
    fn a_bedroll_lays_out_and_a_tent_pitches() {
        let bedroll = roll_item("Bedroll", 4).unwrap();
        assert_eq!(bedroll.camp(), Some(Camp::Bedroll));
        assert!(!bedroll.is_set_camp());
        let laid = set_up(&bedroll).unwrap();
        assert!(laid.is_set_camp());
        assert_ne!(laid.shape.build(), bedroll.shape.build());
        let packed = tent(4);
        assert_eq!(packed.camp(), Some(Camp::Tent));
        let pitched = set_up(&packed).unwrap();
        assert!(pitched.is_set_camp());
        // A pitched tent stays where it was put; a carried one fits a pack.
        assert!(packed.profile.packable && !pitched.profile.packable);
        assert!(armory::Family::Tent == pitched.shape.family());
    }

    #[test]
    fn a_tents_canvas_is_inside_all_its_ropes_and_pegs() {
        for seed in 1..20 {
            let pitched = set_up(&tent(seed)).unwrap();
            let (cx, cz) = canvas_half(&pitched.shape).unwrap();
            let (lo, hi) = pitched.shape.build().bounds().unwrap();
            assert!(cx <= (hi[0] - lo[0]) * 0.5 + 1e-3 && cz <= (hi[2] - lo[2]) * 0.5 + 1e-3, "{seed}: canvas {cx} {cz} in {lo:?} {hi:?}");
        }
        assert_eq!(canvas_half(&roll_item("Bedroll", 1).unwrap().shape), None);
    }

    #[test]
    fn only_camp_goods_set_up_and_only_once() {
        for base in ["Dagger", "Tinderbox", "Rope", "Bread"] {
            let item = roll_item(base, 3).unwrap();
            assert!(set_up(&item).is_none() && pack_up(&item).is_none(), "{base}");
            assert_eq!(item.camp(), None, "{base}");
        }
        let laid = set_up(&roll_item("Bedroll", 2).unwrap()).unwrap();
        assert!(set_up(&laid).is_none());
        assert!(pack_up(&roll_item("Bedroll", 2).unwrap()).is_none());
    }

    #[test]
    fn setting_down_and_packing_up_gives_the_same_item_back() {
        for (base, seed) in [("Bedroll", 1), ("Bedroll", 9), ("Tent", 1), ("Tent", 6), ("Tent", 12)] {
            let item = roll_item(base, seed).unwrap();
            let there = set_up(&item).unwrap();
            // Same item through and through but its form.
            assert_eq!((&there.base, there.seed, there.rarity), (&item.base, item.seed, item.rarity));
            assert_eq!(there.affixes, item.affixes);
            assert_eq!(there.history, item.history);
            let back = pack_up(&there).unwrap();
            assert_eq!(back, item, "{base} {seed}");
        }
    }

    #[test]
    fn wear_and_history_ride_along() {
        let mut item = roll_item_with("Tent", 5, Odds::default()).unwrap();
        item.history.push("Made by Brenna Holt".into());
        let armory::Item::Tent(s) = &mut item.shape else { panic!("tent") };
        s.cond.wear = 0.6;
        s.cond.fray = 0.4;
        let pitched = set_up(&item).unwrap();
        let armory::Item::Tent(p) = &pitched.shape else { panic!("tent") };
        assert_eq!((p.cond.wear, p.cond.fray), (0.6, 0.4));
        assert_eq!(pitched.history, item.history);
        let back = pack_up(&pitched).unwrap();
        let armory::Item::Tent(b) = &back.shape else { panic!("tent") };
        assert_eq!((b.cond.wear, b.cond.fray), (0.6, 0.4));
        assert_eq!(back.history, vec!["Made by Brenna Holt".to_string()]);
    }

    #[test]
    fn the_name_follows_the_form_unless_it_was_given() {
        let item = tent(2);
        let pitched = set_up(&item).unwrap();
        assert!(item.name.starts_with("Rolled") && !pitched.name.starts_with("Rolled"), "{} / {}", item.name, pitched.name);
        let mut given = tent(2);
        given.name = "Brenna's Tent".into();
        assert_eq!(set_up(&given).unwrap().name, "Brenna's Tent");
    }
}
