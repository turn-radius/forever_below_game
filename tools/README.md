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
```

Bevy loads assets from `game/assets/` under `cargo run` (it uses `CARGO_MANIFEST_DIR`). To run the binary directly, set `BEVY_ASSET_ROOT=game`.
