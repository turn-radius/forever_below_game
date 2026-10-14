//! Bodies for the six folk (human, dwarf, gnome, goblin, lizardfolk, pigfolk).
//! Reference: `bodyDims`, `mkArm`, `mkLeg`, `tSurf`, `headFn`, `garmentGeo` and the `sa*`
//! fitting functions in `reference/forever-below-plate.html`. Race parameters: `data/races.json`.
//! See `docs/body.md` for the fitting rules (grips, blade turn, shields, bows, straps).
//!
//! - [`BodySpec`]: one character's folk, build, face and hair, rolled from a seed.
//! - [`Body`]: the body measured from the spec, the Armory forms sized to it
//!   ([`Body::torso_form`], [`Body::lower_body_form`], [`Body::leg_form`],
//!   [`Body::hand_form`], [`Body::head_form`]), where each sits ([`Placement`]s), the anchors
//!   gear hangs from, and its skin ([`Body::skin_mesh`]).
//!
//! Plain Rust, no Bevy; every body is a pure function of its spec.

pub mod body;
pub mod folk;
pub mod hair;
pub mod hand;
pub mod head;
pub mod held;
pub mod limb;
pub mod place;
pub mod spec;
pub mod worn;

pub use body::{ArmPose, Body, GRIP_MAX, Pose};
pub use folk::{Beard, EyeKind, Ears, Folk, HairStyle, Nose, Race};
pub use hair::HairFit;
pub use head::HeadFit;
pub use hand::HandPose;
pub use held::{Hands, HeldFit, Hold, MAIN_HAND, OFF_HAND, fit_hands, fit_held};
pub use limb::{Limb, Side};
pub use place::Placement;
pub use spec::BodySpec;
pub use worn::{Covered, LAYER_ORDER, WornFit, WornLayer, fit_worn};
