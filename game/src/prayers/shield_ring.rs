//! The Shield prayer's look: golden ethereal shields circling whoever holds a
//! [`ShieldPrayer`]. Each is a real Armory shield (the shield family, rolled from the prayer's
//! seed, so the same prayer gives the same shapes and a higher tier adds one more), drawn with
//! [`EtherealMaterial`] instead of the toon look. They turn slowly about the holder's upright axis
//! at about chest height, facing outward, with a gentle bob; they scale in on the cast, flicker
//! and fade as the prayer ends, and the one nearest a blow flares ([`ShieldStruck`]).
//!
//! The ring follows an entity's `Transform` ([`ShieldHolder`], else the delver), so a folk
//! cleric under the prayer gets the same ring.

use armory::{Family, Item};
use bevy::prelude::*;

use super::{ShieldPrayer, ShieldStruck, clock_ratio, sputter_lead};
use crate::camp::Asleep;
use crate::game_clock::GameClock;
use crate::launch::AppState;
use crate::play::{DelveSystems, EYE_HEIGHT, Player};
use crate::render::{EtherealMaterial, ethereal_mesh};
use crate::rules::GameRules;
use crate::tuning::Tuning;

/// How many shields circle at tier one; each tier above adds one.
pub const SHIELDS_AT_TIER_ONE: usize = 3;
/// Distance from the holder's axis the shields circle at, world units (10 cm).
pub const RADIUS: f32 = 12.0;
/// Height below the eyes the ring turns at: about the chest.
pub const BELOW_EYES: f32 = 5.0;
/// How tall each shield is drawn, world units: a bit smaller than a carried one.
pub const SHIELD_HEIGHT: f32 = 6.0;
/// Real seconds for one turn of the ring.
pub const TURN_SECONDS: f32 = 7.0;
/// Real seconds the shields take to grow in.
pub const FADE_IN: f32 = 0.5;
/// Real seconds before the prayer ends that the shields flicker and fade.
pub const FADE_OUT: f32 = 3.0;
/// Real seconds a flare takes to settle.
pub const FLARE_SECONDS: f32 = 0.6;

/// Who the ring circles, when it is not the delver (a folk caster).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShieldHolder(pub Entity);

/// The ring of shields for one [`ShieldPrayer`], placed on its holder.
#[derive(Component, Debug)]
#[require(DespawnOnExit<AppState> = DespawnOnExit(AppState::Delving))]
pub struct ShieldRing {
    pub prayer: Entity,
    pub holder: Entity,
    /// Real seconds since it was cast.
    pub age: f32,
    /// How far the ring has turned, radians.
    pub spin: f32,
}

/// One shield of a ring.
#[derive(Component, Debug)]
pub struct RingShield {
    pub index: usize,
    /// Where it sits round the ring at rest, radians from the holder's +Z.
    pub angle: f32,
    /// 1 on a blow, settling to 0.
    pub flare: f32,
    /// The shield's own seed.
    pub seed: u32,
    /// Its size at rest, a share of full scale from the shield's height.
    pub scale: f32,
    pub material: Handle<EtherealMaterial>,
}

/// The shields a prayer of `tier` rings.
pub fn shield_count(tier: u8) -> usize {
    SHIELDS_AT_TIER_ONE + usize::from(tier.max(1) - 1)
}

/// The seed of shield `index` of a prayer: the first ones never change as a tier adds more.
pub fn shield_seed(prayer_seed: u32, index: usize) -> u32 {
    prayer_seed.wrapping_add((index as u32).wrapping_mul(0x9E37_79B1))
}

/// An armory shield as the parts of an ethereal mesh, centred, and the factor that makes it
/// [`SHIELD_HEIGHT`] tall. The shield lies in the x/y plane facing +z.
pub fn ethereal_shield(seed: u32) -> Option<(Vec<Mesh>, Vec3, f32)> {
    let item = Item::roll(Family::Shield, seed)?;
    let built = item.build();
    let bounds = crate::render::item_bounds(&built)?;
    let centre = Vec3::from(bounds.center());
    let half = Vec3::from(bounds.half_size());
    let scale = SHIELD_HEIGHT / (half.y * 2.0).max(1e-3);
    let meshes = built.parts.iter().filter(|part| part.name != "flame" && !part.positions.is_empty()).map(|part| ethereal_mesh(part, [0, 1], [centre.x, centre.y], [half.x.max(1e-3), half.y.max(1e-3)])).collect();
    Some((meshes, centre, scale))
}

/// Smooth 0 to 1.
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A seed for a prayer that has none of its own: its tier and the moment it was cast.
pub fn prayer_seed(tier: u8, expires: f64) -> u32 {
    (expires.to_bits() as u32 ^ (expires.to_bits() >> 32) as u32).wrapping_mul(0x85EB_CA6B).wrapping_add(u32::from(tier))
}

/// Raises a ring for each prayer that has none.
#[allow(clippy::type_complexity)]
fn raise_the_rings(
    mut commands: Commands,
    prayers: Query<(Entity, &ShieldPrayer, Option<&ShieldHolder>)>,
    rings: Query<&ShieldRing>,
    players: Query<Entity, With<Player>>,
    holders: Query<(), With<Transform>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<EtherealMaterial>>,
) {
    for (prayer, shield, holder) in &prayers {
        if rings.iter().any(|r| r.prayer == prayer) {
            continue;
        }
        let Some(holder) = holder.map(|h| h.0).or_else(|| players.iter().next()) else { continue };
        if !holders.contains(holder) {
            continue;
        }
        let count = shield_count(shield.tier);
        let ring = commands.spawn((ShieldRing { prayer, holder, age: 0.0, spin: 0.0 }, Name::new("Shield ring"), Transform::default(), Visibility::default())).id();
        for index in 0..count {
            let seed = shield_seed(shield.seed, index);
            let Some((parts, centre, scale)) = ethereal_shield(seed) else { continue };
            let angle = std::f32::consts::TAU * index as f32 / count as f32;
            let material = materials.add(EtherealMaterial { band_phase: index as f32 * 2.1, ..EtherealMaterial::gold() });
            let pivot = commands.spawn((RingShield { index, angle, flare: 0.0, seed, scale, material: material.clone() }, Name::new("Ethereal shield"), Transform::from_scale(Vec3::ZERO), Visibility::default())).id();
            for mesh in parts {
                let part = commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), Transform::from_translation(-centre))).id();
                commands.entity(pivot).add_child(part);
            }
            commands.entity(ring).add_child(pivot);
        }
    }
}

/// Lets a ring go with its prayer, or its holder.
fn reap_the_rings(mut commands: Commands, rings: Query<(Entity, &ShieldRing)>, prayers: Query<(), With<ShieldPrayer>>, holders: Query<(), With<Transform>>) {
    for (entity, ring) in &rings {
        if !prayers.contains(ring.prayer) || !holders.contains(ring.holder) {
            commands.entity(entity).despawn();
        }
    }
}

/// Flares the shield nearest each blow (all of them if it came from the holder's own spot).
fn flare_the_struck(mut blows: MessageReader<ShieldStruck>, rings: Query<(&ShieldRing, &Children)>, holders: Query<&Transform>, mut shields: Query<&mut RingShield>) {
    for blow in blows.read() {
        for (ring, children) in &rings {
            let Ok(holder) = holders.get(ring.holder) else { continue };
            let away = blow.from - holder.translation.xz();
            let flared: Vec<Entity> = children.iter().filter(|c| shields.contains(*c)).collect();
            if away.length_squared() < 1e-6 {
                for shield in flared {
                    shields.get_mut(shield).unwrap().flare = 1.0;
                }
                continue;
            }
            let toward = away.x.atan2(away.y);
            let nearest = flared.iter().copied().min_by(|a, b| {
                let gap = |e: &Entity| {
                    let s = shields.get(*e).unwrap();
                    let turned = (s.angle + ring.spin - toward).rem_euclid(std::f32::consts::TAU);
                    turned.min(std::f32::consts::TAU - turned)
                };
                gap(a).total_cmp(&gap(b))
            });
            if let Some(nearest) = nearest {
                shields.get_mut(nearest).unwrap().flare = 1.0;
            }
        }
    }
}

/// Turns the rings, bobs the shields, and sets their light.
#[allow(clippy::type_complexity)]
fn turn_the_rings(
    time: Res<Time>,
    clock: Option<Res<GameClock>>,
    tuning: Option<Res<Tuning>>,
    asleep: Option<Res<Asleep>>,
    rules: Option<Res<GameRules>>,
    prayers: Query<&ShieldPrayer>,
    holders: Query<&Transform, (Without<ShieldRing>, Without<RingShield>)>,
    mut rings: Query<(&mut ShieldRing, &mut Transform, &Children)>,
    mut shields: Query<(&mut RingShield, &mut Transform), (Without<ShieldRing>, Without<Mesh3d>)>,
    mut materials: ResMut<Assets<EtherealMaterial>>,
) {
    let dt = time.delta_secs();
    let now = clock.as_deref().map_or(0.0, GameClock::game_seconds);
    let ratio = clock_ratio(tuning.as_deref(), asleep.as_deref(), rules.as_deref());
    for (mut ring, mut at, children) in &mut rings {
        let (Ok(holder), Ok(prayer)) = (holders.get(ring.holder), prayers.get(ring.prayer)) else { continue };
        ring.age += dt;
        ring.spin += dt * std::f32::consts::TAU / TURN_SECONDS;
        at.translation = holder.translation + Vec3::Y * (EYE_HEIGHT - BELOW_EYES);
        let age = ring.age;
        let fade_in = ease(age / FADE_IN);
        let lead = sputter_lead(FADE_OUT, ratio);
        let left = ((prayer.expires - now) / lead.max(1e-6)).clamp(0.0, 1.0) as f32;
        for child in children.iter() {
            let Ok((mut shield, mut place)) = shields.get_mut(child) else { continue };
            shield.flare = (shield.flare - dt / FLARE_SECONDS).max(0.0);
            let flare = ease(shield.flare);
            let index = shield.index as f32;
            // Sputters out over the last seconds: dimmer, and caught by quick flickers.
            let flicker = if left < 1.0 { 0.55 + 0.45 * (age * 31.0 + index * 2.3).sin().abs().powf(0.5) } else { 1.0 };
            let fade_out = ease(left) * flicker;
            let angle = shield.angle + ring.spin;
            let bob = (age * 1.3 + index * 2.1).sin() * 0.45;
            place.rotation = Quat::from_rotation_y(angle) * Quat::from_rotation_x((age * 0.9 + index * 1.7).sin() * 0.06);
            place.translation = Vec3::new(angle.sin() * RADIUS, bob, angle.cos() * RADIUS);
            place.scale = Vec3::splat(shield.scale * (0.35 + 0.65 * fade_in) * (0.6 + 0.4 * ease(left)));
            if let Some(mut material) = materials.get_mut(&shield.material) {
                material.intensity = fade_in * fade_out * (1.0 + flare * 0.5);
                material.flare = flare;
                material.band_phase += dt * material.band_speed * (1.0 + 5.0 * flare);
            }
        }
    }
}

/// For captures (`FB_SHIELD_FOLK=1`): rings the first folk met with a Shield prayer that does not
/// end, to see the ring round a body.
fn debug_ring_folk(mut commands: Commands, folk: Query<(Entity, &Transform), With<crate::folk::Denizen>>, players: Query<&Transform, With<Player>>, held: Query<&ShieldHolder>, mut done: Local<bool>) {
    if *done || !std::env::var("FB_SHIELD_FOLK").is_ok_and(|v| !v.is_empty()) {
        return;
    }
    let Some(me) = players.iter().next() else { return };
    let Some((folk, _)) = folk.iter().filter(|(f, _)| !held.iter().any(|h| h.0 == *f)).min_by(|a, b| a.1.translation.distance(me.translation).total_cmp(&b.1.translation.distance(me.translation))) else { return };
    *done = true;
    info!("FB_SHIELD_FOLK: ringing {folk:?}");
    commands.spawn((Name::new("Shield on folk"), ShieldPrayer { tier: 2, bonus: 4, expires: 1e12, lasts: 1e12, seed: 0x5eed }, ShieldHolder(folk)));
}

/// For captures (`FB_PRAY_LASTS=70`): a new Shield lasts only that many game seconds, to see it
/// flicker out.
fn debug_shield_lasts(clock: Option<Res<GameClock>>, mut fresh: Query<&mut ShieldPrayer, Added<ShieldPrayer>>) {
    let Some(lasts) = std::env::var("FB_PRAY_LASTS").ok().and_then(|v| v.trim().parse::<f64>().ok()) else { return };
    let now = clock.as_deref().map_or(0.0, GameClock::game_seconds);
    for mut prayer in &mut fresh {
        if prayer.lasts < 1e11 {
            prayer.expires = now + lasts;
            prayer.lasts = lasts;
        }
    }
}

pub fn register(app: &mut App) {
    app.add_systems(
        Update,
        (debug_shield_lasts, raise_the_rings, reap_the_rings, flare_the_struck, turn_the_rings, debug_ring_folk)
            .chain()
            .after(super::end_the_shield)
            .in_set(DelveSystems)
            .run_if(resource_exists::<Assets<EtherealMaterial>>)
            .run_if(resource_exists::<Assets<Mesh>>),
    );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::state::app::StatesPlugin;

    use super::*;
    use crate::play::run_delve_systems_while_delving;

    fn app() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, bevy::asset::AssetPlugin::default()))
            .insert_state(AppState::Delving)
            .init_asset::<Mesh>()
            .init_asset::<EtherealMaterial>()
            .add_message::<ShieldStruck>()
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)));
        run_delve_systems_while_delving(&mut app);
        register(&mut app);
        let player = app.world_mut().spawn((Player, Transform::from_xyz(30.0, 0.0, -20.0))).id();
        (app, player)
    }

    fn pray(app: &mut App, tier: u8, expires: f64, seed: u32) -> Entity {
        app.world_mut().spawn(ShieldPrayer { tier, bonus: 2 * i32::from(tier), expires, lasts: expires, seed }).id()
    }

    fn shields(app: &mut App) -> Vec<(Transform, f32, usize)> {
        let mut found: Vec<_> = app.world_mut().query::<(&RingShield, &Transform)>().iter(app.world()).map(|(s, t)| (*t, s.flare, s.index)).collect();
        found.sort_by_key(|f| f.2);
        found
    }

    fn rings(app: &mut App) -> usize {
        app.world_mut().query::<&ShieldRing>().iter(app.world()).count()
    }

    #[test]
    fn a_tier_adds_a_shield_and_the_first_ones_keep_their_seeds() {
        assert_eq!((shield_count(1), shield_count(2), shield_count(4)), (3, 4, 6));
        assert_eq!(shield_seed(77, 1), shield_seed(77, 1));
        assert_ne!(shield_seed(77, 0), shield_seed(77, 1));
    }

    #[test]
    fn the_same_seed_gives_the_same_shield_and_it_lies_in_the_xy_plane() {
        for seed in 0..40u32 {
            let (a, centre_a, scale_a) = ethereal_shield(seed).expect("a shield");
            let (b, centre_b, scale_b) = ethereal_shield(seed).expect("again");
            assert_eq!((centre_a, scale_a, a.len()), (centre_b, scale_b, b.len()));
            let built = Item::roll(Family::Shield, seed).unwrap().build();
            let half = Vec3::from(crate::render::item_bounds(&built).unwrap().half_size());
            assert!(half.z < half.x.min(half.y), "seed {seed}: a shield is thin along z: {half}");
        }
    }

    #[test]
    fn shields_spawn_with_the_prayer_three_at_tier_one_and_one_more_a_tier() {
        for (tier, count) in [(1, 3), (2, 4), (3, 5)] {
            let (mut app, _) = app();
            pray(&mut app, tier, 1e9, 5);
            app.update();
            app.update();
            assert_eq!((rings(&mut app), shields(&mut app).len()), (1, count), "tier {tier}");
        }
    }

    #[test]
    fn the_ring_sits_round_the_holder_at_chest_height_evenly_spaced_and_facing_out() {
        let (mut app, player) = app();
        pray(&mut app, 2, 1e9, 5);
        for _ in 0..8 {
            app.update();
        }
        let at = app.world().get::<Transform>(player).unwrap().translation;
        let ring = app.world_mut().query_filtered::<&Transform, With<ShieldRing>>().single(app.world()).unwrap().translation;
        assert_eq!(ring, at + Vec3::Y * (crate::play::EYE_HEIGHT - BELOW_EYES), "the ring turns on the delver's upright axis");
        let found = shields(&mut app);
        assert_eq!(found.len(), 4);
        let mut angles = Vec::new();
        for (place, _, _) in &found {
            let flat = Vec2::new(place.translation.x, place.translation.z);
            assert!((flat.length() - RADIUS).abs() < 1e-3, "on the ring: {flat}");
            assert!(place.translation.y.abs() < 1.0, "bobs gently about the ring: {}", place.translation.y);
            let out = place.rotation * Vec3::Z;
            assert!(out.xz().normalize().dot(flat.normalize()) > 0.99, "faces outward: {out} at {flat}");
            angles.push(flat.y.atan2(flat.x));
        }
        for pair in angles.windows(2) {
            let gap = (pair[0] - pair[1]).rem_euclid(std::f32::consts::TAU);
            assert!((gap - std::f32::consts::FRAC_PI_2).abs() < 1e-3 || (std::f32::consts::TAU - gap - std::f32::consts::FRAC_PI_2).abs() < 1e-3, "a quarter turn apart: {gap}");
        }
    }

    #[test]
    fn the_ring_follows_a_folk_holder_and_turns() {
        let (mut app, _) = app();
        let folk = app.world_mut().spawn(Transform::from_xyz(-50.0, 0.0, 80.0)).id();
        let prayer = pray(&mut app, 1, 1e9, 3);
        app.world_mut().entity_mut(prayer).insert(ShieldHolder(folk));
        app.update();
        app.update();
        let before = shields(&mut app)[0].0.translation;
        let ring = app.world_mut().query_filtered::<&Transform, With<ShieldRing>>().single(app.world()).unwrap().translation;
        assert_eq!(ring.xz(), Vec2::new(-50.0, 80.0));
        for _ in 0..10 {
            app.update();
        }
        assert_ne!(before, shields(&mut app)[0].0.translation, "it turns");
    }

    #[test]
    fn a_blow_flares_only_the_shield_nearest_it() {
        let (mut app, player) = app();
        pray(&mut app, 1, 1e9, 5);
        app.update();
        app.update();
        // A blow from +x of the delver.
        let from = app.world().get::<Transform>(player).unwrap().translation.xz() + Vec2::new(40.0, 0.0);
        app.world_mut().write_message(ShieldStruck { caster: player, from, damage: 2 });
        app.update();
        let flared = shields(&mut app);
        let lit: Vec<_> = flared.iter().filter(|s| s.1 > 0.0).collect();
        assert_eq!(lit.len(), 1, "one flares: {flared:?}");
        assert!(lit[0].0.translation.x > 0.0, "the one toward the blow");
        for _ in 0..10 {
            app.update();
        }
        assert!(shields(&mut app).iter().all(|s| s.1 == 0.0), "and it settles");
    }

    #[test]
    fn shields_light_in_and_flicker_out_near_the_end() {
        let (mut app, _) = app();
        pray(&mut app, 1, 1.5, 5);
        let intensity = |app: &mut App| app.world_mut().resource::<Assets<EtherealMaterial>>().iter().map(|(_, m)| m.intensity).fold(0.0, f32::max);
        app.update();
        let early = intensity(&mut app);
        assert!(early < 0.9, "fading in: {early}");
        app.update();
        // Ends in 1.5 game seconds, inside the fade out lead.
        let ending = intensity(&mut app);
        assert!(ending < 1.0, "fading out at the end: {ending}");
    }

    #[test]
    fn the_ring_goes_when_the_prayer_ends_or_is_recast() {
        let (mut app, _) = app();
        let first = pray(&mut app, 1, 1e9, 5);
        app.update();
        app.update();
        assert_eq!(rings(&mut app), 1);
        let again = pray(&mut app, 2, 1e9, 6);
        app.world_mut().despawn(first);
        app.update();
        app.update();
        let ring_prayers: Vec<Entity> = app.world_mut().query::<&ShieldRing>().iter(app.world()).map(|r| r.prayer).collect();
        assert_eq!(ring_prayers, vec![again], "the old ring is gone and a new one stands");
        assert_eq!(shields(&mut app).len(), 4);
        app.world_mut().despawn(again);
        app.update();
        assert_eq!((rings(&mut app), shields(&mut app).len()), (0, 0));
        assert_eq!(app.world_mut().query::<&Mesh3d>().iter(app.world()).count(), 0, "nothing is left behind");
    }
}
