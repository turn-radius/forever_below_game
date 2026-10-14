# Tools

Both scripts read the prototypes in `reference/` and need Node 18+ and three.js r128:

```
npm pack three@0.128.0 && tar xzf three-0.128.0.tgz   # gives package/build/three.min.js
```

**Example fixtures**: 64 seeds per Armory family, plus mulberry32 reference streams. Reference only.

```
N=64 node tools/gen_fixtures.js package/build/three.min.js reference/spline_armory_engine.js fixtures
```

`reference/spline_armory_engine.js` is the engine as embedded in the plate. If `spline-armory.html` changes, re-extract the engine from the new page (everything between the Schema section and the Three setup section, plus the build functions) or ask Claude to do it, then regenerate.

**Game tables**: the plate's data (races, slots, bases, goods, affixes, outfits, Armory mappings, fitting constants) as JSON. Functions are kept as their JS source.

```
node tools/extract_data.js reference/forever-below-plate.html data
```

**Soundtrack**: the sources in `reference/audio/` are Opus in MP4, which Bevy can't decode. They're converted to Ogg Vorbis at quality 6 in `game/assets/audio/`. Homebrew's ffmpeg has no libvorbis encoder, so ffmpeg decodes and `oggenc` (`brew install vorbis-tools`) encodes:

```
ffmpeg -i "reference/audio/The Wizards Tower.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/the-wizards-tower.ogg -
ffmpeg -i "reference/audio/Shadowed Pixel.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/shadowed-pixel.ogg -
ffmpeg -i "reference/audio/Forever Below.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/forever-below.ogg -
ffmpeg -i "reference/audio/Cavernous Arpeggios.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/cavernous-arpeggios.ogg -
ffmpeg -i "reference/audio/Unreachable Home.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/unreachable-home.ogg -
ffmpeg -i "reference/audio/Cavern Descent.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/cavern-descent.ogg -
ffmpeg -i "reference/audio/I'm not alone.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/im-not-alone.ogg -
ffmpeg -i "reference/audio/Untitled.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/untitled.ogg -
```

**Sound effects**: there are no recorded sound effects. `tools/synth_door_shut.py` synthesises the entrance door's slam and latch by modal synthesis (decaying sine modes struck by filtered noise; the script's docstring cites the sources) with Python's standard library alone, the same on every run, and `oggenc` encodes it:

```
python3 tools/synth_door_shut.py /tmp/door-shut.wav
oggenc -q 6 -o game/assets/audio/door-shut.ogg /tmp/door-shut.wav
```

`tools/synth_combat.py` synthesises the fight's sounds the same way: a rat's squeak (a sine chirp with vibrato), a snake's hiss (band-passed noise) and a weapon's whoosh (noise through a sweeping band-pass), its docstring citing the sources:

```
python3 tools/synth_combat.py /tmp/combat
oggenc -q 6 -o game/assets/audio/squeak.ogg /tmp/combat/squeak.wav
oggenc -q 6 -o game/assets/audio/hiss.ogg /tmp/combat/hiss.wav
oggenc -q 6 -o game/assets/audio/whoosh.ogg /tmp/combat/whoosh.wav
```

Bevy loads assets from `game/assets/` under `cargo run` (it uses `CARGO_MANIFEST_DIR`). To run the binary directly, set `BEVY_ASSET_ROOT=game`.
