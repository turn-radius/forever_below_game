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
