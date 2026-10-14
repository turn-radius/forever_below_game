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
