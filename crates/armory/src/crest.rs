//! A crest: a faction's arms as one flat shield-shaped face, made from the same [`Field`] that
//! paints its banners and shields. The face takes the field's division from the surface
//! pattern shader and the charge from [`device_shapes`], exactly as a banner does
//! ([`crate::families::banner`]), so a new division or device shows up on both without extra
//! work. Item space: the shield stands in the XY plane, one unit wide, its face toward +Z.

use crate::build::device::device_shapes;
use crate::build::parts::{Look, Parts};
use crate::build::plate::{PlateOptions, Surface, plate};
use crate::geom::{ItemMesh, MeshBuilder, P2, V3};
use crate::heraldry::Field;
use crate::material::{MaterialKind::Cloth, Rgb};
use crate::sections::device::Device;

/// The shield's width, the unit its device is sized by.
pub const WIDTH: f32 = 1.0;
/// Its height from the top edge to the point.
pub const HEIGHT: f32 = 1.3;
/// How far the top edge stands above the shield's origin.
const TOP: f32 = 0.6;

/// The face is flat.
struct Flat;

impl Surface for Flat {
    fn point(&self, p: P2) -> V3 {
        [p[0], p[1], 0.0]
    }
}

/// The shield's outline, counter-clockwise from its top left: straight sides and top, a point
/// at the foot.
pub fn outline() -> Vec<P2> {
    let hw = WIDTH * 0.5;
    let side = 0.0;
    let foot = TOP - HEIGHT;
    let n = 18;
    let curve = |i: usize| {
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / n as f32;
        (a.cos().powf(0.85), side + (foot - side) * a.sin().powf(1.25))
    };
    let mut ring = vec![[-hw, TOP], [-hw, side]];
    for i in 1..n {
        let (x, y) = curve(i);
        ring.push([-hw * x, y]);
    }
    ring.push([0.0, foot]);
    for i in (1..n).rev() {
        let (x, y) = curve(i);
        ring.push([hw * x, y]);
    }
    ring.extend([[hw, side], [hw, TOP]]);
    ring
}

/// The seed a field's runes are cut from, so a rune charge reads the same on every banner and
/// crest of its faction. A pure function of the field.
pub fn device_seed(field: &Field) -> u32 {
    let text = format!("{:?}{:?}{:?}{:?}", field.division, field.tincture, field.charge_tincture, field.charge);
    text.bytes().fold(0x811C_9DC5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

/// The crest for `field`: a face painted as a banner's cloth is, and its charge laid on.
pub fn crest_mesh(field: &Field) -> ItemMesh {
    let ring = outline();
    let cell = [0.05, 0.05];
    let (min_y, max_y) = (TOP - HEIGHT, TOP);
    let uv_rect = Some(([-WIDTH * 0.5, min_y], [WIDTH, max_y - min_y]));
    let sheet = PlateOptions { thickness: 0.0, cell, crease: 1.2, uv_rect, ..Default::default() };
    let look = if field.painted {
        Look { pattern: field.surface(), ..Look::of(Cloth).tinted(field.tincture.rgb()) }
    } else {
        Look::of(Cloth).tinted(Rgb::hex(0xcbbf9f))
    };
    let mut parts = Parts::default();
    let mut face = plate(&ring, &[], &Flat, &sheet).face;
    face.make_double_sided();
    parts.add_look("cloth", face, look);
    if field.painted && field.charge != Device::None {
        let size = field.charge_size * WIDTH;
        let centre = [0.0, (min_y + max_y) * 0.5];
        let place = |p: &P2| [p[0] * size + centre[0], p[1] * size + centre[1]];
        let options = PlateOptions { thickness: 0.0, offset: 0.02, cell, crease: 1.2, ..Default::default() };
        let mut charge = MeshBuilder::new();
        for (piece, holes) in device_shapes(field.charge, device_seed(field)) {
            let piece: Vec<P2> = piece.iter().map(place).collect();
            let holes: Vec<Vec<P2>> = holes.iter().map(|hole| hole.iter().map(place).collect()).collect();
            charge.append(&plate(&piece, &holes, &Flat, &options).face);
        }
        parts.add_look("charge", charge, Look::of(Cloth).tinted(field.charge_tincture.rgb()));
    }
    parts.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heraldry::{Division, Tincture};

    fn field(division: Division, charge: Device) -> Field {
        Field { painted: true, division, tincture: Tincture::Azure, tincture2: Tincture::Argent, charge, charge_tincture: Tincture::Or, charge_size: 0.5 }
    }

    #[test]
    fn a_crest_is_painted_from_its_field() {
        let f = field(Division::Quarterly, Device::Star);
        let mesh = crest_mesh(&f);
        let cloth = mesh.parts.iter().find(|p| p.name == "cloth").expect("a face");
        assert_eq!(cloth.tint, Some(Tincture::Azure.rgb()));
        assert_eq!(cloth.pattern, f.surface(), "the banner's own pattern");
        let charge = mesh.parts.iter().find(|p| p.name == "charge").expect("a charge");
        assert_eq!(charge.tint, Some(Tincture::Or.rgb()));
        assert!(crest_mesh(&field(Division::Plain, Device::None)).parts.iter().all(|p| p.name != "charge"));
    }

    #[test]
    fn every_division_and_device_makes_a_crest() {
        for division in Division::ALL {
            for device in [Device::None, Device::Cross, Device::Star, Device::Crescent, Device::Tower, Device::Wheel, Device::Rune] {
                assert!(!crest_mesh(&field(division, device)).parts.is_empty(), "{division:?} {device:?}");
            }
        }
    }
}
