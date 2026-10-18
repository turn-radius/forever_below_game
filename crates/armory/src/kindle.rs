//! Lighting and putting out what burns: a torch goes from fresh or doused to burning and back
//! to doused, a lantern from unlit to lit and back. A spent torch has nothing left to light.
//! Whether a light is lit is part of its state, so it is kept wherever the item goes, and
//! decides whether it gives light ([`Item`]'s family `light`).

use crate::families::torch::BurnState;
use crate::item::Item;

/// How long a full torch burns, in game seconds: one hour.
pub const TORCH_SECONDS: f32 = 3_600.0;
/// How long a full lantern burns, in game seconds: six hours of oil (SRD p. 68).
pub const LANTERN_SECONDS: f32 = 21_600.0;

/// Why a light would not light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unlit {
    /// A torch burnt down to a stub.
    Spent,
    /// Not a torch or a lantern.
    NotALight,
    /// A lantern with no fuel left, until it is refuelled.
    Empty,
}

impl Item {
    /// Whether this torch or lantern is lit; `None` for anything else, which cannot be lit or
    /// put out.
    pub fn lit(&self) -> Option<bool> {
        match self {
            Item::Torch(s) => Some(s.burn.state == BurnState::Burning),
            Item::Lantern(s) => Some(s.flame.lit),
            _ => None,
        }
    }

    /// Burning time left in game seconds, as the item holds it; `None` for anything that is not
    /// a torch or a lantern. The time a full one holds is [`Item::fuel_capacity`], times the
    /// item's `BurnTime`.
    pub fn fuel_left(&self) -> Option<f32> {
        match self {
            Item::Torch(s) => Some(s.burn.fuel_left),
            Item::Lantern(s) => Some(s.flame.fuel_left),
            _ => None,
        }
    }

    /// Game seconds a full torch or lantern burns, before the item's `BurnTime` is applied.
    pub fn fuel_capacity(&self) -> Option<f32> {
        match self {
            Item::Torch(_) => Some(TORCH_SECONDS),
            Item::Lantern(_) => Some(LANTERN_SECONDS),
            _ => None,
        }
    }

    /// Scales the fuel left, for the item's `BurnTime`.
    pub fn scale_fuel(&mut self, by: f32) {
        match self {
            Item::Torch(s) => s.burn.fuel_left *= by,
            Item::Lantern(s) => s.flame.fuel_left *= by,
            _ => {}
        }
    }

    /// Burns `seconds` of game time off a lit torch or lantern whose full fuel is `capacity`
    /// seconds; an unlit light, or anything else, is left alone. The torch's char and the
    /// lantern's level follow the fuel (the level keeps the slider's floor of 0.1 while lit, so an almost
    /// empty font still shows a little oil). A torch with none left is spent, and a lantern goes out.
    pub fn burn(&mut self, seconds: f32, capacity: f32) {
        if seconds <= 0.0 || self.lit() != Some(true) {
            return;
        }
        match self {
            Item::Torch(s) => {
                let b = &mut s.burn;
                b.fuel_left = (b.fuel_left - seconds).max(0.0);
                if b.fuel_left <= 0.0 {
                    b.state = BurnState::Spent;
                    b.char = 1.0;
                } else {
                    b.char = b.char.max(1.0 - b.fuel_left / capacity).min(1.0);
                }
            }
            Item::Lantern(s) => {
                let f = &mut s.flame;
                f.fuel_left = (f.fuel_left - seconds).max(0.0);
                f.level = (f.fuel_left / capacity).clamp(0.1, 1.0);
                if f.fuel_left <= 0.0 {
                    f.lit = false;
                }
            }
            _ => {}
        }
    }

    /// Pours up to `seconds` of burning into a lantern whose full fuel is `capacity` seconds,
    /// never past full, and returns the seconds it took. The level follows the fuel, as in
    /// [`Item::burn`]. Whether it is lit is left alone. Anything that is not a lantern takes
    /// nothing: a torch is not refilled.
    pub fn refuel(&mut self, seconds: f32, capacity: f32) -> f32 {
        let Item::Lantern(s) = self else { return 0.0 };
        let f = &mut s.flame;
        let taken = seconds.min(capacity - f.fuel_left).max(0.0);
        if taken > 0.0 {
            f.fuel_left += taken;
            f.level = (f.fuel_left / capacity).clamp(0.1, 1.0);
        }
        taken
    }

    /// Lights it, or puts it out. Putting out a burning torch leaves it doused, charred and
    /// cold; a fresh or doused one lights. A spent torch will not light, and putting out what is
    /// already out changes nothing.
    pub fn set_lit(&mut self, lit: bool) -> Result<(), Unlit> {
        match self {
            Item::Torch(s) => {
                s.burn.state = match (s.burn.state, lit) {
                    (BurnState::Spent, true) => return Err(Unlit::Spent),
                    (BurnState::Burning, false) => BurnState::Doused,
                    (_, true) => BurnState::Burning,
                    (state, false) => state,
                };
                Ok(())
            }
            Item::Lantern(s) => {
                if lit && s.flame.fuel_left <= 0.0 {
                    return Err(Unlit::Empty);
                }
                s.flame.lit = lit;
                Ok(())
            }
            _ => Err(Unlit::NotALight),
        }
    }

    /// Lights it if it is out, or puts it out if it is lit. Returns whether it is lit now.
    pub fn toggle_lit(&mut self) -> Result<bool, Unlit> {
        let lit = !self.lit().ok_or(Unlit::NotALight)?;
        self.set_lit(lit)?;
        Ok(lit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::Generator;
    use crate::families::lantern::Lantern;
    use crate::families::torch::{Torch, TorchStyle};

    fn torch(state: BurnState) -> Item {
        let mut s = Torch::roll_as(5, TorchStyle::Rag);
        s.burn.state = state;
        Item::Torch(s)
    }

    #[test]
    fn a_torch_lights_and_goes_out_doused() {
        for from in [BurnState::Fresh, BurnState::Doused] {
            let mut t = torch(from);
            assert_eq!(t.lit(), Some(false));
            assert_eq!(t.toggle_lit(), Ok(true));
            if let Item::Torch(s) = &t {
                assert!(Torch::light(s).is_some(), "burning, it gives light");
            }
            assert_eq!(t.toggle_lit(), Ok(false));
            assert!(matches!(&t, Item::Torch(s) if s.burn.state == BurnState::Doused));
        }
    }

    #[test]
    fn a_spent_torch_will_not_light() {
        let mut t = torch(BurnState::Spent);
        assert_eq!(t.toggle_lit(), Err(Unlit::Spent));
        assert_eq!(t.lit(), Some(false));
        assert_eq!(t.set_lit(false), Ok(()));
    }

    #[test]
    fn a_lantern_lights_and_goes_out() {
        let mut l = Item::Lantern(Lantern::roll(3));
        let was = l.lit().unwrap();
        assert_eq!(l.toggle_lit(), Ok(!was));
        assert_eq!(l.toggle_lit(), Ok(was));
    }

    #[test]
    fn burning_takes_fuel_off_a_lit_light_only() {
        let mut t = torch(BurnState::Burning);
        if let Item::Torch(s) = &mut t {
            s.burn.fuel_left = TORCH_SECONDS;
        }
        let full = t.fuel_left().unwrap();
        t.burn(600.0, TORCH_SECONDS);
        assert_eq!(t.fuel_left(), Some(full - 600.0));
        t.set_lit(false).unwrap();
        t.burn(600.0, TORCH_SECONDS);
        assert_eq!(t.fuel_left(), Some(full - 600.0), "out, it keeps what is left");
        t.set_lit(true).unwrap();
        t.burn(1.0e6, TORCH_SECONDS);
        assert!(matches!(&t, Item::Torch(s) if s.burn.state == BurnState::Spent && s.burn.char == 1.0));
        assert_eq!(t.fuel_left(), Some(0.0));
    }

    #[test]
    fn the_char_follows_the_fuel() {
        let mut t = torch(BurnState::Fresh);
        if let Item::Torch(s) = &mut t {
            s.burn.char = 0.0;
            s.burn.fuel_left = TORCH_SECONDS;
        }
        t.set_lit(true).unwrap();
        t.burn(TORCH_SECONDS / 2.0, TORCH_SECONDS);
        assert!(matches!(&t, Item::Torch(s) if (s.burn.char - 0.5).abs() < 1e-4));
    }

    #[test]
    fn an_empty_lantern_goes_out_and_will_not_light() {
        let mut l = Item::Lantern(Lantern::roll(3));
        l.set_lit(true).unwrap();
        l.burn(LANTERN_SECONDS * 2.0, LANTERN_SECONDS);
        assert_eq!(l.lit(), Some(false));
        assert_eq!(l.fuel_left(), Some(0.0));
        assert_eq!(l.set_lit(true), Err(Unlit::Empty));
    }

    #[test]
    fn a_lantern_refuels_up_to_full_and_a_torch_not_at_all() {
        let mut l = Item::Lantern(Lantern::roll(3));
        l.set_lit(true).unwrap();
        l.burn(LANTERN_SECONDS * 2.0, LANTERN_SECONDS);
        assert_eq!(l.refuel(LANTERN_SECONDS / 4.0, LANTERN_SECONDS), LANTERN_SECONDS / 4.0);
        assert_eq!(l.fuel_left(), Some(LANTERN_SECONDS / 4.0));
        assert!(matches!(&l, Item::Lantern(s) if (s.flame.level - 0.25).abs() < 1e-6));
        assert_eq!(l.refuel(LANTERN_SECONDS, LANTERN_SECONDS), LANTERN_SECONDS * 0.75, "only what it has room for");
        assert_eq!(l.refuel(1.0, LANTERN_SECONDS), 0.0, "full");
        assert_eq!(l.lit(), Some(false), "refilling does not light it");
        assert_eq!(l.set_lit(true), Ok(()));
        let mut t = torch(BurnState::Spent);
        assert_eq!(t.refuel(600.0, TORCH_SECONDS), 0.0);
    }

    #[test]
    fn only_lights_light() {
        let mut sword = Item::roll(crate::Family::Sword, 1).unwrap();
        assert_eq!(sword.lit(), None);
        assert_eq!(sword.toggle_lit(), Err(Unlit::NotALight));
    }
}
