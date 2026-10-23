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
//! - G on a set camp (in reach, or hovered underfoot with the inventory open) packs it back up
//!   ([`pack`]): the same item in its carried form, in the first bag with room, its wear and
//!   history as they were. A full pack refuses, and the camp stays as it is.
//! - A set tent blocks the way as a prop does ([`BlocksTheWay`]); a bedroll is walked over.
//! - F on a set camp within reach sleeps in it ([`go_to_sleep`]): the delver is [`Asleep`], the
//!   view goes dark, the world holds still round them but for the game clock, which runs at the
//!   camp rules' `sleep_clock_ratio` (a night in under a minute), and their needs step as
//!   `Activity::Sleeping` ([`crate::simulate`]): sleepiness falls at the needs' own rate in a
//!   bedroll and `tent_rest` times that in a tent. A light they carry is put out first. They
//!   wake ([`wake_up`]) rested, or early when something hostile comes within `wake_distance`,
//!   when hunger or thirst reaches the Famished band, when they are hurt, or at a key press. They
//!   cannot lie down while tired too little, hungry or thirsty, or with something hostile near.
//! - Lying down in a set camp is the one place the delve is saved ([`crate::save`], "Your delve is
//!   saved."), before the sleep goes on. A resumed delve puts the delver back to sleep in the camp
//!   they saved in ([`PendingBed`]), and they wake as any sleeper does.
//!
//! Camp goods are drawn at their own size on the floor, not the tuning's larger floor scale: a
//! tent or bedroll set at 115% would no longer fit the room it was checked against.

use bevy::{ecs::system::SystemParam, prelude::*};
use fb_items::camp::Camp;
use fb_level::{CELL, Cell, FloorShape, Level};

use crate::carry::{Aimed, FloorItem, ViewModelCamera};
use crate::character::DelverNeeds;
use crate::combat::{Health, feedback::Corpse};
use crate::creatures::Critter;
use crate::folk::Denizen;
use crate::game_clock::GameClock;
use crate::hostility::Hostility;
use crate::interact::{self, Slot, Verb};
use crate::inventory::{FloorChange, FloorChanges, Hovered, Inventory, ItemId, Toast, underfoot::apply_floor_changes};
use crate::keymap::{Action, ActionInput};
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Obstacle, Obstacles, Player, PlayerCamera, ScreenOpen, inventory_open, level_has_focus};
use crate::player::Locomotion;
use crate::rules::GameRules;
use crate::ui::{BG, BRIGHT, MUTED, UiFonts};

pub struct CampPlugin;

impl Plugin for CampPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Toast>()
            .init_resource::<FloorChanges>()
            .init_resource::<Asleep>()
            .init_resource::<GameRules>()
            .add_systems(OnEnter(crate::launch::AppState::Delving), |mut asleep: ResMut<Asleep>| asleep.0 = None)
            .add_systems(Update, (lay_out_hovered, pack_hovered).run_if(inventory_open).run_if(crate::barter::not_bartering).before(apply_floor_changes).in_set(DelveSystems))
            .add_systems(Update, debug_lay_out.run_if(|| std::env::var_os("FB_LAY_OUT").is_some()).in_set(DelveSystems))
            .add_systems(Update, pack_aimed.run_if(level_has_focus).before(apply_floor_changes).in_set(DelveSystems))
            .add_systems(
                Update,
                (
                    debug_lie_down.run_if(|| std::env::var_os("FB_LIE_DOWN").is_some()),
                    go_to_sleep.run_if(level_has_focus),
                    resume_in_bed.run_if(resource_exists::<PendingBed>),
                    wake_up,
                    spawn_veil,
                    fade_veil,
                )
                    .chain()
                    .in_set(DelveSystems),
            );
    }
}

/// How far in front of the feet the near edge of a camp lies, in world units (30 cm): clear of
/// the body, within reach of the eye.
pub const GAP: f32 = 3.0;

/// How far ahead of the feet a tent's near edge lies, in world units: further than a bedroll's,
/// so the whole tent reads from where the delver stands.
pub const TENT_GAP: f32 = 6.0;

/// The gap a camp is set down at ([`GAP`] or [`TENT_GAP`]).
pub fn gap_for(camp: Camp) -> f32 {
    match camp {
        Camp::Bedroll => GAP,
        Camp::Tent => TENT_GAP,
    }
}

/// A set bedroll or tent, on its [`FloorItem`]: what it is and the floor it covers.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct SetCamp {
    pub camp: Camp,
    /// Everything of it on the floor, guy lines and pegs included.
    pub site: Site,
    /// What stops the way: a tent's canvas, which is inside `site`; the same as `site` for a
    /// bedroll, which stops nothing.
    pub canvas: Site,
}

impl SetCamp {
    /// A camp `shape` lying at `site`, whose mesh bounds are `min..max` in item space.
    pub fn new(camp: Camp, shape: &armory::Item, site: Site, min: Vec3, max: Vec3) -> SetCamp {
        let canvas = fb_items::camp::canvas_half(shape).map_or(site, |(x, z)| {
            // The canvas is centred on the item's origin, which the bounds' centre is off by a
            // little (a guy line on one side).
            let offset = Vec2::new((min.x + max.x) * 0.5, (min.z + max.z) * 0.5);
            Site { centre: site.centre - site.to_floor(offset), half: Vec2::new(x, z), yaw: site.yaw }
        });
        SetCamp { camp, site, canvas }
    }
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

    /// A vector of the site's own x and z as it lies on the floor.
    pub fn to_floor(&self, v: Vec2) -> Vec2 {
        let [x, z] = self.axes();
        x * v.x + z * v.y
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
    /// It would leave no room to walk past it, or cut the floor round it in two.
    Seals,
}

impl CampRefusal {
    pub fn reason(self) -> &'static str {
        match self {
            CampRefusal::Doorway => "It cannot be set in a doorway.",
            CampRefusal::Stair => "It cannot be set across a stair.",
            CampRefusal::Wall => "There is not room for it against the wall.",
            CampRefusal::Prop => "There is something in the way.",
            CampRefusal::Seals => "It would block the way.",
        }
    }
}

/// The site for a camp of item-space half extents `half` (along its x and z) set down by a
/// delver at `feet` (world `(x, z)`) facing `forward` (a unit floor direction): its near edge
/// `gap` ahead ([`gap_for`]), square to the delver. Item +X points back at the delver, so a
/// bedroll lies lengthways with its foot to them and a tent has its door toward them.
pub fn site_ahead(feet: Vec2, forward: Vec2, half: Vec2, gap: f32) -> Site {
    Site { centre: feet + forward * (gap + half.x), half, yaw: forward.y.atan2(-forward.x) }
}

/// How finely [`check_passage`] samples the floor, in world units.
const PASSAGE_STEP: f32 = 1.0;

/// Whether a body of `radius` can stand at `at`: clear of rock, shut cells, obstacles and, if
/// given, the `canvas` of a camp.
fn walkable(at: Vec2, radius: f32, level: &Level, blocked_cells: &BlockedCells, obstacles: &[Obstacle], canvas: Option<&Site>) -> bool {
    let cell = (at / CELL).floor().as_ivec2();
    for y in cell.y - 1..=cell.y + 1 {
        for x in cell.x - 1..=cell.x + 1 {
            if level.is_open(x, y) && !blocked_cells.0.contains(&(x, y)) {
                continue;
            }
            let (low, high) = (Vec2::new(x as f32, y as f32) * CELL, Vec2::new(x as f32 + 1.0, y as f32 + 1.0) * CELL);
            if at.distance(at.clamp(low, high)) < radius {
                return false;
            }
        }
    }
    let body = Obstacle::Circle { centre: at, radius };
    !(obstacles.iter().any(|o| o.overlaps(at, radius)) || canvas.is_some_and(|c| c.blocked_by(&body)))
}

/// How many separate stretches of floor the delver (a body of `radius`) could stand on in the
/// window of cells round `canvas`, with the canvas standing in the way or not.
fn stretches(canvas: &Site, standing: bool, radius: f32, level: &Level, blocked_cells: &BlockedCells, obstacles: &[Obstacle]) -> usize {
    let (low, high) = canvas.cells();
    let (origin, columns, rows) = (Vec2::new((low.x - 1) as f32, (low.y - 1) as f32) * CELL, ((high.x - low.x + 3) as f32 * CELL / PASSAGE_STEP) as usize, ((high.y - low.y + 3) as f32 * CELL / PASSAGE_STEP) as usize);
    let at = |c: usize, r: usize| origin + Vec2::new(c as f32 + 0.5, r as f32 + 0.5) * PASSAGE_STEP;
    let open: Vec<bool> = (0..rows).flat_map(|r| (0..columns).map(move |c| (c, r))).map(|(c, r)| walkable(at(c, r), radius, level, blocked_cells, obstacles, standing.then_some(canvas))).collect();
    let mut seen = vec![false; open.len()];
    let mut count = 0;
    for start in 0..open.len() {
        if !open[start] || seen[start] {
            continue;
        }
        count += 1;
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            let (c, r) = (i % columns, i / columns);
            for (dc, dr) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let (nc, nr) = (c as i32 + dc, r as i32 + dr);
                if nc < 0 || nr < 0 || nc >= columns as i32 || nr >= rows as i32 {
                    continue;
                }
                let j = nr as usize * columns + nc as usize;
                if open[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
    }
    count
}

/// Whether a tent with this `canvas` would leave the way open: no gap past it narrower than the
/// delver (a body of `radius`), and the floor round it no more in pieces than before. A bedroll
/// is walked over and never asks. Refused with [`CampRefusal::Seals`].
pub fn check_passage(canvas: &Site, level: &Level, blocked_cells: &BlockedCells, obstacles: &[Obstacle], radius: f32) -> Result<(), CampRefusal> {
    let before = stretches(canvas, false, radius, level, blocked_cells, obstacles);
    let after = stretches(canvas, true, radius, level, blocked_cells, obstacles);
    if after > before { Err(CampRefusal::Seals) } else { Ok(()) }
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
    body_radius: f32,
) -> Result<String, &'static str> {
    let item = inventory.item(id).ok_or("That is not here.")?;
    let set = fb_items::camp::set_up(item).ok_or("That cannot be set down.")?;
    let (half, offset) = footprint(&set).ok_or("That cannot be set down.")?;
    let camp = set.camp().ok_or("That cannot be set down.")?;
    let site = site_ahead(feet, forward, half, gap_for(camp));
    check_site(&site, level, blocked_cells, obstacles, camps).map_err(CampRefusal::reason)?;
    // The way past a tent: its canvas, not its ropes, is what stands in it.
    if camp == Camp::Tent {
        let canvas = fb_items::camp::canvas_half(&set.shape).map_or(site, |(x, z)| Site { centre: site.centre - site.to_floor(offset), half: Vec2::new(x, z), yaw: site.yaw });
        check_passage(&canvas, level, blocked_cells, obstacles, body_radius).map_err(CampRefusal::reason)?;
    }
    let line = set_down_line(&set);
    // Out of its pack and into its new form in one step, or not at all.
    if !inventory.set_form_out(id, set) {
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
    tuning: Option<Res<crate::tuning::Tuning>>,
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
    match lay_out(&mut inventory, &mut changes, id, body.translation.xz(), facing(eye), &level.0, &blocked_cells, &obstacles.0, &sites, tuning.map_or(3.5, |t| t.moving.body_radius)) {
        Ok(line) => toast.say(line, now),
        Err(reason) => toast.say(reason, now),
    }
}

/// The delver asleep, if they are. Nothing else about the delver changes: the needs step as
/// Sleeping ([`crate::simulate`]), the game clock runs fast ([`crate::game_clock`]), and the level
/// gives no focus to the world ([`level_has_focus`]) until they wake.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub struct Asleep(pub Option<Sleeping>);

/// A sleep under way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sleeping {
    /// What they sleep in, which sets how well they sleep.
    pub camp: Camp,
    /// The set camp's entity: if it goes, they wake.
    pub bed: Entity,
    /// Their hit points as last seen; any fewer wakes them.
    pub hit_points: i32,
    /// The game hour they lay down.
    pub since: f64,
    /// The real second they lay down, so the key press that began the sleep does not end it.
    pub began: f32,
}

/// Seconds after lying down before a key press wakes the delver.
const KEY_GRACE: f32 = 0.4;

/// Whether anything hostile is within range of a point: a creature that is up and about, or a
/// folk who has turned on the delver. Corpses do not count.
#[derive(SystemParam)]
pub struct Watchers<'w, 's> {
    creatures: Query<'w, 's, &'static GlobalTransform, (With<Critter>, Without<Corpse>)>,
    folk: Query<'w, 's, (&'static GlobalTransform, &'static Hostility), (With<Denizen>, Without<Corpse>)>,
}

impl Watchers<'_, '_> {
    /// Whether anything hostile is within `range` of `at` (world `(x, z)`).
    pub fn near(&self, at: Vec2, range: f32) -> bool {
        self.creatures.iter().any(|t| t.translation().xz().distance(at) <= range)
            || self.folk.iter().any(|(t, hostility)| hostility.is_hostile() && t.translation().xz().distance(at) <= range)
    }
}

/// What keeps a delver from lying down, if anything: something hostile `watched` near, hunger or
/// thirst at the Famished band (the sleep would be broken, `exhaustion.famished_at`), or too little
/// sleepiness (`camp.tired_from`).
pub fn cannot_sleep(rules: &fb_character::rules::Rules, needs: &fb_character::Needs, watched: bool) -> Option<&'static str> {
    if watched {
        return Some("You cannot sleep with something so near.");
    }
    if needs.hunger.value >= rules.exhaustion.famished_at {
        return Some("You are too hungry to sleep.");
    }
    if needs.thirst.value >= rules.exhaustion.famished_at {
        return Some("You are too thirsty to sleep.");
    }
    if needs.sleepiness.value < rules.camp.tired_from {
        return Some("You are not tired enough to sleep.");
    }
    None
}

/// Why a delver asleep should wake now, if they should: hurt (fewer hit points than they lay down
/// with), something hostile `watched` near, hunger or thirst at the Famished band, or rested. A
/// key press is not here: it is the input's ([`wake_up`]).
pub fn reason_to_wake(rules: &fb_character::rules::Rules, needs: &fb_character::Needs, hit_points: i32, began_with: i32, watched: bool) -> Option<&'static str> {
    if hit_points < began_with {
        return Some("You wake in pain.");
    }
    if watched {
        return Some("Something stirs nearby, and you wake.");
    }
    if needs.hunger.value >= rules.exhaustion.famished_at {
        return Some("Hunger wakes you.");
    }
    if needs.thirst.value >= rules.exhaustion.famished_at {
        return Some("Thirst wakes you.");
    }
    if needs.sleepiness.value <= 0.0 {
        return Some("You wake rested.");
    }
    None
}

/// "7 hours": the game time from hour `from` to hour `now` in whole hours, at least one.
fn hours_slept(from: f64, now: f64) -> String {
    let hours = (now - from).round().max(1.0) as u32;
    format!("{hours} hour{}", if hours == 1 { "" } else { "s" })
}

/// What a camp is called in a line.
fn camp_name(camp: Camp) -> &'static str {
    match camp {
        Camp::Bedroll => "bedroll",
        Camp::Tent => "tent",
    }
}

/// F on a set camp in reach: lies down in it ([`cannot_sleep`] says if they cannot). A torch or
/// lantern they carry is put out first, since a night would burn it down.
#[allow(clippy::too_many_arguments)]
fn go_to_sleep(
    input: ActionInput,
    aimed: Res<Aimed>,
    camps: Query<&SetCamp>,
    rules: Res<GameRules>,
    clock: Res<GameClock>,
    watchers: Watchers,
    mut asleep: ResMut<Asleep>,
    mut inventory: ResMut<Inventory>,
    mut toast: ResMut<Toast>,
    mut players: Query<(&Transform, &DelverNeeds, &mut Locomotion, Option<&Health>), With<Player>>,
    time: Option<Res<Time<Real>>>,
    mut commands: Commands,
) {
    if asleep.0.is_some() || !input.just_pressed(Action::Primary) {
        return;
    }
    let Some(bed) = aimed.0 else { return };
    let Ok(camp) = camps.get(bed) else { return };
    let Ok((body, needs, mut locomotion, health)) = players.single_mut() else { return };
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    let watched = watchers.near(body.translation.xz(), rules.camp.wake_distance as f32);
    if let Some(reason) = cannot_sleep(&rules, &needs.0, watched) {
        toast.say(reason, now);
        return;
    }
    // Any light they carry is put out; the inventory is only touched if one burns.
    let lit: Vec<ItemId> = crate::carry::carried_lights(&inventory).into_iter().map(|(_, id)| id).collect();
    for id in lit {
        if inventory.item(id).is_some_and(|item| item.shape.lit() == Some(true)) {
            let _ = inventory.toggle_light(id);
        }
    }
    locomotion.velocity = Vec2::ZERO;
    asleep.0 = Some(Sleeping { camp: camp.camp, bed, hit_points: health.map_or(0, |h| h.hp), since: clock.time.hours(), began: now });
    toast.say(format!("You lie down in the {}", camp_name(camp.camp)), now);
    // Lying down in a camp is the one place the delve is saved (`crate::save`), before the sleep
    // goes on; it says so, or that it could not, over the line above.
    commands.queue(crate::save::save_in_camp(bed));
}

/// Where the delver of a delve resumed lies down, until the level has laid its camp out: the spot
/// of the set camp they saved in.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct PendingBed {
    pub spot: fb_level::ItemSpot,
    /// Frames spent looking for the camp.
    pub waited: u32,
}

/// Frames a resumed delve waits for its camp to be laid out before it gives the bed up.
const BED_PATIENCE: u32 = 600;

/// Puts the delver of a resumed delve back to sleep in the camp they saved in, once the level has
/// laid it out: the sleep goes on as it was when they lay down, and ends as any sleep does.
#[allow(clippy::too_many_arguments)]
fn resume_in_bed(
    mut commands: Commands,
    pending: Option<ResMut<PendingBed>>,
    beds: Query<(Entity, &FloorItem, &SetCamp)>,
    clock: Res<GameClock>,
    mut asleep: ResMut<Asleep>,
    mut toast: ResMut<Toast>,
    mut players: Query<(&mut Locomotion, Option<&Health>), With<Player>>,
    time: Option<Res<Time<Real>>>,
) {
    let Some(mut pending) = pending else { return };
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    let Some((bed, _, camp)) = beds.iter().find(|(_, floor, _)| floor.spot == pending.spot) else {
        pending.waited += 1;
        if pending.waited >= BED_PATIENCE {
            commands.remove_resource::<PendingBed>();
            toast.say("Your camp is gone.", now);
        }
        return;
    };
    commands.remove_resource::<PendingBed>();
    let Ok((mut locomotion, health)) = players.single_mut() else { return };
    locomotion.velocity = Vec2::ZERO;
    asleep.0 = Some(Sleeping { camp: camp.camp, bed, hit_points: health.map_or(0, |h| h.hp), since: clock.time.hours(), began: now });
}

/// For captures (`FB_LIE_DOWN`): a while after a camp is laid out (`FB_LAY_OUT`), tires the delver
/// and presses F on it, so the sleep begins as it does for a player, and saves the delve if the
/// delve may (`FB_SAVE_FILE`).
fn debug_lie_down(
    mut frames: Local<u32>,
    mut aimed: ResMut<Aimed>,
    mut keys: Option<ResMut<ButtonInput<KeyCode>>>,
    keymap: Res<crate::keymap::Keymap>,
    camps: Query<Entity, With<SetCamp>>,
    mut players: Query<&mut DelverNeeds, With<Player>>,
) {
    *frames += 1;
    if *frames < 20 || *frames > 24 {
        return;
    }
    let (Some(bed), Some(keys)) = (camps.iter().next(), keys.as_mut()) else { return };
    aimed.0 = Some(bed);
    for mut needs in &mut players {
        needs.0.sleepiness.value = needs.0.sleepiness.value.max(0.8);
    }
    for input in keymap.inputs(Action::Primary) {
        if let crate::keymap::Input::Key(key) = *input {
            keys.press(key);
        }
    }
}

/// Ends the sleep: rested, hurt (any fall in their hit points since they were last looked at,
/// so healing in their sleep hides no later blow), hungry, thirsty, a hostile near, any key or
/// mouse button pressed, a screen opened, or the camp gone from under them.
#[allow(clippy::too_many_arguments)]
fn wake_up(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    screen: Res<ScreenOpen>,
    rules: Res<GameRules>,
    clock: Res<GameClock>,
    watchers: Watchers,
    beds: Query<(), With<SetCamp>>,
    mut asleep: ResMut<Asleep>,
    mut toast: ResMut<Toast>,
    players: Query<(&Transform, &DelverNeeds, Option<&Health>), With<Player>>,
    time: Option<Res<Time<Real>>>,
) {
    let Some(sleep) = asleep.0 else { return };
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    let Ok((body, needs, health)) = players.single() else {
        asleep.0 = None;
        return;
    };
    let watched = watchers.near(body.translation.xz(), rules.camp.wake_distance as f32);
    let pressed = keys.is_some_and(|k| k.get_just_pressed().next().is_some()) || mouse.is_some_and(|m| m.get_just_pressed().next().is_some());
    let keyed = now - sleep.began > KEY_GRACE && (screen.0.is_some() || pressed);
    let hit_points = health.map_or(sleep.hit_points, |h| h.hp);
    let reason = reason_to_wake(&rules, &needs.0, hit_points, sleep.hit_points, watched)
        .or_else(|| (!beds.contains(sleep.bed)).then_some("Your bed is gone, and you wake."))
        .or_else(|| keyed.then_some("You wake."));
    let Some(reason) = reason else {
        // Healed in their sleep: the next blow is measured from here.
        if hit_points != sleep.hit_points {
            asleep.0 = Some(Sleeping { hit_points, ..sleep });
        }
        return;
    };
    asleep.0 = None;
    toast.say(format!("{reason} You slept {}.", hours_slept(sleep.since, clock.time.hours())), now);
}

/// The dark that falls over the view while the delver sleeps.
#[derive(Component, Debug, Default)]
pub struct SleepVeil {
    /// How dark it is now, 0 to 1.
    pub dark: f32,
}

/// The veil's heading (`true`) and the line beneath it.
#[derive(Component, Debug)]
struct VeilWords(bool);

/// How quickly the veil falls and lifts, per real second.
const VEIL_RATE: f32 = 1.2;
/// The veil's most opaque, so the level shows a little through it.
const VEIL_DARK: f32 = 0.96;

/// One veil over each new view, clear until the delver sleeps.
fn spawn_veil(mut commands: Commands, fonts: Option<Res<UiFonts>>, cameras: Query<Entity, Added<ViewModelCamera>>) {
    for camera in &cameras {
        let body = fonts.as_deref().map(|f| f.body.clone()).unwrap_or_default();
        commands
            .spawn((
                Name::new("Sleep veil"),
                SleepVeil::default(),
                DespawnOnExit(crate::launch::AppState::Delving),
                UiTargetCamera(camera),
                Node {
                    width: percent(100),
                    height: percent(100),
                    position_type: PositionType::Absolute,
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    row_gap: px(10),
                    ..default()
                },
                BackgroundColor(BG.with_alpha(0.0)),
                GlobalZIndex(30),
                Pickable::IGNORE,
            ))
            .with_children(|veil| {
                veil.spawn((VeilWords(true), Text::new(""), TextFont::from_font_size(30.0).with_font(body.clone()), TextColor(BRIGHT.with_alpha(0.0)), TextLayout::justify(Justify::Center)));
                veil.spawn((VeilWords(false), Text::new("Any move or key wakes you"), TextFont::from_font_size(18.0).with_font(body), TextColor(MUTED.with_alpha(0.0)), TextLayout::justify(Justify::Center)));
            });
    }
}

/// The veil's darkness after `seconds` of heading from `dark` toward `target` (0 or 1).
fn veil_step(dark: f32, target: f32, seconds: f32) -> f32 {
    let step = VEIL_RATE * seconds;
    if dark < target { (dark + step).min(target) } else { (dark - step).max(target) }
}

/// Darkens the veil while asleep and clears it after, with its words.
fn fade_veil(
    time: Res<Time<Real>>,
    asleep: Res<Asleep>,
    clock: Res<GameClock>,
    mut veils: Query<(&mut SleepVeil, &mut BackgroundColor, &Children)>,
    mut words: Query<(&VeilWords, &mut Text, &mut TextColor)>,
) {
    let target = if asleep.0.is_some() { 1.0 } else { 0.0 };
    for (mut veil, mut background, children) in &mut veils {
        let dark = veil_step(veil.dark, target, time.delta_secs());
        if dark != veil.dark {
            veil.dark = dark;
        }
        background.0 = BG.with_alpha(dark * VEIL_DARK);
        for &child in children {
            let Ok((role, mut text, mut colour)) = words.get_mut(child) else { continue };
            if role.0
                && let Some(sleep) = asleep.0
            {
                let line = format!("Sleeping in the {}: {}", camp_name(sleep.camp), hours_slept(sleep.since, clock.time.hours()));
                if text.0 != line {
                    text.0 = line;
                }
            }
            colour.0 = if role.0 { BRIGHT } else { MUTED }.with_alpha(dark);
        }
    }
}

/// What the toast says when a set camp is packed up.
const PACK_FULL: &str = "There is no room in your pack for it.";

/// Packs the set camp `id`, lying in the level, back into the first bag of the pack with room: the
/// same item in its carried form ([`fb_items::camp::pack_up`]), its wear and history kept. It is
/// taken up off the floor through `changes`. Returns what the toast says, or why not; a refused
/// camp stays as it was, set down.
pub fn pack(inventory: &mut Inventory, changes: &mut FloorChanges, id: ItemId) -> Result<String, &'static str> {
    let item = inventory.item(id).ok_or("That is not here.")?;
    let before = item.clone();
    let packed = fb_items::camp::pack_up(&before).ok_or("That cannot be packed.")?;
    let line = format!("You pack up the {}", packed.title().to_lowercase());
    if !inventory.replace_form(id, packed) {
        return Err("That cannot be packed.");
    }
    if inventory.stow(id).is_err() {
        // Nowhere to put it: it is still lying loose, and goes back as it was.
        inventory.replace_form(id, before);
        return Err(PACK_FULL);
    }
    changes.0.push(FloorChange::Lifted(id));
    Ok(line)
}

/// G on a set camp in the level within reach: packs it up ([`pack`]), or says why not.
fn pack_aimed(
    input: ActionInput,
    aimed: Res<Aimed>,
    camps: Query<&crate::carry::FloorItem, With<SetCamp>>,
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    mut toast: ResMut<Toast>,
    time: Option<Res<Time<Real>>>,
) {
    if !input.just_pressed(Action::Secondary) {
        return;
    }
    let Some(floor) = aimed.0.and_then(|entity| camps.get(entity).ok()) else { return };
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    match pack(&mut inventory, &mut changes, floor.id) {
        Ok(line) => toast.say(line, now),
        Err(reason) => toast.say(reason, now),
    }
}

/// G in the inventory on a set camp lying underfoot: packs it up ([`pack`]).
fn pack_hovered(
    input: ActionInput,
    hovered: Res<Hovered>,
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    mut toast: ResMut<Toast>,
    time: Option<Res<Time<Real>>>,
) {
    if !input.just_pressed(Action::Secondary) {
        return;
    }
    let Some((id, _)) = hovered.0 else { return };
    let Some(target) = interact::item_target(&inventory, id) else { return };
    if interact::verbs_for(&target, &interact::Delver::default()).get(Slot::Secondary) != Some(Verb::PackUp) {
        return;
    }
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    match pack(&mut inventory, &mut changes, id) {
        Ok(line) => toast.say(line, now),
        Err(reason) => toast.say(reason, now),
    }
}

/// For captures (`FB_LAY_OUT=<base>`, `Tent` or `Bedroll`): a few frames in, sets a fresh one of
/// that base down in front of the delver as F would, with the same checks, and says so or why not
/// in the toast.
#[allow(clippy::too_many_arguments)]
fn debug_lay_out(
    mut frames: Local<u32>,
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    mut toast: ResMut<Toast>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    players: Query<&Transform, With<Player>>,
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Res<Obstacles>,
    time: Option<Res<Time<Real>>>,
) {
    *frames += 1;
    if *frames != 6 {
        return;
    }
    let (Some(eye), Some(body)) = (eyes.iter().next(), players.iter().next()) else { return };
    let Some(item) = std::env::var("FB_LAY_OUT").ok().and_then(|base| fb_items::roll_item(base.trim(), 1)) else { return };
    let id = inventory.add(item);
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    let said = lay_out(&mut inventory, &mut changes, id, body.translation.xz(), facing(eye), &level.0, &blocked_cells, &obstacles.0, &[], 3.5);
    toast.say(said.unwrap_or_else(|why| why.to_string()), now);
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
            let site = site_ahead(feet, forward, Vec2::new(8.5, 3.0), GAP);
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
        let site = site_ahead(at_cell(3.5, 3.5), Vec2::NEG_Y, half, GAP);
        assert_eq!(clear(&site, &level), Ok(()));
        // Facing the wall from beside it: the camp would lie in the rock.
        let against = site_ahead(at_cell(3.5, 1.2), Vec2::NEG_Y, half, GAP);
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
        let site = site_ahead(at_cell(3.5, 3.5), Vec2::NEG_Y, half, GAP);
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
        let line = lay_out(&mut inventory, &mut changes, bedroll, feet, Vec2::NEG_Y, &level, &BlockedCells::default(), &[], &[], 3.5).unwrap();
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
        let reason = lay_out(&mut inventory, &mut changes, tent, at_cell(3.5, 1.3), Vec2::NEG_Y, &level, &BlockedCells::default(), &[], &[], 3.5).unwrap_err();
        assert_eq!(reason, CampRefusal::Wall.reason());
        assert!(changes.0.is_empty());
        assert_eq!(inventory.item(tent), Some(&before));
        assert!(inventory.is_carried(tent), "still in the pack");
    }

    /// An open row of floor `width` cells tall through the middle of the `room()` rock.
    fn corridor(cells_wide: u32) -> Level {
        let mut level = room();
        level.cells = (0..64).map(|i| if i / 8 >= 1 && i / 8 < 1 + cells_wide && i % 8 != 0 && i % 8 != 7 { Cell::Floor } else { Cell::Rock }).collect();
        level
    }

    #[test]
    fn a_tent_that_would_seal_a_corridor_is_refused_and_one_in_a_room_is_not() {
        let tent = |centre: Vec2, half: Vec2| Site { centre, half, yaw: 0.0 };
        let none = BlockedCells::default();
        // One cell is 20 across: a canvas 14 wide leaves 3 each side, less than a body of 7.
        let narrow = corridor(1);
        let across = tent(at_cell(3.5, 1.5), Vec2::new(9.0, 7.0));
        assert_eq!(check_passage(&across, &narrow, &none, &[], 3.5), Err(CampRefusal::Seals));
        // A small canvas that leaves a body's width either side does not.
        let small = tent(at_cell(3.5, 1.5), Vec2::new(5.0, 1.5));
        assert_eq!(check_passage(&small, &narrow, &none, &[], 3.5), Ok(()));
        // The same tent in a room, with the way round it, is fine.
        let wide = room();
        assert_eq!(check_passage(&tent(at_cell(3.5, 3.5), Vec2::new(9.0, 7.0)), &wide, &none, &[], 3.5), Ok(()));
        // Two cells wide, a canvas nearly as wide leaves less than a body past it.
        let two = corridor(2);
        assert_eq!(check_passage(&tent(at_cell(3.5, 2.0), Vec2::new(9.0, 18.5)), &two, &none, &[], 3.5), Err(CampRefusal::Seals));
    }

    #[test]
    fn a_tent_that_would_cut_off_the_far_end_of_a_corridor_is_refused() {
        // A dead end three cells long: a canvas across it cuts the floor beyond from the floor before.
        let level = corridor(1);
        let across = Site { centre: at_cell(4.0, 1.5), half: Vec2::new(4.0, 7.0), yaw: 0.0 };
        assert_eq!(check_passage(&across, &level, &BlockedCells::default(), &[], 3.5), Err(CampRefusal::Seals));
    }

    #[test]
    fn lay_out_refuses_a_tent_across_a_corridor_through_the_whole_path() {
        let level = corridor(1);
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 1).unwrap());
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        // A ridge tent, whose canvas is 10 or more across the eaves, lying across 20 of floor.
        let tent = (1..200).filter_map(|s| fb_items::roll_item("Tent", s)).find(|t| fb_items::camp::set_up(t).and_then(|p| fb_items::camp::canvas_half(&p.shape)).is_some_and(|(x, z)| z > 5.5 && x < 9.5)).expect("a ridge tent");
        let id = inventory.add(tent);
        inventory.stow(id).unwrap();
        let mut changes = FloorChanges::default();
        let said = lay_out(&mut inventory, &mut changes, id, at_cell(1.5, 1.5), Vec2::X, &level, &BlockedCells::default(), &[], &[], 3.5);
        assert_eq!(said, Err(CampRefusal::Seals.reason()), "stays in the pack");
        assert!(inventory.is_carried(id) && changes.0.is_empty());
    }

    #[test]
    fn the_way_is_blocked_by_the_canvas_not_the_ropes_and_a_tent_stands_further_off() {
        for seed in 1..12 {
            let pitched = fb_items::camp::set_up(&fb_items::roll_item("Tent", seed).unwrap()).unwrap();
            let (lo, hi) = pitched.shape.build().bounds().unwrap();
            let site = Site { centre: Vec2::ZERO, half: Vec2::new(hi[0] - lo[0], hi[2] - lo[2]) * 0.5, yaw: 0.0 };
            let set = SetCamp::new(Camp::Tent, &pitched.shape, site, Vec3::from_array(lo), Vec3::from_array(hi));
            assert!(set.canvas.half.x <= site.half.x + 1e-3 && set.canvas.half.y <= site.half.y + 1e-3, "{seed}");
            let Some(FloorShape::Square { half_width, half_depth }) = blocks_the_way(Camp::Tent, &set.canvas) else { panic!("a tent blocks") };
            assert_eq!((half_width, half_depth), (set.canvas.half.x, set.canvas.half.y));
        }
        assert!(gap_for(Camp::Tent) > gap_for(Camp::Bedroll));
        let (feet, forward, half) = (Vec2::ZERO, Vec2::X, Vec2::new(9.0, 7.0));
        assert!((site_ahead(feet, forward, half, gap_for(Camp::Tent)).centre.x - (TENT_GAP + 9.0)).abs() < 1e-4);
    }

    #[test]
    fn a_form_change_that_cannot_happen_leaves_the_item_in_its_pack() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 1).unwrap());
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let id = inventory.add(fb_items::roll_item("Bedroll", 3).unwrap());
        inventory.stow(id).unwrap();
        let other = fb_items::camp::set_up(&fb_items::roll_item("Bedroll", 4).unwrap()).unwrap();
        assert!(!inventory.set_form_out(id, other), "another bedroll is not this one");
        assert!(inventory.is_carried(id), "still in the pack");
        let mine = fb_items::camp::set_up(inventory.item(id).unwrap()).unwrap();
        assert!(inventory.set_form_out(id, mine));
        assert_eq!(inventory.location(id), Some(crate::inventory::Location::Loose));
    }

    #[test]
    fn a_tent_blocks_the_way_and_a_bedroll_is_walked_over() {
        let site = Site { centre: Vec2::ZERO, half: Vec2::new(9.0, 7.0), yaw: 0.0 };
        assert_eq!(blocks_the_way(Camp::Tent, &site), Some(FloorShape::Square { half_width: 9.0, half_depth: 7.0 }));
        assert_eq!(blocks_the_way(Camp::Bedroll, &site), None);
    }

    #[test]
    fn a_set_camp_is_slept_in_not_taken_or_equipped() {
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
        assert_eq!(verbs(&inventory), interact::Verbs { primary: Some(Verb::Sleep), secondary: Some(Verb::PackUp), ..interact::Verbs::default() });
    }

    fn rules() -> fb_character::rules::Rules {
        fb_character::rules::Rules::default()
    }

    fn needs(hunger: f64, thirst: f64, sleepiness: f64) -> fb_character::Needs {
        let mut needs = fb_character::Needs::default();
        needs.hunger.value = hunger;
        needs.thirst.value = thirst;
        needs.sleepiness.value = sleepiness;
        needs
    }

    #[test]
    fn a_delver_lies_down_only_when_tired_fed_watered_and_alone() {
        let rules = rules();
        assert_eq!(cannot_sleep(&rules, &needs(0.1, 0.1, 0.5), false), None);
        assert_eq!(cannot_sleep(&rules, &needs(0.1, 0.1, 0.1), false), Some("You are not tired enough to sleep."));
        assert_eq!(cannot_sleep(&rules, &needs(0.8, 0.1, 0.9), false), Some("You are too hungry to sleep."));
        assert_eq!(cannot_sleep(&rules, &needs(0.1, 0.8, 0.9), false), Some("You are too thirsty to sleep."));
        assert_eq!(cannot_sleep(&rules, &needs(0.1, 0.1, 0.9), true), Some("You cannot sleep with something so near."));
    }

    #[test]
    fn a_sleeper_wakes_rested_hurt_hungry_thirsty_or_disturbed() {
        let rules = rules();
        let wakes = |n: fb_character::Needs, hp: i32, watched: bool| reason_to_wake(&rules, &n, hp, 10, watched);
        assert_eq!(wakes(needs(0.1, 0.1, 0.4), 10, false), None, "still tired, nothing amiss");
        assert_eq!(wakes(needs(0.1, 0.1, 0.0), 10, false), Some("You wake rested."));
        assert_eq!(wakes(needs(0.1, 0.1, 0.4), 9, false), Some("You wake in pain."));
        assert_eq!(wakes(needs(0.1, 0.1, 0.4), 10, true), Some("Something stirs nearby, and you wake."));
        assert_eq!(wakes(needs(0.76, 0.1, 0.4), 10, false), Some("Hunger wakes you."));
        assert_eq!(wakes(needs(0.1, 0.76, 0.4), 10, false), Some("Thirst wakes you."));
        // Hurt wins over rested: the pain is what they wake to.
        assert_eq!(wakes(needs(0.1, 0.1, 0.0), 9, false), Some("You wake in pain."));
    }

    /// An app with the sleep systems, a tired delver and a set camp aimed at.
    fn sleeper(camp_base: &str) -> (App, Entity, Entity) {
        use crate::keymap::Keymap;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Keymap>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Aimed>()
            .init_resource::<Inventory>()
            .init_resource::<Toast>()
            .init_resource::<FloorChanges>()
            .init_resource::<ScreenOpen>()
            .init_resource::<GameClock>()
            .init_resource::<GameRules>()
            .init_resource::<Asleep>()
            .add_systems(Update, (go_to_sleep, wake_up).chain());
        let item = fb_items::camp::set_up(&fb_items::roll_item(camp_base, 5).unwrap()).unwrap();
        let camp = item.camp().unwrap();
        let bed = app.world_mut().spawn(SetCamp { camp, site: Site { centre: Vec2::ZERO, half: Vec2::new(8.0, 3.0), yaw: 0.0 }, canvas: Site { centre: Vec2::ZERO, half: Vec2::new(8.0, 3.0), yaw: 0.0 } }).id();
        app.world_mut().resource_mut::<Aimed>().0 = Some(bed);
        let mut tired = DelverNeeds::default();
        tired.0.sleepiness.value = 0.8;
        let delver = app.world_mut().spawn((Player, Transform::default(), tired, Locomotion { velocity: Vec2::new(4.0, 0.0), ..default() }, Health::full(10))).id();
        (app, delver, bed)
    }

    fn press(app: &mut App, key: KeyCode) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.clear();
        keys.press(key);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(key);
    }

    #[test]
    fn f_on_a_set_tent_lies_the_delver_down_and_still() {
        let (mut app, delver, bed) = sleeper("Tent");
        press(&mut app, KeyCode::KeyF);
        let sleep = app.world().resource::<Asleep>().0.expect("asleep");
        assert_eq!((sleep.camp, sleep.bed, sleep.hit_points), (Camp::Tent, bed, 10));
        assert_eq!(app.world().get::<Locomotion>(delver).unwrap().velocity, Vec2::ZERO);
        assert!(app.world().resource::<Toast>().text.starts_with("You lie down in the tent"));
    }

    #[test]
    fn a_rested_delver_cannot_lie_down_and_is_told_why() {
        let (mut app, delver, _) = sleeper("Bedroll");
        app.world_mut().get_mut::<DelverNeeds>(delver).unwrap().0.sleepiness.value = 0.05;
        press(&mut app, KeyCode::KeyF);
        assert_eq!(app.world().resource::<Asleep>().0, None);
        assert_eq!(app.world().resource::<Toast>().text, "You are not tired enough to sleep.");
    }

    #[test]
    fn a_light_they_carry_is_put_out_before_they_sleep() {
        let (mut app, _, _) = sleeper("Bedroll");
        let torch = app.world_mut().resource_mut::<Inventory>().add(crate::carry::lit_torch(7));
        app.world_mut().resource_mut::<Inventory>().equip(torch, fb_items::Slot::Offhand).unwrap();
        assert_eq!(app.world().resource::<Inventory>().item(torch).unwrap().shape.lit(), Some(true));
        press(&mut app, KeyCode::KeyF);
        assert!(app.world().resource::<Asleep>().0.is_some());
        assert_eq!(app.world().resource::<Inventory>().item(torch).unwrap().shape.lit(), Some(false), "put out");
    }

    #[test]
    fn the_press_that_began_the_sleep_does_not_end_it_but_a_later_one_does() {
        let (mut app, _, _) = sleeper("Bedroll");
        press(&mut app, KeyCode::KeyF);
        app.update();
        assert!(app.world().resource::<Asleep>().0.is_some(), "still asleep a frame on");
        // A key a while later (real time) wakes them.
        std::thread::sleep(std::time::Duration::from_millis(500));
        press(&mut app, KeyCode::KeyW);
        assert_eq!(app.world().resource::<Asleep>().0, None);
        assert!(app.world().resource::<Toast>().text.starts_with("You wake."), "{}", app.world().resource::<Toast>().text);
    }

    #[test]
    fn healing_in_their_sleep_hides_no_later_blow() {
        let (mut app, delver, _) = sleeper("Bedroll");
        app.world_mut().get_mut::<Health>(delver).unwrap().hp = 5;
        press(&mut app, KeyCode::KeyF);
        assert_eq!(app.world().resource::<Asleep>().0.map(|s| s.hit_points), Some(5), "lay down hurt");
        // Mended to 9 while they sleep: still asleep, the next blow measured from 9.
        app.world_mut().get_mut::<Health>(delver).unwrap().hp = 9;
        app.update();
        let sleep = app.world().resource::<Asleep>().0.expect("still asleep");
        assert_eq!(sleep.hit_points, 9);
        // A hit that leaves them above where they lay down still wakes them.
        app.world_mut().get_mut::<Health>(delver).unwrap().hp = 7;
        app.update();
        assert_eq!(app.world().resource::<Asleep>().0, None);
        assert!(app.world().resource::<Toast>().text.starts_with("You wake in pain."));
    }

    #[test]
    fn any_key_wakes_them_not_only_the_movement_keys() {
        for key in [KeyCode::ShiftLeft, KeyCode::Digit3, KeyCode::KeyL, KeyCode::KeyV, KeyCode::KeyX] {
            let (mut app, _, _) = sleeper("Bedroll");
            press(&mut app, KeyCode::KeyF);
            std::thread::sleep(std::time::Duration::from_millis(450));
            press(&mut app, key);
            assert_eq!(app.world().resource::<Asleep>().0, None, "{key:?}");
        }
    }

    #[test]
    fn rested_hurt_or_hungry_wakes_them_at_once() {
        for (what, wake) in [
            ("rested", Box::new(|app: &mut App, d: Entity| app.world_mut().get_mut::<DelverNeeds>(d).unwrap().0.sleepiness.value = 0.0) as Box<dyn Fn(&mut App, Entity)>),
            ("hurt", Box::new(|app: &mut App, d: Entity| app.world_mut().get_mut::<Health>(d).unwrap().hp = 6)),
            ("hungry", Box::new(|app: &mut App, d: Entity| app.world_mut().get_mut::<DelverNeeds>(d).unwrap().0.hunger.value = 0.9)),
        ] {
            let (mut app, delver, _) = sleeper("Tent");
            press(&mut app, KeyCode::KeyF);
            assert!(app.world().resource::<Asleep>().0.is_some(), "{what}");
            wake(&mut app, delver);
            app.update();
            assert_eq!(app.world().resource::<Asleep>().0, None, "{what}: woken");
            assert!(app.world().resource::<Toast>().text.contains("You slept"), "{what}: {}", app.world().resource::<Toast>().text);
        }
    }

    #[test]
    fn a_creature_that_comes_near_wakes_them_and_one_far_off_does_not() {
        let (mut app, _, _) = sleeper("Bedroll");
        press(&mut app, KeyCode::KeyF);
        let spot = *fb_level::generate(1207, 1).creatures.first().expect("a creature");
        let critter = crate::creatures::Critter::new(spot, spot.creature());
        let far = Transform::from_xyz(300.0, 0.0, 0.0);
        let rat = app.world_mut().spawn((critter, far, GlobalTransform::from(far))).id();
        app.update();
        assert!(app.world().resource::<Asleep>().0.is_some(), "far off, they sleep on");
        let near = Transform::from_xyz(40.0, 0.0, 0.0);
        app.world_mut().entity_mut(rat).insert((near, GlobalTransform::from(near)));
        app.update();
        assert_eq!(app.world().resource::<Asleep>().0, None);
        assert!(app.world().resource::<Toast>().text.starts_with("Something stirs nearby"), "{}", app.world().resource::<Toast>().text);
        // And they will not lie down with it there.
        press(&mut app, KeyCode::KeyF);
        assert_eq!(app.world().resource::<Asleep>().0, None);
        assert_eq!(app.world().resource::<Toast>().text, "You cannot sleep with something so near.");
    }

    #[test]
    fn the_camp_going_wakes_the_sleeper() {
        let (mut app, _, bed) = sleeper("Bedroll");
        press(&mut app, KeyCode::KeyF);
        app.world_mut().despawn(bed);
        app.update();
        assert_eq!(app.world().resource::<Asleep>().0, None);
    }

    #[test]
    fn the_veil_darkens_while_asleep_and_clears_after() {
        let mut dark = 0.0;
        for _ in 0..4 {
            dark = veil_step(dark, 1.0, 0.25);
        }
        assert!((dark - 1.0).abs() < 1e-6, "dark inside a second: {dark}");
        assert_eq!(veil_step(dark, 1.0, 0.25), 1.0, "no darker than dark");
        for _ in 0..4 {
            dark = veil_step(dark, 0.0, 0.25);
        }
        assert_eq!(dark, 0.0, "clear again");
    }

    /// A pack on the delver's back and `base` set down on the floor as a loose item, from `seed`:
    /// the inventory, the pack, the set item and the carried form it came from.
    fn lying(base: &str, seed: u32) -> (Inventory, ItemId, ItemId, fb_items::Item) {
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 1).unwrap());
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let carried = fb_items::roll_item(base, seed).unwrap();
        let id = inventory.add(carried.clone());
        let set = fb_items::camp::set_up(&carried).unwrap();
        assert!(inventory.replace_form(id, set));
        (inventory, pack, id, carried)
    }

    #[test]
    fn a_set_camp_packs_up_into_the_pack_as_the_same_item() {
        for (base, seed) in [("Bedroll", 3), ("Tent", 4), ("Tent", 9)] {
            let (mut inventory, bag, id, carried) = lying(base, seed);
            let mut changes = FloorChanges::default();
            let line = pack(&mut inventory, &mut changes, id).unwrap();
            assert!(line.starts_with("You pack up the"), "{line}");
            assert_eq!(inventory.item(id), Some(&carried), "{base} {seed}: the item it was, carried form and all");
            assert!(inventory.is_carried(id) && inventory.holds(bag, id), "{base} {seed}: in the pack");
            assert_eq!(changes.0, vec![FloorChange::Lifted(id)], "{base} {seed}: lifted off the floor");
        }
    }

    #[test]
    fn wear_and_history_are_kept_through_setting_down_and_packing_up() {
        let mut worn = fb_items::roll_item("Tent", 5).unwrap();
        worn.history.push("Made by Brenna Holt".into());
        let armory::Item::Tent(s) = &mut worn.shape else { panic!("tent") };
        s.cond.wear = 0.7;
        s.cond.rot = 0.4;
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 1).unwrap());
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let id = inventory.add(worn.clone());
        inventory.stow(id).unwrap();
        let (mut changes, level) = (FloorChanges::default(), room());
        lay_out(&mut inventory, &mut changes, id, at_cell(3.5, 3.5), Vec2::NEG_Y, &level, &BlockedCells::default(), &[], &[], 3.5).unwrap();
        assert!(inventory.item(id).unwrap().is_set_camp());
        super::pack(&mut inventory, &mut changes, id).unwrap();
        assert_eq!(inventory.item(id), Some(&worn), "wear, rot and the maker's line as they were");
    }

    #[test]
    fn a_full_pack_refuses_and_the_camp_stays_set() {
        let (mut inventory, pack, id, _) = lying("Tent", 4);
        // Fill the pack until nothing more fits.
        for n in 0..200 {
            let rope = inventory.add(fb_items::roll_item("Rope", 40 + n).unwrap());
            if inventory.stow(rope).is_err() {
                inventory.forget(rope);
                break;
            }
        }
        let set = inventory.item(id).unwrap().clone();
        let mut changes = FloorChanges::default();
        assert_eq!(super::pack(&mut inventory, &mut changes, id), Err(PACK_FULL));
        assert_eq!(inventory.item(id), Some(&set), "still the pitched tent");
        assert_eq!(inventory.location(id), Some(crate::inventory::Location::Loose));
        assert!(!inventory.holds(pack, id) && changes.0.is_empty(), "nothing taken up");
    }

    #[test]
    fn only_a_set_camp_is_packed() {
        let mut inventory = Inventory::default();
        let sword = inventory.add(fb_items::roll_item("Sword", 2).unwrap());
        let mut changes = FloorChanges::default();
        assert_eq!(super::pack(&mut inventory, &mut changes, sword), Err("That cannot be packed."));
        let bedroll = inventory.add(fb_items::roll_item("Bedroll", 2).unwrap());
        assert_eq!(super::pack(&mut inventory, &mut changes, bedroll), Err("That cannot be packed."), "not set down");
        assert!(changes.0.is_empty());
    }

    #[test]
    fn g_on_a_set_camp_in_reach_packs_it_up() {
        let (mut app, _, bed) = sleeper("Bedroll");
        let (inventory, pack, id, carried) = lying("Bedroll", 5);
        *app.world_mut().resource_mut::<Inventory>() = inventory;
        let spot = fb_level::ItemSpot { x: 0.0, y: 0.0, yaw: 0.0, family: armory::Family::Sundry, seed: 5 };
        app.world_mut().entity_mut(bed).insert(crate::carry::FloorItem { id, spot, min: Vec3::ZERO, max: Vec3::ONE });
        app.add_systems(Update, pack_aimed);
        press(&mut app, KeyCode::KeyG);
        let inventory = app.world().resource::<Inventory>();
        assert_eq!(inventory.item(id), Some(&carried));
        assert!(inventory.holds(pack, id));
        assert_eq!(app.world().resource::<FloorChanges>().0, vec![FloorChange::Lifted(id)]);
        assert!(app.world().resource::<Toast>().text.starts_with("You pack up the"));
    }

    #[test]
    fn the_veil_follows_the_sleeper_on_screen() {
        let (mut app, _, _) = sleeper("Bedroll");
        app.add_systems(Update, fade_veil);
        let veil = app.world_mut().spawn((SleepVeil::default(), BackgroundColor(BG.with_alpha(0.0)), Children::default())).id();
        press(&mut app, KeyCode::KeyF);
        std::thread::sleep(std::time::Duration::from_millis(100));
        app.update();
        let dark = app.world().get::<SleepVeil>(veil).unwrap().dark;
        assert!(dark > 0.0, "it has begun to fall: {dark}");
        assert!(app.world().get::<BackgroundColor>(veil).unwrap().0.alpha() > 0.0);
    }
}
