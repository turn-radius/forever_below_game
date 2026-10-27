//! What fire does to a troll's body: flames on it, and char in its skin.
//!
//! - **Flames.** A burned troll (`Regeneration::is_stopped`, until its next turn) has
//!   [`FLAMES`] Armory flames standing on the broadest of its bones, the same `flame` and
//!   `flame_core` parts a torch has (`armory::build::light::add_flame`), animated as every other
//!   flame (`crate::render::flame`) and so rising however the body lies. When its turn comes they
//!   gutter out, shrinking over [`GUTTER`] seconds.
//! - **Char.** Its skin darkens toward black ([`CHAR`]): each burn takes the [`Scorch`] one step
//!   further ([`BURN_STEPS`]), and a living troll's scorch fades as it regains hit points
//!   ([`Scorch::heal`]). Dead for good, it is fully charred, a dull ember glow in its skin
//!   dying down over [`EMBER_SECONDS`], and it stays charred.
//! - **The hit flash** also tints the materials, and comes back to the colours the flinch saved. The
//!   char writes into those saved colours while a flinch is under way
//!   ([`crate::combat::feedback::Flinch`]), so a flash on a charred troll fades back to the char
//!   and never the green. A troll's materials are its own: spawning builds them per part, so
//!   charring one never tints another.
//! - **Persisting.** A body left behind is recorded with its char
//!   ([`crate::record::CreatureRecord::charred`]) and spawned charred, no longer smouldering.
//!
//! `FB_BURN_TROLL=1` fells the `FB_SPAWN=troll` and burns it where it lies, `=scorch` burns it
//! standing (README, "Debug switches"), for captures.

use bevy::{app::Propagate, light::NotShadowCaster, prelude::*};

use super::skeletons::WornGear;
use super::trolls::Troll;
use super::{Bones, Critter, HitShape};
use crate::combat::feedback::{Burned, Flinch, Struck};
use crate::play::CurrentLevel;
use crate::render::{ItemAppearance, ItemSpawner, ToonMaterial};

/// How many flames stand on a burning troll.
pub const FLAMES: usize = 7;
/// Seconds a flame takes to gutter out once the troll's turn comes.
pub const GUTTER: f32 = 1.0;
/// Seconds a flame takes to rise when lit.
const KINDLE: f32 = 0.3;

/// How far gone each burn takes a troll's skin, 0 untouched to 1 black: the first, then the
/// second, then every one after. Our own.
pub const BURN_STEPS: [f32; 3] = [0.55, 0.8, 0.93];
/// What the skin chars toward.
pub const CHAR: Color = Color::srgb(0.11, 0.095, 0.085);
/// How fast the shown char follows the troll's scorch, per second.
const CHAR_EASE: f32 = 1.5;
/// How long a troll dead for good glows, seconds, and how bright (the toon material's `glow`).
pub const EMBER_SECONDS: f32 = 8.0;
const EMBER_GLOW: f32 = 0.3;
/// The ember's colour, mixed into the char while it glows.
const EMBER: Color = Color::srgb(0.9, 0.3, 0.05);
const EMBER_MIX: f32 = 0.3;
/// The share of its hit points a troll regains to lose all its scorch. Our own.
const HEAL_SHARE: f32 = 0.5;

/// On a troll: how burned its skin is.
#[derive(Component, Clone, Debug, Default)]
pub struct Scorch {
    /// How burned it is, 0 to 1: what the skin is charring toward.
    pub level: f32,
    /// How burned it looks, easing to `level`.
    shown: f32,
    /// Dead for good, the glow left in it, 1 to 0.
    pub ember: f32,
    /// Each part's own material, colour and glow before any char.
    parts: Vec<(Handle<ToonMaterial>, Color, f32)>,
}

impl Scorch {
    /// A body brought back from the record: charred as it was, no longer glowing.
    pub fn restored(level: f32) -> Scorch {
        Scorch { level, shown: level, ..default() }
    }

    /// One more burn: the next step of [`BURN_STEPS`] above where it is.
    pub fn burn(&mut self) {
        self.level = burned_to(self.level);
    }

    /// It is dead for good: charred through, and glowing for a while.
    pub fn die(&mut self) {
        self.level = 1.0;
        self.ember = 1.0;
    }

    /// It regained `hp` of `max` hit points: the scorch fades, wholly once it has regained
    /// [`HEAL_SHARE`] of them.
    pub fn heal(&mut self, hp: u32, max: u32) {
        self.level = (self.level - hp as f32 / (max.max(1) as f32 * HEAL_SHARE)).max(0.0);
    }

    /// How burned it looks now.
    pub fn shown(&self) -> f32 {
        self.shown
    }
}

/// Where a burn takes a scorch of `level`: the first of [`BURN_STEPS`] above it, or no further
/// than the last.
pub fn burned_to(level: f32) -> f32 {
    BURN_STEPS.iter().copied().find(|&step| step > level + 1e-3).unwrap_or(BURN_STEPS[BURN_STEPS.len() - 1]).max(level)
}

/// `own` charred `share` of the way (0 to 1) to black, with an ember of `ember` (1 to 0) in
/// it: its colour, and the glow to add.
pub fn charred(own: Color, share: f32, ember: f32) -> (Color, f32) {
    let (share, ember) = (share.clamp(0.0, 1.0), ember.clamp(0.0, 1.0));
    let lit = own.mix(&CHAR, share).mix(&EMBER, EMBER_MIX * ember * share);
    (lit.with_alpha(own.alpha()), EMBER_GLOW * ember)
}

/// What is not the troll's own body: what it wears, and a flame on it.
type NotBody = Or<(With<WornGear>, With<Fire>)>;

/// The toon materials of `root`'s body.
fn body_materials(root: Entity, children: &Query<&Children>, materials: &Query<&MeshMaterial3d<ToonMaterial>>, skip: &Query<(), NotBody>) -> Vec<Handle<ToonMaterial>> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if entity != root && skip.contains(entity) {
            continue;
        }
        if let Ok(material) = materials.get(entity) {
            found.push(material.0.clone());
        }
        if let Ok(kids) = children.get(entity) {
            stack.extend(kids.iter());
        }
    }
    found
}

/// Chars each troll's skin to its scorch, and lights and cools the ember of one dead for good.
#[allow(clippy::type_complexity)]
pub fn char_trolls(
    time: Res<Time>,
    mut trolls: Query<(Entity, &mut Scorch, Option<&mut Flinch>), With<Troll>>,
    children: Query<&Children>,
    materials: Query<&MeshMaterial3d<ToonMaterial>>,
    skip: Query<(), NotBody>,
    toon: Option<ResMut<Assets<ToonMaterial>>>,
) {
    let Some(mut toon) = toon else { return };
    let dt = time.delta_secs();
    for (entity, mut scorch, mut flinch) in &mut trolls {
        if scorch.level <= 0.0 && scorch.shown <= 0.0 {
            continue;
        }
        let first = scorch.parts.is_empty();
        if first {
            // Its own colours: its flinch's if it is mid-flash, else as they are.
            scorch.parts = body_materials(entity, &children, &materials, &skip)
                .into_iter()
                .filter_map(|handle| {
                    let saved = flinch.as_ref().and_then(|f| f.materials.iter().find(|m| m.0 == handle).map(|m| (m.1, m.2)));
                    let (colour, glow) = saved.or_else(|| toon.get(&handle).map(|t| (t.base.base_color, t.extension.glow)))?;
                    Some((handle, colour, glow))
                })
                .collect();
        }
        let before = (scorch.shown, scorch.ember);
        let target = scorch.level;
        scorch.shown = if scorch.shown < target { (scorch.shown + CHAR_EASE * dt).min(target) } else { (scorch.shown - CHAR_EASE * dt).max(target) };
        scorch.ember = (scorch.ember - dt / EMBER_SECONDS).max(0.0);
        if !first && before == (scorch.shown, scorch.ember) {
            continue;
        }
        let (share, ember) = (scorch.shown, scorch.ember);
        for (handle, own, glow) in &scorch.parts {
            let (colour, add) = charred(*own, share, ember);
            // Mid-flash, the colours the flash comes back to are these.
            if let Some(saved) = flinch.as_mut().and_then(|f| f.materials.iter_mut().find(|m| m.0 == *handle)) {
                saved.1 = colour;
                saved.2 = glow + add;
            } else if let Some(mut material) = toon.get_mut(handle) {
                material.base.base_color = colour;
                material.extension.glow = glow + add;
            }
        }
    }
}

/// On a flame standing on a burning troll: how long it has burned, and how long it has left to
/// gutter once the troll's turn has come.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Fire {
    pub age: f32,
    pub guttering: Option<f32>,
}

impl Fire {
    /// How big it stands, 0 to 1: rising when lit, shrinking as it gutters.
    pub fn size(&self) -> f32 {
        let rise = (self.age / KINDLE).clamp(0.0, 1.0);
        let fall = self.guttering.map_or(1.0, |left| (left / GUTTER).clamp(0.0, 1.0));
        rise.min(fall)
    }
}

/// On a troll with flames on it: their entities.
#[derive(Component, Clone, Debug, Default)]
pub struct Aflame(pub Vec<Entity>);

/// The flame of a bone `radius` wide: an Armory flame (`flame` and `flame_core`) standing on the
/// origin, as tall as the body is broad.
fn flame_mesh(radius: f32, seed: u32) -> armory::ItemMesh {
    let mut parts = armory::build::parts::Parts::new();
    armory::build::light::add_flame(&mut parts, [0.0; 3], (radius * 3.4).max(5.0), (radius * 0.8).max(1.5), 0.15, seed);
    parts.finish()
}

/// The bones the flames stand on: the broadest [`FLAMES`] of the troll's, each with the middle
/// of its hit capsule and its radius.
fn flame_sites(bones: &Bones, shapes: &Query<&HitShape>) -> Vec<(Entity, Vec3, f32)> {
    let mut sites: Vec<(Entity, Vec3, f32)> = bones
        .0
        .iter()
        .filter_map(|&bone| {
            let shape = shapes.get(bone).ok()?;
            let middle = (Vec3::from_array(shape.0.a) + Vec3::from_array(shape.0.b)) * 0.5;
            Some((bone, middle, shape.0.radius))
        })
        .collect();
    sites.sort_by(|a, b| b.2.total_cmp(&a.2));
    sites.truncate(FLAMES);
    sites
}

/// Sets flames on a troll while it burns, and lets them gutter out when its turn comes.
#[allow(clippy::type_complexity)]
pub fn light_flames(
    time: Res<Time>,
    mut spawner: ItemSpawner,
    mut trolls: Query<(Entity, &Troll, &Bones, Option<&mut Aflame>)>,
    shapes: Query<&HitShape>,
    mut fires: Query<(&mut Fire, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, troll, bones, aflame) in &mut trolls {
        let burning = troll.regeneration.is_stopped() && !troll.spent;
        match aflame {
            None if burning => {
                let mut lit = Vec::new();
                for (index, (bone, middle, radius)) in flame_sites(bones, &shapes).into_iter().enumerate() {
                    let mesh = flame_mesh(radius, (troll.height * 100.0) as u32 + index as u32);
                    let mut flame = spawner.spawn(&mesh, &ItemAppearance::default());
                    flame.insert((Name::new("body flame"), Fire::default(), Transform::from_translation(middle).with_scale(Vec3::splat(0.01)), ChildOf(bone), Propagate(NotShadowCaster)));
                    lit.push(flame.id());
                }
                spawner.commands().entity(entity).insert(Aflame(lit));
            }
            None => {}
            Some(mut aflame) => {
                aflame.0.retain(|&flame| fires.contains(flame));
                for &flame in &aflame.0 {
                    let Ok((mut fire, _)) = fires.get_mut(flame) else { continue };
                    match (burning, fire.guttering) {
                        (false, None) => fire.guttering = Some(GUTTER),
                        (true, Some(_)) => fire.guttering = None,
                        _ => {}
                    }
                }
                if aflame.0.is_empty() {
                    spawner.commands().entity(entity).remove::<Aflame>();
                }
            }
        }
    }
    for (mut fire, mut transform) in &mut fires {
        fire.age += dt;
        if let Some(left) = fire.guttering.as_mut() {
            *left -= dt;
        }
        transform.scale = Vec3::splat(fire.size().max(0.01));
    }
}

/// Puts out the flames that have guttered away.
pub fn douse_flames(mut commands: Commands, fires: Query<(Entity, &Fire)>) {
    for (entity, fire) in &fires {
        if fire.guttering.is_some_and(|left| left <= 0.0) {
            commands.entity(entity).despawn();
        }
    }
}

/// `FB_BURN_TROLL`: the capture hook. Waits for the `FB_SPAWN` troll to settle, then fells it and
/// burns it where it lies (`1`), or burns it standing (`scorch`).
pub fn dev_burn(
    mut frame: Local<u32>,
    level: Res<CurrentLevel>,
    trolls: Query<(Entity, &Critter, &Transform), With<Troll>>,
    mut struck: MessageWriter<Struck>,
    mut burned: MessageWriter<Burned>,
) {
    let Some(mode) = std::env::var("FB_BURN_TROLL").ok().filter(|m| !m.is_empty()) else { return };
    *frame += 1;
    let (fell_at, burn_at) = if mode == "scorch" { (u32::MAX, 120) } else { (120, 240) };
    if *frame != fell_at && *frame != burn_at {
        return;
    }
    let dev = super::dev_spots(&level.0);
    for (entity, critter, at) in &trolls {
        if !dev.contains(&critter.spot) {
            continue;
        }
        if *frame == fell_at {
            struck.write(Struck { target: entity, damage: 500, from: at.translation.xz() + Vec2::Y * 5.0, bludgeoning: true, critical: false, by_ally: false, graze: false });
        } else {
            burned.write(Burned { target: entity });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_burn_goes_further_and_the_last_step_holds() {
        let mut scorch = Scorch::default();
        let mut levels = Vec::new();
        for _ in 0..5 {
            scorch.burn();
            levels.push(scorch.level);
        }
        assert_eq!(levels, [0.55, 0.8, 0.93, 0.93, 0.93]);
    }

    #[test]
    fn a_faded_scorch_burns_back_up_from_where_it_is() {
        let mut scorch = Scorch::default();
        scorch.burn();
        scorch.burn();
        scorch.heal(10, 84);
        assert!(scorch.level < 0.8 && scorch.level > 0.55, "{}", scorch.level);
        scorch.burn();
        assert_eq!(scorch.level, 0.8);
        scorch.heal(84, 84);
        assert_eq!(scorch.level, 0.0);
    }

    #[test]
    fn char_darkens_and_an_ember_glows() {
        let green = Color::srgb(0.3, 0.45, 0.2);
        assert_eq!(charred(green, 0.0, 0.0), (green, 0.0));
        let (black, _) = charred(green, 1.0, 0.0);
        assert_eq!(black, CHAR);
        let (lit, glow) = charred(green, 1.0, 1.0);
        assert!(lit.to_srgba().red > black.to_srgba().red && glow > 0.0);
    }

    #[test]
    fn a_flame_rises_when_lit_and_shrinks_as_it_gutters() {
        assert!(Fire { age: 0.0, guttering: None }.size() < 0.1);
        assert_eq!(Fire { age: 5.0, guttering: None }.size(), 1.0);
        assert!(Fire { age: 5.0, guttering: Some(GUTTER / 2.0) }.size() < 0.6);
        assert_eq!(Fire { age: 5.0, guttering: Some(0.0) }.size(), 0.0);
    }
}
