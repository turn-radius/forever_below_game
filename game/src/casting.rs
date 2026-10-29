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

use armory::build::light::{LightAnchor, glowing};
use armory::build::parts::Parts;
use armory::geom::{ItemMesh, Shading, icosphere, torus};
use armory::material::Rgb;
use bevy::app::Propagate;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use fb_character::Casting;
use fb_character::casting::{Cast, CastError, MAX_TIER, Spellbook, cast_with, points};

use crate::carry::holy_light;
use crate::character::{Character, DelverMana};
use crate::game_clock::GameClock;
use crate::render::{ItemAppearance, ItemSpawner, ToonMaterial};
use crate::sparkle::Sparkles;
use crate::tuning::Tuning;
use crate::combat::orbs::{OrbCluster, cluster_layout};
use crate::inventory::{Hovered, Toast};
use crate::keymap::{Action, ActionInput, Keymap};
use crate::launch::AppState;
use crate::play::{DelveSystems, EYE_HEIGHT, Player, PlayerCamera, inventory_open, level_has_focus};
use crate::quickslots::{Quickslots, SLOTS};
use crate::rules::GameRules;
use crate::ui::{BRIGHT, UiFonts, hud};

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
                    dress_baubles.run_if(resource_exists::<Assets<ToonMaterial>>),
                    follow_the_caster,
                    fade_the_light,
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
/// The pearl at the bauble's heart, nearly white.
const HOLY_CORE: Rgb = Rgb::hex(0xffeeb0);
/// How high above the caster's eye the bauble floats, world units.
const BAUBLE_RISE: f32 = 0.9;
/// How far ahead of the caster it floats, world units.
const BAUBLE_AHEAD: f32 = 14.0;
/// How far to the right of where they look, world units, so it does not sit on the aim.
const BAUBLE_ASIDE: f32 = 3.5;
/// The bauble's size, a share of its drawn mesh.
const BAUBLE_SIZE: f32 = 0.55;
/// How quickly it catches up with the caster, per second.
const BAUBLE_FOLLOW: f32 = 5.0;
/// Game seconds a bauble takes to fade out at the end of its life, at most.
const FADE_SECONDS: f64 = 20.0;
/// Flicker of the holy light: nearly steady.
const HOLY_FLICKER: f32 = 0.08;

/// The bauble a cast of Divine Light leaves floating by the caster: a real light in the level's
/// lighting ([`crate::carry::GivenLight`]) of `feet` reach, until `expires` (game seconds),
/// fading at the end.
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

/// The reach in feet of the light cast on the delver, if one burns: what
/// [`crate::character::Character::gear_with_spell`] counts as a carried light.
pub fn spell_light_feet<'a>(lights: impl IntoIterator<Item = &'a DivineLight>) -> Option<f64> {
    lights.into_iter().map(|l| l.feet as f64).reduce(f64::max)
}

/// Does what each cast says. This is the one place a prayer's effect is read, from
/// `cast.written.effect`; a new effect is another arm.
fn cast_effects(
    mut casts: MessageReader<Casted>,
    mut commands: Commands,
    clock: Option<Res<GameClock>>,
    tuning: Option<Res<Tuning>>,
    players: Query<&GlobalTransform, With<Player>>,
    cameras: Query<&GlobalTransform, With<PlayerCamera>>,
    old: Query<Entity, With<DivineLight>>,
) {
    for Casted(cast) in casts.read() {
        if cast.written.effect == "light" {
            let now = clock.as_deref().map_or(0.0, GameClock::game_seconds);
            let tuning = tuning.as_deref().cloned().unwrap_or_default();
            // Recasting replaces the light: the old one goes out, never two.
            for light in &old {
                commands.entity(light).despawn();
            }
            let at = players.iter().next().map_or(Vec3::Y * (EYE_HEIGHT + BAUBLE_RISE), |p| bauble_goal(p, cameras.iter().next()));
            set_divine_light(&mut commands, cast, now, at, &tuning);
        }
    }
}

/// The bauble lit for `cast` at `at`.
fn set_divine_light(commands: &mut Commands, cast: &fb_character::casting::Cast, now: f64, at: Vec3, tuning: &Tuning) {
    let anchor = LightAnchor::feet([0.0; 3], HOLY_LIGHT, cast.range as f32, HOLY_FLICKER);
    let lit = holy_light(anchor, bevy::camera::Exposure::INDOOR, &tuning.light);
    let peak = lit.0.intensity;
    commands.spawn((
        Name::new(format!("Divine Light, tier {}", cast.tier)),
        DivineLight { feet: cast.range as f32, expires: now + cast.duration, fade: FADE_SECONDS.min(cast.duration * 0.25), peak },
        lit,
        Sparkles { color: Color::srgb(1.0, 0.92, 0.6), count: 5, min: Vec3::splat(-0.6), max: Vec3::splat(0.6) },
        Transform::from_translation(at),
        Visibility::default(),
    ));
}

/// Where the bauble floats for a caster at `player`: above the eye, a little ahead and aside.
fn bauble_goal(player: &GlobalTransform, camera: Option<&GlobalTransform>) -> Vec3 {
    let (forward, right) = match camera {
        Some(c) => {
            let f = Vec3::new(c.forward().x, 0.0, c.forward().z).normalize_or(Vec3::NEG_Z);
            (f, Vec3::new(-f.z, 0.0, f.x))
        }
        None => (Vec3::NEG_Z, Vec3::X),
    };
    player.translation() + Vec3::Y * (EYE_HEIGHT + BAUBLE_RISE) + forward * BAUBLE_AHEAD + right * BAUBLE_ASIDE
}

/// The bauble drifts after its caster, easing toward its place and bobbing a little.
fn follow_the_caster(
    time: Res<Time>,
    players: Query<&GlobalTransform, With<Player>>,
    cameras: Query<&GlobalTransform, With<PlayerCamera>>,
    mut baubles: Query<&mut Transform, (With<DivineLight>, Without<Player>)>,
) {
    let Some(player) = players.iter().next() else { return };
    let goal = bauble_goal(player, cameras.iter().next()) + Vec3::Y * (time.elapsed_secs() * 2.0).sin() * 0.25;
    let ease = 1.0 - (-BAUBLE_FOLLOW * time.delta_secs()).exp();
    for mut at in &mut baubles {
        at.translation = at.translation.lerp(goal, ease);
    }
}

/// Dims each bauble toward its end and puts it out when its time is up.
fn fade_the_light(mut commands: Commands, clock: Option<Res<GameClock>>, mut baubles: Query<(Entity, &DivineLight, &mut PointLight, &mut Transform)>) {
    let Some(clock) = clock else { return };
    let now = clock.game_seconds();
    for (entity, bauble, mut light, mut at) in &mut baubles {
        let k = bauble.brightness(now);
        if k <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let intensity = bauble.peak * k;
        if light.intensity != intensity {
            light.intensity = intensity;
        }
        at.scale = Vec3::splat(0.4 + 0.6 * k);
    }
}

/// The bauble's shape: a glowing pearl in two crossed haloes, all unoutlined glow.
pub fn bauble_mesh() -> ItemMesh {
    let mut parts = Parts::new();
    parts.add_look("pearl", icosphere(0.45, 3), glowing(HOLY_CORE, 1.0));
    for (i, tilt) in [0.5_f32, 1.9].into_iter().enumerate() {
        let mut halo = torus(0.75, 0.035, 40, 6, Shading::SMOOTH);
        halo.rotate_x(tilt);
        halo.rotate_y(i as f32 * 1.1);
        parts.add_look(if i == 0 { "halo" } else { "halo_cross" }, halo, glowing(HOLY_LIGHT, 1.0));
    }
    parts.finish()
}

/// Gives each new bauble its glowing shape, with no shadow of its own to black out its light.
fn dress_baubles(new: Query<Entity, Added<DivineLight>>, mut spawner: ItemSpawner) {
    for bauble in &new {
        spawner.spawn(&bauble_mesh(), &ItemAppearance::default()).insert((Propagate(NotShadowCaster), Transform::from_scale(Vec3::splat(BAUBLE_SIZE)), ChildOf(bauble)));
    }
}

/// `FB_PRAY=1` casts once, a moment into the delve; `FB_PRAY_TIER=2` chooses the tier first and
/// `FB_PRAY_DRAIN=1` empties the pool to show a failed cast. For captures.
fn debug_cast(mut frames: Local<u32>, mut chosen: ResMut<Chosen>, mut requests: MessageWriter<CastRequest>, mut players: Query<&mut DelverMana, With<Player>>) {
    *frames += 1;
    if *frames == 60 {
        if let Some(tier) = std::env::var("FB_PRAY_TIER").ok().and_then(|v| v.trim().parse::<u8>().ok()) {
            chosen.tier = tier.clamp(1, MAX_TIER);
        }
        if std::env::var("FB_PRAY_DRAIN").is_ok_and(|v| !v.is_empty())
            && let Some(mut mana) = players.iter_mut().next()
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
            cluster.spawn((
                ChosenLine,
                Text::new(""),
                UiFonts::style(&fonts.body, 16.0, BRIGHT),
                Node { position_type: PositionType::Absolute, left: hud(layout.bar.min.x), top: hud(layout.bar.min.y - layout.slot * 0.95), ..default() },
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

/// Each slot holding a prayer names it and the tier it would cast at.
fn show_slot_labels(slots: Res<AbilitySlots>, chosen: Res<Chosen>, mut labels: Query<(&AbilityLabel, &mut Text)>) {
    for (label, mut text) in &mut labels {
        let want = slots.get(label.0).map_or_else(String::new, |name| format!("{}
{}", name.split(' ').next_back().unwrap_or(name), chosen.tier));
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
        let (light, given) = app.world_mut().query_filtered::<(&PointLight, &crate::carry::GivenLight), With<DivineLight>>().single(app.world()).map(|(l, g)| (l.clone(), *g)).unwrap();
        assert!(light.intensity > 0.0 && light.range > given.anchor.radius);
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
        let bauble = lit(&mut app)[0].0;
        app.world_mut().resource_mut::<GameClock>().advance(bauble.expires - bauble.fade * 0.5);
        app.update();
        let now = app.world_mut().query_filtered::<&PointLight, With<DivineLight>>().single(app.world()).unwrap().intensity;
        assert!(now > 0.0 && now < bauble.peak * 0.6, "half dimmed: {now} of {}", bauble.peak);
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
