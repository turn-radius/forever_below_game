# Forever Below

A first-person, stylized-3D dungeon crawler in the spirit of Ultima Underworld, built in Rust with Bevy 0.20. You come through a door into an endless Stygian abyss, and it shuts behind you. The way out is a rumor the world keeps feeding you. You delve, settle a level for a while, earn favor with its factions, then go deeper. Death sends you back to the beginning, but the items you leave behind stay in the world.

Read `HANDOFF.md` for the current milestone, and `docs/` for the design notes. The browser prototypes in `reference/` show the idea: the look, the item families, the paper doll and the rules. They are a starting point, not a spec to match. The Rust build should go well past them, with higher-fidelity items and characters than three.js allowed.

## Workspace

```
crates/procgen_rng   seeded RNG for every generator
crates/armory        Spline Armory in Rust: item state from a seed, mesh data from a state
crates/fb_items      Forever Below items: rarity, affixes, stats, footprints, history
crates/fb_body       procedural folk bodies and the fitting of worn and held gear
game/                the Bevy app (binary: forever_below)
fixtures/            example generator output from the prototypes (reference only)
data/                game tables extracted from the plate prototype (JSON)
reference/           the prototypes and concept art
tools/               scripts that regenerate fixtures and data from reference/
docs/                design, rendering, items, body
```

## Rules

- **Generators never depend on Bevy.** `procgen_rng`, `armory`, `fb_items` and `fb_body` are plain Rust: data in, data out (parameters, plain mesh buffers). Only `game/` uses Bevy. Heroes' Hold and a native Spline Armory will reuse these crates later.
- **Generators are pure functions of a seed.** The same seed must give the same item on every machine and in every save, because items persist and carry history across runs. Use `procgen_rng::Mulberry32` (or another seeded RNG in that crate), never `rand` or thread RNGs, inside generators. Matching the browser prototypes number for number is not a goal.
- **Improve freely on the prototypes.** Keep their intent (the families, the controls that make items varied, the rules in `docs/`), but rebuild the geometry and materials at higher fidelity: denser and better-shaped meshes, proper normals, real lighting, better hands and faces, cloth that drapes. If a better approach breaks with how the prototype did something, take the better approach and note it in `docs/`.
- **One item generator.** The `armory` crate makes the shape of every item. `fb_items` adds rarity, affixes, stats and history on top. Game code never builds its own one-off item shapes.
- **Units: 1 world unit = 10 cm**, as in the prototypes. A human is about 17 units tall.
- **Bevy 0.20.** Its APIs change every release. Check the 0.20 docs and migration guides (docs.rs/bevy/0.20, bevy.org) and do not rely on memory of older versions. Custom shaders are WESL (`.wesl`), since the old WGSL preprocessor dialect was removed in 0.20.
- Tests: `tools/test.sh` runs the whole suite, each test binary once and all of them side by side (arguments pass through: a name filter, `--include-ignored`). Don't use `cargo nextest` on the work machine: it starts a process per test, and its endpoint security vets every launch in one queue (about 0.27 s each), so 1,700 tests wait minutes to start. `cargo test -p <crate>` is fine for one crate. Slow sweeps are `#[ignore]`, each with a default-run `_smoke` twin; run them (`tools/test.sh --include-ignored`) before merging fitting changes. The run ends with its five slowest binaries and warns past two minutes: keep a new test out of that list, and time anything with the thread's CPU time, not the wall clock. Run the game: `cargo run -p forever_below`.
- Build caches: every worktree keeps its own `target/`, so parallel builds never wait on each other. Agents and other one-shot builds set `CARGO_INCREMENTAL=0`, since they never reuse the incremental cache (a few GB per worktree). `tools/prune-targets.sh` deletes the `target/` of worktrees under `.claude/worktrees/` that are merged into `origin/main` and not building (`--idle HOURS` adds ones untouched that long, `--dry-run` only lists them); run it when the disk runs low or a batch of PRs merges.

## Game decisions already made

- First person. Death restarts the run. A possible ending retires the character.
- Classes: Fighter, Ranger, Mage, Cleric. Freshly fallen and Scavenger are starting looks.
- Folk: human, dwarf, gnome, goblin, lizardfolk, pigfolk. Enemies are classic D&D monsters. D&D coinage. No mining.
- Diablo-style rarity and affixes that can raise or lower any parameter, survival ones included.
- Cataclysm-style simulation for every character: light, warmth, food, water, sleep, dirt, disease, limb health, encumbrance.
- No reforging. Items change only through finding, carrying, wearing and wear and tear.
- Containers keep their contents wherever they go. A bag set down opens underfoot as its own grid.
- Quests come from NPC needs. Keeping a quest's reward instead of finishing it lets the quest lapse and costs faction standing.
