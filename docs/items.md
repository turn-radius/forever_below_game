# Items

These are the rules the prototype settled on. Keep the intent and improve the details. Read alongside `reference/forever-below-plate.html` (`rollItem`, `rollSA`, `rollAffixes`, `legendize`, `dressState`, `saMeta`) and the tables in `data/`.

## Two layers

1. **Shape**: an `armory::Item`, rolled by the Armory's own "Forge new". `fb_items::armory_base` maps each base name (from `data/bases.json`, `data/goods.json` and the plate's own Armory list) to an Armory family, the styles of it the base accepts (for example, `Boots` accepts a `boot.style` of `boot` or `sabaton`), and sometimes a tweak applied after rolling (a Mantle is a short cloak, a Purse always holds coins). `fb_items::armory_base::roll_shape(base, seed)` rolls the shape: an accepted style straight from the family's `roll_as`, then the tweak. Unlike the plate it never retries seeds; see the first point under "Bases from the Armory".
2. **Game layer**: rarity, affixes, stats, footprint, history.

`fb_items::roll_item(base, seed)` does both: the shape, its profile (slot, tags, stats, footprint), then for gear the rarity and affixes, the name, a legend for a legendary, and the dress; goods stay common and plain, and a purse counts its coins. It is pure in the base and seed. The shape draws from the Armory's stream and the game layer from its own (`fb_items::game_rng`), so the game layer can change without changing any shape.

Every base is now made by the Armory; the plate's laced shirt, leather jerkin and pointed hat are no longer drawn outside it. See "Bases from the Armory" below.

### Bases from the Armory

The plate kept its base-to-family map, `SAB`, as JavaScript: each base named a family, an `acc` filter on the rolled state and an optional `tw` tweak. `tools/extract_data.js` wrote the filters out as `s=>arr.includes(s[k][f])`, losing which field and styles they named, and the tweaks set fields (`food.color`, `food.color2`) that the typed Armory states have since renamed. So `data/sab.json` is gone and the map is typed Rust, `fb_items::armory_base::ARMORY_BASES`: each filter is a list of the family's own style enum and each tweak a function on the typed state, so a renamed style or field fails to compile. Where the port departs from the plate:

- The plate rolled and retried (`SA.roll(fam, seed, acc)`, seeds `(seed + i*7919) % 999983 + 1`, up to 120 tries). A base here picks one of its styles uniformly and rolls it with the family's `roll_as`, so it lands first time, and club and axe archetypes, which the Armory does not store, can still be chosen. The plate's retries weighted styles by the family's roll weights; picking uniformly is this port's own choice.
- `Club` takes any club, `Hand axe` a hatchet, hand axe or bearded axe (the one-handers; broad, long, battle and horseman's axes are left to later bases). `Torch`, `Lantern` and `Belt` take any of their family; the plate's belt is a container, so its belt pouch is always on. `Trousers`, `Breeches` and `Leggings` take their legwear style, `Hood` a hood or cowl, `Mail coif` a mail coif.
- The goods are food or sundries: `Glowcap` is a mushroom given a glowing cap if it rolled dull, `Waterskin` and `Lamp oil` (a flask) are always filled, and lamp oil's amber colour is this port's own. `Rope`, `Lockpicks`, `Candle` and `Bandages` are their sundry kinds, and a `Spare torch` is a torch that has never been lit.
- `Blind cavefish` sets the fish's skin (`outer`) and fins (`accent`), the plate's `color` and `color2`. `Healing draught` sets the potion's `liquid` to the healing brew and tints its glass toward it as the Armory tints every potion.
- The plate's native bases become near styles, named for the base rather than the Armory's noun: a `Shirt` is a short, unbelted tunic with straight long sleeves; a `Leather jerkin` is a brigandine cut in a leather colour from `data/leather.json`, riveted one time in three for the plate's "Studded". A `Pointed hat` is the Armory's pointed hat style (see Hats below).
- Level loot names only a family: `fb_level`'s item spots carry an Armory family and a seed. `fb_items::roll_loot(family, seed)` picks one of that family's bases uniformly from the seed (`base_for`) and rolls it from the same seed. `fb_level` keeps its family weights and stays independent of `fb_items`, and the layout of every level seed is unchanged.

### Clubs and axes

Clubs and axes are now Armory families (`armory::families::club`, `armory::families::axe`), after daggers in `Family::ALL`. They have no prototype fixtures, since the prototype's plate faked them outside the Armory. Where the Rust port departs from the plate:

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

### Hats

Pointed hats, wide-brimmed hats and felt caps are the Armory's `hat` family (`armory::families::hat`), new to the Armory, drawn on the same `HeadForm` as hoods. They are a family of their own rather than hood styles: a hood is one draped sheet from the crown to the shoulders, and none of its controls (cape, face opening, liripipe, ties, ventail, mail) mean anything to a stiff crown and brim, while a hat's (crown height, brim width and set, band, feather) mean nothing to a hood. The plate groups them as "Hoods and hats" only on its shelf. Where the port departs from the plate:

- The plate draws its `Pointed hat` as a torus brim under a cone swept along a bent spline, with a band. Here the crown is raised from the head's brow line (`HeadForm::brow_point`), so it follows the head's oval and tips back a little as hats are worn, and is kept off the head where its profile would cut in. A bent crown turns each row square to its spine, so a slumping tip keeps its girth. The brim is a sheet bent along a circular arc: flat, drooping, or turned up against the crown.
- Pointed hats slump at the tip over a modest brim. Wide-brimmed hats have a rounded crown and a broad brim that may droop (a Slouch Hat), often a band and sometimes a feather (a Plumed Hat). Caps sit low over the crown with a turned-up brim or none; a cap whose top falls forward is a Phrygian Cap. `crumple` dents the crown and waves the brim, as hand-blocked felt does.
- The `Pointed hat` base takes the pointed style and the Armory's noun. The level's loot weights give hats 2, so their only base, the pointed hat, now lies about; wide-brimmed hats and caps wait for bases of their own.

### Belts

Belts are the Armory's `belt` family (`armory::families::belt`). The strap is the garment's `belt` section (width, leather, buckle), so a belted tunic and a belt base share it; the fittings are the port's own: plain, studded, sword belts slung toward the left hip with one or two frogs, girdles with a long tail, bar mounts and a strap end, and pouch belts with loops and D-rings. Where the port departs from the plate:

- The plate draws a band with a torus buckle and an ellipsoid pouch. Here the strap has its section, the buckle its frame and tongue, the free end runs through a keeper, and the plate's `holds` belt pouch is drawn as the bags family's own belt pouch (the state's `pouch`), hung behind the right hip with the belt through its loops.

### Food and drink

The Armory's food family (`armory::families::food`) now also makes the glowcap, the waterskin and the lamp-oil flask from `data/goods.json`, as the `mushroom`, `waterskin` and `flask` styles. Where the Rust port departs from the prototype:

- The prototype's `cut` toggle is a portion: whole, cut (a wedge out of a wheel, pie or round loaf, the end off a loaf, a slice out of an apple), a single piece (a slice of bread, a wedge of cheese, a slice of pie) or bitten (bread, fruit, a roast). Cut faces show a band of crust, rind, skin or pastry round the crumb, paste, flesh or filling. `Blind cavefish` and `Healing draught` tweaks set colours, which now have named roles (`outer`, `inner`, `accent`, `liquid`) instead of `color` and `color2`.
- A potion's brew is typed, and the noun still follows the liquid's colour, so a retinted potion is named for the brew it looks like.
- Glass is its own material (`MaterialKind::Glass`), drawn see-through so the drink inside shows; liquids use the gem material in their own tint. A potion's glass is still tinted toward its brew. Cork is tinted wood, wax seals tinted bone, a bottle's paper label tinted cloth.
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
- An affix applies only where its `on` tag matches the item's tags (`cloth`, `leather`, `metal`, `wood`, `wear`, `armor`, `quiet`, `weapon`, `light`, `trinket`, `container`, or `any`). Tables: `data/afx.json`. Each `fx` is a list of typed effects (see "Affix effects" below).
- When affixes are chosen by hand (from the armory), rarity follows from them: two good affixes make it rare, one makes it magic, none makes it common.
- Name: `<prefix> <Armory noun> <suffix>`. The noun comes from `forgeNoun` (Longsword, Rondel Dagger, Kettle Hat).
- A **legendary** gets a title (`<owner>'s <noun>` or `The <adjective> <noun>`). The original name becomes its subtitle, and it gets three lines of history: where it was made and by which faction, who carried it and what they did, and how it was lost.

### Affix effects

Each affix's `fx` in `data/afx.json` is a list of `fb_items::modifier::Modifier`s acting on `fb_items::stats::Stats` or on the Armory `Condition`: `add` or `scale` a stat, `damage_or_armour` (the plate's Rusted: damage bonus on a weapon, armour on anything else), or `condition`, which raises a condition slider to at least a value. `Stats` is the minimal set the affixes need; deriving it per family is still to do. Amounts and distances never fall below zero (the plate's `Math.max(0, ...)` for warmth and armour, applied to every one); bonuses such as stealth and damage can.

Where the effects come from:

- The plate's `fx` source: Warm, Featherlight, Hardened, Keen, Gleaming, Moth-eaten, Rusted, Smoky, of the Hearth, of the Ox. Featherlight keeps the weight exact where the plate rounded it to 0.1 lb; rounding is left to the card.
- `dressState`'s condition for flaws, so a flaw's wear travels with the affix: Rusted raises wear to 0.75 and rust to 1.5; Moth-eaten, Mildewed and Lice-ridden raise wear to 0.6, fray to 1.4 and rot to 1.2; Cracked raises wear to 0.6 and rot to 1.5.
- The card text, for affixes the plate gave no `fx`: Silent, Squeaking and of Quiet Steps (stealth ±2), Balanced (+1 to hit), Long-burning (burn time ×2), of the Mole (darkvision 10 ft), of Plenty and of Wakefulness (hunger and fatigue ×0.85), of Thieves (lockpicking +2), of the Leech (heal 1 on a kill).
- These numbers are this port's own proposals, since the text gives none: Mildewed and Lice-ridden multiply disease risk by 1.5, of Lesser Warding by 0.5, and Cracked adds a 10% chance to break on a hard blow. Lice-ridden's itching and Smoky's stinging eyes are not simulated yet.

`fb_items::naming` writes the names and legends. The noun is the Armory's, or the base's own name for the bases marked `named_by_base` in `fb_items::armory_base` (the plate did this for `Blind cavefish` and `Healing draught`). The plate's material word for its own bases ("Linen Shirt", "Oak Club") is gone, since those bases are Armory shapes now and the Armory noun already says what they are. A legendary's title takes its epithet from the plate's lists (Tooth, Oath, Wick, Keepsake...), chosen by whether it is a weapon, a light, a trinket or anything else, and the owner's name is a first name from `data/syl.json`. The title uses a straight apostrophe where the plate used a curly one, because the HUD's font has no curly one.

## Rarity shows on the object

`dressState` sets the Armory condition from rarity: common wear 0.15–0.50, magic 0.06–0.26, rare 0.02–0.14. A legendary is either pristine (0.02) or old and worn (0.4–0.7). Rare and legendary items get etching; legendary etching is gold inlay. Flaws raise the matching decay: Rusted sets rust to 1.5, and Moth-eaten, Mildewed and Lice-ridden raise fray and rot.

`fb_items::dress::dress` does this from the plate's own stream (`mulberry32(seed * 3 + 1)`). Where it departs from the plate:

- A flaw's decay comes from the affix's own `condition` effects (see "Affix effects"), applied after the rarity's wear, rather than from a list of names inside `dressState`.
- Food, torches, lanterns and sundries have no etching in the Armory, so a rare one is only cleaner.
- The plate also turned a legendary's gem glow up to 0.8. The Armory keeps gems in a different section in each family (a mace's head gem, a ring's or amulet's stone, a coffer's crowning stone), so that is left for later.

## Stats and footprint (`saMeta`)

Each family maps to a slot, tags, weight, armour, warmth and damage, plus a grid footprint from the item's real size: `cells(units) = clamp(ceil(units / 2.4), 1, 9)`, about 24 cm per cell. A staff, spear or bow longer than 14.4 units (six cells) cannot be packed. Bags define their inner grid: backpack 8×5, satchel 6×3, sack 4×4, belt pouch 4×2.

`fb_items::profile::profile(base, &shape)` does this. Where it departs from the plate:

- A base with a row in `data/bases.json` or `data/goods.json` (Club, Torch, Bread, Rope...) takes its slot, tags, weight, armour, warmth, damage, light, stack and card line from that row. Every other base derives them from its shape as `saMeta` does.
- The footprint is measured from the built mesh (`ItemMesh::bounds`): rows from the longest extent, columns from the next. The plate measured only a few fields per family (blade plus grip, shield size) and fixed the rest. Garments, hoods, legwear, gloves, belts and bags fold, so they keep the plate's folded footprints instead of their size on the mannequin.
- The pack limit uses the same longest extent, so a staff's head or a bow's string counts.
- The plate's shields had tower, buckler and everything else; the port's pavise counts as a tower (armour 3, 15 lb).
- Chests cannot be packed, as the plate's `long` flag had it.

## Slots

`data/slots.json` lists the slots: head, neck, shirt, shoulders (cloak), armour (body), main hand, hands, feet, back, belt, off hand, legs, two rings and a charm. Rings fill whichever ring slot is free.

## Containers

A bag keeps its contents wherever it goes, and its weight counts through every bag you carry. A bag set down opens underfoot as its own grid. A bag cannot go inside itself or inside a bag it already holds.

The Armory says where a container's contents go: each container family has a `container(&state)` returning an `armory::build::container::ContainerSpace` (the mouth's centre, facing and size, the interior bounds, and the capacity in litres) in the same item space as its mesh. The game can drop items through the mouth or draw an open container's contents inside it. The capacity is the real volume of the modelled body; the inner grid sizes above stay a game rule.

Departures from the prototype's bags: the front pocket and bedroll roll only on backpacks and spilled coins only on pouches and purses (the prototype rolled them everywhere but drew them only there); a sewn patch can go on any bag, not just sacks; cloth bags take any woven pattern and leather ones can be studded. As in the prototype, the pattern goes on the body (and a backpack's pocket) in the second colour, while the flap, patch and bedroll stay plain in that colour. Every panel has a real thickness, straps lie on the surfaces they cross and close with buckles, and a satchel's strap hangs from D-rings.

Departures from the prototype's chests: each style rolls only the options it shows. Iron hoops go on barrels, shipping straps and rope handles on crates (the prototype ignored `bands` and `handles` there), and `lock` is an amphora's lid. Coffers and strongboxes can stand open as well as chests. A coffer's crowning stone is a gemstone in the state, not the second colour, and stencils can go on barrels as well as crates. A lidded box is hollow, with walls, a rim and a darker inside, so an open lid shows a real interior with the hoard heaped in it. Its lid turns on hinges and carries its hasp and bands. An amphora is clay (the stone material) tinted with its glaze colour and painted with its glaze pattern; the glaze is a `Fabric`, like a bag's cloth, so both reach the shader as a `SurfacePattern`. Stencils are the paint material.

## The inventory screen

Tab opens the plate's inventory leaf over the level (`game/src/inventory/`): a parchment ledger with each carried container as its grid, items drawn as icons at their footprints, dragged between cells and grids and turned with R or the right mouse button, as in the plate's `renderGrids` and drag code. The level keeps drawing behind it; the mouse is freed and the body stops. The grid rules (`inventory::grid`) and where every item is (`inventory::items::Inventory`: worn, in a container's grid at a cell and turn, or loose on the floor) are plain Rust. Where it departs from the plate:

- The delver starts wearing a belt, and its 4×2 pouch is the first grid. The plate started from an outfit; there is no outfit yet.
- Carried grids are the containers worn on the back and the belt, in that order (the plate's `containersOf`). Picking up a bag whose slot is free puts it on, so a pack found on the floor becomes a grid at once; anything else goes into the first grid with room for it, upright first and then turned (the plate's `stow` and `place`). Something with no room for it, or too long to pack, stays on the floor. Wearing a bag on pickup is this build's own rule.
- The carried list in the corner of the HUD, and choosing from it with the number keys and the wheel and setting down with G, are gone: the screen does all of that.
- Icons are rendered from each item's Armory mesh in the toon style, one camera per icon drawing into an image the shape of its footprint at 92 px a cell, posed with its longest side along its footprint's rows, then turned and tilted as the plate's item viewer looks (`(.32, .3, 1)`). Each is drawn for a second and kept, keyed by base, seed and turn. The plate drew its icons the same way into a cached canvas (`iconFor`).

## Classes

`data/outfits.json`: Fighter, Ranger, Mage and Cleric, plus Freshly fallen and Scavenger. Each slot lists candidate bases, optionally with a leading probability. A class's set is rolled once per character, so the outfit you preview is the one you get.
