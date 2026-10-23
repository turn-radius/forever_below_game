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
//! - [`Rig`], [`Skinner`] and [`Gait`]: the joints a body is posed by, the weights that bind
//!   its skin and gear to them, and its walk and idle ([`RigPose`]).
//!
//! Plain Rust, no Bevy; every body is a pure function of its spec.

pub mod body;
pub mod coat;
pub mod gait;
pub mod folk;
pub mod hair;
pub mod hand;
pub mod head;
pub mod held;
pub mod limb;
pub mod parts;
pub mod place;
pub mod rig;
pub mod skin;
#[cfg(test)]
mod denizen_tests;
#[cfg(test)]
mod skin_tests;
#[cfg(test)]
mod shoulder_tests;
#[cfg(test)]
mod hood_tests;
#[cfg(test)]
mod pelvis_tests;
pub mod spec;
pub mod worn;

pub use body::{ArmPose, Body, GRIP_MAX, Pose};
pub use gait::{Gait, IdleCycle};
pub use folk::{Beard, EyeKind, Ears, Folk, HairStyle, Nose, Race};
pub use hair::HairFit;
pub use head::{HeadFit, StandingFeatures};
pub use hand::HandPose;
pub use held::{HIP_SIDE, Hands, HeldFit, HipFit, Hold, MAIN_HAND, OFF_HAND, fit_hands, fit_held, fit_hip};
pub use limb::{Limb, Side};
pub use place::Placement;
pub use rig::{JOINTS, Joint, Quat, Rig, RigPose, Rigid};
pub use skin::{PartSkin, SkinRegion, Skinner};
pub use spec::{BodyShape, BodySpec, STATURE, ShapeSlider};
pub use worn::{Covered, LAYER_ORDER, WornFit, WornLayer, fit_worn};
