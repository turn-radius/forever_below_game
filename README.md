# Forever Below

A first-person delve into an endless Stygian abyss. Rust and Bevy 0.20.

Start with `HANDOFF.md`. Claude Code reads `CLAUDE.md` on its own. `cargo test --workspace` runs every test.

## Running the game

Everything runs from the repository root with `cargo run -p forever_below -- <options>`. The `--` separates cargo's own options from the game's.

### Play a level (the default)

```
cargo run -p forever_below                 # the level for seed 1207
cargo run -p forever_below -- --seed 42    # another level; every seed is a different layout
```

You come through a door into the first level, and it shuts behind you. The keys are listed in the game: press Esc for the menu, then Controls.

| Key | Does |
|---|---|
| W A S D, Shift | walk, hurry |
| Mouse | look round (click to take the mouse back after Esc) |
| E | pick up what you're looking at |
| I | inventory: drag items between your bags, the floor and your slots; R or right click turns a dragged item |
| Esc | close a screen, or open the menu (Resume, Controls, Settings, Quit) |
| − and = | music quieter and louder |

Settings (music and sound volume, mouse sensitivity, invert Y, fullscreen) are saved in `~/Library/Application Support/Forever Below/settings.json` on macOS.

To print a level's map without running the game: `cargo run -p fb_level --example print_level -- <seed>`.

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

### Screenshots and debug switches

These environment variables are for checking the game without playing it. Put them in front of the command, for example `FB_SCREENSHOT=shot.png cargo run -p forever_below`.

| Variable | Does |
|---|---|
| `FB_SCREENSHOT=<file.png>` | renders offscreen, saves one frame and quits; keyboard and mouse are ignored |
| `FB_WEAR=<state>` | turntable starting condition (`--wear` wins) |
| `FB_CLOSEUP=<0–11>` | turntable opens on a close-up of that grid slot |
| `FB_DEBUG_POSE=x,y,yaw,pitch` | holds the player at a grid point (cells), turned `yaw` degrees from north and tilted `pitch` degrees |
| `FB_DEBUG_CAMERA=start\|back\|shut\|top\|brazier\|glowcaps` | a lit camera at a fixed view of the level |
| `FB_DEBUG_LIGHT=1` | a dim light on the player's camera |
| `FB_OPEN_INVENTORY=1` | opens the inventory at the start |
| `FB_DEBUG_KIT=1` | a filled pack, with a sack and a shield set down in front of you |
| `FB_DEBUG_HOVER=<n>` | holds the hover card over the nth item in the first bag |
| `FB_OPEN_MENU=main\|controls\|settings` | opens the menu on that page |
| `FB_SETTINGS=<file.json>` | reads and writes settings from another file |

Running the built binary directly (`./target/debug/forever_below`) needs `BEVY_ASSET_ROOT=game` so it finds the music and fonts; `cargo run` finds them on its own.

### Generator tools

- `cargo run -p armory --example dump_obj -- [family] <seed> [out.obj]` writes an Armory item as OBJ.
- `cargo run -p fb_body --example dump_body -- <folk> <seed> > body.obj` writes a folk body as OBJ (human, dwarf, gnome, goblin, lizard, pig).
- `tools/README.md` covers regenerating the data tables and the soundtrack.
