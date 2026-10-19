# Forever Below

A first-person delve into an endless Stygian abyss. Rust and Bevy 0.20.

Start with `HANDOFF.md`. Claude Code reads `CLAUDE.md` on its own. Run tests with `cargo test --workspace` or `cargo nextest run --workspace` for faster parallel execution.

## Installing

### Download a release

Each release on the repository's [Releases](https://github.com/turn-radius/forever_below_game/releases) page has a zip for Windows, macOS and Linux. Unzip it anywhere and keep the `assets` folder beside the game, which loads everything from there.

- **Windows:** run `forever_below.exe`. If SmartScreen says it protected your PC, choose More info, then Run anyway; the build is not signed.
- **macOS (Apple silicon):** in Terminal, in the unzipped folder, run `xattr -dr com.apple.quarantine .` once, then `./forever_below`. The build is not signed, so Gatekeeper blocks it until the quarantine flag is cleared.
- **Linux (x86-64):** run `./forever_below`. It needs ALSA and udev, which most desktops already have (`libasound2` and `libudev1` on Debian and Ubuntu).

A graphics card with Vulkan, DirectX 12 or Metal is needed, which any computer from the last several years has.

### Build from source

1. Install Rust with [rustup](https://rustup.rs). The repository's `rust-toolchain.toml` picks the toolchain (Rust 1.97.1 or newer), and rustup fetches it on the first build.
2. Install the platform's build tools:
   - **Windows:** the [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the "Desktop development with C++" workload. The rustup installer offers to install them.
   - **macOS:** `xcode-select --install`.
   - **Linux (Debian or Ubuntu):** `sudo apt install build-essential pkg-config libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`. Other distributions are listed in Bevy's [setup guide](https://bevy.org/learn/quick-start/getting-started/setup/).
3. Clone and run:

   ```
   git clone https://github.com/turn-radius/forever_below_game.git
   cd forever_below_game
   cargo run --release -p forever_below
   ```

   The first build compiles Bevy and takes several minutes; later builds are quick. `--release` plays smoothly; leave it off for a faster build while working on the code.

### Making a release

Push a version tag, such as `git tag v0.2.0 && git push origin v0.2.0`. The Release workflow (`.github/workflows/release.yml`) builds the game on Windows, macOS and Linux and attaches a zip for each to a new GitHub release. Running the workflow by hand from the Actions tab builds the same zips as workflow artifacts without publishing a release.

## Running the game

Everything runs from the repository root with `cargo run -p forever_below -- <options>`. The `--` separates cargo's own options from the game's.

### The title screen (the default)

```
cargo run -p forever_below                         # the title screen
cargo run -p forever_below -- --seed 42            # skip it: straight into the level for seed 42
cargo run -p forever_below -- --title --seed 42    # the title, and New delve plays seed 42
```

The game opens on the title screen, to Forever Below's theme. New delve (or Enter) opens the character creator over the card: pick the folk and class, type a name or roll one, roll the ability scores and the look, and watch the delver turn in the kit they start with. Begin the delve (or Enter) rolls a new level from the clock and goes down; Back (or Esc) returns. Opening the creator turns the music to Measured Pulse, which loops until you go in and then fades out. In a level, the Trolls theme takes over while a troll is within a couple of rooms. Once you have gone down as someone, the title offers Continue as their name, which starts a fresh level with the same delver (kept in `last_character.json` beside `settings.json`). Settings is the pause menu's settings page (Esc or Back returns), and Quit leaves.

Giving `--seed`, or any debug switch that looks at the level (`FB_DEBUG_POSE`, `FB_DEBUG_CAMERA`, `FB_DEBUG_HOVER`, `FB_DEBUG_KIT`, `FB_DROP`, `FB_OPEN_INVENTORY`, `FB_OPEN_CONTAINER`, `FB_OPEN_MENU`, `FB_OPEN_MAP`), skips the title and opens straight into the level, 1207 unless `--seed` says otherwise. `--title` keeps the title whatever else is given; `FB_SCREENSHOT=title.png cargo run -p forever_below -- --title` captures it, and adding `FB_OPEN_MENU=settings` captures its settings page, or `FB_OPEN_MENU=creator` the character creator (drafting from `--seed`, or 1).

### Play a level

You come through a door into the first level, and it shuts behind you. Folk of the level's factions stand about in groups; look at one up close to see who they are. The keys are listed in the game: press Esc for the menu, then Controls.

| Key | Does |
|---|---|
| W A S D, Shift | walk, sprint |
| Q, E, Ctrl, Space | dodge left and right, crouch, jump (bound, not yet acted on) |
| Mouse | look round (click to take the mouse back after Esc) |
| Left click | swing what your main hand holds (a weapon in the off hand is held but does not swing yet), each blow from one of the directions it suits and arcing to the creature in reach: mostly overhead for a mace, axe or club, from every side for a sword, flat across for a staff, a stab for a spear or dagger, a punch with nothing (a click while the mouse is free only takes it back) |
| F | pick up what you're looking at, take the stairs, or open the chest, barrel, crate or amphora you're looking at (its contents show beside what lies underfoot) |
| G | equip what you're looking at straight from the floor: worn gear in its slot, a weapon in the main hand (or the off hand, if only that is free), a torch, lantern or shield in the off hand, a ring on a free finger; what it replaces goes into a bag, or down where the new thing lay; a lantern goes in a free hand, else at the hip if you wear a belt, where it lights the way with both hands full |
| Right click | block (bound, not yet acted on) |
| R | tertiary action: with a crossbow in hand and nothing aimed at, span and load it from the bolt case |
| Alt | hold over an item to compare it with what you wear in its place |
| T, J | special action and journal (bound, not yet acted on) |
| L, Shift+L | light or put out the torch or lantern you carry, the one in hand first, then the one at your hip; Shift+L acts on the hip lantern alone. In the inventory, L lights or puts out the torch or lantern under the mouse, wherever it lies; its card says which. A spent torch will not light. Every lit light you carry shines |
| M | map of the level, filled in as you explore: only cells you have seen while they were lit are drawn, as an inked plan with you marked by an amber arrow; scroll to zoom about the mouse, drag to pan; M or Esc closes it, and the body holds still while it is open |
| I | inventory: drag items between your bags, the floor and your slots (or onto the doll to wear them); R or right click turns a dragged item; F eats or drinks the food or drink under the mouse; double-click wears or takes off; drag the doll to turn it |
| 1 to 0 | use the item in that quickslot: eat or drink it, draw a weapon or shield into the hand, or draw a torch or lantern and then light or put it out. In the inventory, the number binds the item under the mouse to that slot, as dragging it onto the strip's slot does (a worn item too; another number moves it; an item a slot cannot use, or one lying underfoot, is refused the same way) |
| K | character: the inventory screen's character tab; K or I swaps between its tabs, and the key of the tab showing closes it |
| Esc | go back: close a screen or page, or open the menu (Resume, Controls, Settings, Quit) |
| − and = | music quieter and louder |

Rats and snakes notice you, close in and bite. Your hit points show low on the left ("HP 8/12", a 1st-level character's from the SRD), and the edge of the view flashes red when you are bitten; a poisonous snake's venom is halved on a Constitution save, and a constrictor holds you, slowed, until you strike it. A blow lands on the nearest creature whose body (head, legs, tail, any part of a snake) is within your weapon's reach ahead, for its damage dice; a thrust drives the point at the crosshair. Struck creatures flash, flinch and squeak or hiss, and at 0 they fall and their bodies stay where they lie. At 0 hit points you fall in the abyss and go back to the title. Combat is a placeholder until the character simulation is designed.

Small round badges just above the left end of the quickslot bar show the conditions you are under right now: hungry, thirsty or sleepy past the first band, exhausted, winded, burdened or heavier, and held by a constrictor. Each is coloured from cream to red as it gets worse and goes when the condition does; when the mouse is free (say, after switching windows), hovering one names it and says what it does.

Settings (music and sound volume, mouse sensitivity, invert Y, fullscreen) are saved in `~/Library/Application Support/Forever Below/settings.json` on macOS, `%APPDATA%\Forever Below\settings.json` on Windows and `~/.config/forever-below/settings.json` on Linux.

To print a level's map without running the game: `cargo run -p fb_level --example print_level -- <seed>`. It marks folk by their initial in capitals (`H` human, `D` dwarf, `N` gnome, `G` goblin, `L` lizardfolk, `P` pigfolk), containers as `c` chest, `b` barrel, `x` crate and `a` amphora, scenery as `%` rubble, `~` bones and `o` pillar, stairs as `>` down and `<` up, and creatures as `r` rat, `R` giant rat, `v` poisonous snake, `S` constrictor snake and `k` skeleton, and lists where each stands, which is handy for `FB_DEBUG_POSE`.

### The Armory turntable

```
cargo run -p forever_below -- --turntable
cargo run -p forever_below -- --family sword --seed 1207 --wear grave_find
```

A grid of twelve items from consecutive seeds, turning, to compare what the generator makes. `--family` alone also opens it.

- `--family <name>` starts on a family: staff, mace, sword, spear, dagger, club, axe, bow, crossbow, helm, hat, hood, garment, belt, hands, legs, feet, shield, ring, amulet, charm, bags, chests, food, torch, lantern, sundry, writing.
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
- `--walk` starts the doll walking in place, `--speed <n>` sets its pace as a multiple of its folk's natural one (0 to 1.6, 1 by default). W toggles the walk in the view, and the panel at the top right has the same toggle and a speed slider.
- `--outfit <key>`: fighter (the default), ranger, mage, cleric, fallen ("Freshly fallen") or scavenger, from `data/outfits.json`.

Tab and Shift+Tab change the folk, Left and Right the seed, Up and Down the outfit, and Space pauses the turn. `FB_SCREENSHOT` works here as on the turntable.

### The creature view

```
cargo run -p forever_below -- --creature
cargo run -p forever_below -- --creature --species constrictor --seed 7 --motion rearing
cargo run -p forever_below -- --skeleton --seed 7 --motion swing
```

One rat, snake or skeleton on the turntable's plate, posed and animated as in the level, with its SRD armor class, hit points and bite (a skeleton's shortsword). `--skeleton` is the view opened on a skeleton: roll seeds with Left and Right to see the bones' colour, cracks and missing pieces, and the gear the seed rolls it (`--bare` takes it off; G toggles it).

- `--species <name>`: rat (the default), giant_rat, viper (the poisonous snake), constrictor or skeleton.
- `--seed <n>` rolls the creature; 1207 by default.
- `--motion <name>`: still (the default), moving (trotting, slithering or walking in place at its own speed), rearing or striking. A skeleton has three: still (`stand`), moving (`walk`) and striking (`swing`, the right arm's blow).

Tab and Shift+Tab change the species, Left and Right the seed, M the motion, G a skeleton's gear, and Space pauses the turn. `FB_SCREENSHOT` works here as on the turntable.

### The Forge view

```
cargo run -p forever_below -- --forge
cargo run -p forever_below -- --forge --family lantern --seed 7
```

One Armory item turning on the turntable's plate, with a panel on the right like the Forge sliders of `reference/spline-armory.html`. The panel has a picker for every ported family, the seed with a "Forge new" button that rolls a fresh one and a "Reset" that goes back to the seed's roll, the five wear presets, and then every parameter of the item's state grouped by section (shaft, head, flame, condition...). Sliders show their value and snap to their step, within the range the family allows; choices (a lantern's style, fuel or metal) are buttons that cycle; toggles are boxes. Every change rebuilds the mesh at once. Edits only change the state on show: the seed still rolls the same item. The panel is built from `armory::Item::fields` (`crates/armory/src/reflect.rs`), so a new family needs nothing here.

- `--family <key>` picks the family; mace by default.
- `--seed <n>` rolls the item; 1207 by default.

Tab and Shift+Tab change the family, Left and Right step the seed, N forges a new seed, R resets, and Space pauses the turn. Dragging the stage orbits and scrolling over it zooms. On the panel, dragging a slider sets it, a click cycles a choice (a right click goes back), and scrolling moves the panel. `FB_SCREENSHOT` works here as on the turntable.

### The level view

```
cargo run -p forever_below -- --level
cargo run -p forever_below -- --level --seed 7
```

A whole level from above, drawn as the map draws it but with every cell shown, beside a panel of the level generator's parameters (`fb_level::LevelParams`): the grid's size and ceiling; how many rooms, how big, how often a hall, the gap kept between them and how deep the entrance may lie; what a corridor pays to turn or to run beside another, and how often rooms loop and reuse doors; and where braziers and loot go. The header gives the seed, a "New seed" button and a "Reset" back to the defaults, and counts what the level holds (rooms, floor cells, doors, items, lights, props, creatures and folk). Moving a slider lays the level out again from the same seed at once. The defaults are the game's own, so with them every seed is the level the game plays. The panel is the Forge's (`game/src/tweak_panel.rs`).

- `--seed <n>` picks the level; 1207 by default.

Left and Right step the seed, N rolls a new one and R resets the parameters. Scrolling over the plan zooms about the mouse and dragging pans; on the panel, sliders drag and the panel scrolls. `FB_SCREENSHOT` works here too.

### The lab

```
cargo run -p forever_below -- --lab
cargo run -p forever_below -- --lab --seed 7
```

A character on the left and their sheet on the right, with no level, creatures or audio. The panel (the Forge's, `game/src/tweak_panel.rs`) holds the folk, class, the five attributes (3 to 20) and the character seed, then for each equip slot a base (or none), an item seed and a switch for each affix the item rolled; switching one off derives the item's stats again without it. The sheet lists hit points, armour class, the held weapon's least, mean and most damage, load against capacity with its band, darkvision, light and the hunger, thirst and sleepiness rates, each with the terms of its `fb_character::explain::Explained` beneath. Every number comes from the rules the game calls, so what is balanced here is what the game plays. Any change rebuilds the sheet at once. The window is no smaller than 960 by 540. Below the sheet, the strike column picks an attacker in the panel (any creature, or a folk with STR 10 holding a melee base rolled from the strike seed) and has Strike (one logged blow: the d20 against the sheet's AC, then the damage, the venom save and the hit points before and after), ×100 (hit rate, damage min, mean and max, and the mean blows to kill from full) and Heal. The strike seed sets every roll, so the same seed gives the same blows and the same hundred. Below that, the time column has an activity choice in the panel (the sheet's meter rates follow it) and buttons that pass 10 minutes, an hour, 8 hours or a day under it with the gear applied (`ExhaustionTimes::step`, as the game steps the delver), plus Eat (a bread), Drink (a tankard) and Reset. It shows the clock (days counted from 0), each meter as a percent with its band word and explained rate, the exhaustion level and what it does, and a log of the game time each band and level was crossed.

- `--seed <n>` rolls the class and scores of a human from the seed; with none it opens on a human Fighter with every score 10.

Scrolling over the panel moves it, and anywhere else moves the sheet. `FB_SCREENSHOT` works here too.

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
| `FB_SCREENSHOT_SIZE=<width>x<height>` | with `FB_SCREENSHOT`, renders at that size instead of 1920x1080 (`1280x720`), to check a layout at another resolution |
| `FB_WEAR=<state>` | turntable starting condition (`--wear` wins) |
| `FB_CLOSEUP=<0–11>` | turntable opens on a close-up of that grid slot |
| `FB_DEBUG_POSE=x,y,yaw,pitch` | holds the player at a grid point (cells), turned `yaw` degrees from north and tilted `pitch` degrees |
| `FB_DEBUG_CAMERA=start\|back\|shut\|top\|brazier\|glowcaps` | a lit camera at a fixed view of the level |
| `FB_DEBUG_LIGHT=1` | a dim light on the player's camera, and on what is held |
| `FB_OPEN_INVENTORY=1` | opens the inventory at the start (`=character` on the character tab) |
| `FB_OPEN_MAP=<frame>` | opens the map that many frames into the delve, once the start has been looked round (`FB_OPEN_MAP=120`) |
| `FB_OPEN_CONTAINER=<n>` | opens the level's `n`th container (0 the first, in `print_level`'s order) a moment in, with the inventory showing what it holds |
| `FB_DEBUG_KIT=1` | dresses you in your class's outfit with a lit light in the off hand, a filled pack, and a sack and a shield set down in front of you |
| `FB_DROP=<base>[:<rarity>][,...]` | sets an item of each named base down in front of you a moment in, of that rarity if one is named (`Sword:legendary,Sword:common`), spread left to right; for captures of floor items such as a legendary's shimmer |
| `FB_SWING=1` | swings the main hand over and over, for captures of the swing and of blows landing (with `FB_HOLD` and `FB_DEBUG_POSE`) |
| `FB_HOLD=<main>[,<off>]` | puts an item of each named base in that hand, after the starting or debug kit (`Mace,Torch`, `Sword,Shield`, `Staff,none`; a torch or lantern is lit, `none` empties the hand) |
| `FB_CONDITIONS=<list>` | holds the delver under conditions for the status badges, comma separated: `hunger:<0 to 1>`, `thirst:<0 to 1>`, `sleep:<0 to 1>`, `exhaustion:<level>`, `winded`, `load:<light\|burdened\|encumbered\|overloaded>`, `poisoned` (an hour) or `poisoned:<minutes>` (`FB_CONDITIONS=hunger:0.8,winded,load:encumbered`) |
| `FB_CONDITION_HOVER=<n>` | shows the card of the `n`th status badge (0 the first, worst first) as if hovered |
| `FB_PROVOKE=<n>` | a second in, gives the level's `n`th folk (0 the first, in `print_level`'s order) cause to fight: they and the kin who see it turn hostile, for captures of folk fighting (with `FB_DEBUG_POSE`) |
| `FB_SLAY=<n>` | two seconds in, strikes the level's `n`th folk down, for captures of a body and the gear it drops |
| `FB_PLAYER_HP=<n>` | starts the delver on `n` hit points (at least 1, at most their full), to check a fall quickly (`FB_PLAYER_HP=1`) |
| `FB_ROLL_LOOK=<seconds>` | the character creator rolls a look by itself that often, for captures of the doll changing (with `FB_OPEN_MENU=creator` and `--title`) |
| `FB_NEW_DELVE=<seconds>` | the title begins a delve by itself, past the creator and with a rolled delver, after that many seconds, each time it shows; with `FB_PLAYER_HP=1` and `FB_SCREENSHOT_FRAMES`, captures a fall, the title and the next delve without input |
| `FB_EQUIP=<base>[,<base>...]` | equips one item of each named base in its slot (or `<base>@<slot>` in another it fits: `Lantern@hip`, `Sword@offhand`), after the starting or debug kit and `FB_HOLD` (`Mail coif,Tunic`, or `Pack` for room); whatever it displaces is stowed |
| `FB_STOW=<base>[,<base>...]` | puts one item of each named base in the carried grids, after `FB_EQUIP` (`Mail coif,Hood,Helm,Tunic`) |
| `FB_FOLK=<folk>` | makes the delver one of that folk (human, dwarf, gnome, goblin, lizard, pig) |
| `FB_FOLK_WALK=<n>` | every folk in the level walks in place at `n` times its natural pace (`FB_FOLK_WALK=1`), to see the gait and measure its cost before anything moves them |
| `FB_DEBUG_HOVER=<n>` | holds the hover card over the nth item in the first bag |
| `FB_COMPARE=1` | holds Alt, so the hover card shows its compare card beside it |
| `FB_OPEN_MENU=main\|controls\|settings\|creator` | opens the menu on that page; with `--title`, `settings` opens the title on its settings page and `creator` on the character creator |
| `FB_SETTINGS=<file.json>` | reads and writes settings from another file |
| `FB_KEYBINDINGS=<file.toml>` | reads and writes keybindings from another file; default `keybindings.toml` beside the settings file |
| `FB_FRAME_TIME=<frames>` | prints Bevy's smoothed frame time at each listed frame (`240,270,300`) and quits after the last; with `FB_DEBUG_POSE`, measures a view's frame rate in the window at vsync |
| `FB_SOAK=<seconds>` | plays the level by itself for that long, then quits: walks random paths, fights the creatures it meets, picks up, equips and drops things, opens containers, the inventory and the menu, and logs a `soak:` line to stderr every 10 s (entities, meshes, images, materials, cameras, readbacks, icons, render phases, frame time). With `--title` it begins a new delve from the title, and after a death begins another, a second after the title shows, first logging a `soak: title` line with the entities on it, which should read the same after every death. For finding crashes and leaks that only show after a long session |
| `FB_SOAK_SEED=<n>` | with `FB_SOAK`, seeds the autopilot's choices (1 by default), so parallel soaks on one level take different ways |
| `FB_SOAK_CHURN=1` | with `FB_SOAK`, forgets every inventory icon at each report, so they are all drawn again by fresh icon cameras |

A soak of twenty minutes: `RUST_BACKTRACE=1 BEVY_ASSET_ROOT=game FB_SOAK=1200 ./target/debug/forever_below --seed 1207 2> soak.log`, then `grep soak: soak.log` for the counts and `grep -A30 panicked soak.log` for a crash.

Running the built binary directly (`./target/debug/forever_below`) needs `BEVY_ASSET_ROOT=game` so it finds the music and fonts; `cargo run` finds them on its own.

### Inspecting the game

In a debug build, F12 (or `--inspect` on the command line) opens a World Inspector window, from `bevy-inspector-egui`. It lists every entity with its components: the player's `Locomotion`, needs, stamina and exhaustion, each creature's `Critter` and `Intent`, each folk's `Denizen`, `Hostility` and `Simulated` needs, and Bevy's own `Transform` and lights. Edit a value and it changes in the game. While it is open the delver stands still and the mouse is free; F12 closes it and takes the mouse back. Values that live in the generator crates (a creature's roll, a person's body) are not shown, as those crates do not depend on Bevy.

The inspector is the `inspector` feature, part of `dev`, which is on by default, and it is added only when debug assertions are on, so `cargo run --release` has none. A release build passes `--no-default-features`, which also leaves egui out of the binary.

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
| `[moving]` | `web_pace` | 0.65 | The share of its pace a walker keeps in a floor web in a spider's nest. Spiders cross webs at full speed. |
| `[creatures]` | `sense_scale` | 1.0 | How far creatures notice you, as a multiple of their senses. |
| `[creatures]` | `rat_cooldown` | 1.5 | Seconds between a rat's bites. |
| `[creatures]` | `snake_cooldown` | 2.5 | Seconds between a snake's strikes. |
| `[creatures]` | `rat_chase_speed` | 2.0 | How fast a rat comes at you once it has seen you, as a multiple of its speed. |
| `[creatures]` | `snake_chase_speed` | 1.0 | How fast a snake comes at you once it has seen you, as a multiple of its speed. |
| `[folk]` | `walk_speed` | 14.0 | A human's unhurried walking pace, world units per second (shorter folk are slower). |
| `[folk]` | `pause_min` | 3.0 | The least seconds folk wait between walks. |
| `[folk]` | `pause_max` | 8.0 | The most seconds folk wait between walks. |
| `[folk]` | `wander_radius` | 90.0 | How far from its spot a person strolls, world units. |
| `[folk]` | `notice_distance` | 28.0 | How near the player must come, in sight, for a person to stop and face them, world units. |
| `[folk]` | `release_distance` | 40.0 | How far the player must go before a person who faced them carries on, world units. |
| `[folk]` | `release_delay` | 1.0 | Seconds the player must stay away before a person carries on. |
| `[folk]` | `lose_interest_after` | 25.0 | Seconds a hostile person goes without seeing the player before giving up the fight, friendly again, and going home. Our own. |
| `[creatures.size]` | `rat` | 1.0 | How large each kind of creature is drawn, and the floor it covers (1 = as rolled). |
| `[creatures.size]` | `giant_rat` | 1.0 | Giant rats, as a multiple. |
| `[creatures.size]` | `viper` | 1.0 | Poisonous snakes, as a multiple. |
| `[creatures.size]` | `constrictor` | 1.0 | Constrictor snakes, as a multiple. |
| `[creatures.size]` | `skeleton` | 1.0 | Skeletons, as a multiple. |
| `[creatures.size]` | `giant_spider` | 1.0 | Giant spiders, as a multiple. |
| `[creatures.size]` | `troll` | 1.0 | Trolls, as a multiple. |
| `[creatures.size]` | `jackalope` | 1.0 | Jackalopes, as a multiple. |
| `[creatures.size]` | `giant_centipede` | 1.0 | Giant centipedes, as a multiple. |
| `[creatures.size]` | `bat` | 1.0 | Bats, as a multiple. |
| `[creatures.size]` | `giant_bat` | 1.0 | Giant bats, as a multiple. |
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
| `[light]` | `shadowed_lights` | 4 | How many of the level's lights nearest you cast shadows at once (the rest light without shadows). Your own torch and lantern always do. |
| `[light]` | `shadow_map_size` | 512 | Size of each face of a light's shadow map, in texels (smaller is faster and softer-edged). |
| `[sound]` | `music_mix` | 0.35 | The music's volume at full, 0 to 1, before your music volume setting. |
| `[sound]` | `silence_min` | 45.0 | The shortest silence between tracks, in seconds. |
| `[sound]` | `silence_max` | 75.0 | The longest silence between tracks, in seconds. |
| `[sound]` | `effects_volume` | 1.0 | Sound effects' volume at full, 0 to 1, before your sound volume setting. |
| `[sound]` | `ambience_volume` | 0.3 | Ambient sounds' volume at full, 0 to 1: drips and rumbles, before your sound volume setting. |
| `[sound]` | `rumble_interval_min` | 60.0 | The shortest time between stone rumbles, in seconds. |
| `[sound]` | `rumble_interval_max` | 180.0 | The longest time between stone rumbles, in seconds. |
| `[sound]` | `troll_rooms` | 2 | The troll theme plays while a troll is this many rooms away or fewer. 0 is the same room. |
| `[sound]` | `troll_linger` | 10.0 | How long the troll theme plays on after the last troll is out of range, in seconds. |
| `[interface]` | `cell_size` | 72.0 | Inventory grid cell size, in logical pixels, in a window 1080 lines high. The inventory screen scales with the window's height from there. |
| `[interface]` | `min_cell_size` | 48.0 | The fewest logical pixels a cell is drawn at, however short the window. |
| `[interface]` | `hover_delay` | 0.0 | Seconds the mouse rests on an item before its card shows. |
| `[input]` | `hold_time` | 0.25 | Seconds a key or button is held before a Hold binding fires. A press let go sooner is a tap. |
| `[clock]` | `game_seconds_per_second` | 12.0 | How many game seconds pass per real second. 12 means 1 game hour passes every 300 real seconds (5 minutes). |
| `[recovery]` | `minutes_per_hit_point` | 10.0 | Game minutes a fed and watered, unpoisoned delver takes to regain one hit point (10 = one every 50 real seconds at the usual clock). Our own. |

The shipped values are the defaults in `game/src/tuning.rs`; a test keeps the two, and this list, in step.

### Slow tests

Twelve fitting and rendering tests sweep every folk, many seeds and many kits, which takes too long for regular runs. Each keeps its full sweep behind `#[ignore = "slow sweep: ..."]` and has a `_smoke` twin that runs by default over fewer inputs with the same check.

Run the sweeps before merging changes to fitting code in `fb_body`, `fb_items` or the game's folk figures:

```
cargo test --workspace -- --ignored
cargo nextest run --workspace --run-ignored only
```

### Generator tools

- `cargo run -p armory --example dump_obj -- [family] <seed> [out.obj]` writes an Armory item as OBJ.
- `cargo run -p fb_body --example dump_body -- <folk> <seed> > body.obj` writes a folk body as OBJ (human, dwarf, gnome, goblin, lizard, pig).
- `cargo run -p fb_creatures --example dump_creature -- <rat|giant_rat|viper|constrictor|skeleton> <seed> > rat.obj` writes a creature at rest as OBJ.
- `tools/README.md` covers regenerating the data tables and the soundtrack.
