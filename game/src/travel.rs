//! Travelling between the levels of the abyss.
//!
//! The abyss is set when the delve begins: [`Abyss`] holds the run's seed and the depth the
//! delver is at, and every depth is always the same level, from a seed the run's seed and the
//! depth give ([`fb_level::depth_seed`], the run's own seed at the first depth). A level is
//! rebuilt from that seed each time the delver comes to it ([`Abyss::level_at`]).
//!
//! Using a stair (Primary, F by default, close to it and looking at it) writes a [`UseStair`],
//! and while it can be used [`prompt_stair`] names the key over the crosshair. The screen fades to
//! black, the level is swapped for the next one ([`Journey`]) and the screen fades in on the
//! delver standing at the matching stair: down arrives at the level below's way up, turned
//! into the level, and up arrives beside the level above's way down, turned away from it. The
//! depth shows on arrival.
//!
//! The delve stays in [`AppState::Delving`] throughout. Only what belongs to a level is
//! swapped: [`CurrentLevel`] changes, and every system that lays a level out (the stone and
//! stairs in [`crate::level`], the folk, the creatures, the props, the items on the floor) runs
//! again on that change and clears what it laid out before. Everything that belongs to the
//! delver stays where it is: the [`Player`] entity with its hit points, needs, stamina and
//! mana; the character; the inventory with its equipped items and containers; the quickslots,
//! the clock and the map's record of what was seen of each level.
//!
//! No level is made on the main thread. [`Prepared::make_after`] makes a level on the async
//! compute pool: the level itself (as its record left it, if it was visited before), its stone
//! as plain mesh data (corner shading is most of that), the figures of its folk, the costliest
//! part, and what it is furnished with ([`crate::staged`]). The level below is made ahead of
//! time: once a level is entered, [`prefetch_below`] starts that for the next depth, a few
//! frames after the arrival so it does not compete with the new level's own spawning, and skips
//! a level the delver has been to. A journey takes the prefetch if it is the stair down to a
//! level not seen before, else starts its own job ([`start_level_load`]) when the screen is
//! black, and the frames go on while it is made ([`Phase::Loading`], with the loading screen
//! drawn if it takes long, [`crate::loading`]). When it is ready [`swap_level`] swaps the level,
//! and the level's systems lay it out over the next frames, a part a frame
//! ([`crate::loading::LayoutTurn`]), before the screen fades in. A delve begun from the title
//! or resumed from a save is made the same way ([`crate::loading::start_delve`]).
//!
//! Persisting what happens on a level (#285) slots in at two places:
//!
//! - The [`LeavingLevel`] system set runs for one frame, with the screen black, after the fade
//!   out and before anything of the old level is touched. Nothing has been despawned or
//!   forgotten yet, `CurrentLevel` and [`Abyss`] still name the level being left (the level
//!   to come is [`depth_after`] of `Abyss.depth` and [`Journey::by`]), and the inventory still
//!   holds the floor items and containers. Systems added to the set can read the live world
//!   (floor items, container contents, creatures and corpses, folk, props) to record it. The
//!   level's job starts in [`start_level_load`] in the same frame, after the set, so the order
//!   is: fade out, [`LeavingLevel`], make the level, swap, the level's systems lay out the new
//!   level (and clear the old one's pieces) over a few frames, fade in.
//! - [`Abyss::level_at`] is where a level is rebuilt from its seed, to which the recorded
//!   changes are applied on arrival.

use bevy::{
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_level::{CELL, Level, Stair, StairKind};

use crate::carry::Aimed;
use crate::figure::Figure;
use crate::fitting::Fitting;
use crate::folk::{AimedFolk, FolkFigures, Person, build_figures};
use crate::interact::{CrosshairPrompt, Delver, Target, key_line, verbs_for};
use crate::keymap::{Action, ActionInput, Keymap};
use crate::level::{PreparedStone, geometry::LevelMesh, stone_chunks};
use crate::staged::{Staged, Staging};
use crate::launch::AppState;
use crate::combat::vitals::{Dying, Fallen};
use crate::inventory::OpenContainer;
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Player, PlayerCamera, ScreenOpen, level_has_focus, show_screen};
use crate::player::Locomotion;
use crate::inventory::underfoot::apply_floor_changes;
use crate::record::{LevelRecord, WorldRecord, record_level_left};
use crate::loading::LoadingLevel;
use crate::ui::{BG, INK, UiFonts};

/// Seconds the screen takes to fade to black, and to fade back in on the new level.
pub const FADE_OUT_SECONDS: f32 = 0.5;
pub const FADE_IN_SECONDS: f32 = 0.7;
/// How long the depth stays on screen after an arrival: it fades in, holds and fades out.
pub const TITLE_SECONDS: f32 = 3.2;
const TITLE_FADE_IN: f32 = 0.7;
const TITLE_FADE_OUT: f32 = 1.0;

/// How close to a stair's cell centre the delver must stand, in cells, to use it, and how
/// squarely they must face it (the cosine of the widest angle between where they look and
/// where it lies).
pub const REACH_CELLS: f32 = 1.8;
pub const FACING_COSINE: f32 = 0.5;

/// How the delver came to the level they are on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Arrival {
    /// Through the entrance door of the first level, which shuts behind them.
    #[default]
    Entered,
    /// Down the stair of the level above.
    FromAbove,
    /// Up the stair of the level below.
    FromBelow,
}

/// The abyss of this delve: its seed, and where the delver is in it.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Abyss {
    /// The seed of the run, which is also the first level's.
    pub run_seed: u32,
    /// The depth the delver is at, 1 for the first level.
    pub depth: u32,
    pub arrival: Arrival,
}

impl Default for Abyss {
    fn default() -> Self {
        Abyss { run_seed: 0, depth: 1, arrival: Arrival::Entered }
    }
}

impl Abyss {
    pub fn new(run_seed: u32) -> Self {
        Abyss { run_seed, ..default() }
    }

    /// The level at `depth`: the same level every time it is asked for. The place to apply what
    /// was recorded on a level (#285) is on the level this returns.
    pub fn level_at(&self, depth: u32) -> Level {
        fb_level::generate_in(self.run_seed, depth, &fb_level::LevelParams::for_depth(depth))
    }
}

/// A level, its stone and its folk, made off the main thread.
pub struct Prepared {
    pub level: Level,
    pub stone: Vec<LevelMesh>,
    /// The people of the level and their figures, which take most of a second to build.
    pub folk: Vec<(Person, Figure)>,
    /// Its creatures, floor items and props, rolled and shaped.
    pub staging: Staging,
}

impl Prepared {
    /// The level at `depth` of the abyss of `run_seed`, with its stone and folk, its loot rolled
    /// against `odds`.
    pub fn make(run_seed: u32, depth: u32, odds: fb_items::Odds, prayers: Vec<String>) -> Self {
        Prepared::make_after(run_seed, depth, odds, prayers, None)
    }

    /// [`Prepared::make`] for a level as `left` left it, if it was visited before: the same
    /// level [`Abyss::level_at`] gives with the record applied to it.
    pub fn make_after(run_seed: u32, depth: u32, odds: fb_items::Odds, prayers: Vec<String>, left: Option<&LevelRecord>) -> Self {
        let mut level = Abyss::new(run_seed).level_at(depth);
        if let Some(left) = left {
            left.apply_to(&mut level);
        }
        let stone = stone_chunks(&level);
        let folk = build_figures(&level.folk, &level.factions, depth);
        let staging = Staging::make(&level, odds, prayers);
        Prepared { level, stone, folk, staging }
    }

    /// Hands the parts to where the systems that lay the level out take them, and gives the
    /// level itself.
    pub fn settle(self, staged: &mut Staged, stone: &mut PreparedStone, figures: &mut FolkFigures) -> Level {
        let Prepared { level, stone: chunks, folk, staging } = self;
        staged.set(&level, staging);
        stone.set(level.seed, level.depth, chunks);
        figures.keep(folk);
        level
    }
}

/// The level below the one the delver is on, being made: the run's seed and the depth it is for,
/// and the job. Dropped with the delve, which cancels it.
#[derive(Resource, Debug)]
pub struct Below(Fitting<(u32, u32), Prepared>);

impl Below {
    /// The run's seed and the depth it is being made for.
    pub fn key(&self) -> (u32, u32) {
        *self.0.key()
    }

    /// Whether the level is made, ready to be taken.
    pub fn is_ready(&self) -> bool {
        self.0.is_ready()
    }
}

/// Written when the delver takes a stair.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UseStair(pub StairKind);

/// A journey between two levels, from the moment the screen starts to fade until it has faded
/// back in. While it lasts the level has no focus ([`level_has_focus`]): the delver stands still.
#[derive(Resource)]
pub struct Journey {
    /// The stair taken.
    pub by: StairKind,
    pub phase: Phase,
    /// Seconds into the phase.
    pub seconds: f32,
    /// The black the screen fades through.
    pub fade: Entity,
    /// The level to come, being made while the screen is black ([`Phase::Loading`]).
    job: Option<Fitting<(u32, u32), Prepared>>,
    /// The level to come, made, until the loading screen lets it go.
    ready: Option<Prepared>,
    /// How many times the level had changed (`LayoutTurn::generation`) when the swap was made.
    laid_after: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    FadingOut,
    /// One frame at black, where [`LeavingLevel`] runs and the level is still the old one.
    Leaving,
    /// Black, and the loading screen once it takes a while, until the next level is made. The
    /// old level is still the level, and the frames go on.
    Loading,
    /// The level has changed and is being laid out, a part a frame, under black.
    Settling,
    FadingIn,
}

/// Systems that record the level being left, in the one frame between the fade out and the swap
/// (see the module docs). They run only then.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LeavingLevel;

fn journey_is_leaving(journey: Option<Res<Journey>>) -> bool {
    journey.is_some_and(|j| j.phase == Phase::Leaving)
}

/// The depth shown after an arrival.
#[derive(Component, Debug)]
pub struct DepthTitle {
    pub seconds: f32,
}

/// The depth the delver arrives at by taking a stair of `kind` from `depth`: down one level,
/// or up one, never above the first.
pub fn depth_after(depth: u32, by: StairKind) -> u32 {
    match by {
        StairKind::Down => depth + 1,
        StairKind::Up => depth.saturating_sub(1).max(1),
    }
}

/// The stair the delver at `feet` looking along `looking` (both on the floor plane, world
/// units) can use: the nearest one in [`REACH_CELLS`] that they face.
pub fn stair_in_reach(level: &Level, feet: Vec2, looking: Vec2) -> Option<&Stair> {
    let looking = looking.try_normalize()?;
    level
        .stairs
        .iter()
        .filter_map(|stair| {
            let to = Vec2::new((stair.x as f32 + 0.5) * CELL, (stair.y as f32 + 0.5) * CELL) - feet;
            let distance = to.length();
            let faced = distance < 1e-3 || to.dot(looking) / distance >= FACING_COSINE;
            (distance <= REACH_CELLS * CELL && faced).then_some((distance, stair))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, stair)| stair)
}

/// The opacity of the depth title `seconds` after an arrival.
pub fn title_opacity(seconds: f32) -> f32 {
    let fade_in = (seconds / TITLE_FADE_IN).clamp(0.0, 1.0);
    let fade_out = ((TITLE_SECONDS - seconds) / TITLE_FADE_OUT).clamp(0.0, 1.0);
    // Eased, so it comes and goes softly.
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    ease(fade_in.min(fade_out))
}

pub struct TravelPlugin;

impl Plugin for TravelPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::loading::LoadingPlugin>() {
            app.add_plugins(crate::loading::LoadingPlugin);
        }
        app.init_resource::<Abyss>()
            .init_resource::<FolkFigures>()
            .init_resource::<Staged>()
            .init_resource::<PreparedStone>()
            .add_message::<UseStair>()
            .add_systems(OnEnter(AppState::Delving), begin_the_run)
            .add_systems(OnExit(AppState::Delving), end_the_run)
            .init_resource::<WorldRecord>()
            .init_resource::<crate::enchanting::Enchanting>()
            .configure_sets(Update, LeavingLevel.run_if(journey_is_leaving).before(start_level_load).in_set(DelveSystems))
            .add_systems(Update, record_level_left.after(apply_floor_changes).in_set(LeavingLevel))
            .add_systems(
                Update,
                (take_stair.run_if(level_has_focus), take_stair_for_capture.run_if(|| crate::launch::env_set("FB_TAKE_STAIR")), begin_journey, start_level_load.run_if(journey_is_leaving), swap_level.run_if(journey_is_loading), step_journey, fade_depth_title, prefetch_below.run_if(resource_exists::<CurrentLevel>)).chain().in_set(DelveSystems),
            )
            .add_systems(Update, prompt_stair.after(crate::props::prompt_aimed_container).in_set(DelveSystems));
    }
}

/// A new delve is a new abyss, from the seed of the level it opens on.
pub(crate) fn begin_the_run(level: Option<Res<CurrentLevel>>, character: Option<Res<crate::character::Character>>, mut abyss: ResMut<Abyss>, mut record: ResMut<WorldRecord>, mut enchanting: ResMut<crate::enchanting::Enchanting>, mut dev_used: Local<bool>) {
    // What was scrapped lasts one delve.
    *enchanting = default();
    // `FB_DEPTH` starts below the first level, whose seed is not the run's: the run keeps the
    // seed `main` made the level from, so the world the sheet and standings read is the level's.
    // Only the first run: a new delve after a death is a new world.
    let dev_run = crate::launch::env_set("FB_DEPTH") && abyss.run_seed != 0 && !std::mem::replace(&mut *dev_used, true);
    *abyss = level.map_or_else(Abyss::default, |level| Abyss { depth: level.0.depth, ..Abyss::new(if dev_run { abyss.run_seed } else { level.0.seed }) });
    // The world record resets with the run for now (#285), and standing starts afresh in the
    // new world from the delver's alignment (#542).
    // A faction that likes or hates the delver's class or folk starts apart from it (#547).
    let delver = character.map_or(fb_faction::Delver::of_alignment(fb_faction::Alignment::TrueNeutral), |c| fb_faction::Delver {
        alignment: c.class.alignment(),
        class: Some(c.class.faction_class()),
        folk: Some(c.folk),
    });
    *record = WorldRecord::new_delve_as(&delver, abyss.run_seed);
}

/// Leaving the delve leaves the abyss.
fn end_the_run(mut commands: Commands, mut abyss: ResMut<Abyss>, mut staged: ResMut<Staged>, mut prepared: ResMut<PreparedStone>, journey: Option<Res<Journey>>, clock: Option<ResMut<Time<Virtual>>>) {
    // A journey ended by a fall holds the world's clock still: let it go.
    if let (Some(_), Some(mut clock)) = (journey, clock) {
        clock.unpause();
    }
    *abyss = Abyss::default();
    *staged = Staged::default();
    prepared.clear();
    commands.remove_resource::<Journey>();
    commands.remove_resource::<Below>();
}

/// Frames on a level before the one below is started, so the work does not overlap the frames
/// that lay the new level out.
const PREFETCH_DELAY_FRAMES: u32 = 10;

/// Starts making the level below the one the delver is on, unless it is under way already or the
/// delver has been there (a visited level is rebuilt from the record, not from the prefetch).
#[allow(clippy::too_many_arguments)]
fn prefetch_below(
    mut commands: Commands,
    abyss: Res<Abyss>,
    level: Res<CurrentLevel>,
    record: Res<WorldRecord>,
    below: Option<Res<Below>>,
    journey: Option<Res<Journey>>,
    character: Option<Res<crate::character::Character>>,
    inventory: Option<Res<crate::inventory::Inventory>>,
    rules: Option<Res<crate::rules::GameRules>>,
    mut waited: Local<(u32, u32)>,
) {
    // A journey makes its own level, or has taken the prefetch.
    if journey.is_some() {
        return;
    }
    // The abyss follows the level a frame after the delve opens on it.
    if abyss.depth != level.0.depth {
        return;
    }
    // Count the frames since the arrival at this depth.
    if waited.0 != abyss.depth {
        *waited = (abyss.depth, 0);
    }
    if waited.1 < PREFETCH_DELAY_FRAMES {
        waited.1 += 1;
        return;
    }
    let key = (abyss.run_seed, abyss.depth + 1);
    if below.is_some_and(|below| *below.0.key() == key) || record.level(key.1).is_some() {
        return;
    }
    // Its loot rolls for the luck worn now; if that changes before the stair down, the level's
    // loot is rolled again on arrival (`crate::staged::Staged::take_items`).
    let odds = crate::character::loot_odds(character.as_deref(), inventory.as_deref(), key.1);
    let prayers = crate::rules::prayer_pool(crate::rules::GameRules::of(&rules));
    commands.insert_resource(Below(Fitting::start(key, move || Prepared::make(key.0, key.1, odds, prayers))));
}

/// F at a stair uses it, unless an item or container is under the crosshair. A delver at 0 hit
/// points takes no stair, even in their last gasp ([`Dying`]): it is for a draught, not an escape.
#[allow(clippy::too_many_arguments)]
fn take_stair(
    input: ActionInput,
    level: Res<CurrentLevel>,
    journey: Option<Res<Journey>>,
    fallen: Option<Res<Fallen>>,
    dying: Option<Res<Dying>>,
    aimed: Option<Res<crate::carry::Aimed>>,
    players: Query<&Transform, With<Player>>,
    cameras: Query<&GlobalTransform, With<PlayerCamera>>,
    mut used: MessageWriter<UseStair>,
) {
    // An item or container under the crosshair takes the press instead.
    let aiming = aimed.is_some_and(|a| a.0.is_some());
    if journey.is_some() || fallen.is_some() || dying.is_some() || aiming || !input.just_pressed(Action::Primary) {
        return;
    }
    let (Some(feet), Some(eye)) = (players.iter().next(), cameras.iter().next()) else { return };
    let looking = eye.forward().as_vec3();
    if let Some(stair) = stair_in_reach(&level.0, feet.translation.xz(), looking.xz()) {
        used.write(UseStair(stair.kind));
    }
}

/// `FB_TAKE_STAIR=<down|up>[,<seconds>]`: that many seconds into the delve (default 1), takes the
/// stair as if F had been pressed at it, once, for captures of a journey and for timing one.
fn take_stair_for_capture(time: Res<Time>, level: Res<CurrentLevel>, journey: Option<Res<Journey>>, mut used: MessageWriter<UseStair>, mut waited: Local<f32>, mut taken: Local<bool>) {
    let Ok(value) = std::env::var("FB_TAKE_STAIR") else { return };
    *waited += time.delta_secs();
    let mut parts = value.split(',');
    let kind = if parts.next().is_some_and(|k| k.trim() == "up") { StairKind::Up } else { StairKind::Down };
    let after = parts.next().and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(1.0);
    if *taken || journey.is_some() || *waited < after || level.0.stair(kind).is_none() {
        return;
    }
    *taken = true;
    used.write(UseStair(kind));
}

/// Says the stair's verb over the crosshair while [`take_stair`] would take it: close, faced,
/// and nothing else aimed at. Runs after the container's prompt and clears only its own line,
/// so an aimed container's stands.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prompt_stair(
    level: Res<CurrentLevel>,
    fallen: Option<Res<Fallen>>,
    dying: Option<Res<Dying>>,
    aimed: Option<Res<crate::carry::Aimed>>,
    keymap: Option<Res<Keymap>>,
    players: Query<&Transform, With<Player>>,
    cameras: Query<&GlobalTransform, With<PlayerCamera>>,
    mut prompt: ResMut<CrosshairPrompt>,
    mut said: Local<Option<String>>,
) {
    let aiming = aimed.is_some_and(|a| a.0.is_some());
    let line = (|| {
        if fallen.is_some() || dying.is_some() || aiming {
            return None;
        }
        let (feet, eye) = (players.iter().next()?, cameras.iter().next()?);
        let stair = stair_in_reach(&level.0, feet.translation.xz(), eye.forward().as_vec3().xz())?;
        let fallback = Keymap::default();
        let target = Target::Stair { down: stair.kind == StairKind::Down };
        Some(key_line(keymap.as_deref().unwrap_or(&fallback), &verbs_for(&target, &Delver::default()))).filter(|line| !line.is_empty())
    })();
    if *said != line {
        if line.is_some() || prompt.0 == *said {
            prompt.0.clone_from(&line);
        }
        *said = line;
    }
}

/// Starts the journey a [`UseStair`] asks for, if the level has that stair and none is under way.
#[allow(clippy::too_many_arguments)]
fn begin_journey(
    mut commands: Commands,
    mut used: MessageReader<UseStair>,
    level: Res<CurrentLevel>,
    journey: Option<Res<Journey>>,
    fallen: Option<Res<Fallen>>,
    dying: Option<Res<Dying>>,
    mut open: ResMut<ScreenOpen>,
    mut container: Option<ResMut<OpenContainer>>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
    view: Query<Entity, With<crate::carry::ViewModelCamera>>,
) {
    let Some(UseStair(by)) = used.read().last().copied() else { return };
    if journey.is_some() || fallen.is_some() || dying.is_some() || level.0.stair(by).is_none() {
        return;
    }
    // No screen stays open over a level that is about to go: its grids show items the swap
    // forgets.
    if open.0.is_some() {
        show_screen(&mut open, None, cursor.as_deref_mut().map(|c| &mut **c));
    }
    if let Some(container) = container.as_mut() {
        container.0 = None;
    }
    let mut fade = commands.spawn((
        Name::new("travel fade"),
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
        BackgroundColor(Color::BLACK.with_alpha(0.0)),
        GlobalZIndex(100),
        Pickable::IGNORE,
        DespawnOnExit(AppState::Delving),
    ));
    // Drawn by the camera the HUD is, so the black covers the HUD too.
    if let Some(camera) = view.iter().next() {
        fade.insert(UiTargetCamera(camera));
    }
    let fade = fade.id();
    commands.insert_resource(Journey { by, phase: Phase::FadingOut, seconds: 0.0, fade, job: None, ready: None, laid_after: None });
}

/// In the frame at black, once [`LeavingLevel`] has recorded the old level, starts making the
/// next one: the level the prefetch has under way if it is the stair down to a level not visited
/// before, else a new job on the async compute pool, for a visited level as its record left it.
#[allow(clippy::too_many_arguments)]
fn start_level_load(
    mut commands: Commands,
    mut journey: ResMut<Journey>,
    abyss: Res<Abyss>,
    record: Res<WorldRecord>,
    mut below: Option<ResMut<Below>>,
    character: Option<Res<crate::character::Character>>,
    inventory: Option<Res<crate::inventory::Inventory>>,
    rules: Option<Res<crate::rules::GameRules>>,
    clock: Option<ResMut<Time<Virtual>>>,
) {
    // The world holds still while the screen is black and the level is made: nothing acts, burns
    // or hungers behind the delver's back. The fades and the loading screen run on the real clock.
    if let Some(mut clock) = clock {
        clock.pause();
    }
    let to = depth_after(abyss.depth, journey.by);
    let key = (abyss.run_seed, to);
    // (A level with a record is never started ahead, and its record sets its floor: the loot
    // rolled ahead is not laid out over it.)
    let left = record.level(to).cloned();
    let ahead = (journey.by == StairKind::Down && left.is_none()).then(|| below.as_mut().filter(|below| below.key() == key).map(|below| below.0.take())).flatten();
    let job = match ahead {
        Some(job) => {
            commands.remove_resource::<Below>();
            job
        }
        None => {
            let odds = crate::character::loot_odds(character.as_deref(), inventory.as_deref(), to);
            let prayers = crate::rules::prayer_pool(crate::rules::GameRules::of(&rules));
            Fitting::start(key, move || Prepared::make_after(key.0, key.1, odds, prayers, left.as_ref()))
        }
    };
    journey.job = Some(job);
    journey.ready = None;
    journey.phase = Phase::Loading;
    journey.seconds = 0.0;
    commands.insert_resource(LoadingLevel::after_black(to, abyss.run_seed));
}

/// The most a frame counts for in a journey's fades, so a frame that stalls does not eat them.
const MAX_STEP: f32 = 0.05;

fn journey_is_loading(journey: Option<Res<Journey>>) -> bool {
    journey.is_some_and(|j| j.phase == Phase::Loading)
}

/// While the screen is black and the next level is being made, takes it when it is ready (and
/// the loading screen is not held, `FB_HOLD_LOADING`) without holding up a frame, and swaps the
/// level for it. The level's parts are laid out over the next frames ([`crate::loading::LayoutTurn`]).
#[allow(clippy::too_many_arguments)]
fn swap_level(
    mut commands: Commands,
    fonts: Option<Res<UiFonts>>,
    mut journey: ResMut<Journey>,
    mut abyss: ResMut<Abyss>,
    record: Res<WorldRecord>,
    mut level: ResMut<CurrentLevel>,
    mut loading: Option<ResMut<LoadingLevel>>,
    turns: Res<crate::loading::LayoutTurn>,
    mut prepared: ResMut<PreparedStone>,
    mut figures: ResMut<FolkFigures>,
    mut staged: ResMut<Staged>,
    mut blocked: ResMut<BlockedCells>,
    mut aimed: ResMut<Aimed>,
    mut aimed_folk: Option<ResMut<AimedFolk>>,
    mut players: Query<(&mut Transform, &mut Locomotion), With<Player>>,
) {
    let journey = &mut *journey;
    if journey.ready.is_none() {
        journey.ready = journey.job.as_mut().and_then(Fitting::poll);
    }
    // Not made yet, or held for a capture of the loading screen.
    if journey.ready.is_none() || loading.as_ref().is_some_and(|l| l.held()) {
        return;
    }
    if let Some(loading) = loading.as_mut() {
        loading.made();
    }
    let to = depth_after(abyss.depth, journey.by);
    let made = journey.ready.take().expect("checked above");
    let mut next = made.settle(&mut staged, &mut prepared, &mut figures);
    // A level visited before is as it was left.
    record.apply_to(&mut next);
    // Down arrives at the level below's way up, which is where it starts; up arrives
    // in front of the level above's way down.
    let (x, y, facing) = match journey.by {
        StairKind::Down => next.arrival(StairKind::Up),
        StairKind::Up => next.arrival(StairKind::Down),
    }
    .unwrap_or_else(|| next.start());
    abyss.depth = to;
    abyss.arrival = if journey.by == StairKind::Down { Arrival::FromAbove } else { Arrival::FromBelow };
    // Nothing blocked on the old level blocks the new, and nothing aimed at is still there.
    *blocked = BlockedCells::default();
    aimed.0 = None;
    if let Some(aimed_folk) = aimed_folk.as_mut() {
        aimed_folk.0 = None;
    }
    // The level's systems lay it out over the next frames, on the change.
    level.0 = next;
    for (mut body, mut locomotion) in &mut players {
        let [wx, _, wz] = Level::world(x as f32 + 0.5, y as f32 + 0.5);
        body.translation = Vec3::new(wx, 0.0, wz);
        body.rotation = Quat::from_rotation_y(facing.yaw());
        *locomotion = Locomotion { yaw: facing.yaw(), ..default() };
    }
    spawn_depth_title(&mut commands, fonts.as_deref(), to);
    journey.job = None;
    journey.laid_after = Some(turns.generation());
    journey.phase = Phase::Settling;
    journey.seconds = 0.0;
}

/// Fades out, holds a frame at black for [`LeavingLevel`] (and [`start_level_load`]), stays black
/// while the level is made and laid out, and fades back in on it.
#[allow(clippy::too_many_arguments)]
fn step_journey(
    mut commands: Commands,
    time: Res<Time<Real>>,
    clock: Option<ResMut<Time<Virtual>>>,
    journey: Option<ResMut<Journey>>,
    turns: Res<crate::loading::LayoutTurn>,
    mut fades: Query<&mut BackgroundColor>,
) {
    let Some(mut journey) = journey else { return };
    journey.seconds += time.delta_secs().min(MAX_STEP);
    let opacity = match journey.phase {
        Phase::FadingOut if journey.seconds >= FADE_OUT_SECONDS => {
            journey.phase = Phase::Leaving;
            1.0
        }
        // [`start_level_load`] takes the frame at black, and [`swap_level`] the frames after it.
        Phase::Leaving | Phase::Loading => 1.0,
        // Laid out when the level has changed and every part has had its turn.
        Phase::Settling => {
            if journey.laid_after.is_some_and(|before| turns.generation() > before) && turns.done() {
                // The loading screen, if it was drawn, fades away over the level as the black does.
                commands.remove_resource::<LoadingLevel>();
                if let Some(mut clock) = clock {
                    clock.unpause();
                }
                journey.phase = Phase::FadingIn;
                journey.seconds = 0.0;
            }
            1.0
        }
        Phase::FadingOut => journey.seconds / FADE_OUT_SECONDS,
        Phase::FadingIn if journey.seconds >= FADE_IN_SECONDS => {
            commands.entity(journey.fade).despawn();
            commands.remove_resource::<Journey>();
            return;
        }
        Phase::FadingIn => 1.0 - journey.seconds / FADE_IN_SECONDS,
    };
    if let Ok(mut fade) = fades.get_mut(journey.fade) {
        fade.0 = Color::BLACK.with_alpha(opacity.clamp(0.0, 1.0));
    }
}

/// "Depth N", large, in the display face, near the top of the view.
pub fn spawn_depth_title(commands: &mut Commands, fonts: Option<&UiFonts>, depth: u32) {
    let font = fonts.map_or_else(TextFont::default, |f| TextFont::from_font_size(46.0).with_font(f.display.clone()));
    commands.spawn((
        Name::new("depth title"),
        DepthTitle { seconds: 0.0 },
        Text::new(format!("Depth {depth}")),
        font,
        TextLayout::justify(Justify::Center),
        TextColor(INK.with_alpha(0.0)),
        TextShadow { offset: Vec2::new(2.0, 2.0), color: BG.with_alpha(0.0) },
        Node { position_type: PositionType::Absolute, width: percent(100), top: percent(26), justify_content: JustifyContent::Center, ..default() },
        GlobalZIndex(160),
        Pickable::IGNORE,
        DespawnOnExit(AppState::Delving),
    ));
}

/// Fades the depth in and out, and takes it away.
fn fade_depth_title(mut commands: Commands, time: Res<Time>, mut titles: Query<(Entity, &mut DepthTitle, &mut TextColor, &mut TextShadow)>) {
    for (entity, mut title, mut color, mut shadow) in &mut titles {
        title.seconds += time.delta_secs();
        if title.seconds >= TITLE_SECONDS {
            commands.entity(entity).despawn();
            continue;
        }
        let opacity = title_opacity(title.seconds);
        color.0 = INK.with_alpha(opacity);
        shadow.color = BG.with_alpha(0.9 * opacity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_abyss_names_the_same_level_for_a_depth_every_time() {
        let abyss = Abyss::new(1207);
        assert_eq!(abyss.level_at(1), fb_level::generate(1207, 1));
        assert_eq!(abyss.level_at(2), abyss.level_at(2));
        assert_ne!(abyss.level_at(2).seed, abyss.level_at(3).seed);
        assert_eq!(abyss.level_at(2).depth, 2);
        assert_ne!(Abyss::new(1208).level_at(2), abyss.level_at(2));
    }

    #[test]
    fn the_level_made_in_the_background_is_the_one_made_in_place() {
        let background = Fitting::start((1207, 2), || Prepared::make(1207, 2, fb_items::Odds::loot(2), vec![])).wait().expect("the job runs");
        let level = Abyss::new(1207).level_at(2);
        assert_eq!(background.level, level);
        assert_eq!(background.stone, stone_chunks(&level));
        // Each folk figure, mesh buffers and all, is the one built here, one at a time.
        assert!(!level.folk.is_empty(), "the level has folk to make");
        assert_eq!(background.folk.len(), level.folk.iter().filter(|s| crate::folk::has_figure(s)).count());
        for spot in &level.folk {
            let person = Person::of(spot, level.faction(spot.faction), 2);
            let figure = Figure::dress(&person.spec, &person.gear, person.dirt);
            let made = background.folk.iter().find(|(p, _)| p.spot == *spot).expect("every spot has a person");
            assert_eq!(made.0, person);
            assert_eq!(format!("{:?}", made.1), format!("{figure:?}"));
        }
    }

    #[test]
    fn a_level_visited_before_is_made_ahead_as_it_was_left() {
        let abyss = Abyss::new(1207);
        // Left with nothing on its floor.
        let left = LevelRecord::default();
        let mut level = abyss.level_at(2);
        assert!(!level.items.is_empty(), "the level generates floor items");
        left.apply_to(&mut level);
        assert!(level.items.is_empty());
        let made = Prepared::make_after(1207, 2, fb_items::Odds::loot(2), vec![], Some(&left));
        assert_eq!(made.level, level, "the record is applied to the level made off the thread");
        assert_eq!(made.stone, stone_chunks(&level));
        assert!(made.staging.items.is_empty(), "no loot is rolled for a floor the record sets");
        // Without a record it is the level as generated.
        assert_eq!(Prepared::make_after(1207, 2, fb_items::Odds::loot(2), vec![], None).level, abyss.level_at(2));
    }

    #[test]
    fn settling_a_prepared_level_hands_its_parts_to_the_systems_that_lay_it_out() {
        let made = Prepared::make(1207, 2, fb_items::Odds::loot(2), vec![]);
        let (mut staged, mut stone, mut figures) = (Staged::default(), PreparedStone::default(), FolkFigures::default());
        let level = made.settle(&mut staged, &mut stone, &mut figures);
        assert_eq!(level, Abyss::new(1207).level_at(2));
        assert!(stone.is_kept() && staged.take_creatures(&level).is_some());
        let built = figures.built_here;
        assert_eq!(figures.for_spots(&level.folk, &level.factions, level.depth).len(), level.folk.iter().filter(|s| crate::folk::has_figure(s)).count());
        assert_eq!(figures.built_here, built, "the folk were built off the thread");
    }

    #[test]
    fn stairs_lead_one_level_down_or_up() {
        assert_eq!(depth_after(1, StairKind::Down), 2);
        assert_eq!(depth_after(5, StairKind::Up), 4);
        assert_eq!(depth_after(1, StairKind::Up), 1, "no way above the first level");
    }

    #[test]
    fn a_stair_is_used_when_close_and_faced() {
        let level = Abyss::new(1207).level_at(1);
        let down = level.stair(StairKind::Down).unwrap();
        let (ax, ay, facing) = down.arrival();
        let feet = Vec2::new((ax as f32 + 0.5) * CELL, (ay as f32 + 0.5) * CELL);
        let toward = Vec2::new(-facing.step().0 as f32, -facing.step().1 as f32);
        assert_eq!(stair_in_reach(&level, feet, toward), Some(down), "beside it, looking at it");
        assert_eq!(stair_in_reach(&level, feet, -toward), None, "with their back to it");
        assert_eq!(stair_in_reach(&level, feet + toward * -3.0 * CELL, toward), None, "too far off");
        assert_eq!(stair_in_reach(&level, feet, Vec2::ZERO), None);
    }

    #[test]
    fn a_stair_in_reach_names_its_key_unless_something_is_aimed_at() {
        let level = Abyss::new(1207).level_at(1);
        let down = *level.stair(StairKind::Down).unwrap();
        let (ax, ay, facing) = down.arrival();
        let feet = Vec3::new((ax as f32 + 0.5) * CELL, 0.0, (ay as f32 + 0.5) * CELL);
        let toward = Vec3::new(-facing.step().0 as f32, 0.0, -facing.step().1 as f32);
        let mut app = App::new();
        app.insert_resource(CurrentLevel(level))
            .init_resource::<CrosshairPrompt>()
            .init_resource::<Aimed>()
            .init_resource::<Keymap>()
            .add_systems(Update, prompt_stair);
        app.world_mut().spawn((Player, Transform::from_translation(feet)));
        let eye = app.world_mut().spawn((PlayerCamera, GlobalTransform::from(Transform::from_translation(feet).looking_to(toward, Vec3::Y)))).id();
        let prompt = |app: &App| app.world().resource::<CrosshairPrompt>().0.clone();
        app.update();
        assert_eq!(prompt(&app).as_deref(), Some("F Go down"));
        // Something aimed at takes the press, and its own line is left standing.
        app.world_mut().resource_mut::<Aimed>().0 = Some(eye);
        app.world_mut().resource_mut::<CrosshairPrompt>().0 = Some("F Open".into());
        app.update();
        assert_eq!(prompt(&app).as_deref(), Some("F Open"));
        app.world_mut().resource_mut::<Aimed>().0 = None;
        app.update();
        assert_eq!(prompt(&app).as_deref(), Some("F Go down"));
        // Turned away, the line goes.
        app.world_mut().entity_mut(eye).insert(GlobalTransform::from(Transform::from_translation(feet).looking_to(-toward, Vec3::Y)));
        app.update();
        assert_eq!(prompt(&app), None);
    }

    #[test]
    fn the_depth_title_fades_in_holds_and_fades_out() {
        assert_eq!(title_opacity(0.0), 0.0);
        assert!(title_opacity(0.35) > 0.2 && title_opacity(0.35) < 0.8);
        assert_eq!(title_opacity(1.5), 1.0);
        assert!(title_opacity(TITLE_SECONDS - 0.5) < 1.0);
        assert_eq!(title_opacity(TITLE_SECONDS), 0.0);
    }
}
