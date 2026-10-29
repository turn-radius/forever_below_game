//! The last delver the player went down as, kept in `last_character.json` beside the settings
//! file ([`SettingsFile::default_path`]) so the title can offer to continue as them after a
//! death or a restart: Continue as their name begins a fresh level, from the clock, with the
//! same folk, class, name, scores and body, in the starting kit.
//!
//! [`LastDelver`] is read when [`LastDelverPlugin`] is added and written whenever the player
//! begins a delve from the creator or continues. A delve begun by `FB_NEW_DELVE` or `--seed`
//! is not remembered, so captures never overwrite it. A missing or unreadable file means no
//! last delver. What the delver learns from books is theirs for good, so it is written as soon
//! as they learn it ([`remember_lessons`]), and Continue brings it back.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use fb_body::Folk;
use fb_character::{AbilityBonus, Attributes};
use serde::{Deserialize, Serialize};

use crate::character::{Character, Class};
use crate::settings::SettingsFile;

/// The file's name, beside `settings.json`.
pub const FILE_NAME: &str = "last_character.json";

/// The delver the title offers to continue as, if any.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct LastDelver(pub Option<Character>);

/// Where [`LastDelver`] is saved; `None` keeps it for this run only.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct LastDelverFile(pub Option<PathBuf>);

impl LastDelverFile {
    /// Beside the settings file.
    pub fn default_path() -> Option<PathBuf> {
        SettingsFile::default_path().map(|settings| settings.with_file_name(FILE_NAME))
    }
}

/// Who the delver is, as saved: all of [`Character`] but what they wear, which is the
/// inventory's. What they have learned from books came later, so a file without it has none.
/// The saved delve ([`crate::save`]) keeps its delver this way too.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Saved {
    pub name: String,
    folk: Folk,
    class: Class,
    attributes: Attributes,
    body_seed: u32,
    /// The creator's body shape; files saved before the sliders have none, and roll it.
    #[serde(default)]
    shape: Option<fb_body::BodyShape>,
    #[serde(default)]
    sex: Option<crate::character::Sex>,
    dirt: f32,
    #[serde(default)]
    learned: AbilityBonus,
    #[serde(default)]
    books_read: Vec<u32>,
}

impl From<&Character> for Saved {
    fn from(c: &Character) -> Self {
        Saved { name: c.name.clone(), folk: c.folk, class: c.class, attributes: c.attributes, body_seed: c.body_seed, shape: c.shape, sex: c.sex, dirt: c.dirt, learned: c.learned, books_read: c.books_read.clone() }
    }
}

impl From<Saved> for Character {
    fn from(s: Saved) -> Self {
        Character { folk: s.folk, body_seed: s.body_seed, shape: s.shape, sex: s.sex, name: s.name, class: s.class, attributes: s.attributes, dirt: s.dirt.clamp(0.0, 1.0), worn: AbilityBonus::default(), learned: s.learned, books_read: s.books_read }
    }
}

/// The delver saved at `path`, or `None` if there is none or it can't be read.
pub fn load(path: &Path) -> Option<Character> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<Saved>(&text) {
        Ok(saved) if !saved.name.trim().is_empty() => Some(saved.into()),
        Ok(_) => None,
        Err(error) => {
            warn!("last delver: {} is unreadable ({error}); the title offers none", path.display());
            None
        }
    }
}

/// Writes `delver` to `path`, making its folder if need be; written beside it and renamed into
/// place, as the settings are.
pub fn save(path: &Path, delver: &Character) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let text = serde_json::to_string_pretty(&Saved::from(delver)).map_err(std::io::Error::other)?;
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, text + "\n")?;
    std::fs::rename(&partial, path)
}

/// Loads the [`LastDelver`] from a [`LastDelverFile`] (or [`LastDelverFile::default_path`]
/// without one) and saves it when it changes.
pub struct LastDelverPlugin;

impl Plugin for LastDelverPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<LastDelverFile>() {
            app.insert_resource(LastDelverFile(LastDelverFile::default_path()));
        }
        let file = app.world().resource::<LastDelverFile>().clone();
        app.insert_resource(LastDelver(file.0.as_deref().and_then(load))).add_systems(Last, (remember_lessons, save_last_delver).chain());
    }
}

/// Copies what the delver has learned from books onto the [`LastDelver`] when it is the same
/// delver (a delve begun by a debug seed is not remembered, so it never overwrites another's).
pub fn remember_lessons(character: Option<Res<Character>>, last: Option<ResMut<LastDelver>>) {
    let (Some(character), Some(mut last)) = (character, last) else { return };
    if !character.is_changed() {
        return;
    }
    let same = |d: &Character| d.name == character.name && d.body_seed == character.body_seed && d.folk == character.folk && d.class == character.class;
    let stale = last.0.as_ref().is_some_and(|d| same(d) && (d.learned != character.learned || d.books_read != character.books_read));
    if stale && let Some(delver) = last.0.as_mut() {
        delver.learned = character.learned;
        delver.books_read = character.books_read.clone();
    }
}

fn save_last_delver(last: Res<LastDelver>, file: Res<LastDelverFile>) {
    if !last.is_changed() || last.is_added() {
        return;
    }
    if let (Some(path), Some(delver)) = (&file.0, &last.0)
        && let Err(error) = save(path, delver)
    {
        warn!("last delver: could not save {}: {error}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("fb-last-delver-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder.join(FILE_NAME)
    }

    #[test]
    fn a_saved_delver_comes_back_the_same() {
        let path = scratch("round");
        let delver = Character { name: "Brakka Stonefist".into(), folk: Folk::Lizardfolk, class: Class::Cleric, ..Character::roll(21) };
        save(&path, &delver).unwrap();
        assert_eq!(load(&path), Some(delver));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"folk\": \"lizard\"") && text.contains("\"class\": \"cleric\""), "{text}");
    }

    #[test]
    fn a_delver_saved_as_a_ranger_loads_as_a_fighter() {
        let path = scratch("ranger");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"name":"Olm","folk":"human","class":"ranger","attributes":{"strength":10,"dexterity":10,"constitution":10,"intelligence":10,"wisdom":10},"body_seed":1,"dirt":0.4}"#).unwrap();
        let old = load(&path).expect("a ranger saved before the class was removed still loads");
        assert_eq!(old.class, Class::Fighter);
    }

    #[test]
    fn what_they_learned_is_saved_and_an_older_file_loads_with_none() {
        let path = scratch("learned");
        let mut delver = Character::roll(21);
        delver.learned.wisdom = 1;
        delver.books_read = vec![77];
        save(&path, &delver).unwrap();
        assert_eq!(load(&path), Some(delver));
        std::fs::write(&path, r#"{"name":"Olm","folk":"human","class":"mage","attributes":{"strength":10,"dexterity":10,"constitution":10,"intelligence":10,"wisdom":10},"body_seed":1,"dirt":0.4}"#).unwrap();
        let old = load(&path).expect("an older file still loads");
        assert_eq!((old.learned, old.books_read), (AbilityBonus::default(), vec![]));
    }

    #[test]
    fn a_lesson_learned_in_the_delve_reaches_the_last_delver_but_not_anothers() {
        let path = scratch("lesson");
        let mut app = App::new();
        let delver = Character::roll(4);
        app.insert_resource(LastDelverFile(Some(path.clone()))).add_plugins(LastDelverPlugin);
        app.world_mut().resource_mut::<LastDelver>().0 = Some(delver.clone());
        app.insert_resource(delver.clone());
        app.update();
        app.world_mut().resource_mut::<Character>().learned.strength = 1;
        app.world_mut().resource_mut::<Character>().books_read.push(5);
        app.update();
        let saved = load(&path).unwrap();
        assert_eq!((saved.learned.strength, saved.books_read), (1, vec![5]));
        // Someone else delving leaves the last delver alone.
        app.insert_resource(Character { learned: AbilityBonus { wisdom: 3, ..default() }, ..Character::roll(9) });
        app.update();
        assert_eq!(load(&path).unwrap().learned.wisdom, 0);
    }

    #[test]
    fn no_file_or_a_bad_one_is_no_delver() {
        let path = scratch("bad");
        assert_eq!(load(&path), None);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path), None);
        std::fs::write(&path, r#"{"name":"  ","folk":"human","class":"mage","attributes":{"strength":10,"dexterity":10,"constitution":10,"intelligence":10,"wisdom":10},"body_seed":1,"dirt":0.4}"#).unwrap();
        assert_eq!(load(&path), None, "a delver needs a name");
    }

    #[test]
    fn a_new_last_delver_is_saved_but_the_loaded_one_is_not_rewritten() {
        let path = scratch("plugin");
        let mut app = App::new();
        app.insert_resource(LastDelverFile(Some(path.clone()))).add_plugins(LastDelverPlugin);
        app.update();
        assert_eq!(app.world().resource::<LastDelver>().0, None);
        assert!(!path.exists());
        let delver = Character::roll(4);
        app.world_mut().resource_mut::<LastDelver>().0 = Some(delver.clone());
        app.update();
        assert_eq!(load(&path), Some(delver.clone()));
        // A fresh run reads it back.
        let mut again = App::new();
        again.insert_resource(LastDelverFile(Some(path))).add_plugins(LastDelverPlugin);
        assert_eq!(again.world().resource::<LastDelver>().0, Some(delver));
    }

    #[test]
    fn a_shaped_delver_comes_back_with_its_body_and_old_files_roll_theirs() {
        let path = scratch("shape");
        let shape = fb_body::BodySpec::roll(Folk::Human, 5).shape().parse("muscle=0.9,bust=0.7,height=1.08").unwrap();
        let delver = Character { shape: Some(shape), ..Character::roll(5) };
        save(&path, &delver).unwrap();
        let back = load(&path).unwrap();
        assert_eq!(back.spec(), delver.spec());
        std::fs::write(&path, r#"{"name":"Old","folk":"human","class":"mage","attributes":{"strength":10,"dexterity":10,"constitution":10,"intelligence":10,"wisdom":10},"body_seed":1,"dirt":0.4}"#).unwrap();
        let old = load(&path).unwrap();
        assert_eq!((old.shape, old.spec()), (None, fb_body::BodySpec::roll(Folk::Human, 1)));
    }
}
