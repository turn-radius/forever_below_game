# Forever Below

A first-person delve into an endless Stygian abyss. Rust and Bevy 0.20.

Start with `HANDOFF.md`. Claude Code reads `CLAUDE.md` on its own. `cargo test --workspace` runs every test.

## Running the game

Everything runs from the repository root with `cargo run -p forever_below -- <options>`. The `--` separates cargo's own options from the game's.

### The title screen (the default)

```
cargo run -p forever_below                         # the title screen
cargo run -p forever_below -- --seed 42            # skip it: straight into the level for seed 42
cargo run -p forever_below -- --title --seed 42    # the title, and New delve plays seed 42
```

The game opens on the title screen, to Forever Below's theme. New delve (or Enter) rolls a new level and a new character from the clock and starts the delve; the theme fades out as you go in. Settings is the pause menu's settings page (Esc or Back returns), and Quit leaves.

Giving `--seed`, or any debug switch that looks at the level (`FB_DEBUG_POSE`, `FB_DEBUG_CAMERA`, `FB_DEBUG_HOVER`, `FB_DEBUG_KIT`, `FB_OPEN_INVENTORY`, `FB_OPEN_CONTAINER`, `FB_OPEN_MENU`), skips the title and opens straight into the level, 1207 unless `--seed` says otherwise. `--title` keeps the title whatever else is given; `FB_SCREENSHOT=title.png cargo run -p forever_below -- --title` captures it, and adding `FB_OPEN_MENU=settings` captures its settings page.

### Play a level

You come through a door into the first level, and it shuts behind you. Folk of the level's factions stand about in groups; look at one up close to see who they are. The keys are listed in the game: press Esc for the menu, then Controls.

| Key | Does |
|---|---|
| W A S D, Shift | walk, hurry |
| Mouse | look round (click to take the mouse back after Esc) |
| E | pick up what you're looking at, or open the chest, barrel, crate or amphora you're looking at (its contents show beside what lies underfoot) |
| I | inventory: drag items between your bags, the floor and your slots (or onto the doll to wear them); R or right click turns a dragged item; double-click wears or takes off; drag the doll to turn it |
| Esc | go back: close a screen or page, or open the menu (Resume, Controls, Settings, Quit) |
| − and = | music quieter and louder |

Settings (music and sound volume, mouse sensitivity, invert Y, fullscreen) are saved in `~/Library/Application Support/Forever Below/settings.json` on macOS.

To print a level's map without running the game: `cargo run -p fb_level --example print_level -- <seed>`. It marks folk by their initial in capitals (`H` human, `D` dwarf, `N` gnome, `G` goblin, `L` lizardfolk, `P` pigfolk), containers as `c` chest, `b` barrel, `x` crate and `a` amphora, and scenery as `%` rubble, `~` bones and `o` pillar, and lists where each stands, which is handy for `FB_DEBUG_POSE`.

### The Armory turntable

```
cargo run -p forever_below -- --turntable
cargo run -p forever_below -- --family sword --seed 1207 --wear grave_find
```

A grid of twelve items from consecutive seeds, turning, to compare what the generator makes. `--family` alone also opens it.

- `--family <name>` starts on a family: staff, mace, sword, spear, dagger, club, axe, bow, crossbow, helm, hat, hood, garment, belt, hands, legs, feet, shield, ring, amulet, charm, bags, chests, food, torch, lantern, sundry.
- `--seed <n>` is the first seed on the page.
- `--wear <state>` sets the condition: pristine, serviceable, battle_worn, neglected or grave_find.

On the grid, the arrows select an item, PgUp and PgDn page through seeds, Tab and Shift+Tab change family, W steps the wear, Space pauses the spin, and Enter or F opens a close-up (drag to orbit, scroll to zoom, Esc to go back).

### The doll view

```
cargo run -p forever_below -- --doll
cargo run -p forever_below -- --doll --folk dwarf --seed 7 --outfit cleric
```

One folk's body, dressed in a rolled outfit and holding what it carries, turning on the turntable's plate.

- `--folk <name>`: human, dwarf, gnome, goblin, lizard (or lizardfolk), pig (or pigfolk). Human by default.
- `--seed <n>` rolls the body and the outfit's items; 1207 by default.
- `--outfit <key>`: fighter (the default), ranger, mage, cleric, fallen ("Freshly fallen") or scavenger, from `data/outfits.json`.

Tab and Shift+Tab change the folk, Left and Right the seed, Up and Down the outfit, and Space pauses the turn. `FB_SCREENSHOT` works here as on the turntable.

### Screenshots and debug switches

These environment variables are for checking the game without playing it. Put them in front of the command, for example `FB_SCREENSHOT=shot.png cargo run -p forever_below`.

| Variable | Does |
|---|---|
| `FB_SCREENSHOT=<file.png>` | renders offscreen, saves one frame and quits; keyboard and mouse are ignored |
| `FB_SCREENSHOT_FRAMES=<from>..<to>` | with `FB_SCREENSHOT=shot.png`, saves each frame from `from` up to (not including) `to` as `shot_<frame>.png` (`120..136` saves `shot_120.png` to `shot_135.png`; `120..=135` and a single `120` also work), to compare consecutive frames |
| `FB_WEAR=<state>` | turntable starting condition (`--wear` wins) |
| `FB_CLOSEUP=<0–11>` | turntable opens on a close-up of that grid slot |
| `FB_DEBUG_POSE=x,y,yaw,pitch` | holds the player at a grid point (cells), turned `yaw` degrees from north and tilted `pitch` degrees |
| `FB_DEBUG_CAMERA=start\|back\|shut\|top\|brazier\|glowcaps` | a lit camera at a fixed view of the level |
| `FB_DEBUG_LIGHT=1` | a dim light on the player's camera, and on what is held |
| `FB_OPEN_INVENTORY=1` | opens the inventory at the start |
| `FB_OPEN_CONTAINER=<n>` | opens the level's `n`th container (0 the first, in `print_level`'s order) a moment in, with the inventory showing what it holds |
| `FB_DEBUG_KIT=1` | dresses you in your class's outfit with a lit light in the off hand, a filled pack, and a sack and a shield set down in front of you |
| `FB_HOLD=<main>[,<off>]` | puts an item of each named base in that hand, after the starting or debug kit (`Mace,Torch`, `Sword,Shield`, `Staff,none`; a torch or lantern is lit, `none` empties the hand) |
| `FB_FOLK=<folk>` | makes the delver one of that folk (human, dwarf, gnome, goblin, lizard, pig) |
| `FB_DEBUG_HOVER=<n>` | holds the hover card over the nth item in the first bag |
| `FB_OPEN_MENU=main\|controls\|settings` | opens the menu on that page; with `--title`, `settings` opens the title on its settings page |
| `FB_SETTINGS=<file.json>` | reads and writes settings from another file |

Running the built binary directly (`./target/debug/forever_below`) needs `BEVY_ASSET_ROOT=game` so it finds the music and fonts; `cargo run` finds them on its own.

### Generator tools

- `cargo run -p armory --example dump_obj -- [family] <seed> [out.obj]` writes an Armory item as OBJ.
- `cargo run -p fb_body --example dump_body -- <folk> <seed> > body.obj` writes a folk body as OBJ (human, dwarf, gnome, goblin, lizard, pig).
- `cargo run -p fb_creatures --example dump_creature -- <rat|giant_rat|viper|constrictor> <seed> > rat.obj` writes a creature at rest as OBJ.
- `tools/README.md` covers regenerating the data tables and the soundtrack.
