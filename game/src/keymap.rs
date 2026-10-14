//! Every key and mouse button the game reads, in one [`Keymap`]. Systems ask [`ActionInput`]
//! whether an [`Action`] is pressed rather than naming keys, and the pause menu lists the same
//! table, so the controls it shows are the controls there are.

use bevy::{ecs::system::SystemParam, prelude::*};

/// Something the player does with a key or button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    WalkForward,
    WalkBack,
    StepLeft,
    StepRight,
    Hurry,
    /// Moving the mouse. Never pressed; listed so the menu shows it.
    Look,
    /// Takes the mouse back for looking round, in the level.
    TakeMouse,
    PickUp,
    DragItem,
    TurnItem,
    Inventory,
    /// Closes the open screen, or opens the menu over the level.
    Back,
    /// Presses a button in the menu.
    Choose,
    MusicQuieter,
    MusicLouder,
}

/// Where an action is listed in the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    Moving,
    Items,
    Screens,
    Sound,
}

impl Group {
    /// Every group, in the order the menu lists them.
    pub const ALL: [Group; 4] = [Group::Moving, Group::Items, Group::Screens, Group::Sound];

    pub fn name(self) -> &'static str {
        match self {
            Group::Moving => "Moving",
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

/// One action's name, group and the inputs that do it.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub action: Action,
    pub name: &'static str,
    pub group: Group,
    pub inputs: Vec<Input>,
}

impl Binding {
    /// The inputs' labels, each once ("Shift" for both shift keys).
    pub fn labels(&self) -> Vec<String> {
        let mut labels: Vec<String> = Vec::new();
        for label in self.inputs.iter().map(|i| i.label()) {
            if !labels.contains(&label) {
                labels.push(label);
            }
        }
        labels
    }
}

/// Every binding the game reads.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Keymap {
    pub bindings: Vec<Binding>,
}

impl Default for Keymap {
    fn default() -> Self {
        use Input::{Key, Mouse};
        let bind = |action, name, group, inputs: &[Input]| Binding { action, name, group, inputs: inputs.to_vec() };
        Keymap {
            bindings: vec![
                bind(Action::WalkForward, "Walk forward", Group::Moving, &[Key(KeyCode::KeyW)]),
                bind(Action::WalkBack, "Walk back", Group::Moving, &[Key(KeyCode::KeyS)]),
                bind(Action::StepLeft, "Step left", Group::Moving, &[Key(KeyCode::KeyA)]),
                bind(Action::StepRight, "Step right", Group::Moving, &[Key(KeyCode::KeyD)]),
                bind(Action::Hurry, "Hurry", Group::Moving, &[Key(KeyCode::ShiftLeft), Key(KeyCode::ShiftRight)]),
                bind(Action::Look, "Look round", Group::Moving, &[Input::MouseMotion]),
                bind(Action::TakeMouse, "Take the mouse back", Group::Moving, &[Mouse(MouseButton::Left)]),
                bind(Action::PickUp, "Pick up or open", Group::Items, &[Key(KeyCode::KeyE)]),
                bind(Action::DragItem, "Drag an item", Group::Items, &[Mouse(MouseButton::Left)]),
                bind(Action::TurnItem, "Turn a dragged item", Group::Items, &[Key(KeyCode::KeyR), Mouse(MouseButton::Right)]),
                bind(Action::Inventory, "Inventory", Group::Screens, &[Key(KeyCode::KeyI)]),
                bind(Action::Back, "Close a screen, or open the menu", Group::Screens, &[Key(KeyCode::Escape)]),
                bind(Action::Choose, "Choose in the menu", Group::Screens, &[Mouse(MouseButton::Left)]),
                bind(Action::MusicQuieter, "Music quieter", Group::Sound, &[Key(KeyCode::Minus)]),
                bind(Action::MusicLouder, "Music louder", Group::Sound, &[Key(KeyCode::Equal)]),
            ],
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

    /// The bindings listed under `group`, in order.
    pub fn in_group(&self, group: Group) -> impl Iterator<Item = &Binding> {
        self.bindings.iter().filter(move |b| b.group == group)
    }
}

/// The keyboard and mouse read through the [`Keymap`]. Without either input resource (in
/// tests) nothing is pressed.
#[derive(SystemParam)]
pub struct ActionInput<'w> {
    keymap: Res<'w, Keymap>,
    keys: Option<Res<'w, ButtonInput<KeyCode>>>,
    mouse: Option<Res<'w, ButtonInput<MouseButton>>>,
}

impl ActionInput<'_> {
    fn any(&self, action: Action, key: impl Fn(&ButtonInput<KeyCode>, KeyCode) -> bool, button: impl Fn(&ButtonInput<MouseButton>, MouseButton) -> bool) -> bool {
        self.keymap.inputs(action).iter().any(|input| match *input {
            Input::Key(k) => self.keys.as_deref().is_some_and(|keys| key(keys, k)),
            Input::Mouse(b) => self.mouse.as_deref().is_some_and(|mouse| button(mouse, b)),
            Input::MouseMotion => false,
        })
    }

    /// Held down now.
    pub fn pressed(&self, action: Action) -> bool {
        self.any(action, |k, c| k.pressed(c), |m, b| m.pressed(b))
    }

    /// Pressed this frame.
    pub fn just_pressed(&self, action: Action) -> bool {
        self.any(action, |k, c| k.just_pressed(c), |m, b| m.just_pressed(b))
    }

    /// Let go this frame.
    pub fn just_released(&self, action: Action) -> bool {
        self.any(action, |k, c| k.just_released(c), |m, b| m.just_released(b))
    }

    /// 1 while `plus` is held, -1 while `minus` is, 0 for both or neither.
    pub fn axis(&self, plus: Action, minus: Action) -> f32 {
        self.pressed(plus) as i32 as f32 - self.pressed(minus) as i32 as f32
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
    fn the_inventory_is_i_and_nothing_is_tab() {
        let keymap = Keymap::default();
        assert_eq!(keymap.inputs(Action::Inventory), &[Input::Key(KeyCode::KeyI)]);
        assert!(keymap.bindings.iter().all(|b| !b.inputs.contains(&Input::Key(KeyCode::Tab))));
    }

    #[test]
    fn keys_have_short_labels() {
        assert_eq!(Input::Key(KeyCode::KeyW).label(), "W");
        assert_eq!(Input::Key(KeyCode::Digit3).label(), "3");
        assert_eq!(Input::Key(KeyCode::Escape).label(), "Esc");
        assert_eq!(Input::Key(KeyCode::Equal).label(), "=");
        assert_eq!(Input::Mouse(MouseButton::Right).label(), "Right click");
        assert_eq!(Keymap::default().binding(Action::Hurry).unwrap().labels(), vec!["Shift".to_string()]);
    }

    #[test]
    fn actions_read_their_keys_and_buttons() {
        let mut app = App::new();
        app.init_resource::<Keymap>().init_resource::<ButtonInput<KeyCode>>().init_resource::<ButtonInput<MouseButton>>();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ShiftRight);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Right);
        let read = app
            .world_mut()
            .run_system_once(|input: ActionInput| (input.pressed(Action::Hurry), input.just_pressed(Action::TurnItem), input.pressed(Action::PickUp), input.axis(Action::WalkForward, Action::WalkBack)))
            .unwrap();
        assert_eq!(read, (true, true, false, 0.0));
    }
}
