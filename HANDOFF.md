# Handoff: from the browser plate to Bevy

The design work happened in two browser prototypes, both in `reference/`:

- `spline-armory.html` is the item generator: eighteen families (staves, maces, swords, spears, daggers, bows, crossbows, helmets, body armour and clothing, gloves, boots, shields, rings, amulets, charms, bags, chests, food). "Forge new" rolls every slider from a seed.
- `forever-below-plate.html` is the character plate. It has procedural bodies for six folk, the paper doll with gear fitted to each body, the grid inventory with drag and drop, the rarity and affix layer, the character sheet and the four classes. It embeds the Armory engine unchanged (`reference/spline_armory_engine.js` is that engine on its own).

Open either page in a browser to see the idea. Both use three.js r128 from a CDN. They are a showcase and a starting point. The Rust and Bevy build should reach higher fidelity than they do, so treat them as the floor for quality, not the target.

## What already works here

- Cargo workspace on Bevy 0.20, with the generator crates kept separate from the game.
- `procgen_rng::Mulberry32`: a seeded RNG (the same algorithm the prototypes use, and bit-exact with them, though that is a convenience, not a requirement).
- `armory`: the family list and an untyped item state that can load the example fixtures (64 seeds per family, with generated names). Use them to see the range each family covers; there is no requirement to reproduce them.
- `game` opens a window with a placeholder cube.

## Milestones

### 1. One Armory item, drawn better than the prototype
Start with **maces** (the Armory's default family). Build a typed mace state and a seeded `roll`, using the prototype's sliders and ranges as the starting point, and a mesh builder at higher fidelity. Draw it in Bevy in the house style from `docs/rendering.md`: banded toon shading, ink outlines, and wear, rust and grime. Add a turntable view to compare variety across seeds.

Done when maces cover at least the variety of `spline-armory.html` (flanged, spiked, orb and onion heads, collars, wraps, gems), look better side by side, and the same seed always gives the same mace.

### 2. The rest of the Armory
Family by family: sword, dagger, staff, spear, shield, helm, bags, food, amulet, ring, charm, garment, hands, feet, bow, crossbow, chests. Also add the bases the prototype had to fake outside the Armory (trousers, hoods, belts, clubs, axes, torches, lanterns, sundries). Shared primitives (lathe, sweep along a spline with parallel-transport frames, grid surfaces, outline normals) go in `armory` first.

### 3. Items
`fb_items`: rarity odds, affix rules, the Armory-family-per-base table, derived stats and footprints, legendary names and histories. The specs are in `docs/items.md` and the tables in `data/`.

### 4. Body and doll
`fb_body`: the folk body, face, hair and beard, then worn gear as offsets of the body and held gear fitted to the hand (`docs/body.md`). In the game, add an inventory screen that matches the plate: grids, underfoot, the doll in the runic seal, hover cards.

### 5. First playable slice
One level, two factions, light and hunger, lockpicking, one affix pass. First person, real time.

## The prototypes after the handoff

They stay useful for quick experiments with a look or a rule before building it properly in Rust. `tools/README.md` explains how to regenerate the example fixtures and data tables if they change.
