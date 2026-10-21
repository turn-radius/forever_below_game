//! A faction's crest as UI nodes, shared by the factions tab and the map.
//!
//! The crest is drawn flat from the faction's heraldic field (`armory::heraldry::Field`): its
//! first tincture, the division in the second on a six by six grid of cells, and the charge as
//! a small boss at the fess point. The banners show the same field in the level. A faction not
//! yet met has no crest to show, only a [`plain_shield`] of no colours.

use armory::heraldry::{Division, Field, Tincture};
use bevy::prelude::*;

use crate::ui::{BG, FAINT, FRAME_GOLD, MUTED};

/// Cells across and down the crest's field.
pub const CELLS: usize = 6;

/// A tincture's colour on the page.
pub fn tincture_colour(tincture: Tincture) -> Color {
    let armory::material::Rgb(r, g, b) = tincture.rgb();
    Color::srgb(r, g, b)
}

/// Whether cell (`i` across, `j` down) of the crest's grid is in the field's second tincture.
pub fn second_tincture(division: Division, i: usize, j: usize) -> bool {
    let x = (i as f32 + 0.5) / CELLS as f32;
    let y = (j as f32 + 0.5) / CELLS as f32;
    let (i, j) = (i as i32, j as i32);
    match division {
        Division::Plain => false,
        Division::PerPale => x > 0.5,
        Division::PerFess => y > 0.5,
        Division::Quarterly => (x > 0.5) != (y > 0.5),
        Division::PerBend => y > x,
        Division::PerChevron => y > 0.35 + (x - 0.5).abs(),
        Division::PerSaltire => (x - 0.5).abs() < (y - 0.5).abs(),
        Division::Paly => i % 2 == 1,
        Division::Barry => j % 2 == 1,
        Division::Bendy => (i - j).rem_euclid(4) < 2,
        Division::Checky => (i + j) % 2 == 1,
        Division::Lozengy => (i + j) % 3 == 0,
    }
}

/// The shield's node: `w` by `h` logical pixels, its bottom corners rounded to a point.
fn shield_node(w: f32, h: f32) -> Node {
    let border = (w / 23.0).clamp(1.0, 3.0);
    Node {
        width: px(w),
        height: px(h),
        flex_shrink: 0.0,
        border: UiRect::all(px(border)),
        border_radius: BorderRadius { top_left: px(3).into(), top_right: px(3).into(), bottom_right: px(w * 0.5).into(), bottom_left: px(w * 0.5).into() },
        overflow: Overflow::clip(),
        position_type: PositionType::Relative,
        ..default()
    }
}

/// The crest: a shield `w` by `h` of the field's tinctures.
pub fn crest<'a>(parent: &'a mut ChildSpawnerCommands, field: &Field, w: f32, h: f32) -> EntityCommands<'a> {
    let mut shield = parent.spawn((Name::new("Crest"), shield_node(w, h), BackgroundColor(tincture_colour(field.tincture)), BorderColor::all(FRAME_GOLD)));
    shield.with_children(|shield| {
        if field.painted && field.division != Division::Plain {
            let second = tincture_colour(field.tincture2);
            for j in 0..CELLS {
                for i in 0..CELLS {
                    if second_tincture(field.division, i, j) {
                        shield.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: percent(100.0 * i as f32 / CELLS as f32),
                                top: percent(100.0 * j as f32 / CELLS as f32),
                                width: percent(100.0 / CELLS as f32 + 1.0),
                                height: percent(100.0 / CELLS as f32 + 1.0),
                                ..default()
                            },
                            BackgroundColor(second),
                        ));
                    }
                }
            }
        }
        if field.charge != armory::sections::device::Device::None {
            let size = 100.0 * field.charge_size.clamp(0.2, 0.8) * 0.7;
            shield.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(50.0 - size * 0.5),
                    top: percent(42.0 - size * 0.5),
                    width: percent(size),
                    height: percent(size * w / h),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(6)),
                    ..default()
                },
                BackgroundColor(tincture_colour(field.charge_tincture)),
                BorderColor::all(BG),
            ));
        }
    });
    shield
}

/// A shield of no colours or device, for ground whose faction is not yet known.
pub fn plain_shield<'a>(parent: &'a mut ChildSpawnerCommands, w: f32, h: f32) -> EntityCommands<'a> {
    parent.spawn((Name::new("Plain shield"), shield_node(w, h), BackgroundColor(FAINT), BorderColor::all(MUTED)))
}
