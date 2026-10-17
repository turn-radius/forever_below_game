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
