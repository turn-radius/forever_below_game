//! Bedrolls and tents: set down from the pack onto open floor, slept in, and packed up again.
//!
//! - F on a bedroll or a tent in the pack (the inventory screen open, the mouse over it) sets it
//!   down in front of the delver ([`lay_out_hovered`]). It is the same item in its set-down form
//!   ([`fb_items::camp::set_up`]): a bedroll laid out, a tent pitched, drawn by the Armory in
//!   that form, lying in the level as any floor item does. It is a [`FloorItem`] with a
//!   [`SetCamp`] on it for the floor it covers, so it is kept with the level, in the world record
//!   as the item it now is, and is back where it was when the delver returns.
//! - Where it goes is the floor just ahead of the feet ([`site_ahead`]), turned square to the
//!   delver, a bedroll lengthways and a tent with its door toward them. [`check_site`] refuses a
//!   place it does not fit: a doorway, the way to a stair, rock or a wall, a prop or someone
//!   standing, or another camp. A refusal says why in the toast.
//! - A set tent blocks the way as a prop does ([`BlocksTheWay`]); a bedroll is walked over.
//!
//! Camp goods are drawn at their own size on the floor, not the tuning's larger floor scale: a
//! tent or bedroll set at 115% would no longer fit the room it was checked against.

use bevy::prelude::*;
use fb_items::camp::Camp;
use fb_level::{CELL, Cell, FloorShape, Level};

use crate::inventory::{FloorChange, FloorChanges, Hovered, Inventory, ItemId, Toast};
use crate::interact::{self, Slot, Verb};
use crate::keymap::{Action, ActionInput};
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Obstacle, Obstacles, Player, PlayerCamera, inventory_open};

pub struct CampPlugin;

impl Plugin for CampPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Toast>()
            .init_resource::<FloorChanges>()
            .add_systems(Update, lay_out_hovered.run_if(inventory_open).run_if(crate::barter::not_bartering).in_set(DelveSystems));
    }
}

/// How far in front of the feet the near edge of a camp lies, in world units (30 cm): clear of
/// the body, within reach of the eye.
pub const GAP: f32 = 3.0;

/// A set bedroll or tent, on its [`FloorItem`]: what it is and the floor it covers.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct SetCamp {
    pub camp: Camp,
    pub site: Site,
}

/// The floor a camp covers: a rectangle on the floor plane (world `(x, z)`), its half extents
/// along its own x and z, turned `yaw` about +Y as a [`Transform`]'s rotation turns it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Site {
    pub centre: Vec2,
    pub half: Vec2,
    pub yaw: f32,
}

/// How far two shapes may overlap before they count as overlapping, in world units: a camp may
/// lie flush with a wall.
const TOUCH: f32 = 0.05;

impl Site {
    /// The site's own x and z axes on the floor.
    fn axes(&self) -> [Vec2; 2] {
        let (sin, cos) = self.yaw.sin_cos();
        [Vec2::new(cos, -sin), Vec2::new(sin, cos)]
    }

    /// How far the rectangle reaches along the unit `axis`, from its centre.
    fn reach(&self, axis: Vec2) -> f32 {
        let [x, z] = self.axes();
        self.half.x * x.dot(axis).abs() + self.half.y * z.dot(axis).abs()
    }

    /// Whether the two rectangles overlap by more than [`TOUCH`] (the separating axis test).
    pub fn overlaps(&self, other: &Site) -> bool {
        let [ax, az] = self.axes();
        let [bx, bz] = other.axes();
        let apart = other.centre - self.centre;
        [ax, az, bx, bz].into_iter().all(|axis| self.reach(axis) + other.reach(axis) - apart.dot(axis).abs() > TOUCH)
    }

    /// Whether an obstacle on the floor is in the way of the site.
    pub fn blocked_by(&self, obstacle: &Obstacle) -> bool {
        match *obstacle {
            Obstacle::Circle { centre, radius } => {
                let [x, z] = self.axes();
                let apart = centre - self.centre;
                let local = Vec2::new(apart.dot(x), apart.dot(z));
                (local - local.clamp(-self.half, self.half)).length() < radius - TOUCH
            }
            Obstacle::Rect { centre, half, yaw } => self.overlaps(&Site { centre, half, yaw }),
        }
    }

    /// The cells the site's corners reach between, `(min, max)`.
    fn cells(&self) -> (IVec2, IVec2) {
        let reach = Vec2::new(self.reach(Vec2::X), self.reach(Vec2::Y));
        (((self.centre - reach) / CELL).floor().as_ivec2(), ((self.centre + reach) / CELL).floor().as_ivec2())
    }

    /// The floor of grid cell `(x, y)` as a site.
    fn cell(x: i32, y: i32) -> Site {
        Site { centre: (Vec2::new(x as f32, y as f32) + 0.5) * CELL, half: Vec2::splat(CELL * 0.5), yaw: 0.0 }
    }
}

/// Why a camp cannot be set where it would go. [`CampRefusal::reason`] is what the toast says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CampRefusal {
    /// It would lie across a doorway or a gate.
    Doorway,
    /// It would lie across a stair, or the cell you stand in to use one.
    Stair,
    /// It would overlap rock, a pillar, bars, or a cell the way is shut across.
    Wall,
    /// It would overlap a prop, someone standing, or another camp.
    Prop,
}

impl CampRefusal {
    pub fn reason(self) -> &'static str {
        match self {
            CampRefusal::Doorway => "It cannot be set in a doorway.",
            CampRefusal::Stair => "It cannot be set across a stair.",
            CampRefusal::Wall => "There is not room for it against the wall.",
            CampRefusal::Prop => "There is something in the way.",
        }
    }
}

/// The site for a camp of item-space half extents `half` (along its x and z) set down by a
/// delver at `feet` (world `(x, z)`) facing `forward` (a unit floor direction): its near edge
/// [`GAP`] ahead, square to the delver. Item +X points back at the delver, so a bedroll lies
/// lengthways with its foot to them and a tent has its door toward them.
pub fn site_ahead(feet: Vec2, forward: Vec2, half: Vec2) -> Site {
    Site { centre: feet + forward * (GAP + half.x), half, yaw: forward.y.atan2(-forward.x) }
}

/// Whether `site` is clear to set a camp on: not across a doorway, a stair, rock or a blocked
/// cell, a prop or someone standing, or another set camp (`camps`). The first thing found wins
/// in that order.
pub fn check_site(site: &Site, level: &Level, blocked_cells: &BlockedCells, obstacles: &[Obstacle], camps: &[Site]) -> Result<(), CampRefusal> {
    let (low, high) = site.cells();
    let (mut doorway, mut stair, mut wall) = (false, false, false);
    for y in low.y..=high.y {
        for x in low.x..=high.x {
            if !site.overlaps(&Site::cell(x, y)) {
                continue;
            }
            if level.cell(x, y).is_doorway() {
                doorway = true;
            } else if level.stairs.iter().any(|s| (s.x as i32, s.y as i32) == (x, y) || (s.approach().0 as i32, s.approach().1 as i32) == (x, y)) {
                stair = true;
            } else if !level.is_open(x, y) || level.cell(x, y) != Cell::Floor || blocked_cells.0.contains(&(x, y)) {
                wall = true;
            }
        }
    }
    if doorway {
        return Err(CampRefusal::Doorway);
    }
    if stair {
        return Err(CampRefusal::Stair);
    }
    if wall {
        return Err(CampRefusal::Wall);
    }
    if obstacles.iter().any(|o| site.blocked_by(o)) || camps.iter().any(|c| site.overlaps(c)) {
        return Err(CampRefusal::Prop);
    }
    Ok(())
}

/// The way the delver faces on the floor, from their eye: straight ahead of the view, or, looking
/// straight down, the way the top of the head points.
pub fn facing(eye: &GlobalTransform) -> Vec2 {
    eye.forward().as_vec3().xz().try_normalize().unwrap_or_else(|| eye.up().as_vec3().xz().normalize_or(Vec2::NEG_Y))
}

/// The floor `item` covers once set down as `camp`, centred over the origin of its mesh bounds:
/// half its item-space extents along x and z, and the centre of its bounds in item space.
pub fn footprint(item: &fb_items::Item) -> Option<(Vec2, Vec2)> {
    let (lo, hi) = item.shape.build().bounds()?;
    Some((Vec2::new(hi[0] - lo[0], hi[2] - lo[2]) * 0.5, Vec2::new(lo[0] + hi[0], lo[2] + hi[2]) * 0.5))
}

/// The line the toast says when `item` is set down.
pub fn set_down_line(item: &fb_items::Item) -> String {
    let name = item.title().to_lowercase();
    match item.camp() {
        Some(Camp::Tent) => format!("You pitch the {name}"),
        _ => format!("You lay out the {name}"),
    }
}

/// Sets camp good `id` down in front of the delver, if the place ahead of them takes it: the item
/// takes its set-down form where it is, comes out of the pack, and lies on the floor. The reason
/// it cannot, if so ([`CampRefusal`]), is `Err`. Returns what the toast says.
#[allow(clippy::too_many_arguments)]
pub fn lay_out(
    inventory: &mut Inventory,
    changes: &mut FloorChanges,
    id: ItemId,
    feet: Vec2,
    forward: Vec2,
    level: &Level,
    blocked_cells: &BlockedCells,
    obstacles: &[Obstacle],
    camps: &[Site],
) -> Result<String, &'static str> {
    let item = inventory.item(id).ok_or("That is not here.")?;
    let set = fb_items::camp::set_up(item).ok_or("That cannot be set down.")?;
    let (half, _) = footprint(&set).ok_or("That cannot be set down.")?;
    let site = site_ahead(feet, forward, half);
    check_site(&site, level, blocked_cells, obstacles, camps).map_err(CampRefusal::reason)?;
    let line = set_down_line(&set);
    // Out of its pack first: only a loose item changes form, since a grid holds it at the
    // footprint it had.
    inventory.take_out(id);
    if !inventory.replace_form(id, set) {
        return Err("That cannot be set down.");
    }
    changes.0.push(FloorChange::SetDownAt(id, site.centre / CELL, site.yaw));
    Ok(line)
}

/// F in the inventory on a bedroll or tent in the pack: sets it down in front of the delver
/// ([`lay_out`]), or says why not.
#[allow(clippy::too_many_arguments)]
fn lay_out_hovered(
    input: ActionInput,
    hovered: Res<Hovered>,
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    mut toast: ResMut<Toast>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    players: Query<&Transform, With<Player>>,
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Res<Obstacles>,
    camps: Query<&SetCamp>,
    time: Option<Res<Time<Real>>>,
) {
    if !input.just_pressed(Action::Primary) {
        return;
    }
    let Some((id, _)) = hovered.0 else { return };
    let Some(target) = interact::item_target(&inventory, id) else { return };
    if !matches!(interact::verbs_for(&target, &interact::Delver::default()).get(Slot::Primary), Some(Verb::LayOut | Verb::Pitch)) {
        return;
    }
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    let (Some(eye), Some(body)) = (eyes.iter().next(), players.iter().next()) else { return };
    let sites: Vec<Site> = camps.iter().map(|c| c.site).collect();
    match lay_out(&mut inventory, &mut changes, id, body.translation.xz(), facing(eye), &level.0, &blocked_cells, &obstacles.0, &sites) {
        Ok(line) => toast.say(line, now),
        Err(reason) => toast.say(reason, now),
    }
}

/// The floor shape a set camp blocks the way with: a tent, but not a bedroll, which is walked
/// over.
pub fn blocks_the_way(camp: Camp, site: &Site) -> Option<FloorShape> {
    (camp == Camp::Tent).then_some(FloorShape::Square { half_width: site.half.x, half_depth: site.half.y })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_level::{Facing, Stair, StairKind};

    /// A level of open floor, 8 by 8 cells, rock round the edge.
    fn room() -> Level {
        let mut level = fb_level::generate(1207, 1);
        level.width = 8;
        level.height = 8;
        level.cells = (0..64).map(|i| if i % 8 == 0 || i % 8 == 7 || i / 8 == 0 || i / 8 == 7 { Cell::Rock } else { Cell::Floor }).collect();
        level.stairs.clear();
        level
    }

    fn at_cell(x: f32, y: f32) -> Vec2 {
        Vec2::new(x, y) * CELL
    }

    fn clear(site: &Site, level: &Level) -> Result<(), CampRefusal> {
        check_site(site, level, &BlockedCells::default(), &[], &[])
    }

    #[test]
    fn a_site_lies_ahead_square_to_the_delver_with_item_x_toward_them() {
        let feet = at_cell(3.5, 3.5);
        for forward in [Vec2::NEG_Y, Vec2::X, Vec2::new(0.6, 0.8)] {
            let site = site_ahead(feet, forward, Vec2::new(8.5, 3.0));
            // Its near edge is the gap ahead of the feet.
            let near = site.centre - forward * site.half.x;
            assert!(((near - feet).dot(forward) - GAP).abs() < 1e-4, "{forward}");
            assert!((near - feet).perp_dot(forward).abs() < 1e-3);
            // Item +X, turned by the yaw, points back at the delver.
            let x = Quat::from_rotation_y(site.yaw) * Vec3::X;
            assert!(x.xz().abs_diff_eq(-forward, 1e-4), "{forward}: {x}");
        }
    }

    #[test]
    fn open_floor_takes_a_camp_and_rock_does_not() {
        let level = room();
        let half = Vec2::new(8.5, 3.0);
        let site = site_ahead(at_cell(3.5, 3.5), Vec2::NEG_Y, half);
        assert_eq!(clear(&site, &level), Ok(()));
        // Facing the wall from beside it: the camp would lie in the rock.
        let against = site_ahead(at_cell(3.5, 1.2), Vec2::NEG_Y, half);
        assert_eq!(clear(&against, &level), Err(CampRefusal::Wall));
        // A site laid exactly flush to the wall is not overlapping it.
        let flush = Site { centre: at_cell(3.5, 1.0) + Vec2::new(0.0, half.y), half, yaw: 0.0 };
        assert_eq!(clear(&flush, &level), Ok(()));
        // A cell the way is shut across counts as rock.
        let mut shut = BlockedCells::default();
        shut.0.insert((3, 3));
        assert_eq!(check_site(&site, &level, &shut, &[], &[]), Err(CampRefusal::Wall));
    }

    #[test]
    fn a_doorway_a_stair_a_prop_and_another_camp_refuse() {
        let mut level = room();
        let half = Vec2::new(8.5, 3.0);
        let site = site_ahead(at_cell(3.5, 3.5), Vec2::NEG_Y, half);
        let site_cell = (site.centre / CELL).floor().as_ivec2();
        let was = level.cells.clone();
        for door in [Cell::Door, Cell::Gate] {
            level.cells[(site_cell.y as u32 * level.width + site_cell.x as u32) as usize] = door;
            assert_eq!(clear(&site, &level), Err(CampRefusal::Doorway), "{door:?}");
        }
        level.cells = was;
        // The stair's own cell and the approach cell both refuse.
        level.stairs.push(Stair { x: site_cell.x as u32, y: site_cell.y as u32 - 1, facing: Facing::North, kind: StairKind::Down });
        assert_eq!(clear(&site, &level), Err(CampRefusal::Stair));
        level.stairs.clear();
        let chest = Obstacle::Rect { centre: site.centre + Vec2::new(0.0, 1.0), half: Vec2::new(3.0, 2.0), yaw: 0.3 };
        assert_eq!(check_site(&site, &level, &BlockedCells::default(), &[chest], &[]), Err(CampRefusal::Prop));
        let barrel = Obstacle::Circle { centre: site.centre, radius: 2.5 };
        assert_eq!(check_site(&site, &level, &BlockedCells::default(), &[barrel], &[]), Err(CampRefusal::Prop));
        let far = Obstacle::Circle { centre: site.centre + Vec2::new(0.0, 40.0), radius: 2.5 };
        assert_eq!(check_site(&site, &level, &BlockedCells::default(), &[far], &[]), Ok(()));
        let other = Site { centre: site.centre + Vec2::new(2.0, 1.0), half, yaw: 0.2 };
        assert_eq!(check_site(&site, &level, &BlockedCells::default(), &[], &[other]), Err(CampRefusal::Prop));
    }

    #[test]
    fn turned_rectangles_overlap_by_their_real_outlines() {
        let long = Site { centre: Vec2::ZERO, half: Vec2::new(10.0, 1.0), yaw: 0.0 };
        // Beside the long side, close: clear. Turned a quarter, the same rectangle reaches up.
        let beside = Site { centre: Vec2::new(0.0, 5.0), half: Vec2::new(2.0, 2.0), yaw: 0.0 };
        assert!(!long.overlaps(&beside));
        let turned = Site { yaw: std::f32::consts::FRAC_PI_2, ..long };
        let reached = Site { centre: Vec2::new(0.0, 8.0), half: Vec2::new(1.0, 1.0), yaw: 0.0 };
        assert!(turned.overlaps(&reached) && !long.overlaps(&reached));
        // A corner poking in at an angle.
        let diamond = Site { centre: Vec2::new(0.0, 3.0), half: Vec2::splat(2.0), yaw: std::f32::consts::FRAC_PI_4 };
        assert!(long.overlaps(&diamond));
    }

    #[test]
    fn a_bedroll_from_the_pack_is_laid_out_in_front_and_leaves_the_pack() {
        let level = room();
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 1).unwrap());
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let bedroll = inventory.add(fb_items::roll_item("Bedroll", 3).unwrap());
        inventory.stow(bedroll).unwrap();
        assert!(inventory.is_carried(bedroll));
        let mut changes = FloorChanges::default();
        let feet = at_cell(3.5, 3.5);
        let line = lay_out(&mut inventory, &mut changes, bedroll, feet, Vec2::NEG_Y, &level, &BlockedCells::default(), &[], &[]).unwrap();
        assert!(line.starts_with("You lay out the"), "{line}");
        // Out of the pack, loose, in its laid form, set down in front of the feet.
        assert_eq!(inventory.location(bedroll), Some(crate::inventory::Location::Loose));
        assert!(inventory.item(bedroll).unwrap().is_set_camp());
        assert_eq!(inventory.contents_count(pack), 0);
        let [FloorChange::SetDownAt(id, at, _)] = changes.0.as_slice() else { panic!("{:?}", changes.0) };
        assert_eq!(*id, bedroll);
        assert!(at.y * CELL < feet.y, "ahead is north: {at}");
    }

    #[test]
    fn a_refused_camp_stays_in_the_pack_as_it_was() {
        let level = room();
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 1).unwrap());
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let tent = fb_items::roll_item("Tent", 2).unwrap();
        let before = tent.clone();
        let tent = inventory.add(tent);
        inventory.stow(tent).unwrap();
        let mut changes = FloorChanges::default();
        // Facing the rock from the room's edge.
        let reason = lay_out(&mut inventory, &mut changes, tent, at_cell(3.5, 1.3), Vec2::NEG_Y, &level, &BlockedCells::default(), &[], &[]).unwrap_err();
        assert_eq!(reason, CampRefusal::Wall.reason());
        assert!(changes.0.is_empty());
        assert_eq!(inventory.item(tent), Some(&before));
        assert!(inventory.is_carried(tent), "still in the pack");
    }

    #[test]
    fn a_tent_blocks_the_way_and_a_bedroll_is_walked_over() {
        let site = Site { centre: Vec2::ZERO, half: Vec2::new(9.0, 7.0), yaw: 0.0 };
        assert_eq!(blocks_the_way(Camp::Tent, &site), Some(FloorShape::Square { half_width: 9.0, half_depth: 7.0 }));
        assert_eq!(blocks_the_way(Camp::Bedroll, &site), None);
    }

    #[test]
    fn set_camps_have_no_verbs_to_take_or_equip() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 1).unwrap());
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let bedroll = inventory.add(fb_items::roll_item("Bedroll", 3).unwrap());
        inventory.stow(bedroll).unwrap();
        let verbs = |inventory: &Inventory| interact::verbs_for(&interact::item_target(inventory, bedroll).unwrap(), &interact::Delver::default());
        assert_eq!(verbs(&inventory).primary, Some(Verb::LayOut));
        let tent = inventory.add(fb_items::roll_item("Tent", 3).unwrap());
        inventory.stow(tent).unwrap();
        let tent_verbs = interact::verbs_for(&interact::item_target(&inventory, tent).unwrap(), &interact::Delver::default());
        assert_eq!(tent_verbs.primary, Some(Verb::Pitch));
        // Once down, a bedroll is taken by neither Take nor Equip.
        inventory.take_out(bedroll);
        let laid = fb_items::camp::set_up(inventory.item(bedroll).unwrap()).unwrap();
        assert!(inventory.replace_form(bedroll, laid));
        assert_eq!(verbs(&inventory), interact::Verbs::default());
    }
}
