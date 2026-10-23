//! The player's keybindings, kept in `keybindings.toml` beside the settings file.
//!
//! The file holds only what differs from [`Keymap::default`], so a rebind is a line and a
//! default that changes in a later version reaches everyone who never touched it:
//!
//! ```toml
//! [level]
//! primary = ["KeyE"]
//! sprint = ["ShiftLeft", "ShiftRight"]
//!
//! [options]
//! crouch = "toggle"
//! ```
//!
//! A section is the binding's first context in lower case (`level`, `inventory`, `menus`) and a
//! key is [`Action::id`], the action in snake case. Keys are Bevy's `KeyCode` names. Mouse
//! buttons are `MouseLeft`, `MouseRight`, `MouseMiddle`, `MouseBack`, `MouseForward` and
//! `Mouse<n>` for any other, and moving the mouse is `MouseMotion`. An empty list unbinds.
//! Only the inputs and the crouch option are saved: how a binding fires (press or hold) and
//! where it applies (its contexts) are fixed by the game and never persisted.
//!
//! [`KeybindingsPlugin`] reads the file at startup; the options menu calls [`Keymap::save`]
//! after a rebind. `FB_KEYBINDINGS=path` puts the file elsewhere. A missing file gives the
//! defaults, an unreadable one gives the defaults with a warning, and an unknown action,
//! section or key is skipped with a warning while the rest still loads.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::keymap::{Action, Context, CrouchMode, Input, Keymap};

impl Action {
    /// Every action, in the order the keymap lists them.
    pub const ALL: [Action; 35] = [
        Action::WalkForward,
        Action::WalkBack,
        Action::StepLeft,
        Action::StepRight,
        Action::Sprint,
        Action::DodgeLeft,
        Action::DodgeRight,
        Action::Crouch,
        Action::Jump,
        Action::Look,
        Action::TakeMouse,
        Action::Attack,
        Action::Block,
        Action::Stab,
        Action::Primary,
        Action::Secondary,
        Action::Tertiary,
        Action::Special,
        Action::Compare,
        Action::Light,
        Action::Quickslot,
        Action::DrinkHealing,
        Action::DragItem,
        Action::TurnItem,
        Action::Drop,
        Action::Inventory,
        Action::Journal,
        Action::Character,
        Action::NextTab,
        Action::Map,
        Action::Back,
        Action::Choose,
        Action::NewDelve,
        Action::MusicQuieter,
        Action::MusicLouder,
    ];

    /// The name the keybindings file knows it by. Fixed once shipped, whatever the variant
    /// is later called.
    pub fn id(self) -> &'static str {
        match self {
            Action::WalkForward => "walk_forward",
            Action::WalkBack => "walk_back",
            Action::StepLeft => "step_left",
            Action::StepRight => "step_right",
            Action::Sprint => "sprint",
            Action::DodgeLeft => "dodge_left",
            Action::DodgeRight => "dodge_right",
            Action::Crouch => "crouch",
            Action::Jump => "jump",
            Action::Look => "look",
            Action::TakeMouse => "take_mouse",
            Action::Attack => "attack",
            Action::Block => "block",
            Action::Stab => "stab",
            Action::Primary => "primary",
            Action::Secondary => "secondary",
            Action::Tertiary => "tertiary",
            Action::Special => "special",
            Action::Compare => "compare",
            Action::Light => "light",
            Action::Quickslot => "quickslot",
            Action::DrinkHealing => "drink_healing",
            Action::DragItem => "drag_item",
            Action::TurnItem => "turn_item",
            Action::Drop => "drop",
            Action::Inventory => "inventory",
            Action::Journal => "journal",
            Action::Character => "character",
            Action::NextTab => "next_tab",
            Action::Map => "map",
            Action::Back => "back",
            Action::Choose => "choose",
            Action::NewDelve => "new_delve",
            Action::MusicQuieter => "music_quieter",
            Action::MusicLouder => "music_louder",
        }
    }

    /// The action with this [`id`](Action::id), if there is one.
    pub fn from_id(id: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|a| a.id() == id)
    }
}

impl Context {
    /// The section name in the keybindings file.
    fn section(self) -> &'static str {
        match self {
            Context::Level => "level",
            Context::Inventory => "inventory",
            Context::Menus => "menus",
        }
    }
}

impl Input {
    /// The input as the keybindings file spells it; `None` for a key `KeyCode` can't name.
    fn to_id(self) -> Option<String> {
        match self {
            Input::Key(key) => match toml::Value::try_from(key) {
                Ok(toml::Value::String(name)) => Some(name),
                _ => None,
            },
            Input::Mouse(MouseButton::Left) => Some("MouseLeft".into()),
            Input::Mouse(MouseButton::Right) => Some("MouseRight".into()),
            Input::Mouse(MouseButton::Middle) => Some("MouseMiddle".into()),
            Input::Mouse(MouseButton::Back) => Some("MouseBack".into()),
            Input::Mouse(MouseButton::Forward) => Some("MouseForward".into()),
            Input::Mouse(MouseButton::Other(n)) => Some(format!("Mouse{n}")),
            Input::MouseMotion => Some("MouseMotion".into()),
        }
    }

    /// The input a keybindings file name stands for.
    fn from_id(id: &str) -> Option<Input> {
        match id {
            "MouseLeft" => Some(Input::Mouse(MouseButton::Left)),
            "MouseRight" => Some(Input::Mouse(MouseButton::Right)),
            "MouseMiddle" => Some(Input::Mouse(MouseButton::Middle)),
            "MouseBack" => Some(Input::Mouse(MouseButton::Back)),
            "MouseForward" => Some(Input::Mouse(MouseButton::Forward)),
            "MouseMotion" => Some(Input::MouseMotion),
            _ => match id.strip_prefix("Mouse").map(str::parse::<u16>) {
                Some(Ok(n)) => Some(Input::Mouse(MouseButton::Other(n))),
                Some(Err(_)) => None,
                None => toml::Value::String(id.into()).try_into::<KeyCode>().ok().map(Input::Key),
            },
        }
    }
}

impl Keymap {
    /// The keymap `text` describes: the defaults with its bindings and options laid over them.
    /// The warnings say what was skipped, one for each; an unparseable file is one warning and
    /// the defaults.
    pub fn from_toml(text: &str) -> (Keymap, Vec<String>) {
        let mut keymap = Keymap::default();
        let mut warnings = Vec::new();
        let mut seen: Vec<Action> = Vec::new();
        let table = match text.parse::<toml::Table>() {
            Ok(table) => table,
            Err(error) => {
                warnings.push(format!("unreadable ({}); using the defaults", error.message()));
                return (keymap, warnings);
            }
        };
        for (section, entries) in &table {
            let Some(entries) = entries.as_table() else {
                warnings.push(format!("{section} is not a section; skipped"));
                continue;
            };
            if section == "options" {
                for (name, value) in entries {
                    match (name.as_str(), value.as_str()) {
                        ("crouch", Some("hold")) => keymap.crouch = CrouchMode::Hold,
                        ("crouch", Some("toggle")) => keymap.crouch = CrouchMode::Toggle,
                        ("crouch", _) => warnings.push(format!("options: crouch must be \"hold\" or \"toggle\", not {value}; skipped")),
                        _ => warnings.push(format!("options: unknown option {name}; skipped")),
                    }
                }
            } else if [Context::Level, Context::Inventory, Context::Menus].iter().any(|c| c.section() == section) {
                for (name, value) in entries {
                    let Some(action) = Action::from_id(name) else {
                        warnings.push(format!("{section}: unknown action {name}; skipped"));
                        continue;
                    };
                    let Some(names) = value.as_array() else {
                        warnings.push(format!("{section}: {name} must be a list of keys; skipped"));
                        continue;
                    };
                    let own = keymap.binding(action).and_then(|b| b.contexts.first()).map(|c| c.section());
                    if own.is_some_and(|own| own != section.as_str()) {
                        warnings.push(format!("{section}: {name} belongs in [{}]; loaded anyway", own.unwrap_or_default()));
                    }
                    if seen.contains(&action) {
                        warnings.push(format!("{section}: {name} is listed in more than one section; the later one wins"));
                    }
                    seen.push(action);
                    let mut inputs = Vec::new();
                    for item in names {
                        match item.as_str().and_then(Input::from_id) {
                            Some(input) if !inputs.contains(&input) => inputs.push(input),
                            Some(_) => {}
                            None => warnings.push(format!("{section}: {name}: unknown key {item}; skipped")),
                        }
                    }
                    // A list whose every entry was skipped keeps the default; only `[]` unbinds.
                    if inputs.is_empty() && !names.is_empty() {
                        continue;
                    }
                    if let Some(binding) = keymap.bindings.iter_mut().find(|b| b.action == action) {
                        binding.inputs = inputs;
                    }
                }
            } else {
                warnings.push(format!("unknown section {section}; skipped"));
            }
        }
        (keymap, warnings)
    }

    /// The file's text: only the bindings and options that differ from the defaults.
    pub fn to_toml(&self) -> String {
        let defaults = Keymap::default();
        let mut table = toml::Table::new();
        for binding in &self.bindings {
            if defaults.binding(binding.action).is_some_and(|d| d.inputs == binding.inputs) {
                continue;
            }
            let names: Vec<_> = binding.inputs.iter().filter_map(|i| i.to_id()).map(toml::Value::String).collect();
            // Writing fewer names than there are inputs could write `[]`, which means unbound.
            if names.len() != binding.inputs.len() {
                warn!("keybindings: {:?} has an input that can't be named; not saved", binding.action);
                continue;
            }
            let section = binding.contexts.first().copied().unwrap_or_default().section();
            let toml::Value::Table(section) = table.entry(section).or_insert_with(|| toml::Value::Table(toml::Table::new())) else { unreachable!() };
            section.insert(binding.action.id().into(), toml::Value::Array(names));
        }
        if self.crouch != defaults.crouch {
            let crouch = match self.crouch {
                CrouchMode::Hold => "hold",
                CrouchMode::Toggle => "toggle",
            };
            table.insert("options".into(), toml::Value::Table(toml::Table::from_iter([("crouch".to_string(), toml::Value::String(crouch.into()))])));
        }
        // Sections in the order the file documents; the table sorts them alphabetically.
        let mut text = String::new();
        for name in ["level", "inventory", "menus", "options"] {
            if let Some(toml::Value::Table(section)) = table.get(name) {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&format!("[{name}]\n"));
                for (key, value) in section {
                    text.push_str(&format!("{key} = {value}\n"));
                }
            }
        }
        text
    }

    /// The keymap saved at `path`, or the defaults if there is none or it can't be read. One
    /// warning is logged for each thing skipped.
    pub fn load(path: &Path) -> Keymap {
        let Ok(text) = std::fs::read_to_string(path) else { return Keymap::default() };
        let (keymap, warnings) = Keymap::from_toml(&text);
        for warning in warnings {
            warn!("keybindings: {}: {warning}", path.display());
        }
        keymap
    }

    /// Writes the differences from the defaults to `path`, making its folder if need be.
    /// Written beside it and renamed into place, so a crash mid-write never leaves half a file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let partial = path.with_extension("toml.partial");
        std::fs::write(&partial, self.to_toml())?;
        std::fs::rename(&partial, path)
    }
}

/// Where the [`Keymap`] is saved; `None` keeps it for this run only.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct KeybindingsFile(pub Option<PathBuf>);

impl KeybindingsFile {
    /// `FB_KEYBINDINGS` if set, else `keybindings.toml` in the settings file's folder.
    pub fn default_path() -> Option<PathBuf> {
        Self::resolve(std::env::var_os("FB_KEYBINDINGS"), crate::settings::SettingsFile::default_path())
    }

    /// The path for an `FB_KEYBINDINGS` value and the settings file's path.
    fn resolve(env: Option<OsString>, settings: Option<PathBuf>) -> Option<PathBuf> {
        env.filter(|p| !p.is_empty()).map(PathBuf::from).or_else(|| settings.and_then(|s| s.parent().map(|folder| folder.join("keybindings.toml"))))
    }
}

/// Loads the [`Keymap`] from a [`KeybindingsFile`]. Add it before the plugins that need the
/// keymap, so they keep what it loaded. Without a `KeybindingsFile` resource already in the
/// app it uses [`KeybindingsFile::default_path`]. Nothing is saved on its own: whatever
/// rebinds a key calls [`Keymap::save`] on the file's path.
pub struct KeybindingsPlugin;

impl Plugin for KeybindingsPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<KeybindingsFile>() {
            app.insert_resource(KeybindingsFile(KeybindingsFile::default_path()));
        }
        let keymap = app.world().resource::<KeybindingsFile>().0.as_deref().map(Keymap::load).unwrap_or_default();
        app.insert_resource(keymap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A keybindings path of its own in the temp folder, cleared first.
    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("forever-below-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder.join("keybindings.toml")
    }

    fn rebind(keymap: &mut Keymap, action: Action, inputs: &[Input]) {
        keymap.bindings.iter_mut().find(|b| b.action == action).unwrap().inputs = inputs.to_vec();
    }

    #[test]
    fn action_ids_are_unique_and_cover_every_action() {
        let keymap = Keymap::default();
        assert!(keymap.bindings.iter().all(|b| Action::from_id(b.action.id()) == Some(b.action)));
        for action in Action::ALL {
            assert_eq!(Action::from_id(action.id()), Some(action));
            assert!(keymap.binding(action).is_some(), "{action:?} is in the default keymap");
        }
        let mut ids: Vec<_> = Action::ALL.iter().map(|a| a.id()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), keymap.bindings.len());
    }

    #[test]
    fn every_default_input_survives_its_name() {
        for binding in &Keymap::default().bindings {
            for &input in &binding.inputs {
                let id = input.to_id().unwrap_or_else(|| panic!("{input:?} has a name"));
                assert_eq!(Input::from_id(&id), Some(input), "{id}");
            }
        }
    }

    #[test]
    fn names_are_pinned() {
        for (input, name) in [
            (Input::Key(KeyCode::KeyE), "KeyE"),
            (Input::Key(KeyCode::Digit1), "Digit1"),
            (Input::Key(KeyCode::ArrowUp), "ArrowUp"),
            (Input::Key(KeyCode::Escape), "Escape"),
            (Input::Key(KeyCode::Space), "Space"),
            (Input::Mouse(MouseButton::Left), "MouseLeft"),
        ] {
            assert_eq!(input.to_id().as_deref(), Some(name));
            assert_eq!(Input::from_id(name), Some(input));
        }
    }

    #[test]
    fn a_list_of_only_bad_keys_keeps_the_default_and_empty_unbinds() {
        let (keymap, warnings) = Keymap::from_toml("[level]\nsprint = [\"KeyNope\"]\njump = []\n");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(keymap.inputs(Action::Sprint), Keymap::default().inputs(Action::Sprint));
        assert!(keymap.inputs(Action::Jump).is_empty());
    }

    #[test]
    fn an_action_in_the_wrong_or_two_sections_warns_but_loads() {
        let (keymap, warnings) = Keymap::from_toml("[menus]\njump = [\"KeyX\"]\n[level]\njump = [\"KeyY\"]\n");
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        // Sections load in alphabetical order, so [menus] comes last and wins.
        assert_eq!(keymap.inputs(Action::Jump), &[Input::Key(KeyCode::KeyX)]);
    }

    #[test]
    fn the_defaults_write_nothing() {
        assert_eq!(Keymap::default().to_toml(), "");
        let (keymap, warnings) = Keymap::from_toml("");
        assert_eq!((keymap, warnings), (Keymap::default(), vec![]));
    }

    #[test]
    fn only_the_differences_are_written() {
        let mut keymap = Keymap::default();
        rebind(&mut keymap, Action::Primary, &[Input::Key(KeyCode::KeyE)]);
        assert_eq!(keymap.to_toml(), "[level]\nprimary = [\"KeyE\"]\n");
    }

    #[test]
    fn a_rebound_keymap_round_trips() {
        let mut keymap = Keymap::default();
        rebind(&mut keymap, Action::Primary, &[Input::Key(KeyCode::KeyE)]);
        rebind(&mut keymap, Action::Sprint, &[Input::Key(KeyCode::ShiftLeft), Input::Mouse(MouseButton::Back), Input::Mouse(MouseButton::Other(7))]);
        rebind(&mut keymap, Action::Block, &[Input::Mouse(MouseButton::Middle)]);
        rebind(&mut keymap, Action::DragItem, &[Input::Mouse(MouseButton::Right)]);
        rebind(&mut keymap, Action::Jump, &[]);
        keymap.crouch = CrouchMode::Toggle;
        let text = keymap.to_toml();
        assert!(text.contains("[inventory]\ndrag_item = [\"MouseRight\"]"), "{text}");
        assert!(text.contains("sprint = [\"ShiftLeft\", \"MouseBack\", \"Mouse7\"]"), "{text}");
        assert!(text.contains("jump = []"), "{text}");
        let (loaded, warnings) = Keymap::from_toml(&text);
        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(loaded, keymap);
    }

    #[test]
    fn crouch_toggle_is_an_option() {
        let keymap = Keymap { crouch: CrouchMode::Toggle, ..Keymap::default() };
        assert_eq!(keymap.to_toml(), "[options]\ncrouch = \"toggle\"\n");
        assert_eq!(Keymap::from_toml("[options]\ncrouch = \"toggle\"").0.crouch, CrouchMode::Toggle);
        assert_eq!(Keymap::from_toml("[options]\ncrouch = \"hold\"").0.crouch, CrouchMode::Hold);
        let (loaded, warnings) = Keymap::from_toml("[options]\ncrouch = \"sneak\"");
        assert_eq!((loaded.crouch, warnings.len()), (CrouchMode::Hold, 1));
    }

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let path = scratch("missing");
        assert_eq!(Keymap::load(&path), Keymap::default());
    }

    #[test]
    fn an_unreadable_file_gives_the_defaults_with_a_warning() {
        let (keymap, warnings) = Keymap::from_toml("this is [not toml");
        assert_eq!(keymap, Keymap::default());
        assert_eq!(warnings.len(), 1, "{warnings:?}");
    }

    #[test]
    fn an_unknown_action_is_skipped_and_the_rest_loads() {
        let (keymap, warnings) = Keymap::from_toml("[level]\nfly = [\"KeyQ\"]\nprimary = [\"KeyE\"]\n");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("fly"), "{warnings:?}");
        assert_eq!(keymap.inputs(Action::Primary), &[Input::Key(KeyCode::KeyE)]);
    }

    #[test]
    fn an_unknown_key_is_skipped_and_the_rest_loads() {
        let (keymap, warnings) = Keymap::from_toml("[level]\nsprint = [\"ShiftLeft\", \"KeyNope\", \"MouseNope\"]\njump = [\"KeyX\"]\n");
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert_eq!(keymap.inputs(Action::Sprint), &[Input::Key(KeyCode::ShiftLeft)]);
        assert_eq!(keymap.inputs(Action::Jump), &[Input::Key(KeyCode::KeyX)]);
    }

    #[test]
    fn saving_and_loading_goes_through_the_file() {
        let path = scratch("save");
        let mut keymap = Keymap::default();
        rebind(&mut keymap, Action::Primary, &[Input::Key(KeyCode::KeyE)]);
        keymap.save(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[level]\nprimary = [\"KeyE\"]\n");
        assert_eq!(Keymap::load(&path), keymap);
    }

    #[test]
    fn the_plugin_loads_the_keymap() {
        let path = scratch("plugin");
        let mut keymap = Keymap::default();
        rebind(&mut keymap, Action::Jump, &[Input::Key(KeyCode::KeyX)]);
        keymap.save(&path).unwrap();
        let mut app = App::new();
        app.insert_resource(KeybindingsFile(Some(path))).add_plugins(KeybindingsPlugin).add_plugins(crate::keymap::KeymapPlugin);
        assert_eq!(*app.world().resource::<Keymap>(), keymap, "KeymapPlugin keeps what was loaded");
    }

    #[test]
    fn the_path_is_the_override_or_beside_the_settings() {
        let settings = Some(PathBuf::from("/config/Forever Below/settings.json"));
        assert_eq!(KeybindingsFile::resolve(Some("/tmp/keys.toml".into()), settings.clone()), Some(PathBuf::from("/tmp/keys.toml")));
        assert_eq!(KeybindingsFile::resolve(None, settings.clone()), Some(PathBuf::from("/config/Forever Below/keybindings.toml")));
        assert_eq!(KeybindingsFile::resolve(Some("".into()), settings), Some(PathBuf::from("/config/Forever Below/keybindings.toml")));
        assert_eq!(KeybindingsFile::resolve(None, None), None);
    }
}
