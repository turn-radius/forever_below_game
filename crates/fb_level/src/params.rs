//! What shapes a level, beyond its seed: the grid's size, how many rooms and how big, how
//! corridors run and join, where lights and loot go. [`crate::generate`] uses the defaults,
//! which are the values the generator has always used, so a seed gives the same level it
//! always has; [`crate::generate_with`] takes others, for the level view's sliders.
//!
//! Each parameter has a slider range ([`LevelParams::fields`]), described with the Armory's
//! own [`Field`] so one slider panel serves items and levels alike. The ranges are our own,
//! wide enough to explore and narrow enough that every level still lays out.

use armory::reflect::{Field, FieldKind};
use armory::slider::Slider;
use serde::{Deserialize, Serialize};

use crate::LevelStyle;

/// Declares [`LevelParams`], its defaults and its sliders from one table:
/// `section: field: type = default, min, max, step; "what it is"`.
macro_rules! level_params {
    ($($section:ident { $($field:ident: $ty:ty = $default:expr, $min:expr, $max:expr, $step:expr; $doc:literal)* })*) => {
        /// The parameters of a level's layout and furnishing.
        #[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
        pub struct LevelParams {
            $($(#[doc = $doc] pub $field: $ty,)*)*
        }

        impl Default for LevelParams {
            /// The generator's own values: every seed lays out as it always has.
            fn default() -> Self {
                Self { $($($field: $default,)*)* }
            }
        }

        impl LevelParams {
            /// Every parameter, its section, name and slider range, in declaration order.
            pub const SLIDERS: &[(&str, &str, Slider)] = &[$($((stringify!($section), stringify!($field), Slider::new($min, $max, $step)),)*)*];

            fn get(&self, name: &str) -> Option<f32> {
                match name {
                    $($(stringify!($field) => Some(self.$field as f32),)*)*
                    _ => None,
                }
            }

            fn put(&mut self, name: &str, value: f32) -> bool {
                match name {
                    $($(stringify!($field) => { self.$field = value as $ty; true })*)*
                    _ => false,
                }
            }
        }
    };
}

level_params! {
    grid {
        width: u32 = 48, 32.0, 96.0, 1.0; "Cells east to west."
        height: u32 = 40, 28.0, 80.0, 1.0; "Cells north to south."
        ceiling: f32 = 30.0, 20.0, 60.0, 1.0; "Ceiling height, world units (3 m by default)."
    }
    rooms {
        rooms_min: u32 = 6, 2.0, 20.0, 1.0; "Fewest rooms a level has; a layout with fewer is tried again."
        rooms_max: u32 = 12, 2.0, 30.0, 1.0; "Most rooms a level has."
        side_min: u32 = 3, 2.0, 6.0, 1.0; "Shortest side of an ordinary room, in cells."
        side_max: u32 = 7, 3.0, 14.0, 1.0; "Longest side of an ordinary room, in cells."
        hall_chance: f64 = 0.2, 0.0, 1.0, 0.01; "Chance a room is a hall, grown on both sides."
        hall_grow_min: u32 = 3, 0.0, 8.0, 1.0; "Least a hall grows."
        hall_grow_max: u32 = 5, 0.0, 12.0, 1.0; "Most a hall grows."
        regular_chance: f64 = 0.0, 0.0, 1.0, 0.01; "Chance a room is made regular: both sides even, no longer than twice the other."
        room_gap: u32 = 3, 3.0, 8.0, 1.0; "Rock kept between rooms and the edge: three leaves a door each side and a corridor between."
        placement_tries: u32 = 400, 20.0, 2000.0, 10.0; "Rooms tried before settling for those placed."
        entrance_depth: u32 = 6, 1.0, 20.0, 1.0; "Deepest rock between the entrance edge and the first room."
    }
    corridors {
        turn_cost: u32 = 4, 0.0, 20.0, 1.0; "Extra cost for a corridor to turn: higher runs straighter."
        beside_cost: u32 = 6, 0.0, 20.0, 1.0; "Extra cost to run beside another corridor rather than share it."
        loop_chance: f64 = 0.3, 0.0, 1.0, 0.01; "Chance a room also links its nearest unlinked neighbour, making loops."
        door_reuse_chance: f64 = 0.6, 0.0, 1.0, 0.01; "Chance a corridor reuses a door already on that side of a room."
    }
    furnishing {
        brazier_min_area: u32 = 25, 4.0, 100.0, 1.0; "Rooms at least this many cells may hold a brazier."
        spot_spacing: f32 = 0.9, 0.3, 3.0, 0.05; "Closest two items or lights lie, in cells."
        bare_room_chance: f64 = 0.15, 0.0, 1.0, 0.01; "Chance a room holds no loot."
        cells_per_item: u32 = 10, 2.0, 40.0, 1.0; "Room cells for each item of loot."
        items_per_room_max: u32 = 5, 1.0, 12.0, 1.0; "Most loot in one room, before the odd extra."
    }
}

impl LevelParams {
    /// The defaults of `style`: rough rock's are [`LevelParams::default`], the values the
    /// generator has always used; stone halls lay out wider, more regular rooms, more halls,
    /// straighter corridors and a higher ceiling.
    pub fn for_style(style: LevelStyle) -> Self {
        match style {
            LevelStyle::RoughRock => Self::default(),
            LevelStyle::StoneHalls => Self {
                width: 64,
                height: 52,
                ceiling: 40.0,
                rooms_min: 5,
                rooms_max: 9,
                side_min: 4,
                side_max: 8,
                hall_chance: 0.45,
                hall_grow_min: 3,
                hall_grow_max: 5,
                regular_chance: 0.8,
                turn_cost: 12,
                loop_chance: 0.4,
                brazier_min_area: 30,
                ..Self::default()
            },
        }
    }

    /// Every parameter as a slider [`Field`], in declaration order.
    pub fn fields(&self) -> Vec<Field> {
        Self::SLIDERS
            .iter()
            .map(|&(section, name, range)| Field {
                path: format!("{section}.{name}"),
                section: section.to_string(),
                name: name.to_string(),
                kind: FieldKind::Slider { value: self.get(name).unwrap_or_default(), range },
            })
            .collect()
    }

    /// The parameter at `path` (`rooms.rooms_max`, or just `rooms_max`).
    pub fn slider(&self, path: &str) -> Option<f32> {
        self.get(path.rsplit('.').next()?)
    }

    /// Sets the parameter at `path`, snapped to its range and step, then keeps the pairs in
    /// order ([`LevelParams::fit`]). Returns the value it took.
    pub fn set_slider(&mut self, path: &str, value: f32) -> Result<f32, String> {
        let name = path.rsplit('.').next().unwrap_or(path);
        let (_, _, range) = Self::SLIDERS.iter().find(|(_, n, _)| *n == name).ok_or_else(|| format!("no level parameter {path}"))?;
        self.put(name, range.snap(value));
        self.fit();
        Ok(self.get(name).unwrap_or_default())
    }

    /// Keeps every parameter in its range and each pair in order: no fewer rooms than the
    /// least asked for, rooms and halls that fit the grid between its gaps.
    pub fn fit(&mut self) {
        // Only what is off its range or step is moved, so a value already fine keeps its exact
        // bits (a chance of 0.2 stays the f64 0.2, not the f32 0.2 widened).
        for &(_, name, range) in Self::SLIDERS {
            let v = self.get(name).unwrap_or_default();
            if (range.snap(v) - v).abs() > range.step * 1e-3 {
                self.put(name, range.snap(v));
            }
        }
        self.rooms_max = self.rooms_max.max(self.rooms_min);
        self.side_max = self.side_max.max(self.side_min);
        self.hall_grow_max = self.hall_grow_max.max(self.hall_grow_min);
        // The biggest room, a hall at its largest, still fits between the gaps.
        let room_room = self.width.min(self.height).saturating_sub(2 * self.room_gap + 1).max(2);
        self.side_max = self.side_max.min(room_room);
        self.side_min = self.side_min.min(self.side_max);
        self.hall_grow_max = self.hall_grow_max.min(room_room - self.side_max);
        self.hall_grow_min = self.hall_grow_min.min(self.hall_grow_max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_sits_in_its_range() {
        let params = LevelParams::default();
        for field in params.fields() {
            let FieldKind::Slider { value, range } = field.kind else { panic!("{}", field.path) };
            assert!(range.contains(value), "{} = {value} outside {}..{}", field.path, range.min, range.max);
            assert!((range.snap(value) - value).abs() < range.step * 1e-3, "{} off its step", field.path);
        }
        let mut fitted = params;
        fitted.fit();
        assert_eq!(fitted, params, "the defaults already fit");
    }

    #[test]
    fn a_slider_snaps_clamps_and_keeps_pairs_in_order() {
        let mut params = LevelParams::default();
        assert_eq!(params.set_slider("rooms.rooms_max", 99.0), Ok(30.0));
        assert!((params.set_slider("hall_chance", 0.333).unwrap() - 0.33).abs() < 1e-5);
        assert_eq!(params.slider("corridors.loop_chance"), Some(0.3));
        // Raising the least past the most raises the most with it.
        params.set_slider("rooms_min", 18.0).unwrap();
        params.set_slider("rooms_max", 10.0).unwrap();
        assert!(params.rooms_max >= params.rooms_min);
        assert!(params.set_slider("rooms.nonsense", 1.0).is_err());
    }
}
