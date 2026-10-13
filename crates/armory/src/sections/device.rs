//! Devices: the simple emblems the prototype engraves on signets and medallions, paints on
//! shields and stencils on crates (`DEVICES` and `devicePts` in
//! `reference/spline_armory_engine.js`). Their outlines are in [`crate::build::device`].

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Device {
    None,
    Cross,
    Star,
    Crescent,
    Tower,
    /// A stave with two or three branches, different per item.
    Rune,
    Wheel,
}

impl Device {
    pub const ALL: [Device; 7] = [Device::None, Device::Cross, Device::Star, Device::Crescent, Device::Tower, Device::Rune, Device::Wheel];
}
