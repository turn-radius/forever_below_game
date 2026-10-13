# The look

Spline Armory's renderer sets the house style: banded toon light, ink outlines, and wear in the shader. Keep the style and raise the fidelity (better meshes and normals, real light and shadow from carried light sources, richer materials). Everything here is read from `reference/spline_armory_engine.js` and `reference/forever-below-plate.html`. Search those files for the names in `code`.

## Toon shading

Materials are three.js `MeshToonMaterial` with a 1-pixel-tall gradient map sampled with nearest filtering, so light falls off in hard bands.

- Spline Armory (`makeGradient(n)`): band `i` of `n` has value `0.12 + 0.88 * i/(n-1)`. Default `n = 3` (`S.render.bands`), giving 0.12, 0.56, 1.0.
- The plate uses `0.16 + 0.84 * i/(n-1)` with `n = 3`, giving 0.16, 0.58, 1.0, a little lighter in the shadows.

In Bevy, use a custom material (WESL) or an `ExtendedMaterial` on `StandardMaterial` that quantizes the diffuse term into these bands. Keep the band count and values as material parameters.

## Ink outline

This is an inverted hull, drawn for every mesh unless that part opts out.

- A second mesh shares the geometry and is drawn with **back faces only**. Its vertex shader pushes each vertex out along an **outline normal** by width `w`, then writes a flat ink colour.
- Outline normals are **smoothed across hard edges**: weld vertices by position (rounded to 1e-4) and average the face normals (`withOutlineNormals`). Without this, faceted parts crack open at the corners. Surfaces that are already smooth use their own normals (`smoothOutline`).
- Ink colour: `#1a1f22` in the Armory, `#1c1814` (a warm near-black) in the plate.
- The width is in world units, rescaled to the view so the line reads the same at any zoom. In the Armory, `w = outline * clamp(viewHeight/9, 0.45, 2.2)` with `outline = 0.032`. Jewellery, food and bags use a floor of 0.02 instead of 0.45. In the plate, `w = 0.028 * clamp(frustum/9, 0.18, 2.2)`.

For first person this becomes a function of distance to the camera. Keep the same feel: a line about 1.5–2.5 px wide on screen, thinner on small things.

## Wear, rust and grime (the decay shader)

Each material carries a **type** and an **amount**. Its fragment shader mixes in procedural decay from 3D value noise (4-octave fbm) in item space, so the pattern stays fixed to the object as it turns (`DECAY_HEAD`, `DECAY_BODY`, `decayAmt`).

| type | material | what decay does |
|---|---|---|
| 1 | iron (`metal`) | dulls, rust blooms, pitting |
| 2 | brass (`accent`) | tarnish, verdigris |
| 3 | wood | grain, silvering, rot, moss |
| 4 | leather, string | darkening, scuffs, grime |
| 5 | gems and stones | clouding, fractures |
| 6 | gold | soft dulling, grime in hollows |
| 7 | silver | tarnish, warm toning, black patches |
| 8 | bone | yellowing, stains, hairline cracks |
| 9 | stone | weathering, lichen |
| 10 | cloth | fading, grime, moth holes |
| 11 | paint over wood | fading, chips showing the boards |
| 12 | food | browning, going stale, mould |

The amount is `cond.wear` times a per-material multiplier (`cond.rust`, `cond.patina`, `cond.rot`, `cond.fray`). The same shader also draws fabric patterns (stripes, checks, tartan, lozenges, quilting, mail rings, scales, rivets, heraldic divisions) and etching (scrollwork, border bands, knotwork, chevrons, diaper, runes) as dark engraving or gold inlay.

The plate adds a character-level **dirt** and **wear** on its own body and clothing materials: grime pools low on the body, clothes fray and get moth holes. Both are driven by the character's conditions.

## Light

- The Armory lights from the camera: a key light of 0.78 at azimuth −40°, elevation 42°, plus ambient `#dfe6ee` at 0.42.
- The plate uses a directional key of `#fff1dc` at 0.62 from (−6, 10, 9), plus ambient `#b6b0c8` at 0.42.
- A torch is a warm point light (`#ffb36a`, intensity 0.55, range 16, quadratic falloff) with flicker `f = .88 + .08 sin(13.1t) + .05 sin(7.3t + 1.2) + .03 sin(23.7t)` and an additive halo sprite.

In the abyss, carried light is the main light. Ambient should be very low so torches, lanterns and glowcaps matter (the light radius is a game stat).

## Palette and UI

The UI is a parchment ledger. Tokens are at the top of the plate's `<style>`: paper `#ebe5d5`, cells `#d7d1c4`, ink `#2b2420`, accent `#6f4526`, gold `#a8782e`, Stygian portrait blue `#22325c`. Rarity colours: common `#4a4239`, magic `#2c5bb0`, rare `#99700f`, legendary `#bb5119`. Display face Almendra, body Alegreya Sans, data IBM Plex Mono, runes Noto Sans Runic. The title lettering is gold and horizontally striped, from `reference/concept/forever-below-title-card.jpg`.

## In the Bevy build

Where the Rust renderer (`game/src/render/`) departs from the prototypes:

- Bands are applied per light, to `N·L * 0.5 + 0.5` as three.js toon shading does, so each carried light adds its own hard steps. A fragment in shadow drops to the lowest band.
- Wear and light work on the display (sRGB) colour, as three.js r128 did with no output encoding: the prototype's wear colours are display values, and the summed light scales the display colour before it is converted to linear. Lit in linear space, the same band values read as nearly flat on screen.
- Rust, verdigris, gold grime, silver blotches, wood rot and moss cover a set share of the surface instead of a threshold that sweeps the whole item: fbm is rescaled to zero mean and unit spread, then cut at the quantile for the coverage the decay calls for. A grave find keeps about half its iron bare (blooms ringed with stain and pitted) and three quarters of its brass free of verdigris. Wear 0 still draws no decay.
- The Armory turntable (`game/src/turntable.rs`) lights like the Armory (`StudioLight::ARMORY`: a key riding on the camera plus ambient), draws on the Armory's plate colour with no tone curve, and closes in on one item (Enter or F; `FB_CLOSEUP=<slot>` for captures), so items can be judged beside `reference/spline-armory.html`. The grid is seen through an orthographic camera, as the Armory's plate is, so each cell is sized from the page's largest bounds (the width an item sweeps as it turns, and its full height) and the labels get a band of their own in pixels under the items. The close-up stays in perspective.
- The outline width is set in pixels at 1080p (2 px by default) and converted to world units at each vertex's view depth, capped at 0.07 units so distant hulls don't balloon.
- Fine parts that opt out of the ink line (strings, chains, claws, small settings) are drawn at least 1.5 px wide at 1080p: the toon vertex shader pushes them out along their normals by the shortfall, estimated from the part's own half thickness (`2V / A`). A bowstring stays a visible line across a room and is untouched up close.
- Bands fall on the item's form, so relief finer than a band should not reach the shading normals. Under hard bands, a surface whose light sits near a band edge breaks into islands wherever small relief tips it over. The gambeson's quilting puffs stay in its silhouette, but the body shades with the normals of the coat without them; the quilting itself is meant to come from the `Quilted` fabric pattern once patterns are ported.
- Decay noise is in mesh-local space, so item parts keep an identity transform under the item root.
- Paint chips scale with the decay amount; in the prototype they showed at wear 0.
- Fabric patterns and etching are not ported yet.
