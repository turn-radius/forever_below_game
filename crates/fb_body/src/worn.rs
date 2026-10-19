//! Worn gear on a body: [`fit_worn`] dresses a [`Body`] in Armory clothing, armour, boots,
//! gloves, headgear, amulets, belts and bags (the plate prototype's `buildGear`,
//! `garmentGeo`, `saGarment`, `saFeet`, `saHands`, `saHelm`, `saAmulet` and `saBack`;
//! `docs/body.md`, "Worn gear").
//!
//! Every item keeps its own Armory shape. Garments, trousers, hoods and hats are built by the
//! Armory's own builders on the body's forms ([`Body::torso_form`],
//! [`Body::lower_body_form`], [`Body::head_form`]), so one tunic fits a gnome and a pigfolk;
//! sleeves and pauldrons follow the body's arms; boots and gloves are built on its foot and
//! hand ([`Body::leg_form`], [`Body::hand_form`]) and replace the bare feet and hands; a helm
//! is resized to the head and its hair.
//!
//! **Layering.** Slots are dressed innermost first ([`LAYER_ORDER`]). After each item the
//! fitting measures how far its surface stands out past the bare trunk at each of the torso
//! form's sections, how far past each arm and each lower leg, and the next item is built on
//! the form inflated by that much (the plate keeps one running `maxOff` instead). Measuring
//! the built meshes rather than adding up nominal offsets is our own approach: it accounts
//! for drape, folds, belts and collars whatever they are, so an outer layer clears whatever
//! is under it.
//!
//! Departures from the plate, all our own:
//! - The plate dresses shirt, legs, armour, neck, cloak, belt, back. Trousers go on before
//!   the shirt here, so a shirt or tunic hangs over the waistband (the plate's own offsets put
//!   the trousers' seat inside the shirt too), and the belt goes on before the amulet and the
//!   cloak, so a cloak hangs over the belt rather than being strapped down by it; a hood's
//!   cape goes under the cloak.
//! - Trousers are tucked into boots whose shaft rises above the ankle.
//! - A cloak and a hood's cape hang clear of the arms, sleeves, gloves and pauldrons as well
//!   as the trunk (the plate widens its cloak past a fixed arm line).

use std::f32::consts::{PI, TAU};

use armory::build::boot::boot_parts;
use armory::build::form::{BACK, FRONT, HeadForm, LimbForm, TorsoForm, TorsoProfile};
use armory::build::garment::{Fitting, Notch, Underlay, garment_drape, garment_parts_fitted};
use armory::build::hand::hand_parts_fitted;
use armory::build::hat::hat_parts;
use armory::build::hood::hood_parts;
use armory::build::legwear::{LegDrape, LegHole, legwear_parts_holed};
use armory::build::parts::{Look, Parts};
use armory::build::strap::strap;
use armory::families::Generator;
use armory::families::amulet::{AmuletState, NecklaceStyle, necklace_parts, pendant_parts};
use armory::families::bags::{BagState, BagStyle, Bags};
use armory::families::belt::{BeltState, Belts};
use armory::families::feet::FeetState;
use armory::families::garment::GarmentState;
use armory::families::hands::HandsState;
use armory::families::hat::HatState;
use armory::families::helm::{Helm, HelmState, Openings};
use armory::families::hood::HoodState;
use armory::families::legs::LegsState;
use armory::geom::vec3::{self, Mat3, V3};
use armory::geom::CatmullRom;
use armory::sections::boot::BootStyle;
use armory::sections::garment::{GarmentStyle, Neckline, Sleeves};
use armory::sections::hand::HandStyle;
use armory::sections::legwear::LegwearStyle;
use armory::{Item, ItemMesh, MaterialKind, Part};
use fb_items::Slot;

use crate::body::Body;
use crate::folk::{Ears, Folk};
use crate::hair::{HairFit, REST_COLUMNS, REST_ROWS, Rest};
use crate::head::HeadFit;
use crate::limb::{Limb, Side};
use crate::place::Placement;

/// The order slots are dressed in, innermost first. Slots not listed (held gear, rings and
/// charms) are not worn on the body's surface and are skipped.
pub const LAYER_ORDER: [Slot; 10] = [Slot::Legs, Slot::Feet, Slot::Shirt, Slot::Body, Slot::Hands, Slot::Belt, Slot::Neck, Slot::Head, Slot::Cloak, Slot::Back];

/// Clearance kept between a layer and the one under it, over the outer layer's own ease.
const GAP: f32 = 0.04;
/// How far above the ankle a robe's hem hangs, in multiples of the wearer's scale: 1.5 cm on
/// a human, just clear of the top of the foot.
const ROBE_CLEARANCE: f32 = 0.15;
/// A forearm whose direction points further down than this (the downward share of its unit
/// direction) hangs by the body, under a cloak. Our own threshold: about 37 degrees off the
/// vertical, so an arm hanging relaxed counts and one bent forward to hold something does not.
const FOREARM_HANGS: f32 = 0.8;

/// One worn item fitted to the body, in body space.
#[derive(Clone, Debug, PartialEq)]
pub struct WornLayer {
    pub slot: Slot,
    pub mesh: ItemMesh,
}

/// Which bare skin the gear hides, so the body can leave it out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Covered {
    /// Gloves, gauntlets or mittens (not bracers): the bare hands and nails go.
    pub hands: bool,
    /// Footwear: the bare feet (and a lizard's claws) go.
    pub feet: bool,
    /// Trousers or leggings to the ankle: the bare legs go (wholly covered, they need not be
    /// drawn).
    pub legs: bool,
    /// Anything over the seat (legwear, a shirt, armour or a cloak) on a pig: its curled
    /// tail tucks in under it (our own; a lizard's tail comes out through vents and holes).
    pub tail: bool,
}

impl Covered {
    /// Whether the skin part `name` ([`Body::skin_mesh`]) is hidden.
    pub fn hides(&self, name: &str) -> bool {
        match name {
            "hands" | "nails" => self.hands,
            "feet" | "claws" => self.feet,
            "legs" => self.legs,
            "tail" => self.tail,
            _ => false,
        }
    }
}

/// Everything worn, fitted to one body.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WornFit {
    /// Each item, innermost first.
    pub layers: Vec<WornLayer>,
    /// How the hair and beard sit under it all.
    pub hair: HairFit,
    /// How the head's features sit under headgear.
    pub head: HeadFit,
    pub covered: Covered,
}

impl WornFit {
    /// Every worn part together.
    pub fn gear(&self) -> ItemMesh {
        ItemMesh { parts: self.layers.iter().flat_map(|l| l.mesh.parts.iter().cloned()).collect() }
    }

    /// The layer worn in `slot`, if any.
    pub fn layer(&self, slot: Slot) -> Option<&WornLayer> {
        self.layers.iter().find(|l| l.slot == slot)
    }
}

/// Fits `worn` (each item with the slot it is worn in) to `body`, innermost layer first
/// whatever order they are given in. Items in slots outside [`LAYER_ORDER`] are skipped, as
/// are items whose family has no place on the body (a sword in the shirt slot).
pub fn fit_worn(body: &Body, worn: &[(Slot, Item)]) -> WornFit {
    let mut fit = WornFit::default();
    let mut env = Envelope::new(body);
    let tucked = worn.iter().any(|(slot, item)| *slot == Slot::Feet && matches!(item, Item::Feet(s) if has_shaft(s)));
    let hooded = worn.iter().any(|(slot, item)| *slot == Slot::Head && matches!(item, Item::Hood(_)));
    let robed = worn.iter().any(|(slot, item)| matches!(slot, Slot::Shirt | Slot::Body) && matches!(item, Item::Garment(g) if g.cut.style == GarmentStyle::Robe));
    // Ears fold back under a hood, and a pig's flop ears under anything; standing ears come
    // out through openings cut in a helm; a frill folds flat under anything on the head.
    fit.head = match worn.iter().find(|(slot, _)| *slot == Slot::Head).map(|(_, item)| item) {
        Some(Item::Hood(_)) => HeadFit { fold_ears: true, fold_frill: true },
        Some(Item::Hat(_) | Item::Helm(_)) => HeadFit { fold_ears: body.race().ears == Ears::Flop, fold_frill: true },
        _ => HeadFit::default(),
    };
    let mut hair = None;
    for slot in LAYER_ORDER {
        // Long hair hangs over what is on the shoulders: under a hood and its cape, else over
        // everything but a pack.
        if hair.is_none() && slot == if hooded { Slot::Head } else { Slot::Back } {
            hair = Some((env.shoulder_clearance(), env.chest_clearance(), env.rest()));
        }
        for (_, item) in worn.iter().filter(|(s, _)| *s == slot) {
            let mesh = match (slot, item) {
                (Slot::Legs, Item::Legs(s)) => {
                    fit.covered.legs |= s.legwear.style != LegwearStyle::Breeches;
                    legwear(body, s, tucked)
                }
                (Slot::Feet, Item::Feet(s)) => {
                    fit.covered.feet = true;
                    boots(body, &env, s)
                }
                (Slot::Shirt | Slot::Body | Slot::Cloak, Item::Garment(s)) => garment(body, &env, s, &Cover::of(worn, slot)),
                (Slot::Hands, Item::Hands(s)) => {
                    fit.covered.hands |= s.hand.style != HandStyle::Bracer;
                    gloves(body, &env, s)
                }
                (Slot::Belt, Item::Belt(s)) => belt(body, &env, s, robed),
                (Slot::Neck, Item::Amulet(s)) => amulet(body, &env, s),
                (Slot::Head, Item::Helm(s)) => helm(body, s, fit.head),
                (Slot::Head, Item::Hood(s)) => hood(body, &env, s, fit.head),
                (Slot::Head, Item::Hat(s)) => hat(body, s, fit.head),
                (Slot::Back, Item::Bags(s)) => bag(body, &env, s),
                _ => continue,
            };
            if slot == Slot::Head {
                fit.hair.covered = true;
            }
            fit.covered.tail |= body.folk() == Folk::Pigfolk && matches!(slot, Slot::Legs | Slot::Shirt | Slot::Body | Slot::Cloak);
            env.absorb(slot, &mesh);
            fit.layers.push(WornLayer { slot, mesh });
        }
    }
    let rest = |env: &Envelope| (env.shoulder_clearance(), env.chest_clearance(), env.rest());
    let (clearance, chest, surface) = hair.unwrap_or_else(|| rest(&env));
    (fit.hair.clearance, fit.hair.chest, fit.hair.rest) = (clearance, chest, Some(surface));
    fit
}

impl Body {
    /// The whole figure dressed in `worn`: skin (less what the gear hides), head, hair fitted
    /// under the gear, and the gear.
    pub fn dressed(&self, worn: &[(Slot, Item)]) -> ItemMesh {
        let fit = fit_worn(self, worn);
        let mut mesh = self.under(&fit);
        mesh.parts.extend(fit.gear().parts);
        mesh
    }

    /// The body as it shows under `fit`'s gear: skin less what the gear hides, the head with
    /// its features folded under headgear, and the hair fitted under it all.
    pub fn under(&self, fit: &WornFit) -> ItemMesh {
        let mut mesh = self.skin_mesh();
        mesh.parts.retain(|p| !fit.covered.hides(p.name));
        mesh.parts.extend(self.head_mesh_fitted(fit.head).parts);
        mesh.parts.extend(self.hair_mesh(fit.hair).parts);
        mesh
    }
}

// ───────────────────────── The worn surface ─────────────────────────

/// Straight ahead (+Z) as a [`Relief`] direction, `atan2(z, x)`.
const FRONT_PHI: f32 = PI / 2.0;
/// How far either side of straight ahead the front of the chest reaches, radians (45
/// degrees), for [`Envelope::chest_clearance`]. Our own choice: about the width a beard hangs
/// over.
const CHEST_SPREAD: f32 = PI / 4.0;

/// How many of a [`Rest`]'s columns (1.4 degrees each) either side an edge holds hair off
/// over, and how far it must stand above the surface's own run from there to count. Our own
/// figures, tuned against the hair-on-a-cloak test: a little over the spacing of long hair's
/// strands (about 6 degrees), and a little over a cloth's thickness.
const EDGE_COLUMNS: isize = 6;
const EDGE_RISE: f32 = 0.05;

/// Bins along a limb for its clearance.
const LIMB_BINS: usize = 24;
/// Nothing measured yet.
const NONE: f32 = f32::NEG_INFINITY;

/// How far everything worn so far stands out past the bare body (torso space for the trunk:
/// the waist at y = 0).
struct Envelope<'a> {
    body: &'a Body,
    /// The bare trunk's garment form, and how far it reaches toward each direction.
    form: TorsoForm,
    reach: Reach,
    /// How far worn surfaces stand out past the bare trunk: the trunk's own layers, and
    /// those together with the arms (and anything on them) for a cloak hanging over both.
    trunk: Relief,
    over_arms: Relief,
    /// How far worn surfaces stand out past each arm's and each leg's skin, along it.
    arms: [[f32; LIMB_BINS]; 2],
    legs: [[f32; LIMB_BINS]; 2],
    /// How far the bare arms and hands and everything worn reach from the trunk's axis, finely
    /// gridded: what a cloak hangs over.
    outline: Outline,
    /// The points of everything worn on each arm (right then left), body space: what a cuff
    /// is drawn over.
    on_arm: [Vec<V3>; 2],
    /// The surface of everything worn but what is on the head, and of the bare arms under a
    /// cloak, as triangles (torso space): what long hair and a beard rest on. An aventail or
    /// a hood's cape hangs over them, not under.
    under_head: Vec<[V3; 3]>,
}

/// Which worn surface a trunk form is eased over ([`Envelope::form`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Over {
    /// What is worn on the trunk.
    Trunk,
    /// The trunk and the arms with whatever is on them, its shoulder section moved up to the
    /// top of the arms (a hood's cape).
    Arms,
    /// The bare trunk, its shoulder section moved up to the top of the arms, for a cloak that
    /// hangs over everything worn, and the arms, through an underlay ([`Envelope::outline`]).
    Shoulders,
}

impl<'a> Envelope<'a> {
    fn new(body: &'a Body) -> Envelope<'a> {
        let form = body.torso_form();
        let s = body.scale;
        let profile = form.profile();
        let relief = || Relief::new(profile.bottom(), profile.top() + 0.7, 0.1 * s.max(0.5));
        let mut env = Envelope {
            body,
            form,
            reach: Reach::of(&profile),
            trunk: relief(),
            over_arms: relief(),
            arms: [[NONE; LIMB_BINS]; 2],
            legs: [[NONE; LIMB_BINS]; 2],
            outline: Outline::new(profile.bottom() - 1.0, profile.top() + 0.7),
            on_arm: [vec![], vec![]],
            under_head: vec![],
        };
        // The bare arms and hands as drawn, for a cloak over them: a raised forearm and its
        // hand come out through the cloak instead ([`Envelope::under_cloak`]).
        for part in body.skin_mesh().parts.iter().filter(|p| matches!(p.name, "arms" | "hands")) {
            let under: Vec<bool> = part.positions.iter().map(|&p| {
                let (side, t, _) = env.nearest_arm(p);
                env.under_cloak(side, t)
            }).collect();
            for (&p, _) in part.positions.iter().zip(&under).filter(|(_, u)| **u) {
                let q = env.torso(p);
                env.outline.add(q);
            }
            for t in part.indices.chunks(3).filter(|t| t.iter().all(|&i| under[i as usize])) {
                env.under_head.push([t[0], t[1], t[2]].map(|i| env.torso(part.positions[i as usize])));
            }
        }
        // The bare arms, for whatever hangs over them.
        for side in Side::BOTH {
            let arm = body.arm(side);
            let n = arm.path.len();
            for (i, &c) in arm.path.iter().enumerate() {
                let t = i as f32 / (n - 1) as f32;
                if !env.under_cloak(side, t) {
                    continue;
                }
                let r = arm.radius_at(t);
                let along = arm.tangent_at(t);
                let a = vec3::normalize_or(vec3::cross(along, vec3::Z), vec3::X);
                let b = vec3::cross(along, a);
                for k in 0..12 {
                    let th = TAU * k as f32 / 12.0;
                    let p = vec3::add(c, vec3::add(vec3::scale(a, r * th.cos()), vec3::scale(b, r * th.sin())));
                    let q = env.torso(p);
                    let e = env.trunk_excess(q);
                    if e > 0.0 {
                        env.over_arms.add(q, e);
                    }
                }
            }
        }
        env
    }

    /// Whether the point `t` along `side`'s arm lies under a cloak hung over the arms: the
    /// upper arm always, the forearm and hand only while the forearm hangs down by the body. A
    /// forearm raised to hold a staff or a torch comes out through the cloak's front, as an arm
    /// comes out of a real cloak. Our own rule: measured over a raised forearm, a cloak stood
    /// off the shoulders like a shelf.
    fn under_cloak(&self, side: Side, t: f32) -> bool {
        let arm = self.body.arm(side);
        let forearm = vec3::normalize_or(vec3::sub(arm.joints[2], arm.joints[1]), [0.0, -1.0, 0.0]);
        t <= arm.t_joints[1] || forearm[1] < -FOREARM_HANGS
    }

    /// A body-space point in torso space.
    fn torso(&self, p: V3) -> V3 {
        [p[0], p[1] - self.body.waist_y, p[2]]
    }

    /// How far torso-space `q` stands out past the bare trunk, along the horizontal ray from
    /// its axis.
    fn trunk_excess(&self, q: V3) -> f32 {
        q[0].hypot(q[2]) - self.reach.at(q[2].atan2(q[0]), q[1])
    }

    /// The arm `p` stands nearest the skin of, the share along it, and how far off its skin,
    /// from the shoulder out (the root runs inside the trunk). Past the wrist the hand counts as part of the arm, as a stub the hand's length and
    /// about its half width (our own approximation, for sorting points only).
    fn nearest_arm(&self, p: V3) -> (Side, f32, f32) {
        let hand = self.body.hand_form();
        let reach = hand.palm_length + hand.fingers[1].length;
        Side::BOTH
            .into_iter()
            .map(|side| {
                let arm = self.body.arm(side);
                // From near the shoulder out: the root runs inside the trunk.
                let (t, d) = nearest_from(arm, 0.7 * arm.t_joints[0], p);
                let along = vec3::dot(vec3::sub(p, arm.joints[2]), arm.tangent_at(1.0));
                if t > 0.995 && along > 0.0 {
                    let on = vec3::add(arm.joints[2], vec3::scale(arm.tangent_at(1.0), along.min(reach)));
                    return (side, 1.0, vec3::distance(p, on) - hand.knuckle_half_width);
                }
                (side, t, d - arm.radius_at(t))
            })
            .fold((Side::Right, 0.0, f32::MAX), |best, x| if x.2 < best.2 { x } else { best })
    }

    /// Whether a worn part is on the arms (a sleeve, a cuff, a pauldron, a glove) rather than
    /// the trunk: most of its points lie close on an arm or hand. Parts are judged whole, so
    /// a skirt hanging by the hands stays on the trunk.
    fn on_arms(&self, part: &Part) -> bool {
        // Pauldrons are on the arms whatever they stand over: over a robe's belled sleeves
        // they stood too far off the arm to be judged so, and counted as the trunk's.
        if part.name == "pauldrons" {
            return true;
        }
        let near = part
            .positions
            .iter()
            .filter(|&&p| {
                let (side, t, da) = self.nearest_arm(p);
                t >= 0.85 * self.body.arm(side).t_joints[0] && da < 0.8 * self.body.scale
            })
            .count();
        near * 10 > part.positions.len() * 7
    }

    /// Takes a layer's surface into the envelope. Footwear is under everything that could
    /// cover it already, gloves are all on the arms, and of a belt only the strap round the
    /// waist counts: what hangs from it (a pouch, sword frogs, rings) bulges out on one side,
    /// which the forms' sections cannot follow.
    fn absorb(&mut self, slot: Slot, mesh: &ItemMesh) {
        if slot == Slot::Feet {
            return;
        }
        let body = self.body;
        let top = self.form.neck.y + 0.7; // up to the top of a high collar
        let crotch = body.hip_y - 0.14 * body.trunk_length;
        // Everything counts toward what a cloak hangs over, a belt's pouch included, but for
        // what is on a raised forearm.
        for part in &mesh.parts {
            let on_arms = slot == Slot::Hands || self.on_arms(part);
            for &p in &part.positions {
                if on_arms {
                    let (side, t, _) = self.nearest_arm(p);
                    if !self.under_cloak(side, t) {
                        continue;
                    }
                }
                let q = self.torso(p);
                self.outline.add(q);
            }
            // What hair rests on, as the triangles themselves.
            if slot != Slot::Head {
                for t in part.indices.chunks(3) {
                    let tri = [t[0], t[1], t[2]].map(|i| part.positions[i as usize]);
                    if on_arms && tri.iter().any(|p| {
                        let (side, t, _) = self.nearest_arm(*p);
                        !self.under_cloak(side, t)
                    }) {
                        continue;
                    }
                    self.under_head.push(tri.map(|p| self.torso(p)));
                }
            }
        }
        for part in mesh.parts.iter().filter(|p| measured(slot, p)) {
            let on_arms = slot == Slot::Hands || self.on_arms(part);
            for &p in &part.positions {
                let q = self.torso(p);
                if q[1] > top {
                    continue;
                }
                let e = self.trunk_excess(q).max(0.0);
                if !on_arms {
                    self.over_arms.add(q, e);
                }
                if on_arms {
                    let (side, t, da) = self.nearest_arm(p);
                    if self.under_cloak(side, t) {
                        self.over_arms.add(q, e);
                    }
                    // Past the wrist is the hand, not the arm.
                    let limb = body.arm(side);
                    let beyond = t > 0.995 && vec3::dot(vec3::sub(p, limb.joints[2]), limb.tangent_at(1.0)) > 0.0;
                    if !beyond {
                        let bin = &mut self.arms[index(side)][limb_bin(t)];
                        *bin = bin.max(da);
                        self.on_arm[index(side)].push(p);
                    }
                } else {
                    self.trunk.add(q, e);
                }
                if p[1] < crotch {
                    let (side, t, dl) = Side::BOTH
                        .into_iter()
                        .map(|side| {
                            let leg = body.leg(side);
                            let (t, d) = nearest(leg, p);
                            (side, t, d - leg.radius_at(t))
                        })
                        .fold((Side::Right, 0.0, f32::MAX), |best, x| if x.2 < best.2 { x } else { best });
                    if dl < 0.6 * body.scale {
                        let bin = &mut self.legs[index(side)][limb_bin(t)];
                        *bin = bin.max(dl);
                    }
                }
            }
        }
    }

    /// The trunk's form eased over everything worn so far (and over the arms too when
    /// `over_arms`), `GAP` clear of it once a garment eases it by `fit`
    /// ([`TorsoForm::eased`]), in torso space. A loose garment over a close one needs little
    /// more than its own fit.
    ///
    /// The form has nine sections, the worn surface any shape, so each section is widened
    /// and deepened until the form's surface clears every measured point: each pass widens
    /// the two sections either side of a point still inside, shared by how near each is
    /// (the least change that lifts a linear blend of the two by the shortfall), toward the
    /// sides or the front and back as the point faces, by half the shortfall a pass (the
    /// sections lie on a spline, so a full step overshoots and the form balloons). Our own
    /// fitting.
    fn form(&self, over: Over, fit: f32) -> TorsoForm {
        let relief = match over {
            Over::Trunk => Some(&self.trunk),
            Over::Arms => Some(&self.over_arms),
            Over::Shoulders => None,
        };
        let mut base = self.form.rings();
        if over != Over::Trunk {
            // Over the arms the shoulder line is the top of the arms (and whatever is on
            // them), not the trunk's slope to the neck: move the section there, so a cloak
            // rests on the shoulders and still closes round the neck.
            let s = self.body.scale;
            let neck = base[8].y;
            let top = self
                .over_arms
                .cells()
                .filter(|&(y, phi, e)| phi.cos().abs() > 0.7 && e > 0.25 * s && y < neck - 0.25 * s)
                .fold(base[7].y, |m, c| m.max(c.0));
            base[7].y = top.min(neck - 0.25 * s);
        }
        let ring_y = base.map(|r| r.y);
        let mut grow = [[0.0f32; 2]; 9];
        let inflate = |grow: &[[f32; 2]; 9]| {
            let mut rings = base;
            for (ring, [across, deep]) in rings.iter_mut().zip(grow) {
                *ring = ring.inflated(*across, *deep);
            }
            self.form.with_rings(rings)
        };
        for _ in 0..48 {
            let reach = Reach::of(&inflate(&grow).eased(fit).profile());
            let mut need = [[0.0f32; 2]; 9];
            for (y, phi, e) in relief.into_iter().flat_map(Relief::cells) {
                let short = self.reach.at(phi, y) + e.max(0.0) + GAP - reach.at(phi, y);
                if short <= 0.002 {
                    continue;
                }
                let k = ring_y.partition_point(|ry| *ry <= y).clamp(1, 8) - 1;
                let f = ((y - ring_y[k]) / (ring_y[k + 1] - ring_y[k])).clamp(0.0, 1.0);
                // Widening lifts the surface toward polar angle phi by about cos^2 of it,
                // deepening by sin^2; share the lift between them by the square of each, so a
                // point at the side widens and hardly deepens.
                let (c, s) = (phi.cos().powi(2), phi.sin().powi(2));
                let (wa, wd) = (c * c / (c * c + s * s), s * s / (c * c + s * s));
                let lift = wa * c + wd * s;
                let (w0, w1) = (1.0 - f, f);
                let norm = (w0 * w0 + w1 * w1) * lift;
                for (ring, w) in [(k, w0), (k + 1, w1)] {
                    need[ring][0] = need[ring][0].max(short * w * wa / norm);
                    need[ring][1] = need[ring][1].max(short * w * wd / norm);
                }
            }
            if need.iter().flatten().all(|v| *v <= 0.0) {
                break;
            }
            for (g, n) in grow.iter_mut().zip(need) {
                g[0] += 0.5 * n[0];
                g[1] += 0.5 * n[1];
            }
        }
        inflate(&grow)
    }

    /// Whether anything is worn on the trunk yet.
    fn trunk_worn(&self) -> bool {
        self.trunk.cells().next().is_some()
    }

    /// The arms as sleeves see them (torso space, right then left): the skin, eased over any
    /// sleeves already worn.
    fn arm_forms(&self) -> [LimbForm; 2] {
        let w = self.body.waist_y;
        [Side::Right, Side::Left].map(|side| {
            let arm = self.body.arm(side);
            let bins = &self.arms[index(side)];
            let n = arm.path.len();
            let radii = (0..n)
                .map(|i| {
                    let t = i as f32 / (n - 1) as f32;
                    let worn = bin_around(bins, t);
                    arm.radius_at(t) + if worn > NONE { worn.max(0.0) + GAP } else { 0.0 }
                })
                .collect();
            LimbForm { path: arm.path.iter().map(|p| [p[0], p[1] - w, p[2]]).collect(), radii, t_joints: arm.t_joints }
        })
    }

    /// How far worn surfaces stand off limb `bins` between shares `t0` and `t1` of it, if any.
    fn limb_clearance(bins: &[f32; LIMB_BINS], t0: f32, t1: f32) -> Option<f32> {
        let v = (limb_bin(t0)..=limb_bin(t1)).map(|i| bins[i]).fold(NONE, f32::max);
        (v > NONE).then(|| v.max(0.0) + GAP)
    }

    /// How far the worn surface reaches from the axis toward polar angle `phi` at height
    /// `y` (torso space): the bare trunk plus the most anything worn stands out there and in
    /// the cells about it. Where the fitted forms are sections all round, this follows
    /// what is actually worn at each side, for what rests on it (a pack and its straps).
    fn worn_reach(&self, phi: f32, y: f32) -> f32 {
        self.reach.at(phi, y) + self.over_arms.around(phi, y).max(0.0)
    }

    /// `q` (torso space) moved along the horizontal ray from the axis onto the worn
    /// surface, `off` clear of it. The plate's `pushOut` only pushes out; a strap lies on
    /// what it crosses, so here it is drawn in as well.
    fn lay_on(&self, q: V3, off: f32) -> V3 {
        let r = q[0].hypot(q[2]);
        if r < 1e-6 {
            return q;
        }
        let target = self.worn_reach(q[2].atan2(q[0]), q[1]) + off;
        [q[0] * target / r, q[1], q[2] * target / r]
    }

    /// How far long hair must hang outside the skin: what stands out over the chest and
    /// shoulders.
    fn shoulder_clearance(&self) -> f32 {
        let chest = self.form.chest.y;
        self.trunk.cells().filter(|c| c.0 >= chest).fold(0.0f32, |m, c| m.max(c.2))
    }

    /// The outermost surface worn so far over the chest, shoulders and neck, or the bare
    /// trunk where nothing is: what long hair and a beard rest on ([`HairFit::rest`]).
    fn rest(&self) -> Rest {
        let b = self.body;
        let w = b.waist_y;
        let (y0, y1) = (b.chest_y - 0.9 * b.trunk_length, b.shoulder_y + 0.35 * b.trunk_length);
        // The worn surface drawn into Rest's own grid of directions and heights: each cell
        // takes the farthest any triangle reaches across it (the triangle's corners in
        // cylindrical coordinates, its reach between them by barycentric weights), and then
        // the most of the cells about it, so the grid never falls inside a surface that bulges
        // between its nodes. Measured on the surface itself; binning its points by angle and
        // taking the most within several degrees held long hair up to 2 cm off a cloak where
        // it curves round the shoulders.
        let (rows, cols) = (REST_ROWS, REST_COLUMNS);
        let dy = (y1 - y0) / (rows - 1) as f32;
        let dphi = TAU / cols as f32;
        let mut grid = vec![NONE; rows * cols];
        for tri in &self.under_head {
            let c: Vec<(f32, f32, f32)> = tri.iter().map(|p| (p[2].atan2(p[0]), p[1] + w, p[0].hypot(p[2]))).collect();
            // Unwrapped round the first corner, so a triangle across the back seam stays whole.
            let unwrap = |a: f32| c[0].0 + (a - c[0].0 + PI).rem_euclid(TAU) - PI;
            let c: Vec<(f32, f32, f32)> = c.iter().map(|&(a, y, r)| (unwrap(a), y, r)).collect();
            let (a0, a1) = c.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.0), hi.max(p.0)));
            let (h0, h1) = c.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
            if h1 < y0 - dy || h0 > y1 + dy || a1 - a0 > PI {
                continue;
            }
            let det = (c[1].1 - c[2].1) * (c[0].0 - c[2].0) + (c[2].0 - c[1].0) * (c[0].1 - c[2].1);
            let (i0, i1) = (((h0 - y0) / dy).ceil().max(0.0) as isize, (((h1 - y0) / dy).floor() as isize).min(rows as isize - 1));
            let (j0, j1) = (((a0 + PI) / dphi).ceil() as isize, ((a1 + PI) / dphi).floor() as isize);
            let mut put = |i: usize, j: isize, r: f32| {
                let cell = &mut grid[i * cols + j.rem_euclid(cols as isize) as usize];
                *cell = cell.max(r);
            };
            // Its edges, sampled finer than the cells, so a sheet's thin rim (the edge of a
            // cloak, standing at the nape or down its front) is never missed.
            for k in 0..3 {
                let (p, q) = (c[k], c[(k + 1) % 3]);
                let n = (((q.0 - p.0).abs() / dphi).max((q.1 - p.1).abs() / dy) * 2.0).ceil().max(1.0) as usize;
                for m in 0..=n {
                    let t = m as f32 / n as f32;
                    let (a, y, r) = (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t, p.2 + (q.2 - p.2) * t);
                    let i = ((y - y0) / dy).round();
                    if (0.0..rows as f32).contains(&i) {
                        put(i as usize, ((a + PI) / dphi).round() as isize, r);
                    }
                }
            }
            if det.abs() < 1e-9 || i0 > i1 || j0 > j1 {
                continue;
            }
            for i in i0..=i1 {
                let y = y0 + dy * i as f32;
                for j in j0..=j1 {
                    let a = -PI + dphi * j as f32;
                    let l0 = ((c[1].1 - c[2].1) * (a - c[2].0) + (c[2].0 - c[1].0) * (y - c[2].1)) / det;
                    let l1 = ((c[2].1 - c[0].1) * (a - c[2].0) + (c[0].0 - c[2].0) * (y - c[2].1)) / det;
                    let l2 = 1.0 - l0 - l1;
                    if l0 >= -1e-4 && l1 >= -1e-4 && l2 >= -1e-4 {
                        put(i as usize, j, l0 * c[0].2 + l1 * c[1].2 + l2 * c[2].2);
                    }
                }
            }
        }
        let cell = |i: isize, j: isize| grid[i.clamp(0, rows as isize - 1) as usize * cols + j.rem_euclid(cols as isize) as usize];
        Rest::new(y0, y1, |phi, y| {
            let (i, j) = (((y - y0) / dy).round() as isize, ((phi + PI) / dphi).round() as isize);
            let column = |k: isize| (-1..=1).map(|di| cell(i + di, j + k)).fold(NONE, f32::max);
            let own = column(0);
            let mut worn = (-1..=1).map(column).fold(NONE, f32::max);
            // A few columns further, only an edge standing clear of the surface's own run from
            // here (a cloak's front edge over the tunic beside it), not a surface that merely
            // curves away, so hair beside the edge does not dip under it.
            for dir in [-1, 1] {
                let step = if own > NONE && column(dir) > NONE { column(dir) - own } else { 0.0 };
                for k in 2..=EDGE_COLUMNS {
                    let c = column(dir * k);
                    if c > own.max(0.0) + step * k as f32 + EDGE_RISE {
                        worn = worn.max(c);
                    }
                }
            }
            worn.max(b.trunk_reach([phi.cos(), phi.sin()], y))
        })
    }

    /// The same over the front of the chest only, within [`CHEST_SPREAD`] of straight ahead:
    /// what a beard rests on when nothing better is known.
    fn chest_clearance(&self) -> f32 {
        let chest = self.form.chest.y;
        let ahead = |phi: f32| (phi - FRONT_PHI + PI).rem_euclid(TAU) - PI;
        self.trunk.cells().filter(|c| c.0 >= chest && ahead(c.1).abs() <= CHEST_SPREAD).fold(0.0f32, |m, c| m.max(c.2))
    }
}

/// How far points reach from the trunk's axis, gridded finely by height and by angle round
/// the axis (torso space, lathe convention, as the Armory's forms take angles): the most
/// any point in each cell reaches.
struct Outline {
    y0: f32,
    rows: Vec<[f32; OUTLINE_COLUMNS]>,
}

/// The garment drape's own column count, so its columns read cells directly.
const OUTLINE_COLUMNS: usize = 128;
const OUTLINE_STEP: f32 = 0.05;

impl Outline {
    fn new(y0: f32, y1: f32) -> Outline {
        Outline { y0, rows: vec![[NONE; OUTLINE_COLUMNS]; ((y1 - y0) / OUTLINE_STEP).ceil() as usize + 1] }
    }

    fn column(angle: f32) -> usize {
        (angle.rem_euclid(TAU) / TAU * OUTLINE_COLUMNS as f32).round() as usize % OUTLINE_COLUMNS
    }

    /// Records torso-space `q`.
    fn add(&mut self, q: V3) {
        let i = ((q[1] - self.y0) / OUTLINE_STEP).round();
        if i < 0.0 || i >= self.rows.len() as f32 {
            return;
        }
        let j = Self::column((-q[2]).atan2(q[0]));
        let cell = &mut self.rows[i as usize][j];
        *cell = cell.max(q[0].hypot(q[2]));
    }

    /// How far anything reaches toward `angle` at height `y`, taking the rows either side
    /// too so sparse sampling leaves no gaps ([`NONE`] where nothing is).
    fn at(&self, angle: f32, y: f32) -> f32 {
        let i = ((y - self.y0) / OUTLINE_STEP).round() as isize;
        let j = Self::column(angle);
        (i - 2..=i + 2).filter_map(|k| self.rows.get(usize::try_from(k).ok()?)).map(|row| row[j]).fold(NONE, f32::max)
    }
}

/// How far worn surfaces stand out past the bare trunk, gridded by height and by direction
/// round the axis (torso space).
struct Relief {
    y0: f32,
    dy: f32,
    rows: Vec<[f32; SECTORS]>,
}

const SECTORS: usize = 36;

impl Relief {
    fn new(y0: f32, y1: f32, dy: f32) -> Relief {
        Relief { y0, dy, rows: vec![[NONE; SECTORS]; ((y1 - y0) / dy).ceil() as usize + 1] }
    }

    /// Records a point `q` standing `e` past the trunk.
    fn add(&mut self, q: V3, e: f32) {
        let i = ((q[1] - self.y0) / self.dy).floor().clamp(0.0, (self.rows.len() - 1) as f32) as usize;
        let j = (((q[2].atan2(q[0]) + PI) / TAU * SECTORS as f32) as usize).min(SECTORS - 1);
        self.rows[i][j] = self.rows[i][j].max(e);
    }

    /// The most anything stands out in the cell at `phi` and `y` and the cells next to it.
    fn around(&self, phi: f32, y: f32) -> f32 {
        let i = ((y - self.y0) / self.dy).floor() as isize;
        let j = ((phi + PI) / TAU * SECTORS as f32).floor() as isize;
        let mut m = NONE;
        for di in -1..=1 {
            let Some(row) = self.rows.get((i + di).clamp(0, self.rows.len() as isize - 1) as usize) else { continue };
            for dj in -1..=1 {
                m = m.max(row[(j + dj).rem_euclid(SECTORS as isize) as usize]);
            }
        }
        m
    }

    /// Every cell with something in it: its height, the polar angle of its middle, and the
    /// excess. A cell is checked at its top and bottom so a sloping form clears it whole.
    fn cells(&self) -> impl Iterator<Item = (f32, f32, f32)> + '_ {
        self.rows.iter().enumerate().flat_map(move |(i, row)| {
            row.iter().enumerate().filter(|(_, e)| **e > NONE).flat_map(move |(j, &e)| {
                let phi = -PI + TAU * (j as f32 + 0.5) / SECTORS as f32;
                let y = self.y0 + self.dy * i as f32;
                [(y, phi, e), (y + self.dy, phi, e)]
            })
        })
    }
}

/// Whether a worn part counts toward the worn surface: of a belt only the strap round the
/// waist does, not what hangs from it.
fn measured(slot: Slot, part: &Part) -> bool {
    slot != Slot::Belt || matches!(part.name, "belt" | "buckle" | "keeper" | "tail" | "tip" | "studs")
}

fn index(side: Side) -> usize {
    match side {
        Side::Right => 0,
        Side::Left => 1,
    }
}

fn limb_bin(t: f32) -> usize {
    ((t.clamp(0.0, 1.0) * LIMB_BINS as f32) as usize).min(LIMB_BINS - 1)
}

/// The largest of the bin at `t` and its neighbours.
fn bin_around(bins: &[f32; LIMB_BINS], t: f32) -> f32 {
    let i = limb_bin(t);
    bins[i.saturating_sub(1)..=(i + 1).min(LIMB_BINS - 1)].iter().copied().fold(NONE, f32::max)
}

/// The share along `limb` nearest `p`, and the distance to it.
fn nearest(limb: &Limb, p: V3) -> (f32, f32) {
    nearest_from(limb, 0.0, p)
}

/// [`nearest`] among the shares from `t0` on.
fn nearest_from(limb: &Limb, t0: f32, p: V3) -> (f32, f32) {
    let n = limb.path.len();
    let first = ((t0 * (n - 1) as f32) as usize).min(n - 1);
    let (i, d) = limb.path.iter().enumerate().skip(first).map(|(i, c)| (i, vec3::distance(*c, p))).fold((first, f32::MAX), |best, x| if x.1 < best.1 { x } else { best });
    (i as f32 / (n - 1).max(1) as f32, d)
}

/// How far a trunk form reaches from its axis toward each horizontal direction, by height:
/// the section's radius tabulated by polar angle (`atan2(z, x)`) for quick lookups.
struct Reach {
    y0: f32,
    dy: f32,
    rows: Vec<Vec<f32>>,
}

const REACH_COLUMNS: usize = 128;

impl Reach {
    fn of(profile: &TorsoProfile) -> Reach {
        let (y0, y1) = (profile.bottom(), profile.top());
        let n = 96;
        let dy = (y1 - y0) / (n - 1) as f32;
        let rows = (0..n)
            .map(|i| {
                let y = y0 + dy * i as f32;
                let mut samples: Vec<(f32, f32)> = (0..256)
                    .map(|k| {
                        let [x, z] = profile.section(TAU * k as f32 / 256.0, y, 0.0);
                        (z.atan2(x), x.hypot(z))
                    })
                    .collect();
                samples.sort_by(|a, b| a.0.total_cmp(&b.0));
                (0..REACH_COLUMNS)
                    .map(|j| {
                        let phi = -PI + TAU * j as f32 / REACH_COLUMNS as f32;
                        let k = samples.partition_point(|s| s.0 < phi);
                        let (a, b) = (samples[(k + samples.len() - 1) % samples.len()], samples[k % samples.len()]);
                        let span = (b.0 - a.0).rem_euclid(TAU).max(1e-6);
                        let f = ((phi - a.0).rem_euclid(TAU) / span).clamp(0.0, 1.0);
                        a.1 + (b.1 - a.1) * f
                    })
                    .collect()
            })
            .collect();
        Reach { y0, dy, rows }
    }

    /// The reach toward polar angle `phi` at height `y` (clamped to the form).
    fn at(&self, phi: f32, y: f32) -> f32 {
        let fy = ((y - self.y0) / self.dy).clamp(0.0, (self.rows.len() - 1) as f32);
        let (i0, ty) = (fy.floor() as usize, fy.fract());
        let i1 = (i0 + 1).min(self.rows.len() - 1);
        let fx = (phi + PI).rem_euclid(TAU) / TAU * REACH_COLUMNS as f32;
        let (j0, tx) = (fx.floor() as usize % REACH_COLUMNS, fx.fract());
        let j1 = (j0 + 1) % REACH_COLUMNS;
        let row = |i: usize| self.rows[i][j0] * (1.0 - tx) + self.rows[i][j1] * tx;
        row(i0) * (1.0 - ty) + row(i1) * ty
    }

    /// The form's outward normal toward polar angle `phi` at height `y`.
    fn normal(&self, phi: f32, y: f32) -> V3 {
        let h = 0.05;
        let slope = (self.at(phi, y + h) - self.at(phi, y - h)) / (2.0 * h);
        vec3::normalize_or([phi.cos(), -slope, phi.sin()], [phi.cos(), 0.0, phi.sin()])
    }
}

// ───────────────────────── Each family on the body ─────────────────────────

/// Moves a mesh built in torso space onto the body.
fn on_body(body: &Body, mut mesh: ItemMesh) -> ItemMesh {
    body.torso_placement().apply_to_item(&mut mesh);
    mesh
}

/// Scales a mesh by `k` about the origin (normals unchanged).
fn scale_mesh(mesh: &mut ItemMesh, k: f32) {
    for p in mesh.parts.iter_mut().flat_map(|part| part.positions.iter_mut()) {
        *p = vec3::scale(*p, k);
    }
}

/// What is worn over a garment, which presses it flatter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Cover {
    /// A closed garment over its body and skirt.
    garment: bool,
    /// A cloak hanging round its skirt.
    cloak: bool,
    /// Sleeves over its sleeves.
    sleeves: bool,
    /// Gloves over its cuffs.
    gloves: bool,
    /// A worn belt in place of its own.
    belt: bool,
    /// A hood on the head in place of its own cowl.
    hood: bool,
}

impl Cover {
    /// What `worn` puts over a garment in `slot`.
    fn of(worn: &[(Slot, Item)], slot: Slot) -> Cover {
        let order = |s: Slot| LAYER_ORDER.iter().position(|x| *x == s);
        let over = |s: Slot| order(s) > order(slot);
        let mut cover = Cover::default();
        for (s, item) in worn {
            match item {
                Item::Garment(g) if over(*s) && matches!(s, Slot::Shirt | Slot::Body | Slot::Cloak) && !g.cut.style.is_open() => {
                    cover.garment = true;
                    cover.sleeves |= g.cut.sleeves != Sleeves::None && !matches!(g.cut.style, GarmentStyle::Cuirass | GarmentStyle::Brigandine);
                }
                Item::Garment(_) if over(*s) && *s == Slot::Cloak => cover.cloak = true,
                Item::Hands(h) if *s == Slot::Hands => cover.gloves |= h.hand.style != HandStyle::Bracer,
                Item::Belt(_) if *s == Slot::Belt => cover.belt = true,
                Item::Hood(_) if *s == Slot::Head => cover.hood = true,
                _ => {}
            }
        }
        cover
    }
}

/// Clothing and body armour: the Armory garment on the trunk eased over what is worn under
/// it, its sleeves and pauldrons on the arms. A cloak also hangs clear of the arms. Lengths
/// scale with the wearer and the hem stays clear of the ankles (the plate's `saGarment`
/// `yBot`). Under another garment the skirt hangs straight and its folds lie flat, sleeves
/// under sleeves or gloves do not bell, and a garment's own belt gives way to a worn one:
/// cloth pressed under cloth, which the layering otherwise takes at its fullest.
fn garment(body: &Body, env: &Envelope, state: &GarmentState, cover: &Cover) -> ItemMesh {
    let mut s = state.clone();
    let room = body.waist_y - body.ankle_y - 0.35 * body.scale;
    s.cut.length = if s.cut.style == GarmentStyle::Robe {
        // A robe is cut to its wearer and hangs to just above the feet (our own). Scaled as
        // other lengths are, the longest robe rolled ended at mid-shin on a human, and none
        // could reach below 19 cm off the ground.
        body.waist_y - body.ankle_y - ROBE_CLEARANCE * body.scale
    } else {
        (s.cut.length * body.scale * 0.95).min(room)
    };
    s.belt.width *= body.scale.sqrt();
    if cover.garment {
        s.cut.flare = 0.0;
        s.cut.folds *= 0.3;
    } else if cover.cloak {
        // A cloak hangs round the skirt and gathers its spread in.
        s.cut.flare = 0.0;
    }
    if cover.sleeves || cover.gloves {
        s.cut.sleeve_flare = 0.0;
    }
    if cover.belt {
        s.belt.on = false;
    }
    // A hood worn on the head stands in for a cowl's own (the plate draws no hood roll then).
    if cover.hood && s.cut.neck == Neckline::Cowl {
        s.cut.neck = Neckline::Round;
    }
    // A cloak rests on the shoulders and hangs over the arms and what is on them through an
    // underlay, falling past them and back toward the legs below them, instead of on a form
    // widened all round to clear them (which stood out like a tent).
    let open = s.cut.style.is_open();
    let mut form = env.form(if open { Over::Shoulders } else { Over::Trunk }, s.cut.fit);
    // The Armory flares a skirt as a share of the form it hangs over: over gear that would
    // be wider than on the body, so keep the flare's spread as it would be on the body alone.
    s.cut.flare *= (env.form.hip.half_width / form.hip.half_width).min(1.0);
    if env.trunk_worn() && !open {
        // The skirt's folds dip inward by up to 0.4 of their depth ([`Drape::fold_wave`]):
        // keep their valleys clear of what is under them.
        let drape = garment_drape(&form.eased(s.cut.fit), &s.cut, &s.belt, &s.plates, s.seed);
        let mut rings = form.rings();
        for ring in rings.iter_mut().filter(|r| r.y < form.waist.y) {
            let fold = 0.4 * drape.fold_depth(ring.y);
            *ring = ring.inflated(fold, fold);
        }
        form = form.with_rings(rings);
    }
    let arms = env.arm_forms();
    let under = |a: f32, y: f32| env.outline.at(a, y) + GAP;
    let mut fitting = Fitting { arms: Some(&arms), over: open.then_some(&under as Underlay), notch: None };
    let mut mesh = on_body(body, garment_parts_fitted(&form, &fitting, &s.cut, &s.hem, &s.belt, &s.plates, s.seed).finish());
    // A tail passing out through the skirt (of a cloak, a robe, a tunic) gets a vent cut up
    // from the hem round it.
    if let Some(notch) = tail_notch(body, &mesh) {
        fitting.notch = Some(notch);
        mesh = on_body(body, garment_parts_fitted(&form, &fitting, &s.cut, &s.hem, &s.belt, &s.plates, s.seed).finish());
    }
    mesh
}

/// The vent a tail needs in a cloak hung as `cloak` (body space): from the hem up past the
/// highest point where the tail comes within reach of the cloth, wide enough for it there.
/// Our own: the plate's lizardfolk wear no cloaks over their tails.
fn tail_notch(body: &Body, cloak: &ItemMesh) -> Option<Notch> {
    let (path, radius) = body.tail()?;
    let w = body.waist_y;
    let mut reach = Outline::new(body.ankle_y - w - 1.0, body.height - w);
    let mut hem = f32::MAX;
    for p in cloak.parts.iter().filter(|p| p.name == "body").flat_map(|p| p.positions.iter()) {
        reach.add([p[0], p[1] - w, p[2]]);
        hem = hem.min(p[1] - w);
    }
    let margin = 0.15 * body.scale.sqrt();
    let n = path.len();
    // Where the tail's surface, `margin` out, comes within reach of the cloth's inner face (or
    // is outside it already): its angle round the body, height, top and the half angle it
    // spans there. Points all round the tail, not only its centre line, since the cloth
    // curves round it.
    let mut near: Vec<(f32, f32, f32, f32)> = vec![];
    let dirs: Vec<V3> = (0..16)
        .flat_map(|k| {
            let a = TAU * k as f32 / 16.0;
            [-0.6f32, 0.0, 0.6].map(|y| vec3::normalize_or([a.cos(), y, a.sin()], vec3::X))
        })
        .chain([vec3::Y, vec3::neg(vec3::Y)])
        .collect();
    for (i, c) in path.iter().enumerate() {
        let r = radius(i as f32 / (n - 1) as f32) + margin;
        for dir in &dirs {
            let q = [c[0] + dir[0] * r, c[1] - w + dir[1] * r, c[2] + dir[2] * r];
            if q[1] < hem {
                continue;
            }
            let angle = (-q[2]).atan2(q[0]);
            let cloth = reach.at(angle, q[1]);
            if cloth > NONE && q[0].hypot(q[2]) > cloth - 0.05 {
                near.push((angle, q[1], q[1], 0.03));
            }
        }
    }
    let first = near.iter().max_by(|a, b| a.2.total_cmp(&b.2))?;
    let (angle, top) = (first.0, first.2 + 0.1 * body.scale);
    // The vent rounds over to its top (a half ellipse): wide enough at the hem that it still
    // clears the tail at every height it passes through.
    let half_width = near
        .iter()
        .map(|&(a, y, _, half)| {
            let off = ((a - angle + PI).rem_euclid(TAU) - PI).abs();
            let t = ((y - hem) / (top - hem)).clamp(0.0, 0.97);
            (half + off) / (1.0 - t * t).sqrt()
        })
        .fold(0.0f32, f32::max)
        .min(1.4);
    Some(Notch { angle, top, half_width })
}

/// Whether footwear has a shaft above the ankle that trousers tuck into.
fn has_shaft(s: &FeetState) -> bool {
    matches!(s.boot.style, BootStyle::Wrapped | BootStyle::Sabaton) || s.boot.height > 0.65
}

/// Trousers, breeches and leggings on the body's own legs and hips, tucked into tall boots,
/// with a hole in the seat where a tail comes out (pigfolk tuck theirs in).
fn legwear(body: &Body, state: &LegsState, tucked: bool) -> ItemMesh {
    let mut wear = state.legwear;
    if tucked {
        wear.taper = 1.0;
    }
    let form = body.lower_body_form();
    let holes = if body.folk() == Folk::Lizardfolk { [1.0f32, -1.0].map(|side| tail_hole(body, &LegDrape::new(&form, &wear, state.seed), side)) } else { [None, None] };
    legwear_parts_holed(&form, &wear, &state.hem, state.seed, holes).finish()
}

/// The hole a tail needs through the cloth of leg `side`: round where the cloth comes within
/// reach of the tail (or lies inside it), `margin` wider. Our own: the plate's lizardfolk wear
/// trousers through their tails.
fn tail_hole(body: &Body, drape: &LegDrape, side: f32) -> Option<LegHole> {
    let (path, radius) = body.tail()?;
    let margin = 0.08 * body.scale.sqrt();
    let n = path.len();
    let near = |p: V3| {
        (0..n - 1).any(|i| {
            let (a, b) = (path[i], path[i + 1]);
            let ab = vec3::sub(b, a);
            let t = (vec3::dot(vec3::sub(p, a), ab) / vec3::dot(ab, ab).max(1e-9)).clamp(0.0, 1.0);
            let r = radius((i as f32 + t) / (n - 1) as f32);
            vec3::distance(p, vec3::add(a, vec3::scale(ab, t))) < r + margin
        })
    };
    let mut hits: Vec<(f32, f32)> = vec![];
    let rows = ((drape.top - drape.hem) / 0.04).ceil() as usize;
    for k in 0..=rows {
        let y = drape.hem + (drape.top - drape.hem) * k as f32 / rows as f32;
        for j in 0..96 {
            let a = TAU * j as f32 / 96.0;
            if near(drape.point(side, a, y, 0.0)) {
                hits.push((a, y));
            }
        }
    }
    if hits.is_empty() {
        return None;
    }
    let (sx, sz) = hits.iter().fold((0.0, 0.0), |(x, z), (a, _)| (x + a.cos(), z + a.sin()));
    let angle = sz.atan2(sx);
    let off = |a: f32| ((a - angle + PI).rem_euclid(TAU) - PI).abs();
    let (y0, y1) = hits.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &(_, y)| (lo.min(y), hi.max(y)));
    let y = 0.5 * (y0 + y1);
    // The smallest ellipse of the hits' own proportions that takes them all in, a cell wider
    // all round.
    let (a0, h0) = (hits.iter().map(|&(a, _)| off(a)).fold(0.0f32, f32::max).max(1e-3), (0.5 * (y1 - y0)).max(1e-3));
    let k = hits.iter().map(|&(a, hy)| ((off(a) / a0).powi(2) + ((hy - y) / h0).powi(2)).sqrt()).fold(0.0f32, f32::max);
    Some(LegHole { angle, y, half_angle: k * a0 + 1.5 * TAU / 96.0, half_height: k * h0 + 0.06 })
}

/// Footwear on each foot: the body's own foot with the boot's toe and width, the shaft
/// eased over the trousers, no taller than the knee.
fn boots(body: &Body, env: &Envelope, state: &FeetState) -> ItemMesh {
    let mut b = state.boot;
    let mut form = body.leg_form();
    let size = form.foot_length / 2.6;
    b.size = size;
    let toe = if b.style == BootStyle::Poulaine || b.toe > 0.05 { b.toe } else { 0.0 };
    form.toe_point = toe * 1.8 * size;
    form.toe_lift = toe * 0.4 * size;
    form.ball_half_width *= b.width;
    form.heel_half_width *= b.width;
    b.height = b.height.min(form.knee_height * 1.05 / size);
    let mut out = ItemMesh::default();
    for side in Side::BOTH {
        let mut f = form;
        let leg = body.leg(side);
        // Eased over what is worn under it from the knee down to mid-shin, and from there to
        // the ankle, where trousers taper in.
        let tk = leg.t_joints[1];
        let mid = tk + 0.5 * (1.0 - tk);
        let clearance = |t0: f32, t1: f32| Envelope::limb_clearance(&env.legs[index(side)], t0, t1).unwrap_or(0.0);
        let knee = clearance(tk - 0.1, mid);
        f.ease = [clearance(mid, 1.0).min(knee.max(GAP)), knee];
        let mut mesh = boot_parts(&f, &b, &state.hem).finish();
        body.foot_placement(side).apply_to_item(&mut mesh);
        out.parts.extend(mesh.parts);
    }
    out
}

/// Gloves on the body's hands, in the hands' pose, their cuffs over the sleeves and no
/// longer than most of the forearm (the plate's `saHands` caps them at 0.95 of it).
fn gloves(body: &Body, env: &Envelope, state: &HandsState) -> ItemMesh {
    let mut h = state.hand;
    let form = body.hand_form();
    h.size = form.palm_length / 0.95;
    let mut out = ItemMesh::default();
    for side in Side::BOTH {
        let arm = body.arm(side);
        let forearm = vec3::distance(arm.joints[1], arm.joints[2]);
        let mut hand = h;
        hand.cuff = hand.cuff.min(0.85 * forearm / h.size);
        // What is worn on the forearm under the cuff, as the cuff sees it: in the hand's
        // space, gridded by how far below the wrist and by angle round the forearm, the most
        // it reaches from the forearm's axis in each cell and the cells about it. The cuff is
        // drawn close over that, so a bracer follows a sleeve all along. Our own fitting; the
        // first Rust fit eased the whole cuff by the most anything stood out under it.
        let place = body.hand_placement(side);
        let (x, y, z) = (place.x(), place.y(), place.z());
        let reach = hand.cuff * h.size + 0.3;
        const DOWN: f32 = 0.05;
        const ROUND: usize = 32;
        let rows = (reach / DOWN).ceil() as usize + 2;
        let mut grid = vec![[NONE; ROUND]; rows];
        for &p in &env.on_arm[index(side)] {
            let q = vec3::sub(p, place.origin);
            let l = [vec3::dot(q, x), vec3::dot(q, y), vec3::dot(q, z)];
            let down = -l[1];
            if !(-0.1..reach).contains(&down) {
                continue;
            }
            let i = ((down + 0.1) / DOWN) as usize;
            let j = ((((-l[2]).atan2(l[0])).rem_euclid(TAU) / TAU * ROUND as f32) as usize).min(ROUND - 1);
            if let Some(row) = grid.get_mut(i) {
                row[j] = row[j].max(l[0].hypot(l[2]));
            }
        }
        // A sleeve's rings lie further apart than the cells: fill between them, down each
        // column of the grid.
        #[allow(clippy::needless_range_loop)]
        for j in 0..ROUND {
            let filled: Vec<usize> = (0..rows).filter(|&i| grid[i][j] > NONE).collect();
            for w in filled.windows(2) {
                let (a, b) = (w[0], w[1]);
                if b - a > 1 && b - a <= 8 {
                    for i in a + 1..b {
                        let t = (i - a) as f32 / (b - a) as f32;
                        grid[i][j] = grid[a][j] + (grid[b][j] - grid[a][j]) * t;
                    }
                }
            }
        }
        let under = |angle: f32, down: f32| -> Option<f32> {
            let i = ((down + 0.1) / DOWN).floor() as isize;
            let j = ((angle.rem_euclid(TAU) / TAU * ROUND as f32) as isize).rem_euclid(ROUND as isize);
            let m = (i - 2..=i + 2)
                .filter_map(|k| grid.get(usize::try_from(k).ok()?))
                .flat_map(|row| (j - 2..=j + 2).map(move |c| row[c.rem_euclid(ROUND as isize) as usize]))
                .fold(NONE, f32::max);
            (m > NONE).then_some(m)
        };
        let pose = body.hand_pose(side);
        let mut mesh = hand_parts_fitted(&form, &hand, &state.hem, pose.fingers, pose.thumb, Some(&under)).finish();
        place.apply_to_item(&mut mesh);
        out.parts.extend(mesh.parts);
    }
    out
}

/// A belt round the waist over what is worn under it. Over a robe its pouch is left off (our
/// own): it hung where the robe's skirt falls and caught in it.
fn belt(body: &Body, env: &Envelope, state: &BeltState, robed: bool) -> ItemMesh {
    let mut s = state.clone();
    s.fittings.pouch &= !robed;
    on_body(body, Belts::on_form(&env.form(Over::Trunk, 1.0), &s))
}

/// The head form grown by its covered hair and its folded features, in torso space.
fn head_over_hair(body: &Body, head: HeadFit) -> HeadForm {
    let h = body.head_form();
    let v = body.hair_volume(true) + body.feature_volume(head);
    HeadForm { half_width: h.half_width + v, half_height: h.half_height + v, half_depth: h.half_depth + v, ..h }
}

/// A helmet resized to the head and its covered hair (the plate's `saHelm`: radius
/// `1.12 * head half width + hair volume`), its brow a little above the head's centre.
///
/// Ears come out through openings cut where they pass through the shell (our own: the plate
/// draws none), and a folded frill under it sizes it up as hair does.
fn helm(body: &Body, state: &HelmState, fit: HeadFit) -> ItemMesh {
    let head = &body.head;
    // A head much deeper than it is wide (a lizard's) sizes it by its depth instead, so the
    // back of the skull stays inside (our own: the bowl is 1.12 times deeper than wide).
    let s = state.sized((1.12 * head.half_width).max(0.96 * head.half_depth) + body.hair_volume(true) + body.feature_volume(fit));
    let at = vec3::add(head.centre, [0.0, 0.1 * head.half_height, 0.0]);
    // In the helm's space: standing ears come out through it, and its rim lifts over a
    // lizard's snout and brow.
    let features = body.standing_features(fit);
    let local = |f: Vec<V3>| f.into_iter().map(|p| vec3::sub(p, at)).collect::<Vec<V3>>();
    let openings = Openings { through: features.ears.into_iter().map(local).collect(), lift: features.face.into_iter().map(local).collect(), margin: 0.08 * body.scale.sqrt() };
    let mut mesh = Helm::build_with_openings(&s, &openings);
    Placement::translation(at).apply_to_item(&mut mesh);
    mesh
}

/// A hood over the head and hair, its cape over everything on the shoulders.
fn hood(body: &Body, env: &Envelope, state: &HoodState, head: HeadFit) -> ItemMesh {
    let mut hood = state.hood;
    hood.cape *= body.scale;
    on_body(body, hood_parts(&head_over_hair(body, head), &env.form(Over::Arms, 1.0), &hood, &state.hem, state.seed).finish())
}

/// A hat over the head and hair.
fn hat(body: &Body, state: &HatState, head: HeadFit) -> ItemMesh {
    on_body(body, hat_parts(&head_over_hair(body, head), &state.hat, state.seed).finish())
}

/// The Armory pendant on a necklace laid over the body: round the base of the neck behind
/// and at the sides, dipping to the pendant in front, on whatever is worn there (the plate's
/// `saAmulet`). The pendant is drawn a little over true size, as the plate's is, so it reads
/// on the figure.
fn amulet(body: &Body, env: &Envelope, state: &AmuletState) -> ItemMesh {
    let n = &state.necklace;
    let (t, w) = (body.trunk_length, body.waist_y);
    let profile = env.form(Over::Trunk, 1.0).profile();
    let neck_y = body.shoulder_y + 0.075 * t - w;
    let drop = (n.drop * 0.45 * body.scale.sqrt()).clamp(0.14 * t, 0.42 * t);
    let low = neck_y - drop;
    let strand = match n.style {
        NecklaceStyle::Beads => n.link * 0.5,
        NecklaceStyle::Cable => n.link * 0.3,
        _ => n.thickness,
    } + 0.01;
    let at = |a: f32, y: f32, off: f32| -> V3 {
        let [x, z] = profile.section(a, y, off);
        [x, y + w, z]
    };
    let path: Vec<V3> = (0..96)
        .map(|i| {
            let u = i as f32 / 96.0;
            let f = (TAU * (u - 0.5)).cos().max(0.0).powf(2.4);
            at(BACK + TAU * u, neck_y + (low - neck_y) * f, strand)
        })
        .collect();
    let (pendant, bail) = pendant_parts(state);
    let mut pendant = pendant.finish();
    let k = (1.45 * body.scale.sqrt()).clamp(1.0, 2.2);
    scale_mesh(&mut pendant, k);
    let (lo, hi) = pendant.bounds().unwrap_or(([0.0; 3], [0.0; 3]));
    let hang = hi[1] - lo[1];
    let top = at(FRONT, low, strand);
    let foot = at(FRONT, low - hang, (hi[2] - lo[2]) * 0.5 + 0.01);
    let up = vec3::normalize_or(vec3::sub(top, foot), vec3::Y);
    let out = vec3::normalize_or(vec3::sub(vec3::Z, vec3::scale(up, up[2])), vec3::Z);
    let origin = vec3::sub(top, vec3::scale(up, bail.rest * k));
    Placement::new(origin, vec3::cross(up, out), up, out).apply_to_item(&mut pendant);
    let mut mesh = necklace_parts(n, &path, true, Some((top, bail.clear * k))).finish();
    mesh.parts.extend(pendant.parts);
    mesh
}

/// A bag on the back, sized to the shoulders, resting on whatever is worn there, carried on
/// straps laid over the worn surface (the plate's `saBack`, `pushOut` and `strapCurve`).
/// The bag's own carrying straps give way to these.
fn bag(body: &Body, env: &Envelope, state: &BagState) -> ItemMesh {
    let style = state.bag.style;
    let mut mesh = Bags::build(state);
    let tint = mesh.parts.iter().find(|p| p.name == "straps").and_then(|p| p.tint);
    mesh.parts.retain(|p| p.name != "carry");
    // Its back against the wearer's.
    mesh.transform(&Mat3::from_axis_angle(vec3::Y, PI), [0.0; 3]);
    let Some((lo, hi)) = mesh.bounds() else { return mesh };
    let sw = body.shoulder_half_width;
    let width = match style {
        BagStyle::Backpack => sw * 1.25,
        BagStyle::Satchel => sw * 0.95,
        _ => sw * 1.1,
    };
    let k = (width / (hi[0] - lo[0]).max(1e-3)).clamp(0.3, 3.0);
    scale_mesh(&mut mesh, k);
    let (lo, hi) = (vec3::scale(lo, k), vec3::scale(hi, k));
    let (t, s, w) = (body.trunk_length, body.scale, body.waist_y);
    let height = hi[1] - lo[1];
    let cy = match style {
        BagStyle::Backpack => body.chest_y - 0.1 * t,
        BagStyle::Satchel => body.hip_y + 0.25 * t,
        _ => body.waist_y,
    };
    // How far the back reaches under the bag's upper and lower halves: the bag rests on the
    // upper and tilts out over whatever stands further out below (a cloak's skirt).
    let back = |from: f32, to: f32| (0..=4).map(|i| env.worn_reach(-PI / 2.0, cy - w + height * (from + (to - from) * i as f32 / 4.0))).fold(0.0f32, f32::max);
    let (upper, lower) = (back(0.0, 0.5), back(-0.5, 0.0));
    let tilt = ((lower - upper).max(0.0) / height).atan();
    let x = -(lo[0] + hi[0]) * 0.5 + if style == BagStyle::Sack { 0.3 * s } else { 0.0 };
    let face = -(upper + 0.06);
    let hang = cy + height * 0.5;
    // Its top edge against the back, then tilted about that edge.
    mesh.transform(&Mat3::IDENTITY, [x, cy - (lo[1] + hi[1]) * 0.5 - hang, -hi[2]]);
    mesh.transform(&Mat3::from_axis_angle(vec3::X, tilt), [0.0, hang, face]);
    let face_at = |y: f32| face - (hang - y) * tilt.tan();

    let (top, bottom, half) = (cy + height * 0.42, cy - height * 0.38, (hi[0] - lo[0]) * 0.42);
    let ty = body.shoulder_y + 0.08 * t;
    let front = body.chest.half_depth * 0.5;
    let (width, thickness) = (0.24 * s.sqrt(), 0.05 * s.sqrt());
    let mut straps = armory::geom::MeshBuilder::new();
    let mut lay = |control: Vec<V3>| {
        let path: Vec<V3> = CatmullRom::centripetal(control)
            .sample_evenly(64)
            .into_iter()
            .map(|p| {
                let q = [p[0], p[1] - w, p[2]];
                let q = env.lay_on(q, 0.01);
                [q[0], q[1] + w, q[2]]
            })
            .collect();
        let faces: Vec<V3> = path.iter().map(|p| env.reach.normal(p[2].atan2(p[0]), p[1] - w)).collect();
        straps.append(&strap(&path, &faces, width, thickness, false));
    };
    if style == BagStyle::Backpack {
        for sg in [1.0f32, -1.0] {
            lay(vec![
                [sg * half, top, face_at(top)],
                [sg * sw * 0.46, ty, -0.15 * s],
                [sg * sw * 0.5, body.shoulder_y - 0.06 * t, front],
                [sg * sw * 0.52, body.chest_y - 0.25 * t, front],
                [sg * body.waist.half_width * 0.7, body.waist_y + 0.05 * t, -body.waist.half_depth * 0.5],
                [sg * half, bottom, face_at(bottom)],
            ]);
        }
    } else {
        // One strap from the bag over the right shoulder, across the chest to the left hip.
        let x0 = x;
        lay(vec![
            [x0 - half * 0.5, top, face_at(top)],
            [-sw * 0.46, ty, -0.08 * s],
            [-sw * 0.3, body.chest_y, front],
            [sw * 0.55, body.waist_y, front],
            [body.waist.half_width * 0.8, cy, -body.waist.half_depth * 0.3],
            [x0 + half, cy, face_at(cy)],
        ]);
    }
    let look = tint.map_or(Look::of(MaterialKind::Leather), |c| Look::of(MaterialKind::Leather).tinted(c));
    let mut parts = Parts::new();
    parts.add_look("carry", straps, look);
    mesh.parts.extend(parts.finish().parts);
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::BodySpec;
    use armory::families::bags::Bags;
    use armory::families::garment::Garment;
    use armory::families::helm::HelmStyle;
    use armory::sections::garment::GarmentStyle;
    use armory::Family;

    fn roll(family: Family, seed: u32) -> Item {
        Item::roll(family, seed).unwrap()
    }

    /// A full outfit: trousers and boots, a shirt under armour (styles turning with the
    /// seed), gloves, a belt, an amulet, a cloak, a hood and a pack.
    fn outfit(seed: u32) -> Vec<(Slot, Item)> {
        use GarmentStyle::*;
        let shirt = [Tunic, Robe, Gambeson][seed as usize % 3];
        let armour = [Hauberk, Scale, Cuirass, Brigandine, Gambeson][seed as usize % 5];
        vec![
            (Slot::Back, Item::Bags(Bags::roll_as(seed, BagStyle::Backpack))),
            (Slot::Shirt, Item::Garment(Garment::roll_as(seed, shirt))),
            (Slot::Body, Item::Garment(Garment::roll_as(seed + 1, armour))),
            (Slot::Cloak, Item::Garment(Garment::roll_as(seed + 2, GarmentStyle::Cloak))),
            (Slot::Legs, roll(Family::Legs, seed)),
            (Slot::Feet, roll(Family::Feet, seed)),
            (Slot::Hands, roll(Family::Hands, seed)),
            (Slot::Belt, roll(Family::Belt, seed)),
            (Slot::Neck, roll(Family::Amulet, seed)),
            (Slot::Head, roll(Family::Hood, seed)),
        ]
    }

    fn bodies() -> impl Iterator<Item = Body> {
        Folk::ALL.into_iter().flat_map(|f| [3u32, 1207].map(move |s| Body::new(&BodySpec::roll(f, s))))
    }

    /// Radial reach from the body's axis, binned by polar angle and height.
    struct Radial {
        bins: std::collections::HashMap<(i32, i32), f32>,
    }

    const PHI_BINS: f32 = 90.0;
    const Y_BIN: f32 = 0.04;

    fn bin(p: V3) -> (i32, i32) {
        let phi = p[2].atan2(p[0]);
        (((phi + PI) / TAU * PHI_BINS) as i32, (p[1] / Y_BIN).floor() as i32)
    }

    /// Points of a layer's parts on the trunk (not on the arms or hands, nor hung from a belt).
    fn trunk_points<'a>(body: &'a Body, layer: &'a WornLayer) -> impl Iterator<Item = V3> + 'a {
        let env = Envelope::new(body);
        let parts: Vec<&Part> = layer.mesh.parts.iter().filter(|p| measured(layer.slot, p) && !env.on_arms(p)).collect();
        parts.into_iter().flat_map(|p| p.positions.iter().copied())
    }

    impl Radial {
        fn of(points: impl Iterator<Item = V3>) -> Radial {
            let mut bins = std::collections::HashMap::new();
            for p in points {
                let r = p[0].hypot(p[2]);
                let e = bins.entry(bin(p)).or_insert(f32::MIN);
                *e = f32::max(*e, r);
            }
            Radial { bins }
        }
    }

    fn no_layer_pokes_through_the_ones_under_it_check(bodies: Vec<Body>, kits: &[u32]) {
        // 8 mm: about two cloth thicknesses, where sleeves join a body and at the shoulders.
        let tolerance = 0.08;
        let mut failures = vec![];
        let mut compared = 0;
        for (body, seed) in bodies.iter().flat_map(|b| kits.iter().map(move |&s| (b, s))) {
            let fit = fit_worn(body, &outfit(seed));
            let torso = [Slot::Legs, Slot::Shirt, Slot::Body, Slot::Belt, Slot::Cloak];
            let layers: Vec<&WornLayer> = fit.layers.iter().filter(|l| torso.contains(&l.slot)).collect();
            let env = Envelope::new(body);
            for (li, outer) in layers.iter().enumerate().skip(1) {
                let under = Radial::of(layers[..li].iter().flat_map(|l| trunk_points(body, l)));
                let mut worst = (0.0f32, [0.0; 3], Slot::Legs);
                let mut count = 0;
                let mut total = 0;
                // Every garment's neckline is cut at the neck itself, so their top edges meet
                // there by design: compare from just below it down.
                let neck = body.shoulder_y + 0.11 * body.trunk_length - 0.2 * body.scale;
                for p in trunk_points(body, outer).filter(|p| p[1] < neck) {
                    total += 1;
                    let (i, j) = bin(p);
                    if let Some(&r) = under.bins.get(&(i, j)) {
                        // The bins are coarse in height: where the layer under slopes steeply
                        // (over the shoulders) allow for its change across one.
                        let slope = match (under.bins.get(&(i, j + 1)), under.bins.get(&(i, j - 1))) {
                            (Some(a), Some(b)) => (a - b).abs() * 0.5,
                            (Some(a), None) | (None, Some(a)) => (a - r).abs(),
                            _ => 0.0,
                        };
                        let mut depth = r - p[0].hypot(p[2]) - slope;
                        if depth > tolerance {
                            // Over a shelf (a cloak on a pauldron's top) the bins mix heights:
                            // measure the layers under along this point's ray exactly.
                            let exact = layers[..li].iter().flat_map(|l| l.mesh.parts.iter().filter(|q| measured(l.slot, q) && !env.on_arms(q))).filter_map(|q| ray_reach(q, p)).fold(0.0f32, f32::max);
                            depth = exact - p[0].hypot(p[2]);
                        }
                        compared += 1;
                        if depth > tolerance {
                            count += 1;
                            if depth > worst.0 {
                                worst = (depth, p, outer.slot);
                            }
                        }
                    }
                }
                if count > 0 {
                    failures.push(format!("{:?} body {} outfit {seed}: {count} of {total} points of {:?} inside the layers under it, worst {:?}", body.folk(), body.spec.seed, outer.slot, worst));
                }
            }
        }
        assert!(compared > 100_000, "layers overlap in only {compared} places: the check is not seeing them");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    #[ignore = "slow sweep: body fitting test, every folk and seed; run before merging fitting changes"]
    fn no_layer_pokes_through_the_ones_under_it() {
        no_layer_pokes_through_the_ones_under_it_check(bodies().collect(), &[10, 11, 12, 13, 14]);
    }

    #[test]
    fn no_layer_pokes_through_the_ones_under_it_smoke() {
        no_layer_pokes_through_the_ones_under_it_check([Folk::Human, Folk::Lizardfolk, Folk::Pigfolk].into_iter().map(|f| Body::new(&BodySpec::roll(f, 1207))).collect(), &[10]);
    }

    #[test]
    fn a_cloak_hangs_clear_of_the_arms() {
        for body in bodies() {
            let fit = fit_worn(&body, &outfit(5));
            let cloak = &fit.layer(Slot::Cloak).unwrap().mesh;
            for side in Side::BOTH {
                let arm = body.arm(side);
                for p in cloak.parts.iter().flat_map(|p| p.positions.iter()) {
                    let (t, d) = nearest(arm, *p);
                    if t > arm.t_joints[0] {
                        assert!(d > arm.radius_at(t), "{:?}: cloak inside the {side:?} arm at {p:?}", body.folk());
                    }
                }
            }
        }
    }

    /// A robe with long bell sleeves under a cloak, with trousers and boots.
    fn robe_and_cloak(seed: u32) -> Vec<(Slot, Item)> {
        let mut robe = Garment::roll_as(seed, GarmentStyle::Robe);
        robe.cut.sleeves = Sleeves::Long;
        robe.cut.sleeve_flare = 1.0;
        vec![
            (Slot::Shirt, Item::Garment(robe)),
            (Slot::Cloak, Item::Garment(Garment::roll_as(seed + 1, GarmentStyle::Cloak))),
            (Slot::Legs, roll(Family::Legs, seed)),
            (Slot::Feet, roll(Family::Feet, seed)),
        ]
    }

    /// The bare body's silhouettes (not a tail's) from the front and the side: how far it
    /// reaches to either side (`|x|`), and its front and back (`z`).
    fn silhouette(body: &Body) -> (f32, f32, f32) {
        let mut mesh = body.skin_mesh();
        mesh.parts.retain(|p| p.name != "tail");
        mesh.parts.extend(body.head_mesh().parts);
        let (lo, hi) = mesh.bounds().unwrap();
        (lo[0].abs().max(hi[0]), lo[2], hi[2])
    }

    /// How far `part`'s surface reaches along the horizontal ray from the body's axis through
    /// `p`: the farthest triangle the ray meets (Moeller-Trumbore).
    fn ray_reach(part: &Part, p: V3) -> Option<f32> {
        let origin = [0.0, p[1], 0.0];
        let dir = vec3::normalize_or([p[0], 0.0, p[2]], vec3::X);
        part.indices
            .chunks(3)
            .filter_map(|t| {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| part.positions[i as usize]);
                let (e1, e2) = (vec3::sub(b, a), vec3::sub(c, a));
                let h = vec3::cross(dir, e2);
                let det = vec3::dot(e1, h);
                if det.abs() < 1e-9 {
                    return None;
                }
                let s = vec3::sub(origin, a);
                let u = vec3::dot(s, h) / det;
                let q = vec3::cross(s, e1);
                let v = vec3::dot(dir, q) / det;
                let t = vec3::dot(e2, q) / det;
                (u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.0).then_some(t)
            })
            .reduce(f32::max)
    }

    fn a_cloak_over_bell_sleeves_falls_close_to_the_body_check(folks: &[Folk], seeds: &[u32], kits: &[u32]) {
        let mut failures = vec![];
        for body in folks.iter().flat_map(|&f| seeds.iter().map(move |&s| Body::new(&BodySpec::roll(f, s)))) {
            for &seed in kits {
                let fit = fit_worn(&body, &robe_and_cloak(seed));
                let cloak = fit.layer(Slot::Cloak).unwrap().mesh.parts.iter().find(|p| p.name == "body").unwrap().clone();
                let (side, back, front) = silhouette(&body);
                let lo = cloak.positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
                // Its hem stays within the silhouette plus a hand's width (about 1 unit), seen
                // from the front and from the side.
                let hand = 1.0;
                let mut worst = (f32::MIN, [0.0; 3]);
                for p in cloak.positions.iter().filter(|p| p[1] < lo + 0.3) {
                    let over = (p[0].abs() - side).max(back - p[2]).max(p[2] - front) - hand;
                    if over > worst.0 {
                        worst = (over, *p);
                    }
                }
                if worst.0 > 0.0 {
                    failures.push(format!("{:?} body {} seed {seed}: hem {:.2} past the silhouette and a hand at {:?}", body.folk(), body.spec.seed, worst.0, worst.1));
                }
                // Nothing under it pokes through: every point worn under it, sleeves and all,
                // lies inside it wherever it covers.
                let outer = Radial::of(cloak.positions.iter().copied());
                let top = body.shoulder_y - 0.1 * body.trunk_length;
                for layer in fit.layers.iter().filter(|l| matches!(l.slot, Slot::Shirt | Slot::Legs)) {
                    for p in layer.mesh.parts.iter().flat_map(|p| p.positions.iter()).filter(|p| p[1] > lo + 0.1 && p[1] < top) {
                        // The cloak's reach there, from the rows of its mesh either side.
                        let (i, j) = bin(*p);
                        let near = (j - 2..=j + 2).filter_map(|k| outer.bins.get(&(i, k))).copied().reduce(f32::max);
                        if let Some(r) = near {
                            // Where the bins say it may poke through, measure the cloak along
                            // the ray exactly.
                            let r = if p[0].hypot(p[2]) > r + 0.03 { ray_reach(&cloak, *p).unwrap_or(r) } else { r };
                            let depth = p[0].hypot(p[2]) - r;
                            if depth > 0.03 {
                                failures.push(format!("{:?} body {} seed {seed}: {:?} pokes {depth:.2} through the cloak at {p:?}", body.folk(), body.spec.seed, layer.slot));
                                break;
                            }
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    #[ignore = "slow sweep: body fitting test, every folk and seed; run before merging fitting changes"]
    fn a_cloak_over_bell_sleeves_falls_close_to_the_body() {
        a_cloak_over_bell_sleeves_falls_close_to_the_body_check(&Folk::ALL, &[3, 1207, 40], &[10, 11, 12, 13]);
    }

    #[test]
    fn a_cloak_over_bell_sleeves_falls_close_to_the_body_smoke() {
        a_cloak_over_bell_sleeves_falls_close_to_the_body_check(&Folk::ALL, &[1207], &[10]);
    }

    /// The triangles of `part` whose bounds meet the box `lo..hi`.
    fn triangles_in(part: &Part, lo: V3, hi: V3) -> Vec<[V3; 3]> {
        part.indices
            .chunks(3)
            .map(|t| [t[0], t[1], t[2]].map(|i| part.positions[i as usize]))
            .filter(|t| (0..3).all(|k| t.iter().any(|p| p[k] >= lo[k]) && t.iter().any(|p| p[k] <= hi[k])))
            .collect()
    }

    /// Whether segment `a`-`b` passes through triangle `t` (Moeller-Trumbore).
    fn segment_crosses(a: V3, b: V3, t: &[V3; 3]) -> bool {
        let dir = vec3::sub(b, a);
        let (e1, e2) = (vec3::sub(t[1], t[0]), vec3::sub(t[2], t[0]));
        let h = vec3::cross(dir, e2);
        let det = vec3::dot(e1, h);
        if det.abs() < 1e-12 {
            return false;
        }
        let s = vec3::sub(a, t[0]);
        let u = vec3::dot(s, h) / det;
        let q = vec3::cross(s, e1);
        let v = vec3::dot(dir, q) / det;
        let w = vec3::dot(e2, q) / det;
        u >= 0.0 && v >= 0.0 && u + v <= 1.0 && (0.0..=1.0).contains(&w)
    }

    /// How many edges of either part pass through a triangle of the other: 0 when their
    /// surfaces do not meet.
    fn crossings(a: &Part, b: &Part) -> usize {
        let bounds = |p: &Part| {
            p.positions.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), q| ([lo[0].min(q[0]), lo[1].min(q[1]), lo[2].min(q[2])], [hi[0].max(q[0]), hi[1].max(q[1]), hi[2].max(q[2])]))
        };
        let one_way = |a: &Part, b: &Part| {
            let (lo, hi) = bounds(a);
            let tris = triangles_in(b, lo, hi);
            if tris.is_empty() {
                return 0;
            }
            let (blo, bhi) = bounds(b);
            a.indices
                .chunks(3)
                .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
                .map(|(i, j)| (a.positions[i as usize], a.positions[j as usize]))
                .filter(|(p, q)| (0..3).all(|k| p[k].max(q[k]) >= blo[k] && p[k].min(q[k]) <= bhi[k]))
                .filter(|(p, q)| tris.iter().any(|t| segment_crosses(*p, *q, t)))
                .count()
        };
        one_way(a, b) + one_way(b, a)
    }

    #[test]
    fn a_tail_comes_out_through_a_cloak() {
        let mut failures = vec![];
        for seed in [3u32, 1207, 40, 8] {
            let body = Body::new(&BodySpec::roll(Folk::Lizardfolk, seed));
            let tail = body.skin_mesh().parts.into_iter().find(|p| p.name == "tail").unwrap();
            for kit in [10u32, 11, 12, 13] {
                for worn in [robe_and_cloak(kit), outfit(kit)] {
                    let fit = fit_worn(&body, &worn);
                    for part in &fit.layer(Slot::Cloak).unwrap().mesh.parts {
                        let n = crossings(&tail, part);
                        if n > 0 {
                            failures.push(format!("lizardfolk {seed} kit {kit}: the tail passes through the cloak's {} ({n} crossings)", part.name));
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    #[ignore = "a diagnostic dump, not a check: prints where a lizardfolk's tail crosses its shirt and cloak"]
    fn zz_tail_debug() {
        let body = Body::new(&BodySpec::roll(Folk::Lizardfolk, 3));
        let tail = body.skin_mesh().parts.into_iter().find(|p| p.name == "tail").unwrap();
        for worn in [robe_and_cloak(12), outfit(12), robe_and_cloak(13), outfit(13)] {
            let fit = fit_worn(&body, &worn);
            for layer in fit.layers.iter().filter(|l| matches!(l.slot, Slot::Shirt | Slot::Cloak)) {
                let part = layer.mesh.parts.iter().find(|p| p.name == "body").unwrap();
                let notch = tail_notch(&body, &layer.mesh);
                let tris = triangles_in(part, [-99.0; 3], [99.0; 3]);
                let mut hits = vec![];
                for t in tail.indices.chunks(3) {
                    let (a, b) = (tail.positions[t[0] as usize], tail.positions[t[1] as usize]);
                    if tris.iter().any(|tr| segment_crosses(a, b, tr)) {
                        hits.push(((-a[2]).atan2(a[0]), a[1] - body.waist_y));
                    }
                }
                let hem = part.positions.iter().map(|p| p[1] - body.waist_y).fold(f32::MAX, f32::min);
                eprintln!("{:?} hem {hem:.2} notch {notch:?} hits {}: {:?}", layer.slot, hits.len(), hits.iter().take(6).collect::<Vec<_>>());
            }
        }
    }

    fn a_tail_comes_out_through_robes_and_trousers_check(folks: &[Folk], seeds: &[u32], kits: &[u32], min_checked: usize) {
        use armory::families::legs::Legs;
        let mut failures = vec![];
        let mut checked = 0;
        for &folk in folks {
            for &seed in seeds {
                let body = Body::new(&BodySpec::roll(folk, seed));
                let tail = body.skin_mesh().parts.into_iter().find(|p| p.name == "tail").unwrap();
                for &kit in kits {
                    let robe = vec![
                        (Slot::Shirt, Item::Garment(Garment::roll_as(kit, GarmentStyle::Robe))),
                        (Slot::Legs, Item::Legs(Legs::roll_as(kit, LegwearStyle::Trousers))),
                        (Slot::Feet, roll(Family::Feet, kit)),
                    ];
                    let leggings = vec![(Slot::Legs, Item::Legs(Legs::roll_as(kit, [LegwearStyle::Leggings, LegwearStyle::Breeches][kit as usize % 2])))];
                    for worn in [robe, leggings, robe_and_cloak(kit), outfit(kit)] {
                        let fit = fit_worn(&body, &worn);
                        if fit.covered.hides("tail") {
                            // (A pig's tail tucks in under them.)
                            continue;
                        }
                        for layer in fit.layers.iter().filter(|l| matches!(l.slot, Slot::Legs | Slot::Shirt | Slot::Body)) {
                            for part in &layer.mesh.parts {
                                checked += 1;
                                let n = crossings(&tail, part);
                                if n > 0 {
                                    failures.push(format!("{folk:?} {seed} kit {kit}: the tail passes through the {:?} layer's {} ({n} crossings)", layer.slot, part.name));
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(checked > min_checked, "{checked}");
        assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    }

    #[test]
    #[ignore = "slow sweep: body fitting test, every folk and seed; run before merging fitting changes"]
    fn a_tail_comes_out_through_robes_and_trousers() {
        a_tail_comes_out_through_robes_and_trousers_check(&[Folk::Lizardfolk, Folk::Pigfolk], &[3, 1207, 40, 8], &[10, 11, 12, 13], 300);
    }

    #[test]
    fn a_tail_comes_out_through_robes_and_trousers_smoke() {
        a_tail_comes_out_through_robes_and_trousers_check(&[Folk::Lizardfolk], &[1207], &[10], 10);
    }

    fn ears_and_frills_clear_hoods_and_helms_check(folks: &[Folk], seeds: &[u32], min_checked: usize) {
        use armory::families::helm::HelmStyle;
        use armory::families::hood::Hoods;
        use armory::sections::hood::HoodStyle;
        let mut failures = vec![];
        let mut checked = 0;
        // Goblin and pig ears under hoods and closed helms, a lizard's frill under any.
        let closed = [HelmStyle::Bascinet, HelmStyle::GreatHelm, HelmStyle::Sallet, HelmStyle::Barbute];
        for &folk in folks {
            for &seed in seeds {
                let body = Body::new(&BodySpec::roll(folk, seed));
                let mut heads: Vec<(String, Item)> = HoodStyle::ALL.iter().map(|&st| (format!("{st:?}"), Item::Hood(Hoods::roll_as(seed, st)))).collect();
                let helms = if folk == Folk::Lizardfolk { &HelmStyle::ALL[..] } else { &closed[..] };
                heads.extend(helms.iter().map(|&st| (format!("{st:?}"), Item::Helm(Helm::roll_as(seed, st)))));
                for (name, item) in heads {
                    let fit = fit_worn(&body, &[(Slot::Head, item)]);
                    let head = body.under(&fit);
                    for feature in head.parts.iter().filter(|p| matches!(p.name, "ears" | "frill")) {
                        for part in &fit.layer(Slot::Head).unwrap().mesh.parts {
                            checked += 1;
                            let n = crossings(feature, part);
                            if n > 0 {
                                failures.push(format!("{folk:?} {seed}: its {} pass through the {name}'s {} ({n} crossings)", feature.name, part.name));
                            }
                        }
                    }
                }
            }
        }
        assert!(checked > min_checked, "{checked}");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    #[ignore = "slow sweep: body fitting test, every folk and seed; run before merging fitting changes"]
    fn ears_and_frills_clear_hoods_and_helms() {
        ears_and_frills_clear_hoods_and_helms_check(&[Folk::Goblin, Folk::Pigfolk, Folk::Lizardfolk], &[3, 1207, 40], 100);
    }

    #[test]
    fn ears_and_frills_clear_hoods_and_helms_smoke() {
        ears_and_frills_clear_hoods_and_helms_check(&[Folk::Lizardfolk], &[1207], 10);
    }

    /// The distance from `p` to triangle `t` (Ericson, "Real-Time Collision Detection",
    /// 5.1.5).
    fn point_triangle(p: V3, t: &[V3; 3]) -> f32 {
        let [a, b, c] = *t;
        let (ab, ac, ap) = (vec3::sub(b, a), vec3::sub(c, a), vec3::sub(p, a));
        let (d1, d2) = (vec3::dot(ab, ap), vec3::dot(ac, ap));
        if d1 <= 0.0 && d2 <= 0.0 {
            return vec3::distance(p, a);
        }
        let bp = vec3::sub(p, b);
        let (d3, d4) = (vec3::dot(ab, bp), vec3::dot(ac, bp));
        if d3 >= 0.0 && d4 <= d3 {
            return vec3::distance(p, b);
        }
        let vc = d1 * d4 - d3 * d2;
        if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
            return vec3::distance(p, vec3::add(a, vec3::scale(ab, d1 / (d1 - d3))));
        }
        let cp = vec3::sub(p, c);
        let (d5, d6) = (vec3::dot(ab, cp), vec3::dot(ac, cp));
        if d6 >= 0.0 && d5 <= d6 {
            return vec3::distance(p, c);
        }
        let vb = d5 * d2 - d1 * d6;
        if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
            return vec3::distance(p, vec3::add(a, vec3::scale(ac, d2 / (d2 - d6))));
        }
        let va = d3 * d6 - d5 * d4;
        if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
            return vec3::distance(p, vec3::add(b, vec3::scale(vec3::sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6)))));
        }
        let denom = 1.0 / (va + vb + vc);
        vec3::distance(p, vec3::add(a, vec3::add(vec3::scale(ab, vb * denom), vec3::scale(ac, vc * denom))))
    }

    /// How far `p` is from the surface of `part`, looking no further than `reach`.
    fn distance_to(part: &Part, p: V3, reach: f32) -> f32 {
        let (lo, hi) = (vec3::sub(p, [reach; 3]), vec3::add(p, [reach; 3]));
        triangles_in(part, lo, hi).iter().map(|t| point_triangle(p, t)).fold(reach, f32::min)
    }

    #[test]
    fn a_full_beard_lies_on_mail() {
        use crate::folk::Beard;
        let mut failures = vec![];
        // On a flat chest it lies on the mail all down; over a full bust it hangs from the chin
        // onto the bust's crest and stands off the slope above it, but never passes through.
        for (folk, seed, bust) in [Folk::Dwarf, Folk::Human].into_iter().flat_map(|f| [(3u32, 0.0), (1207, 0.0), (40, 0.0), (3, 1.0)].map(move |(s, b)| (f, s, b))) {
            {
                let mut spec = BodySpec::roll(folk, seed);
                spec.beard = Beard::Full;
                spec.bust = bust;
                let body = Body::new(&spec);
                for kit in [10u32, 11] {
                    let mut mail = Garment::roll_as(kit + 1, GarmentStyle::Hauberk);
                    mail.cut.neck = Neckline::Round;
                    let worn = vec![(Slot::Shirt, Item::Garment(Garment::roll_as(kit, GarmentStyle::Tunic))), (Slot::Body, Item::Garment(mail))];
                    let fit = fit_worn(&body, &worn);
                    let mail = fit.layer(Slot::Body).unwrap().mesh.parts.iter().find(|p| p.name == "body").unwrap().clone();
                    let beard = body.hair_mesh(fit.hair).parts.into_iter().find(|p| p.name == "beard").unwrap();
                    // Down its length, wherever the mail is behind it, the beard's back lies
                    // within 1 cm of the mail and never in it.
                    let lo = beard.positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
                    let mut y = body.chin_y - 0.1;
                    while y > lo + 0.05 {
                        let band: Vec<V3> = beard.positions.iter().filter(|p| (p[1] - y).abs() < 0.05 && p[0].abs() < 0.3 * body.scale).copied().collect();
                        let behind = mail.positions.iter().any(|q| (q[1] - y).abs() < 0.1 && q[0].abs() < 0.3 * body.scale && q[2] > 0.0);
                        if behind && !band.is_empty() && bust == 0.0 {
                            let gap = band.iter().map(|p| distance_to(&mail, *p, 1.0)).fold(f32::MAX, f32::min);
                            if gap > 0.1 {
                                failures.push(format!("{folk:?} {seed} kit {kit}: the beard stands {gap:.3} off the mail at height {y:.2}"));
                            }
                        }
                        y -= 0.1;
                    }
                    let n = crossings(&beard, &mail);
                    if n > 0 {
                        failures.push(format!("{folk:?} {seed} kit {kit}: the beard passes through the mail ({n} crossings)"));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    fn long_hair_rests_on_a_cloak_check(folks: &[Folk], seeds: &[u32], kits: &[u32]) {
        use crate::folk::HairStyle;
        let mut failures = vec![];
        for &folk in folks {
            for &seed in seeds {
                let mut spec = BodySpec::roll(folk, seed);
                spec.hair = HairStyle::Long;
                let body = Body::new(&spec);
                for &kit in kits {
                    let worn = vec![(Slot::Shirt, Item::Garment(Garment::roll_as(kit, GarmentStyle::Tunic))), (Slot::Cloak, Item::Garment(Garment::roll_as(kit + 1, GarmentStyle::Cloak)))];
                    let fit = fit_worn(&body, &worn);
                    let mut hair = body.hair_mesh(fit.hair).parts.into_iter().find(|p| p.name == "hair").unwrap();
                    // What hangs below the chin (a cloak's cowl can meet the scalp itself at
                    // the nape, as it meets the bare head).
                    let chin = body.chin_y;
                    let kept: Vec<u32> = hair.indices.chunks(3).filter(|t| t.iter().all(|&i| hair.positions[i as usize][1] < chin)).flatten().copied().collect();
                    hair.indices = kept;
                    for part in &fit.layer(Slot::Cloak).unwrap().mesh.parts {
                        let n = crossings(&hair, part);
                        if n > 0 {
                            failures.push(format!("{folk:?} {seed} kit {kit}: the hair passes through the cloak's {} ({n} crossings)", part.name));
                        }
                    }
                    // Where it falls past the shoulders it lies on the cloak, not off it: round
                    // the back it comes within 1.2 cm of it in every direction, the edges of
                    // the curtain too.
                    let cloak = &fit.layer(Slot::Cloak).unwrap().mesh.parts;
                    let (a, b) = (body.shoulder_y - 0.25 * body.trunk_length, body.shoulder_y + 0.05 * body.trunk_length);
                    let mut nearest = [f32::MAX; 24];
                    for p in hair.positions.iter().filter(|p| p[1] > a && p[1] < b) {
                        let j = (((p[2].atan2(p[0]) + PI) / TAU * 24.0) as usize).min(23);
                        nearest[j] = nearest[j].min(cloak.iter().map(|c| distance_to(c, *p, 1.0)).fold(1.0, f32::min));
                    }
                    let mut sectors: Vec<f32> = nearest.into_iter().filter(|d| *d < f32::MAX).collect();
                    sectors.sort_by(f32::total_cmp);
                    if sectors.is_empty() || sectors.iter().any(|d| *d > 0.12) {
                        failures.push(format!("{folk:?} {seed} kit {kit}: the hair stands off the cloak: {sectors:.2?}"));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    #[ignore = "slow sweep: body fitting test, every folk and seed; run before merging fitting changes"]
    fn long_hair_rests_on_a_cloak() {
        long_hair_rests_on_a_cloak_check(&[Folk::Human, Folk::Dwarf, Folk::Gnome], &[3, 1207, 40, 8], &[10, 11, 12]);
    }

    #[test]
    fn long_hair_rests_on_a_cloak_smoke() {
        long_hair_rests_on_a_cloak_check(&[Folk::Human, Folk::Dwarf], &[1207], &[10]);
    }

    fn a_bracer_lies_close_over_a_mail_sleeve_check(folks: &[Folk], seeds: &[u32], kits: &[u32]) {
        use armory::families::hands::Hands;
        let mut failures = vec![];
        for &folk in folks {
            for &seed in seeds {
                let body = Body::new(&BodySpec::roll(folk, seed));
                for &kit in kits {
                    let mut mail = Garment::roll_as(kit + 1, GarmentStyle::Hauberk);
                    mail.cut.sleeves = Sleeves::Long;
                    let worn = vec![
                        (Slot::Shirt, Item::Garment(Garment::roll_as(kit, GarmentStyle::Tunic))),
                        (Slot::Body, Item::Garment(mail)),
                        (Slot::Hands, Item::Hands(Hands::roll_as(kit, HandStyle::Bracer))),
                    ];
                    let fit = fit_worn(&body, &worn);
                    let sleeves: Vec<Part> = fit.layer(Slot::Body).unwrap().mesh.parts.iter().filter(|p| p.name == "sleeves").cloned().collect();
                    let cuff = fit.layer(Slot::Hands).unwrap().mesh.parts.iter().find(|p| p.name == "cuff").unwrap().clone();
                    let thickness = 0.03 * body.hand_form().palm_length / 0.95;
                    // Along its length up the forearm (from a little above the wrist, where the
                    // sleeve ends), within 1 cm of the sleeve under it, and never in it.
                    let mut worst = 0.0f32;
                    for side in Side::BOTH {
                        let arm = body.arm(side);
                        let (w, along) = (arm.joints[2], arm.tangent_at(1.0));
                        for p in cuff.positions.iter().filter(|p| vec3::dot(vec3::sub(**p, w), along) < -0.25 && (p[0] > 0.0) == (side == Side::Left)) {
                            let d = sleeves.iter().map(|s| distance_to(s, *p, 1.0)).fold(1.0, f32::min);
                            worst = worst.max(d - thickness);
                        }
                    }
                    if worst > 0.1 {
                        failures.push(format!("{folk:?} {seed} kit {kit}: the bracer stands {worst:.3} off the sleeve"));
                    }
                    for s in &sleeves {
                        let n = crossings(&cuff, s);
                        if n > 0 {
                            failures.push(format!("{folk:?} {seed} kit {kit}: the sleeve passes through the bracer ({n} crossings)"));
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    #[ignore = "slow sweep: body fitting test, every folk and seed; run before merging fitting changes"]
    fn a_bracer_lies_close_over_a_mail_sleeve() {
        a_bracer_lies_close_over_a_mail_sleeve_check(&Folk::ALL, &[3, 1207], &[10, 11, 12]);
    }

    #[test]
    fn a_bracer_lies_close_over_a_mail_sleeve_smoke() {
        a_bracer_lies_close_over_a_mail_sleeve_check(&[Folk::Human, Folk::Dwarf, Folk::Lizardfolk], &[1207], &[10]);
    }

    /// How far along the ray from `origin` toward `dir` the surface of `part` reaches: the
    /// farthest triangle it meets (Moeller-Trumbore).
    fn ray_from(part: &Part, origin: V3, dir: V3) -> Option<f32> {
        part.indices
            .chunks(3)
            .filter_map(|t| {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| part.positions[i as usize]);
                let (e1, e2) = (vec3::sub(b, a), vec3::sub(c, a));
                let h = vec3::cross(dir, e2);
                let det = vec3::dot(e1, h);
                if det.abs() < 1e-9 {
                    return None;
                }
                let s = vec3::sub(origin, a);
                let u = vec3::dot(s, h) / det;
                let q = vec3::cross(s, e1);
                let v = vec3::dot(dir, q) / det;
                let t = vec3::dot(e2, q) / det;
                (u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.0).then_some(t)
            })
            .reduce(f32::max)
    }

    fn legwear_bends_with_the_knees_check(folks: &[Folk], seeds: &[u32], styles: &[LegwearStyle], kits: &[u32]) {
        use armory::families::legs::Legs;
        let mut failures = vec![];
        for &folk in folks {
            for &seed in seeds {
                let body = Body::new(&BodySpec::roll(folk, seed));
                let skin = body.skin_mesh().parts.into_iter().find(|p| p.name == "legs").unwrap();
                for &style in styles {
                    for &kit in kits {
                        let wear = Legs::roll_as(kit, style);
                        let bloused = wear.legwear.puttees;
                        let fit = fit_worn(&body, &[(Slot::Legs, Item::Legs(wear))]);
                        let legs = fit.layer(Slot::Legs).unwrap().mesh.parts.iter().find(|p| p.name == "legs").unwrap().clone();
                        let hem = legs.positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
                        let crotch = body.hip_y - 0.14 * body.trunk_length;
                        // No skin shows through: every point of the bare leg between the hem and
                        // the crotch lies inside the cloth, measured out from the leg's own line.
                        let mut shows = 0;
                        let mut worst = 0.0f32;
                        let mut at = [0.0f32; 3];
                        for p in skin.positions.iter().filter(|p| p[1] > hem + 0.15 && p[1] < crotch - 0.1).step_by(3) {
                            let leg = body.leg(if p[0] > 0.0 { Side::Left } else { Side::Right });
                            let (t, _) = nearest(leg, *p);
                            let c = leg.point_at(t);
                            let dir = vec3::normalize_or(vec3::sub(*p, c), vec3::Z);
                            let skin_r = vec3::distance(*p, c);
                            if let Some(cloth) = ray_from(&legs, c, dir).filter(|cloth| skin_r > *cloth) {
                                shows += 1;
                                if skin_r - cloth > worst {
                                    at = *p;
                                }
                                worst = worst.max(skin_r - cloth);
                            }
                        }
                        // Nor the hips above the crotch, under the waistband.
                        let trunk = body.skin_mesh().parts.into_iter().find(|p| p.name == "trunk").unwrap();
                        let band = body.waist_y - 0.2 * body.scale;
                        // (Off the midline, where the two halves' seams meet and a ray along
                        // it slips between them, and away from a tail's root, which comes out
                        // through a hole in the seat.)
                        let by_tail = |p: V3| body.tail().is_some_and(|(path, radius)| path.iter().enumerate().any(|(i, c)| vec3::distance(*c, p) < radius(i as f32 / (path.len() - 1) as f32) + 0.4));
                        for p in trunk.positions.iter().filter(|p| p[1] > crotch + 0.05 && p[1] < band && p[0].abs() > 0.03 && !by_tail(**p)) {
                            let dir = vec3::normalize_or([p[0], 0.0, p[2]], vec3::Z);
                            let skin_r = p[0].hypot(p[2]);
                            if let Some(cloth) = ray_from(&legs, [0.0, p[1], 0.0], dir).filter(|cloth| skin_r > *cloth) {
                                shows += 1;
                                if skin_r - cloth > worst {
                                    at = *p;
                                }
                                worst = worst.max(skin_r - cloth);
                            }
                        }
                        if shows > 0 {
                            failures.push(format!("{folk:?} {seed} {style:?} kit {kit}: skin shows through at {shows} points, by up to {worst:.3} at {at:?}"));
                        }
                        // And the cloth follows the knee forward: in front of the knee it stands
                        // no further off the skin than in front of the shin below it (unless it
                        // blouses there over puttees).
                        let leg = body.leg(Side::Left);
                        let off = |y: f32| {
                            let t = (0..=200).map(|i| i as f32 / 200.0).min_by(|a, b| (leg.point_at(*a)[1] - y).abs().total_cmp(&(leg.point_at(*b)[1] - y).abs())).unwrap();
                            let c = leg.point_at(t);
                            let cloth = ray_from(&legs, c, vec3::Z).unwrap_or(0.0);
                            let flesh = ray_from(&skin, c, vec3::Z).unwrap_or(0.0);
                            cloth - flesh
                        };
                        let knee = leg.joints[1][1];
                        if knee > hem + 0.6 && !bloused {
                            let (at_knee, at_shin) = (off(knee), off(knee - 0.4 * (knee - hem)));
                            if at_knee > at_shin + 0.1 {
                                failures.push(format!("{folk:?} {seed} {style:?} kit {kit}: the cloth stands {at_knee:.2} off the knee, {at_shin:.2} off the shin"));
                            }
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    #[ignore = "slow sweep: body fitting test, every folk and seed; run before merging fitting changes"]
    fn legwear_bends_with_the_knees() {
        legwear_bends_with_the_knees_check(&Folk::ALL, &[3, 1207], &LegwearStyle::ALL, &[10, 11]);
    }

    #[test]
    fn legwear_bends_with_the_knees_smoke() {
        legwear_bends_with_the_knees_check(&[Folk::Human, Folk::Pigfolk], &[1207], &[LegwearStyle::Trousers, LegwearStyle::Breeches], &[10]);
    }

    #[test]
    fn a_clerics_beard_and_hair_keep_close_under_mail_and_a_helm() {
        use crate::folk::{Beard, HairStyle};
        let mut failures = vec![];
        let cleric = fb_items::outfit::outfit("cleric").unwrap();
        for seed in 1..=8u32 {
            let mut spec = BodySpec::roll(Folk::Human, seed);
            spec.beard = Beard::Full;
            spec.hair = if seed % 2 == 0 { HairStyle::Ponytail } else { HairStyle::Long };
            let body = Body::new(&spec);
            let mut worn: Vec<(Slot, Item)> = cleric.roll(seed, "human").worn.into_iter().filter(|(s, _)| LAYER_ORDER.contains(s)).map(|(s, i)| (s, i.shape)).collect();
            // A mail shirt with pauldrons and a helm with an aventail, as on the doll in #129.
            let mut mail = Garment::roll_as(seed, GarmentStyle::Hauberk);
            mail.plates.pauldrons = true;
            let mut helm = Helm::roll_as(seed, armory::families::helm::HelmStyle::Nasal);
            helm.aventail.on = true;
            worn.retain(|(s, _)| !matches!(s, Slot::Body | Slot::Head));
            worn.push((Slot::Body, Item::Garment(mail)));
            worn.push((Slot::Head, Item::Helm(helm)));
            let fit = fit_worn(&body, &worn);
            // No worn part, nor the hair, reaches more than a hand's width (about 1 unit) past
            // the body's silhouette from the front.
            let mut skin = body.skin_mesh();
            skin.parts.extend(body.head_mesh().parts);
            let (lo, hi) = skin.bounds().unwrap();
            let side = lo[0].abs().max(hi[0]);
            let hair = body.hair_mesh(fit.hair);
            for part in fit.layers.iter().flat_map(|l| l.mesh.parts.iter()).chain(hair.parts.iter()) {
                let wide = part.positions.iter().map(|p| p[0].abs()).fold(0.0f32, f32::max);
                if wide > side + 1.0 {
                    failures.push(format!("seed {seed}: the {} reaches {wide:.2} to the side, the body {side:.2}", part.name));
                }
            }
            // The beard hangs under the chin at its own width, not fanned across the shoulders.
            let width = |mesh: &ItemMesh| mesh.parts.iter().filter(|p| p.name == "beard").flat_map(|p| &p.positions).map(|p| (p[0] - body.head.centre[0]).abs()).fold(0.0f32, f32::max);
            let (dressed, bare) = (width(&hair), width(&body.hair_mesh(HairFit::default())));
            if dressed > bare + 0.1 {
                failures.push(format!("seed {seed}: the beard spreads to {dressed:.2}, {bare:.2} bare"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn one_tunic_fits_a_gnome_and_a_pigfolk() {
        let tunic = Item::Garment(Garment::roll_as(40, GarmentStyle::Tunic));
        for folk in [Folk::Gnome, Folk::Pigfolk] {
            let body = Body::new(&BodySpec::typical(folk));
            let fit = fit_worn(&body, &[(Slot::Shirt, tunic.clone())]);
            let mesh = &fit.layer(Slot::Shirt).unwrap().mesh;
            // From the neck to above the ankles.
            let body_part = mesh.parts.iter().find(|p| p.name == "body").unwrap();
            let (lo, hi) = body_part.positions.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
            // A gnome's neck is short enough that the back of the neckline sits behind the jaw.
            assert!(hi > body.shoulder_y && hi < body.head.centre[1], "{folk:?} top {hi}");
            assert!(lo > body.ankle_y && lo < body.hip_y, "{folk:?} hem {lo}");
            // Snug over the chest and belly: outside the skin, not far off it.
            for y in [body.chest_y, body.waist_y] {
                let skin = body.trunk().point(FRONT, y)[2];
                let cloth = body_part.positions.iter().filter(|p| (p[1] - y).abs() < 0.1 && p[0].abs() < 0.2).map(|p| p[2]).fold(f32::MIN, f32::max);
                assert!(cloth > skin && cloth < skin + 0.6 * body.scale.max(0.5), "{folk:?} at {y}: cloth {cloth} skin {skin}");
            }
            // The sleeves follow this body's arms.
            if let Some(sleeves) = mesh.parts.iter().find(|p| p.name == "sleeves") {
                let arm = body.arm(Side::Left);
                for p in &sleeves.positions {
                    let (t, d) = nearest(arm, *p);
                    assert!(d > arm.radius_at(t) * 0.99 && d < arm.radius_at(t) * 3.0 + 0.5, "{folk:?} sleeve {p:?}");
                }
            }
        }
    }

    #[test]
    fn fitting_is_deterministic() {
        let body = Body::new(&BodySpec::roll(Folk::Dwarf, 9));
        assert_eq!(fit_worn(&body, &outfit(21)), fit_worn(&body, &outfit(21)));
    }

    #[test]
    fn gloves_and_boots_replace_the_bare_hands_and_feet() {
        let body = Body::new(&BodySpec::roll(Folk::Lizardfolk, 2));
        let names = |m: &ItemMesh| m.parts.iter().map(|p| p.name).collect::<Vec<_>>();
        let bare = names(&body.dressed(&[]));
        assert!(bare.contains(&"hands") && bare.contains(&"feet") && bare.contains(&"claws"));
        let dressed = names(&body.dressed(&outfit(3)));
        for gone in ["hands", "nails", "feet", "claws"] {
            assert!(!dressed.contains(&gone), "{gone} in {dressed:?}");
        }
        for part in &body.dressed(&outfit(3)).parts {
            part.validate().unwrap_or_else(|e| panic!("{}: {e}", part.name));
        }
    }

    #[test]
    fn a_helm_fits_over_the_head_and_hair() {
        for body in bodies() {
            let helm = roll(Family::Helm, body.spec.seed);
            let fit = fit_worn(&body, &[(Slot::Head, helm)]);
            assert!(fit.hair.covered);
            let mesh = &fit.layer(Slot::Head).unwrap().mesh;
            let shell = mesh.parts.iter().find(|p| p.name == "shell").unwrap();
            let c = body.head.centre;
            // The bowl's widest ring, at the brow, clears the head's sides.
            let wide = shell.positions.iter().filter(|p| (p[1] - c[1] - 0.1 * body.head.half_height).abs() < 0.15).map(|p| (p[0] - c[0]).abs()).fold(0.0f32, f32::max);
            assert!(wide > body.head.half_width + body.hair_volume(true), "{:?}: {wide} vs {}", body.folk(), body.head.half_width);
        }
    }

    #[test]
    fn a_cloak_over_raised_arms_neither_shelves_nor_spreads_the_beard() {
        use crate::folk::Beard;
        use crate::held::fit_held;
        let mut spec = BodySpec::roll(Folk::Dwarf, 1207);
        spec.beard = Beard::Forked;
        // A staff in one hand and a torch in the other: both forearms raised level.
        let mut body = Body::new(&spec);
        for (side, family, seed) in [(Side::Right, Family::Staff, 3), (Side::Left, Family::Torch, 4)] {
            let fit = fit_held(&body, side, &roll(family, seed)).unwrap();
            body = Body::posed(&spec, fit.pose);
        }
        let worn = vec![
            (Slot::Shirt, Item::Garment(Garment::roll_as(5, GarmentStyle::Robe))),
            (Slot::Cloak, Item::Garment(Garment::roll_as(6, GarmentStyle::Cloak))),
        ];
        let fit = fit_worn(&body, &worn);
        let half_width = |mesh: &ItemMesh, name: &str| {
            mesh.parts.iter().filter(|p| p.name == name).flat_map(|p| &p.positions).map(|p| p[0].abs()).fold(0.0f32, f32::max)
        };
        // The raised forearms come out through the cloak's front rather than pushing it out
        // (measured over them, it stood 3.6 out in front of shoulders 2.45 wide).
        let ahead = fit.layer(Slot::Cloak).unwrap().mesh.parts.iter().filter(|p| p.name == "body").flat_map(|p| &p.positions).map(|p| p[2]).fold(0.0f32, f32::max);
        assert!(ahead < body.shoulder_half_width * 1.2, "the cloak stands {ahead} out in front");
        let beard = half_width(&body.hair_mesh(fit.hair), "beard");
        assert!(beard < body.shoulder_half_width, "the beard fans out to {beard}");
        assert!(fit.hair.chest <= fit.hair.clearance);
    }

    fn standing_ears_clear_open_caps_check(folks: &[Folk], seeds: &[u32], styles: &[HelmStyle], kits: std::ops::Range<u32>, min_checked: usize) {
        let mut failures = vec![];
        let mut checked = 0;
        // A goblin's long ears and a gnome's pointed ones under every open cap, with and
        // without its aventail and cheek plates.
        for &folk in folks {
            for &seed in seeds {
                let body = Body::new(&BodySpec::roll(folk, seed));
                for &style in styles {
                    for kit in kits.clone() {
                        let mut helm = Helm::roll_as(seed + kit, style);
                        helm.aventail.on = kit % 2 == 0;
                        helm.cheeks.on = kit % 4 < 2;
                        let fit = fit_worn(&body, &[(Slot::Head, Item::Helm(helm))]);
                        let ears = body.under(&fit).parts.into_iter().find(|p| p.name == "ears").unwrap();
                        for part in &fit.layer(Slot::Head).unwrap().mesh.parts {
                            checked += 1;
                            let n = crossings(&ears, part);
                            if n > 0 {
                                failures.push(format!("{folk:?} {seed} {style:?} kit {kit}: its ears pass through the {} ({n} crossings)", part.name));
                            }
                        }
                    }
                }
            }
        }
        assert!(checked > min_checked, "{checked}");
        assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    }

    #[test]
    #[ignore = "slow sweep: body fitting test, every folk and seed; run before merging fitting changes"]
    fn standing_ears_clear_open_caps() {
        standing_ears_clear_open_caps_check(&[Folk::Goblin, Folk::Gnome], &[3, 1207, 40, 8, 77, 501], &[HelmStyle::Kettle, HelmStyle::Nasal, HelmStyle::Skullcap], 0..8, 500);
    }

    #[test]
    fn standing_ears_clear_open_caps_smoke() {
        standing_ears_clear_open_caps_check(&[Folk::Goblin, Folk::Gnome], &[1207], &[HelmStyle::Kettle, HelmStyle::Skullcap], 0..2, 10);
    }

    fn a_lizards_snout_and_brow_clear_helms_check(seeds: &[u32], styles: &[HelmStyle], kits: std::ops::Range<u32>, min_checked: usize) {
        let mut failures = vec![];
        let mut checked = 0;
        for &seed in seeds {
            let body = Body::new(&BodySpec::roll(Folk::Lizardfolk, seed));
            for &style in styles {
                for kit in kits.clone() {
                    let mut helm = Helm::roll_as(seed + kit, style);
                    helm.aventail.on = kit % 2 == 0;
                    helm.cheeks.on = kit < 2;
                    let fit = fit_worn(&body, &[(Slot::Head, Item::Helm(helm))]);
                    // The head's skin (snout and brow ridges with it), its eyes, mouth and teeth.
                    for feature in body.head_mesh_fitted(fit.head).parts.iter().filter(|p| p.name != "frill") {
                        for part in &fit.layer(Slot::Head).unwrap().mesh.parts {
                            checked += 1;
                            let n = crossings(feature, part);
                            if n > 0 {
                                failures.push(format!("lizardfolk {seed} {style:?} kit {kit}: its {} passes through the {} ({n} crossings)", feature.name, part.name));
                            }
                        }
                    }
                }
            }
        }
        assert!(checked > min_checked, "{checked}");
        assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    }

    #[test]
    #[ignore = "slow sweep: body fitting test, every folk and seed; run before merging fitting changes"]
    fn a_lizards_snout_and_brow_clear_helms() {
        a_lizards_snout_and_brow_clear_helms_check(&[3, 1207, 40, 8, 77, 501], &HelmStyle::ALL, 0..4, 500);
    }

    #[test]
    fn a_lizards_snout_and_brow_clear_helms_smoke() {
        a_lizards_snout_and_brow_clear_helms_check(&[1207], &[HelmStyle::Bascinet], 0..1, 10);
    }

    /// How far `part` reaches from the left leg's line at height `y` (body space), if it is there.
    fn reach_from_leg(body: &Body, part: &Part, y: f32) -> Option<f32> {
        let sections = body.leg_sections();
        let s = sections.iter().min_by(|a, b| (a.y - y).abs().total_cmp(&(b.y - y).abs()))?;
        part.positions.iter().filter(|p| p[0] > 0.0 && (p[1] - y).abs() < 0.08).map(|p| (p[0] - s.x).hypot(p[2] - s.z)).reduce(f32::max)
    }

    /// How wide `mesh`'s foot and sole are across the left foot, below `below`.
    fn across_foot(body: &Body, mesh: &ItemMesh, below: f32) -> f32 {
        let at = body.foot_placement(Side::Left);
        let side = at.z();
        let (lo, hi) = mesh
            .parts
            .iter()
            .filter(|p| matches!(p.name, "foot" | "sole"))
            .flat_map(|p| &p.positions)
            .filter(|p| p[0] > 0.0 && p[1] < below)
            .map(|p| vec3::dot(vec3::sub(*p, at.origin), side))
            .fold((f32::MAX, f32::MIN), |(lo, hi), d| (lo.min(d), hi.max(d)));
        hi - lo
    }

    #[test]
    fn boots_fit_close_to_the_foot_and_shin() {
        use armory::families::feet::Feet;
        use armory::families::legs::Legs;
        for folk in Folk::ALL {
            for seed in [3u32, 1207] {
                let body = Body::new(&BodySpec::roll(folk, seed));
                let mut boot = Feet::roll_as(seed, BootStyle::Boot);
                boot.boot.height = 3.6;
                boot.boot.cuff = 0.0;
                let width = boot.boot.width;
                let worn = [(Slot::Legs, Item::Legs(Legs::roll_as(seed, LegwearStyle::Trousers))), (Slot::Feet, Item::Feet(boot))];
                let fit = fit_worn(&body, &worn);
                let feet = &fit.layer(Slot::Feet).unwrap().mesh;
                let trousers = &fit.layer(Slot::Legs).unwrap().mesh;
                let shafts: Vec<&Part> = feet.parts.iter().filter(|p| p.name == "shaft").collect();
                let top = shafts.iter().flat_map(|p| &p.positions).map(|p| p[1]).fold(f32::MIN, f32::max);
                let form = body.leg_form();
                // Up the shaft, above the heel, it stands no more than 2 cm off the trousers under it.
                for k in 1..8 {
                    let y0 = 1.6 * form.instep_height;
                    let y = y0 + (top - 0.3 * body.scale - y0) * k as f32 / 8.0;
                    let boot = shafts.iter().filter_map(|p| reach_from_leg(&body, p, y)).fold(0.0f32, f32::max);
                    let cloth = trousers.parts.iter().filter_map(|p| reach_from_leg(&body, p, y)).fold(0.0f32, f32::max);
                    assert!(boot - cloth < 0.2, "{folk:?} {seed} at {y:.2}: boot {boot:.3}, trousers {cloth:.3}");
                }
                // Across the foot, within 2.5 cm of the bare foot at the boot's own width.
                let mut bare = ItemMesh { parts: vec![armory::build::boot::foot_mesh(&form, 0.0).into_part("foot", armory::geom::OutlineMode::Welded)] };
                body.foot_placement(Side::Left).apply_to_item(&mut bare);
                let (shod, bare) = (across_foot(&body, feet, form.instep_height), across_foot(&body, &bare, form.instep_height));
                assert!(shod - bare * width < 0.25, "{folk:?} {seed}: boot {shod:.3} across, bare foot {bare:.3} x {width:.2}");
            }
        }
    }

    #[test]
    fn robes_hang_to_just_above_the_feet() {
        for folk in Folk::ALL {
            for seed in [3u32, 1207] {
                let body = Body::new(&BodySpec::roll(folk, seed));
                for kit in [1u32, 40] {
                    let robe = Item::Garment(Garment::roll_as(kit, GarmentStyle::Robe));
                    let fit = fit_worn(&body, &[(Slot::Shirt, robe)]);
                    let skirt = &fit.layer(Slot::Shirt).unwrap().mesh;
                    let hem = skirt.parts.iter().filter(|p| p.name == "body").flat_map(|p| &p.positions).map(|p| p[1]).fold(f32::MAX, f32::min);
                    // Below the ankle's top, the folds' wave aside, and clear of the sole.
                    assert!(hem > 0.5 * body.ankle_y && hem < body.ankle_y + 0.35 * body.scale, "{folk:?} {seed} robe {kit}: hem at {hem:.2}, ankle {:.2}", body.ankle_y);
                }
            }
        }
    }

    #[test]
    fn a_robe_hides_the_belts_pouch() {
        let body = Body::new(&BodySpec::typical(Folk::Human));
        let mut belt = Belts::roll(3);
        belt.fittings.pouch = true;
        let pouch = |shirt: GarmentStyle| {
            let fit = fit_worn(&body, &[(Slot::Shirt, Item::Garment(Garment::roll_as(3, shirt))), (Slot::Belt, Item::Belt(belt.clone()))]);
            fit.layer(Slot::Belt).unwrap().mesh.parts.iter().any(|p| p.name == "pouch")
        };
        assert!(pouch(GarmentStyle::Tunic) && !pouch(GarmentStyle::Robe));
    }
}
