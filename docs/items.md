# Items

These are the rules the prototype settled on. Keep the intent and improve the details. Read alongside `reference/forever-below-plate.html` (`rollItem`, `rollSA`, `rollAffixes`, `legendize`, `dressState`, `saMeta`) and the tables in `data/`.

## Two layers

1. **Shape**: an `armory::ItemState`, rolled by the Armory's own "Forge new" (`SA.roll(family, seed, accept, preset)`). `data/sab.json` maps each base item name to an Armory family and, where needed, an `acc` filter on the rolled style (for example, `Boots` accepts `boot.style` of `boot` or `sabaton`). `roll` retries with seeds `(seed + i*7919) % 999983 + 1` until the filter accepts, up to 120 tries. Some bases have a `tw` tweak applied after rolling (a Mantle is a short cloak, a Purse always holds coins).
2. **Game layer**: rarity, affixes, stats, footprint, history.

A few bases are native to the plate because the Armory has no family for them yet: trousers, breeches, leggings, laced shirt, leather jerkin, hood, pointed hat, mail coif, belt, club, hand axe, torch and lantern (`data/bases.json`), plus sundries like rope, candles and waterskins (`data/goods.json`). Adding them to the Armory would bring them into the same generator.

### Food and drink

The Armory's food family (`armory::families::food`) now also makes the glowcap, the waterskin and the lamp-oil flask from `data/goods.json`, as the `mushroom`, `waterskin` and `flask` styles. Where the Rust port departs from the prototype:

- The prototype's `cut` toggle is a portion: whole, cut (a wedge out of a wheel, pie or round loaf, the end off a loaf, a slice out of an apple), a single piece (a slice of bread, a wedge of cheese, a slice of pie) or bitten (bread, fruit, a roast). Cut faces show a band of crust, rind, skin or pastry round the crumb, paste, flesh or filling. `Blind cavefish` and `Healing draught` tweaks set colours, which now have named roles (`outer`, `inner`, `accent`, `liquid`) instead of `color` and `color2`.
- A potion's brew is typed, and the noun still follows the liquid's colour, so a retinted potion is named for the brew it looks like.
- Glass and liquids use the gem material with their own tint, until there is a glass material with transparency; a potion's glass is tinted toward its brew so it reads even when drawn opaque. Cork is tinted wood, wax seals tinted bone, a bottle's paper label tinted cloth.
- Food's condition word is `freshness()` (Fresh, Day-old, Stale, Mouldy, Rotten), the prototype's `condWord` for food.

## Rarity and affixes

- Roll `x`: `x < .58` common, `< .85` magic, `< .97` rare, otherwise legendary. Containers cap at rare.
- Common: 30% chance of one **flaw** prefix. Magic: one good prefix or suffix (50/50). Rare: a good prefix and a good suffix. Legendary: a good prefix and two good suffixes.
- An affix applies only where its `on` tag matches the item's tags (`cloth`, `leather`, `metal`, `wood`, `wear`, `armor`, `quiet`, `weapon`, `light`, `trinket`, `container`, or `any`). Tables: `data/afx.json`. Each `fx` is kept as the prototype's JS source; port it to a typed effect.
- When affixes are chosen by hand (from the armory), rarity follows from them: two good affixes make it rare, one makes it magic, none makes it common.
- Name: `<prefix> <Armory noun> <suffix>`. The noun comes from `forgeNoun` (Longsword, Rondel Dagger, Kettle Hat).
- A **legendary** gets a title (`<owner>'s <noun>` or `The <adjective> <noun>`). The original name becomes its subtitle, and it gets three lines of history: where it was made and by which faction, who carried it and what they did, and how it was lost.

## Rarity shows on the object

`dressState` sets the Armory condition from rarity: common wear 0.15–0.50, magic 0.06–0.26, rare 0.02–0.14. A legendary is either pristine (0.02) or old and worn (0.4–0.7). Rare and legendary items get etching; legendary etching is gold inlay. Flaws raise the matching decay: Rusted sets rust to 1.5, and Moth-eaten, Mildewed and Lice-ridden raise fray and rot.

## Stats and footprint (`saMeta`)

Each family maps to a slot, tags, weight, armour, warmth and damage, plus a grid footprint from the item's real size: `cells(units) = clamp(ceil(units / 2.4), 1, 9)`, about 24 cm per cell. A staff, spear or bow longer than 14.4 units (six cells) cannot be packed. Bags define their inner grid: backpack 8×5, satchel 6×3, sack 4×4, belt pouch 4×2.

## Slots

`data/slots.json` lists the slots: head, neck, shirt, shoulders (cloak), armour (body), main hand, hands, feet, back, belt, off hand, legs, two rings and a charm. Rings fill whichever ring slot is free.

## Containers

A bag keeps its contents wherever it goes, and its weight counts through every bag you carry. A bag set down opens underfoot as its own grid. A bag cannot go inside itself or inside a bag it already holds.

## Classes

`data/outfits.json`: Fighter, Ranger, Mage and Cleric, plus Freshly fallen and Scavenger. Each slot lists candidate bases, optionally with a leading probability. A class's set is rolled once per character, so the outfit you preview is the one you get.
