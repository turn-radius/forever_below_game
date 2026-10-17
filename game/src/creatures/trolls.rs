//! Trolls in the game: regenerating giants that rise again unless burned.
//!
//! A troll carries its measured joints and its [`Regeneration`] clock (`fb_creatures::troll`,
//! the rule itself, tested without Bevy) as a [`Troll`] on its root.
//!
//! - **Regaining.** Every [`TURN_SECONDS`](fb_creatures::troll::TURN_SECONDS) a living troll
//!   regains [`REGAIN`](fb_creatures::troll::REGAIN) hit points, up to its maximum.
//! - **Burning.** A blow from a lit torch counts as fire ([`crate::combat::swing::flaming_blow`]
//!   sends a [`Burned`] with it): the troll's next regeneration is stopped.
//! - **Falling and rising.** At 0 hit points it falls like any creature ([`Corpse`], its mind
//!   gone). A turn after, unless it was burned since, it rises with the hit points it
//!   regenerated: its collapse is undone, its [`Corpse`] removed and a new mind given
//!   ([`brain_for`]) so it walks and fights again. Burned, it dies for good: its [`Troll`] is
//!   spent and the body stays where it lies.
//! - A body brought back from the record (a level left behind) is long past rising, and stays.

use bevy::prelude::*;
use fb_creatures::Creature;
use fb_creatures::skeleton::SkeletonDims;
use fb_creatures::troll::{Regeneration, Turn};

use super::behaviour::brain_for;
use super::{Critter, Intent};
use crate::combat::Health;
use crate::combat::feedback::{Burned, Corpse, take_blows};
use crate::play::{CurrentLevel, DelveSystems};

pub struct TrollsPlugin;

impl Plugin for TrollsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Burned>().add_systems(Update, (burn, fall, regenerate).chain().after(take_blows).in_set(DelveSystems));
    }
}

/// On a troll: its measured joints (so nothing rebuilds its body each frame), its height, and
/// its regeneration.
#[derive(Component, Clone, Debug)]
pub struct Troll {
    pub dims: SkeletonDims,
    /// Crown to soles, stooped, world units.
    pub height: f32,
    pub regeneration: Regeneration,
    /// It died for good: burned while down, or long fallen. Nothing brings it back.
    pub spent: bool,
}

impl Troll {
    pub fn new(state: &fb_creatures::TrollState) -> Troll {
        Troll { dims: state.dims(), height: state.height(), regeneration: Regeneration::new(), spent: false }
    }
}

/// Fire on a troll stops its next regeneration.
fn burn(mut burned: MessageReader<Burned>, mut trolls: Query<&mut Troll>) {
    for blow in burned.read() {
        if let Ok(mut troll) = trolls.get_mut(blow.target) {
            troll.regeneration.burn();
            info!("The troll burns: it will not regenerate");
        }
    }
}

/// A troll just felled has a whole turn to wait before it may rise; a body that had already
/// lain a while when it appeared (from the record) never will.
fn fall(mut trolls: Query<(&mut Troll, &Corpse), Added<Corpse>>) {
    for (mut troll, corpse) in &mut trolls {
        if corpse.since > 1.0 {
            troll.spent = true;
        } else {
            troll.regeneration.fall();
        }
    }
}

/// What a troll's turn does to it, apart from rising: `health` regains what it regained. The
/// turn's outcome, if one ended.
pub fn take_turn(troll: &mut Troll, health: &mut Health, dt: f32) -> Option<Turn> {
    let turn = troll.regeneration.tick(dt, health.hp.max(0) as u32, health.max.max(0) as u32)?;
    match turn {
        Turn::Regained(n) => {
            health.heal(n);
        }
        Turn::Rose(n) => health.hp = n as i32,
        Turn::Died => troll.spent = true,
        Turn::Stopped => {}
    }
    Some(turn)
}

/// Each troll's turns: living ones regain, a fallen one rises or stays down.
fn regenerate(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<CurrentLevel>,
    mut trolls: Query<(Entity, &Critter, &mut Troll, &mut Health, &mut Transform, &mut Intent, Option<&Corpse>)>,
) {
    for (entity, critter, mut troll, mut health, mut transform, mut intent, corpse) in &mut trolls {
        if troll.spent || !matches!(critter.creature, Creature::Troll(_)) {
            continue;
        }
        match take_turn(&mut troll, &mut health, time.delta_secs()) {
            Some(Turn::Rose(hp)) => {
                let Some(corpse) = corpse else { continue };
                // On its feet where it lay, collapse undone, with a mind again.
                *transform = corpse.fell;
                *intent = Intent::default();
                commands.entity(entity).remove::<Corpse>().insert(brain_for(&level.0, critter, corpse.fell.translation.xz()));
                info!("The troll rises with {hp} hit points");
            }
            Some(Turn::Died) => info!("The troll, burned, does not rise"),
            Some(Turn::Regained(n)) if n > 0 => debug!("The troll regains {n} hit points"),
            _ => {}
        }
    }
}
