//! Dice in D&D notation ("1d6", "2d6"), as the plate writes weapon damage.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// `count` dice of `sides` sides each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Dice {
    pub count: u8,
    pub sides: u8,
}

impl Dice {
    pub const fn new(count: u8, sides: u8) -> Self {
        Dice { count, sides }
    }

    /// The average roll, `count * (sides + 1) / 2`.
    pub fn mean(self) -> f32 {
        self.count as f32 * (self.sides as f32 + 1.0) / 2.0
    }
}

impl FromStr for Dice {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (count, sides) = s.split_once('d').ok_or_else(|| format!("not dice: {s:?}"))?;
        let count = if count.is_empty() { 1 } else { count.parse().map_err(|_| format!("bad dice count in {s:?}"))? };
        let sides = sides.parse().map_err(|_| format!("bad dice sides in {s:?}"))?;
        if count == 0 || sides == 0 {
            return Err(format!("empty dice: {s:?}"));
        }
        Ok(Dice { count, sides })
    }
}

impl TryFrom<String> for Dice {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Dice> for String {
    fn from(d: Dice) -> String {
        d.to_string()
    }
}

impl fmt::Display for Dice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}d{}", self.count, self.sides)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints() {
        let d: Dice = "2d6".parse().unwrap();
        assert_eq!(d, Dice::new(2, 6));
        assert_eq!(d.to_string(), "2d6");
        assert_eq!("d8".parse::<Dice>().unwrap(), Dice::new(1, 8));
        assert_eq!(Dice::new(1, 6).mean(), 3.5);
        assert!("6".parse::<Dice>().is_err());
        assert!("0d6".parse::<Dice>().is_err());
    }
}
