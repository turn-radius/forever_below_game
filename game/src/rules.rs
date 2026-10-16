//! The character rules in play: `data/rules.toml` ([`fb_character::rules::Rules`]) as a Bevy
//! asset, reloaded when it is saved, and kept in the [`GameRules`] resource every system that
//! runs a rule reads (the needs and exhaustion, stamina and mana, load, visibility, combat and
//! the creatures' stat blocks). The lab reads the same table.
//!
//! The file lives in the repository's `data/` folder, with the other game tables, not in
//! `game/assets`. [`use_data_folder`] makes that folder an asset source named `data` (before
//! `DefaultPlugins`, as Bevy asks), so the asset server loads `data://rules.toml` and, in a
//! debug build, watches it like `assets/tuning.toml` ([`crate::tuning`]): saving the file changes
//! [`GameRules`] within the frame, and the systems that put a rule on something spawned (the
//! meters' rates) put it there again. The alternative, a copy under `game/assets`, would be two
//! files to keep in step; a symlink does not reliably wake the watcher.
//!
//! A key the file leaves out keeps its default ([`fb_character::rules::Rules`] is
//! `#[serde(default)]` throughout). A file that is not valid TOML, or gives a key the wrong kind
//! of value, is passed over with one warning and the rules stay as they were; the game never
//! stops on it. Without the file, or without an asset server (in tests), the rules are
//! [`Rules::default`], which `data/rules.toml` holds exactly.

use bevy::{
    asset::{
        AssetLoader, LoadContext,
        io::{AssetSourceBuilder, Reader},
    },
    prelude::*,
};
use fb_character::rules::{Rules, defaults};

/// The asset source the rules file is read from: the repository's `data` folder.
pub const DATA_SOURCE: &str = "data";
/// The rules file within [`DATA_SOURCE`].
pub const RULES_FILE: &str = "rules.toml";
/// Where the asset server finds the rules file.
pub const RULES_PATH: &str = "data://rules.toml";

/// The rules every system reads. Insert nothing and it holds [`Rules::default`].
#[derive(Asset, Resource, TypePath, Deref, DerefMut, Clone, Debug, Default, PartialEq)]
pub struct GameRules(pub Rules);

impl GameRules {
    /// The rules in `resource`, or the defaults where the app has none (a test).
    pub fn of<'a>(resource: &'a Option<Res<GameRules>>) -> &'a Rules {
        match resource {
            Some(rules) => &rules.0,
            None => defaults(),
        }
    }
}

/// The `data` folder, beside the asset root: `BEVY_ASSET_ROOT` (made absolute, as
/// [`crate::tuning::asset_file_path`] does) or the game crate's folder, then `../data`.
pub fn data_folder() -> String {
    let root = match std::env::var_os("BEVY_ASSET_ROOT").map(std::path::PathBuf::from) {
        Some(root) => std::path::absolute(&root).unwrap_or(root),
        None => std::env::var_os("CARGO_MANIFEST_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| std::env::current_exe().ok().and_then(|exe| exe.parent().map(ToOwned::to_owned)).unwrap_or_default()),
    };
    root.join("../data").to_string_lossy().into_owned()
}

/// Registers the `data` asset source. Call it before `DefaultPlugins` are added.
pub fn use_data_folder(app: &mut App) {
    use bevy::asset::AssetApp;
    app.register_asset_source(DATA_SOURCE, AssetSourceBuilder::platform_default(&data_folder(), None));
}

/// Reads `rules.toml` into [`GameRules`].
#[derive(Default, TypePath)]
pub struct RulesLoader;

impl AssetLoader for RulesLoader {
    type Asset = GameRules;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(&self, reader: &mut dyn Reader, _settings: &(), _load_context: &mut LoadContext<'_>) -> Result<GameRules, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        // A file that is not valid rules is an error the asset server logs, and the rules stay as they were.
        toml::from_str(&String::from_utf8_lossy(&bytes)).map(GameRules).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    fn extensions(&self) -> &[&str] {
        &["toml"]
    }
}

/// The loaded rules file, kept so it stays loaded and is reloaded when it changes.
#[derive(Resource, Debug)]
pub struct RulesFile(pub Handle<GameRules>);

/// Where [`GameRules`] takes the file's values, in `PreUpdate`: systems that copy it elsewhere
/// before `Update` run after it.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct RulesSet;

/// Loads [`RULES_PATH`] and keeps [`GameRules`] to what it says. Without an asset server, or
/// without the `data` source ([`use_data_folder`]), the resource keeps the defaults.
pub struct RulesPlugin;

impl Plugin for RulesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameRules>();
        let has_source = app.world().get_resource::<AssetServer>().is_some_and(|server| server.get_source(DATA_SOURCE).is_ok());
        if has_source {
            app.init_asset::<GameRules>()
                .init_asset_loader::<RulesLoader>()
                .add_systems(Startup, load_rules.run_if(not(resource_exists::<RulesFile>)))
                .add_systems(PreUpdate, apply_rules_file.in_set(RulesSet));
        }
    }
}

fn load_rules(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(RulesFile(assets.load(RULES_PATH)));
}

/// Copies the file into [`GameRules`] when it loads and each time it is saved.
fn apply_rules_file(mut events: MessageReader<AssetEvent<GameRules>>, file: Option<Res<RulesFile>>, files: Res<Assets<GameRules>>, mut rules: ResMut<GameRules>) {
    let Some(file) = file else { return };
    for event in events.read() {
        let (AssetEvent::Added { id } | AssetEvent::Modified { id }) = *event else { continue };
        if id != file.0.id() {
            continue;
        }
        if let Some(loaded) = files.get(id) {
            if *rules != *loaded {
                info!("rules: applied {RULES_FILE}");
            }
            rules.set_if_neq(loaded.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::asset::AssetPlugin;

    use super::*;

    #[test]
    fn the_shipped_file_is_the_defaults() {
        let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("data").join(RULES_FILE)).unwrap();
        assert_eq!(GameRules(toml::from_str(&text).unwrap()), GameRules::default());
    }

    #[test]
    fn without_the_data_source_the_rules_are_the_defaults() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default())).add_plugins(RulesPlugin);
        app.update();
        assert_eq!(*app.world().resource::<GameRules>(), GameRules::default());
        assert!(app.world().get_resource::<RulesFile>().is_none());
    }

    #[test]
    fn a_missing_resource_reads_as_the_defaults() {
        let mut app = App::new();
        app.add_systems(Update, |rules: Option<Res<GameRules>>| assert_eq!(*GameRules::of(&rules), Rules::default()));
        app.update();
    }

    #[test]
    fn a_bad_file_is_an_error_and_a_partial_one_is_not() {
        assert!(toml::from_str::<Rules>("[needs]\nhunger_hours_to_full = \"soon\"").is_err());
        assert!(toml::from_str::<Rules>("[needs\n").is_err());
        let rules: Rules = toml::from_str("[needs]\nhunger_hours_to_full = 8").unwrap();
        assert_eq!(rules.needs.hunger_hours_to_full, 8.0);
        assert_eq!(rules.combat, Rules::default().combat);
    }

    #[test]
    fn saving_the_file_changes_the_resource() {
        let mut app = App::new();
        // As `use_data_folder` leaves the asset server: a `data` source is there to load from.
        use_data_folder(&mut app);
        app.add_plugins((MinimalPlugins, AssetPlugin::default())).add_plugins(RulesPlugin);
        let mut faster = Rules::default();
        faster.needs.hunger_hours_to_full = 8.0;
        let handle = app.world_mut().resource_mut::<Assets<GameRules>>().add(GameRules::default());
        app.insert_resource(RulesFile(handle.clone()));
        app.update();
        assert_eq!(*app.world().resource::<GameRules>(), GameRules::default());
        // As a reload does: the asset's value is replaced, and an event says so.
        let _ = app.world_mut().resource_mut::<Assets<GameRules>>().insert(&handle, GameRules(faster.clone()));
        app.update();
        app.update();
        assert_eq!(app.world().resource::<GameRules>().needs.hunger_hours_to_full, 8.0);
    }

    #[test]
    fn the_data_folder_is_beside_the_game() {
        let folder = std::path::PathBuf::from(data_folder());
        assert!(folder.join(RULES_FILE).exists(), "{folder:?}");
    }
}
