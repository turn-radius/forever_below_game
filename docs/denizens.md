# Denizens

## Tongues

Minotaurs, cyclopes, fauns and demons do not speak human languages. Each `DenizenKind` has a tongue (`crates/fb_denizens/src/tongue/`): Minotaur speech, Cyclopean, Sylvan and Infernal. The delver is born knowing none and learns them from books and scrolls found in the abyss. Until a line's words are learned it shows as glyphs, and talk is limited to gesture.

- **Vocabulary.** Forty-two words in seven topics (greeting, farewell, kin, trade, quest, place, threat), the same keys and meanings in every tongue. Rust constants in `tongue/vocabulary.rs`.
- **Native words.** Each tongue has a phonology (minotaur: bovine and guttural; cyclops: booming and hard; faun: lilting, open syllables; demon: harsh and sibilant). A word's spelling is rolled from the kind and the word's key alone, so it is the same in every world and save.
- **Lines.** A line of talk names the words it needs: `needs(kind, topic)` gives a topic's core words (the first three), `needs_with` adds more by key. `Lexicon::readable` is true once all are learned.
- **Lexicon.** The words learned per tongue, saved with the delve record; they stay learned for the rest of the delve. The next delver starts with none.
- **Barter and quests.** `can_barter` needs the greeting and trade phrases, or `BARTER_SHARE` (0.4) of the tongue. `can_take_quests` needs the greeting and quest phrases, or `QUEST_SHARE` (0.6). Both constants are for a designer to tune.
- **Teaching.** A scroll teaches every word of one or two topics, a book of three to five, rolled from the item's seed. The card reads like "Minotaur phrases: greetings and trade".

## Unreadable speech

`glyphs(kind, text, seed)` turns a line into its tongue's script: letters and digits become glyphs, spaces and punctuation stay. The scripts are borrowed letters, because the UI's running-text face (Alegreya Sans, which the talk panel uses) has no runic or invented-script blocks and Almendra has next to no Greek: Greek for minotaurs, Cyrillic for cyclopes, macron and acute Latin for fauns, caron and stroke Latin for demons. A test reads each Alegreya face's character map and checks every glyph.
