//! Hats: a stiffened crown over the head and a brim round it, of felt or wool. The Armory
//! prototype has no hat section (the plate draws its `Pointed hat` as a torus brim under a
//! swept cone); ranges here are this port's own.

use crate::fabric::Fabric;
use crate::material::Rgb;
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HatStyle {
    /// A tall cone of felt, its tip slumping over, on a modest brim.
    Pointed,
    /// A rounded crown on a broad brim that may droop.
    WideBrim,
    /// A low felt cap close over the crown, its brim turned up or none at all; a cap whose top
    /// falls forward is a Phrygian cap.
    Cap,
}

impl HatStyle {
    pub const ALL: [HatStyle; 3] = [HatStyle::Pointed, HatStyle::WideBrim, HatStyle::Cap];
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hat {
    pub style: HatStyle,
    /// How loose it sits off the head, 0 (close) to 1 (loose).
    pub ease: f32,
    /// Height of the crown above the band.
    pub crown: f32,
    /// How far the top of the crown slumps over: backward on a pointed hat, forward on a cap.
    pub bend: f32,
    /// How far the brim reaches out from the crown (0 for none).
    pub brim: f32,
    /// The brim's set: -1 turned straight up against the crown, 0 flat, 1 drooping steeply.
    pub droop: f32,
    /// How unevenly the felt has been blocked: dents in the crown and waves in the brim.
    pub crumple: f32,
    /// Height of the band round the base of the crown (0 for none).
    pub band: f32,
    pub cloth: Fabric,
    pub band_cloth: Fabric,
    /// A feather tucked into the band.
    pub feather: bool,
    pub feather_color: Rgb,
}

sliders! {
    HatLimits for Hat {
        ease: 0.0, 1.0, 0.01;
        crown: 0.5, 5.0, 0.05;
        bend: 0.0, 1.0, 0.01;
        brim: 0.0, 2.6, 0.01;
        droop: -1.0, 1.0, 0.01;
        crumple: 0.0, 1.0, 0.01;
        band: 0.0, 0.5, 0.01;
    }
}

impl Default for Hat {
    /// A plain felt pointed hat.
    fn default() -> Self {
        Hat {
            style: HatStyle::Pointed,
            ease: 0.4,
            crown: 3.2,
            bend: 0.4,
            brim: 0.9,
            droop: 0.15,
            crumple: 0.3,
            band: 0.0,
            cloth: Fabric::default(),
            band_cloth: Fabric::plain(Fabric::TRIM_DYES[0]),
            feather: false,
            feather_color: Rgb::hex(0xe8e2d0),
        }
    }
}
