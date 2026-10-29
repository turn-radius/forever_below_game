//! Taking an item apart into enchanting resources: essence from any gear, by rarity, and special
//! materials from rare and legendary weapons. An enchanter spends them (`docs/items.md`,
//! "Scrapping"). The numbers are in `data/scrap.json` and are our own.

use crate::item::Item;
use crate::{Kind, Rarity, Tag};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// Where a material comes from.
#[derive(Deserialize, Debug, Default)]
#[serde(default)]
struct Source {
    /// A weapon of this kind (blade, blunt or ranged).
    tag: Option<Tag>,
    /// A weapon with a good affix of this group.
    group: Option<String>,
    /// A weapon with a good affix whose group starts with this.
    group_prefix: Option<String>,
    /// Every legendary weapon.
    legendary: bool,
}

/// One special material in `data/scrap.json`.
#[derive(Deserialize, Debug)]
pub struct MaterialKind {
    pub id: String,
    pub name: String,
    from: Source,
}

#[derive(Deserialize)]
struct Table {
    essence: BTreeMap<Rarity, u32>,
    per_good_affix: BTreeMap<Rarity, u32>,
    flaw_keeps: f64,
    materials: Vec<MaterialKind>,
}

static TABLE: LazyLock<Table> = LazyLock::new(|| serde_json::from_str(include_str!("../../../data/scrap.json")).unwrap_or_else(|e| panic!("data/scrap.json: {e}")));

/// Every special material, in table order.
pub fn material_kinds() -> &'static [MaterialKind] {
    &TABLE.materials
}

/// The name of material `id`, or the id itself if the table does not know it.
pub fn material_name(id: &str) -> &str {
    TABLE.materials.iter().find(|m| m.id == id).map_or(id, |m| m.name.as_str())
}

/// Some of one special material.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Material {
    /// The id in `data/scrap.json`.
    pub id: String,
    pub count: u32,
}

/// What scrapping an item gives.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Salvage {
    pub essence: u32,
    pub materials: Vec<Material>,
}

/// What `item` is scrapped into; `None` for what is not gear (food, purses, containers, lights
/// and the like). A container's contents are the game's to check: it is scrapped only empty.
/// Rarity gives the essence, each good affix adds by its tier and each flaw halves it,
/// so a flawed item gives less. Only rare and legendary weapons give materials, one for the
/// kind of weapon and one for each kind of good affix it carries, and a legendary a heartstone
/// besides. A pure function of the item.
pub fn scrap(item: &Item) -> Option<Salvage> {
    if item.profile.kind != Kind::Gear || item.coins.is_some() || item.profile.tags.iter().any(|t| matches!(t, Tag::Light | Tag::Container | Tag::Food)) {
        return None;
    }
    let t = &*TABLE;
    let mut essence = f64::from(t.essence[&item.rarity]);
    for a in item.affixes.iter().filter(|a| a.good && !a.holy) {
        essence += f64::from(t.per_good_affix[&a.least]);
    }
    for _ in item.affixes.iter().filter(|a| !a.good) {
        essence *= t.flaw_keeps;
    }
    let mut materials = Vec::new();
    if item.profile.tags.contains(&Tag::Weapon) && item.rarity >= Rarity::Rare {
        let good_groups: Vec<&str> = item.affixes.iter().filter(|a| a.good && !a.holy).filter_map(|a| a.group.as_deref()).collect();
        for m in &t.materials {
            let has = m.from.tag.is_some_and(|tag| item.profile.tags.contains(&tag))
                || m.from.group.as_deref().is_some_and(|g| good_groups.contains(&g))
                || m.from.group_prefix.as_deref().is_some_and(|p| good_groups.iter().any(|g| g.starts_with(p)))
                || (m.from.legendary && item.rarity == Rarity::Legendary);
            if has {
                materials.push(Material { id: m.id.clone(), count: 1 });
            }
        }
    }
    // A stack gives every piece's salvage.
    let n = item.quantity.max(1);
    for m in &mut materials {
        m.count *= n;
    }
    Some(Salvage { essence: (essence.round() as u32).max(1) * n, materials })
}

/// A delver's enchanting resources for one delve: essence and materials, kept by id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnchantingStock {
    pub essence: u32,
    pub materials: BTreeMap<String, u32>,
}

impl EnchantingStock {
    /// Adds what a scrapped item gave.
    pub fn add(&mut self, salvage: &Salvage) {
        self.essence = self.essence.saturating_add(salvage.essence);
        for m in &salvage.materials {
            let n = self.materials.entry(m.id.clone()).or_insert(0);
            *n = n.saturating_add(m.count);
        }
    }

    /// How many of material `id` are held.
    pub fn count(&self, id: &str) -> u32 {
        self.materials.get(id).copied().unwrap_or(0)
    }

    /// Whether `essence` and every one of `materials` are held.
    pub fn can_spend(&self, essence: u32, materials: &[Material]) -> bool {
        self.essence >= essence && materials.iter().all(|m| self.count(&m.id) >= m.count)
    }

    /// Takes `essence` and `materials` out, or takes nothing and returns `false` if any is short.
    pub fn spend(&mut self, essence: u32, materials: &[Material]) -> bool {
        if !self.can_spend(essence, materials) {
            return false;
        }
        self.essence -= essence;
        for m in materials {
            if let Some(n) = self.materials.get_mut(&m.id) {
                *n -= m.count;
                if *n == 0 {
                    self.materials.remove(&m.id);
                }
            }
        }
        true
    }

    /// Nothing held.
    pub fn is_empty(&self) -> bool {
        self.essence == 0 && self.materials.is_empty()
    }

    /// One line for a sheet: "Essence 38 · Gravesteel sliver ×1 · Heartstone ×1".
    pub fn line(&self) -> String {
        let mut parts = vec![format!("Essence {}", self.essence)];
        parts.extend(self.materials.iter().map(|(id, n)| format!("{} ×{n}", material_name(id))));
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Odds, roll_item, roll_item_with};

    /// A sword of `rarity` with only good affixes.
    fn sword_of(rarity: Rarity) -> Item {
        (0..6_000).filter_map(|s| roll_item_with("Sword", s, Odds::loot(8))).find(|i| i.rarity == rarity && i.affixes.iter().all(|a| a.good)).unwrap()
    }

    fn plain_cloak() -> Item {
        (0..200).filter_map(|s| roll_item("Cloak", s)).find(|i| i.rarity == Rarity::Common && i.affixes.is_empty()).unwrap()
    }

    #[test]
    fn every_material_is_named_and_unique() {
        let ids: std::collections::BTreeSet<_> = material_kinds().iter().map(|m| &m.id).collect();
        assert_eq!(ids.len(), material_kinds().len());
        assert_eq!(material_name("heartstone"), "Heartstone");
    }

    #[test]
    fn a_common_cloak_is_a_little_essence() {
        assert_eq!(scrap(&plain_cloak()), Some(Salvage { essence: 2, materials: vec![] }));
    }

    #[test]
    fn a_stack_scraps_into_every_pieces_salvage() {
        let one = sword_of(Rarity::Rare);
        let (single, three) = (scrap(&one).unwrap(), scrap(&one.clone().with_quantity(3)).unwrap());
        assert_eq!(three.essence, single.essence * 3);
        assert_eq!(three.materials, single.materials.iter().map(|m| Material { id: m.id.clone(), count: m.count * 3 }).collect::<Vec<_>>());
    }

    #[test]
    fn goods_are_not_scrapped() {
        for base in ["Purse", "Torch", "Satchel"] {
            assert_eq!(scrap(&roll_item(base, 4).unwrap()), None, "{base}");
        }
    }

    #[test]
    fn a_rare_sword_gives_essence_and_a_material() {
        let s = scrap(&sword_of(Rarity::Rare)).unwrap();
        assert!(s.essence >= 30, "{s:?}");
        assert!(s.materials.iter().any(|m| m.id == "gravesteel"), "{s:?}");
    }

    #[test]
    fn a_legendary_gives_the_most_and_a_heartstone() {
        let (r, l) = (scrap(&sword_of(Rarity::Rare)).unwrap(), scrap(&sword_of(Rarity::Legendary)).unwrap());
        assert!(l.essence > r.essence, "{l:?} {r:?}");
        assert!(l.materials.iter().any(|m| m.id == "heartstone"), "{l:?}");
        assert!(!r.materials.iter().any(|m| m.id == "heartstone"));
    }

    #[test]
    fn a_flawed_item_gives_less() {
        let mut cloak = plain_cloak();
        let before = scrap(&cloak).unwrap().essence;
        let flaw = crate::tables::affixes().iter().find(|a| !a.good).unwrap();
        cloak.affixes.push(flaw);
        let after = scrap(&cloak).unwrap().essence;
        assert!(after < before, "{after} {before}");
        let mut sword = sword_of(Rarity::Rare);
        let before = scrap(&sword).unwrap().essence;
        sword.affixes.push(flaw);
        assert!(scrap(&sword).unwrap().essence < before);
    }

    #[test]
    fn spending_more_than_held_fails_and_takes_nothing() {
        let mut stock = EnchantingStock::default();
        stock.add(&Salvage { essence: 10, materials: vec![Material { id: "gravesteel".into(), count: 1 }] });
        let heart = [Material { id: "heartstone".into(), count: 1 }];
        assert!(!stock.spend(5, &heart));
        assert!(!stock.spend(11, &[]));
        assert_eq!(stock.essence, 10);
        assert!(stock.spend(4, &[Material { id: "gravesteel".into(), count: 1 }]));
        assert_eq!((stock.essence, stock.count("gravesteel")), (6, 0));
    }

    #[test]
    fn the_stock_round_trips() {
        let mut stock = EnchantingStock::default();
        stock.add(&Salvage { essence: 38, materials: vec![Material { id: "gravesteel".into(), count: 2 }] });
        let back: EnchantingStock = serde_json::from_str(&serde_json::to_string(&stock).unwrap()).unwrap();
        assert_eq!(back, stock);
        assert_eq!(back.line(), "Essence 38 · Gravesteel sliver ×2");
    }
}
