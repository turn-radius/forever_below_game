//! A socket: the hollow cone that seats a head on the end of a haft, with raised rings and
//! langets (strips nailed down the haft). Spears use it; javelins, pikes and other socketed
//! polearm heads can reuse it. Axes use it for the eye their head is mounted through
//! (`length` is the eye's height, `flare` its girth). From the prototype's `socket` section.

use crate::material::MaterialKind;
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Socket {
    pub on: bool,
    pub length: f32,
    /// Widest radius, as a multiple of the haft's radius.
    pub flare: f32,
    /// Raised rings round the socket.
    pub rings: u32,
    pub facets: u32,
    /// Strips running down the haft from the socket's mouth.
    pub langets: u32,
    pub langet_length: f32,
    pub material: MaterialKind,
}

sliders! {
    SocketLimits for Socket {
        length: 0.3, 3.0, 0.01;
        flare: 1.0, 2.0, 0.01;
        rings: 0.0, 5.0, 1.0;
        facets: 3.0, 32.0, 1.0;
        langets: 0.0, 4.0, 1.0;
        langet_length: 0.3, 4.0, 0.05;
    }
}

impl Default for Socket {
    fn default() -> Self {
        Socket { on: true, length: 1.3, flare: 1.2, rings: 2, facets: 8, langets: 0, langet_length: 1.2, material: MaterialKind::Iron }
    }
}
