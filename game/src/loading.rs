//! A level is made off the main thread ([`crate::travel::Prepared::make`]) and the frame never
//! waits for it. While it is made, [`LoadingLevel`] is in the world: a journey inserts it when
//! the screen has faded to black ([`crate::travel`]), and a delve begun from the title or
//! resumed from a save inserts it with the job ([`start_delve`]). It goes when the level is made
//! and laid out. A level is laid out over a few frames, a part a frame ([`LayoutTurn`]).

use std::time::Instant;

use bevy::prelude::*;

use crate::character::{Character, ChosenDelver};
use crate::fitting::Fitting;
use crate::folk::FolkFigures;
use crate::launch::AppState;
use crate::level::PreparedStone;
use crate::play::{CurrentLevel, DelveSystems};
use crate::record::LevelRecord;
use crate::save::SavedDelve;
use crate::staged::Staged;
use crate::travel::Prepared;

/// Seconds a load must last before it is worth showing, after a fade to black. A load that is
/// over before this is not shown.
pub const GRACE_SECONDS: f32 = 0.2;

/// A level being made: present from the moment the work starts until the level is ready.
#[derive(Resource, Debug)]
pub struct LoadingLevel {
    /// The depth to come.
    pub depth: u32,
    /// Picks the flavour line with the depth.
    pub seed: u32,
    started: Instant,
    /// Seconds of work before the screen shows.
    grace: f32,
    /// Seconds the load is held for at least.
    hold: f32,
    /// Whether the level is made, though it is still being laid out.
    made: bool,
}

impl LoadingLevel {
    fn new(depth: u32, seed: u32, grace: f32) -> Self {
        LoadingLevel { depth, seed, started: Instant::now(), grace, hold: 0.0, made: false }
    }

    /// A load that begins at black, as a journey's: the screen shows only if it takes a while.
    pub fn after_black(depth: u32, seed: u32) -> Self {
        LoadingLevel::new(depth, seed, GRACE_SECONDS)
    }

    /// A load that begins from the title, where nothing is hidden by black and it is never
    /// quick: the screen shows at once.
    pub fn from_title(depth: u32, seed: u32) -> Self {
        LoadingLevel::new(depth, seed, 0.0)
    }

    /// Seconds since the work started.
    pub fn seconds(&self) -> f32 {
        self.started.elapsed().as_secs_f32()
    }

    /// Holds the load up for `seconds` more, past its work.
    pub fn hold_for(&mut self, seconds: f32) {
        self.hold = self.hold.max(self.seconds() + seconds);
    }

    /// Whether the load is held up still, past its work.
    pub fn held(&self) -> bool {
        self.seconds() < self.hold
    }

    /// Says the level is made, though it is still being laid out: a load that was over within
    /// its grace is not worth showing for the frames that lay it out.
    pub fn made(&mut self) {
        self.made = true;
    }

    pub fn shows(&self) -> bool {
        !self.made && self.seconds() >= self.grace
    }
}

/// The last turn a level is laid out on.
pub const LAST_TURN: u32 = 6;

/// How far a level's laying out has got. A level is laid out over [`LAST_TURN`] + 1 frames, a part
/// at a time (the stone, the doors and braziers, the floor items, the props, the creatures, the
/// folk, the webs and drips), so no frame stalls on the whole of it and the loading screen keeps
/// moving. The systems that lay a part out run on its turn with [`laid_out_at`].
#[derive(Resource, Default, Debug)]
pub struct LayoutTurn {
    /// How many times the level has changed since the game began.
    generation: u32,
    /// Frames since it last changed: 0 on the first frame of the new level.
    turns: u32,
}

impl LayoutTurn {
    /// Whether every turn of the level has been taken.
    pub fn done(&self) -> bool {
        self.generation > 0 && self.turns > LAST_TURN
    }

    /// How many times the level has changed.
    pub fn generation(&self) -> u32 {
        self.generation
    }
}

/// Counts the frames since the level changed, before the frame's systems run.
fn track_layout(level: Option<Res<CurrentLevel>>, mut turn: ResMut<LayoutTurn>) {
    if level.is_some_and(|level| level.is_changed()) {
        turn.generation += 1;
        turn.turns = 0;
    } else {
        turn.turns = turn.turns.saturating_add(1);
    }
}

/// A run condition: true once for each level, on the frame of its turn `turn` (0 the frame it
/// changes). Where nothing counts the turns ([`LayoutTurn`] is not in the world) it is true on
/// the frame the level changes, as `resource_exists_and_changed::<CurrentLevel>` is.
pub fn laid_out_at(turn: u32) -> impl FnMut(Option<Res<CurrentLevel>>, Option<Res<LayoutTurn>>, Local<Option<u32>>) -> bool {
    move |level, turns, mut seen| {
        let Some(level) = level else { return false };
        let Some(turns) = turns else { return level.is_changed() };
        if turns.generation == 0 || turns.turns < turn || *seen == Some(turns.generation) {
            return false;
        }
        *seen = Some(turns.generation);
        true
    }
}

/// A delve begun from the title: its level is being made, and the delve opens on it when ready.
#[derive(Resource)]
pub struct StartingDelve {
    job: Fitting<(u32, u32), Prepared>,
    ready: Option<Prepared>,
    then: Option<Opening>,
}

/// How the delve opens once its level is ready.
pub enum Opening {
    /// A delve of a delver chosen in the creator.
    New(Character),
    /// A saved delve, put back on the delve's first frame ([`crate::save::restore_delve`]).
    Resume(Box<SavedDelve>),
}

/// Makes the first level of a delve off the main thread: the level at
/// `depth` of the abyss of `run_seed`, as `saved` left it if that is a level visited before. The
/// delve opens on it ([`finish_starting`]).
pub fn start_delve(commands: &mut Commands, run_seed: u32, depth: u32, saved: Option<LevelRecord>, then: Opening) {
    let odds = crate::character::loot_odds(None, None, depth);
    let key = (run_seed, depth);
    let job = Fitting::start(key, move || Prepared::make_after(run_seed, depth, odds, saved.as_ref()));
    commands.insert_resource(StartingDelve { job, ready: None, then: Some(then) });
    commands.insert_resource(LoadingLevel::from_title(depth, run_seed));
}

/// Opens the delve once its level is made, laying the level's parts where the systems that lay
/// it out will find them.
#[allow(clippy::too_many_arguments)]
fn finish_starting(
    mut commands: Commands,
    starting: Option<ResMut<StartingDelve>>,
    loading: Option<Res<LoadingLevel>>,
    mut staged: ResMut<Staged>,
    mut stone: ResMut<PreparedStone>,
    mut figures: ResMut<FolkFigures>,
    mut next: ResMut<NextState<AppState>>,
) {
    let Some(mut starting) = starting else { return };
    if starting.ready.is_none() {
        starting.ready = starting.job.poll();
    }
    if starting.ready.is_none() || loading.is_some_and(|l| l.held()) {
        return;
    }
    let (Some(made), Some(then)) = (starting.ready.take(), starting.then.take()) else { return };
    let level = made.settle(&mut staged, &mut stone, &mut figures);
    commands.insert_resource(CurrentLevel(level));
    match then {
        Opening::New(delver) => {
            commands.insert_resource(ChosenDelver(delver));
        }
        Opening::Resume(saved) => {
            info!("save: {} resumes on depth {}", saved.delver.name, saved.depth);
            commands.insert_resource(ChosenDelver(Character::from(saved.delver.clone())));
            commands.insert_resource(crate::save::ResumeDelve(saved));
        }
    }
    commands.remove_resource::<crate::title::creator::Draft>();
    commands.remove_resource::<StartingDelve>();
    next.set(AppState::Delving);
}

/// The delve has laid its first level out: the load is over.
fn end_start_loading(mut commands: Commands, starting: Option<Res<StartingDelve>>, journey: Option<Res<crate::travel::Journey>>, loading: Option<Res<LoadingLevel>>, turns: Res<LayoutTurn>) {
    if starting.is_none() && journey.is_none() && turns.done() && loading.is_some_and(|l| !l.held()) {
        commands.remove_resource::<LoadingLevel>();
    }
}

pub struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Staged>()
            .init_resource::<PreparedStone>()
            .init_resource::<FolkFigures>()
            .init_resource::<LayoutTurn>()
            .add_systems(PreUpdate, track_layout)
            .add_systems(Update, finish_starting.run_if(resource_exists::<StartingDelve>))
            .add_systems(Update, end_start_loading.run_if(in_state(AppState::Delving)).after(DelveSystems));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_load_waits_out_its_grace_before_it_is_worth_showing() {
        let quick = LoadingLevel::after_black(2, 1207);
        assert!(!quick.shows(), "a level ready at once is not worth a screen");
        assert!(LoadingLevel::from_title(1, 1207).shows(), "the title has nothing to hide behind");
    }
}
