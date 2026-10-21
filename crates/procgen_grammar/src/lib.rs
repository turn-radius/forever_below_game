//! Tracery-style grammar expansion, a pure function of a seed.
//!
//! A [`Grammar`] maps a symbol to a list of rules. A rule is plain text with `#symbol#`
//! references, which expand recursively; `#symbol.capitalize.s#` applies modifiers to the
//! result. Each choice of rule comes from a `procgen_rng::Mulberry32` stream, so
//! `expand(&grammar, "#origin#", seed)` always gives the same text for the same seed.
//! `##` is a literal `#`.
//!
//! Modifiers: `capitalize`, `upper`, `lower`, `s` (plural), `a` (article: "a" or "an").
//!
//! Faction names live in `data/faction_names.json` and come out of [`faction_name`].

use procgen_rng::Mulberry32;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

/// How deep `#symbol#` references may nest before an expansion is abandoned.
pub const MAX_DEPTH: usize = 24;
/// How many symbol expansions one call may make, which stops a rule that doubles itself.
pub const MAX_EXPANSIONS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrammarError {
    /// A rule refers to a symbol the grammar does not define.
    MissingSymbol(String),
    /// A symbol is defined with no rules.
    EmptySymbol(String),
    /// A modifier name that does not exist.
    UnknownModifier(String),
    /// References nested past [`MAX_DEPTH`] (a rule that reaches itself).
    TooDeep,
    /// More than [`MAX_EXPANSIONS`] expansions in one call.
    TooLarge,
    /// A grammar key (folk or intention) the name data does not have.
    UnknownKey(String),
}

impl fmt::Display for GrammarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSymbol(s) => write!(f, "missing symbol `{s}`"),
            Self::EmptySymbol(s) => write!(f, "symbol `{s}` has no rules"),
            Self::UnknownModifier(m) => write!(f, "unknown modifier `.{m}`"),
            Self::TooDeep => write!(f, "references nest too deeply"),
            Self::TooLarge => write!(f, "expansion grew too large"),
            Self::UnknownKey(k) => write!(f, "unknown key `{k}`"),
        }
    }
}

impl std::error::Error for GrammarError {}

/// A map of symbol to rules.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct Grammar {
    pub symbols: BTreeMap<String, Vec<String>>,
}

impl Grammar {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Build from `(symbol, rules)` pairs; handy in tests.
    pub fn from_pairs(pairs: &[(&str, &[&str])]) -> Self {
        Self {
            symbols: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
                .collect(),
        }
    }

    /// This grammar with `other`'s symbols laid over it (a symbol in both takes `other`'s rules).
    pub fn merged(&self, other: &Grammar) -> Grammar {
        let mut symbols = self.symbols.clone();
        for (k, v) in &other.symbols {
            symbols.insert(k.clone(), v.clone());
        }
        Grammar { symbols }
    }

    /// Every symbol that `start` or a rule refers to but the grammar lacks, every symbol
    /// defined with no rules, and every unknown modifier. Empty when the grammar is sound.
    pub fn problems(&self, start: &str) -> Vec<GrammarError> {
        let mut out = Vec::new();
        let mut push = |e: GrammarError| {
            if !out.contains(&e) {
                out.push(e);
            }
        };
        let mut texts: Vec<&str> = vec![start];
        for (name, rules) in &self.symbols {
            if rules.is_empty() {
                push(GrammarError::EmptySymbol(name.clone()));
            }
            texts.extend(rules.iter().map(String::as_str));
        }
        for text in texts {
            for tag in tags(text) {
                let (sym, mods) = split_tag(tag);
                if !self.symbols.contains_key(sym) {
                    push(GrammarError::MissingSymbol(sym.to_string()));
                }
                for m in mods {
                    if !MODIFIERS.contains(&m) {
                        push(GrammarError::UnknownModifier(m.to_string()));
                    }
                }
            }
        }
        out
    }
}

const MODIFIERS: [&str; 5] = ["capitalize", "upper", "lower", "s", "a"];

/// The text of each `#...#` tag in `text`, skipping `##` and an unclosed `#`.
fn tags(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('#') {
        let after = &rest[i + 1..];
        match after.find('#') {
            Some(j) => {
                if j > 0 {
                    out.push(&after[..j]);
                }
                rest = &after[j + 1..];
            }
            None => break,
        }
    }
    out
}

fn split_tag(tag: &str) -> (&str, Vec<&str>) {
    let mut parts = tag.split('.');
    let sym = parts.next().unwrap_or("");
    (sym, parts.collect())
}

/// Expand `start` (usually `"#origin#"`) with a stream seeded by `seed`.
pub fn expand(grammar: &Grammar, start: &str, seed: u32) -> Result<String, GrammarError> {
    let mut rng = Mulberry32::new(seed);
    expand_with(grammar, start, &mut rng)
}

/// Expand from an RNG the caller owns, so several expansions can share one stream.
pub fn expand_with(
    grammar: &Grammar,
    start: &str,
    rng: &mut Mulberry32,
) -> Result<String, GrammarError> {
    let mut budget = MAX_EXPANSIONS;
    expand_text(grammar, start, rng, 0, &mut budget)
}

fn expand_text(
    g: &Grammar,
    text: &str,
    rng: &mut Mulberry32,
    depth: usize,
    budget: &mut usize,
) -> Result<String, GrammarError> {
    if depth > MAX_DEPTH {
        return Err(GrammarError::TooDeep);
    }
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('#') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let Some(j) = after.find('#') else {
            // Unclosed: keep the `#` as text.
            out.push('#');
            rest = after;
            continue;
        };
        if j == 0 {
            out.push('#');
        } else {
            if *budget == 0 {
                return Err(GrammarError::TooLarge);
            }
            *budget -= 1;
            let (sym, mods) = split_tag(&after[..j]);
            let rules = g
                .symbols
                .get(sym)
                .ok_or_else(|| GrammarError::MissingSymbol(sym.to_string()))?;
            if rules.is_empty() {
                return Err(GrammarError::EmptySymbol(sym.to_string()));
            }
            let rule = rng.pick(rules);
            let mut s = expand_text(g, rule, rng, depth + 1, budget)?;
            for m in mods {
                s = apply_modifier(m, &s)?;
            }
            out.push_str(&s);
        }
        rest = &after[j + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn apply_modifier(m: &str, s: &str) -> Result<String, GrammarError> {
    Ok(match m {
        "capitalize" => {
            let mut c = s.chars();
            match c.next() {
                Some(f) => f.to_uppercase().chain(c).collect(),
                None => String::new(),
            }
        }
        "upper" => s.to_uppercase(),
        "lower" => s.to_lowercase(),
        "s" => pluralize(s),
        // Simple case: "an" before a written vowel, else "a". "hour" and "unicorn" get it wrong.
        "a" => {
            let vowel = s.chars().next().is_some_and(|c| "aeiouAEIOU".contains(c));
            format!("{} {s}", if vowel { "an" } else { "a" })
        }
        other => return Err(GrammarError::UnknownModifier(other.to_string())),
    })
}

/// Plural of a regular English noun, simple cases only: -y after a consonant becomes -ies,
/// sibilants (-s, -x, -z, -ch, -sh) take -es, the rest take -s. Irregulars ("wolf", "mouse")
/// are not handled; write the plural into the word list instead. a consonant becomes -ies, sibilants take -es, the rest take -s.
fn pluralize(s: &str) -> String {
    let lower = s.to_lowercase();
    if lower.ends_with('y') {
        let before = lower.chars().rev().nth(1).unwrap_or('a');
        if !"aeiou".contains(before) {
            return format!("{}ies", &s[..s.len() - 1]);
        }
    }
    if ["s", "x", "z", "ch", "sh"].iter().any(|x| lower.ends_with(x)) {
        format!("{s}es")
    } else {
        format!("{s}s")
    }
}

/// The faction name rule sets: `shared` symbols, then a folk's word lists, then an
/// intention's `origin` rules, laid over each other in that order.
#[derive(Debug, Clone, Deserialize)]
pub struct FactionNames {
    #[serde(default)]
    pub shared: Grammar,
    pub folk: BTreeMap<String, Grammar>,
    pub intention: BTreeMap<String, Grammar>,
}

impl FactionNames {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// The grammar for one folk and intention.
    pub fn grammar(&self, folk: &str, intention: &str) -> Result<Grammar, GrammarError> {
        let f = self
            .folk
            .get(folk)
            .ok_or_else(|| GrammarError::UnknownKey(folk.to_string()))?;
        let i = self
            .intention
            .get(intention)
            .ok_or_else(|| GrammarError::UnknownKey(intention.to_string()))?;
        Ok(self.shared.merged(f).merged(i))
    }

    pub fn name(&self, folk: &str, intention: &str, seed: u32) -> Result<String, GrammarError> {
        let g = self.grammar(folk, intention)?;
        expand(&g, "#origin#", mix(seed, folk, intention))
    }
}

/// Fold the keys into the seed so two factions with the same seed but different folk or
/// intention don't pick the same rule indices.
pub(crate) fn mix(seed: u32, folk: &str, intention: &str) -> u32 {
    let mut h: u32 = 0x811C_9DC5 ^ seed;
    for b in folk.bytes().chain([b'/']).chain(intention.bytes()) {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    h
}

const FACTION_NAMES_JSON: &str = include_str!("../../../data/faction_names.json");

/// The faction name data shipped in `data/faction_names.json`.
pub fn faction_names() -> &'static FactionNames {
    static DATA: OnceLock<FactionNames> = OnceLock::new();
    DATA.get_or_init(|| {
        FactionNames::from_json(FACTION_NAMES_JSON).expect("data/faction_names.json parses")
    })
}

/// A faction's name, from its folk (`human`, `dwarf`, `gnome`, `goblin`, `lizardfolk`,
/// `pigfolk`) and intention (`way_out`, `trade`, `worship`, `plunder`). The same arguments
/// always give the same name. An unknown key gives `"the Nameless"`.
pub fn faction_name(folk_key: &str, intention_key: &str, seed: u32) -> String {
    faction_names()
        .name(folk_key, intention_key, seed)
        .unwrap_or_else(|_| "the Nameless".to_string())
}

#[cfg(test)]
mod tests;
