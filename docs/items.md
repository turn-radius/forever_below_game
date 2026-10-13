# Items

These are the rules the prototype settled on. Keep the intent and improve the details. Read alongside `reference/forever-below-plate.html` (`rollItem`, `rollSA`, `rollAffixes`, `legendize`, `dressState`, `saMeta`) and the tables in `data/`.

## Two layers

1. **Shape**: an `armory::ItemState`, rolled by the Armory's own "Forge new" (`SA.roll(family, seed, accept, preset)`). `data/sab.json` maps each base item name to an Armory family and, where needed, an `acc` filter on the rolled style (for example, `Boots` accepts `boot.style` of `boot` or `sabaton`). `roll` retries with seeds `(seed + i*7919) % 999983 + 1` until the filter accepts, up to 120 tries. Some bases have a `tw` tweak applied after rolling (a Mantle is a short cloak, a Purse always holds coins).
2. **Game layer**: rarity, affixes, stats, footprint, history.

A few bases are native to the plate because the Armory has no family for them yet: laced shirt, leather jerkin and pointed hat (`data/bases.json`). Torches, lanterns and the sundries in `data/goods.json` are now Armory families (see "Lights" and "Sundries" below), and the waterskin, lamp oil and glowcap are food.

### Clubs and axes

Clubs and axes are now Armory families (`armory::families::club`, `armory::families::axe`), after daggers in `Family::ALL`. They have no prototype fixtures, since the prototype's plate faked them outside the Armory. `data/sab.json` does not map the plate's `Club` and `Hand axe` bases to them yet. Where the Rust port departs from the plate:

- A club is one piece of wood: the haft swells into the striking end and closes in a dome (`Swell` in `sections::hafted`), and knots, iron bands and studs follow that surface. The plate's materials become archetypes: Oak and Ash are the wood's palette colour, and Bone-studded is bone teeth set in the swelling. Cudgels, plain, gnarled, knobbed (and shillelaghs), studded, spiked and iron-bound clubs roll too.
- An axe head is a bit crosswise through an eye (`sections::axe_head`, `build::axe_head`). It is wedge-sectioned, with a hard bevel line and a blade cross-section across its height, and it takes nicks and a broken edge from the item's condition. The eye is a socket section, with optional rings and langets. A plain, hammer, spike or second-bit poll backs it. Hatchets, hand, bearded, broad, long, battle, horseman's and throwing axes roll. The plate's Iron and Steel are both iron.
- The bit faces +X in item space, along the haft frame's normal. The plate's "turn 90° so the bit faces forward" (`docs/body.md`) still applies when the axe is held.

### Legwear

Trousers, breeches and leggings are the Armory's `legs` family (`armory::families::legs`), which the Armory prototype never had: its sliders, ranges and nouns are the port's own. They are drawn on `LowerBodyForm` (both legs and the hips, built from the garment's `TorsoForm` and the boot's `LegForm`), so `fb_body` can fit them by passing a folk body's dimensions. Where the port departs from the plate:

- The plate sweeps a tube down each leg at a fixed offset and lays a band over the hips. Here each leg becomes one half of the hips above the crotch, seamed down the front and back, so the pair reads as one garment.
- Breeches gather into a buckled knee band; trousers can have turned cuffs, knee patches, or puttees wound from the ankle to below the knee with the cloth bloused above them. The waistband closes with a drawstring, laces or toggles.
- A patterned pair of leggings is named Hose.

### Hoods and coifs

Hoods, cowls, coifs and mail coifs are the Armory's `hood` family (`armory::families::hood`), also new to the Armory, drawn on `HeadForm` (a parametric head from the plate's `headFn`) set on the garment's `TorsoForm`. Where the port departs from the plate:

- The plate draws a hood as one offset of the head with a torus for the cowl, and a mail coif as a lathed dome with a skirt. Here each is one sheet with thickness: over the head, round the face opening, falling from the back of the head and the chin to the shoulders, and out as a cape hung with the garment drape.
- Hoods draw their crown to a point and may trail a liripipe down the back; capes may be dagged. Cowls fall in a rolled loop over the chest. Coifs stop at the jaw and nape and tie under the chin; a quilted coif is an Arming Cap. Mail coifs take the shader's mail pattern, a leather binding round the face and sometimes a strapped ventail.
- A hood's hem trims the cape's edge (`hemline`) and the face opening (`neck`).

### Belts

Belts are the Armory's `belt` family (`armory::families::belt`). The strap is the garment's `belt` section (width, leather, buckle), so a belted tunic and a belt base share it; the fittings are the port's own: plain, studded, sword belts slung toward the left hip with one or two frogs, girdles with a long tail, bar mounts and a strap end, and pouch belts with loops and D-rings. Where the port departs from the plate:

- The plate draws a band with a torus buckle and an ellipsoid pouch. Here the strap has its section, the buckle its frame and tongue, the free end runs through a keeper, and the plate's `holds` belt pouch is drawn as the bags family's own belt pouch (the state's `pouch`), hung behind the right hip with the belt through its loops.

### Food and drink

The Armory's food family (`armory::families::food`) now also makes the glowcap, the waterskin and the lamp-oil flask from `data/goods.json`, as the `mushroom`, `waterskin` and `flask` styles. Where the Rust port departs from the prototype:

- The prototype's `cut` toggle is a portion: whole, cut (a wedge out of a wheel, pie or round loaf, the end off a loaf, a slice out of an apple), a single piece (a slice of bread, a wedge of cheese, a slice of pie) or bitten (bread, fruit, a roast). Cut faces show a band of crust, rind, skin or pastry round the crumb, paste, flesh or filling. `Blind cavefish` and `Healing draught` tweaks set colours, which now have named roles (`outer`, `inner`, `accent`, `liquid`) instead of `color` and `color2`.
- A potion's brew is typed, and the noun still follows the liquid's colour, so a retinted potion is named for the brew it looks like.
- Glass and liquids use the gem material with their own tint, until there is a glass material with transparency; a potion's glass is tinted toward its brew so it reads even when drawn opaque. Cork is tinted wood, wax seals tinted bone, a bottle's paper label tinted cloth.
- Food's condition word is `freshness()` (Fresh, Day-old, Stale, Mouldy, Rotten), the prototype's `condWord` for food.

### Lights

Torches and lanterns are Armory families (`armory::families::torch`, `armory::families::lantern`), drawn from the bases the plate drew in `weaponGroup()`. Each light-giving family has a `light(&state)` that returns an `armory::build::light::LightAnchor`: the light's position in item space, a suggested colour, a radius in item units converted from the base's light stat in feet (`light` in `data/bases.json`), and how much it flickers. The game hangs the carried light there. Where the Rust port departs from the plate:

- A torch has three heads (a wrapped rag, a bound bundle of resinous splints, an iron cresset packed with tow) and burns through four states: fresh, burning, doused and spent. The plate's `Pitch` and `Tallow` materials are the head's fuel. How charred the head is, and how tall the flame stands, are sliders. A spent torch is named `Torch Stub`.
- Flames and embers are glowing meshes rather than an unlit material and a sprite. The halo is left to the game.
- A torch's light reaches 20 ft at an ordinary flame, scaled by the flame's height and the head: a cresset reaches further and a splint bundle less far. Tallow burns a little dimmer.
- A lantern (`armory::families::lantern`) has three bodies (horn panes in a frame, a pierced tin drum, a wire cage round a glass chimney), each under a hood and over a base, hung by a wire bail on 0 to 6 chain links from the carrying ring the hand holds. Inside stands a candle in a socket or an oil font with a burner and a filler cap. The plate's `Tin`, `Brass` and `Iron` are the lantern's metal; tin is iron tinted pale and horn is bone tinted amber, until there are materials for them.
- A lit lantern's panes, holes or chimney glow. Its light reaches the base's 30 ft through glass, less through horn and least through a pierced drum, and an oil flame reaches further than a candle.

- A glowcap (a food mushroom glowing at 0.25 or more) gives light too: `Food::light` puts it at the middle of its caps, in the caps' colour, steady. The goods table only says it "glows faintly", so its reach (4 ft at full glow) is a proposal, not a number from the plate.

### Sundries

`armory::families::sundry` makes the small goods: candles, a coil of rope, lockpicks, a ring of keys, a tinderbox, a bedroll and bandages. Each rests on the ground, centred over the origin. Where it departs from the plate's `goodGroup()`:

- Keys, the tinderbox and the bedroll are new; the plate's text names them (lockpicking, "fever took them in their bedroll") but never drew them. A bedroll is the same rolled, strapped blanket a backpack carries (`armory::build::roll`).
- A candle can stand in a chamberstick, carry runs of wax, burn down and be lit, and `Sundry::light` gives its 5 ft light from the flame. The rope is one twisted rope laid in a coil and tied, not four stacked rings, and the lockpicks are hooks, rakes, diamonds and a tension wrench in an open leather roll.

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

The Armory says where a container's contents go: each container family has a `container(&state)` returning an `armory::build::container::ContainerSpace` (the mouth's centre, facing and size, the interior bounds, and the capacity in litres) in the same item space as its mesh. The game can drop items through the mouth or draw an open container's contents inside it. The capacity is the real volume of the modelled body; the inner grid sizes above stay a game rule.

Departures from the prototype's bags: the front pocket and bedroll roll only on backpacks and spilled coins only on pouches and purses (the prototype rolled them everywhere but drew them only there); a sewn patch can go on any bag, not just sacks; cloth bags take any woven pattern and leather ones can be studded. Every panel has a real thickness, straps lie on the surfaces they cross and close with buckles, and a satchel's strap hangs from D-rings.

Departures from the prototype's chests: each style rolls only the options it shows. Iron hoops go on barrels, shipping straps and rope handles on crates (the prototype ignored `bands` and `handles` there), and `lock` is an amphora's lid. Coffers and strongboxes can stand open as well as chests. A coffer's crowning stone is a gemstone in the state, not the second colour, and stencils can go on barrels as well as crates. A lidded box is hollow, with walls, a rim and a darker inside, so an open lid shows a real interior with the hoard heaped in it. Its lid turns on hinges and carries its hasp and bands. An amphora is clay (the stone material) tinted with its glaze colour, and stencils are the paint material.

## Classes

`data/outfits.json`: Fighter, Ranger, Mage and Cleric, plus Freshly fallen and Scavenger. Each slot lists candidate bases, optionally with a leading probability. A class's set is rolled once per character, so the outfit you preview is the one you get.
