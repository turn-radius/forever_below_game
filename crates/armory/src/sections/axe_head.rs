//! An axe head: a bit (a wedge-sectioned blade profile) mounted crosswise through an eye on
//! the haft, and whatever backs it (the poll). Axes use it; bardiches, halberds, poleaxes and
//! picks can reuse it. The eye is a [`crate::sections::socket::Socket`] (its length is the
//! eye's height, its flare the eye's outer radius, its rings and langets the eye's).
//!
//! Not in the prototype's Armory: its plate faked a hand axe as an extruded outline on a
//! stick (`reference/forever-below-plate.html`). The sliders here are new.
//!
//! Head space: y = 0 is the middle of the eye and the haft runs along Y; the bit reaches
//! toward +X, the poll toward -X, and the bit's cheeks face ±Z.

use crate::material::MaterialKind;
use crate::sections::blade::BladeSection;
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

/// What backs the eye, opposite the bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxePoll {
    /// Nothing: the eye's rounded back.
    Plain,
    /// A squared hammer face.
    Hammer,
    /// A back spike (a horseman's axe).
    Spike,
    /// A second bit, the first mirrored (a double-bitted axe).
    Bit,
}

impl AxePoll {
    pub const ALL: [AxePoll; 4] = [AxePoll::Plain, AxePoll::Hammer, AxePoll::Spike, AxePoll::Bit];
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxeHead {
    /// From the haft's axis to the farthest point of the edge.
    pub reach: f32,
    /// Height of the bit where it leaves the eye.
    pub root: f32,
    /// How far the edge's upper corner (the toe) rises above the root.
    pub toe: f32,
    /// How far the edge's lower corner (the heel) hangs below the root: a bearded axe's beard.
    pub beard: f32,
    /// How the flanks run from the eye to the edge: 0 straight, 1 a narrow neck that flares
    /// late into the edge.
    pub waist: f32,
    /// How far the edge bows out, as a fraction of its height (a crescent).
    pub curve: f32,
    /// Raises (+) or lowers (-) the edge against the eye, as on a throwing axe.
    pub droop: f32,
    /// Thickness of the bit's cheeks next to the eye.
    pub thickness: f32,
    /// Length of the edge bevel, as a fraction of the bit.
    pub bevel: f32,
    /// The shape across the bit's height: flat cheeks, domed, or ridged down the middle.
    pub section: BladeSection,
    pub poll: AxePoll,
    /// How far a hammer or spike poll stands out behind the eye.
    pub poll_length: f32,
    pub material: MaterialKind,
}

sliders! {
    AxeHeadLimits for AxeHead {
        reach: 0.4, 3.0, 0.01;
        root: 0.1, 1.2, 0.01;
        toe: 0.0, 1.5, 0.01;
        beard: 0.0, 2.0, 0.01;
        waist: 0.0, 1.0, 0.01;
        curve: 0.0, 0.5, 0.01;
        droop: -0.6, 0.6, 0.01;
        thickness: 0.03, 0.3, 0.005;
        bevel: 0.04, 0.4, 0.01;
        poll_length: 0.05, 1.5, 0.01;
    }
}

impl Default for AxeHead {
    /// A plain hand axe.
    fn default() -> Self {
        AxeHead {
            reach: 1.0,
            root: 0.35,
            toe: 0.12,
            beard: 0.2,
            waist: 0.3,
            curve: 0.12,
            droop: 0.0,
            thickness: 0.12,
            bevel: 0.15,
            section: BladeSection::Flat,
            poll: AxePoll::Plain,
            poll_length: 0.3,
            material: MaterialKind::Iron,
        }
    }
}

impl AxeHead {
    /// Height of the cutting edge, toe to heel.
    pub fn edge_height(&self) -> f32 {
        self.root + self.toe + self.beard
    }
}
