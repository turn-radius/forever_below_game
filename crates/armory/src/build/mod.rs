//! Geometry builders shared between families. Each takes section structs from
//! [`crate::sections`] (never a family's whole state) and returns [`crate::geom::MeshBuilder`]s
//! in a local space the family places.
//!
//! - [`hafted`]: shaft, bands, grip, wrap, rivets and pommel, and a club's swelling (staves,
//!   maces, swords, spears, daggers, clubs, axes).
//! - [`head`]: collar, head body, flanges, spikes, prongs and finial, and the whole
//!   [`head::head_assembly`] (maces, staves).
//! - [`gem`]: cut stones, bezels, collets and claw settings (head gems, pommels, guards,
//!   rings, amulets, charms).
//! - [`socket`]: a socket sleeved over a haft, its rings and langets (spears).
//!   [`head::head_assembly`] (maces, staves), and spikes driven into a haft (clubs).
//! - [`spearhead`]: a spearhead blade and its wings or bar (spears).
//! - [`limb`]: bow limbs and crossbow prods, their nocks, the string, a cord wound on a handle.
//! - [`arrow`]: arrows and crossbow bolts.
//! - [`parts`]: [`parts::Parts`], collecting meshes and their looks into an item.
//! - [`blade`]: blades of every profile and section (swords, daggers, spears).
//! - [`guard`]: crosses, bars, discs and lobes, with terminals, langets and stones (swords,
//!   daggers, spears).
//! - [`hilt`]: a whole hilted blade, handle to point (swords, daggers).
//! - [`strap`]: straps laid on a surface, buckles, D-rings and rivets (bags, chests, belts,
//!   armour, boots).
//! - [`hoard`]: spilled and heaped coins (purses, treasure chests).
//! - [`container`]: a container's opening and inside ([`container::ContainerSpace`]).
//! - [`plank`]: boards, planked panels and planked round heads (crates, barrels, chests).
//! - [`fitting`]: box hardware: drop handles, lock plates, hasps, padlocks.
//! - [`device`]: emblem outlines and their raised or painted shapes (stencils, charges).
//! - [`plate`]: curved plates, any outline laid onto any surface with a thickness, creases kept
//!   hard (shield boards, helmet shells and everything riveted on them).
//! - [`form`]: the mannequin's body-segment forms, thickened sheets, plate lames and edges.
//! - [`garment`]: draped cloth, sleeves, trims, belts and plate fittings on a torso form.
//! - [`hand`]: gloves and gauntlets on a hand form.
//! - [`boot`]: footwear and greaves on a leg form.
//! - [`chain`]: cable chains, twisted cords, thongs and beads along a path (necklaces,
//!   charm hangers, straps, lantern chains).
//! - [`emblem`]: plaque outlines (signet faces, pendants, badges).
//! - [`axe_head`]: an eye, a bit mounted crosswise through it, and a poll (axes).
//! - [`legwear`]: trousers, breeches and leggings on a lower-body form.
//! - [`hood`]: hoods, cowls and coifs on a head form and the shoulders.
//! - [`hat`]: hats, crown and brim, on a head form.
//! - [`belt`]: a belt buckled round a torso form, with frogs, loops and rings.
//! - [`key`]: keys with ring, trefoil or lozenge bows and warded bits.
//! - [`roll`]: rolled sheets and the straps round them (bedrolls, bandages).
//! - [`light`]: carried light ([`light::LightAnchor`]), flames and embers (torches,
//!   lanterns, candles, glowcaps).

pub mod arrow;
pub mod axe_head;
pub mod belt;
pub mod blade;
pub mod boot;
pub mod chain;
pub mod container;
pub mod device;
pub mod emblem;
pub mod fitting;
pub mod form;
pub mod garment;
pub mod gem;
pub mod guard;
pub mod hafted;
pub mod hat;
pub mod hand;
pub mod head;
pub mod hilt;
pub mod hoard;
pub mod hood;
pub mod key;
pub mod legwear;
pub mod light;
pub mod limb;
pub mod parts;
pub mod plank;
pub mod plate;
pub mod roll;
pub mod socket;
pub mod spearhead;
pub mod strap;
