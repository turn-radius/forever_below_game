//! Held gear: an Armory item fitted to a hand (the plate prototype's `saHeld`, `fitGrip` and
//! `poseFor`, `reference/forever-below-plate.html`). The rules are in `docs/body.md`, "Held
//! gear"; the constants come from `data/constants.json`.
//!
//! [`fit_held`] takes a body, a hand and an item and returns a [`HeldFit`]: the pose the arm
//! takes to hold it (the fingers closed on its grip), the item's state as drawn (its handle
//! thinned to fit the fist), and its mesh placed in body space. Draw the body with
//! [`HeldFit::pose`] ([`Body::posed`]) so hand and item meet.
//!
//! How each family is held ([`Hold`]):
//! - **One-handed** (sword, dagger, mace, club, axe): the haft runs through the fist, so it
//!   stands perpendicular to the forearm, with the elbow bent. Swords and daggers turn
//!   [`BLADE_TURN`] about the grip so an edge leads and draw at [`BLADE_SCALE`]` × sqrt(s)`;
//!   axes turn so the bit faces forward.
//! - **Upright** (staff, spear, and any haft longer than [`LONG_HAFT`] of the bearer's
//!   height): the forearm comes up level so the haft stands upright in the fist, the butt
//!   kept off the ground.
//! - **Bow**: the fist closes on the riser at the bow's origin, the bow upright, the string
//!   back toward the archer, at most [`BOW_MAX`] of the archer's height.
//! - **Shield**: strapped at the fist with the forearm along its back, the board clear of the
//!   hand and forearm, at most [`SHIELD_MAX`] of the bearer's height.
//! - **Torch**: upright in the fist like a staff, the forearm level, so the flame burns
//!   straight up at shoulder height, forward of the body and clear of the arm, the arm out to
//!   the side so the flame burns beside the head.
//! - **Lantern**: the fist closes on the carrying ring and the lantern hangs plumb below it.
//! - **Crossbow**: carried by the stock's waist, deck up.
//!
//! A lantern can also hang from the belt at the hip ([`fit_hip`]), plumb from its carrying
//! ring, clear of the hips.

use armory::build::hafted::{Haft, grip_span};
use armory::geom::vec3::{self, Mat3, V3};
use armory::sections::hafted::{Grip, Pommel, Shaft, Swell, Wrap};
use armory::{Family, Item, ItemMesh};

use crate::body::{ArmPose, Body, Pose};
use crate::hand::hand_meshes;
use crate::limb::Side;
use crate::place::Placement;
use crate::spec::BodySpec;

/// Held swords and daggers turn this far about the grip so an edge leads (`BLADE_TURN`).
pub const BLADE_TURN: f32 = 55.0 * std::f32::consts::PI / 180.0;
/// Held swords and daggers draw at this times `sqrt(s)` (`BLADE_SCALE`).
pub const BLADE_SCALE: f32 = 0.78;
/// A held bow draws at this times `sqrt(s)` (`BOW_SCALE`)...
pub const BOW_SCALE: f32 = 0.8;
/// ...and at most this fraction of the archer's height tip to tip (`BOW_MAX`).
pub const BOW_MAX: f32 = 0.62;
/// A shield draws at this times `sqrt(s)` (`SHIELD_SCALE`)...
pub const SHIELD_SCALE: f32 = 0.9;
/// ...and at most this fraction of the bearer's height (`SHIELD_MAX`).
pub const SHIELD_MAX: f32 = 0.42;
/// A mace, club or axe whose haft is longer than this fraction of the bearer's height is a
/// two-handed polearm and is held upright like a staff. Our own proposal: the plate only
/// had one-handed clubs and hand axes.
pub const LONG_HAFT: f32 = 0.45;
/// How far out from the body a torch arm spreads, radians. Our own: the plate's 0.14 brought
/// the flame up across a gnome's face (its head is large and its arms short); this far out it
/// burns beside the head on every folk.
pub const TORCH_SPREAD: f32 = 0.5;
/// The smallest grip a hand closes on, the floor of `Body::grip_radius`'s clamp: a fist
/// round a shield's strap.
pub const STRAP_GRIP: f32 = 0.07;
/// How far the back of a shield's board stands clear of the hand and forearm (the plate's
/// `+ .07` in `saHeld`).
pub const SHIELD_CLEARANCE: f32 = 0.07;
/// How far above the ground the butt of an upright haft stays (the plate's `a.hand.y - .15`).
pub const BUTT_CLEARANCE: f32 = 0.15;

/// How a family is held. See the module docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    OneHanded,
    Upright,
    Bow,
    Shield,
    Torch,
    Lantern,
    Crossbow,
}

impl Hold {
    /// How `item` is held by `body`, or `None` for an item that is worn or carried, not held.
    pub fn of(item: &Item, body: &Body) -> Option<Hold> {
        let long = |shaft: &Shaft| shaft.length > LONG_HAFT * body.height;
        Some(match item {
            Item::Sword(_) | Item::Dagger(_) => Hold::OneHanded,
            Item::Mace(s) => if long(&s.shaft) { Hold::Upright } else { Hold::OneHanded },
            Item::Club(s) => if long(&s.shaft) { Hold::Upright } else { Hold::OneHanded },
            Item::Axe(s) => if long(&s.shaft) { Hold::Upright } else { Hold::OneHanded },
            Item::Staff(_) | Item::Spear(_) => Hold::Upright,
            Item::Bow(_) => Hold::Bow,
            Item::Shield(_) => Hold::Shield,
            Item::Torch(_) => Hold::Torch,
            Item::Lantern(_) => Hold::Lantern,
            Item::Crossbow(_) => Hold::Crossbow,
            _ => return None,
        })
    }

    /// The arm's pose before any solving (the plate's `poseFor`, by item rather than by
    /// slot; the lantern's is our own, out to the side so it hangs clear of the leg).
    fn arm_pose(self) -> ArmPose {
        let pose = |spread, bend, forward| ArmPose { spread, bend, forward, grip: None };
        match self {
            Hold::OneHanded => pose(0.2, 0.72, 0.1),
            Hold::Upright => pose(0.1, 0.62, 0.12),
            Hold::Bow => pose(0.12, 0.4, 0.06),
            Hold::Shield => pose(0.22, 0.85, 0.08),
            // Out to the side, so the flame burns beside the head rather than across the face.
            Hold::Torch => pose(TORCH_SPREAD, 0.8, 0.18),
            Hold::Lantern => pose(0.32, 0.45, 0.12),
            Hold::Crossbow => pose(0.16, 0.32, 0.06),
        }
    }

    /// Whether the forearm is solved level so the grip stands upright. A torch is held
    /// upright too, the flame clear of the face and forearm (the plate leaned it forward).
    fn stands_upright(self) -> bool {
        matches!(self, Hold::Upright | Hold::Bow | Hold::Torch)
    }
}

/// An item fitted to one hand.
#[derive(Clone, Debug, PartialEq)]
pub struct HeldFit {
    pub hold: Hold,
    /// The body's pose with the holding arm set and its fingers closed on the grip.
    pub pose: Pose,
    /// The item's state as drawn: its handle thinned to fit the fist, if it had to be.
    pub item: Item,
    /// The drawn item in body space.
    pub mesh: ItemMesh,
    /// The uniform scale the item is drawn at, before [`HeldFit::placement`].
    pub scale: f32,
    /// Where the scaled item space sits in body space.
    pub placement: Placement,
}

impl HeldFit {
    /// A point in the item's own space (an anchor, a light) in body space.
    pub fn to_body(&self, p: V3) -> V3 {
        self.placement.point(vec3::scale(p, self.scale))
    }

    /// The radius of the grip the fingers close on, as drawn.
    pub fn grip_radius(&self, side: Side) -> Option<f32> {
        self.pose.arm(side).grip
    }
}

/// Fits `item` to `body`'s hand on `side`: poses the arm, thins the grip to the fist, and
/// places the item. `None` for items that are not held (worn gear, rings, food...).
pub fn fit_held(body: &Body, side: Side, item: &Item) -> Option<HeldFit> {
    let hold = Hold::of(item, body)?;
    let first = fit_with(body, side, item, hold, hold.arm_pose())?;
    let Item::Torch(torch) = item else { return Some(first) };
    // A torch burns beside the head, not across the face: spread the arm further out until its
    // flame (where it is, or would be, lit) clears the head as seen from the front. Our own
    // rule; a goblin's ears reach far out to the sides.
    let mut lit = torch.clone();
    lit.burn.state = armory::families::torch::BurnState::Burning;
    let flame = armory::families::torch::Torch::light(&lit).map_or([0.0, 0.0, 0.0], |l| l.position);
    let head = body.head_mesh().bounds();
    let clear = |fit: &HeldFit| {
        let at = fit.to_body(flame);
        head.is_none_or(|(lo, hi)| at[0] < lo[0] - HEAD_CLEARANCE || at[0] > hi[0] + HEAD_CLEARANCE || at[1] < lo[1] - HEAD_CLEARANCE || at[1] > hi[1] + HEAD_CLEARANCE)
    };
    if clear(&first) {
        return Some(first);
    }
    for k in 1..=6 {
        let arm = ArmPose { spread: TORCH_SPREAD + 0.05 * k as f32, ..hold.arm_pose() };
        let Some(fit) = fit_with(body, side, item, hold, arm) else { break };
        if clear(&fit) {
            return Some(fit);
        }
    }
    // Spread further the arm would only lift the flame along the head's side (past a
    // goblin's ears): lean the torch out from the fist instead, a little at a time.
    let mut leaned = first;
    for k in 1..=8 {
        leaned = lean_out(&fit_with(body, side, item, hold, hold.arm_pose())?, body, side, 0.05 * k as f32);
        if clear(&leaned) {
            break;
        }
    }
    Some(leaned)
}

/// `fit` with its item turned `angle` radians about the fist, its top out from the body.
fn lean_out(fit: &HeldFit, body: &Body, side: Side, angle: f32) -> HeldFit {
    let posed = Body::posed(&body.spec, fit.pose);
    let pivot = posed.grip(side).origin;
    let turn = Mat3::from_axis_angle(vec3::Z, -side.sign() * angle);
    let placement = Placement { rotation: turn.mul_mat(&fit.placement.rotation), origin: vec3::add(pivot, turn.transform(vec3::sub(fit.placement.origin, pivot))) };
    let mut mesh = fit.item.build();
    scale_mesh(&mut mesh, fit.scale);
    placement.apply_to_item(&mut mesh);
    HeldFit { placement, mesh, ..fit.clone() }
}

/// How far a held flame keeps clear of the head's outline from the front.
const HEAD_CLEARANCE: f32 = 0.15;

/// [`fit_held`] with the arm starting from `arm`.
fn fit_with(body: &Body, side: Side, item: &Item, hold: Hold, mut arm: ArmPose) -> Option<HeldFit> {
    let scale = draw_scale(body, item);
    if hold.stands_upright() {
        arm = level_forearm(body, side, arm);
    }
    // The fit needs the hand's height (an upright haft's butt stays off the ground), and the
    // grip channel's position barely moves with the radius it closes on.
    let provisional = Body::posed(&body.spec, with_arm(body.pose, side, arm.holding(body.grip_radius())));
    let hand_y = provisional.grip(side).origin[1];
    let fitted = fit_grip(body, item, hold, scale, hand_y);
    let pose = with_arm(body.pose, side, arm.holding(fitted.radius));
    let posed = Body::posed(&body.spec, pose);
    let placement = place(&posed, side, hold, &fitted, scale);
    let mut mesh = fitted.item.build();
    scale_mesh(&mut mesh, scale);
    placement.apply_to_item(&mut mesh);
    Some(HeldFit { hold, pose, item: fitted.item, mesh, scale, placement })
}

/// The hand the main-hand slot holds in: the right (the plate's `buildCharacter` draws
/// `C.items.mainhand` on arm `'R'` and the off hand's on `'L'`).
pub const MAIN_HAND: Side = Side::Right;
/// The off hand.
pub const OFF_HAND: Side = Side::Left;

/// What both hands hold, fitted together: the body posed for both, and each hand's fit.
#[derive(Clone, Debug)]
pub struct Hands {
    /// The body with both arms posed, as each fit drew its item for.
    pub body: Body,
    pub main: Option<HeldFit>,
    pub off: Option<HeldFit>,
}

/// Fits `main` to [`MAIN_HAND`] and `off` to [`OFF_HAND`] of the body for `spec`. Each fit
/// poses only its own arm ([`fit_held`] starts from the body's pose), so the off hand is fitted
/// on the body already posed for the main hand, and the body both are drawn on is posed for
/// both. An item that is not held (worn gear, food) is left out, its arm relaxed.
pub fn fit_hands(spec: &BodySpec, main: Option<&Item>, off: Option<&Item>) -> Hands {
    let mut body = Body::new(spec);
    let main = main.and_then(|item| fit_held(&body, MAIN_HAND, item));
    if let Some(fit) = &main {
        body = Body::posed(spec, fit.pose);
    }
    let off = off.and_then(|item| fit_held(&body, OFF_HAND, item));
    if let Some(fit) = &off {
        body = Body::posed(spec, fit.pose);
    }
    Hands { body, main, off }
}

/// The scale an item is drawn at in the hand.
fn draw_scale(body: &Body, item: &Item) -> f32 {
    let root = body.scale.sqrt();
    // Bows and shields are capped by their built height along item Y (a bow's nocks and
    // string loops stand past the limbs' measured length).
    let height = || item.build().bounds().map_or(1.0, |(lo, hi)| hi[1] - lo[1]).max(1e-3);
    match item {
        Item::Sword(_) | Item::Dagger(_) => BLADE_SCALE * root,
        Item::Bow(_) => (BOW_SCALE * root).min(body.height * BOW_MAX / height()),
        Item::Shield(_) => (SHIELD_SCALE * root).min(body.height * SHIELD_MAX / height()),
        _ => 1.0,
    }
}

fn with_arm(mut pose: Pose, side: Side, arm: ArmPose) -> Pose {
    match side {
        Side::Right => pose.right = arm,
        Side::Left => pose.left = arm,
    }
    pose
}

/// Raises the forearm until the grip channel (which runs across the fist, perpendicular to
/// the forearm) stands as near upright as it can: a level forearm. A grid search over the
/// bend keeps it a pure function of the body.
fn level_forearm(body: &Body, side: Side, arm: ArmPose) -> ArmPose {
    let channel = |bend: f32| {
        let a = ArmPose { bend, ..arm }.holding(body.grip_radius());
        Body::posed(&body.spec, with_arm(body.pose, side, a)).grip(side).y()[1]
    };
    let search = |bends: &mut dyn Iterator<Item = f32>| {
        bends.fold((arm.bend, f32::MIN), |best, bend| {
            let up = channel(bend);
            if up > best.1 { (bend, up) } else { best }
        })
    };
    // Coarse, then fine round the best.
    let (coarse, _) = search(&mut (0..=26).map(|i| 0.3 + i as f32 * 0.05));
    let (fine, _) = search(&mut (0..=20).map(|i| coarse - 0.05 + i as f32 * 0.005));
    ArmPose { bend: fine, ..arm }
}

// ── Fitting the grip ──

/// The most a thinned haft may flare toward its head (top radius over butt radius). Our own
/// bound: past it the haft would read as a club's swelling, so it flares no further and the
/// head's neck narrows a little.
const MAX_TAPER: f32 = 2.5;

/// The parts of a built item a fist closes round: a haft's grip, wrap, bare shaft and bands,
/// a torch's handle and its thong, a bow's riser and its wrap, a crossbow's stock.
const HANDLE_PARTS: &[&str] = &["grip", "wrap", "shaft", "bands", "handle", "riser", "grip_wrap", "stock"];

/// An item with its handle fitted, and where the fist closes on it in item space.
struct FittedGrip {
    item: Item,
    /// The centre of the grip and the direction of the channel through the fist, in item
    /// space before scaling.
    hold: V3,
    axis: V3,
    /// The radius the fingers close on, as drawn.
    radius: f32,
}

/// Thins the item's handle so its drawn radius stays within the hand's
/// ([`Body::grip_radius`]), and finds where the fist closes on it.
///
/// The radius is measured on the built item: the farthest any handle part stands from the
/// line through the grip, across the width of the fist. So a wrap's cord, its bindings, a
/// carved grip and a torch's thong all count, without restating how the Armory builds them.
///
/// Only the handle shrinks. For a hafted item the state's shaft is thinned and the item
/// rebuilt by the Armory, so blades, heads, guards and bits keep their size, exactly as the
/// plate's `fitGrip` thinned `shaft.radius` before building. Scaling the built mesh round
/// the grip instead would have squeezed the guard, langets and head necks that overlap it,
/// and scaling the whole item would have shrunk its blade or head with the grip. Where a
/// head sits on the shaft's top (or an axe's eye), the shaft is tapered so its radius there
/// is unchanged and the head still fits it; a club's swelling widens to keep its girth.
/// A torch's handle, a bow's riser and a crossbow's stock thin the same way.
fn fit_grip(body: &Body, item: &Item, hold: Hold, scale: f32, hand_y: f32) -> FittedGrip {
    // In item units: the cap and the fist's width over the draw scale.
    let cap = body.grip_radius() / scale;
    let half_fist = body.hand_form().knuckle_half_width / scale;
    let reach = (hold == Hold::Upright).then_some(hand_y - BUTT_CLEARANCE);
    let mut item = item.clone();
    match &item {
        Item::Lantern(s) => {
            // The fist closes round the wire at the top of the carrying ring (centred on the
            // origin, in the YZ plane).
            let ring = s.bail.ring;
            let top = item.build().parts.iter().filter(|p| p.name == "ring").flat_map(|p| &p.positions).fold(ring, |m, p| m.max(p[1]));
            return FittedGrip { hold: [0.0, ring, 0.0], axis: vec3::Z, radius: (top - ring).max(0.01) * scale, item };
        }
        // A strap folded in the fist; the shield is placed by the forearm, not by this.
        Item::Shield(_) => return FittedGrip { hold: [0.0; 3], axis: vec3::Y, radius: STRAP_GRIP, item },
        _ => {}
    }
    let (mut hold_point, mut axis, mut radius) = ([0.0; 3], vec3::Y, 0.0);
    // Not all of the radius thins in step (a wrap's cord thins by the square root, a taper
    // moves the widest point), so each pass learns how far the last thinning reached and
    // aims past the cap by that much; two or three passes settle it.
    let (mut reached, mut last): (f32, Option<(f32, f32)>) = (1.0, None);
    for _ in 0..8 {
        (hold_point, axis) = grip_line(&mut item, reach);
        radius = widest(&item.build(), hold_point, axis, half_fist);
        if radius <= cap * 1.001 {
            break;
        }
        if let Some((before, k)) = last {
            reached = ((radius / before).ln() / k.ln()).clamp(0.25, 1.0);
        }
        let k = (cap * 0.998 / radius).powf(1.0 / reached);
        thin(&mut item, k, reach);
        last = Some((radius, k));
    }
    FittedGrip { item, hold: hold_point, axis, radius: radius.min(cap) * scale }
}

/// The farthest any handle part of `mesh` stands from the line through `at` along `axis`,
/// within `half_fist` of `at` along it.
fn widest(mesh: &ItemMesh, at: V3, axis: V3, half_fist: f32) -> f32 {
    mesh.parts
        .iter()
        .filter(|p| HANDLE_PARTS.contains(&p.name))
        .flat_map(|p| &p.positions)
        .map(|p| vec3::sub(*p, at))
        .filter(|d| vec3::dot(*d, axis).abs() <= half_fist)
        .map(|d| vec3::length(vec3::sub(d, vec3::scale(axis, vec3::dot(d, axis)))))
        .fold(0.0, f32::max)
}

/// Where the fist closes on an item, and the direction of the channel through it, in item
/// space.
fn grip_line(item: &mut Item, reach: Option<f32>) -> (V3, V3) {
    let hafted = with_haft(item, |h| {
        let t = h.hold_t(reach);
        let haft = h.haft();
        (haft.point(t), haft.tangent(t))
    });
    if let Some(line) = hafted {
        return line;
    }
    // A torch's hand is at y = 0, its handle up +Y; a bow's riser is at its origin, its
    // limbs along Y; a crossbow is held by the waist of its stock, in the narrow middle
    // (0.42 to 0.82 of its length, where `Stock::width_at` is `mid_width`). A torch's handle
    // may bow, so the line runs through the handle's measured centre there.
    let y = match item {
        Item::Crossbow(s) => s.stock.length * 0.6,
        _ => 0.0,
    };
    let mesh = item.build();
    let (below, above) = (centre_at(&mesh, y - 0.25), centre_at(&mesh, y + 0.25));
    (vec3::lerp(below, above, 0.5), vec3::normalize_or(vec3::sub(above, below), vec3::Y))
}

/// The middle of the handle parts' cross-section at height `y` (item space).
fn centre_at(mesh: &ItemMesh, y: f32) -> V3 {
    let slab = mesh.parts.iter().filter(|p| HANDLE_PARTS.contains(&p.name)).flat_map(|p| &p.positions).filter(|p| (p[1] - y).abs() < 0.06);
    let (lo, hi) = slab.fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[2])]));
    if lo[0] > hi[0] {
        return [0.0, y, 0.0];
    }
    [(lo[0] + hi[0]) * 0.5, y, (lo[1] + hi[1]) * 0.5]
}

/// Thins an item's handle by `k`.
fn thin(item: &mut Item, k: f32, reach: Option<f32>) {
    let hafted = with_haft(item, |mut h| {
        let t = h.hold_t(reach);
        h.thin(k, t);
    });
    if hafted.is_some() {
        return;
    }
    match item {
        Item::Torch(s) => s.handle.radius *= k,
        Item::Bow(s) => {
            s.riser.width *= k;
            s.riser.thickness *= k;
        }
        Item::Crossbow(s) => {
            s.stock.mid_width *= k;
            s.stock.depth *= k;
        }
        _ => {}
    }
}

/// Runs `f` on the handle of a hafted item; `None` for any other.
fn with_haft<R>(item: &mut Item, f: impl FnOnce(HaftedGrip<'_>) -> R) -> Option<R> {
    Some(match item {
        Item::Sword(s) => f(HaftedGrip::new(&mut s.shaft, &s.grip, &mut s.wrap, &mut s.pommel, s.seed)),
        Item::Dagger(s) => f(HaftedGrip::new(&mut s.shaft, &s.grip, &mut s.wrap, &mut s.pommel, s.seed)),
        Item::Mace(s) => f(HaftedGrip::new(&mut s.shaft, &s.grip, &mut s.wrap, &mut s.pommel, s.seed)),
        Item::Club(s) => f(HaftedGrip { swell: Some(&mut s.swell), ..HaftedGrip::new(&mut s.shaft, &s.grip, &mut s.wrap, &mut s.pommel, s.seed) }),
        Item::Axe(s) => {
            let eye = s.eye_position();
            f(HaftedGrip { head_at: eye, ..HaftedGrip::new(&mut s.shaft, &s.grip, &mut s.wrap, &mut s.pommel, s.seed) })
        }
        Item::Staff(s) => f(HaftedGrip { bare_hold: 0.45, ..HaftedGrip::new(&mut s.shaft, &s.grip, &mut s.wrap, &mut s.pommel, s.seed) }),
        Item::Spear(s) => f(HaftedGrip { bare_hold: 0.42, ..HaftedGrip::new(&mut s.shaft, &s.grip, &mut s.wrap, &mut s.pommel, s.seed) }),
        _ => return None,
    })
}

/// The parts of a hafted item's state that make its handle.
struct HaftedGrip<'a> {
    shaft: &'a mut Shaft,
    grip: &'a Grip,
    wrap: &'a mut Wrap,
    pommel: &'a mut Pommel,
    swell: Option<&'a mut Swell>,
    /// Where the head sits on the shaft (0 butt, 1 top): its radius there is kept.
    head_at: f32,
    /// Where the hand holds a shaft without a grip, as a fraction from the butt (the plate's
    /// 0.2, or 0.45 for a staff and 0.42 for a spear).
    bare_hold: f32,
    seed: u32,
}

impl<'a> HaftedGrip<'a> {
    fn new(shaft: &'a mut Shaft, grip: &'a Grip, wrap: &'a mut Wrap, pommel: &'a mut Pommel, seed: u32) -> HaftedGrip<'a> {
        HaftedGrip { shaft, grip, wrap, pommel, swell: None, head_at: 1.0, bare_hold: 0.2, seed }
    }

    /// The haft as the Armory lays it out (a club's with its swelling).
    fn haft(&self) -> Haft {
        let haft = Haft::new(self.shaft, self.seed);
        match &self.swell {
            Some(sw) if sw.on => haft.with_swell(sw),
            _ => haft,
        }
    }

    /// The middle of the grip (`grip.position * L + grip.length / 2`), or the bare hold,
    /// as a fraction of the shaft, no further up than `reach` (in units from the end of the
    /// butt cap, which hangs below the shaft's foot).
    fn hold_t(&self, reach: Option<f32>) -> f32 {
        let t = if self.grip.on || self.wrap.on {
            let (a, b) = grip_span(self.shaft, self.grip);
            (a + b) * 0.5
        } else {
            self.bare_hold
        };
        let butt = if self.pommel.on { self.pommel.length } else { 0.0 };
        let limit = reach.map_or(1.0, |r| (r - butt) / self.shaft.length.max(0.1));
        t.min(limit).max(0.02)
    }

    /// Thins the handle by `k` at `t`, keeping the radius where the head sits (by tapering
    /// the shaft) when the head is far enough above the hand; the wrap's cord, the pommel
    /// and the shaft's bands thin by `sqrt(k)`, as in the plate's `fitGrip`.
    fn thin(&mut self, k: f32, t: f32) {
        let s = &mut *self.shaft;
        let line = |r0: f32, taper: f32, t: f32| r0 * (1.0 + (taper - 1.0) * t);
        let (r0, taper) = (s.radius, s.taper);
        let at_hand = k * line(r0, taper, t);
        let gap = self.head_at - t;
        let mut tapered = false;
        if gap > 0.15 {
            // A straight taper through the thinned radius at the hand and the old radius at
            // the head. This may flare past the taper slider's 1.6: a haft swelling to its
            // head is how a thick-necked mace is held in a small hand.
            let slope = (line(r0, taper, self.head_at) - at_hand) / gap;
            let base = at_hand - slope * t;
            let new_taper = if base > 0.0 { (base + slope) / base } else { f32::MAX };
            if new_taper >= 0.5 {
                // Flared as far as it may go: the head's neck as near its own as it can be.
                let new_taper = new_taper.min(MAX_TAPER);
                (s.radius, s.taper) = (at_hand / (1.0 + (new_taper - 1.0) * t), new_taper);
                tapered = true;
            }
        }
        if !tapered {
            s.radius *= k;
        }
        if let Some(sw) = self.swell.as_deref_mut().filter(|sw| sw.on) {
            let peak = sw.start + sw.peak * (1.0 - sw.start);
            sw.girth *= line(r0, taper, peak) / line(s.radius, s.taper, peak);
        }
        let root = k.sqrt();
        s.band_size *= root;
        self.wrap.cord *= root;
        self.pommel.radius *= root;
    }
}

// ── Placing ──

/// An orthonormal frame with `y` exactly and `z` as near `hint` as it can be (`fallback`
/// where `hint` runs along `y`); `x = y × z`.
fn frame_yz(y: V3, hint: V3, fallback: V3) -> Mat3 {
    let y = vec3::normalize_or(y, vec3::Y);
    let off = |h: V3| vec3::sub(h, vec3::scale(y, vec3::dot(h, y)));
    let z = vec3::try_normalize(off(hint)).filter(|_| vec3::length(off(hint)) > 0.05).unwrap_or_else(|| vec3::normalize_or(off(fallback), vec3::any_perpendicular(y)));
    Mat3::from_cols(vec3::cross(y, z), y, z)
}

/// The same with `x` as near `hint` as it can be; `z = x × y`.
fn frame_yx(y: V3, hint: V3, fallback: V3) -> Mat3 {
    let m = frame_yz(y, hint, fallback);
    let [z, y, _] = m.cols;
    Mat3::from_cols(z, y, vec3::cross(z, y))
}

fn transpose(m: &Mat3) -> Mat3 {
    let [a, b, c] = m.cols;
    Mat3::from_cols([a[0], b[0], c[0]], [a[1], b[1], c[1]], [a[2], b[2], c[2]])
}

/// The placement taking an item-space frame `local` at `local_origin` onto `body` at
/// `body_origin`.
fn align(local: &Mat3, local_origin: V3, body: &Mat3, body_origin: V3) -> Placement {
    let rotation = body.mul_mat(&transpose(local));
    Placement { rotation, origin: vec3::sub(body_origin, rotation.transform(local_origin)) }
}

/// Where the scaled item goes on the posed body.
fn place(body: &Body, side: Side, hold: Hold, fitted: &FittedGrip, scale: f32) -> Placement {
    let grip = body.grip(side);
    let channel = grip.y();
    let hold_point = vec3::scale(fitted.hold, scale);
    let forward = vec3::Z;
    let sg = side.sign();
    match hold {
        Hold::Shield => place_shield(body, side, &fitted.item, scale),
        Hold::Lantern => {
            // Plumb below the fist, the ring's wire running through it.
            let rotation = frame_yz(vec3::Y, channel, forward);
            Placement { rotation, origin: vec3::sub(grip.origin, rotation.transform(hold_point)) }
        }
        Hold::Bow => {
            // The string (item +X) back toward the archer: from the fist toward the shoulder.
            let toward = vec3::sub(body.arm(side).joints[0], grip.origin);
            let local = frame_yx(fitted.axis, vec3::X, vec3::Z);
            align(&local, hold_point, &frame_yx(channel, toward, vec3::neg(forward)), grip.origin)
        }
        Hold::Crossbow => {
            let local = frame_yz(fitted.axis, vec3::Z, vec3::X);
            align(&local, hold_point, &frame_yz(channel, vec3::Y, forward), grip.origin)
        }
        _ if matches!(fitted.item, Item::Axe(_)) => {
            // The bit (item +X) faces forward: a 90° turn about the haft from the blades' frame.
            let local = frame_yx(fitted.axis, vec3::X, vec3::Z);
            align(&local, hold_point, &frame_yx(channel, forward, vec3::Y), grip.origin)
        }
        _ => {
            // The flats (item ±Z) face forward; blades then turn so an edge leads.
            let local = frame_yz(fitted.axis, vec3::Z, vec3::X);
            let mut target = frame_yz(channel, forward, vec3::Y);
            if matches!(fitted.item.family(), Family::Sword | Family::Dagger) {
                target = Mat3::from_axis_angle(channel, sg * BLADE_TURN).mul_mat(&target);
            }
            align(&local, hold_point, &target, grip.origin)
        }
    }
}

/// A shield on the forearm (after the plate's `saHeld` shield branch): its face
/// perpendicular to the forearm, so the forearm lies along its back, and level, so the board
/// stands upright facing out from the body (the plate turned it out and forward, which leans
/// the board whenever the forearm slopes); its centre a little back along the forearm from
/// the fist (the plate's 12% of its height), and its back clear of the hand and forearm by
/// [`SHIELD_CLEARANCE`].
fn place_shield(body: &Body, side: Side, item: &Item, scale: f32) -> Placement {
    let arm = body.arm(side);
    let fist = body.grip(side).origin;
    let along = arm.tangent_at(1.0);
    // The face (item +Z) is perpendicular to the forearm and level, so the board stands
    // upright facing out from the body; its up (+Y) is then the world's up.
    let side_out = [side.sign(), 0.0, 0.0];
    let level = vec3::cross(vec3::Y, along);
    let level = if vec3::dot(level, side_out) < 0.0 { vec3::neg(level) } else { level };
    let out = vec3::normalize_or(level, side_out);
    let up = vec3::normalize_or(vec3::sub(vec3::Y, vec3::scale(out, out[1])), vec3::Y);
    let rotation = Mat3::from_cols(vec3::cross(up, out), up, out);

    // The board's back in item space: the box of every part but the straps and grip behind.
    let mesh = item.build();
    let board: Vec<V3> = mesh.parts.iter().filter(|p| !matches!(p.name, "straps" | "grip")).flat_map(|p| p.positions.iter().copied()).collect();
    let (lo, hi) = board.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    });
    let back_centre = vec3::scale([(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, lo[2]], scale);
    let height = (hi[1] - lo[1]) * scale;

    // How far the hand and forearm reach toward the board, from the fist.
    let (hand, _) = hand_meshes(&body.hand_form(), &body.hand_pose(side));
    let at = body.hand_placement(side);
    let hand_reach = hand.positions.iter().map(|p| vec3::dot(vec3::sub(at.point(*p), fist), out)).fold(f32::MIN, f32::max);
    let te = arm.t_joints[1];
    let arm_reach = (0..=10)
        .map(|i| te + (1.0 - te) * i as f32 / 10.0)
        .map(|t| vec3::dot(vec3::sub(arm.point_at(t), fist), out) + arm.radius_at(t))
        .fold(f32::MIN, f32::max);
    let clear = hand_reach.max(arm_reach) + SHIELD_CLEARANCE;
    let centre = vec3::add(vec3::add(fist, vec3::scale(out, clear)), vec3::scale(along, -height * 0.12));
    Placement { rotation, origin: vec3::sub(centre, rotation.transform(back_centre)) }
}

/// Scales an item uniformly about its origin; normals keep their direction.
fn scale_mesh(mesh: &mut ItemMesh, k: f32) {
    for part in &mut mesh.parts {
        for p in &mut part.positions {
            *p = vec3::scale(*p, k);
        }
    }
}

// ── Hung at the hip ──

/// The hip a lantern hangs at: the off hand's, so a scabbard's side stays the main hand's
/// reach across. Our own choice.
pub const HIP_SIDE: Side = OFF_HAND;
/// How far round the waist from the hip's side toward the front the lantern hangs, radians:
/// forward of an arm hanging at the side, where it can be seen when looking down. Our own.
pub const HIP_TURN: f32 = 0.6;
/// How far the lantern's body stands clear of the hips and thigh below the belt, over the
/// hip ring's own width (trousers and a tunic's skirt lie there). Our own.
const HIP_CLEARANCE: f32 = 0.12;

/// A lantern hung from the belt at the hip ([`fit_hip`]).
#[derive(Clone, Debug, PartialEq)]
pub struct HipFit {
    /// The lantern as drawn (as rolled; nothing is thinned).
    pub item: Item,
    /// The drawn lantern in body space.
    pub mesh: ItemMesh,
    /// The uniform scale it is drawn at, before [`HipFit::placement`].
    pub scale: f32,
    /// Where the scaled item space sits in body space.
    pub placement: Placement,
    /// The point it hangs from, in body space: the top of its carrying ring, on the belt. It
    /// swings about this point.
    pub hook: V3,
    pub side: Side,
}

impl HipFit {
    /// A point in the item's own space (its light) in body space.
    pub fn to_body(&self, p: V3) -> V3 {
        self.placement.point(vec3::scale(p, self.scale))
    }
}

/// Hangs a lantern from `belt` (the worn belt's mesh in body space, from
/// [`crate::fit_worn`]) at [`HIP_SIDE`]'s hip, [`HIP_TURN`] toward the front: plumb, its
/// carrying ring over the belt (the belt runs through the ring, as the fist's fingers do when
/// it is held), the top of the ring at the middle of the belt's band, and its body far enough
/// out to clear the hips below. `None` for anything but a lantern, or a belt with no mesh.
/// The plate drew no hip lantern; the fit is our own.
pub fn fit_hip(body: &Body, item: &Item, belt: &ItemMesh) -> Option<HipFit> {
    let Item::Lantern(state) = item else { return None };
    let side = HIP_SIDE;
    let out = [side.sign() * HIP_TURN.cos(), 0.0, HIP_TURN.sin()];
    // The belt's outermost point that way, and the middle of its band there.
    let band: Vec<V3> = belt.parts.iter().filter(|p| p.name == "belt").flat_map(|p| p.positions.iter().copied()).collect();
    let band = if band.is_empty() { belt.parts.iter().flat_map(|p| p.positions.iter().copied()).collect() } else { band };
    let reach = |p: &V3| p[0] * out[0] + p[2] * out[2];
    let support = band.iter().map(reach).fold(f32::MIN, f32::max);
    if support == f32::MIN {
        return None;
    }
    let near: Vec<&V3> = band.iter().filter(|p| reach(p) > support - 0.15).collect();
    let belt_y = near.iter().map(|p| p[1]).sum::<f32>() / near.len() as f32;
    // How far out the hips stand that way, as an ellipse (body space is centred on the trunk).
    let hip = &body.hip;
    let hips = ((hip.half_width * out[0]).powi(2) + (hip.half_depth * out[2]).powi(2)).sqrt() + HIP_CLEARANCE;

    let scale = 1.0;
    let mesh = item.build();
    let ring = state.bail.ring;
    let top = mesh.parts.iter().filter(|p| p.name == "ring").flat_map(|p| &p.positions).fold(ring, |m, p| m.max(p[1]));
    // The widest the lantern stands out from its hanging line, below the ring.
    let girth = mesh.parts.iter().filter(|p| !matches!(p.name, "ring" | "bail" | "chain")).flat_map(|p| &p.positions).map(|p| p[0].hypot(p[2])).fold(0.0, f32::max) * scale;
    let distance = support.max(hips + girth);
    let hook = [out[0] * distance, belt_y, out[2] * distance];
    // Plumb, the ring's plane (item YZ) along the belt: item +Z runs round the waist.
    let along = [-out[2], 0.0, out[0]];
    let rotation = frame_yz(vec3::Y, along, out);
    let grip = vec3::scale([0.0, top, 0.0], scale);
    let placement = Placement { rotation, origin: vec3::sub(hook, rotation.transform(grip)) };
    let mut drawn = mesh;
    scale_mesh(&mut drawn, scale);
    placement.apply_to_item(&mut drawn);
    Some(HipFit { item: item.clone(), mesh: drawn, scale, placement, hook, side })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BodySpec, Folk};

    fn body(folk: Folk) -> Body {
        Body::new(&BodySpec::roll(folk, 1207))
    }

    fn roll(family: Family, seed: u32) -> Item {
        Item::roll(family, seed).unwrap()
    }

    /// The first rolled item of `family` that `want` accepts.
    fn find(family: Family, want: impl Fn(&Item) -> bool) -> Item {
        (0..4000).map(|s| roll(family, s)).find(|i| want(i)).unwrap_or_else(|| panic!("no {family:?} found"))
    }

    fn fit(body: &Body, side: Side, item: &Item) -> HeldFit {
        fit_held(body, side, item).unwrap()
    }

    #[test]
    fn a_lantern_hangs_plumb_from_the_belt_at_the_off_hand_hip() {
        use fb_items::Slot;
        for folk in Folk::ALL {
            let body = body(folk);
            let belt = crate::fit_worn(&body, &[(Slot::Belt, roll(Family::Belt, 3))]);
            let belt = &belt.layer(Slot::Belt).unwrap().mesh;
            let (lo, hi) = belt.bounds().unwrap();
            for seed in [1, 2, 3, 1207] {
                let lantern = roll(Family::Lantern, seed);
                let hip = fit_hip(&body, &lantern, belt).unwrap();
                // Hung from the belt's band, on the off hand's side and toward the front.
                assert!(hip.hook[1] >= lo[1] - 1e-3 && hip.hook[1] <= hi[1] + 1e-3, "{folk:?} {seed}: hook {:?} off the belt {lo:?}..{hi:?}", hip.hook);
                assert!(hip.hook[0] * HIP_SIDE.sign() > 0.0 && hip.hook[2] > 0.0, "{folk:?} {seed}: {:?}", hip.hook);
                // Plumb: all of it below the hook, and its ring's top at the hook.
                let (mlo, mhi) = hip.mesh.bounds().unwrap();
                assert!(mhi[1] <= hip.hook[1] + 0.02, "{folk:?} {seed}: {mhi:?} above {:?}", hip.hook);
                assert!(mlo[1] > 0.0, "{folk:?} {seed}: through the floor");
                // Its body clear of the hips: no part inside the hip ring's ellipse below the belt.
                let inside = hip.mesh.parts.iter().filter(|p| !matches!(p.name, "ring" | "bail" | "chain")).flat_map(|p| &p.positions).any(|p| {
                    let (x, z) = (p[0] / body.hip.half_width, p[2] / body.hip.half_depth);
                    p[1] < body.waist_y && x * x + z * z < 1.0
                });
                assert!(!inside, "{folk:?} {seed}: inside the hips");
            }
        }
        assert!(fit_hip(&body(Folk::Human), &roll(Family::Sword, 1), &ItemMesh::default()).is_none());
    }

    fn points<'a>(mesh: &'a ItemMesh, names: &'a [&str]) -> impl Iterator<Item = V3> + 'a {
        mesh.parts.iter().filter(move |p| names.contains(&p.name)).flat_map(|p| p.positions.iter().copied())
    }

    fn centroid(mesh: &ItemMesh, names: &[&str]) -> V3 {
        let all: Vec<V3> = points(mesh, names).collect();
        assert!(!all.is_empty(), "no {names:?}");
        vec3::centroid(&all)
    }

    /// The posed body's grip frame for `side`.
    fn grip(body: &Body, side: Side, fit: &HeldFit) -> Placement {
        Body::posed(&body.spec, fit.pose).grip(side)
    }

    /// Forward, with any part along `axis` removed.
    fn forward_across(axis: V3) -> V3 {
        vec3::normalize_or(vec3::sub(vec3::Z, vec3::scale(axis, axis[2])), vec3::Z)
    }

    /// The spread of `pts` along `axis`.
    fn span(pts: impl Iterator<Item = V3>, axis: V3) -> f32 {
        let d: Vec<f32> = pts.map(|p| vec3::dot(p, axis)).collect();
        d.iter().copied().fold(f32::MIN, f32::max) - d.iter().copied().fold(f32::MAX, f32::min)
    }

    const HANDLES: &[&str] = &["grip", "wrap", "shaft", "handle", "riser", "grip_wrap", "stock"];

    #[test]
    fn the_grip_never_exceeds_the_hand() {
        let families = [Family::Sword, Family::Dagger, Family::Mace, Family::Club, Family::Axe, Family::Staff, Family::Spear, Family::Torch, Family::Bow, Family::Crossbow];
        for folk in Folk::ALL {
            let body = body(folk);
            let cap = body.grip_radius();
            let half_fist = body.hand_form().knuckle_half_width;
            for family in families {
                for seed in 0..10 {
                    let item = roll(family, seed);
                    let fit = fit(&body, Side::Right, &item);
                    let r = fit.grip_radius(Side::Right).unwrap();
                    assert!(r <= cap + 1e-4, "{folk:?} {}: grip {r} over {cap}", item.noun());
                    // And as drawn: the handle inside the fist stays within the hand's grip.
                    let g = grip(&body, Side::Right, &fit);
                    let widest = points(&fit.mesh, HANDLES)
                        .map(|p| vec3::sub(p, g.origin))
                        .filter(|d| vec3::dot(*d, g.y()).abs() < half_fist * 0.6)
                        .map(|d| vec3::length(vec3::sub(d, vec3::scale(g.y(), vec3::dot(d, g.y())))))
                        .fold(0.0, f32::max);
                    assert!(widest > 0.0 && widest <= cap * 1.1 + 0.005, "{folk:?} {} seed {seed}: drawn handle {widest} in a fist of {cap}", item.noun());
                }
            }
        }
    }

    #[test]
    fn one_handed_weapons_stand_across_the_forearm_with_the_elbow_bent() {
        let human = body(Folk::Human);
        let short = |item: &Item| Hold::of(item, &human) == Some(Hold::OneHanded);
        let items = [
            (roll(Family::Sword, 3), "blade"),
            (roll(Family::Dagger, 1), "blade"),
            (find(Family::Mace, short), "head"),
            (find(Family::Club, |i| short(i) && matches!(i, Item::Club(s) if s.has_knob())), "head"),
            (find(Family::Axe, short), "bit"),
        ];
        for side in Side::BOTH {
            for (item, business_end) in &items {
                let fit = fit(&human, side, item);
                assert_eq!(fit.hold, Hold::OneHanded);
                let posed = Body::posed(&human.spec, fit.pose);
                let g = posed.grip(side);
                let arm = posed.arm(side);
                let forearm = arm.tangent_at(1.0);
                assert!(vec3::dot(g.y(), forearm).abs() < 0.15, "{}: channel {:?} forearm {forearm:?}", item.noun(), g.y());
                // The blade or head stands out of the fist along the channel.
                let out = vec3::normalize_or(vec3::sub(centroid(&fit.mesh, &[business_end]), g.origin), vec3::Y);
                assert!(vec3::dot(out, g.y()) > 0.9, "{} {side:?}: {out:?} vs {:?}", item.noun(), g.y());
                let [s, e, w] = arm.joints;
                let bend = vec3::dot(vec3::normalize_or(vec3::sub(e, s), vec3::Y), vec3::normalize_or(vec3::sub(w, e), vec3::Y)).acos();
                assert!(bend > 0.7, "{}: elbow bent {bend} rad", item.noun());
            }
        }
    }

    #[test]
    fn swords_and_daggers_turn_an_edge_forward_and_draw_smaller() {
        for folk in [Folk::Human, Folk::Gnome] {
            let body = body(folk);
            for item in [roll(Family::Sword, 3), roll(Family::Dagger, 1)] {
                for side in Side::BOTH {
                    let fit = fit(&body, side, &item);
                    let g = grip(&body, side, &fit);
                    let ahead = forward_across(g.y());
                    // The blade's flats face item ±Z and its edge +X (`armory::build::blade`).
                    let flat = fit.placement.direction(vec3::Z);
                    let turn = vec3::dot(flat, ahead).clamp(-1.0, 1.0).acos();
                    assert!((turn - BLADE_TURN).abs() < 0.02, "{folk:?} {side:?}: turned {turn}");
                    assert!(vec3::dot(fit.placement.direction(vec3::X), ahead).abs() > vec3::dot(flat, ahead).abs());
                    assert!((fit.scale - BLADE_SCALE * body.scale.sqrt()).abs() < 1e-5);
                    // The blade is the item's own, drawn at that scale.
                    let own = span(points(&item.build(), &["blade"]), vec3::Y);
                    let drawn = span(points(&fit.mesh, &["blade"]), g.y());
                    assert!((drawn - own * fit.scale).abs() < own * fit.scale * 0.02, "{drawn} vs {own} x {}", fit.scale);
                }
            }
        }
    }

    #[test]
    fn hand_axes_turn_their_bit_forward() {
        let human = body(Folk::Human);
        let axe = find(Family::Axe, |i| Hold::of(i, &human) == Some(Hold::OneHanded));
        for side in Side::BOTH {
            let fit = fit(&human, side, &axe);
            let g = grip(&human, side, &fit);
            let to_bit = vec3::sub(centroid(&fit.mesh, &["bit"]), g.origin);
            let across = vec3::normalize_or(vec3::sub(to_bit, vec3::scale(g.y(), vec3::dot(to_bit, g.y()))), vec3::X);
            assert!(vec3::dot(across, forward_across(g.y())) > 0.95, "{side:?}: bit toward {across:?}");
        }
    }

    #[test]
    fn bows_close_on_the_riser_upright_with_the_string_back() {
        let length = |i: &Item| if let Item::Bow(s) = i { s.limbs.length } else { 0.0 };
        // The longest bow there is, so the cap bites, and an ordinary one.
        let longest = (0..400).map(|s| roll(Family::Bow, s)).max_by(|a, b| length(a).total_cmp(&length(b))).unwrap();
        for folk in [Folk::Human, Folk::Dwarf, Folk::Gnome] {
            let body = body(folk);
            for bow in [longest.clone(), roll(Family::Bow, 5)] {
                for side in Side::BOTH {
                    let fit = fit(&body, side, &bow);
                    let posed = Body::posed(&body.spec, fit.pose);
                    let g = posed.grip(side);
                    assert!(vec3::distance(fit.to_body([0.0; 3]), g.origin) < 1e-3, "the fist closes on the riser");
                    assert!(fit.placement.y()[1] > 0.985, "{folk:?}: upright {:?}", fit.placement.y());
                    let toward = vec3::sub(posed.arm(side).joints[0], g.origin);
                    let string = vec3::sub(centroid(&fit.mesh, &["string"]), g.origin);
                    assert!(string[0] * toward[0] + string[2] * toward[2] > 0.0, "{folk:?}: the string is back toward the archer");
                    let tall = span(fit.mesh.parts.iter().flat_map(|p| p.positions.iter().copied()), fit.placement.y());
                    assert!(tall <= body.height * BOW_MAX + 1e-3, "{folk:?}: bow {tall} for {}", body.height);
                }
            }
        }
    }

    #[test]
    fn shields_strap_at_the_fist_clear_of_the_hand() {
        let size = |i: &Item| if let Item::Shield(s) = i { s.body.size } else { 0.0 };
        let biggest = (0..400).map(|s| roll(Family::Shield, s)).max_by(|a, b| size(a).total_cmp(&size(b))).unwrap();
        for folk in [Folk::Human, Folk::Dwarf, Folk::Goblin] {
            let body = body(folk);
            for shield in [biggest.clone(), roll(Family::Shield, 2)] {
                for side in Side::BOTH {
                    let fit = fit(&body, side, &shield);
                    assert_eq!(fit.hold, Hold::Shield);
                    let posed = Body::posed(&body.spec, fit.pose);
                    let tall = span(fit.mesh.parts.iter().flat_map(|p| p.positions.iter().copied()), vec3::Y);
                    assert!(tall <= body.height * SHIELD_MAX + 1e-3, "{folk:?}: shield {tall} for {}", body.height);
                    // The forearm lies along the back: in the board's plane.
                    let face = fit.placement.z();
                    assert!(vec3::dot(face, posed.arm(side).tangent_at(1.0)).abs() < 0.02);
                    // The board stands clear of the hand.
                    let fist = posed.grip(side).origin;
                    let board_back = points(&fit.mesh, &["body", "face", "rim"]).map(|p| vec3::dot(vec3::sub(p, fist), face)).fold(f32::MAX, f32::min);
                    let (hand, _) = hand_meshes(&posed.hand_form(), &posed.hand_pose(side));
                    let at = posed.hand_placement(side);
                    let hand_front = hand.positions.iter().map(|p| vec3::dot(vec3::sub(at.point(*p), fist), face)).fold(f32::MIN, f32::max);
                    assert!(board_back > hand_front + SHIELD_CLEARANCE * 0.9, "{folk:?}: board {board_back} hand {hand_front}");
                    // And the fist is behind the board, not beside it.
                    let local = fit.placement.rotation.cols.map(|c| vec3::dot(c, vec3::sub(fist, fit.placement.origin)) / fit.scale);
                    let (lo, hi) = shield.build().bounds().unwrap();
                    assert!(local[0] > lo[0] && local[0] < hi[0] && local[1] > lo[1] && local[1] < hi[1], "{folk:?}: fist at {local:?}");
                }
            }
        }
    }

    #[test]
    fn staves_and_spears_stand_upright_off_the_ground() {
        for folk in Folk::ALL {
            let body = body(folk);
            for item in [roll(Family::Staff, 4), roll(Family::Spear, 8), roll(Family::Spear, 2)] {
                let fit = fit(&body, Side::Right, &item);
                assert_eq!(fit.hold, Hold::Upright);
                let g = grip(&body, Side::Right, &fit);
                assert!(g.y()[1] > 0.985, "{folk:?} {}: channel {:?}", item.noun(), g.y());
                let (lo, _) = fit.mesh.bounds().unwrap();
                assert!(lo[1] > 0.0, "{folk:?} {}: the butt is {} below the ground", item.noun(), -lo[1]);
            }
        }
    }

    #[test]
    fn held_lights_burn_beside_the_body_clear_of_the_head() {
        use armory::families::Generator;
        use armory::families::lantern::Lantern;
        use armory::families::torch::{BurnState, Torch};
        let mut failures = vec![];
        for folk in Folk::ALL {
            for seed in [1u32, 8, 1207] {
                let spec = BodySpec::roll(folk, seed);
                for light_seed in [3u32, 4, 9] {
                    let mut torch = Torch::roll(light_seed);
                    torch.burn.state = BurnState::Burning;
                    let mut lantern = Lantern::roll(light_seed);
                    lantern.flame.lit = true;
                    let staff = roll(Family::Staff, seed);
                    for (light, flame) in [(Item::Torch(torch.clone()), Torch::light(&torch).unwrap().position), (Item::Lantern(lantern.clone()), Lantern::light(&lantern).unwrap().position)] {
                        for main in [None, Some(&staff)] {
                            let hands = fit_hands(&spec, main, Some(&light));
                            let fit = hands.off.as_ref().unwrap();
                            let at = fit.to_body(flame);
                            // The head's silhouette from the front (the doll and its portrait):
                            // the head and its features, ears and all.
                            let (lo, hi) = hands.body.head_mesh().bounds().unwrap();
                            let margin = 0.1;
                            if at[0] > lo[0] - margin && at[0] < hi[0] + margin && at[1] > lo[1] - margin && at[1] < hi[1] + margin {
                                failures.push(format!("{folk:?} {seed}: a {} burns across the head at {at:?} (head {lo:?}..{hi:?})", light.family().key()));
                            }
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn torches_stand_upright_and_lanterns_hang_plumb() {
        for folk in [Folk::Human, Folk::Gnome, Folk::Pigfolk] {
            let body = body(folk);
            let torch = roll(Family::Torch, 4);
            let fit_t = fit(&body, Side::Left, &torch);
            let g = grip(&body, Side::Left, &fit_t);
            assert!(g.y()[1] > 0.985, "{folk:?}: torch leans {:?}", g.y());
            assert!(centroid(&fit_t.mesh, &["head"])[1] > g.origin[1] + 1.0);

            let lantern = roll(Family::Lantern, 3);
            let Item::Lantern(state) = &lantern else { unreachable!() };
            let fit_l = fit(&body, Side::Left, &lantern);
            let posed = Body::posed(&body.spec, fit_l.pose);
            let g = posed.grip(Side::Left);
            assert!(fit_l.placement.y()[1] > 0.9999, "hangs plumb");
            assert!(vec3::distance(fit_l.to_body([0.0, state.bail.ring, 0.0]), g.origin) < 1e-3, "the fist closes on the ring");
            let wire = fit_l.grip_radius(Side::Left).unwrap();
            assert!(fit_l.mesh.bounds().unwrap().1[1] <= g.origin[1] + wire + 1e-3);
            // It hangs clear of the leg.
            let leg = posed.leg(Side::Left);
            for p in points(&fit_l.mesh, &["body", "frame", "panes", "glass", "holes"]) {
                let gap = (0..=40).map(|i| i as f32 / 40.0).map(|t| vec3::distance(p, leg.point_at(t)) - leg.radius_at(t)).fold(f32::MAX, f32::min);
                assert!(gap > 0.0, "{folk:?}: lantern inside the leg at {p:?}");
            }
        }
    }

    #[test]
    fn fingers_close_round_the_grip() {
        let human = body(Folk::Human);
        let fit = fit(&human, Side::Right, &roll(Family::Mace, 5));
        let r = fit.grip_radius(Side::Right).unwrap();
        assert!(r >= STRAP_GRIP * 0.99 && r <= human.grip_radius() + 1e-4);
        let posed = Body::posed(&human.spec, fit.pose);
        assert_eq!(posed.hand_pose(Side::Right), crate::HandPose::gripping(&human.hand_form(), r));
        assert_ne!(posed.hand_pose(Side::Right), crate::HandPose::RELAXED);
        assert_eq!(fit.pose.left, human.pose.left, "the other arm is left alone");
    }

    #[test]
    fn only_the_handle_thins() {
        let human = body(Folk::Human);
        let cap = human.grip_radius();
        // A thick-gripped sword: its blade is untouched.
        let sword = find(Family::Sword, |i| matches!(i, Item::Sword(s) if s.shaft.radius * s.grip.thickness > cap * 1.05 / (BLADE_SCALE * human.scale.sqrt())));
        let fit_s = fit(&human, Side::Right, &sword);
        assert_ne!(fit_s.item, sword, "the grip was thinned");
        let blade = |i: &Item| points(&i.build(), &["blade"]).collect::<Vec<_>>();
        assert_eq!(blade(&fit_s.item), blade(&sword));
        // A thick-hafted mace: its head keeps its size.
        let mace = find(Family::Mace, |i| matches!(i, Item::Mace(s) if s.shaft.radius * s.grip.thickness > cap * 1.05 && s.shaft.length < LONG_HAFT * human.height));
        let fit_m = fit(&human, Side::Right, &mace);
        assert_ne!(fit_m.item, mace);
        // The head's length, and its girth above the neck it flares from.
        let size = |i: &Item| {
            let head: Vec<V3> = points(&i.build(), &["head"]).collect();
            let length = span(head.iter().copied(), vec3::Y);
            let foot = head.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
            (span(head.iter().copied().filter(|p| p[1] > foot + length * 0.3), vec3::X), length)
        };
        let (before, after) = (size(&mace), size(&fit_m.item));
        assert!((before.0 - after.0).abs() < before.0 * 0.01 && (before.1 - after.1).abs() < before.1 * 0.01, "{before:?} {after:?}");
    }

    #[test]
    fn fitting_is_deterministic() {
        for folk in [Folk::Dwarf, Folk::Lizardfolk] {
            for family in [Family::Sword, Family::Club, Family::Bow, Family::Shield, Family::Lantern] {
                let item = roll(family, 77);
                assert_eq!(fit_held(&body(folk), Side::Right, &item), fit_held(&body(folk), Side::Right, &item));
            }
        }
    }

    #[test]
    fn worn_things_are_not_held() {
        assert!(fit_held(&body(Folk::Human), Side::Right, &roll(Family::Helm, 1)).is_none());
        assert!(fit_held(&body(Folk::Human), Side::Right, &roll(Family::Ring, 1)).is_none());
    }

    #[test]
    fn held_meshes_validate() {
        let body = body(Folk::Goblin);
        for family in [Family::Sword, Family::Axe, Family::Shield, Family::Bow, Family::Torch, Family::Lantern, Family::Crossbow, Family::Staff] {
            for side in Side::BOTH {
                for part in &fit(&body, side, &roll(family, 9)).mesh.parts {
                    part.validate().unwrap_or_else(|e| panic!("{family:?}: {e}"));
                }
            }
        }
    }

    #[test]
    fn both_hands_hold_at_once() {
        let spec = BodySpec::roll(Folk::Human, 1207);
        let (mace, lantern) = (roll(Family::Mace, 3), roll(Family::Lantern, 4));
        let hands = fit_hands(&spec, Some(&mace), Some(&lantern));
        let (main, off) = (hands.main.as_ref().unwrap(), hands.off.as_ref().unwrap());
        // The body is posed for both, each arm as its own fit set it.
        assert_eq!(hands.body.pose.arm(MAIN_HAND), main.pose.arm(MAIN_HAND));
        assert_eq!(hands.body.pose.arm(OFF_HAND), off.pose.arm(OFF_HAND));
        // Each item sits at its own fist on that body.
        for (side, fit) in [(MAIN_HAND, main), (OFF_HAND, off)] {
            let fist = hands.body.grip(side).origin;
            let item = centroid(&fit.mesh, &["grip", "wrap", "shaft", "ring"]);
            assert!(vec3::length(vec3::sub(item, fist)) < hands.body.height * 0.25, "{side:?}");
        }
        assert!(fit_hands(&spec, Some(&roll(Family::Helm, 1)), None).main.is_none());
        assert_eq!(fit_hands(&spec, None, None).body.pose, Pose::RELAXED);
    }
}
