//! Lighting and putting out what burns: a torch goes from fresh or doused to burning and back
//! to doused, a lantern from unlit to lit and back. A spent torch has nothing left to light.
//! Whether a light is lit is part of its state, so it is kept wherever the item goes, and
//! decides whether it gives light ([`Item`]'s family `light`).

use crate::families::torch::BurnState;
use crate::item::Item;

/// Why a light would not light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unlit {
    /// A torch burnt down to a stub.
    Spent,
    /// Not a torch or a lantern.
    NotALight,
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
    fn only_lights_light() {
        let mut sword = Item::roll(crate::Family::Sword, 1).unwrap();
        assert_eq!(sword.lit(), None);
        assert_eq!(sword.toggle_lit(), Err(Unlit::NotALight));
    }
}
