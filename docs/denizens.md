# Denizens

## Tongues

Minotaurs, cyclopes, fauns and demons do not speak human languages. Each `DenizenKind` has a tongue (`crates/fb_denizens/src/tongue/`): Minotaur speech, Cyclopean, Sylvan and Infernal. The delver is born knowing none and learns them from books and scrolls found in the abyss. Until a line's words are learned it shows as glyphs, and talk is limited to gesture.

- **Vocabulary.** Forty-two words in seven topics (greeting, farewell, kin, trade, quest, place, threat), the same keys and meanings in every tongue. Rust constants in `tongue/vocabulary.rs`.
- **Native words.** Each tongue has a phonology (minotaur: bovine and guttural; cyclops: booming and hard; faun: lilting, open syllables; demon: harsh and sibilant). A word's spelling is rolled from the kind and the word's key alone, so it is the same in every world and save, and adding or reordering words never respells another.
- **Lines.** A line of talk names the words it needs: `needs(kind, topic)` gives a topic's core words (the first three), `needs_with` adds more by key. `Lexicon::readable` is true once all are learned.
- **Lexicon.** The words learned per tongue, saved with the delve record; they stay learned for the rest of the delve. The next delver starts with none.
- **Barter and quests.** `can_barter` needs the greeting and trade phrases; `can_take_quests` needs the greeting and quest phrases. `known_share` is for display only.
- **Teaching.** A scroll teaches every word of one or two topics, a book of three to five, rolled from the item's seed. The card reads like "Minotaur phrases: greetings and trade".

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
