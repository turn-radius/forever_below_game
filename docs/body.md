# Bodies and fitting

This is how the prototype builds and dresses bodies. The Rust version should go further: better hands, faces and cloth. The fitting rules under "Held gear" came from review and still hold. Read alongside `reference/forever-below-plate.html`. Folk parameters are in `data/races.json`. Units are 10 cm, so a human is about 17.2 units tall.

## The body (`bodyDims`)

- Height `h = race.h * hs` (hs is 0.95–1.05). Proportions scale with `s = h / 17.5`, and girth `ws = s^0.72 * race.girth * (0.88 + frame * 0.3)`.
- The torso is a spline of cross-sections (hip, waist, chest, shoulder, neck). Each section is a superellipse with exponent 0.77 (`tSurf`), giving a slightly boxy, carved look. The belly and bust bulge only at the front.
- Arms (`mkArm`) and legs (`mkLeg`) are tubes swept along Catmull-Rom splines through shoulder, elbow and wrist, or hip, knee and ankle, with radius profiles that taper. Each arm root starts inside the chest, so sleeve openings never show skin.
- The head (`headFn`) is an ellipsoid with a narrower jaw, a chin, and a flatter face. Each folk adds its own features: snout, pig nose, long ears, frill, tusks.
- Hair, brows and beards are offset surfaces of the head with lumpy noise. Long hair drapes over the shoulders and clears the clothing.

## Worn gear

Clothing is not modelled per item. Each garment is an **offset of the body surface** (`garmentGeo`), shaped by the Armory state: style, length, fit, flare, folds, sleeves, neck, trim and pattern. That lets one Armory tunic fit a gnome and a pigfolk. Layers stack outward with a running `maxOff` (shirt, legs, armour, neck, cloak, belt, then back). Boots extend the foot surface; gloves and gauntlets wrap the hand and cuff. Helmets are the Armory helm itself, resized so its radius matches the head. Amulets use the Armory pendant, with the chain drawn on the body surface from the back of the neck down to the pendant. Bags are the Armory bag on the back, with straps pushed onto the worn surface (`pushOut`, `strapCurve`).

## Held gear (`saHeld`, `fitGrip`)

These rules came from review and are deliberate:

- **Grips fit the hand.** The effective grip radius (shaft radius × grip thickness × wrap) is capped at `clamp(arm_radius * 0.27, 0.07, GRIP_MAX = 0.13)`, about a torch handle. Blades and heads keep their size.
- **One-handed weapons** (sword, dagger, mace, club, hand axe) stand perpendicular to the forearm, through the fist. The weapon arm bends at the elbow.
- **Swords and daggers** turn `BLADE_TURN = 55°` about the grip so an edge leads, and draw at `BLADE_SCALE = 0.78 × sqrt(s)`.
- **Hand axes** turn 90° about the haft so the bit faces forward.
- **Bows** close the fist on the riser (the bow's origin), stand upright with the string back toward the archer, and are capped at `BOW_MAX = 0.62` of the archer's height.
- **Shields** strap at the fist with the forearm along the back, held clear of the hand, and are capped at `SHIELD_MAX = 0.42` of the bearer's height.
- Staves and spears stand upright in the hand.

All constants are in `data/constants.json`.
