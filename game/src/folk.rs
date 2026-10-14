//! Folk met in the level: each [`FolkSpot`] drawn as a dressed `fb_body` figure, standing on
//! the floor at its spot, lit by the level's lights.
//!
//! - [`Person::of`] says who stands at a spot: their body ([`BodySpec::roll`]), a name from
//!   `data/syl.json`, and an outfit from their faction's leaning ([`fb_level::FACTIONS`])
//!   rolled through `fb_items` ([`fb_items::outfit::Outfit::roll`], which also gives a bare-headed gnome a pointed hat as the plate does). Everyone holds something: a weapon, a light or
//!   both.
//! - [`Figure::of`] fits it all to the body: held gear first ([`fit_held`], each hand posing
//!   its arm, [`Body::posed`]), then what is worn ([`fit_worn`]). Its meshes split into the
//!   body and the head (the head, hair, and a helm or hat), which turns about the neck.
//! - Each item is drawn with its own look (palette, condition, etching) by the
//!   [`ItemSpawner`], in the house style: toon bands, ink and decay. Skin carries the person's
//!   dirt. A lit torch or lantern in hand lights the room round them.
//!
//! Figures are built once per spot, in parallel, and kept in [`FolkFigures`], so the same
//! level shows the same people without building them again.

use std::sync::Arc;

use armory::{ItemMesh, build::light::LightAnchor, cond::Condition};
use bevy::{app::Propagate, light::NotShadowCaster, platform::collections::HashMap, prelude::*, tasks::ComputeTaskPool};
use fb_body::{Body, BodySpec, HeldFit, Side, WornFit, fit_held, fit_worn};
use fb_items::{Item, Slot, roll_item};
use fb_level::{FolkSpot, Level};
use procgen_rng::Mulberry32;

use crate::carry::{appearance_of, item_light, world_light};
use crate::level::EXPOSURE;
use crate::play::CurrentLevel;
use crate::render::{ItemAppearance, ItemSpawner};

pub struct FolkPlugin;

impl Plugin for FolkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FolkFigures>().add_systems(Update, spawn_folk.run_if(resource_exists_and_changed::<CurrentLevel>));
    }
}

/// Salts for a person's name and dirt, drawn apart from their body and outfit. Our own.
const NAME_SALT: u32 = 0x4E41_4D45;
const DIRT_SALT: u32 = 0xD127_0001;

/// How far above a held flame its light hangs, in world units (10 cm).
const HELD_LIGHT_LIFT: f32 = 1.0;

/// Who stands at a spot.
#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    pub spot: FolkSpot,
    pub name: String,
    pub spec: BodySpec,
    /// The key of the outfit they dress from (`data/outfits.json`).
    pub outfit: &'static str,
    /// Everything worn and held, by slot.
    pub gear: Vec<(Slot, Item)>,
    /// How dirty their skin is, 0 clean to about 0.85 (the plate's `C.dirt`).
    pub dirt: f32,
}

impl Person {
    /// The person at `spot`: a pure function of it.
    pub fn of(spot: &FolkSpot) -> Person {
        let faction = spot.faction.row();
        let mut rng = Mulberry32::new(spot.seed ^ NAME_SALT);
        let name = fb_items::naming::folk_name(&mut rng, spot.folk.key());
        let outfit = *rng.pick(faction.outfits);
        let mut gear = fb_items::outfit::outfit(outfit).map_or_else(Vec::new, |o| o.roll(spot.seed, spot.folk.key()).worn);
        let has = |gear: &[(Slot, Item)], slot| gear.iter().any(|(s, _)| *s == slot);
        // Nobody stands about empty-handed: a torch if the outfit gave nothing to hold.
        if !has(&gear, Slot::Mainhand) && !has(&gear, Slot::Offhand) {
            gear.extend(roll_item("Torch", spot.seed.wrapping_add(2)).map(|torch| (Slot::Mainhand, torch)));
        }
        // The plate's `rollChar`: scavengers are filthy, the freshly fallen grubby, the rest
        // a little dirty.
        let mut dirt_rng = Mulberry32::new(spot.seed ^ DIRT_SALT);
        let dirt = match outfit {
            "scavenger" => dirt_rng.range_f32(0.55, 0.85),
            "fallen" => dirt_rng.range_f32(0.3, 0.5),
            _ => dirt_rng.range_f32(0.08, 0.38),
        };
        Person { spot: *spot, name, spec: BodySpec::roll(spot.folk, spot.seed), outfit, gear, dirt }
    }

    /// What they wear, innermost slots first as listed.
    pub fn worn(&self) -> impl Iterator<Item = &(Slot, Item)> {
        self.gear.iter().filter(|(slot, _)| !matches!(slot, Slot::Mainhand | Slot::Offhand))
    }

    /// What they hold.
    pub fn held(&self) -> impl Iterator<Item = &(Slot, Item)> {
        self.gear.iter().filter(|(slot, _)| matches!(slot, Slot::Mainhand | Slot::Offhand))
    }

    pub fn item(&self, slot: Slot) -> Option<&Item> {
        self.gear.iter().find(|(s, _)| *s == slot).map(|(_, item)| item)
    }
}

/// The hand each holding slot is held in.
pub fn hand_of(slot: Slot) -> Option<Side> {
    match slot {
        Slot::Mainhand => Some(Side::Right),
        Slot::Offhand => Some(Side::Left),
        _ => None,
    }
}

/// One piece of a figure: a mesh in body space and how it looks.
#[derive(Clone, Debug)]
pub struct Piece {
    pub name: String,
    pub mesh: ItemMesh,
    pub appearance: ItemAppearance,
    /// Whether it casts a shadow. A lit torch or lantern does not, or its own flame and body
    /// would black out its light (as for lit things on the floor, `crate::carry`).
    pub shadow: bool,
}

/// A light a figure carries: where its heart is in body space, and what light it gives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeldLight {
    pub at: Vec3,
    pub anchor: LightAnchor,
}

/// A person fitted and built, in body space (soles on y = 0, face +Z).
#[derive(Clone, Debug)]
pub struct Figure {
    pub body: Body,
    /// What each hand holds, as fitted.
    pub held: Vec<(Slot, HeldFit)>,
    pub worn: WornFit,
    /// Skin, held gear and everything worn below the head.
    pub torso: Vec<Piece>,
    /// What turns with the head: head, hair, and a helm or hat. Empty under a hood, whose cape
    /// lies on the shoulders, so the whole figure turns a little instead.
    pub head: Vec<Piece>,
    /// Where the head turns, in body space: the base of the neck.
    pub neck: Vec3,
    /// The lights its hands hold.
    pub lights: Vec<HeldLight>,
}

impl Figure {
    /// Fits `person`'s gear to their body and builds every mesh.
    pub fn of(person: &Person) -> Figure {
        let spec = &person.spec;
        let mut body = Body::new(spec);
        let mut held = Vec::new();
        for (slot, item) in person.held() {
            let Some(side) = hand_of(*slot) else { continue };
            if let Some(fit) = fit_held(&body, side, &item.shape) {
                body = Body::posed(spec, fit.pose);
                held.push((*slot, fit));
            }
        }
        let worn_items: Vec<(Slot, armory::Item)> = person.worn().map(|(slot, item)| (*slot, item.shape.clone())).collect();
        let worn = fit_worn(&body, &worn_items);

        let flesh = ItemAppearance {
            seed: spec.seed,
            condition: Condition { wear: person.dirt, ..Condition::default() },
            ..default()
        };
        let mut skin = body.skin_mesh();
        skin.parts.retain(|p| !worn.covered.hides(p.name));
        let mut torso = vec![Piece { name: "skin".into(), mesh: skin, appearance: flesh, shadow: true }];
        let hooded = matches!(person.item(Slot::Head).map(|i| &i.shape), Some(armory::Item::Hood(_)));
        let mut head = vec![
            Piece { name: "head".into(), mesh: body.head_mesh(), appearance: flesh, shadow: true },
            Piece { name: "hair".into(), mesh: body.hair_mesh(worn.hair), appearance: flesh, shadow: true },
        ];
        for layer in &worn.layers {
            let Some(item) = person.item(layer.slot) else { continue };
            let piece = Piece { name: item.title().to_string(), mesh: layer.mesh.clone(), appearance: appearance_of(&item.shape), shadow: true };
            if layer.slot == Slot::Head && !hooded { head.push(piece) } else { torso.push(piece) }
        }
        let mut lights = Vec::new();
        for (slot, fit) in &held {
            let name = person.item(*slot).map_or_else(String::new, |i| i.title().to_string());
            let light = item_light(&fit.item);
            lights.extend(light.map(|anchor| HeldLight { at: Vec3::from_array(fit.to_body(anchor.position)), anchor }));
            torso.push(Piece { name, mesh: fit.mesh.clone(), appearance: appearance_of(&fit.item), shadow: light.is_none() });
        }
        if hooded {
            torso.append(&mut head);
        }
        // A bald head has no hair to draw.
        torso.retain(|p| !p.mesh.parts.is_empty());
        head.retain(|p| !p.mesh.parts.is_empty());
        let neck = Vec3::from_array(body.neck_base().origin);
        Figure { body, held, worn, torso, head, neck, lights }
    }

    /// The figure's bounds in body space, every piece included.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let points = self.torso.iter().chain(&self.head).flat_map(|p| &p.mesh.parts).flat_map(|part| &part.positions).map(|&p| Vec3::from_array(p));
        points.fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(p), hi.max(p)))
    }
}

/// Figures already built, by spot seed, so a level drawn again reuses them.
#[derive(Resource, Default)]
pub struct FolkFigures(HashMap<u32, Arc<(Person, Figure)>>);

impl FolkFigures {
    /// The person and figure for every spot in `spots`, building (in parallel) those not yet
    /// built.
    pub fn for_spots(&mut self, spots: &[FolkSpot]) -> Vec<Arc<(Person, Figure)>> {
        let missing: Vec<FolkSpot> = spots.iter().filter(|s| !self.0.get(&s.seed).is_some_and(|built| built.0.spot == **s)).copied().collect();
        let built = ComputeTaskPool::get_or_init(Default::default).scope(|scope| {
            for spot in &missing {
                scope.spawn(async move {
                    let person = Person::of(spot);
                    let figure = Figure::of(&person);
                    (person, figure)
                });
            }
        });
        for pair in built {
            self.0.insert(pair.0.spot.seed, Arc::new(pair));
        }
        spots.iter().filter_map(|s| self.0.get(&s.seed).cloned()).collect()
    }
}

/// A person standing in the level: the root of their figure, on the floor at their spot.
#[derive(Component, Clone, Debug)]
pub struct Denizen {
    pub person: Person,
}

/// The part of a figure that turns with the head, pivoting at the neck.
#[derive(Component, Debug)]
pub struct FolkHead;

/// The world transform of someone standing at `spot`: on the floor, facing the spot's yaw.
/// The body faces +Z and [`FolkSpot::yaw`] is a camera's yaw (0 faces -Z), so it turns a half
/// turn more.
pub fn stand_at(spot: &FolkSpot) -> Transform {
    let at = Vec3::from_array(Level::world(spot.x, spot.y));
    Transform::from_translation(at).with_rotation(Quat::from_rotation_y(spot.yaw + std::f32::consts::PI))
}

/// Spawns every folk spot of the level once, and again only if the spots change.
fn spawn_folk(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    mut figures: ResMut<FolkFigures>,
    old: Query<Entity, With<Denizen>>,
    mut spawner: ItemSpawner,
    mut shown: Local<Option<Vec<FolkSpot>>>,
) {
    if shown.as_deref() == Some(&level.0.folk[..]) {
        return;
    }
    for entity in &old {
        commands.entity(entity).despawn();
    }
    for built in figures.for_spots(&level.0.folk) {
        let (person, figure) = &*built;
        spawn_figure(&mut commands, &mut spawner, person, figure);
    }
    *shown = Some(level.0.folk.clone());
}

/// Spawns one figure: a root at the spot, the body's pieces under it, and the head's under a
/// pivot at the neck.
pub fn spawn_figure(commands: &mut Commands, spawner: &mut ItemSpawner, person: &Person, figure: &Figure) -> Entity {
    let root = commands
        .spawn((Name::new(person.name.clone()), Denizen { person: person.clone() }, stand_at(&person.spot), Visibility::default()))
        .id();
    let head = commands.spawn((FolkHead, Transform::from_translation(figure.neck), Visibility::default(), ChildOf(root))).id();
    for piece in &figure.torso {
        let mut entity = spawner.spawn(&piece.mesh, &piece.appearance);
        entity.insert((Name::new(piece.name.clone()), ChildOf(root)));
        if !piece.shadow {
            entity.insert(Propagate(NotShadowCaster));
        }
    }
    for piece in &figure.head {
        spawner.spawn(&piece.mesh, &piece.appearance).insert((Name::new(piece.name.clone()), Transform::from_translation(-figure.neck), ChildOf(head)));
    }
    for light in &figure.lights {
        // A little above the flame, so the hand and the stone under it do not burn white.
        commands.spawn((world_light(&light.anchor, &EXPOSURE), Transform::from_translation(light.at + Vec3::Y * HELD_LIGHT_LIFT), ChildOf(root)));
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folk_1207() -> Vec<FolkSpot> {
        fb_level::generate(1207).folk
    }

    #[test]
    fn the_same_spot_is_the_same_person() {
        for spot in folk_1207() {
            assert_eq!(Person::of(&spot), Person::of(&spot));
        }
    }

    #[test]
    fn everyone_on_1207_is_dressed_and_holds_something() {
        for spot in folk_1207() {
            let person = Person::of(&spot);
            let figure = Figure::of(&person);
            assert!(!figure.held.is_empty(), "{} holds nothing: {:?}", person.name, person.gear.iter().map(|g| &g.1.name).collect::<Vec<_>>());
            assert!(!figure.worn.layers.is_empty(), "{} wears nothing", person.name);
            assert!(figure.head.iter().chain(&figure.torso).all(|p| !p.mesh.parts.is_empty()), "{}", person.name);
            // Standing on the floor, about as tall as their folk.
            let (lo, hi) = figure.bounds();
            assert!(lo.y > -0.5 && lo.y < 0.5, "{}: lowest point {}", person.name, lo.y);
            assert!(hi.y > spot.folk.race().h * 0.85, "{}: {} tall", person.name, hi.y);
        }
    }

    #[test]
    fn faction_outfits_name_real_outfits() {
        for row in fb_level::FACTIONS {
            for key in row.outfits {
                assert!(fb_items::outfit::outfit(key).is_some(), "{}: {key}", row.name);
            }
        }
    }

    #[test]
    fn figures_are_built_once_per_spot() {
        let spots = folk_1207();
        let mut figures = FolkFigures::default();
        let first = figures.for_spots(&spots[..2]);
        let again = figures.for_spots(&spots[..2]);
        assert!(first.iter().zip(&again).all(|(a, b)| Arc::ptr_eq(a, b)));
        assert_eq!(first[0].0.spot, spots[0]);
    }

    #[test]
    fn folk_stand_on_their_spot_facing_its_yaw() {
        let spot = folk_1207()[0];
        let t = stand_at(&spot);
        assert_eq!(t.translation, Vec3::new(spot.x * fb_level::CELL, 0.0, spot.y * fb_level::CELL));
        // The body's face (+Z) points the way the spot faces.
        let face = t.rotation * Vec3::Z;
        let (dx, dy) = spot.facing();
        assert!(face.abs_diff_eq(Vec3::new(dx, 0.0, dy), 1e-5), "{face} vs ({dx}, {dy})");
    }
}
