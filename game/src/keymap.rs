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
    /// Swings what the main hand holds.
    Attack,
    PickUp,
    /// Equips the item under the crosshair straight from the floor.
    EquipAimed,
    /// Lights or puts out the carried light (with Shift, the hip lantern), or in the inventory
    /// the light under the mouse.
    Light,
    /// Eats or drinks the food or drink under the mouse in the inventory.
    Consume,
    /// Uses the item in a quickslot: its ten keys, 1 to 0, are the slots in order.
    Quickslot,
    DragItem,
    TurnItem,
    /// Held over an item that can be worn, shows what is worn in its place beside its card.
    Compare,
    Inventory,
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

/// One action's name, group and the inputs that do it.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub action: Action,
    pub name: &'static str,
    pub group: Group,
    pub inputs: Vec<Input>,
}

impl Binding {
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
                bind(Action::Attack, "Swing what you hold", Group::Fighting, &[Mouse(MouseButton::Left)]),
                bind(Action::PickUp, "Pick up or open", Group::Items, &[Key(KeyCode::KeyE)]),
                bind(Action::EquipAimed, "Equip what you look at", Group::Items, &[Key(KeyCode::KeyF)]),
                bind(Action::Light, "Light or put out your light (Shift: the hip lantern; in the inventory, the one under the mouse)", Group::Items, &[Key(KeyCode::KeyL)]),
                bind(Action::Consume, "Eat or drink what is under the mouse in the inventory", Group::Items, &[Key(KeyCode::KeyU)]),
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
                ),
                bind(Action::DragItem, "Drag an item", Group::Items, &[Mouse(MouseButton::Left)]),
                bind(Action::TurnItem, "Turn a dragged item", Group::Items, &[Key(KeyCode::KeyR), Mouse(MouseButton::Right)]),
                bind(Action::Compare, "Compare with what you wear (hold over an item)", Group::Items, &[Key(KeyCode::AltLeft), Key(KeyCode::AltRight)]),
                bind(Action::Inventory, "Inventory", Group::Screens, &[Key(KeyCode::KeyI)]),
                bind(Action::Character, "Character", Group::Screens, &[Key(KeyCode::KeyC)]),
                bind(Action::Map, "Map", Group::Screens, &[Key(KeyCode::KeyM)]),
                bind(Action::Back, "Go back, or open the menu", Group::Screens, &[Key(KeyCode::Escape)]),
                bind(Action::Choose, "Choose in a menu", Group::Screens, &[Mouse(MouseButton::Left)]),
                bind(Action::NewDelve, "New delve, on the title", Group::Screens, &[Key(KeyCode::Enter), Key(KeyCode::NumpadEnter)]),
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

    /// Which of `action`'s inputs was pressed this frame, as its position in the binding (the
    /// first if several were): the slot a quickslot key names.
    pub fn just_pressed_index(&self, action: Action) -> Option<usize> {
        self.keymap.inputs(action).iter().position(|input| match *input {
            Input::Key(k) => self.keys.as_deref().is_some_and(|keys| keys.just_pressed(k)),
            Input::Mouse(b) => self.mouse.as_deref().is_some_and(|mouse| mouse.just_pressed(b)),
            Input::MouseMotion => false,
        })
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
    fn the_inventory_is_i_the_character_c_and_nothing_is_tab() {
        let keymap = Keymap::default();
        assert_eq!(keymap.inputs(Action::Inventory), &[Input::Key(KeyCode::KeyI)]);
        assert_eq!(keymap.inputs(Action::Character), &[Input::Key(KeyCode::KeyC)]);
        assert!(keymap.bindings.iter().all(|b| !b.inputs.contains(&Input::Key(KeyCode::Tab))));
    }

    #[test]
    fn keys_have_short_labels() {
        assert_eq!(Input::Key(KeyCode::KeyW).label(), "W");
        assert_eq!(Input::Key(KeyCode::Digit3).label(), "3");
        assert_eq!(Input::Key(KeyCode::Escape).label(), "Esc");
        assert_eq!(Input::Key(KeyCode::Equal).label(), "=");
        assert_eq!(Keymap::default().binding(Action::NewDelve).unwrap().labels(), vec!["Enter".to_string()]);
        assert_eq!(Input::Mouse(MouseButton::Right).label(), "Right click");
        assert_eq!(Keymap::default().binding(Action::Hurry).unwrap().labels(), vec!["Shift".to_string()]);
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
        let read = app
            .world_mut()
            .run_system_once(|input: ActionInput| (input.pressed(Action::Hurry), input.just_pressed(Action::TurnItem), input.pressed(Action::PickUp), input.axis(Action::WalkForward, Action::WalkBack)))
            .unwrap();
        assert_eq!(read, (true, true, false, 0.0));
    }
}
