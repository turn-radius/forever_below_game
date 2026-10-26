# Forever Below: character attributes

The delver has five attributes (D&D 5e SRD 5.1, p. 76): Strength, Dexterity, Constitution, Intelligence and Wisdom. Each ranges from 3 to 18, rolled from the folk's baseline in `data/races.json` nudged by a pseudo-random distribution and clamped.

## Ability modifiers

Each attribute score has a modifier: `floor((score - 10) / 2)`. A score of 3 gives -4, a score of 10 or 11 gives 0, and a score of 18 gives +4.

In the game now:
- STR modifier: carrying capacity (STR × 15 lb).
- DEX modifier: Armour Class (AC = 10 + DEX modifier + item armour).

Planned (#122):
- CON modifier: hit points and saving throws.
- INT modifier: crafting and arcane knowledge.
- WIS modifier: perception and saving throws.

## Folk baselines

The D&D 5e SRD does not assign a Wisdom score to its folk. Here, each folk's Wisdom ties to a documented trait (see `data/races.json`) or defaults to 10 (average).

|        Folk | STR | DEX | CON | INT | WIS |                             Justification |
|-------------|-----|-----|-----|-----|-----|------------------------------------------|
|       Human |  10 |  10 |  10 |  10 |  10 | Average across the board.                |
|       Dwarf |  13 |   9 |  10 |  13 |  13 | Darkvision (60 ft), insight trait.       |
|       Gnome |   8 |  12 |  13 |   9 |  12 | Darkvision (60 ft).                      |
|      Goblin |   8 |  14 |  10 |   9 |  11 | Darkvision (60 ft).                      |
|   Lizardfolk|  12 |  11 |   9 |  13 |  12 | Instinctive cunning (SRD 5.1, p. 99).     |
|     Pigfolk |  14 |   8 |   9 |  13 |  10 | Average (darkvision 30 ft).              |

## Alignment

The delver has an alignment, which sets where each faction's standing starts (`fb_faction::Standings::start_from_alignment`, design.md). The creator does not offer a choice yet, so it is the class's typical one (`Class::alignment`): Fighter lawful neutral, Ranger neutral good, Mage true neutral, Cleric lawful good. It is not saved with the delver; it follows from the class.

## Checks

A check (`fb_character::check`) rolls one die, usually a d20, plus named modifiers against a target, and meets it on a total at or over the target. Modifiers come from stats (the ability modifier), class, item affixes and faction standing; a zero modifier is left out. The roll is one draw from the seeded RNG and comes back with its breakdown as an `Explained`, so the screen, a tooltip and a test all read the same terms. Persuading folk and identifying items use checks, and the game shows each as a die rolling on screen.

Persuading ([`game/src/talk/persuade.rs`]): the Persuade topic rolls a d20 against 14 to be let through a locked door of the folk's faction, or 16 for a little coin when there is none, with Wisdom, class, worn charisma and standing (one per 25 points) as modifiers. A failure costs 2 standing, a Hostile faction cannot be persuaded, and each folk gives once.

Identifying ([`crates/fb_items/src/identify.rs`], [`game/src/identify.rs`]): magic and better items found in containers are unknown, shown as "Unknown ring" with their affixes hidden (what they do still counts). Y over one in the inventory rolls a d20 against 12, 15 or 18 by rarity with Intelligence and class as modifiers: meeting it reveals every affix, falling short reveals a share, falling 8 short reveals nothing. The item may be tried again, and what is revealed stays. The state is on the item record, so it lasts through saves, and records from before read as known.

