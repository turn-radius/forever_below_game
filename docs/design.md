# Forever Below: design

A copy of the seed design doc from the "Procedural Ultima" project, as of the handoff.

## Premise
A first-person, stylized-3D dungeon crawler in the spirit of Ultima Underworld. You come through a door into a Stygian abyss and it closes behind you. There is only one way out, and the abyss is infinitely deep. The way out is a rumor the world keeps feeding you, told differently by every culture.

## Core loop
Delve until you find a level worth inhabiting. Gain favor with its factions and make it your base for a time. Faction quests lead to rare items. You can keep a quest's reward instead of completing it; the quest expires and your standing drops. Then the abyss pulls you deeper.

## Systems
- Every character, you and NPCs alike, has skills, stats and an inventory, and is simulated: hunger, thirst, sleep, warmth, light, dirt, sickness, disease, limb health, pain, breath, encumbrance.
- Items persist in the world and carry history. Legendaries from earlier runs come back as found objects.
- Item stats are Diablo-style: rarity, plus prefixes and suffixes that can raise or lower any parameter.
- Lockpicking, sneaking, and breaking doors and containers by material and stats.
- Bartering depends on stats, skills, faction, personal relationship, scarcity, alignment and appearance.
- Quests are generated from NPC needs, alignments and goals.
- Folk: goblins, gnomes, humans, lizardfolk, dwarves, pigfolk. Enemies: slime cubes, skeletons, beholders, demons, ghouls, rats, snakes, bats, scorpions, spiders, giant hornets, worms, dragons, vampires, werewolves, tentacle monsters, liches, devils, ghosts.
- D&D coinage. No mining: resources come from what is in the abyss.
- Classes: Fighter, Ranger, Mage, Cleric. Freshly fallen and Scavenger are starting looks.

## Visual direction
- Spline Armory's renderer is the style guide (see `rendering.md`).
- Ultima Underworld's portrait-and-doll lineup is the character reference, at higher fidelity (`reference/concept/`).
- The inventory follows the parchment concept, restyled as a dark ledger so it does not glare over the abyss: engraved item icons in footprint grids, an underfoot drop zone, prepared outfits on invisible mannequins, and the doll inside a runic seal with slot boxes around it.

## Folk in the level (since the handoff)

Our own proposal; neither the seed doc nor the plate places anyone in a level.

- Each level holds three to ten folk (`fb_level::FolkSpot`), in groups of two to four, one faction to a room. A group stands round the room's brazier if it has one, otherwise round a point inside it, facing in as if talking. Nobody stands in the starting room, in a corridor, in a doorway or beside one, or on the loot.
- The factions are `data/factions.json`. Each leans toward a folk and a pair of outfits (`fb_level::FACTIONS`): the Lantern-keepers human clerics and mages, the Mudfoot clan goblin scavengers and fighters, the Brothers of the Last Door human fighters and clerics, the Scale Court lizardfolk fighters and rangers, the Hogsmeet traders pigfolk rangers and scavengers, the Gnome Guild of Gears gnome mages and scavengers, the Ash Choir dwarf clerics and mages. Three in four members are the faction's folk; the rest are any folk.
- Folk come from a third stream of the level seed, after the layout and the loot, so they never move a wall or an item.
- In the game (`game/src/folk.rs`) each spot is a dressed `fb_body` figure. Who they are is a pure function of the spot: a body from its folk and seed, a name from `data/syl.json`, and one of the faction's outfits rolled through `fb_items::outfit` (`data/outfits.json`, the plate's `preparedSet`, which also gives a bare-headed gnome a pointed hat). Anyone the outfit leaves empty-handed carries a torch. They are dressed by the same `game/src/figure.rs` as the doll (`fit_hands`, then `fit_worn`), with the head, hair and headgear split off to turn about the neck. A lit torch or lantern in hand lights the room.
- Standing, folk breathe (about 15 breaths a minute, each a little faster or slower) and now and then shift their weight. Without a skeleton the whole figure moves a little about its feet. Within 4 m they turn their head toward the player, up to 70 degrees either side, about the neck; under a hood, whose cape lies on the shoulders, the body turns a little instead. Each stands in a circle the player cannot walk into (`play::Obstacles`, `player::slide_around`).
- Looking at someone within reach (2 m, as for picking things up) shows a card under the crosshair in the item card's style: their name, folk and faction ("Dwarf of the Ash Choir"), and what they wear and hold, each item's name in its rarity's colour. The item card wins when both are in view.

## Props in the level

Our own proposal; neither the seed doc nor the plate furnishes a level.

- Each level stands containers and scenery along the walls of its rooms (`fb_level::PropSpot`): Armory chests, barrels, crates and amphorae (`Family::Chests` styles, each with its own seed and a loot seed for what it holds), and rubble, bones and pillars (`armory::build::scenery`). Strongboxes and coffers stay small loot on the floor.
- Props come from a fourth stream of the level seed, after the layout, the loot and the folk, so they never move a wall, an item, a light or anyone standing about.
- A room holds about one prop per nine cells of floor, at most five, and one room in seven stands bare. Each stands with its back to a wall, or in a corner against two, far enough off the grid's wall face for the rock's relief and, for a chest, for its lid to swing open. Pillars stand only in corners. The first container in every level is a chest, and every level has at least one container.
- None stands in or beside a doorway, in a corridor, on the straight way in from the entrance, or near an item, a light or a folk. Treating every cell with a prop as rock, every other open cell is still reachable from the start, and every cell with a prop borders one that is, so a prop against its wall never closes the way.
- In the game (`game/src/props.rs`) a container is its Armory item, shut, drawn in its own palette, condition and etching like any item; scenery is drawn as the level's fixtures are. Scenery is a set of builders in `armory::build::scenery` rather than an Armory family, since it is never an item: no state, noun, turntable page or loot entry. Rubble is a heap settled in layers of roughened, faceted stone chunks with now and then a slab of fallen ceiling; bones are a skull with its eye sockets and nose pressed in (`geom::carve_hollows`), long bones turned on a lathe and curved ribs; a pillar is a faceted column with a slight swell on a stepped plinth, either whole with its capital bedded in the ceiling's relief or broken off short with a ragged top and chips of it lying on the plinth.
- Each prop rests at the lowest point of the drawn floor under it and blocks the way (`play::BlocksTheWay`, gathered into `play::Obstacles` with the folk): a circle for round things, a rectangle turned with it for chests, crates and pillars. Things set down from the inventory stop short of props as they do of rock.
- Looking at a container within reach and pressing the pick-up key opens it rather than picking it up: the inventory opens with the container's grid ahead of what lies underfoot, and a chest stands open in the level from then on, its lid raised (the Armory's `chest.open`). Every container is in the `Inventory` from the start, loose, filled from its loot seed, so what is left in it stays for the rest of the run (`docs/items.md`, "Containers").
- `print_level` marks containers `c` chest, `b` barrel, `x` crate and `a` amphora, and scenery `%` rubble, `~` bones and `o` pillar.
