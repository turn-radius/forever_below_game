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
                bind(Action::DodgeLeft, "Dodge left", Group::Moving, &[Key(KeyCode::KeyQ)]),
                bind(Action::DodgeRight, "Dodge right", Group::Moving, &[Key(KeyCode::KeyE)]),
                bind(Action::Sprint, "Sprint", Group::Moving, &[Key(KeyCode::ShiftLeft), Key(KeyCode::ShiftRight)]),
                bind(Action::Crouch, "Crouch", Group::Moving, &[Key(KeyCode::ControlLeft), Key(KeyCode::ControlRight)]),
                bind(Action::Jump, "Jump", Group::Moving, &[Key(KeyCode::Space)]),
                bind(Action::Look, "Look round", Group::Moving, &[Input::MouseMotion]),
                bind(Action::TakeMouse, "Take the mouse back", Group::Moving, &[Mouse(MouseButton::Left)]),
                bind(Action::Attack, "Swing what you hold", Group::Fighting, &[Mouse(MouseButton::Left)]),
                bind(Action::Block, "Block", Group::Fighting, &[Mouse(MouseButton::Right)]),
                bind(Action::Stab, "Stab", Group::Fighting, &[Mouse(MouseButton::Left)]),
                bind(Action::Primary, "Primary action", Group::Items, &[Key(KeyCode::KeyF)]),
                bind(Action::Secondary, "Secondary action", Group::Items, &[Key(KeyCode::KeyG)]),
                bind(Action::Tertiary, "Tertiary action", Group::Items, &[Key(KeyCode::KeyR)]),
                bind(Action::Special, "Special action", Group::Items, &[Key(KeyCode::KeyT)]),
                bind(Action::Compare, "Compare", Group::Items, &[Key(KeyCode::AltLeft), Key(KeyCode::AltRight)]),
                bind(Action::Light, "Light or put out your light (Shift: the hip lantern; in the inventory, the one under the mouse)", Group::Items, &[Key(KeyCode::KeyL)]),
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
                bind(Action::TurnItem, "Turn a dragged item", Group::Items, &[Mouse(MouseButton::Right)]),
                bind(Action::Inventory, "Inventory", Group::Screens, &[Key(KeyCode::KeyI)]),
                bind(Action::Character, "Character", Group::Screens, &[Key(KeyCode::KeyK)]),
                bind(Action::Journal, "Journal", Group::Screens, &[Key(KeyCode::KeyJ)]),
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

    /// What `action` is called on screen: its inputs' labels joined with " or " ("Esc", "M or Esc"), empty if unbound.
    pub fn label(&self, action: Action) -> String {
        self.binding(action).map_or(String::new(), |b| b.labels().join(" or "))
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
    fn an_action_is_labelled_by_its_inputs() {
        let keymap = Keymap::default();
        assert_eq!(keymap.label(Action::Quickslot), "1 to 0");
        assert_eq!(keymap.label(Action::TurnItem), "Right click");
        let mut keymap = keymap;
        keymap.bindings.iter_mut().find(|b| b.action == Action::TurnItem).unwrap().inputs.clear();
        assert_eq!(keymap.label(Action::TurnItem), "");
    }

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
        let read = app
            .world_mut()
            .run_system_once(|input: ActionInput| (input.pressed(Action::Sprint), input.just_pressed(Action::TurnItem), input.pressed(Action::Primary), input.axis(Action::WalkForward, Action::WalkBack)))
            .unwrap();
        assert_eq!(read, (true, true, false, 0.0));
    }
}
