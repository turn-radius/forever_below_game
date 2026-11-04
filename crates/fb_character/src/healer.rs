//! What a healer charges to tend the delver. All our own, a first guess to play against
//! (`[healer]` in `data/rules.toml`).
//!
//! - Restoring hit points costs [`HealerRules::per_hit_point`] copper for each one missing, so
//!   the price grows with the hurt.
//! - Healing a permanent wound ([`crate::revival::Wound`]) costs [`HealerRules::wound`] copper, and
//!   gives back one revive ([`crate::revival::RevivalRules::give_back`]).
//! - Curing the poison costs [`HealerRules::cure_poison`] copper. Disease is not in the
//!   simulation yet; when it is, it gets a price here.
//! - Every price is then scaled by how the healer's faction regards the delver: a Friendly
//!   faction charges [`HealerRules::friendly`] of it and an Allied one [`HealerRules::allied`].
//!   A Hostile or Wary faction's healer does not tend at all, so no price is asked of them.

use serde::{Deserialize, Serialize};

/// How a healer's faction regards the delver, as far as the price goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Regard {
    Indifferent,
    Friendly,
    Allied,
}

/// The healer's prices (`[healer]` in `data/rules.toml`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HealerRules {
    /// Copper for each hit point missing.
    pub per_hit_point: f64,
    /// Copper to cure the delver's poisoning.
    pub cure_poison: f64,
    /// Copper to heal one permanent wound.
    pub wound: f64,
    /// Share of the price a Friendly faction's healer charges.
    pub friendly: f64,
    /// Share of the price an Allied faction's healer charges.
    pub allied: f64,
}

impl Default for HealerRules {
    fn default() -> Self {
        HealerRules { per_hit_point: 5.0, cure_poison: 50.0, wound: 500.0, friendly: 0.75, allied: 0.5 }
    }
}

impl HealerRules {
    /// The share of a price this regard pays.
    pub fn share(&self, regard: Regard) -> f64 {
        match regard {
            Regard::Indifferent => 1.0,
            Regard::Friendly => self.friendly,
            Regard::Allied => self.allied,
        }
        .max(0.0)
    }

    /// `base` copper at this regard, rounded up, never less than 1.
    pub fn price(&self, base: f64, regard: Regard) -> u32 {
        ((base.max(0.0) * self.share(regard)).ceil() as u32).max(1)
    }

    /// The price of restoring `missing` hit points.
    pub fn hit_points(&self, missing: i32, regard: Regard) -> u32 {
        self.price(self.per_hit_point * missing.max(0) as f64, regard)
    }

    /// The price of curing the poison.
    pub fn poison(&self, regard: Regard) -> u32 {
        self.price(self.cure_poison, regard)
    }

    /// The price of healing one permanent wound.
    pub fn wound(&self, regard: Regard) -> u32 {
        self.price(self.wound, regard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_price_grows_with_the_hurt() {
        let rules = HealerRules::default();
        assert!(rules.hit_points(10, Regard::Indifferent) > rules.hit_points(4, Regard::Indifferent));
        assert_eq!(rules.hit_points(6, Regard::Indifferent), 30);
    }

    #[test]
    fn a_friendly_healer_charges_less_than_an_indifferent_one() {
        let rules = HealerRules::default();
        assert!(rules.hit_points(8, Regard::Friendly) < rules.hit_points(8, Regard::Indifferent));
        assert!(rules.poison(Regard::Allied) < rules.poison(Regard::Friendly));
        assert!(rules.poison(Regard::Friendly) < rules.poison(Regard::Indifferent));
    }

    #[test]
    fn a_wound_costs_more_than_a_cure_and_a_friend_pays_less() {
        let rules = HealerRules::default();
        assert!(rules.wound(Regard::Indifferent) > rules.poison(Regard::Indifferent));
        assert!(rules.wound(Regard::Friendly) < rules.wound(Regard::Indifferent));
    }

    #[test]
    fn a_price_is_never_nothing() {
        let rules = HealerRules { per_hit_point: 0.0, cure_poison: 0.0, ..Default::default() };
        assert_eq!(rules.hit_points(3, Regard::Allied), 1);
    }
}
