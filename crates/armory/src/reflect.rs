//! Looking at an [`Item`]'s parameters without knowing its family: the Forge view's sliders.
//!
//! Every family state is plain serde data, and every family declares its slider ranges in a
//! `LIMITS` const (the [`crate::slider::sliders!`] macro's `*Limits` structs, narrowed per
//! family). So the reflection goes through JSON instead of a hand-written list per family:
//!
//! - a number with a `{min, max, step}` in the family's limits is a [`FieldKind::Slider`];
//! - a bool is a [`FieldKind::Toggle`];
//! - a unit enum (serialized as a string) is a [`FieldKind::Choice`], whose options are the
//!   variants serde names when asked to read an unknown one;
//! - everything else (the seed, colours, numbers with no declared range) is not listed.
//!
//! Fields are named by a dotted path from the state: `head.radius`, `flame.fuel`, `cond.wear`.
//! The section is the path without its last part. Sections and fields come in the order the
//! state structs declare them. Setting a field edits only the item's state; generator output
//! for a seed is unchanged.

use serde_json::{Number, Value};

use crate::Item;
use crate::slider::Slider;

/// One adjustable parameter of an [`Item`].
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    /// Dotted path from the state: `shaft.length`.
    pub path: String,
    /// The path without its last part: `shaft`.
    pub section: String,
    /// The last part of the path: `length`.
    pub name: String,
    pub kind: FieldKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FieldKind {
    /// An amount or a count, with the range and step its family allows.
    Slider { value: f32, range: Slider },
    /// One of a few named options (an enum's variants, in snake case).
    Choice { value: String, options: Vec<String> },
    Toggle { value: bool },
}

/// What a probe string must never be: an option.
const PROBE: &str = "\u{1}probe";

impl Item {
    /// Every slider, choice and toggle of the item, in declaration order.
    pub fn fields(&self) -> Vec<Field> {
        let (_, state) = self.tagged();
        let limits = self.limits_value();
        let mut fields = Vec::new();
        if let Value::Object(sections) = &state {
            for (key, value) in sections {
                if value.is_object() {
                    self.walk(&state, key, value, &limits, &mut fields);
                }
            }
        }
        fields
    }

    /// The sliders only, with their current values.
    pub fn sliders(&self) -> Vec<Field> {
        self.fields().into_iter().filter(|f| matches!(f.kind, FieldKind::Slider { .. })).collect()
    }

    /// The value of the slider at `path`, if there is one.
    pub fn slider(&self, path: &str) -> Option<f32> {
        let (_, state) = self.tagged();
        slider_at(&self.limits_value(), path)?;
        state_at(&state, path)?.as_f64().map(|v| v as f32)
    }

    /// Sets the slider at `path` to `value`, clamped to the family's range and snapped to its
    /// step. Returns the value the item now has.
    pub fn set_slider(&mut self, path: &str, value: f32) -> Result<f32, String> {
        let range = slider_at(&self.limits_value(), path).ok_or_else(|| format!("{path} is not a slider"))?;
        let snapped = snap_to_ends(&range, value);
        let (_, mut state) = self.tagged();
        let slot = state_at_mut(&mut state, path).ok_or_else(|| format!("no field {path}"))?;
        *slot = number_like(slot, snapped).ok_or_else(|| format!("{path} is not a number"))?;
        self.replace_state(state)?;
        Ok(self.slider(path).unwrap_or(snapped))
    }

    /// The option the choice at `path` holds, in snake case.
    pub fn choice(&self, path: &str) -> Option<String> {
        let (_, state) = self.tagged();
        state_at(&state, path)?.as_str().map(str::to_string)
    }

    /// Every option of the choice at `path`, in declaration order (empty if `path` is not an
    /// enum field).
    pub fn choice_options(&self, path: &str) -> Vec<String> {
        let (_, state) = self.tagged();
        self.options_at(&state, path)
    }

    /// Picks an option of the choice at `path` by name.
    pub fn set_choice(&mut self, path: &str, option: &str) -> Result<(), String> {
        let (_, mut state) = self.tagged();
        let slot = state_at_mut(&mut state, path).ok_or_else(|| format!("no field {path}"))?;
        if !slot.is_string() {
            return Err(format!("{path} is not a choice"));
        }
        *slot = Value::String(option.to_string());
        self.replace_state(state)
    }

    /// Moves the choice at `path` `by` options along, wrapping. Returns the new option.
    pub fn cycle_choice(&mut self, path: &str, by: isize) -> Result<String, String> {
        let options = self.choice_options(path);
        let current = self.choice(path).ok_or_else(|| format!("{path} is not a choice"))?;
        let at = options.iter().position(|o| *o == current).ok_or_else(|| format!("{current} is not an option of {path}"))?;
        let next = options[(at as isize + by).rem_euclid(options.len() as isize) as usize].clone();
        self.set_choice(path, &next)?;
        Ok(next)
    }

    /// The toggle at `path`.
    pub fn toggle(&self, path: &str) -> Option<bool> {
        let (_, state) = self.tagged();
        state_at(&state, path)?.as_bool()
    }

    pub fn set_toggle(&mut self, path: &str, on: bool) -> Result<(), String> {
        let (_, mut state) = self.tagged();
        let slot = state_at_mut(&mut state, path).ok_or_else(|| format!("no field {path}"))?;
        if !slot.is_boolean() {
            return Err(format!("{path} is not a toggle"));
        }
        *slot = Value::Bool(on);
        self.replace_state(state)
    }

    /// The item as `(variant name, state as JSON)`.
    fn tagged(&self) -> (String, Value) {
        match serde_json::to_value(self) {
            Ok(Value::Object(map)) => map.into_iter().next().unwrap_or_default(),
            _ => Default::default(),
        }
    }

    /// Puts a state back, or says why it would not read back and leaves the item alone.
    fn replace_state(&mut self, state: Value) -> Result<(), String> {
        let (variant, _) = self.tagged();
        *self = Self::from_tagged(&variant, state)?;
        Ok(())
    }

    fn from_tagged(variant: &str, state: Value) -> Result<Item, String> {
        let mut map = serde_json::Map::new();
        map.insert(variant.to_string(), state);
        serde_json::from_value(Value::Object(map)).map_err(|e| e.to_string())
    }

    fn walk(&self, state: &Value, prefix: &str, section: &Value, limits: &Value, out: &mut Vec<Field>) {
        let Value::Object(entries) = section else { return };
        for (name, value) in entries {
            let path = format!("{prefix}.{name}");
            let kind = match value {
                Value::Object(_) => {
                    self.walk(state, &path, value, limits, out);
                    continue;
                }
                Value::Number(n) => match slider_at(limits, &path) {
                    Some(range) => FieldKind::Slider { value: n.as_f64().unwrap_or_default() as f32, range },
                    None => continue,
                },
                Value::Bool(on) => FieldKind::Toggle { value: *on },
                Value::String(current) => {
                    let options = self.options_at(state, &path);
                    if options.is_empty() {
                        continue;
                    }
                    FieldKind::Choice { value: current.clone(), options }
                }
                _ => continue,
            };
            out.push(Field { path, section: prefix.to_string(), name: name.clone(), kind });
        }
    }

    /// Asks serde for the variants of the enum at `path` by offering it a name it must refuse:
    /// the error lists them ("unknown variant `x`, expected one of `a`, `b`").
    fn options_at(&self, state: &Value, path: &str) -> Vec<String> {
        let mut probe = state.clone();
        let Some(slot) = state_at_mut(&mut probe, path) else { return Vec::new() };
        if !slot.is_string() {
            return Vec::new();
        }
        *slot = Value::String(PROBE.to_string());
        let (variant, _) = self.tagged();
        match Self::from_tagged(&variant, probe) {
            Ok(_) => Vec::new(),
            Err(message) => variants_in(&message),
        }
    }
}

/// The back-quoted names after the first one (the refused one) in a serde variant error.
fn variants_in(message: &str) -> Vec<String> {
    if !message.contains("unknown variant") {
        return Vec::new();
    }
    message.split('`').skip(1).step_by(2).skip(1).map(str::to_string).collect()
}

fn state_at<'a>(state: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(state, |value, part| value.get(part))
}

fn state_at_mut<'a>(state: &'a mut Value, path: &str) -> Option<&'a mut Value> {
    path.split('.').try_fold(state, |value, part| value.get_mut(part))
}

/// The slider declared at `path` in a family's limits JSON.
fn slider_at(limits: &Value, path: &str) -> Option<Slider> {
    let leaf = state_at(limits, path)?;
    let get = |key: &str| leaf.get(key).and_then(Value::as_f64).map(|v| v as f32);
    Some(Slider { min: get("min")?, max: get("max")?, step: get("step")? })
}

/// [`Slider::snap`], but a value at or past either end lands exactly on it: snapping alone
/// stops a float error (`0.01 + 18 * 0.005` is not `0.1`) or a part of a step short.
fn snap_to_ends(range: &Slider, value: f32) -> f32 {
    if value >= range.max {
        range.max
    } else if value <= range.min || value.is_nan() {
        range.min
    } else {
        range.snap(value)
    }
}

/// `value` as the same kind of JSON number as `like`: whole for counts, fractional otherwise.
fn number_like(like: &Value, value: f32) -> Option<Value> {
    let Value::Number(n) = like else { return None };
    Some(Value::Number(if n.is_f64() { Number::from_f64(value as f64)? } else { Number::from(value.round().max(0.0) as u64) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Family;
    use crate::cond::Condition;

    fn items() -> impl Iterator<Item = Item> {
        Item::PORTED.iter().flat_map(|&family| [7, 1207].map(|seed| Item::roll(family, seed).unwrap()))
    }

    #[test]
    fn every_family_lists_sliders_by_section() {
        for family in Item::PORTED {
            let item = Item::roll(*family, 7).unwrap();
            let sliders = item.sliders();
            assert!(sliders.len() >= 8, "{family:?} lists only {} sliders", sliders.len());
            assert!(sliders.iter().any(|f| f.path == "cond.wear"), "{family:?} has no wear slider");
            for f in &sliders {
                let FieldKind::Slider { value, range } = &f.kind else { unreachable!() };
                assert!(range.min < range.max && range.step > 0.0, "{family:?} {}", f.path);
                assert!(range.contains(*value), "{family:?} {} = {value} outside {range:?}", f.path);
                assert_eq!(f.path, format!("{}.{}", f.section, f.name));
            }
        }
    }

    #[test]
    fn sections_keep_their_declared_order() {
        let item = Item::roll(Family::Lantern, 7).unwrap();
        let mut sections: Vec<String> = item.sliders().into_iter().map(|f| f.section).collect();
        sections.dedup();
        assert_eq!(sections, ["body", "bail", "flame", "cond"]);
        let names: Vec<String> = item.sliders().into_iter().filter(|f| f.section == "flame").map(|f| f.name).collect();
        assert_eq!(names, ["level", "flame"]);
    }

    #[test]
    fn lantern_flame_has_the_declared_range() {
        let item = Item::roll(Family::Lantern, 7).unwrap();
        let flame = item.sliders().into_iter().find(|f| f.path == "flame.flame").unwrap();
        assert_eq!(flame.kind, FieldKind::Slider { value: item.slider("flame.flame").unwrap(), range: Slider::new(0.6, 1.3, 0.01) });
    }

    #[test]
    fn set_then_get_round_trips_on_the_step() {
        for mut item in items() {
            for f in item.sliders() {
                let FieldKind::Slider { range, .. } = f.kind else { unreachable!() };
                let target = range.snap((range.min + range.max) * 0.5 + range.step * 3.0);
                let stored = item.set_slider(&f.path, target).unwrap();
                assert!((stored - target).abs() <= range.step * 0.01 + 1e-5, "{:?} {}: {target} became {stored}", item.family(), f.path);
                assert_eq!(item.slider(&f.path), Some(stored));
            }
        }
    }

    #[test]
    fn values_are_clamped_to_the_limits() {
        for mut item in items() {
            for f in item.sliders() {
                let FieldKind::Slider { range, .. } = f.kind else { unreachable!() };
                assert_eq!(item.set_slider(&f.path, range.min - 1000.0).unwrap(), range.min, "{:?} {}", item.family(), f.path);
                assert_eq!(item.set_slider(&f.path, range.max + 1000.0).unwrap(), range.max, "{:?} {}", item.family(), f.path);
                assert_eq!(item.set_slider(&f.path, f32::NEG_INFINITY).unwrap(), range.min);
            }
        }
    }

    #[test]
    fn counts_stay_whole() {
        let mut item = Item::roll(Family::Lantern, 7).unwrap();
        assert_eq!(item.set_slider("body.sides", 5.4).unwrap(), 5.0);
        let Item::Lantern(state) = &item else { unreachable!() };
        assert_eq!(state.body.sides, 5);
    }

    #[test]
    fn setting_a_slider_changes_only_that_value() {
        let before = Item::roll(Family::Lantern, 7).unwrap();
        let mut item = before.clone();
        item.set_slider("flame.flame", 1.25).unwrap();
        let (Item::Lantern(a), Item::Lantern(b)) = (&before, &item) else { unreachable!() };
        assert_eq!(b.flame.flame, 1.25);
        assert_eq!(b.seed, a.seed);
        assert_eq!(b.body, a.body);
        assert_ne!(a, b);
        // The seed still rolls the same item.
        assert_eq!(before, Item::roll(Family::Lantern, 7).unwrap());
    }

    #[test]
    fn unknown_or_unranged_paths_are_refused() {
        let mut item = Item::roll(Family::Lantern, 7).unwrap();
        assert!(item.set_slider("flame.nope", 1.0).is_err());
        assert!(item.set_slider("seed", 1.0).is_err());
        assert!(item.set_slider("body.style", 1.0).is_err());
        assert_eq!(item.slider("body.style"), None);
        assert!(item.set_choice("flame.flame", "x").is_err());
        assert!(item.set_choice("body.style", "no_such_style").is_err());
        assert!(item.set_toggle("flame.flame", true).is_err());
    }

    #[test]
    fn lantern_choices_list_their_variants() {
        let mut item = Item::roll(Family::Lantern, 7).unwrap();
        assert_eq!(item.choice_options("body.style"), ["horn", "pierced", "cage"]);
        assert_eq!(item.choice_options("flame.fuel"), ["candle", "oil"]);
        item.set_choice("flame.fuel", "candle").unwrap();
        assert_eq!(item.cycle_choice("flame.fuel", 1).unwrap(), "oil");
        assert_eq!(item.cycle_choice("flame.fuel", 1).unwrap(), "candle");
        assert_eq!(item.cycle_choice("flame.fuel", -1).unwrap(), "oil");
        assert_eq!(item.choice("flame.fuel").as_deref(), Some("oil"));
    }

    #[test]
    fn toggles_flip() {
        let mut item = Item::roll(Family::Lantern, 7).unwrap();
        item.set_toggle("flame.lit", false).unwrap();
        assert_eq!(item.toggle("flame.lit"), Some(false));
        item.set_toggle("flame.lit", true).unwrap();
        assert_eq!(item.toggle("flame.lit"), Some(true));
    }

    #[test]
    fn every_choice_cycles_through_all_its_options_and_back() {
        let mut choices = 0;
        for mut item in items() {
            for f in item.fields() {
                let FieldKind::Choice { value, options } = f.kind else { continue };
                choices += 1;
                assert!(options.contains(&value), "{:?} {}: {value} not in {options:?}", item.family(), f.path);
                let original = item.clone();
                for _ in 0..options.len() {
                    item.cycle_choice(&f.path, 1).unwrap();
                }
                assert_eq!(item, original, "{:?} {}", item.family(), f.path);
            }
        }
        assert!(choices > 100, "only {choices} choices found");
    }

    #[test]
    fn every_family_rebuilds_after_edits() {
        for family in Item::PORTED {
            let mut item = Item::roll(*family, 7).unwrap();
            for f in item.fields() {
                match f.kind {
                    FieldKind::Slider { range, .. } => {
                        item.set_slider(&f.path, range.max).unwrap();
                    }
                    FieldKind::Choice { options, .. } => {
                        item.set_choice(&f.path, options.last().unwrap()).unwrap();
                    }
                    FieldKind::Toggle { value } => item.set_toggle(&f.path, !value).unwrap(),
                }
            }
            let _ = item.build();
        }
    }

    #[test]
    fn a_condition_preset_replaces_the_wear() {
        let mut item = Item::roll(Family::Mace, 7).unwrap();
        item.set_condition(Condition::of(crate::cond::ConditionState::GraveFind));
        assert_eq!(item.slider("cond.wear"), Some(1.0));
        assert_eq!(item.condition().state, crate::cond::ConditionState::GraveFind);
    }

    #[test]
    fn variant_errors_parse() {
        assert_eq!(variants_in("unknown variant `x`, expected one of `a`, `b`, `c` at line 1"), ["a", "b", "c"]);
        assert_eq!(variants_in("unknown variant `x`, expected `a`"), ["a"]);
        assert_eq!(variants_in("unknown variant `x`, expected `a` or `b`"), ["a", "b"]);
        assert!(variants_in("invalid type").is_empty());
    }
}
