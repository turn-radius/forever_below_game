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
