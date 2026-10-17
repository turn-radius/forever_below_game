//! Every key and mouse button the game reads, in one [`Keymap`]. Systems ask [`ActionInput`]
//! whether an [`Action`] is pressed rather than naming keys, and the pause menu lists the same
//! table, so the controls it shows are the controls there are.

use std::collections::{HashMap, HashSet};

use bevy::{ecs::system::SystemParam, input::InputSystems, prelude::*};

use crate::launch::AppState;
use crate::play::{Screen, ScreenOpen};
use crate::tuning::Tuning;

/// Something the player does with a key or button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    WalkForward,
    WalkBack,
    StepLeft,
    StepRight,
    Sprint,
    DodgeLeft,
    DodgeRight,
    Crouch,
    Jump,
    /// Moving the mouse. Never pressed; listed so the menu shows it.
    Look,
    /// Takes the mouse back for looking round, in the level.
    TakeMouse,
    /// Swings what the main hand holds.
    Attack,
    /// Holds a blocking guard. No gameplay yet.
    Block,
    /// A thrust with what the main hand holds. No gameplay yet.
    Stab,
    /// In the level, picks up the item under the crosshair, opens a container or takes a
    /// stair; in the inventory, eats or drinks the item under the mouse.
    Primary,
    /// Equips the item under the crosshair straight from the floor.
    Secondary,
    /// No gameplay yet.
    Tertiary,
    /// No gameplay yet.
    Special,
    /// Compares the item under the crosshair or mouse with what is worn. No gameplay yet.
    Compare,
    /// Lights or puts out the carried light (with Shift, the hip lantern), or in the inventory
    /// the light under the mouse.
    Light,
    /// Uses the item in a quickslot: its ten keys, 1 to 0, are the slots in order.
    Quickslot,
    DragItem,
    TurnItem,
    Inventory,
    /// Opens the journal. No gameplay yet.
    Journal,
    /// Opens the inventory screen on its character tab.
    Character,
    /// Opens and closes the map of the level.
    Map,
    /// Closes the open screen or page, or opens the menu over the level.
    Back,
    /// Presses a button in a menu: the title's or the pause menu.
    Choose,
    /// Begins a new delve from the title screen.
    NewDelve,
    MusicQuieter,
    MusicLouder,
}

/// Where an action is listed in the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    Moving,
    Fighting,
    Items,
    Screens,
    Sound,
}

impl Group {
    /// Every group, in the order the menu lists them.
    pub const ALL: [Group; 5] = [Group::Moving, Group::Fighting, Group::Items, Group::Screens, Group::Sound];

    pub fn name(self) -> &'static str {
        match self {
            Group::Moving => "Moving",
            Group::Fighting => "Fighting",
            Group::Items => "Items",
            Group::Screens => "Screens",
            Group::Sound => "Sound",
        }
    }
}

/// A key, a mouse button, or moving the mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Input {
    Key(KeyCode),
    Mouse(MouseButton),
    MouseMotion,
}

impl Input {
    /// What the key or button is called on screen: "W", "Shift", "Esc", "Right click".
    pub fn label(self) -> String {
        match self {
            Input::Key(key) => key_label(key),
            Input::Mouse(MouseButton::Left) => "Left click".into(),
            Input::Mouse(MouseButton::Right) => "Right click".into(),
            Input::Mouse(MouseButton::Middle) => "Middle click".into(),
            Input::Mouse(button) => format!("Mouse {button:?}"),
            Input::MouseMotion => "Mouse".into(),
        }
    }
}

fn key_label(key: KeyCode) -> String {
    let named = match key {
        KeyCode::ShiftLeft | KeyCode::ShiftRight => "Shift",
        KeyCode::ControlLeft | KeyCode::ControlRight => "Ctrl",
        KeyCode::AltLeft | KeyCode::AltRight => "Alt",
        KeyCode::Escape => "Esc",
        KeyCode::Enter | KeyCode::NumpadEnter => "Enter",
        KeyCode::Minus => "-",
        KeyCode::Equal => "=",
        KeyCode::ArrowUp => "Up",
        KeyCode::ArrowDown => "Down",
        KeyCode::ArrowLeft => "Left",
        KeyCode::ArrowRight => "Right",
        _ => "",
    };
    if !named.is_empty() {
        return named.into();
    }
    let debug = format!("{key:?}");
    debug.strip_prefix("Key").or_else(|| debug.strip_prefix("Digit")).unwrap_or(&debug).to_string()
}

/// Which part of the game reads a binding: the level, the inventory screen, or the menus (the
/// title, the pause menu and the map). Two actions may share an input only in different
/// contexts, and [`ActionInput`] reads only the open screen's context.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Context {
    #[default]
    Level,
    Inventory,
    Menus,
}

impl Context {
    /// The context of the title (`app`) or the screen open over the level, if any. The map
    /// reads as a menu: nothing in it but the keys that swap screens.
    pub fn of(app: Option<AppState>, open: Option<Screen>) -> Context {
        match (app, open) {
            (Some(AppState::Title), _) | (_, Some(Screen::Menu | Screen::Map)) => Context::Menus,
            (_, Some(Screen::Inventory)) => Context::Inventory,
            (_, None) => Context::Level,
        }
    }
}

/// The context [`ActionInput`] reads, kept by [`track_context`] from the screen open (a frame
/// behind a screen that opens or closes in `Update`). It is a resource of its own so that the
/// systems that open screens can still take [`ScreenOpen`] mutably beside `ActionInput`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActiveContext(pub Context);

/// How a binding fires.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PressType {
    /// Once, on the frame the input goes down.
    #[default]
    Press,
    /// Once the input has been down for the hold time ([`crate::tuning::InputTuning`]), and
    /// the action ends when it is let go. Let go sooner it is a tap ([`ActionInput::tapped`]).
    Hold,
}

/// Whether Crouch is held down or toggled by each press.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CrouchMode {
    #[default]
    Hold,
    Toggle,
}

/// One action's name, group and the inputs that do it.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub action: Action,
    pub name: &'static str,
    pub group: Group,
    pub inputs: Vec<Input>,
    pub press: PressType,
    /// Where it applies. One entry covers an action that does the same kind of thing in more
    /// than one place, as Primary does (takes in the level, uses in the inventory).
    pub contexts: &'static [Context],
}

/// Every context, for the keys that work on any screen.
const EVERYWHERE: &[Context] = &[Context::Level, Context::Inventory, Context::Menus];
/// The item keys, which act on the crosshair's item in the level and the mouse's in the inventory.
const ITEMS: &[Context] = &[Context::Level, Context::Inventory];

impl Binding {
    /// This binding, fired by holding rather than pressing.
    fn hold(mut self) -> Self {
        self.press = PressType::Hold;
        self
    }

    /// This binding, applying in `contexts` rather than the level alone.
    fn in_contexts(mut self, contexts: &'static [Context]) -> Self {
        self.contexts = contexts;
        self
    }

    /// Whether the two bindings apply in some context together.
    fn overlaps(&self, other: &Binding) -> bool {
        self.contexts.iter().any(|c| other.contexts.contains(c))
    }

    /// The inputs' labels, each once ("Shift" for both shift keys). A run of digit keys, as
    /// the quickslots', is one label: "1 to 0".
    pub fn labels(&self) -> Vec<String> {
        let mut labels: Vec<String> = Vec::new();
        for label in self.inputs.iter().map(|i| i.label()) {
            if !labels.contains(&label) {
                labels.push(label);
            }
        }
        let digits = self.inputs.iter().all(|i| matches!(i, Input::Key(k) if format!("{k:?}").starts_with("Digit")));
        if digits && labels.len() >= 4 {
            return vec![format!("{} to {}", labels[0], labels[labels.len() - 1])];
        }
        labels
    }
}

/// Every binding the game reads.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Keymap {
    pub bindings: Vec<Binding>,
    /// Crouch held or toggled. The menu offers the choice later.
    pub crouch: CrouchMode,
}

impl Default for Keymap {
    fn default() -> Self {
        use Input::{Key, Mouse};
        let bind = |action, name, group, inputs: &[Input]| Binding { action, name, group, inputs: inputs.to_vec(), press: PressType::Press, contexts: &[Context::Level] };
        Keymap {
            bindings: vec![
                bind(Action::WalkForward, "Walk forward", Group::Moving, &[Key(KeyCode::KeyW)]),
                bind(Action::WalkBack, "Walk back", Group::Moving, &[Key(KeyCode::KeyS)]),
                bind(Action::StepLeft, "Step left", Group::Moving, &[Key(KeyCode::KeyA)]),
                bind(Action::StepRight, "Step right", Group::Moving, &[Key(KeyCode::KeyD)]),
                bind(Action::DodgeLeft, "Dodge left", Group::Moving, &[Key(KeyCode::KeyQ)]),
                bind(Action::DodgeRight, "Dodge right", Group::Moving, &[Key(KeyCode::KeyE)]),
                bind(Action::Sprint, "Sprint", Group::Moving, &[Key(KeyCode::ShiftLeft), Key(KeyCode::ShiftRight)]),
                bind(Action::Crouch, "Crouch", Group::Moving, &[Key(KeyCode::ControlLeft), Key(KeyCode::ControlRight)]),
                bind(Action::Jump, "Jump", Group::Moving, &[Key(KeyCode::Space)]),
                bind(Action::Look, "Look round", Group::Moving, &[Input::MouseMotion]),
                bind(Action::TakeMouse, "Take the mouse back", Group::Moving, &[Mouse(MouseButton::Left)]),
                bind(Action::Attack, "Swing what you hold", Group::Fighting, &[Mouse(MouseButton::Left)]),
                bind(Action::Block, "Block", Group::Fighting, &[Mouse(MouseButton::Right)]).hold(),
                bind(Action::Stab, "Stab", Group::Fighting, &[Mouse(MouseButton::Left)]).hold(),
                bind(Action::Primary, "Primary action", Group::Items, &[Key(KeyCode::KeyF)]).in_contexts(ITEMS),
                bind(Action::Secondary, "Secondary action", Group::Items, &[Key(KeyCode::KeyG)]).in_contexts(ITEMS),
                bind(Action::Tertiary, "Tertiary action", Group::Items, &[Key(KeyCode::KeyR)]).in_contexts(ITEMS),
                bind(Action::Special, "Special action", Group::Items, &[Key(KeyCode::KeyT)]).in_contexts(ITEMS),
                bind(Action::Compare, "Compare", Group::Items, &[Key(KeyCode::AltLeft), Key(KeyCode::AltRight)]).in_contexts(ITEMS),
                bind(Action::Light, "Light or put out your light (Shift: the hip lantern; in the inventory, the one under the mouse)", Group::Items, &[Key(KeyCode::KeyL)]).in_contexts(ITEMS),
                bind(
                    Action::Quickslot,
                    "Use a quickslot, or in the inventory bind the item under the mouse to it",
                    Group::Items,
                    &[
                        Key(KeyCode::Digit1),
                        Key(KeyCode::Digit2),
                        Key(KeyCode::Digit3),
                        Key(KeyCode::Digit4),
                        Key(KeyCode::Digit5),
                        Key(KeyCode::Digit6),
                        Key(KeyCode::Digit7),
                        Key(KeyCode::Digit8),
                        Key(KeyCode::Digit9),
                        Key(KeyCode::Digit0),
                    ],
                )
                .in_contexts(ITEMS),
                bind(Action::DragItem, "Drag an item", Group::Items, &[Mouse(MouseButton::Left)]).in_contexts(&[Context::Inventory]),
                bind(Action::TurnItem, "Turn a dragged item", Group::Items, &[Mouse(MouseButton::Right)]).in_contexts(&[Context::Inventory]),
                bind(Action::Inventory, "Inventory", Group::Screens, &[Key(KeyCode::KeyI)]).in_contexts(EVERYWHERE),
                bind(Action::Character, "Character", Group::Screens, &[Key(KeyCode::KeyK)]).in_contexts(EVERYWHERE),
                bind(Action::Journal, "Journal", Group::Screens, &[Key(KeyCode::KeyJ)]).in_contexts(EVERYWHERE),
                bind(Action::Map, "Map", Group::Screens, &[Key(KeyCode::KeyM)]).in_contexts(EVERYWHERE),
                bind(Action::Back, "Go back, or open the menu", Group::Screens, &[Key(KeyCode::Escape)]).in_contexts(EVERYWHERE),
                bind(Action::Choose, "Choose in a menu", Group::Screens, &[Mouse(MouseButton::Left)]).in_contexts(&[Context::Menus]),
                bind(Action::NewDelve, "New delve, on the title", Group::Screens, &[Key(KeyCode::Enter), Key(KeyCode::NumpadEnter)]).in_contexts(&[Context::Menus]),
                bind(Action::MusicQuieter, "Music quieter", Group::Sound, &[Key(KeyCode::Minus)]).in_contexts(EVERYWHERE),
                bind(Action::MusicLouder, "Music louder", Group::Sound, &[Key(KeyCode::Equal)]).in_contexts(EVERYWHERE),
            ],
            crouch: CrouchMode::default(),
        }
    }
}

impl Keymap {
    pub fn binding(&self, action: Action) -> Option<&Binding> {
        self.bindings.iter().find(|b| b.action == action)
    }

    /// The inputs bound to `action`; none if it is unbound.
    pub fn inputs(&self, action: Action) -> &[Input] {
        self.binding(action).map_or(&[], |b| b.inputs.as_slice())
    }

    /// Whether some other action in one of `action`'s contexts holds `input`.
    fn is_held_by_another(&self, action: Action, input: Input) -> bool {
        let Some(own) = self.binding(action) else { return false };
        self.bindings.iter().any(|b| b.action != action && b.press == PressType::Hold && b.inputs.contains(&input) && b.overlaps(own))
    }

    /// The action already using `input` in a context `action` shares, if any: what rebinding
    /// `action` to `input` would clash with. Two actions may share an input in different
    /// contexts, so left click is Attack in the level and DragItem in the inventory.
    ///
    /// Two pairs share an input in one context by design and never clash. A Press and a Hold
    /// of different actions on one input are a tap and a hold (Attack and Stab on left click,
    /// [`ActionInput::tapped`]). And TakeMouse only takes the mouse back, so the click that
    /// does it is free to do its own job as well, whatever that is.
    pub fn conflicts(&self, action: Action, input: Input) -> Option<Action> {
        let own = self.binding(action)?;
        self.bindings
            .iter()
            .filter(|b| b.action != action && b.inputs.contains(&input) && b.overlaps(own))
            .filter(|b| b.press == own.press && ![action, b.action].contains(&Action::TakeMouse))
            .map(|b| b.action)
            .next()
    }

    /// The bindings listed under `group`, in order.
    pub fn in_group(&self, group: Group) -> impl Iterator<Item = &Binding> {
        self.bindings.iter().filter(move |b| b.group == group)
    }
}

/// How long each bound input has been down, for the Hold bindings. [`track_holds`] keeps it.
#[derive(Resource, Debug, PartialEq)]
pub struct InputHolds {
    /// Seconds a press needs to become a hold.
    hold_time: f32,
    inputs: HashMap<Input, Held>,
    /// Whether a toggled crouch is on.
    crouch_toggled: bool,
}

impl Default for InputHolds {
    fn default() -> Self {
        InputHolds { hold_time: crate::tuning::InputTuning::default().hold_time, inputs: HashMap::new(), crouch_toggled: false }
    }
}

/// One input's press this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Held {
    down: bool,
    /// Seconds down as of last frame and as of this one.
    before: f32,
    now: f32,
    /// Seconds it had been down, on the frame it was let go.
    released: Option<f32>,
}

impl InputHolds {
    /// Down long enough to count as held, and still down.
    fn is_held(&self, input: Input) -> bool {
        self.inputs.get(&input).is_some_and(|h| h.down && h.now >= self.hold_time)
    }

    /// Reached the hold time this frame.
    fn became_held(&self, input: Input) -> bool {
        self.inputs.get(&input).is_some_and(|h| h.before < self.hold_time && h.now >= self.hold_time)
    }

    /// Let go this frame after being held.
    fn ended_hold(&self, input: Input) -> bool {
        self.inputs.get(&input).and_then(|h| h.released).is_some_and(|t| t >= self.hold_time)
    }

    /// Let go this frame before the hold time.
    fn was_tapped(&self, input: Input) -> bool {
        self.inputs.get(&input).and_then(|h| h.released).is_some_and(|t| t < self.hold_time)
    }
}

/// Where [`track_holds`] runs, in `PreUpdate` after the input is read. Anything that drives
/// `ButtonInput` itself runs before it.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct HoldSet;

/// Keeps [`ActiveContext`] to the screen open, or the title. It runs in `PreUpdate`, so a
/// screen opened or closed this frame changes the context only next frame. That lag is what
/// keeps the click that closes a menu (Resume) from arming a swing in `combat::swing`.
pub(crate) fn track_context(screen: Option<Res<ScreenOpen>>, app: Option<Res<State<AppState>>>, mut active: ResMut<ActiveContext>) {
    active.set_if_neq(ActiveContext(Context::of(app.map(|s| *s.get()), screen.and_then(|s| s.0))));
}

/// Counts how long every bound input has been down, and flips a toggled crouch.
pub(crate) fn track_holds(
    keymap: Res<Keymap>,
    tuning: Option<Res<Tuning>>,
    time: Option<Res<Time<Real>>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    screen: Option<Res<crate::play::ScreenOpen>>,
    mut holds: ResMut<InputHolds>,
    mut bound: Local<Vec<Input>>,
) {
    let delta = time.map_or(0.0, |t| t.delta_secs());
    holds.hold_time = tuning.map_or(crate::tuning::InputTuning::default().hold_time, |t| t.input.hold_time).max(0.01);
    // Pressed, just pressed, just released.
    let read = |input: Input| -> (bool, bool, bool) {
        match input {
            Input::Key(k) => keys.as_deref().map_or((false, false, false), |b| (b.pressed(k), b.just_pressed(k), b.just_released(k))),
            Input::Mouse(m) => mouse.as_deref().map_or((false, false, false), |b| (b.pressed(m), b.just_pressed(m), b.just_released(m))),
            Input::MouseMotion => (false, false, false),
        }
    };
    // Several actions share an input, which must be counted once a frame.
    if keymap.is_changed() || bound.is_empty() {
        *bound = keymap.bindings.iter().flat_map(|b| b.inputs.iter().copied()).collect::<HashSet<_>>().into_iter().collect();
    }
    for &input in bound.iter() {
        let (pressed, just_pressed, just_released) = read(input);
        let held = holds.inputs.entry(input).or_default();
        if just_pressed {
            *held = Held { down: !just_released, released: just_released.then_some(0.0), ..Held::default() };
        } else if held.down {
            held.before = held.now;
            held.now += delta;
            held.released = (just_released || !pressed).then_some(held.now);
            held.down = held.released.is_none();
        } else {
            *held = Held::default();
        }
    }
    // A press in a menu or screen is not a crouch.
    let flipped = !screen.is_some_and(|s| s.0.is_some()) && keymap.inputs(Action::Crouch).iter().any(|&i| read(i).1);
    holds.crouch_toggled = keymap.crouch == CrouchMode::Toggle && (holds.crouch_toggled != flipped);
}

/// The keyboard and mouse read through the [`Keymap`]. Without either input resource (in
/// tests) nothing is pressed, and without [`InputHolds`] no Hold binding fires.
///
/// A Press binding reads the raw input. A Hold binding is `pressed` once its input has been
/// down for the hold time, `just_pressed` on the frame it gets there and `just_released` when
/// let go after that. Let go sooner, it is [`tapped`](ActionInput::tapped).
///
/// An action reads only while its binding's [`Context`] is the open screen's
/// ([`ActionInput::context`]), so left click is Attack in the level and DragItem in the inventory.
#[derive(SystemParam)]
pub struct ActionInput<'w> {
    keymap: Res<'w, Keymap>,
    keys: Option<Res<'w, ButtonInput<KeyCode>>>,
    mouse: Option<Res<'w, ButtonInput<MouseButton>>>,
    holds: Option<Res<'w, InputHolds>>,
    active: Option<Res<'w, ActiveContext>>,
}

impl ActionInput<'_> {
    /// The context of the open screen: the title and the menus, the inventory, or the level.
    /// Without the resource (in tests) it is the level.
    pub fn context(&self) -> Context {
        self.active.as_deref().map_or(Context::Level, |a| a.0)
    }

    /// Whether `action` applies in the open screen's context. Everything but `crouching` reads
    /// false for an action that does not.
    pub fn is_active(&self, action: Action) -> bool {
        self.keymap.binding(action).is_some_and(|b| b.contexts.contains(&self.context()))
    }

    fn press_type(&self, action: Action) -> PressType {
        self.keymap.binding(action).map_or(PressType::Press, |b| b.press)
    }

    fn any(&self, action: Action, key: impl Fn(&ButtonInput<KeyCode>, KeyCode) -> bool, button: impl Fn(&ButtonInput<MouseButton>, MouseButton) -> bool) -> bool {
        self.keymap.inputs(action).iter().any(|input| match *input {
            Input::Key(k) => self.keys.as_deref().is_some_and(|keys| key(keys, k)),
            Input::Mouse(b) => self.mouse.as_deref().is_some_and(|mouse| button(mouse, b)),
            Input::MouseMotion => false,
        })
    }

    /// Whether `held` is true of any of `action`'s inputs.
    fn any_held(&self, action: Action, held: impl Fn(&InputHolds, Input) -> bool) -> bool {
        self.holds.as_deref().is_some_and(|holds| self.keymap.inputs(action).iter().any(|&i| held(holds, i)))
    }

    /// Down now; for a Hold binding, down for the hold time.
    pub fn pressed(&self, action: Action) -> bool {
        if !self.is_active(action) {
            return false;
        }
        match self.press_type(action) {
            PressType::Press => self.any(action, |k, c| k.pressed(c), |m, b| m.pressed(b)),
            PressType::Hold => self.any_held(action, InputHolds::is_held),
        }
    }

    /// Which of `action`'s inputs was pressed this frame, as its position in the binding (the
    /// first if several were): the slot a quickslot key names.
    pub fn just_pressed_index(&self, action: Action) -> Option<usize> {
        if !self.is_active(action) {
            return None;
        }
        self.keymap.inputs(action).iter().position(|input| match *input {
            Input::Key(k) => self.keys.as_deref().is_some_and(|keys| keys.just_pressed(k)),
            Input::Mouse(b) => self.mouse.as_deref().is_some_and(|mouse| mouse.just_pressed(b)),
            Input::MouseMotion => false,
        })
    }

    /// Pressed this frame; for a Hold binding, held for the hold time this frame.
    pub fn just_pressed(&self, action: Action) -> bool {
        if !self.is_active(action) {
            return false;
        }
        match self.press_type(action) {
            PressType::Press => self.any(action, |k, c| k.just_pressed(c), |m, b| m.just_pressed(b)),
            PressType::Hold => self.any_held(action, InputHolds::became_held),
        }
    }

    /// Let go this frame; for a Hold binding, let go after being held (a tap does not count).
    pub fn just_released(&self, action: Action) -> bool {
        if !self.is_active(action) {
            return false;
        }
        match self.press_type(action) {
            PressType::Press => self.any(action, |k, c| k.just_released(c), |m, b| m.just_released(b)),
            PressType::Hold => self.any_held(action, InputHolds::ended_hold),
        }
    }

    /// A short press: let go before the hold time. A Press binding on an input some other
    /// action holds (Attack, with Stab held on the same click) fires here, on release, so a
    /// click is Attack or Stab and never both; the swing waits for the click to be let go.
    /// A Press binding on an input nothing holds fires as it goes down, as `just_pressed`.
    pub fn tapped(&self, action: Action) -> bool {
        if !self.is_active(action) {
            return false;
        }
        let contested = self.keymap.inputs(action).iter().any(|&i| self.keymap.is_held_by_another(action, i));
        // Without the tracker (in tests) there is no hold to wait out.
        if self.press_type(action) == PressType::Press && (!contested || self.holds.is_none()) {
            return self.just_pressed(action);
        }
        self.any_held(action, InputHolds::was_tapped)
    }

    /// Whether the delver is crouching, by the keymap's [`CrouchMode`]: Crouch held down, or
    /// the state each press of it flips.
    pub fn crouching(&self) -> bool {
        match self.keymap.crouch {
            CrouchMode::Hold => self.any(Action::Crouch, |k, c| k.pressed(c), |m, b| m.pressed(b)),
            CrouchMode::Toggle => self.holds.as_deref().is_some_and(|h| h.crouch_toggled),
        }
    }

    /// 1 while `plus` is held, -1 while `minus` is, 0 for both or neither.
    pub fn axis(&self, plus: Action, minus: Action) -> f32 {
        self.pressed(plus) as i32 as f32 - self.pressed(minus) as i32 as f32
    }
}

/// Reads how long the bound inputs are down, for the Hold bindings.
pub struct KeymapPlugin;

impl Plugin for KeymapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Keymap>().init_resource::<InputHolds>().init_resource::<ActiveContext>().add_systems(
            PreUpdate,
            (track_context, track_holds.in_set(HoldSet)).after(InputSystems),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn every_action_is_bound_once_and_named() {
        let keymap = Keymap::default();
        for (i, binding) in keymap.bindings.iter().enumerate() {
            assert!(!binding.name.is_empty());
            assert!(!binding.inputs.is_empty(), "{:?}", binding.action);
            assert!(keymap.bindings[i + 1..].iter().all(|b| b.action != binding.action), "{:?} twice", binding.action);
        }
        assert!(Group::ALL.iter().all(|&g| keymap.in_group(g).next().is_some()));
    }

    #[test]
    fn the_inventory_is_i_and_the_character_k() {
        let keymap = Keymap::default();
        assert_eq!(keymap.inputs(Action::Inventory), &[Input::Key(KeyCode::KeyI)]);
        assert_eq!(keymap.inputs(Action::Character), &[Input::Key(KeyCode::KeyK)]);
    }

    #[test]
    fn the_spare_keys_are_unbound() {
        let keymap = Keymap::default();
        for key in [KeyCode::KeyC, KeyCode::KeyV, KeyCode::KeyX, KeyCode::KeyZ, KeyCode::KeyB, KeyCode::KeyN, KeyCode::KeyO, KeyCode::KeyP, KeyCode::KeyU, KeyCode::Tab] {
            assert!(keymap.bindings.iter().all(|b| !b.inputs.contains(&Input::Key(key))), "{key:?} is bound");
        }
    }

    #[test]
    fn the_new_layout_binds_the_action_keys() {
        let keymap = Keymap::default();
        assert_eq!(keymap.inputs(Action::Primary), &[Input::Key(KeyCode::KeyF)]);
        assert_eq!(keymap.inputs(Action::Secondary), &[Input::Key(KeyCode::KeyG)]);
        assert_eq!(keymap.inputs(Action::Tertiary), &[Input::Key(KeyCode::KeyR)]);
        assert_eq!(keymap.inputs(Action::Special), &[Input::Key(KeyCode::KeyT)]);
        assert_eq!(keymap.inputs(Action::Compare), &[Input::Key(KeyCode::AltLeft), Input::Key(KeyCode::AltRight)]);
        assert_eq!(keymap.inputs(Action::DodgeLeft), &[Input::Key(KeyCode::KeyQ)]);
        assert_eq!(keymap.inputs(Action::DodgeRight), &[Input::Key(KeyCode::KeyE)]);
        assert_eq!(keymap.inputs(Action::Jump), &[Input::Key(KeyCode::Space)]);
        assert_eq!(keymap.inputs(Action::Journal), &[Input::Key(KeyCode::KeyJ)]);
        assert_eq!(keymap.inputs(Action::TurnItem), &[Input::Mouse(MouseButton::Right)]);
        assert_eq!(keymap.inputs(Action::Block), &[Input::Mouse(MouseButton::Right)]);
        assert_eq!(keymap.binding(Action::Sprint).unwrap().name, "Sprint");
    }

    #[test]
    fn keys_have_short_labels() {
        assert_eq!(Input::Key(KeyCode::KeyW).label(), "W");
        assert_eq!(Input::Key(KeyCode::Digit3).label(), "3");
        assert_eq!(Input::Key(KeyCode::Escape).label(), "Esc");
        assert_eq!(Input::Key(KeyCode::Equal).label(), "=");
        assert_eq!(Keymap::default().binding(Action::NewDelve).unwrap().labels(), vec!["Enter".to_string()]);
        assert_eq!(Input::Mouse(MouseButton::Right).label(), "Right click");
        assert_eq!(Keymap::default().binding(Action::Sprint).unwrap().labels(), vec!["Shift".to_string()]);
        assert_eq!(Keymap::default().binding(Action::Quickslot).unwrap().labels(), vec!["1 to 0".to_string()], "ten digit keys are one label");
    }

    #[test]
    fn the_quickslot_key_pressed_is_the_slot_it_names() {
        let mut app = App::new();
        app.init_resource::<Keymap>().init_resource::<ButtonInput<KeyCode>>();
        let slot = |app: &mut App| {
            app.world_mut().run_system_once(|input: ActionInput| input.just_pressed_index(Action::Quickslot)).unwrap()
        };
        assert_eq!(slot(&mut app), None);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Digit3);
        assert_eq!(slot(&mut app), Some(2));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Digit0);
        assert_eq!(slot(&mut app), Some(9));
    }

    #[test]
    fn actions_read_their_keys_and_buttons() {
        let mut app = App::new();
        app.init_resource::<Keymap>().init_resource::<ButtonInput<KeyCode>>().init_resource::<ButtonInput<MouseButton>>();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ShiftRight);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Right);
        app.insert_resource(ActiveContext(Context::Inventory));
        let read = app
            .world_mut()
            .run_system_once(|input: ActionInput| (input.pressed(Action::Sprint), input.just_pressed(Action::TurnItem), input.pressed(Action::Primary), input.axis(Action::WalkForward, Action::WalkBack)))
            .unwrap();
        assert_eq!(read, (false, true, false, 0.0), "Sprint is the level's, so the inventory does not read it");
    }

    /// An app with the keymap, both inputs, a manual clock and the hold tracker.
    fn held_app() -> App {
        let mut app = App::new();
        app.init_resource::<Keymap>().init_resource::<InputHolds>().init_resource::<ButtonInput<KeyCode>>().init_resource::<ButtonInput<MouseButton>>().init_resource::<Time<Real>>();
        app.add_systems(Update, track_holds);
        app
    }

    /// Advances the clock by `seconds`, then runs the tracker.
    fn step(app: &mut App, seconds: f32) {
        app.world_mut().resource_mut::<Time<Real>>().advance_by(std::time::Duration::from_secs_f32(seconds));
        app.update();
    }

    /// Ends a frame as Bevy's input system does at the start of the next: the transient flags clear.
    fn end(app: &mut App) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
    }

    /// Pressed, just pressed, just released and tapped, for `action`.
    fn read(app: &mut App, action: Action) -> (bool, bool, bool, bool) {
        app.world_mut().run_system_once(move |input: ActionInput| (input.pressed(action), input.just_pressed(action), input.just_released(action), input.tapped(action))).unwrap()
    }

    #[test]
    fn a_press_binding_fires_once_on_the_down_frame() {
        let mut app = held_app();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyF);
        step(&mut app, 0.016);
        assert!(read(&mut app, Action::Primary).1);
        end(&mut app);
        step(&mut app, 0.016);
        let (pressed, just, _, _) = read(&mut app, Action::Primary);
        assert!(pressed && !just);
    }

    #[test]
    fn a_hold_binding_fires_once_after_the_hold_time_and_not_before() {
        let mut app = held_app();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        step(&mut app, 0.016);
        assert_eq!(read(&mut app, Action::Stab), (false, false, false, false));
        end(&mut app);
        step(&mut app, 0.2);
        assert_eq!(read(&mut app, Action::Stab), (false, false, false, false), "0.216s is short of 0.25s");
        end(&mut app);
        step(&mut app, 0.05);
        assert_eq!(read(&mut app, Action::Stab), (true, true, false, false), "fires on the frame it gets there");
        end(&mut app);
        step(&mut app, 0.1);
        assert_eq!(read(&mut app, Action::Stab), (true, false, false, false), "and only once");
    }

    #[test]
    fn letting_go_ends_a_hold() {
        let mut app = held_app();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Right);
        step(&mut app, 0.016);
        end(&mut app);
        step(&mut app, 0.3);
        assert!(read(&mut app, Action::Block).0);
        end(&mut app);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Right);
        step(&mut app, 0.016);
        assert_eq!(read(&mut app, Action::Block), (false, false, true, false));
        end(&mut app);
        step(&mut app, 0.016);
        assert_eq!(read(&mut app, Action::Block), (false, false, false, false));
    }

    #[test]
    fn a_short_click_is_attack_and_not_stab() {
        let mut app = held_app();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        step(&mut app, 0.016);
        assert!(!read(&mut app, Action::Attack).3, "the click is not decided on the way down");
        end(&mut app);
        step(&mut app, 0.1);
        assert!(!read(&mut app, Action::Attack).3, "nor while it is still down and short");
        end(&mut app);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Left);
        step(&mut app, 0.016);
        assert!(read(&mut app, Action::Attack).3);
        assert_eq!(read(&mut app, Action::Stab), (false, false, false, true));
        end(&mut app);
        step(&mut app, 0.016);
        assert!(!read(&mut app, Action::Attack).3, "once");
    }

    #[test]
    fn a_long_click_is_stab_and_not_attack() {
        let mut app = held_app();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        step(&mut app, 0.016);
        end(&mut app);
        step(&mut app, 0.3);
        assert!(read(&mut app, Action::Stab).1);
        assert!(!read(&mut app, Action::Attack).3);
        end(&mut app);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Left);
        step(&mut app, 0.016);
        assert!(read(&mut app, Action::Stab).2);
        assert!(!read(&mut app, Action::Attack).3, "letting go after the hold is not a tap");
    }

    #[test]
    fn right_click_still_turns_a_dragged_item_at_once() {
        let mut app = held_app();
        app.insert_resource(ActiveContext(Context::Inventory));
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Right);
        step(&mut app, 0.016);
        assert!(read(&mut app, Action::TurnItem).1);
    }

    #[test]
    fn crouch_is_held_by_default_and_can_toggle() {
        let mut app = held_app();
        assert_eq!(app.world().resource::<Keymap>().crouch, CrouchMode::Hold);
        let crouching = |app: &mut App| app.world_mut().run_system_once(|input: ActionInput| input.crouching()).unwrap();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ControlLeft);
        step(&mut app, 0.016);
        assert!(crouching(&mut app));
        end(&mut app);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::ControlLeft);
        step(&mut app, 0.016);
        assert!(!crouching(&mut app), "held: let go stands up");

        app.world_mut().resource_mut::<Keymap>().crouch = CrouchMode::Toggle;
        end(&mut app);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ControlLeft);
        step(&mut app, 0.016);
        assert!(crouching(&mut app));
        end(&mut app);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::ControlLeft);
        step(&mut app, 0.016);
        assert!(crouching(&mut app), "toggled: let go stays down");
        end(&mut app);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ControlLeft);
        step(&mut app, 0.016);
        assert!(!crouching(&mut app), "the next press stands up");
    }

    #[test]
    fn the_defaults_have_no_conflicts() {
        let keymap = Keymap::default();
        for binding in &keymap.bindings {
            for &input in &binding.inputs {
                assert_eq!(keymap.conflicts(binding.action, input), None, "{:?} on {input:?}", binding.action);
            }
        }
    }

    #[test]
    fn left_click_is_attack_in_the_level_and_drag_in_the_inventory() {
        let keymap = Keymap::default();
        let click = Input::Mouse(MouseButton::Left);
        assert_eq!(keymap.conflicts(Action::Attack, click), None);
        assert_eq!(keymap.conflicts(Action::DragItem, click), None);
        // Binding drag to a level key clashes with the item keys, which reach into the inventory.
        assert_eq!(keymap.conflicts(Action::DragItem, Input::Key(KeyCode::KeyF)), Some(Action::Primary));
    }

    #[test]
    fn binding_jump_to_f_in_the_level_conflicts_with_primary() {
        let keymap = Keymap::default();
        assert_eq!(keymap.conflicts(Action::Jump, Input::Key(KeyCode::KeyF)), Some(Action::Primary));
        assert_eq!(keymap.conflicts(Action::Jump, Input::Key(KeyCode::KeyC)), None);
        assert_eq!(keymap.conflicts(Action::Jump, Input::Key(KeyCode::Space)), None, "its own key is no clash");
    }

    #[test]
    fn a_press_and_a_hold_may_share_an_input() {
        let keymap = Keymap::default();
        assert_eq!(keymap.conflicts(Action::Stab, Input::Mouse(MouseButton::Left)), None);
        // Block holds right click and nothing in the level presses it, but a press would clash.
        assert_eq!(keymap.conflicts(Action::Sprint, Input::Mouse(MouseButton::Right)), None);
        assert_eq!(keymap.conflicts(Action::Jump, Input::Mouse(MouseButton::Left)), Some(Action::Attack));
    }

    /// A single frame of `action` in `screen`, with `keys` and the left button down.
    fn read_in(screen: Option<Screen>, keys: &[KeyCode], actions: &[Action]) -> Vec<bool> {
        let mut app = App::new();
        app.init_resource::<Keymap>().init_resource::<ButtonInput<KeyCode>>().init_resource::<ButtonInput<MouseButton>>().insert_resource(ActiveContext(Context::of(None, screen)));
        for &key in keys {
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        }
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        let actions = actions.to_vec();
        app.world_mut().run_system_once(move |input: ActionInput| actions.iter().map(|&a| input.just_pressed(a)).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn the_inventory_reads_its_own_context() {
        let all = [Action::Primary, Action::DragItem, Action::Attack, Action::Jump];
        // Inventory open: F is still Primary, and left click is a drag, not a swing.
        assert_eq!(read_in(Some(Screen::Inventory), &[KeyCode::KeyF, KeyCode::Space], &all), [true, true, false, false]);
        // In the level: left click swings and does not drag; F is Primary.
        assert_eq!(read_in(None, &[KeyCode::KeyF, KeyCode::Space], &all), [true, false, true, true]);
    }

    #[test]
    fn menus_read_only_the_menu_keys() {
        let all = [Action::Choose, Action::Attack, Action::DragItem, Action::Back];
        assert_eq!(read_in(Some(Screen::Menu), &[KeyCode::Escape], &all), [true, false, false, true]);
        assert_eq!(read_in(None, &[KeyCode::Escape], &all), [false, true, false, true]);
    }
}
