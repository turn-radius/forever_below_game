# Bodies and fitting

This is how the prototype builds and dresses bodies. The Rust version should go further: better hands, faces and cloth. The fitting rules under "Held gear" came from review and still hold. Read alongside `reference/forever-below-plate.html`. Folk parameters are in `data/races.json`. Units are 10 cm, so a human is about 17.2 units tall.

## The body (`bodyDims`)

- Height `h = race.h * hs` (hs is 0.95–1.05). Proportions scale with `s = h / 17.5`, and girth `ws = s^0.72 * race.girth * (0.88 + frame * 0.3)`.
- The torso is a spline of cross-sections (hip, waist, chest, shoulder, neck). Each section is a superellipse with exponent 0.77 (`tSurf`), giving a slightly boxy, carved look. The belly and bust bulge only at the front.
- Arms (`mkArm`) and legs (`mkLeg`) are tubes swept along Catmull-Rom splines through shoulder, elbow and wrist, or hip, knee and ankle, with radius profiles that taper. Each arm root starts inside the chest, so sleeve openings never show skin.
- The head (`headFn`) is an ellipsoid with a narrower jaw, a chin, and a flatter face. Each folk adds its own features: snout, pig nose, long ears, frill, tusks.
- Hair, brows and beards are offset surfaces of the head with lumpy noise. Long hair drapes over the shoulders and clears the clothing.

### In the Rust build (`crates/fb_body`)

`BodySpec::roll(folk, seed)` rolls the plate's body fields (`C.form` is `figure`, `C.hs` is `stature`); `Body::new(&spec)` measures it as `bodyDims` does, in body space (soles on y = 0, face +Z, the wearer's left +X). Departures, all our own:

- **One body for skin and clothes.** The body yields the Armory's forms sized to itself (`torso_form`, `lower_body_form`, `leg_form`, `hand_form`, `head_form`) and where each sits (`Placement`s), so the garments, trousers, boots, gloves and hoods already built on those forms fit it unchanged. `TorsoForm` gained `Bulge`s (belly, bust and seat) that `TorsoProfile::point` adds, so cloth drapes over a belly instead of through it.
- **A chest and a seat.** The plate swells a bust only past figure 0.55 and leaves the back flat. Every trunk here also has a slight swell of the breast muscles and a seat behind the hips.
- **Limb radii** run through their knots on a smooth cubic (Catmull-Rom slopes) rather than the plate's piecewise-linear `lerp3`, with a small shoulder cap. The forearm flattens toward the wrist to the hand form's wrist (wide across the palm, thin through it), so arm and hand meet without a step. Legs swell at the calf behind and the knee in front.
- **Hands** are the glove builder's palm and digit tubes ([`armory::build::hand`]): four fingers and a thumb that curl joint by joint (relaxed, or closed round a grip with each joint bent by phalanx length over grip radius), the ball of the thumb as a pad, and nails. Their length is the plate's `handL`; the knuckles widen with the wrist, so dwarf hands are broad and goblin hands slender. The plate's hand was an ellipsoid and a thumb.
- **Feet** are the boot builder's `LegForm` foot, sized from the plate's `footFn` (length `0.148 h * race.foot`), turned out 0.12 rad. Pigfolk keep the plate's darker trotters; lizardfolk get three bone claws per foot.
- **Heads** (`Body::head_mesh`, `Body::face`) are the plate's `headFn` on `HeadForm`, with a little relief added to the skin only (hoods and helmets still fit the plain form): a brow ridge, eye sockets the eyes sit in, cheekbones, and a muzzle and chin so the profile is more than a flat face and a nose. Human eyes have an upper lid, so they read at doll size and hold an expression close up. Eye whites, irises, pupils and the mouth carry no material (a tint only), so the character's dirt never clouds them. Ears are cupped sheets with a rolled rim instead of ellipsoids and extruded blades. A lizard's snout has a jaw line with a row of teeth, and its frill is spines joined by a membrane (drawn with the head whatever its hair, in its hair colour). The pig's snout disc is deeper than the plate's, and its flop ears are broader and fold further. Ear size, eye spacing and mouth width get small seeded nudges on top of the spec's jaw, brow and nose.
- **Hair, brows and beards** (`Body::hair_mesh(HairFit)`) follow the plate's `buildHair`, brows and `buildBeard`, with a little thickness on every sheet and seeded value noise over the plate's ripples, so two heads of one style differ. Long hair hangs from behind the ears only (the plate started it beside the face, which wrapped a mantle over the chest) and is pushed clear of the trunk and the upper arms, outward along the ray from the body's axis, by `HairFit::clearance` plus a margin; a ponytail rests on the back at the same clearance. A beard's sides by the ears stay short and it narrows as it hangs, so it falls from the chin and jaw instead of over the shoulders, and it rests on the chest. `HairFit::covered` (under a helmet or hat) drops the cap, ridge and topknot, as the plate does; `Body::hair_volume` is the plate's `hairVol` for sizing a helmet over the hair. Hair is `MaterialKind::Hair`, which dirties as skin does.
- **Skin** is its own material (`MaterialKind::Skin`, decay type 13): it wears with the character's dirt, a dull film and smudges pooling low on the body. Lizardfolk skin carries the Armory's `Scales` pattern.

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
