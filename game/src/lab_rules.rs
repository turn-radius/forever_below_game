//! The rules the lab edits: [`Rules`] (`data/rules.toml`) and the affix strengths
//! (`data/afx.json`), held as a [`RuleSet`]. The lab keeps two, `saved` (as on disk) and the
//! edited one in [`crate::lab_view::LabState`], and shows the difference everywhere ([`was_text`]).
//!
//! Every number a designer can change is a [`Leaf`] named by a path (`load/capacity_per_strength`,
//! `creatures/Giant Rat/bite`, `affixes/Warm/0`), found by walking the rules as a TOML table, so a
//! number added to [`Rules`] shows up here with a label made from its name until it is given
//! a better one in [`LABELS`]. [`write_rules_toml`] and [`write_afx_json`] put the edited numbers
//! into the files' text and leave everything else, the comments above all, as it was.

use std::collections::BTreeMap;

use fb_character::explain::number;
use fb_character::rules::Rules;
use fb_items::modifier::Modifier;
use fb_items::tables::{Affix, affixes};
use toml::{Table, Value};

/// How strong each affix is: the `by` of each of its effects that has one, in order, by name.
pub type Strengths = BTreeMap<String, Vec<f32>>;

/// The rules and affix strengths, which the lab edits and saves together.
#[derive(Clone, Debug, PartialEq)]
pub struct RuleSet {
    pub rules: Rules,
    pub strengths: Strengths,
}

impl Default for RuleSet {
    fn default() -> Self {
        RuleSet { rules: Rules::default(), strengths: default_strengths() }
    }
}

/// The `by` of an effect, if it has one.
pub fn strength_of(m: &Modifier) -> Option<f32> {
    match *m {
        Modifier::Add { by, .. } | Modifier::Scale { by, .. } | Modifier::DamageOrArmour { by } | Modifier::Ward { by, .. } => Some(by),
        Modifier::Shun { chance, .. } => Some(chance),
        Modifier::Condition { .. } | Modifier::Bane { .. } => None,
    }
}

fn with_strength(m: Modifier, by: f32) -> Modifier {
    match m {
        Modifier::Add { stat, .. } => Modifier::Add { stat, by },
        Modifier::Scale { stat, .. } => Modifier::Scale { stat, by },
        Modifier::DamageOrArmour { .. } => Modifier::DamageOrArmour { by },
        Modifier::Ward { foe, .. } => Modifier::Ward { foe, by },
        Modifier::Shun { foe, .. } => Modifier::Shun { foe, chance: by },
        other => other,
    }
}

/// What `affix` has in `data/afx.json`.
pub fn default_strength(affix: &Affix) -> Vec<f32> {
    affix.fx.iter().filter_map(strength_of).collect()
}

/// The strengths `data/afx.json` has.
pub fn default_strengths() -> Strengths {
    affixes().iter().map(|a| (a.name.clone(), a.fx.iter().filter_map(strength_of).collect())).collect()
}

/// `affix` with the strengths of `strengths` for its name, or its own where there are none.
pub fn strengthened(affix: &Affix, strengths: &Strengths) -> Affix {
    let Some(by) = strengths.get(&affix.name) else { return affix.clone() };
    let mut by = by.iter().copied();
    let mut out = affix.clone();
    for m in &mut out.fx {
        if strength_of(m).is_some()
            && let Some(by) = by.next()
        {
            *m = with_strength(*m, by);
        }
    }
    out
}

/// One editable number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Leaf {
    Int(i64),
    Float(f64),
    Bool(bool),
    /// Dice: a count, the sides and a bonus.
    Dice([i64; 3]),
}

impl Leaf {
    /// The number as the lab writes it: `10`, `0.5`, `on`, `1d4+2`.
    pub fn text(&self) -> String {
        match *self {
            Leaf::Int(n) => n.to_string(),
            Leaf::Float(x) => number(x),
            Leaf::Bool(on) => if on { "on" } else { "off" }.to_string(),
            Leaf::Dice([count, sides, bonus]) => match (count, bonus) {
                (0, bonus) => bonus.to_string(),
                (_, 0) => format!("{count}d{sides}"),
                (_, bonus) => format!("{count}d{sides}{bonus:+}"),
            },
        }
    }

    fn of(value: &Value) -> Option<Leaf> {
        match value {
            Value::Integer(n) => Some(Leaf::Int(*n)),
            Value::Float(x) => Some(Leaf::Float(*x)),
            Value::Boolean(b) => Some(Leaf::Bool(*b)),
            Value::Table(t) if t.len() == 3 => {
                let part = |k: &str| t.get(k).and_then(Value::as_integer);
                Some(Leaf::Dice([part("count")?, part("sides")?, part("bonus")?]))
            }
            _ => None,
        }
    }

    fn value(&self) -> Value {
        match *self {
            Leaf::Int(n) => Value::Integer(n),
            Leaf::Float(x) => Value::Float(x),
            Leaf::Bool(b) => Value::Boolean(b),
            Leaf::Dice([count, sides, bonus]) => {
                let mut t = Table::new();
                t.insert("count".into(), Value::Integer(count));
                t.insert("sides".into(), Value::Integer(sides));
                t.insert("bonus".into(), Value::Integer(bonus));
                Value::Table(t)
            }
        }
    }
}

/// The path of a leaf: its keys joined with `/`.
pub fn join(path: &[String]) -> String {
    path.join("/")
}

fn walk(path: &mut Vec<String>, value: &Value, out: &mut Vec<(String, Leaf)>) {
    if let Some(leaf) = Leaf::of(value) {
        out.push((join(path), leaf));
    } else if let Value::Table(table) = value {
        for (key, inner) in table {
            path.push(key.clone());
            walk(path, inner, out);
            path.pop();
        }
    } else if let Value::Array(items) = value {
        // An effect's stages and influences: each by its place.
        for (i, inner) in items.iter().enumerate() {
            path.push(i.to_string());
            walk(path, inner, out);
            path.pop();
        }
    }
}

/// The rules as the file has them: the effects only when they differ from `data/effects.json`.
fn tree(rules: &Rules) -> Table {
    Table::try_from(rules).expect("the rules serialise")
}

/// The rules as the lab edits them: the effects always.
fn lab_tree(rules: &Rules) -> Table {
    let mut root = tree(rules);
    root.insert("effects".into(), Value::try_from(&rules.effects).expect("the effects serialise"));
    root
}

/// Every editable number of the rules, then of the affixes, with its path.
pub fn leaves(set: &RuleSet) -> Vec<(String, Leaf)> {
    let mut out = Vec::new();
    walk(&mut Vec::new(), &Value::Table(lab_tree(&set.rules)), &mut out);
    for affix in affixes() {
        for (i, by) in set.strengths.get(&affix.name).into_iter().flatten().enumerate() {
            out.push((format!("affixes/{}/{i}", affix.name), Leaf::Float(*by as f64)));
        }
    }
    out
}

/// The leaf at `path`.
pub fn leaf(set: &RuleSet, path: &str) -> Option<Leaf> {
    leaves(set).into_iter().find(|(p, _)| p == path).map(|(_, l)| l)
}

/// Puts `to` at `path`.
pub fn set_leaf(set: &mut RuleSet, path: &str, to: Leaf) {
    let keys: Vec<&str> = path.split('/').collect();
    if let ["affixes", name, i] = keys[..] {
        if let (Some(by), Ok(i), Leaf::Float(x)) = (set.strengths.get_mut(name), i.parse::<usize>(), to)
            && let Some(slot) = by.get_mut(i)
        {
            *slot = x as f32;
        }
        return;
    }
    let mut root = Value::Table(lab_tree(&set.rules));
    let mut at = &mut root;
    for key in &keys {
        let next = match at {
            Value::Table(t) => t.get_mut(*key),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get_mut(i)),
            _ => None,
        };
        let Some(next) = next else { return };
        at = next;
    }
    *at = to.value();
    if let Ok(rules) = root.try_into::<Rules>() {
        set.rules = rules;
    }
}

/// How far one press moves a number of this size.
pub fn step_of(x: f64) -> f64 {
    match x.abs() {
        a if a < 0.2 => 0.01,
        a if a < 1.0 => 0.05,
        a if a < 10.0 => 0.5,
        _ => 1.0,
    }
}

/// Moves the number at `path` one step up (`dir` 1) or down (`dir` -1); a switch flips.
pub fn step(set: &mut RuleSet, path: &str, dir: i32) {
    let round = |x: f64| (x * 10_000.0).round() / 10_000.0;
    let Some(now) = leaf(set, path) else { return };
    // How an effect changes by itself may be a fall (below 0) as well as a rise.
    let least = if path.starts_with("effects/") && path.ends_with("/per_hour") { f64::MIN } else { 0.0 };
    let to = match now {
        Leaf::Int(n) => Leaf::Int(n + dir as i64),
        Leaf::Float(x) => {
            // Stepping toward 0 takes the smaller step at a boundary (1 down to 0.95, not 0.5).
            let size = step_of(if x != 0.0 && (dir < 0) == (x > 0.0) { x - 1e-9 * x.signum() } else { x });
            Leaf::Float(round(x + dir as f64 * size).max(least))
        }
        Leaf::Bool(b) => Leaf::Bool(!b),
        Leaf::Dice(_) => return,
    };
    set_leaf(set, path, to);
}

/// Which part of dice a control moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DicePart {
    Count,
    Sides,
    Bonus,
}

/// Moves one part of the dice at `path`: a count or the sides never below 0 and 1, the bonus anywhere.
pub fn step_dice(set: &mut RuleSet, path: &str, part: DicePart, dir: i32) {
    let Some(Leaf::Dice(mut d)) = leaf(set, path) else { return };
    let i = part as usize;
    d[i] += dir as i64;
    d[0] = d[0].max(0);
    d[1] = d[1].max(if d[0] > 0 { 1 } else { 0 });
    set_leaf(set, path, Leaf::Dice(d));
}

/// The paths of the numbers that differ between `edited` and `saved`.
pub fn changed(edited: &RuleSet, saved: &RuleSet) -> Vec<String> {
    let was = leaves(saved);
    leaves(edited).into_iter().filter(|(p, l)| was.iter().find(|(q, _)| q == p).is_none_or(|(_, w)| w != l)).map(|(p, _)| p).collect()
}

/// `was {was}` when it is not `now`: what the lab writes small beside a number that the edited
/// rules moved. The one helper every view of a number uses.
pub fn was_text(now: &str, was: &str) -> Option<String> {
    (now != was).then(|| format!("was {was}"))
}

/// What kind of control a field gets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FieldKind {
    Number,
    Switch,
    Dice,
}

/// A field of the rules column.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub path: String,
    pub label: String,
    pub kind: FieldKind,
    pub now: String,
    /// What it was, when the edit has moved it.
    pub was: Option<String>,
}

/// A titled group of fields.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub title: String,
    pub fields: Vec<Field>,
}

/// Plain words for the fields, by path, in the units the designer thinks in.
const LABELS: &[(&str, &str)] = &[
    ("combat/armour_base", "Armour base"),
    ("combat/natural_hit", "Roll that always hits"),
    ("combat/natural_miss", "Roll that always misses"),
    ("combat/venom_save_dc", "Venom save difficulty"),
    ("combat/hit_dice/fighter", "Fighter hit die"),
    ("combat/hit_dice/ranger", "Ranger hit die"),
    ("combat/hit_dice/mage", "Mage hit die"),
    ("combat/hit_dice/cleric", "Cleric hit die"),
    ("needs/hunger_hours_to_full", "Hours walking to Starving"),
    ("needs/thirst_hours_to_full", "Hours walking to Dehydrated"),
    ("needs/sleepiness_hours_to_full", "Hours walking to Spent"),
    ("needs/resting_rate", "Rate while resting (\u{d7})"),
    ("needs/walking_rate", "Rate while walking (\u{d7})"),
    ("needs/exerting_rate", "Rate while exerting (\u{d7})"),
    ("needs/sleeping_rate", "Hunger and thirst rate asleep (\u{d7})"),
    ("needs/sleep_hours_to_rested", "Hours asleep to rested"),
    ("exhaustion/famished_at", "Famished and Dry begin at (share)"),
    ("exhaustion/sleep_hours_to_recover", "Hours asleep to lose a level"),
    ("exhaustion/starving_first_days", "Days starving before exhaustion (+ CON)"),
    ("exhaustion/starving_first_days_least", "Fewest days starving before exhaustion"),
    ("exhaustion/starving_every_hours", "Hours starving per further level"),
    ("exhaustion/dehydrated_first_hours", "Hours dehydrated before exhaustion"),
    ("exhaustion/dehydrated_every_hours", "Hours dehydrated per further level"),
    ("exhaustion/spent_first_hours", "Hours spent before exhaustion"),
    ("exhaustion/spent_every_hours", "Hours spent per further level"),
    ("exhaustion/slowed_from", "Slowed from level"),
    ("exhaustion/slowed_speed", "Slowed pace (\u{d7})"),
    ("exhaustion/halted_from", "Cannot move from level"),
    ("stamina/sprint_seconds", "Seconds of sprint"),
    ("stamina/burdened_drain", "Drain while Burdened (\u{d7})"),
    ("stamina/encumbered_drain", "Drain while Encumbered (\u{d7})"),
    ("stamina/stand_refill_seconds", "Seconds to refill standing"),
    ("stamina/walk_refill_seconds", "Seconds to refill walking"),
    ("stamina/sleep_refill_factor", "Refill asleep (\u{d7})"),
    ("stamina/second_wind", "Second wind at (share)"),
    ("stamina/swing_cost", "Cost of a swing (share)"),
    ("stamina/block_cost", "Cost of a block (share)"),
    ("mana/martial_pool", "Mana of a fighter or ranger"),
    ("mana/caster_base", "Caster mana at modifier 0"),
    ("mana/caster_per_modifier", "Mana per point of modifier"),
    ("mana/caster_floor", "Caster mana at least"),
    ("mana/rest_per_hour", "Mana regained resting per hour (share)"),
    ("mana/walk_per_hour", "Mana regained walking per hour (share)"),
    ("mana/sleep_per_hour", "Mana regained asleep per hour (share)"),
    ("load/capacity_per_strength", "Capacity per STR (lb)"),
    ("load/burdened_from", "Burdened from (share of capacity)"),
    ("load/encumbered_from", "Encumbered from (share of capacity)"),
    ("load/overloaded_over", "Overloaded over (share of capacity)"),
    ("load/burdened_walk", "Pace while Burdened (\u{d7})"),
    ("load/encumbered_walk", "Pace while Encumbered (\u{d7})"),
    ("load/overloaded_walk", "Pace while Overloaded (\u{d7})"),
    ("visibility/in_dark", "Seen in the dark"),
    ("visibility/in_light", "Added by full light"),
    ("visibility/moving", "Added by sprinting"),
    ("visibility/sprint_pace", "Sprint pace (\u{d7})"),
    ("visibility/stealth_step", "Taken off per point of Stealth"),
    ("visibility/burdened_bulk", "Added by Burdened"),
    ("visibility/encumbered_bulk", "Added by Encumbered"),
    ("warmth/comfort", "Warmth the first level spares"),
    ("warmth/chill_per_depth", "Warmth needed per level deeper"),
    ("warmth/brazier", "Warmth of a brazier"),
    ("warmth/brazier_reach", "Reach of a brazier (units)"),
    ("warmth/held_flame", "Warmth of a flame in hand"),
];

/// The words for an effect's field, by what follows `effects/<id>/`; `None` for one the lab
/// does not offer (whether it sits on a body part).
fn effect_label(def: &fb_character::effect::EffectDef, rest: &[&str]) -> Option<String> {
    use fb_character::effect::Influence;
    Some(match rest {
        ["hours"] => "Lasts (hours)".into(),
        ["per_hour"] => "Change by itself per hour".into(),
        ["exposure_per_hour"] => "Added per hour of exposure".into(),
        ["incubation_hours"] => "Hidden for (hours)".into(),
        ["catch_chance"] => "Chance to catch (share)".into(),
        ["stages", i, "at"] => format!("{} from", def.stages.get(i.parse::<usize>().ok()?)?.word),
        ["influences", i, key] => {
            let what = match def.influences.get(i.parse::<usize>().ok()?)? {
                Influence::AttackDisadvantage { .. } => "Disadvantage".to_string(),
                Influence::NoRecovery { .. } => "No recovery".into(),
                Influence::NeedRate { meter, .. } => format!("{meter:?} rate at full (\u{d7})"),
                Influence::Sight { .. } => "Sight at full (\u{d7})".into(),
                Influence::Pace { .. } => "Pace at full (\u{d7})".into(),
                Influence::Noticed { .. } => "Added to being seen at full".into(),
                Influence::HitPointsPerHour { .. } => "HP lost per hour at full".into(),
                Influence::Glare { .. } => "Glare at full".into(),
            };
            match *key {
                "at_full" => what,
                "from_stage" => format!("{what}: from stage"),
                _ => return None,
            }
        }
        _ => return None,
    })
}

const CREATURE_LABELS: &[(&str, &str)] =
    &[("armour_class", "Armour"), ("attack_bonus", "Attack bonus"), ("hit_points", "Hit points"), ("hit_dice", "Hit dice"), ("bite", "Bite"), ("venom", "Venom")];

/// The sections of the rules column, in order, each field with what it was under `saved`.
pub fn sections(edited: &RuleSet, saved: &RuleSet) -> Vec<Section> {
    let was = leaves(saved);
    let all = leaves(edited);
    let field = |path: &str, now: Leaf| -> Field {
        let keys: Vec<&str> = path.split('/').collect();
        let label = match keys[..] {
            ["creatures", _, key] => CREATURE_LABELS.iter().find(|(k, _)| *k == key).map(|(_, l)| l.to_string()),
            ["effects", id, ref rest @ ..] => edited.rules.effects.get(id).and_then(|def| effect_label(def, rest)),
            ["needs", "band_edges", i] => i.parse::<usize>().ok().map(|i| format!("Band {} begins at (share)", i + 1)),
            ["affixes", name, i] => Some(format!("{name}, effect {}", i.parse::<usize>().map_or(1, |i| i + 1))),
            _ => LABELS.iter().find(|(p, _)| *p == path).map(|(_, l)| l.to_string()),
        }
        .unwrap_or_else(|| humanise(keys[keys.len() - 1]));
        let kind = match now {
            Leaf::Bool(_) => FieldKind::Switch,
            Leaf::Dice(_) => FieldKind::Dice,
            _ => FieldKind::Number,
        };
        let before = was.iter().find(|(p, _)| p == path).map(|(_, l)| *l);
        let moved = before.filter(|b| *b != now);
        Field { path: path.to_string(), label, kind, now: now.text(), was: moved.map(|b| b.text()) }
    };
    let under = |prefix: &str| -> Vec<Field> { all.iter().filter(|(p, _)| p.starts_with(prefix)).map(|(p, l)| field(p, *l)).collect() };
    let mut out = vec![Section { title: "Combat".into(), fields: under("combat/") }];
    let mut names: Vec<&String> = edited.rules.creatures.keys().collect();
    names.sort();
    for name in names {
        out.push(Section { title: name.clone(), fields: under(&format!("creatures/{name}/")) });
    }
    for (title, key) in [("Needs", "needs"), ("Exhaustion", "exhaustion"), ("Stamina", "stamina"), ("Mana", "mana"), ("Load", "load"), ("Visibility", "visibility"), ("Warmth", "warmth")] {
        out.push(Section { title: title.into(), fields: under(&format!("{key}/")) });
    }
    for (id, def) in &edited.rules.effects {
        let offered = |p: &str| p.split('/').collect::<Vec<_>>().get(2..).is_some_and(|rest| effect_label(def, rest).is_some());
        let fields = all.iter().filter(|(p, _)| p.starts_with(&format!("effects/{id}/")) && offered(p)).map(|(p, l)| field(p, *l)).collect();
        out.push(Section { title: def.name.clone(), fields });
    }
    out.push(Section { title: "Affixes".into(), fields: under("affixes/") });
    out.retain(|s| !s.fields.is_empty());
    out
}

/// `hunger_hours_to_full` as `Hunger hours to full`.
fn humanise(key: &str) -> String {
    crate::lab_sheet::capitalised(&key.replace('_', " "))
}

/// The label of the affix field with its effect, for a tooltip or a test: `Warm adds warmth`.
pub fn affix_effect(name: &str, i: usize) -> Option<String> {
    let affix = affixes().iter().find(|a| a.name == name)?;
    let m = affix.fx.iter().filter(|m| strength_of(m).is_some()).nth(i)?;
    let json = serde_json::to_value(m).ok()?;
    let (kind, body) = json.as_object()?.iter().next()?;
    let stat = body.get("stat").and_then(|s| s.as_str()).unwrap_or("damage or armour").replace('_', " ");
    Some(match kind.as_str() {
        "scale" => format!("{stat} \u{d7}"),
        _ => format!("adds {stat}"),
    })
}

/// `text` (the contents of `data/rules.toml`) with the numbers of `rules` set in it, and its
/// comments, order and layout as they were. A number already equal is left as written.
pub fn write_rules_toml(text: &str, rules: &Rules) -> Result<String, String> {
    use toml_edit::{DocumentMut, Item};
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("rules.toml: {e}"))?;
    let mut scalars: Vec<(Vec<String>, Leaf)> = Vec::new();
    let mut found = Vec::new();
    let mut root = tree(rules);
    root.remove("effects");
    let root = Value::Table(root);
    add_missing_arrays(doc.as_item_mut(), &root);
    walk(&mut Vec::new(), &root, &mut found);
    for (path, leaf) in found {
        let keys: Vec<String> = path.split('/').map(String::from).collect();
        match leaf {
            Leaf::Dice([c, s, b]) => {
                for (part, n) in [("count", c), ("sides", s), ("bonus", b)] {
                    let mut k = keys.clone();
                    k.push(part.to_string());
                    scalars.push((k, Leaf::Int(n)));
                }
            }
            other => scalars.push((keys, other)),
        }
    }
    for (keys, leaf) in scalars {
        let mut item: &mut Item = doc.as_item_mut();
        for key in &keys {
            item = match key.parse::<usize>() {
                Ok(i) if item.is_array() => &mut item[i],
                _ => &mut item[key.as_str()],
            };
        }
        let same = match leaf {
            Leaf::Int(n) => item.as_integer() == Some(n) || item.as_float() == Some(n as f64),
            Leaf::Float(x) => item.as_float().or(item.as_integer().map(|n| n as f64)).is_some_and(|y| (x - y).abs() < 1e-6),
            Leaf::Bool(b) => item.as_bool() == Some(b),
            Leaf::Dice(_) => true,
        };
        if !same {
            *item = match leaf {
                Leaf::Int(n) => toml_edit::value(n),
                Leaf::Float(x) => toml_edit::value((x as f32).to_string().parse::<f64>().unwrap_or(x)),
                Leaf::Bool(b) => toml_edit::value(b),
                Leaf::Dice(_) => continue,
            };
        }
    }
    write_effects(&mut doc, rules)?;
    Ok(doc.to_string())
}

/// Writes into `item` each array of `value` it does not have, whole, so its numbers can be set
/// by their place.
fn add_missing_arrays(item: &mut toml_edit::Item, value: &Value) {
    match value {
        Value::Table(table) => {
            for (key, inner) in table {
                if matches!(inner, Value::Table(_) | Value::Array(_)) {
                    add_missing_arrays(&mut item[key.as_str()], inner);
                }
            }
        }
        Value::Array(_) if item.is_none() => {
            if let Ok(array) = value.to_string().parse::<toml_edit::Value>() {
                *item = toml_edit::Item::Value(array);
            }
        }
        _ => {}
    }
}

/// Puts in `doc` an `[effects.<id>]` table for each effect of `rules` that differs from
/// `data/effects.json`, naming only the fields that differ (the rules read the rest from that
/// file), and takes out the tables of effects that are back to it.
fn write_effects(doc: &mut toml_edit::DocumentMut, rules: &Rules) -> Result<(), String> {
    use toml_edit::{DocumentMut, Item};
    let shipped = fb_character::effect::shipped();
    let json = |def: &fb_character::effect::EffectDef| serde_json::to_value(def).expect("an effect serialises");
    if !doc.contains_key("effects") {
        let mut effects = toml_edit::Table::new();
        effects.set_implicit(true);
        doc.insert("effects", Item::Table(effects));
    }
    let effects = doc["effects"].as_table_mut().ok_or("rules.toml: effects is not a table")?;
    for (id, def) in &rules.effects {
        let now = json(def);
        let was = shipped.get(id).map(json).unwrap_or_default();
        let differ: serde_json::Map<String, serde_json::Value> =
            now.as_object().into_iter().flatten().filter(|(k, v)| was.get(k.as_str()) != Some(*v)).map(|(k, v)| (k.clone(), v.clone())).collect();
        if differ.is_empty() {
            effects.remove(id);
            continue;
        }
        let text = toml::to_string(&serde_json::json!({ "effects": { id: differ } })).map_err(|e| format!("effect {id}: {e}"))?;
        let part: DocumentMut = text.parse().map_err(|e| format!("effect {id}: {e}"))?;
        let mut table = part["effects"][id.as_str()].clone();
        // Keep a comment the designer wrote above the table.
        if let (Some(old), Some(new)) = (effects.get(id).and_then(Item::as_table), table.as_table_mut()) {
            new.decor_mut().clone_from(old.decor());
        }
        effects.insert(id, std::mem::take(&mut table));
    }
    if effects.is_empty() {
        doc.remove("effects");
    }
    Ok(())
}

/// `text` (the contents of `data/afx.json`) with each effect's `by` set to `strengths`, nothing
/// else touched: the nth `"by":` in the file is the nth strength of the table.
pub fn write_afx_json(text: &str, strengths: &Strengths) -> String {
    let wanted: Vec<f32> = affixes().iter().flat_map(|a| strengths.get(&a.name).cloned().unwrap_or_default()).collect();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut wanted = wanted.into_iter();
    while let Some(at) = rest.find("\"by\":") {
        let (head, tail) = rest.split_at(at + "\"by\":".len());
        out.push_str(head);
        let spaces = tail.len() - tail.trim_start().len();
        out.push_str(&tail[..spaces]);
        let tail = &tail[spaces..];
        let end = tail.find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '.' | 'e' | 'E' | '+'))).unwrap_or(tail.len());
        let old = &tail[..end];
        match (wanted.next(), old.parse::<f32>()) {
            (Some(new), Ok(was)) if new != was => out.push_str(&new.to_string()),
            _ => out.push_str(old),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// The strengths a copy of `data/afx.json` holds, or `None` if it does not read.
pub fn strengths_from_json(text: &str) -> Option<Strengths> {
    let parsed: Vec<Affix> = serde_json::from_str(text).ok()?;
    Some(parsed.iter().map(|a| (a.name.clone(), default_strength(a))).collect())
}

/// Writes `edited` into `rules.toml` and `afx.json` in `folder`, each through a temp file
/// beside it and `rename`, so a file is whole or as it was. If the second cannot be put in
/// place the first is put back, so the disk never holds half a save.
pub fn save_files(folder: &std::path::Path, set: &RuleSet, rename: &dyn Fn(&std::path::Path, &std::path::Path) -> std::io::Result<()>) -> Result<(), String> {
    use std::fs;
    let (rules, affixes) = (folder.join(crate::rules::RULES_FILE), folder.join("afx.json"));
    let read = |path: &std::path::Path| fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()));
    let (old_rules, old_affixes) = (read(&rules)?, read(&affixes)?);
    let new_rules = write_rules_toml(&old_rules, &set.rules)?;
    let new_affixes = write_afx_json(&old_affixes, &set.strengths);
    let temp = |path: &std::path::Path| path.with_extension("saving");
    let (temp_rules, temp_affixes) = (temp(&rules), temp(&affixes));
    let write = |path: &std::path::Path, text: &str| fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()));
    let cleanup = || {
        let _ = fs::remove_file(&temp_rules);
        let _ = fs::remove_file(&temp_affixes);
    };
    if let Err(e) = write(&temp_rules, &new_rules).and_then(|_| write(&temp_affixes, &new_affixes)) {
        cleanup();
        return Err(e);
    }
    if let Err(e) = rename(&temp_rules, &rules) {
        cleanup();
        return Err(format!("{}: {e}", rules.display()));
    }
    if let Err(e) = rename(&temp_affixes, &affixes) {
        cleanup();
        // Put the first back, whole, the same way.
        let restored = write(&temp_rules, &old_rules).and_then(|_| fs::rename(&temp_rules, &rules).map_err(|e| e.to_string()));
        cleanup();
        return Err(match restored {
            Ok(()) => format!("{}: {e}", affixes.display()),
            Err(r) => format!("{}: {e}; and {} could not be put back: {r}", affixes.display(), rules.display()),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(path: &str) -> String {
        std::fs::read_to_string(format!("{}/../data/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn every_number_of_the_rules_is_a_field_with_plain_words() {
        let set = RuleSet::default();
        let sections = sections(&set, &set);
        let titles: Vec<&str> = sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(&titles[..1], ["Combat"]);
        for want in ["Giant Rat", "Needs", "Exhaustion", "Stamina", "Mana", "Load", "Visibility", "Affixes"] {
            assert!(titles.contains(&want), "{want} in {titles:?}");
        }
        let all: Vec<&Field> = sections.iter().flat_map(|s| &s.fields).collect();
        let field = |path: &str| all.iter().find(|f| f.path == path).unwrap_or_else(|| panic!("no {path}"));
        assert_eq!((field("load/capacity_per_strength").label.as_str(), field("load/capacity_per_strength").now.as_str()), ("Capacity per STR (lb)", "15"));
        assert_eq!(field("creatures/Giant Rat/bite").now, "1d4+2");
        assert_eq!(field("creatures/Giant Rat/bite").kind, FieldKind::Dice);
        assert_eq!(field("creatures/Giant Rat/attack_bonus").label, "Attack bonus");
        assert!(all.iter().all(|f| f.was.is_none()));
        assert!(all.iter().all(|f| !f.label.contains('_')), "{:?}", all.iter().find(|f| f.label.contains('_')));
    }

    #[test]
    fn a_step_changes_one_number_and_marks_it() {
        let saved = RuleSet::default();
        let mut edited = saved.clone();
        for _ in 0..2 {
            step(&mut edited, "creatures/Giant Rat/attack_bonus", -1);
        }
        assert_eq!(edited.rules.creatures["Giant Rat"].attack_bonus, 2);
        assert_eq!(changed(&edited, &saved), vec!["creatures/Giant Rat/attack_bonus".to_string()]);
        let rat = sections(&edited, &saved).into_iter().find(|s| s.title == "Giant Rat").unwrap();
        let bonus = rat.fields.iter().find(|f| f.label == "Attack bonus").unwrap();
        assert_eq!((bonus.now.as_str(), bonus.was.as_deref()), ("2", Some("4")));
        edited = saved.clone();
        assert!(changed(&edited, &saved).is_empty());
    }

    #[test]
    fn floats_dice_and_switches_step() {
        let mut set = RuleSet::default();
        step(&mut set, "load/capacity_per_strength", 1);
        assert_eq!(set.rules.load.capacity_per_strength, 16.0);
        step(&mut set, "mana/rest_per_hour", -1);
        assert!((set.rules.mana.rest_per_hour - 0.115).abs() < 1e-9, "{}", set.rules.mana.rest_per_hour);
        step_dice(&mut set, "creatures/Giant Rat/bite", DicePart::Count, 1);
        step_dice(&mut set, "creatures/Giant Rat/bite", DicePart::Bonus, -1);
        assert_eq!(leaf(&set, "creatures/Giant Rat/bite").unwrap().text(), "2d4+1");
        step_dice(&mut set, "creatures/Giant Rat/bite", DicePart::Count, -5);
        assert_eq!(leaf(&set, "creatures/Giant Rat/bite").unwrap().text(), "1");
        step(&mut set, "affixes/Warm/0", 1);
        assert_eq!(set.strengths["Warm"][0], 2.5);
    }

    #[test]
    fn was_text_says_only_what_moved() {
        assert_eq!(was_text("15", "13").as_deref(), Some("was 13"));
        assert_eq!(was_text("15", "15"), None);
    }

    #[test]
    fn a_stronger_affix_changes_the_items_it_rolls() {
        let mut set = RuleSet::default();
        step(&mut set, "affixes/Warm/0", 1);
        let warm = affixes().iter().find(|a| a.name == "Warm").unwrap();
        let bumped = strengthened(warm, &set.strengths);
        assert_eq!(strength_of(&bumped.fx[0]), Some(2.5));
        assert_eq!(strengthened(warm, &default_strengths()), *warm);
        assert_eq!(affix_effect("Warm", 0).as_deref(), Some("adds warmth"));
    }

    #[test]
    fn saving_the_rules_keeps_the_files_comments_and_reads_back_the_same() {
        let file = text("rules.toml");
        let defaults = Rules::default();
        assert_eq!(write_rules_toml(&file, &defaults).unwrap(), file, "nothing changed, nothing rewritten");
        let mut set = RuleSet::default();
        step(&mut set, "load/capacity_per_strength", 5);
        step(&mut set, "creatures/Giant Rat/attack_bonus", -1);
        step_dice(&mut set, "creatures/Giant Rat/bite", DicePart::Sides, 2);
        step(&mut set, "combat/armour_base", 1);
        let written = write_rules_toml(&file, &set.rules).unwrap();
        assert_eq!(toml::from_str::<Rules>(&written).unwrap(), set.rules);
        for comment in ["# The character rules: every number", "# Pounds of capacity for each point of STR (SRD p. 79).", "# Giant Rat, SRD p. 378"] {
            assert!(written.contains(comment), "{comment}");
        }
        assert!(written.contains("capacity_per_strength = 20"), "{written}");
        // Saved, the file reads as the saved rules: no change is left.
        let saved = RuleSet { rules: toml::from_str(&written).unwrap(), ..set.clone() };
        assert!(changed(&set, &saved).is_empty());
    }

    #[test]
    fn a_file_that_names_few_keys_gets_the_rest_added() {
        let mut set = RuleSet::default();
        step(&mut set, "load/capacity_per_strength", 1);
        let written = write_rules_toml("[combat]\narmour_base = 10 # keep\n", &set.rules).unwrap();
        assert!(written.contains("armour_base = 10 # keep"), "{written}");
        assert_eq!(toml::from_str::<Rules>(&written).unwrap().load.capacity_per_strength, 16.0);
    }

    #[test]
    fn saving_the_affixes_changes_only_the_strengths() {
        let file = text("afx.json");
        assert_eq!(write_afx_json(&file, &default_strengths()), file);
        let mut set = RuleSet::default();
        step(&mut set, "affixes/Warm/0", 1);
        let written = write_afx_json(&file, &set.strengths);
        assert_ne!(written, file);
        assert_eq!(file.lines().count(), written.lines().count());
        let parsed: Vec<fb_items::tables::Affix> = serde_json::from_str(&written).unwrap();
        assert_eq!(strength_of(&parsed[0].fx[0]), Some(2.5));
        assert_eq!(parsed[1..], affixes()[1..]);
    }

    fn copy_of_data() -> std::path::PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("fb_lab_save_{}_{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["rules.toml", "afx.json"] {
            std::fs::write(dir.join(name), text(name)).unwrap();
        }
        dir
    }

    fn edited() -> RuleSet {
        let mut set = RuleSet::default();
        step(&mut set, "load/capacity_per_strength", 5);
        step(&mut set, "affixes/Warm/0", 1);
        set
    }

    #[test]
    fn a_save_replaces_both_files_and_leaves_no_temp_behind() {
        let dir = copy_of_data();
        save_files(&dir, &edited(), &|a, b| std::fs::rename(a, b)).unwrap();
        let rules: Rules = toml::from_str(&std::fs::read_to_string(dir.join("rules.toml")).unwrap()).unwrap();
        assert_eq!(rules.load.capacity_per_strength, 20.0);
        assert_eq!(strengths_from_json(&std::fs::read_to_string(dir.join("afx.json")).unwrap()).unwrap()["Warm"][0], 2.5);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_save_that_fails_halfway_leaves_the_files_as_they_were() {
        let dir = copy_of_data();
        let calls = std::cell::Cell::new(0);
        let second_fails = |a: &std::path::Path, b: &std::path::Path| {
            calls.set(calls.get() + 1);
            if calls.get() == 2 { Err(std::io::Error::other("disk full")) } else { std::fs::rename(a, b) }
        };
        let error = save_files(&dir, &edited(), &second_fails).unwrap_err();
        assert!(error.contains("disk full"), "{error}");
        assert_eq!(std::fs::read_to_string(dir.join("rules.toml")).unwrap(), text("rules.toml"));
        assert_eq!(std::fs::read_to_string(dir.join("afx.json")).unwrap(), text("afx.json"));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2, "no temp left");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_without_the_files_is_an_error_not_a_panic() {
        let error = save_files(std::path::Path::new("/nonexistent/fb"), &edited(), &|a, b| std::fs::rename(a, b)).unwrap_err();
        assert!(error.contains("rules.toml"), "{error}");
    }

    #[test]
    fn each_effect_is_a_section_of_its_numbers_and_warmth_is_one_too() {
        let set = RuleSet::default();
        let sections = sections(&set, &set);
        let section = |title: &str| sections.iter().find(|s| s.title == title).unwrap_or_else(|| panic!("no {title}"));
        let plague: Vec<(&str, &str)> = section("Sewer plague").fields.iter().map(|f| (f.label.as_str(), f.now.as_str())).collect();
        for want in [("Lasts (hours)", "96"), ("Hidden for (hours)", "24"), ("Chance to catch (share)", "0.1"), ("Wasting from", "0.6"), ("No recovery: from stage", "1")] {
            assert!(plague.contains(&want), "{want:?} in {plague:?}");
        }
        assert!(section("Cold").fields.iter().any(|f| f.label == "Change by itself per hour" && f.now == "-0.25"));
        assert!(section("Warmth").fields.iter().any(|f| f.label == "Warmth of a brazier"));
        assert!(sections.iter().flat_map(|s| &s.fields).all(|f| !f.path.ends_with("on_a_part")));
        assert!(section("Needs").fields.iter().any(|f| f.label == "Band 4 begins at (share)" && f.now == "1"));
    }

    #[test]
    fn an_effects_numbers_step_even_below_zero() {
        let mut set = RuleSet::default();
        step(&mut set, "effects/cold/per_hour", 1);
        step(&mut set, "effects/sewer_plague/stages/1/at", -1);
        step(&mut set, "effects/cold/influences/1/at_full", 1);
        let cold = &set.rules.effects["cold"];
        assert_eq!((cold.per_hour, cold.stages.len()), (-0.2, 3));
        assert!((set.rules.effects["sewer_plague"].stages[1].at - 0.55).abs() < 1e-9);
        for _ in 0..25 {
            step(&mut set, "effects/cold/per_hour", 1);
        }
        assert!(set.rules.effects["cold"].per_hour > 0.0, "{}", set.rules.effects["cold"].per_hour);
        assert_eq!(changed(&set, &RuleSet::default()).len(), 3);
    }

    #[test]
    fn a_saved_effect_names_only_what_moved_and_reads_back() {
        let file = text("rules.toml");
        let mut set = RuleSet::default();
        step(&mut set, "effects/sewer_plague/hours", 1);
        step(&mut set, "effects/cold/stages/2/at", 1);
        let written = write_rules_toml(&file, &set.rules).unwrap();
        assert!(written.contains("[effects.sewer_plague]\nhours = 97"), "{written}");
        assert!(!written.contains("incubation_hours"), "only what moved: {written}");
        assert_eq!(toml::from_str::<Rules>(&written).unwrap(), set.rules);
        // Set back, the tables go.
        assert_eq!(write_rules_toml(&written, &Rules::default()).unwrap(), file);
    }

    #[test]
    fn strengths_read_from_a_json_text() {
        assert_eq!(strengths_from_json(&text("afx.json")), Some(default_strengths()));
        assert_eq!(strengths_from_json("nonsense"), None);
    }
}
