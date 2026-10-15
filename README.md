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

Giving `--seed`, or any debug switch that looks at the level (`FB_DEBUG_POSE`, `FB_DEBUG_CAMERA`, `FB_DEBUG_HOVER`, `FB_DEBUG_KIT`, `FB_DROP`, `FB_OPEN_INVENTORY`, `FB_OPEN_CONTAINER`, `FB_OPEN_MENU`), skips the title and opens straight into the level, 1207 unless `--seed` says otherwise. `--title` keeps the title whatever else is given; `FB_SCREENSHOT=title.png cargo run -p forever_below -- --title` captures it, and adding `FB_OPEN_MENU=settings` captures its settings page.

### Play a level

You come through a door into the first level, and it shuts behind you. Folk of the level's factions stand about in groups; look at one up close to see who they are. The keys are listed in the game: press Esc for the menu, then Controls.

| Key | Does |
|---|---|
| W A S D, Shift | walk, hurry |
| Mouse | look round (click to take the mouse back after Esc) |
| Left click | swing what your main hand holds (a weapon in the off hand is held but does not swing yet), each blow from one of the directions it suits and arcing to the creature in reach: mostly overhead for a mace, axe or club, from every side for a sword, flat across for a staff, a stab for a spear or dagger, a punch with nothing (a click while the mouse is free only takes it back) |
| E | pick up what you're looking at, or open the chest, barrel, crate or amphora you're looking at (its contents show beside what lies underfoot) |
| F | equip what you're looking at straight from the floor: worn gear in its slot, a weapon in the main hand (or the off hand, if only that is free), a torch, lantern or shield in the off hand, a ring on a free finger; what it replaces goes into a bag, or down where the new thing lay; a lantern goes in a free hand, else at the hip if you wear a belt, where it lights the way with both hands full |
| L, Shift+L | light or put out the torch or lantern you carry, the one in hand first, then the one at your hip; Shift+L acts on the hip lantern alone. In the inventory, L lights or puts out the torch or lantern under the mouse, wherever it lies; its card says which. A spent torch will not light. Every lit light you carry shines |
| I | inventory: drag items between your bags, the floor and your slots (or onto the doll to wear them); R or right click turns a dragged item; double-click wears or takes off; drag the doll to turn it |
| Esc | go back: close a screen or page, or open the menu (Resume, Controls, Settings, Quit) |
| − and = | music quieter and louder |

Rats and snakes notice you, close in and bite. Your hit points show low on the left ("HP 8/12", a 1st-level character's from the SRD), and the edge of the view flashes red when you are bitten; a poisonous snake's venom is halved on a Constitution save, and a constrictor holds you, slowed, until you strike it. A blow lands on the nearest creature whose body (head, legs, tail, any part of a snake) is within your weapon's reach ahead, for its damage dice; a thrust drives the point at the crosshair. Struck creatures flash, flinch and squeak or hiss, and at 0 they fall and their bodies stay where they lie. At 0 hit points you fall in the abyss and go back to the title. Combat is a placeholder until the character simulation is designed.

Settings (music and sound volume, mouse sensitivity, invert Y, fullscreen) are saved in `~/Library/Application Support/Forever Below/settings.json` on macOS.

To print a level's map without running the game: `cargo run -p fb_level --example print_level -- <seed>`. It marks folk by their initial in capitals (`H` human, `D` dwarf, `N` gnome, `G` goblin, `L` lizardfolk, `P` pigfolk), containers as `c` chest, `b` barrel, `x` crate and `a` amphora, scenery as `%` rubble, `~` bones and `o` pillar, and creatures as `r` rat, `R` giant rat, `v` poisonous snake and `S` constrictor snake, and lists where each stands, which is handy for `FB_DEBUG_POSE`.

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

### The creature view

```
cargo run -p forever_below -- --creature
cargo run -p forever_below -- --creature --species constrictor --seed 7 --motion rearing
```

One rat or snake on the turntable's plate, posed and animated as in the level, with its SRD hit points and bite.

- `--species <name>`: rat (the default), giant_rat, viper (the poisonous snake) or constrictor.
- `--seed <n>` rolls the creature; 1207 by default.
- `--motion <name>`: still (the default), moving (trotting or slithering in place at its own speed), rearing or striking.

Tab and Shift+Tab change the species, Left and Right the seed, M the motion, and Space pauses the turn. `FB_SCREENSHOT` works here as on the turntable.

### The Forge view

```
cargo run -p forever_below -- --forge
cargo run -p forever_below -- --forge --family lantern --seed 7
```

One Armory item turning on the turntable's plate, with a panel on the right like the Forge sliders of `reference/spline-armory.html`. The panel has a picker for every ported family, the seed with a "Forge new" button that rolls a fresh one and a "Reset" that goes back to the seed's roll, the five wear presets, and then every parameter of the item's state grouped by section (shaft, head, flame, condition...). Sliders show their value and snap to their step, within the range the family allows; choices (a lantern's style, fuel or metal) are buttons that cycle; toggles are boxes. Every change rebuilds the mesh at once. Edits only change the state on show: the seed still rolls the same item. The panel is built from `armory::Item::fields` (`crates/armory/src/reflect.rs`), so a new family needs nothing here.

- `--family <key>` picks the family; mace by default.
- `--seed <n>` rolls the item; 1207 by default.

Tab and Shift+Tab change the family, Left and Right step the seed, N forges a new seed, R resets, and Space pauses the turn. Dragging the stage orbits and scrolling over it zooms. On the panel, dragging a slider sets it, a click cycles a choice (a right click goes back), and scrolling moves the panel. `FB_SCREENSHOT` works here as on the turntable.

### Swing practice

```
cargo run -p forever_below -- --swings
```

The level, with a panel at the top left and keys for trying every melee weapon's swings. Walk up to a rat to see blows bend toward it.

- `[` and `]` hold the previous or next melee base (every sword, mace, axe, club, dagger, spear and staff base); `N` rolls another of the same base.
- `1` to `7` throw every swing as one strike, whatever the weapon: overhead, high right, high left, from the right, from the left, rising, stab. `0` goes back to rolling them.
- `G` swings over and over; `T` slows time to a quarter.
- `FB_STRIKE=<1 to 7>` starts with that strike pinned, for captures with `FB_SWING=1`.

### Screenshots and debug switches

These environment variables are for checking the game without playing it. Put them in front of the command, for example `FB_SCREENSHOT=shot.png cargo run -p forever_below`.

| Variable | Does |
|---|---|
| `FB_SCREENSHOT=<file.png>` | renders offscreen, saves one frame and quits; keyboard and mouse are ignored |
| `FB_SCREENSHOT_FRAMES=<from>..<to>` | with `FB_SCREENSHOT=shot.png`, saves each frame from `from` up to (not including) `to` as `shot_<frame>.png` (`120..136` saves `shot_120.png` to `shot_135.png`; `120..=135` and a single `120` also work), to compare consecutive frames |
| `FB_SCREENSHOT_EVERY=<n>` | with `FB_SCREENSHOT_FRAMES`, saves only every `n`th frame of the run (`1..900` every `15` saves `shot_1.png`, `shot_16.png`, ...), for long runs such as a fall, the title and the next delve |
| `FB_WEAR=<state>` | turntable starting condition (`--wear` wins) |
| `FB_CLOSEUP=<0–11>` | turntable opens on a close-up of that grid slot |
| `FB_DEBUG_POSE=x,y,yaw,pitch` | holds the player at a grid point (cells), turned `yaw` degrees from north and tilted `pitch` degrees |
| `FB_DEBUG_CAMERA=start\|back\|shut\|top\|brazier\|glowcaps` | a lit camera at a fixed view of the level |
| `FB_DEBUG_LIGHT=1` | a dim light on the player's camera, and on what is held |
| `FB_OPEN_INVENTORY=1` | opens the inventory at the start |
| `FB_OPEN_CONTAINER=<n>` | opens the level's `n`th container (0 the first, in `print_level`'s order) a moment in, with the inventory showing what it holds |
| `FB_DEBUG_KIT=1` | dresses you in your class's outfit with a lit light in the off hand, a filled pack, and a sack and a shield set down in front of you |
| `FB_DROP=<base>[:<rarity>][,...]` | sets an item of each named base down in front of you a moment in, of that rarity if one is named (`Sword:legendary,Sword:common`), spread left to right; for captures of floor items such as a legendary's shimmer |
| `FB_SWING=1` | swings the main hand over and over, for captures of the swing and of blows landing (with `FB_HOLD` and `FB_DEBUG_POSE`) |
| `FB_HOLD=<main>[,<off>]` | puts an item of each named base in that hand, after the starting or debug kit (`Mace,Torch`, `Sword,Shield`, `Staff,none`; a torch or lantern is lit, `none` empties the hand) |
| `FB_PLAYER_HP=<n>` | starts the delver on `n` hit points (at least 1, at most their full), to check a fall quickly (`FB_PLAYER_HP=1`) |
| `FB_NEW_DELVE=<seconds>` | the title chooses New delve by itself after that many seconds, each time it shows; with `FB_PLAYER_HP=1` and `FB_SCREENSHOT_FRAMES`, captures a fall, the title and the next delve without input |
| `FB_EQUIP=<base>[,<base>...]` | equips one item of each named base in its slot (or `<base>@<slot>` in another it fits: `Lantern@hip`, `Sword@offhand`), after the starting or debug kit and `FB_HOLD` (`Mail coif,Tunic`, or `Pack` for room); whatever it displaces is stowed |
| `FB_STOW=<base>[,<base>...]` | puts one item of each named base in the carried grids, after `FB_EQUIP` (`Mail coif,Hood,Helm,Tunic`) |
| `FB_FOLK=<folk>` | makes the delver one of that folk (human, dwarf, gnome, goblin, lizard, pig) |
| `FB_DEBUG_HOVER=<n>` | holds the hover card over the nth item in the first bag |
| `FB_OPEN_MENU=main\|controls\|settings` | opens the menu on that page; with `--title`, `settings` opens the title on its settings page |
| `FB_SETTINGS=<file.json>` | reads and writes settings from another file |
| `FB_FRAME_TIME=<frames>` | prints Bevy's smoothed frame time at each listed frame (`240,270,300`) and quits after the last; with `FB_DEBUG_POSE`, measures a view's frame rate in the window at vsync |
| `FB_SOAK=<seconds>` | plays the level by itself for that long, then quits: walks random paths, fights the creatures it meets, picks up, equips and drops things, opens containers, the inventory and the menu, and logs a `soak:` line to stderr every 10 s (entities, meshes, images, materials, cameras, readbacks, icons, render phases, frame time). With `--title` it begins a new delve from the title, and after a death begins another, a second after the title shows, first logging a `soak: title` line with the entities on it, which should read the same after every death. For finding crashes and leaks that only show after a long session |
| `FB_SOAK_SEED=<n>` | with `FB_SOAK`, seeds the autopilot's choices (1 by default), so parallel soaks on one level take different ways |
| `FB_SOAK_CHURN=1` | with `FB_SOAK`, forgets every inventory icon at each report, so they are all drawn again by fresh icon cameras |

A soak of twenty minutes: `RUST_BACKTRACE=1 BEVY_ASSET_ROOT=game FB_SOAK=1200 ./target/debug/forever_below --seed 1207 2> soak.log`, then `grep soak: soak.log` for the counts and `grep -A30 panicked soak.log` for a crash.

Running the built binary directly (`./target/debug/forever_below`) needs `BEVY_ASSET_ROOT=game` so it finds the music and fonts; `cargo run` finds them on its own.

### Tuning

`game/assets/tuning.toml` holds how the game looks and feels: how large held and dropped things are drawn, how fast you walk, how big creatures are and how they fight, how bright the lights burn, the sound mix and the inventory's layout. Edit a value and save; in a debug build (`cargo run`) the game watches the file and applies the change within a second, while it runs. A key left out, misspelt or given the wrong kind of value keeps its default, and the log names it in a warning; nothing in the file can stop the game. A release build reads the file once at startup. 1 world unit is 10 cm; times are in seconds.

| Table | Key | Shipped | What it does |
| --- | --- | --- | --- |
| `[view]` | `held_scale` | 1.4 | How much larger than its own size a held item is drawn (1 = true size). |
| `[view]` | `held_fov_degrees` | 46.0 | Vertical field of view of the camera that draws what you hold, in degrees. |
| `[view]` | `world_fov_degrees` | 70.0 | Vertical field of view of the world, in degrees. |
| `[view]` | `head_bob` | 1.0 | How much the view and what you hold bob as you walk (0 = still, 1 = usual). |
| `[view]` | `floor_item_scale` | 1.15 | How much larger than its own size an item lying on the floor is drawn (1 = true size). |
| `[view]` | `aim_buffer_degrees` | 2.0 | How far off the crosshair, in degrees, something can be and still be aimed at. |
| `[view]` | `aim_min_radius` | 0.6 | The least that leeway is, in world units off the crosshair, for small things close by (0.6 = 6 cm). |
| `[view]` | `legendary_shimmer` | 0.25 | How strongly a legendary lying on the floor shimmers (0 = not at all). It lights nothing. |
| `[view]` | `container_scale` | 1.6 | How much larger than its own size a chest, barrel, crate or other container standing in the level is drawn (1 = true size; 1.6 makes a chest about a metre wide). |
| `[moving]` | `walk_speed` | 14.0 | Walking pace, in world units per second (14 = 1.4 m/s). |
| `[moving]` | `hurry_speed` | 45.0 | Pace with Shift held, in world units per second. |
| `[moving]` | `acceleration` | 10.0 | How quickly you reach your pace and stop, per second (higher is snappier). |
| `[moving]` | `body_radius` | 3.5 | Your body's radius on the floor, in world units (3.5 = 35 cm). |
| `[creatures]` | `sense_scale` | 1.0 | How far creatures notice you, as a multiple of their senses. |
| `[creatures]` | `rat_cooldown` | 1.5 | Seconds between a rat's bites. |
| `[creatures]` | `snake_cooldown` | 2.5 | Seconds between a snake's strikes. |
| `[creatures]` | `rat_chase_speed` | 2.0 | How fast a rat comes at you once it has seen you, as a multiple of its speed. |
| `[creatures]` | `snake_chase_speed` | 1.0 | How fast a snake comes at you once it has seen you, as a multiple of its speed. |
| `[creatures.size]` | `rat` | 1.0 | How large each kind of creature is drawn, and the floor it covers (1 = as rolled). |
| `[creatures.size]` | `giant_rat` | 1.0 | Giant rats, as a multiple. |
| `[creatures.size]` | `viper` | 1.0 | Poisonous snakes, as a multiple. |
| `[creatures.size]` | `constrictor` | 1.0 | Constrictor snakes, as a multiple. |
| `[combat]` | `reach_scale` | 1.0 | How far your blows reach, as a multiple of the weapon's own reach. |
| `[combat]` | `hit_cone_degrees` | 50.0 | Half the angle of the cone ahead a blow can land in, in degrees either side of the crosshair. |
| `[combat]` | `flash_hold` | 0.12 | Seconds a struck creature's flash holds at full. |
| `[combat]` | `flash_fade` | 0.2 | Seconds the flash then takes to fade. |
| `[combat]` | `knockback_scale` | 1.0 | How far a blow knocks a creature back, as a multiple of the usual. |
| `[combat]` | `hit_kick` | 1.0 | How much the view jolts when hit, as a multiple of the usual (0 = no jolt). |
| `[light]` | `torch_peak` | 1.0 | A torch, lantern or other burning thing's strength close up (1 = full). |
| `[light]` | `torch_reach` | 2.0 | How far its light reaches, as a multiple of its bright radius. |
| `[light]` | `brazier_peak` | 1.5 | A brazier's strength close up (1 = full). |
| `[light]` | `brazier_reach` | 150.0 | How far a brazier's light reaches, in world units. |
| `[light]` | `glowcap_peak` | 0.5 | A patch of glowcaps' strength close up (1 = full). |
| `[light]` | `glowcap_reach` | 30.0 | How far glowcaps' light reaches, in world units. |
| `[light]` | `flicker_depth` | 1.0 | How deeply flames flicker (0 = steady, 1 = usual). |
| `[light]` | `ambient_lux` | 4.0 | The abyss's own light where no flame reaches, in lux. |
| `[light]` | `fog_start` | 40.0 | Where the dark starts to swallow far stone, in world units. |
| `[light]` | `fog_end` | 260.0 | Where the dark is complete, in world units. |
| `[sound]` | `music_mix` | 0.35 | The music's volume at full, 0 to 1, before your music volume setting. |
| `[sound]` | `silence_min` | 45.0 | The shortest silence between tracks, in seconds. |
| `[sound]` | `silence_max` | 75.0 | The longest silence between tracks, in seconds. |
| `[sound]` | `effects_volume` | 1.0 | Sound effects' volume at full, 0 to 1, before your sound volume setting. |
| `[interface]` | `cell_size` | 46.0 | Inventory grid cell size, in logical pixels. |
| `[interface]` | `hover_delay` | 0.0 | Seconds the mouse rests on an item before its card shows. |

The shipped values are the defaults in `game/src/tuning.rs`; a test keeps the two, and this list, in step.

### Generator tools

- `cargo run -p armory --example dump_obj -- [family] <seed> [out.obj]` writes an Armory item as OBJ.
- `cargo run -p fb_body --example dump_body -- <folk> <seed> > body.obj` writes a folk body as OBJ (human, dwarf, gnome, goblin, lizard, pig).
- `cargo run -p fb_creatures --example dump_creature -- <rat|giant_rat|viper|constrictor> <seed> > rat.obj` writes a creature at rest as OBJ.
- `tools/README.md` covers regenerating the data tables and the soundtrack.
