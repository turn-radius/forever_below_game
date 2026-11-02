# Denizens

## Tongues

Minotaurs, cyclopes, fauns and demons do not speak human languages. Each `DenizenKind` has a tongue (`crates/fb_denizens/src/tongue/`): Minotaur speech, Cyclopean, Sylvan and Infernal. The delver is born knowing none and learns them from books and scrolls found in the abyss. Until a line's words are learned it shows as glyphs, and talk is limited to gesture.

- **Vocabulary.** Forty-two words in seven topics (greeting, farewell, kin, trade, quest, place, threat), the same keys and meanings in every tongue. Rust constants in `tongue/vocabulary.rs`.
- **Native words.** Each tongue has a phonology (minotaur: bovine and guttural; cyclops: booming and hard; faun: lilting, open syllables; demon: harsh and sibilant). A word's spelling is rolled from the kind and the word's key alone, so it is the same in every world and save, and adding or reordering words never respells another.
- **Lines.** A line of talk names the words it needs: `needs(kind, topic)` gives a topic's core words (the first three), `needs_with` adds more by key. `Lexicon::readable` is true once all are learned.
- **Lexicon.** The words learned per tongue, saved with the delve record; they stay learned for the rest of the delve. The next delver starts with none.
- **Barter and quests.** `can_barter` needs the greeting and trade phrases; `can_take_quests` needs the greeting and quest phrases. `known_share` is for display only.
- **Teaching.** A scroll teaches every word of three or four topics, a book of five to seven, rolled from the item's seed. The card reads like "Minotaur phrases: greetings and trade" and names the topics it rolled. Reading swaps each covered topic already fully known for the next topic not yet known (table order), so any three scrolls of a tongue, in any order, teach all of it, and a complete tongue learns nothing more ("There is nothing new in it."). Each copy teaches once: it is marked read on the item (`Teaching::studied`, kept in saves), and reading it again says "You have learned all this writing can teach."; another copy still teaches.

## Writing in a tongue

Code: `fb_items::tongues`, `Level::floor_item`, `game/src/reading.rs`. All odds are our own, for a designer to tune.

- **Split.** A book teaches an ability score (`Item::lesson`) or, written in a tongue, words (`Item::teaching`), never both. A scroll can teach words and otherwise nothing. A note never teaches. The teaching is stored on the item (`Item::teaching`, in `ItemParts` and `ItemRecord`), so an item that is changed or recorded keeps it; it is rolled once, when the item is found, from its own salted stream, so no shape or ability lesson moves.
- **Where.** Only at depth 10 and below. Beside a denizen faction's ground (inside a claim's rooms or within 10 cells of one; floor items leave claimed rooms, so in practice just outside) a scroll is in its tongue 50% of the time and a book 70%; elsewhere 6% and 20%, in the tongue of one of the level's denizen factions (any people if it has none). Each denizen claim also gets two or three extra books and scrolls in the nearest free rooms (`tongue_writing`, its own stream, depth 10 and below only), so levels above are unchanged.
- **Reading.** F on a teaching book or scroll adds its words to the `Lexicon` in `WorldRecord` and toasts "You learn 12 words of Minotaur speech: greetings and trade." or "You already know these words." Words stay learned for the delve and its saves; a new delve starts empty. The sheet lists each tongue known with its share, and what it allows (barter, quests).
- **Rumours and knowledge.** Folk near the top have little chance of knowing the denizens; folk deeper do. Above depth 10 a note carries a rumour of a denizen people instead of the level's usual words, with chance `RUMOUR_ODDS_TOP` (5%) on level 1 rising evenly to `RUMOUR_ODDS_DEEP` (60%) on level 9 (`tongues::rumour_odds`). At depth 10 and below, `KNOWLEDGE_ODDS` (40%) of notes on a level with a denizen faction speak of it plainly, by name and kind ("The Red Maze hold the halls ..., minotaurs every one"), from `knows_<kind>` in `data/rumours.json` (`tongues::knowledge`). Both are in the reader's own language, read from the note's own salted stream, and teach nothing.
- **Containers.** Chests and crates (`Level::container_contents`) roll writing the same way from their own position, so a denizen camp's chests hold writing in its tongue; a corpse holds no loot table, so none is rolled there. Notes (`Level::books`) tell of a book as it is found, so a lesson a tongue replaced is not named.
- **Captures.** `FB_TEACH=minotaur:scroll:7` puts one in the pack; `FB_TEACH_READ=1` reads it a moment in.

## Unreadable speech

`glyphs(kind, text, seed)` turns a line into its tongue's script: letters and digits become glyphs, spaces and punctuation stay. The scripts are borrowed letters, because the UI's running-text face (Alegreya Sans, which the talk panel uses) has no runic or invented-script blocks and Almendra has next to no Greek: Greek for minotaurs, Cyrillic for cyclopes, macron and acute Latin for fauns, caron and stroke Latin for demons. A test reads each Alegreya face's character map and checks every glyph.

## Bodies

A denizen is a race of the folk body (`fb_body`), not a separate creature rig: `Folk::Minotaur` has a row in `data/races.json` like the six folk, and its body, skin, rig, gait, `fit_worn`, `fit_held`, hair and head fitting are the folk's own. `Folk::ALL` stays the six folk (so factions and members roll as before); `Folk::DENIZENS` lists the denizen races built so far, with `Folk::denizen_body()` and `Folk::of_denizen()` mapping to and from `DenizenBody`. See "Denizen parts" in `docs/body.md`.

The Faun is `Folk::Faun`: about a human's height, lithe, with a human torso, arms and face (slightly pointed ears, curly hair, a beard in four of ten), short curled goat horns, goat legs from the hip (fur in brown, black, grey or tawny on the thighs and shanks only, a backward-bending hock, small cloven hooves) and a short upright tail. The SRD has no faun; the nearest is the Satyr (SRD 5.1 p. 344): Medium fey, chaotic neutral, Armor Class 14 (leather armor), 31 hit points (7d8), speed 40 ft., Str 12, Dex 16, Con 11, Int 12, Wis 10, Cha 14, skills Perception +2, Performance +6, Stealth +5, passive Perception 12 (no darkvision), Common, Elvish and Sylvan, Magic Resistance (advantage on saves against spells and other magical effects); a ram +3 for 6 (2d4 + 1) bludgeoning, a shortsword +5 for 6 (1d6 + 3) piercing and a shortbow +5 for 6 (1d6 + 3) piercing. Its row's `stats` are the ability scores, `dv` is 0; the rest waits for the denizen combat task.

The Minotaur's SRD stat block (5.1 p. 333) is Large, Armor Class 14 (natural armor), 76 hit points (9d10 + 27), speed 40 ft., Str 18, Dex 11, Con 16, Int 6, Wis 16, Cha 9; a greataxe hit is +6 for 17 (2d12 + 4) slashing and a gore +6 for 13 (2d8 + 4) piercing. The ability scores are the race row's `stats`; the rest waits for the denizen combat task.

### The demons

Demons come in three bodies (`Folk::WingedDemon`, `Folk::HornedDemon`, `Folk::Succubus`, rows `winged_demon`, `horned_demon`, `succubus`), all races of the folk body with horns, a spade tail and claws or nails; hair colours are in `data/hair_demon.json`, names in `data/syl.json`. Each row's `srd` field names the stat block (SRD 5.1, https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf, printed page numbers as in the document) and its `stats` and `dv` are its scores and darkvision.

| Body | Looks | SRD 5.1 block | Scores (Str Dex Con Int Wis) | AC, hit points, CR |
| --- | --- | --- | --- | --- |
| Winged demon | lean, 1.1 times a human, large wings, small swept horns, spade tail, hooves, red, black or ash skin | Vrock, p. 273 (Large fiend, demon) | 17 15 18 8 13 | 15, 104 (11d10 + 44), 6; speed 40 ft., fly 60 ft.; darkvision 120 ft. |
| Horned demon | big and brutish, 1.4 times a human, massive curling horns, heavy brow, spade tail, hooves, no wings, dark red or charcoal skin | Glabrezu, p. 271 (Large fiend, demon) | 20 15 21 19 17 | 17, 157, 9; speed 40 ft.; truesight 120 ft. |
| Succubus | human height, slender, small horns, small folded wings, spade tail, human legs, lilac to deep red skin, long hair | Succubus/Incubus, p. 349 (Medium fiend, shapechanger) | 8 17 13 15 12 | 15, 66 (12d8 + 12), 4; speed 30 ft., fly 60 ft.; darkvision 60 ft. |

The winged demon's wings and the succubus's make flight a combat matter for the denizen combat task; the bodies only draw them. The horned demon is the SRD's Glabrezu (Large fiend, demon, chaotic evil; 157 hit points is 15d10 + 75): a devil such as the Horned Devil is lawful and of Hell, not a demon of the abyss, so it was set aside. The body is kept as it is; the Glabrezu's pincer arms (its second pair) are not modelled. The Vrock's (winged demon) and the Succubus's blocks are demons too. The `srd` fields and `stats` of a race row are data for the denizen combat task, which has not read them yet.
