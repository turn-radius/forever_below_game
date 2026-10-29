//! The loading screen, drawn while a level is made.
//!
//! A level is made off the main thread ([`crate::travel::Prepared::make`]) and the frame never
//! waits for it. While it is made, [`LoadingLevel`] is in the world: a journey inserts it when
//! the screen has faded to black ([`crate::travel`]), and a delve begun from the title or
//! resumed from a save inserts it with the job ([`start_delve`]). [`show_loading`] draws the
//! screen once the work has lasted [`GRACE_SECONDS`], so a level that is ready at once (the
//! prefetched one below) is never flashed: the depth to come ("Level 3"), a line of flavour from
//! the grammar in `data/loading.json`, and a row of lamps that keeps chasing along while the
//! main thread is free. When the level is ready [`LoadingLevel`] goes, and the screen fades away
//! over the level, which the journey fades in from black beneath it.
//!
//! The screen is laid out for 1920 by 1080 like every other and scales with the window
//! ([`crate::ui::InterfaceScalePlugin`]).
//!
//! `FB_HOLD_LOADING=<seconds>` holds every load at least that long, for captures of the screen.

use std::sync::OnceLock;
use std::time::Instant;

use bevy::prelude::*;
use procgen_grammar::{Grammar, expand_with};
use procgen_rng::Mulberry32;

use crate::character::{Character, ChosenDelver};
use crate::fitting::Fitting;
use crate::folk::FolkFigures;
use crate::launch::AppState;
use crate::level::PreparedStone;
use crate::play::{CurrentLevel, DelveSystems};
use crate::record::LevelRecord;
use crate::save::SavedDelve;
use crate::staged::Staged;
use crate::title::ABYSS;
use crate::travel::Prepared;
use crate::ui::{BRIGHT, GLOW, GOLD, LINE, MUTED, UiFonts};

/// Seconds a load must last before the screen is drawn, after a fade to black. A load that is
/// over before this never shows it.
pub const GRACE_SECONDS: f32 = 0.2;
/// Seconds the screen takes to fade in once drawn, and to fade away over the level.
pub const FADE_IN_SECONDS: f32 = 0.3;
pub const FADE_OUT_SECONDS: f32 = 0.7;
/// The most a frame counts for in the fades, so a frame that stalls does not eat them.
const MAX_STEP: f32 = 0.05;
/// How many lamps chase along the row.
const LAMPS: usize = 7;
/// Z over the journey's black fade and the depth title.
const Z: i32 = 150;

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
    /// Seconds the load is held for at least (`FB_HOLD_LOADING`).
    hold: f32,
    /// Whether the level is made, though it is still being laid out.
    made: bool,
}

impl LoadingLevel {
    fn new(depth: u32, seed: u32, grace: f32) -> Self {
        let hold = std::env::var("FB_HOLD_LOADING").ok().and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.0);
        LoadingLevel { depth, seed, started: Instant::now(), grace, hold, made: false }
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

    /// Holds the load up for `seconds` more, past its work, as `FB_HOLD_LOADING` does.
    pub fn hold_for(&mut self, seconds: f32) {
        self.hold = self.hold.max(self.seconds() + seconds);
    }

    /// Whether the load is held up still, past its work (`FB_HOLD_LOADING`).
    pub fn held(&self) -> bool {
        self.seconds() < self.hold
    }

    /// Says the level is made. A screen not drawn yet is not drawn for the frames that lay it
    /// out: a level that was ready within the grace never shows it.
    pub fn made(&mut self) {
        self.made = true;
    }

    fn shows(&self) -> bool {
        !self.made && self.seconds() >= self.grace
    }
}

/// The grammar of the flavour lines.
fn grammar() -> &'static Grammar {
    static GRAMMAR: OnceLock<Grammar> = OnceLock::new();
    GRAMMAR.get_or_init(|| Grammar::from_json(include_str!("../../data/loading.json")).unwrap_or_else(|e| panic!("data/loading.json: {e}")))
}

/// The line of flavour shown while the level at `depth` of the abyss of `run_seed` loads: the
/// same line for the same level, every time.
pub fn flavour(run_seed: u32, depth: u32) -> String {
    let mut rng = Mulberry32::new(run_seed ^ depth.wrapping_mul(0x9E37_79B1) ^ 0x10AD);
    let words = expand_with(grammar(), "#line#", &mut rng).unwrap_or_else(|e| panic!("data/loading.json: {e}"));
    words.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The words over the depth to come.
pub fn level_line(depth: u32) -> String {
    format!("Level {depth}")
}

/// The root of the screen: how long it has been up, and how long it has been going.
#[derive(Component, Debug, Default)]
pub struct LoadingScreen {
    /// Seconds since it was drawn.
    pub seen: f32,
    /// Seconds since the level was ready, once it is.
    pub leaving: Option<f32>,
}

impl LoadingScreen {
    /// How opaque the screen is, 0 to 1: in over [`FADE_IN_SECONDS`], away over [`FADE_OUT_SECONDS`].
    pub fn opacity(&self) -> f32 {
        let ease = |t: f32| t * t * (3.0 - 2.0 * t);
        let rising = ease((self.seen / FADE_IN_SECONDS).clamp(0.0, 1.0));
        let falling = self.leaving.map_or(1.0, |gone| 1.0 - ease((gone / FADE_OUT_SECONDS).clamp(0.0, 1.0)));
        rising.min(falling)
    }
}

/// A part of the screen drawn in `base`, faded with the screen.
#[derive(Component, Clone, Copy, Debug)]
struct Tint(Color);

/// One lamp of the row, which brightens as the chase passes.
#[derive(Component, Debug)]
struct Lamp(usize);

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

/// Makes the first level of a delve off the main thread under the loading screen: the level at
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

/// The delve ends (a fall) while a level is being made: no load or screen outlasts it.
fn end_loading(mut commands: Commands, screens: Query<Entity, With<LoadingScreen>>) {
    commands.remove_resource::<LoadingLevel>();
    for screen in &screens {
        commands.entity(screen).despawn();
    }
}

/// The delve has laid its first level out (the frame it opened on): the loading screen can go.
fn end_start_loading(mut commands: Commands, starting: Option<Res<StartingDelve>>, journey: Option<Res<crate::travel::Journey>>, loading: Option<Res<LoadingLevel>>, turns: Res<LayoutTurn>) {
    if starting.is_none() && journey.is_none() && turns.done() && loading.is_some_and(|l| !l.held()) {
        commands.remove_resource::<LoadingLevel>();
    }
}

/// Draws the screen once the load has lasted its grace. The screen is not drawn twice.
fn show_loading(mut commands: Commands, loading: Option<Res<LoadingLevel>>, fonts: Option<Res<UiFonts>>, mut screens: Query<(Entity, &mut LoadingScreen, Has<UiTargetCamera>)>, view: Query<Entity, With<crate::carry::ViewModelCamera>>) {
    let Some(loading) = loading else { return };
    // A screen on its way out is called back by a load that begins before it is gone.
    if let Some((entity, mut screen, targeted)) = screens.iter_mut().next() {
        screen.leaving = None;
        // Begun on the title, the screen is retargeted once the delve has the HUD's camera.
        if !targeted && let Some(camera) = view.iter().next() {
            commands.entity(entity).insert(UiTargetCamera(camera));
        }
        return;
    }
    if !loading.shows() {
        return;
    }
    let fonts = fonts.map_or_else(|| UiFonts { display: default(), display_bold: default(), body: default(), body_italic: default(), body_medium: default(), mono: default(), mono_bold: default(), rune: default() }, |f| f.clone());
    // In the delve the screen is drawn by the camera the HUD is, so it covers the HUD too.
    spawn_screen(&mut commands, &fonts, view.iter().next(), loading.depth, &flavour(loading.seed, loading.depth));
}

/// The screen: the dark, the depth to come in the display face between gold rules, the line of
/// flavour under it, and the row of lamps.
fn spawn_screen(commands: &mut Commands, fonts: &UiFonts, camera: Option<Entity>, depth: u32, line: &str) {
    let rule = |parent: &mut ChildSpawnerCommands| {
        parent.spawn((Node { width: px(260), height: px(1), ..default() }, BackgroundColor(Color::NONE), Tint(LINE)));
    };
    let diamond = |parent: &mut ChildSpawnerCommands, size: f32| {
        parent.spawn((Node { width: px(size), height: px(size), ..default() }, UiTransform::from_rotation(Rot2::degrees(45.0)), BackgroundColor(Color::NONE), Tint(GOLD)));
    };
    let mut root = commands.spawn((
        Name::new("loading screen"),
        LoadingScreen::default(),
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
        BackgroundColor(Color::NONE),
        Tint(ABYSS),
        GlobalZIndex(Z),
    ));
    if let Some(camera) = camera {
        root.insert(UiTargetCamera(camera));
    }
    root.with_children(|screen| {
            screen
                .spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(26), margin: UiRect::top(px(-60)), ..default() })
                .with_children(|column| {
                    column.spawn((
                        Name::new("loading depth"),
                        Text::new(level_line(depth)),
                        UiFonts::style(&fonts.display_bold, 104.0, BRIGHT.with_alpha(0.0)),
                        TextShadow { offset: Vec2::new(3.0, 3.0), color: Color::BLACK.with_alpha(0.0) },
                        Tint(BRIGHT),
                    ));
                    column.spawn(Node { align_items: AlignItems::Center, column_gap: px(18), ..default() }).with_children(|rules| {
                        rule(rules);
                        diamond(rules, 10.0);
                        rule(rules);
                    });
                    column.spawn((
                        Name::new("loading line"),
                        Text::new(line),
                        UiFonts::style(&fonts.body_italic, 32.0, MUTED.with_alpha(0.0)),
                        TextLayout::justify(Justify::Center),
                        Node { max_width: px(940), margin: UiRect::top(px(8)), ..default() },
                        Tint(MUTED),
                    ));
                });
            screen.spawn(Node { position_type: PositionType::Absolute, bottom: px(150), column_gap: px(26), align_items: AlignItems::Center, ..default() }).with_children(|lamps| {
                for index in 0..LAMPS {
                    lamps.spawn((Lamp(index), Node { width: px(12), height: px(12), ..default() }, UiTransform::from_rotation(Rot2::degrees(45.0)), BackgroundColor(Color::NONE)));
                }
            });
        });
}

/// How bright lamp `index` is `seconds` into the chase, 0 to 1: a swell that runs along the row
/// and starts over.
pub fn lamp_glow(index: usize, seconds: f32) -> f32 {
    let along = (seconds * 1.6 - index as f32 * 0.9 / LAMPS as f32).rem_euclid(1.0);
    // Up fast, down slow.
    if along < 0.18 { along / 0.18 } else { (1.0 - (along - 0.18) / 0.82).powi(2) }
}

/// Counts the screen's time and fades it with the load: in while loading, away once the load is
/// done, and gone when it has faded.
fn fade_loading(mut commands: Commands, time: Res<Time<Real>>, loading: Option<Res<LoadingLevel>>, mut screens: Query<(Entity, &mut LoadingScreen)>) {
    let step = time.delta_secs().min(MAX_STEP);
    for (entity, mut screen) in &mut screens {
        screen.seen += step;
        if loading.is_none() {
            let gone = screen.leaving.get_or_insert(0.0);
            *gone += step;
            if *gone >= FADE_OUT_SECONDS {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Paints the screen's parts in their colours at the screen's opacity, and the lamps in their
/// chase: the sign that the main thread is free.
#[allow(clippy::type_complexity)]
fn paint_loading(
    time: Res<Time<Real>>,
    screens: Query<&LoadingScreen>,
    mut panels: Query<(&Tint, &mut BackgroundColor), (Without<Lamp>, Without<Text>)>,
    mut words: Query<(&Tint, &mut TextColor, Option<&mut TextShadow>), With<Text>>,
    mut lamps: Query<(&Lamp, &mut BackgroundColor)>,
) {
    let Some(opacity) = screens.iter().map(LoadingScreen::opacity).reduce(f32::max) else { return };
    for (tint, mut fill) in &mut panels {
        fill.0 = tint.0.with_alpha(tint.0.alpha() * opacity);
    }
    for (tint, mut ink, shadow) in &mut words {
        ink.0 = tint.0.with_alpha(tint.0.alpha() * opacity);
        if let Some(mut shadow) = shadow {
            shadow.color = Color::BLACK.with_alpha(0.8 * opacity);
        }
    }
    let seconds = time.elapsed_secs();
    for (Lamp(index), mut fill) in &mut lamps {
        let glow = lamp_glow(*index, seconds);
        let dim = GOLD.with_alpha(0.25);
        let lit = GLOW;
        fill.0 = dim.mix(&lit, glow).with_alpha((0.25 + 0.75 * glow) * opacity);
    }
}

pub struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiFonts>()
            .init_resource::<Staged>()
            .init_resource::<PreparedStone>()
            .init_resource::<FolkFigures>()
            .init_resource::<LayoutTurn>()
            .add_systems(PreUpdate, track_layout)
            .add_systems(OnExit(AppState::Delving), end_loading)
            .add_systems(Update, (finish_starting.run_if(resource_exists::<StartingDelve>), show_loading, fade_loading, paint_loading).chain())
            .add_systems(Update, end_start_loading.run_if(in_state(AppState::Delving)).after(DelveSystems));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grammar_is_sound() {
        assert!(grammar().problems("line").is_empty(), "{:?}", grammar().problems("line"));
    }

    #[test]
    fn a_level_has_the_same_line_every_time_and_the_lines_vary() {
        assert_eq!(flavour(1207, 3), flavour(1207, 3));
        let lines: std::collections::BTreeSet<String> = (1..40).map(|depth| flavour(1207, depth)).collect();
        assert!(lines.len() > 6, "{} different lines in 39 levels", lines.len());
        assert!(lines.iter().all(|l| !l.is_empty() && !l.contains('#') && l.ends_with(['.', '!'])), "{lines:?}");
    }

    #[test]
    fn the_depth_to_come_is_named_for_the_player() {
        assert_eq!(level_line(3), "Level 3");
    }

    #[test]
    fn the_screen_fades_in_and_then_away_when_the_level_is_ready() {
        let mut screen = LoadingScreen::default();
        assert_eq!(screen.opacity(), 0.0);
        screen.seen = FADE_IN_SECONDS / 2.0;
        assert!(screen.opacity() > 0.2 && screen.opacity() < 0.8);
        screen.seen = 5.0;
        assert_eq!(screen.opacity(), 1.0);
        screen.leaving = Some(FADE_OUT_SECONDS / 2.0);
        assert!(screen.opacity() > 0.2 && screen.opacity() < 0.8);
        screen.leaving = Some(FADE_OUT_SECONDS);
        assert_eq!(screen.opacity(), 0.0);
    }

    #[test]
    fn the_lamps_chase_along_the_row_and_keep_going() {
        for seconds in [0.0, 0.7, 12.3, 1000.0] {
            for index in 0..LAMPS {
                assert!((0.0..=1.0).contains(&lamp_glow(index, seconds)));
            }
        }
        // Over a second the row is never still: some lamp is bright and some is dim.
        for step in 0..20 {
            let seconds = step as f32 * 0.05;
            let glows: Vec<f32> = (0..LAMPS).map(|i| lamp_glow(i, seconds)).collect();
            assert!(glows.iter().cloned().fold(0.0, f32::max) > 0.3, "{seconds}: {glows:?}");
        }
        assert_ne!(lamp_glow(0, 0.1), lamp_glow(0, 0.2));
    }

    #[test]
    fn the_screen_waits_out_its_grace_before_it_shows() {
        let quick = LoadingLevel::after_black(2, 1207);
        assert!(!quick.shows(), "a level ready at once is not flashed");
        assert!(LoadingLevel::from_title(1, 1207).shows(), "the title has nothing to hide behind");
    }

    #[test]
    fn a_screen_begun_on_the_title_is_retargeted_to_the_delves_camera() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<UiFonts>().add_systems(Update, show_loading);
        app.insert_resource(LoadingLevel::from_title(1, 1207));
        app.update();
        let screen = app.world_mut().query_filtered::<Entity, With<LoadingScreen>>().single(app.world()).unwrap();
        assert!(app.world().get::<UiTargetCamera>(screen).is_none(), "no camera of the delve yet");
        let camera = app.world_mut().spawn(crate::carry::ViewModelCamera { follows: Entity::PLACEHOLDER }).id();
        app.update();
        assert_eq!(app.world().get::<UiTargetCamera>(screen).map(|t| t.entity()), Some(camera), "the HUD's camera draws it, so the HUD is under it");
    }

    #[test]
    fn the_screen_is_drawn_once_and_goes_when_the_level_is_ready() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(50))).init_resource::<UiFonts>().add_systems(Update, (show_loading, fade_loading, paint_loading).chain());
        app.insert_resource(LoadingLevel::from_title(4, 1207));
        app.update();
        app.update();
        let count = |app: &mut App| app.world_mut().query::<&LoadingScreen>().iter(app.world()).count();
        assert_eq!(count(&mut app), 1);
        let texts: Vec<String> = app.world_mut().query::<&Text>().iter(app.world()).map(|t| t.0.clone()).collect();
        assert!(texts.contains(&"Level 4".to_string()), "{texts:?}");
        app.world_mut().remove_resource::<LoadingLevel>();
        // The screen is gone after its fade.
        for _ in 0..16 {
            app.update();
        }
        assert_eq!(count(&mut app), 0, "the screen faded away");
    }
}
