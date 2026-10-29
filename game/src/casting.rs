//! Casting in the game: the prayers and spells every caster shares ([`fb_character::casting`]).
//!
//! A caster writes prayers from terms ([`Spellbook`], on the delver as [`DelverSpellbook`] and
//! saved with them); a cleric starts with Divine Light written. The Pray key ([`Action::Pray`],
//! C) casts the chosen prayer at the chosen tier; the tier key ([`Action::CastTier`], V) steps
//! the tier 1, 2, 3 and round again. A quickslot can hold a written prayer as well as an item
//! ([`AbilitySlots`]): the slot's key then casts it at the chosen tier. A cleric starts with
//! Divine Light in the first free slot; in the inventory the slot's key with the mouse over
//! nothing binds the chosen prayer to that slot.
//!
//! Every cast goes through a [`CastRequest`] message, which anything can send. [`cast_requests`]
//! checks the caster and the terms, spends the mana from [`DelverMana`] and either sends a
//! [`Casted`] message, which is what a prayer's effect reacts to (#747 gives the light term its
//! light, reading `cast.written.effect`, `cast.range` and `cast.duration`), or says on screen
//! why it failed. [`cast_effects`] is the one place a cast does what it says: it reads
//! `cast.written.effect` and hands the cast to that effect (`"light"` sets Divine Light, a
//! [`DivineLight`] bauble that follows the caster; the ward, frighten and charm effects join it
//! the same way). The HUD shows the chosen prayer, its tier and its cost above the quickslot bar.

use armory::build::light::LightAnchor;
use armory::material::Rgb;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use fb_character::Casting;
use fb_character::casting::{Cast, CastError, MAX_TIER, Spellbook, cast_with, points};

use crate::carry::holy_light;
use crate::character::{Character, DelverMana};
use crate::game_clock::GameClock;
use crate::render::{WispMaterial, wisp_card};
use crate::tuning::Tuning;
use crate::combat::orbs::{OrbCluster, cluster_layout};
use crate::inventory::{Hovered, Toast};
use crate::keymap::{Action, ActionInput, Keymap};
use crate::launch::AppState;
use crate::play::{DelveSystems, EYE_HEIGHT, Player, PlayerCamera, inventory_open, level_has_focus};
use crate::quickslots::{Quickslots, SLOTS};
use crate::rules::GameRules;
use crate::ui::{BRIGHT, UiFonts, hud, hud_font};

/// What the delver has written and can write with.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct DelverSpellbook(pub Spellbook);

pub struct CastingPlugin;

impl Plugin for CastingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AbilitySlots>()
            .init_resource::<Chosen>()
            .add_message::<CastRequest>()
            .add_message::<Casted>()
            .add_systems(OnExit(AppState::Delving), forget_the_abilities)
            .add_systems(
                Update,
                (
                    start_the_spellbook,
                    give_clerics_their_prayer,
                    items_displace_abilities.after(crate::quickslots::bind_hovered),
                    (step_tier, pray_key, ability_slot_key).run_if(level_has_focus),
                    bind_ability_hovered.run_if(inventory_open),
                    debug_cast,
                    cast_requests,
                    cast_effects,
                    dress_baubles.run_if(resource_exists::<Assets<WispMaterial>>),
                    follow_the_caster,
                    fade_the_light,
                    shimmer_baubles.run_if(resource_exists::<Assets<WispMaterial>>),
                    spawn_hud,
                    show_chosen,
                    show_slot_labels,
                )
                    .chain()
                    .in_set(DelveSystems),
            );
        crate::casting_influence::register(app);
    }
}

/// Asks for a cast of the written prayer or spell called `name`. Sent by the Pray key and the
/// quickslots, and by anything else that casts.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct CastRequest {
    pub name: String,
    pub tier: u8,
}

/// A cast that went through, its mana spent. The prayer's effect reacts to this: the light term's
/// light is filled in on it.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct Casted(pub Cast);

/// The prayer (by name) and tier the Pray key and the quickslots cast.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct Chosen {
    pub name: String,
    pub tier: u8,
}

impl Default for Chosen {
    fn default() -> Self {
        Chosen { name: "Divine Light".into(), tier: 1 }
    }
}

/// The written prayer held by each quickslot, by name. A slot holds an item or a prayer, never both.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct AbilitySlots(pub [Option<String>; SLOTS]);

impl AbilitySlots {
    /// The prayer in `slot`.
    pub fn get(&self, slot: usize) -> Option<&str> {
        self.0.get(slot).and_then(|s| s.as_deref())
    }

    /// Holds `name` in `slot` alone: it leaves any other slot.
    pub fn bind(&mut self, slot: usize, name: &str) {
        if slot >= SLOTS {
            return;
        }
        for bound in &mut self.0 {
            if bound.as_deref() == Some(name) {
                *bound = None;
            }
        }
        self.0[slot] = Some(name.to_string());
    }
}

/// A new caster's spellbook starts as their class's: the known terms, and for a cleric Divine
/// Light. A spellbook already filled (a delve resumed) is left alone.
fn start_the_spellbook(mut players: Query<&mut DelverSpellbook>, character: Option<Res<Character>>, rules: Res<GameRules>, mut chosen: ResMut<Chosen>) {
    let Some(character) = character else { return };
    for mut book in &mut players {
        if book.0 == Spellbook::default() {
            let start = Spellbook::starting_with(&rules, character.class.casting());
            if start != Spellbook::default() {
                book.0 = start;
            }
        }
        // The chosen prayer is one the book holds.
        if book.0.find(&chosen.name).is_none() {
            chosen.name = book.0.written.first().map_or_else(String::new, |w| w.name.clone());
        }
    }
}

fn forget_the_abilities(mut slots: ResMut<AbilitySlots>, mut chosen: ResMut<Chosen>) {
    *slots = AbilitySlots::default();
    *chosen = Chosen::default();
}

/// A new cleric holds Divine Light in the first slot with nothing in it.
fn give_clerics_their_prayer(new: Query<(), Added<Player>>, character: Option<Res<Character>>, items: Res<Quickslots>, mut slots: ResMut<AbilitySlots>) {
    if new.is_empty() || character.is_none_or(|c| c.class.casting() != Casting::Divine) || slots.0.iter().any(Option::is_some) {
        return;
    }
    if let Some(free) = (0..SLOTS).find(|&s| items.get(s).is_none()) {
        slots.bind(free, "Divine Light");
    }
}

/// An item bound to a slot takes it from an ability.
fn items_displace_abilities(items: Res<Quickslots>, mut slots: ResMut<AbilitySlots>) {
    if !items.is_changed() {
        return;
    }
    for slot in 0..SLOTS {
        if items.get(slot).is_some() && slots.0[slot].is_some() {
            slots.0[slot] = None;
        }
    }
}

/// The tier key steps the chosen tier and says what it costs.
fn step_tier(input: ActionInput, mut chosen: ResMut<Chosen>, rules: Res<GameRules>, players: Query<&DelverSpellbook, With<Player>>, mut toast: ResMut<Toast>, time: Option<Res<Time<Real>>>) {
    if !input.just_pressed(Action::CastTier) {
        return;
    }
    chosen.tier = chosen.tier % MAX_TIER + 1;
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    let preview = players.iter().next().and_then(|b| b.0.find(&chosen.name)).map(|w| w.preview_with(&rules, chosen.tier));
    if let Some(p) = preview {
        toast.say(format!("{} tier {}: {} mana, {} ft", chosen.name, chosen.tier, points(p.cost), points(p.range)), now);
    }
}

fn pray_key(input: ActionInput, chosen: Res<Chosen>, mut requests: MessageWriter<CastRequest>) {
    if input.just_pressed(Action::Pray) {
        requests.write(CastRequest { name: chosen.name.clone(), tier: chosen.tier });
    }
}

/// A slot's key casts the prayer in it at the chosen tier.
fn ability_slot_key(input: ActionInput, slots: Res<AbilitySlots>, chosen: Res<Chosen>, mut requests: MessageWriter<CastRequest>) {
    let Some(slot) = input.just_pressed_index(Action::Quickslot) else { return };
    if let Some(name) = slots.get(slot) {
        requests.write(CastRequest { name: name.to_string(), tier: chosen.tier });
    }
}

/// In the inventory a slot's key over no item binds the chosen prayer to the slot.
fn bind_ability_hovered(
    input: ActionInput,
    hovered: Res<Hovered>,
    chosen: Res<Chosen>,
    players: Query<&DelverSpellbook, With<Player>>,
    mut slots: ResMut<AbilitySlots>,
    items: Res<Quickslots>,
    mut toast: ResMut<Toast>,
    time: Option<Res<Time<Real>>>,
) {
    let Some(slot) = input.just_pressed_index(Action::Quickslot) else { return };
    if hovered.0.is_some() || !players.iter().next().is_some_and(|b| b.0.find(&chosen.name).is_some()) {
        return;
    }
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    if items.get(slot).is_some() {
        toast.say(format!("Slot {} already holds an item", crate::quickslots::number(slot)), now);
        return;
    }
    slots.bind(slot, &chosen.name);
    toast.say(format!("{} is bound to slot {}", chosen.name, crate::quickslots::number(slot)), now);
}

/// Does each request: spends the mana and sends [`Casted`], or says why not.
pub(crate) fn cast_requests(
    mut requests: MessageReader<CastRequest>,
    mut done: MessageWriter<Casted>,
    character: Option<Res<Character>>,
    rules: Res<GameRules>,
    mut players: Query<(&mut DelverMana, &DelverSpellbook), With<Player>>,
    mut toast: ResMut<Toast>,
    time: Option<Res<Time<Real>>>,
) {
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    for request in requests.read() {
        let Some((mut mana, book)) = players.iter_mut().next() else { continue };
        if character.as_deref().is_some_and(|c| c.class.casting() == Casting::Martial) {
            toast.say(CastError::WrongCaster { needs: Casting::Martial }.say(&request.name), now);
            continue;
        }
        let Some(written) = book.0.find(&request.name) else {
            toast.say(format!("You have not written {}", request.name), now);
            continue;
        };
        let casting = character.as_deref().map_or(Casting::Martial, |c| c.class.casting());
        let mut pool = mana.0;
        match cast_with(&rules, &mut pool, casting, &book.0.unlocked, written, request.tier) {
            Ok(cast) => {
                mana.0 = pool;
                toast.say(format!("You cast {}, tier {}: {} mana", cast.written.name, cast.tier, points(cast.cost)), now);
                done.write(Casted(cast));
            }
            Err(why) => {
                toast.say(why.say(&request.name), now);
            }
        }
    }
}

/// The colour of Divine Light: a soft pale gold, not a flame's orange.
pub const HOLY_LIGHT: Rgb = Rgb::hex(0xfff8e8);
/// The bauble's rim and its motes, a warmer gold than its light so the glow reads holy.
const HOLY_RIM: Color = Color::srgb(1.0, 0.82, 0.45);
/// The bauble's middle, nearly white.
const HOLY_CORE: Color = Color::srgb(1.0, 0.98, 0.9);
/// How high above the caster's eye the bauble floats, world units.
const BAUBLE_RISE: f32 = 0.9;
/// How far ahead of the caster it floats, world units.
const BAUBLE_AHEAD: f32 = 14.0;
/// How far to the right of where they look, world units, so it does not sit on the aim.
const BAUBLE_ASIDE: f32 = 3.5;
/// Where a fresh bauble gathers, from the caster's eye (right, up, ahead): down by the hands
/// and a little ahead.
const BAUBLE_GATHER: Vec3 = Vec3::new(1.5, -3.0, 4.0);
/// How near a wall or a shut door it comes along its line from the eye, world units (30 cm),
/// so it never sinks into the stone and its light is never cut off by it.
const BAUBLE_CLEARANCE: f32 = 3.0;
/// The width of the card its glowing heart is drawn on, world units; the glow fades well
/// inside it.
const HEART_SIZE: f32 = 4.2;
/// The width of a mote's card, world units.
const MOTE_SIZE: f32 = 0.5;
/// How quickly it catches up with the caster, per second.
const BAUBLE_FOLLOW: f32 = 5.0;
/// Game seconds a bauble takes to gutter out at the end of its life, at most: about ten real
/// seconds at the usual clock, long enough to see it dying.
const FADE_SECONDS: f64 = 120.0;
/// Real seconds a fresh bauble takes to gather and bloom.
const RISE_SECONDS: f32 = 1.2;
/// Flicker of the holy light: nearly steady.
const HOLY_FLICKER: f32 = 0.08;

/// The orbits motes swirl along: the axis and angle each is tilted by, how fast its motes go
/// round (radians a second, the sign the way round) and its radius in world units. Each also
/// turns slowly about the up, so the orbits never settle into a fixed atom.
const ORBITS: [(Vec3, f32, f32, f32); 3] = [(Vec3::X, 0.5, 2.2, 1.0), (Vec3::Z, -1.1, -1.7, 1.15), (Vec3::new(1.0, 0.0, 1.0), 1.9, 1.35, 0.88)];
/// Streams of motes on each orbit, spaced evenly round it.
const STREAMS: u8 = 2;
/// Motes in a stream: a lead mote and the tail that trails it, shrinking.
const TRAIL: u8 = 8;
/// How far behind each mote of a tail falls, radians round its orbit.
const TRAIL_STEP: f32 = 0.085;
/// How fast the orbits turn about the up, radians a second.
const PRECESS: f32 = 0.4;

/// The bauble a cast of Divine Light leaves floating by the caster: a real light in the level's
/// lighting ([`crate::carry::GivenLight`]) of `feet` reach, until `expires` (game seconds),
/// guttering out at the end.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[require(DespawnOnExit<AppState> = DespawnOnExit(AppState::Delving))]
pub struct DivineLight {
    /// Feet the light reaches: the cast's range.
    pub feet: f32,
    /// The game time it goes out, seconds.
    pub expires: f64,
    /// Game seconds it fades over at the end.
    pub fade: f64,
    /// The light's intensity at full, set when it first lights.
    pub peak: f32,
    /// The real time it was cast (`Time::elapsed_secs`), which its gathering is timed from.
    pub cast_at: f32,
}

impl DivineLight {
    /// How bright it is at game time `now`: 1 until it begins to fade, then down to 0.
    pub fn brightness(&self, now: f64) -> f32 {
        if self.fade <= 0.0 {
            return if now < self.expires { 1.0 } else { 0.0 };
        }
        ((self.expires - now) / self.fade).clamp(0.0, 1.0) as f32
    }
}

/// How the bauble shows at a moment: its light and its heart as shares of their fullest, how
/// far its motes stand out from their orbits (1 on them), the motes' size as a share of
/// theirs, and how far round they have been swept besides their orbit, radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    pub light: f32,
    pub heart: f32,
    pub spread: f32,
    pub motes: f32,
    pub swirl: f32,
}

/// The bauble `age` real seconds after it was cast, with `left` of its brightness
/// ([`DivineLight::brightness`]), at real time `t` for its sputter. Cast, its motes gather
/// from wide and spiral in while its heart blooms past its size and settles
/// ([`RISE_SECONDS`]). Dying, it sputters harder the nearer it is to out, its heart shrinks
/// and its motes loosen from their orbits.
pub fn glow(age: f32, left: f32, t: f32) -> Glow {
    let risen = (age / RISE_SECONDS).clamp(0.0, 1.0);
    // An ease that overshoots and settles back: a bloom.
    let (c1, c3) = (1.70158, 2.70158);
    let bloom = (1.0 + c3 * (risen - 1.0).powi(3) + c1 * (risen - 1.0).powi(2)).max(0.0);
    let dying = 1.0 - left.clamp(0.0, 1.0);
    let flutter = (0.5 + 0.5 * (t * 17.0 + 3.0 * (t * 5.3).sin()).sin()) * (0.5 + 0.5 * (t * 29.0 + 1.3).sin());
    let sputter = 1.0 - 0.7 * dying.powf(1.5) * flutter;
    let gathering = (1.0 - risen).powi(2);
    Glow {
        light: bloom * left * sputter,
        heart: bloom * (0.3 + 0.7 * left) * sputter.sqrt(),
        spread: 1.0 + 2.4 * gathering + 0.9 * dying,
        motes: (risen / 0.6).min(1.0) * (0.25 + 0.75 * left),
        swirl: 5.0 * gathering,
    }
}

/// How much of orbit `orbit` still shows with `left` of the bauble's brightness: dying, its
/// orbits go out one after another, the last a little before the heart.
fn orbit_left(orbit: usize, left: f32) -> f32 {
    let at = 0.1 + 0.15 * orbit as f32;
    ((left - at) / 0.15).clamp(0.0, 1.0)
}

/// Where mote `trail` of stream `stream` on orbit `orbit` is, about the bauble's middle, at
/// real time `t` with `glow`.
fn mote_at(orbit: usize, stream: u8, trail: u8, t: f32, glow: &Glow) -> Vec3 {
    let (axis, tilt, speed, radius) = ORBITS[orbit];
    let turn = Quat::from_rotation_y(t * PRECESS + orbit as f32 * 2.1) * Quat::from_axis_angle(axis.normalize(), tilt);
    let way = speed.signum();
    let angle = t * speed + glow.swirl * way + std::f32::consts::TAU * stream as f32 / STREAMS as f32 - way * TRAIL_STEP * trail as f32;
    // Each mote breathes in and out a little on its own, so a stream is never a rigid ring.
    let breathe = 1.0 + 0.06 * (t * 3.0 + angle * 2.0 + orbit as f32).sin();
    turn * Vec3::new(angle.cos(), 0.0, angle.sin()) * radius * breathe * glow.spread
}

/// The reach in feet of the light cast on the delver, if one burns: what
/// [`crate::character::Character::gear_with_spell`] counts as a carried light.
pub fn spell_light_feet<'a>(lights: impl IntoIterator<Item = &'a DivineLight>) -> Option<f64> {
    lights.into_iter().map(|l| l.feet as f64).reduce(f64::max)
}

/// The level's walls and shut doors as the delver meets them, if a level is loaded.
#[derive(bevy::ecs::system::SystemParam)]
struct Walls<'w> {
    level: Option<Res<'w, crate::play::CurrentLevel>>,
    blocked: Option<Res<'w, crate::play::BlockedCells>>,
    doors: Option<Res<'w, crate::doors::Doors>>,
}

impl Walls<'_> {
    /// `to` drawn back along its line from `from` to short of the first wall or shut door
    /// ([`clear_of_walls`]); `to` itself with no level.
    fn clear(&self, from: Vec3, to: Vec3) -> Vec3 {
        let Some(level) = self.level.as_deref() else { return to };
        let none = crate::play::BlockedCells::default();
        let cells = self.blocked.as_deref().unwrap_or(&none);
        clear_of_walls(from, to, crate::doors::solid_for(&level.0, cells, self.doors.as_deref(), None))
    }
}

/// `to`, or the point on the line from `from` to it [`BAUBLE_CLEARANCE`] short of the first
/// cell `solid` says is a wall or a shut door (never behind `from`): where something floating
/// from `from` toward `to` stops. A `to` just short of a wall is drawn back from it too.
pub fn clear_of_walls(from: Vec3, to: Vec3, solid: impl Fn(i32, i32) -> bool) -> Vec3 {
    let span = to - from;
    let length = span.xz().length();
    if length < 1e-4 {
        return to;
    }
    let cell = |p: Vec3| ((p.x / fb_level::CELL).floor() as i32, (p.z / fb_level::CELL).floor() as i32);
    let reach = length + BAUBLE_CLEARANCE;
    let steps = (reach / 0.5).ceil() as usize;
    for i in 1..=steps {
        let along = reach * i as f32 / steps as f32;
        let (x, y) = cell(from + span * (along / length));
        if solid(x, y) {
            let back = ((along - BAUBLE_CLEARANCE) / length).clamp(0.0, 1.0);
            return from + span * back;
        }
    }
    to
}

/// Does what each cast says. This is the one place a prayer's effect is read, from
/// `cast.written.effect`; a new effect is another arm.
#[allow(clippy::too_many_arguments)]
fn cast_effects(
    mut casts: MessageReader<Casted>,
    mut commands: Commands,
    clock: Option<Res<GameClock>>,
    time: Res<Time>,
    tuning: Option<Res<Tuning>>,
    players: Query<&GlobalTransform, With<Player>>,
    cameras: Query<&GlobalTransform, With<PlayerCamera>>,
    old: Query<Entity, With<DivineLight>>,
    walls: Walls,
) {
    for Casted(cast) in casts.read() {
        if cast.written.effect == "light" {
            let now = clock.as_deref().map_or(0.0, GameClock::game_seconds);
            let tuning = tuning.as_deref().cloned().unwrap_or_default();
            // Recasting replaces the light: the old one goes out, never two.
            for light in &old {
                commands.entity(light).despawn();
            }
            // It gathers by the caster's hands and floats out to its place.
            let at = match players.iter().next() {
                Some(player) => {
                    let eye = player.translation() + Vec3::Y * EYE_HEIGHT;
                    let (forward, right) = facing(cameras.iter().next());
                    walls.clear(eye, eye + right * BAUBLE_GATHER.x + Vec3::Y * BAUBLE_GATHER.y + forward * BAUBLE_GATHER.z)
                }
                None => Vec3::Y * (EYE_HEIGHT + BAUBLE_RISE),
            };
            set_divine_light(&mut commands, cast, now, time.elapsed_secs(), at, &tuning);
        }
    }
}

/// The bauble lit for `cast` at `at`, cast at real time `cast_at`.
fn set_divine_light(commands: &mut Commands, cast: &fb_character::casting::Cast, now: f64, cast_at: f32, at: Vec3, tuning: &Tuning) {
    let anchor = LightAnchor::feet([0.0; 3], HOLY_LIGHT, cast.range as f32, HOLY_FLICKER);
    let lit = holy_light(anchor, bevy::camera::Exposure::INDOOR, &tuning.light);
    let peak = lit.0.intensity;
    commands.spawn((
        Name::new(format!("Divine Light, tier {}", cast.tier)),
        DivineLight { feet: cast.range as f32, expires: now + cast.duration, fade: FADE_SECONDS.min(cast.duration * 0.25), peak, cast_at },
        lit,
        Transform::from_translation(at),
        Visibility::default(),
    ));
}

/// The way the eye looks along the floor, and its right, from the camera.
fn facing(camera: Option<&GlobalTransform>) -> (Vec3, Vec3) {
    match camera {
        Some(c) => {
            let f = Vec3::new(c.forward().x, 0.0, c.forward().z).normalize_or(Vec3::NEG_Z);
            (f, Vec3::new(-f.z, 0.0, f.x))
        }
        None => (Vec3::NEG_Z, Vec3::X),
    }
}

/// Where the bauble floats for a caster at `player`: above the eye, a little ahead and aside.
fn bauble_goal(player: &GlobalTransform, camera: Option<&GlobalTransform>) -> Vec3 {
    let (forward, right) = facing(camera);
    player.translation() + Vec3::Y * (EYE_HEIGHT + BAUBLE_RISE) + forward * BAUBLE_AHEAD + right * BAUBLE_ASIDE
}

/// The bauble drifts after its caster, easing toward its place and bobbing a little, and stays
/// on the caster's side of every wall and shut door: its place is drawn back short of them,
/// and so is wherever it has drifted, so it never passes through one.
fn follow_the_caster(
    time: Res<Time>,
    players: Query<&GlobalTransform, With<Player>>,
    cameras: Query<&GlobalTransform, With<PlayerCamera>>,
    mut baubles: Query<&mut Transform, (With<DivineLight>, Without<Player>)>,
    walls: Walls,
) {
    let Some(player) = players.iter().next() else { return };
    let eye = player.translation() + Vec3::Y * EYE_HEIGHT;
    let goal = bauble_goal(player, cameras.iter().next()) + Vec3::Y * (time.elapsed_secs() * 2.0).sin() * 0.25;
    let goal = walls.clear(eye, goal);
    let ease = 1.0 - (-BAUBLE_FOLLOW * time.delta_secs()).exp();
    for mut at in &mut baubles {
        at.translation = walls.clear(eye, at.translation.lerp(goal, ease));
    }
}

/// Sets each bauble's light by its [`glow`] and puts it out when its time is up.
fn fade_the_light(mut commands: Commands, clock: Option<Res<GameClock>>, time: Res<Time>, mut baubles: Query<(Entity, &DivineLight, &mut PointLight)>) {
    let Some(clock) = clock else { return };
    let now = clock.game_seconds();
    let t = time.elapsed_secs();
    for (entity, bauble, mut light) in &mut baubles {
        let left = bauble.brightness(now);
        if left <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let intensity = bauble.peak * glow(t - bauble.cast_at, left, t).light;
        if light.intensity != intensity {
            light.intensity = intensity;
        }
    }
}

/// The bauble's glowing heart, a child of its [`DivineLight`].
#[derive(Component, Clone, Copy, Debug)]
struct BaubleHeart;

/// One mote of a bauble: mote `trail` of stream `stream` on orbit `orbit`.
#[derive(Component, Clone, Copy, Debug)]
struct BaubleMote {
    orbit: u8,
    stream: u8,
    trail: u8,
}

/// Gives each new bauble its look: a wavering heart of light ([`WispMaterial::heart`]) and
/// streams of motes swirling round it on [`ORBITS`], all drawn on cards turned to the eye.
fn dress_baubles(
    mut commands: Commands,
    new: Query<Entity, Added<DivineLight>>,
    mut shared: Local<Option<(Handle<Mesh>, Handle<WispMaterial>)>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WispMaterial>>,
) {
    for bauble in &new {
        let (card, mote) = shared.get_or_insert_with(|| (meshes.add(wisp_card()), materials.add(WispMaterial::mote(HOLY_RIM)))).clone();
        let phase = (bauble.to_bits() % 997) as f32 * 0.37;
        commands.spawn((
            Name::new("Bauble heart"),
            BaubleHeart,
            Mesh3d(card.clone()),
            MeshMaterial3d(materials.add(WispMaterial::heart(HOLY_RIM, HOLY_CORE, phase))),
            NotShadowCaster,
            Transform::from_scale(Vec3::ZERO),
            ChildOf(bauble),
        ));
        for orbit in 0..ORBITS.len() as u8 {
            for stream in 0..STREAMS {
                for trail in 0..TRAIL {
                    commands.spawn((
                        BaubleMote { orbit, stream, trail },
                        Mesh3d(card.clone()),
                        MeshMaterial3d(mote.clone()),
                        NotShadowCaster,
                        Transform::from_scale(Vec3::ZERO),
                        ChildOf(bauble),
                    ));
                }
            }
        }
    }
}

/// Moves each bauble's heart and motes by its [`glow`]: the heart blooms, sputters and shrinks,
/// the motes swirl round their orbits, every card turned to the eye.
#[allow(clippy::type_complexity)]
fn shimmer_baubles(
    time: Res<Time>,
    clock: Option<Res<GameClock>>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    baubles: Query<&DivineLight>,
    mut hearts: Query<(&ChildOf, &mut Transform, &MeshMaterial3d<WispMaterial>), (With<BaubleHeart>, Without<BaubleMote>)>,
    mut motes: Query<(&ChildOf, &BaubleMote, &mut Transform), Without<BaubleHeart>>,
    mut materials: ResMut<Assets<WispMaterial>>,
) {
    let now = clock.as_deref().map_or(0.0, GameClock::game_seconds);
    let t = time.elapsed_secs();
    let facing = eyes.iter().next().map_or(Quat::IDENTITY, |eye| eye.rotation());
    let glow_of = |bauble: &DivineLight| {
        let left = bauble.brightness(now);
        (glow(t - bauble.cast_at, left, t), left)
    };
    for (parent, mut at, material) in &mut hearts {
        let Ok(bauble) = baubles.get(parent.parent()) else { continue };
        let (glow, _) = glow_of(bauble);
        *at = Transform::from_rotation(facing).with_scale(Vec3::splat(HEART_SIZE * glow.heart.max(0.0).sqrt()));
        if let Some(mut look) = materials.get_mut(&material.0) {
            look.strength = glow.heart;
        }
    }
    for (parent, mote, mut at) in &mut motes {
        let Ok(bauble) = baubles.get(parent.parent()) else { continue };
        let (glow, left) = glow_of(bauble);
        let orbit = mote.orbit as usize;
        let tail = 1.0 - mote.trail as f32 / TRAIL as f32;
        let twinkle = 0.8 + 0.2 * (t * 9.0 + (mote.trail as f32 + 3.0 * mote.stream as f32 + 7.0 * orbit as f32)).sin();
        let size = MOTE_SIZE * glow.motes * orbit_left(orbit, left) * tail.powf(1.2) * twinkle;
        *at = Transform::from_translation(mote_at(orbit, mote.stream, mote.trail, t, &glow)).with_rotation(facing).with_scale(Vec3::splat(size));
    }
}

/// `FB_PRAY=1` casts once, a moment into the delve; `FB_PRAY_TIER=2` chooses the tier first and
/// `FB_PRAY_DRAIN=1` empties the pool to show a failed cast. `FB_PRAY_TURN=180` turns the
/// delver that many degrees first (back at the door behind them), and `FB_PRAY_LASTS=150` has
/// the light last only that many game seconds, to see it gutter out. For captures.
#[allow(clippy::type_complexity)]
fn debug_cast(
    mut frames: Local<u32>,
    mut chosen: ResMut<Chosen>,
    mut requests: MessageWriter<CastRequest>,
    mut players: Query<(&mut DelverMana, Option<&mut crate::player::Locomotion>), With<Player>>,
    mut lights: Query<&mut DivineLight, Added<DivineLight>>,
    clock: Option<Res<GameClock>>,
) {
    *frames += 1;
    let number = |name: &str| std::env::var(name).ok().and_then(|v| v.trim().parse::<f64>().ok());
    if *frames == 60
        && let Some(turn) = number("FB_PRAY_TURN")
        && let Some((_, Some(mut walk))) = players.iter_mut().next()
    {
        walk.yaw += (turn as f32).to_radians();
    }
    if let Some(lasts) = number("FB_PRAY_LASTS") {
        let now = clock.as_deref().map_or(0.0, GameClock::game_seconds);
        for mut light in &mut lights {
            light.expires = now + lasts;
            light.fade = light.fade.min(lasts);
        }
    }
    if *frames == 60 {
        if let Some(tier) = std::env::var("FB_PRAY_TIER").ok().and_then(|v| v.trim().parse::<u8>().ok()) {
            chosen.tier = tier.clamp(1, MAX_TIER);
        }
        if std::env::var("FB_PRAY_DRAIN").is_ok_and(|v| !v.is_empty())
            && let Some((mut mana, _)) = players.iter_mut().next()
        {
            mana.0.points = 2.0;
        }
    }
    if *frames == 90 && std::env::var("FB_PRAY").is_ok_and(|v| !v.is_empty()) {
        requests.write(CastRequest { name: chosen.name.clone(), tier: chosen.tier });
    }
}

/// The line over the quickslot bar naming the chosen ability.
#[derive(Component)]
struct ChosenLine;

/// The ability's name and tier inside a slot.
#[derive(Component, Clone, Copy, Debug)]
pub struct AbilityLabel(pub usize);

fn spawn_hud(mut commands: Commands, clusters: Query<Entity, Added<OrbCluster>>, fonts: Option<Res<UiFonts>>) {
    let Some(fonts) = fonts else { return };
    let layout = cluster_layout();
    for cluster in &clusters {
        commands.entity(cluster).with_children(|cluster| {
            // Above the row of status badges (which sits on the bar), not on it.
            cluster.spawn((
                ChosenLine,
                Text::new(""),
                TextFont::from_font_size(hud_font(2.0)).with_font(fonts.body_medium.clone()),
                TextColor(BRIGHT),
                TextShadow { offset: Vec2::splat(1.5), color: Color::BLACK.with_alpha(0.85) },
                Node { position_type: PositionType::Absolute, left: hud(layout.bar.min.x), top: hud(layout.status.min.y - 2.9), ..default() },
                Pickable::IGNORE,
            ));
        });
    }
}

/// "Divine Light, tier 2, 6 mana, 40 ft (C pray, V tier)" for a caster; nothing for anyone else.
fn show_chosen(chosen: Res<Chosen>, rules: Res<GameRules>, keymap: Res<Keymap>, players: Query<&DelverSpellbook, With<Player>>, mut lines: Query<&mut Text, With<ChosenLine>>) {
    let text = match players.iter().next().and_then(|b| b.0.find(&chosen.name)) {
        Some(w) => {
            let p = w.preview_with(&rules, chosen.tier);
            format!("{}, tier {}, {} mana, {} ft  ({} pray, {} tier)", w.name, chosen.tier, points(p.cost), points(p.range), keymap.label(Action::Pray), keymap.label(Action::CastTier))
        }
        None => String::new(),
    };
    for mut line in &mut lines {
        if line.0 != text {
            line.0 = text.clone();
        }
    }
}

/// Each slot holding a prayer names it, along its bottom edge.
fn show_slot_labels(slots: Res<AbilitySlots>, mut labels: Query<(&AbilityLabel, &mut Text)>) {
    for (label, mut text) in &mut labels {
        let want = slots.get(label.0).map_or_else(String::new, |name| name.split(' ').next_back().unwrap_or(name).chars().take(5).collect());
        if text.0 != want {
            text.0 = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::state::app::StatesPlugin;

    use super::*;
    use crate::play::{ScreenOpen, run_delve_systems_while_delving};

    fn delve(class_seed_cleric: bool, mana: f64) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Delving)
            .init_resource::<Keymap>()
            .init_resource::<Quickslots>()
            .init_resource::<Hovered>()
            .init_resource::<Toast>()
            .init_resource::<ScreenOpen>()
            .init_resource::<GameRules>()
            .init_resource::<crate::game_clock::GameClock>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(crate::combat::RunDice(procgen_rng::Mulberry32::new(7)))
            .add_plugins(CastingPlugin);
        run_delve_systems_while_delving(&mut app);
        let mut character = Character::roll(1);
        character.class = if class_seed_cleric { crate::character::Class::Cleric } else { crate::character::Class::Fighter };
        app.insert_resource(character);
        let player = app.world_mut().spawn((Player, GlobalTransform::default(), DelverSpellbook::default(), DelverMana(fb_character::Mana { points: mana, max: 10.0 }))).id();
        app.update();
        (app, player)
    }

    fn pool(app: &App, player: Entity) -> f64 {
        app.world().get::<DelverMana>(player).unwrap().0.points
    }

    fn casted(app: &mut App) -> Vec<Cast> {
        app.world_mut().resource_mut::<Messages<Casted>>().drain().map(|c| c.0).collect()
    }

    fn press(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
    }

    #[test]
    fn a_tier_two_prayer_with_ten_mana_spends_six_and_leaves_four() {
        let (mut app, player) = delve(true, 10.0);
        app.world_mut().resource_mut::<Chosen>().tier = 2;
        press(&mut app, KeyCode::KeyC);
        assert_eq!(pool(&app, player), 4.0);
        let casts = casted(&mut app);
        assert_eq!(casts.len(), 1);
        assert_eq!((casts[0].written.name.as_str(), casts[0].tier, casts[0].cost), ("Divine Light", 2, 6.0));
        assert!(app.world().resource::<Toast>().text.contains("tier 2"));
    }

    #[test]
    fn too_little_mana_fails_and_says_so() {
        let (mut app, player) = delve(true, 5.0);
        app.world_mut().resource_mut::<Chosen>().tier = 2;
        press(&mut app, KeyCode::KeyC);
        assert_eq!(pool(&app, player), 5.0, "nothing spent");
        assert!(casted(&mut app).is_empty());
        assert!(app.world().resource::<Toast>().text.starts_with("Not enough mana"));
    }

    #[test]
    fn only_clerics_pray() {
        let (mut app, player) = delve(false, 10.0);
        press(&mut app, KeyCode::KeyC);
        assert_eq!(pool(&app, player), 10.0);
        assert!(casted(&mut app).is_empty());
        assert!(app.world().resource::<Toast>().text.starts_with("You cannot cast"));
    }

    #[test]
    fn the_tier_key_steps_one_to_three_and_round() {
        let (mut app, _) = delve(true, 10.0);
        let mut seen = vec![];
        for _ in 0..4 {
            press(&mut app, KeyCode::KeyV);
            seen.push(app.world().resource::<Chosen>().tier);
        }
        assert_eq!(seen, vec![2, 3, 1, 2]);
        assert!(app.world().resource::<Toast>().text.contains("6 mana"));
    }

    #[test]
    fn a_cleric_holds_divine_light_in_a_slot_and_its_key_casts_it() {
        let (mut app, player) = delve(true, 10.0);
        assert_eq!(app.world().resource::<AbilitySlots>().get(0), Some("Divine Light"));
        press(&mut app, KeyCode::Digit1);
        assert_eq!(pool(&app, player), 7.0);
        assert_eq!(casted(&mut app).len(), 1);
    }

    #[test]
    fn the_chosen_prayer_is_what_c_casts() {
        let (mut app, player) = delve(true, 10.0);
        let mut mercy = fb_character::casting::Written::divine_light();
        mercy.name = "Hand of Mercy".into();
        mercy.reach = "touch".into();
        app.world_mut().get_mut::<DelverSpellbook>(player).unwrap().0.save(mercy).unwrap();
        app.world_mut().resource_mut::<Chosen>().name = "Hand of Mercy".into();
        press(&mut app, KeyCode::KeyC);
        let casts = casted(&mut app);
        assert_eq!(casts.len(), 1);
        assert_eq!((casts[0].written.name.as_str(), casts[0].written.reach.as_str(), casts[0].cost), ("Hand of Mercy", "touch", 2.0));
        assert_eq!(pool(&app, player), 8.0);
    }

    #[test]
    fn a_fighter_gets_no_prayer_in_a_slot() {
        let (app, _) = delve(false, 10.0);
        assert_eq!(app.world().resource::<AbilitySlots>().get(0), None);
    }

    #[test]
    fn an_item_bound_to_the_slot_takes_it_from_the_ability() {
        let (mut app, _) = delve(true, 10.0);
        app.world_mut().resource_mut::<Quickslots>().bind(0, crate::inventory::ItemId(9));
        app.update();
        assert_eq!(app.world().resource::<AbilitySlots>().get(0), None);
    }

    #[test]
    fn a_slot_holds_an_item_or_a_prayer_and_binding_refuses_to_clear_an_item() {
        let (mut app, _) = delve(true, 10.0);
        app.world_mut().resource_mut::<Quickslots>().bind(2, crate::inventory::ItemId(9));
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(crate::play::Screen::Inventory);
        press(&mut app, KeyCode::Digit3);
        assert_eq!(app.world().resource::<AbilitySlots>().get(2), None, "refused over an item");
        assert_eq!(app.world().resource::<Quickslots>().get(2), Some(crate::inventory::ItemId(9)));
        assert!(app.world().resource::<Toast>().text.contains("already holds an item"));
        press(&mut app, KeyCode::Digit4);
        assert_eq!(app.world().resource::<AbilitySlots>().get(3), Some("Divine Light"));
        assert_eq!(app.world().resource::<AbilitySlots>().get(0), None, "a prayer sits in one slot only");
    }

    #[test]
    fn the_chosen_prayer_follows_the_book_and_a_late_character_fills_it() {
        let (mut app, player) = delve(false, 10.0);
        assert_eq!(app.world().resource::<Chosen>().name, "", "a fighter chooses nothing");
        app.world_mut().resource_mut::<Character>().class = crate::character::Class::Cleric;
        app.update();
        assert_eq!(app.world().get::<DelverSpellbook>(player).unwrap().0.written.len(), 1);
        assert_eq!(app.world().resource::<Chosen>().name, "Divine Light");
    }

    fn lit(app: &mut App) -> Vec<(DivineLight, crate::carry::GivenLight, Vec3)> {
        app.world_mut().query::<(&DivineLight, &crate::carry::GivenLight, &Transform)>().iter(app.world()).map(|(d, g, t)| (*d, *g, t.translation)).collect()
    }

    fn cast_at_tier(app: &mut App, tier: u8) {
        app.world_mut().resource_mut::<Chosen>().tier = tier;
        press(app, KeyCode::KeyC);
    }

    #[test]
    fn divine_light_reaches_twenty_forty_and_sixty_feet_by_tier() {
        for (tier, units) in [(1, 61.0), (2, 122.0), (3, 183.0)] {
            let (mut app, _) = delve(true, 20.0);
            cast_at_tier(&mut app, tier);
            let lights = lit(&mut app);
            assert_eq!(lights.len(), 1, "tier {tier}");
            assert!((lights[0].1.anchor.radius - units).abs() < 0.5, "tier {tier}: {}", lights[0].1.anchor.radius);
            assert_eq!(lights[0].0.feet, 20.0 * tier as f32);
        }
    }

    #[test]
    fn the_light_is_pale_gold_and_a_real_point_light() {
        let (mut app, _) = delve(true, 10.0);
        cast_at_tier(&mut app, 1);
        let (light, given, bauble) = app.world_mut().query::<(&PointLight, &crate::carry::GivenLight, &DivineLight)>().single(app.world()).map(|(l, g, d)| (*l, *g, *d)).unwrap();
        // It blooms up to its peak from the cast ([`glow`]).
        assert!(bauble.peak > 0.0 && light.intensity <= bauble.peak && light.range > given.anchor.radius);
        let Rgb(r, g, b) = given.anchor.color;
        assert!(r >= g && g > b && b > 0.4, "pale gold, not orange: {r} {g} {b}");
    }

    #[test]
    fn the_light_follows_the_player() {
        let (mut app, player) = delve(true, 10.0);
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(50)));
        cast_at_tier(&mut app, 1);
        let before = lit(&mut app)[0].2;
        let to = Vec3::new(120.0, 0.0, -40.0);
        app.world_mut().entity_mut(player).insert(GlobalTransform::from_translation(to));
        for _ in 0..120 {
            app.update();
        }
        let after = lit(&mut app)[0].2;
        assert!(before.distance(to) > 60.0, "it began elsewhere");
        let hover = Vec3::new(after.x - to.x, after.y - to.y, after.z - to.z);
        assert!((hover.xz().length() - BAUBLE_AHEAD.hypot(BAUBLE_ASIDE)).abs() < 1.0 && (hover.y - (EYE_HEIGHT + BAUBLE_RISE)).abs() < 1.0, "it arrived above and a little ahead: {hover}");
    }

    #[test]
    fn the_light_goes_out_when_its_time_is_up() {
        let (mut app, _) = delve(true, 10.0);
        cast_at_tier(&mut app, 1);
        let duration = lit(&mut app)[0].0.expires;
        assert!(duration > 0.0);
        app.world_mut().resource_mut::<GameClock>().advance(duration * 0.5);
        app.update();
        assert_eq!(lit(&mut app).len(), 1, "still burning at half time");
        app.world_mut().resource_mut::<GameClock>().advance(duration);
        app.update();
        assert!(lit(&mut app).is_empty(), "out");
    }

    #[test]
    fn the_light_fades_before_it_goes() {
        let (mut app, _) = delve(true, 10.0);
        cast_at_tier(&mut app, 1);
        // Long since gathered: the test's real clock barely moves.
        for mut light in app.world_mut().query::<&mut DivineLight>().iter_mut(app.world_mut()) {
            light.cast_at -= RISE_SECONDS * 10.0;
        }
        let bauble = lit(&mut app)[0].0;
        app.world_mut().resource_mut::<GameClock>().advance(bauble.expires - bauble.fade * 0.5);
        app.update();
        let now = app.world_mut().query_filtered::<&PointLight, With<DivineLight>>().single(app.world()).unwrap().intensity;
        assert!(now > 0.0 && now < bauble.peak * 0.6, "half dimmed: {now} of {}", bauble.peak);
    }

    #[test]
    fn a_fresh_light_blooms_and_settles() {
        let at = |age| glow(age, 1.0, 0.0);
        assert_eq!(at(0.0).light, 0.0, "nothing at the cast");
        assert!(at(0.0).spread > 3.0, "the motes gather from wide");
        let peak = (1..12).map(|i| at(RISE_SECONDS * i as f32 / 12.0).heart).fold(0.0, f32::max);
        assert!(peak > 1.05, "blooms past its size: {peak}");
        let settled = at(RISE_SECONDS * 2.0);
        assert!((settled.light - 1.0).abs() < 1e-5 && (settled.spread - 1.0).abs() < 1e-5 && settled.swirl.abs() < 1e-5, "{settled:?}");
    }

    #[test]
    fn a_dying_light_sputters_shrinks_and_loses_its_orbits() {
        let risen = RISE_SECONDS * 2.0;
        let lights: Vec<f32> = (0..200).map(|i| glow(risen, 0.15, i as f32 * 0.013).light).collect();
        let (low, high) = lights.iter().fold((f32::MAX, 0.0_f32), |(l, h), &v| (l.min(v), h.max(v)));
        assert!(high <= 0.15 + 1e-5 && low > 0.0 && high - low > 0.05, "sputters under what is left: {low}..{high}");
        assert!(glow(risen, 0.15, 0.0).heart < glow(risen, 1.0, 0.0).heart, "shrinks");
        assert!(glow(risen, 0.15, 0.0).spread > 1.5, "its motes loosen");
        assert_eq!(orbit_left(2, 1.0), 1.0);
        assert_eq!(orbit_left(2, 0.3), 0.0, "the outer orbit has gone");
        assert!(orbit_left(0, 0.3) > 0.0, "the inner one still turns");
    }

    #[test]
    fn the_bauble_stops_short_of_walls_and_shut_doors() {
        let cell = fb_level::CELL;
        // Open cells x 0 and 1 in row 0; a wall (or a shut door) at x 2.
        let solid = |x: i32, y: i32| !(y == 0 && (0..2).contains(&x));
        let eye = Vec3::new(cell * 0.5, 15.0, cell * 0.5);
        let through = Vec3::new(cell * 2.5, 16.0, cell * 0.5);
        let stop = clear_of_walls(eye, through, solid);
        assert!((stop.x - (cell * 2.0 - BAUBLE_CLEARANCE)).abs() <= 0.5, "stops short of the wall: {stop}");
        assert!(stop.y > eye.y && stop.y < through.y, "along the line: {stop}");
        let near = Vec3::new(cell * 2.0 - 0.5, 15.0, cell * 0.5);
        assert!(clear_of_walls(eye, near, solid).x <= cell * 2.0 - BAUBLE_CLEARANCE + 0.5, "pulled off the wall's face");
        let open = Vec3::new(cell * 1.2, 15.0, cell * 0.5);
        assert_eq!(clear_of_walls(eye, open, solid), open, "a clear place is kept");
    }

    #[test]
    fn recasting_replaces_the_light() {
        let (mut app, _) = delve(true, 10.0);
        cast_at_tier(&mut app, 1);
        cast_at_tier(&mut app, 2);
        let lights = lit(&mut app);
        assert_eq!(lights.len(), 1, "never two");
        assert_eq!(lights[0].0.feet, 40.0);
    }

    #[test]
    fn a_cast_light_counts_as_the_carried_light() {
        let (mut app, _) = delve(true, 10.0);
        let character = Character::roll(3);
        let inventory = crate::inventory::Inventory::default();
        assert_eq!(character.derive(&inventory).light, 0.0);
        cast_at_tier(&mut app, 3);
        let feet = spell_light_feet(app.world_mut().query::<&DivineLight>().iter(app.world()));
        assert_eq!(feet, Some(60.0));
        let rules = fb_character::rules::defaults();
        assert_eq!(character.derive_lit(rules, &inventory, feet).light, 60.0);
        assert_eq!(character.gear_with_spell(&inventory, feet).light, 60.0);
    }
}
