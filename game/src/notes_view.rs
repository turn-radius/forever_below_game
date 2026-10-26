//! The notes lab (`--notes`): one level of a world, its lore down the left, and a dozen of the
//! notes that could lie on it down the right, each tagged with its kind. For reading what the
//! notes grammar (`data/notes.json`) writes of a real level, and how often each kind turns up.
//!
//! Keys: Up and Down change the depth, Left and Right the world, R rerolls the notes (the lore
//! stays). Command line: `--notes --world 1207 --depth 3 --seed 1` ([`NotesView::from_args`]).

use bevy::prelude::*;
use fb_items::notes::{Lore, NoteKind, note_kind, write_note};

use crate::ui::{BG, FAINT, GOLD, INK, InterfaceScalePlugin, MUTED, UiFonts};

/// How many notes are shown.
pub const NOTES: u32 = 12;

pub struct NotesViewPlugin;

impl Plugin for NotesViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NotesView>()
            .init_resource::<UiFonts>()
            .insert_resource(ClearColor(BG))
            .add_plugins(InterfaceScalePlugin)
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (keys, show.run_if(resource_changed::<NotesView>)).chain(),
            );
    }
}

/// What the lab shows. Changing it rebuilds the page.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotesView {
    pub world: u32,
    pub depth: u32,
    /// The first note's seed; the rest follow it.
    pub seed: u32,
}

impl Default for NotesView {
    fn default() -> Self {
        NotesView {
            world: 1207,
            depth: 1,
            seed: 1,
        }
    }
}

impl NotesView {
    /// Parses `--world`, `--depth` and `--seed` (also `--flag=value`); `--notes` itself is
    /// skipped. `args` excludes the program name.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<NotesView, String> {
        let mut view = NotesView::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
                None => (arg, None),
            };
            let mut value = || {
                inline
                    .clone()
                    .or_else(|| args.next())
                    .ok_or(format!("{flag} needs a value"))
            };
            let number = |text: String| {
                text.parse::<u32>()
                    .map_err(|_| format!("{flag}: {text:?} is not a number"))
            };
            match flag.as_str() {
                "--notes" => {}
                "--world" => view.world = number(value()?)?,
                "--depth" => view.depth = number(value()?)?.max(1),
                "--seed" => view.seed = number(value()?)?,
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        Ok(view)
    }

    /// The lore of the level shown.
    pub fn lore(&self) -> Lore {
        fb_level::generate_in(
            self.world,
            self.depth,
            &fb_level::LevelParams::for_style(fb_level::LevelStyle::for_depth(self.depth)),
        )
        .lore()
    }

    /// The seeds of the notes shown.
    pub fn seeds(&self) -> impl Iterator<Item = u32> + use<> {
        let first = self.seed;
        (0..NOTES).map(move |i| first.wrapping_add(i))
    }

    /// Moves on to the next dozen notes.
    pub fn reroll(&mut self) {
        self.seed = self.seed.wrapping_add(NOTES);
    }
}

/// The lore as lines for the left column: a heading and its entries.
pub fn lore_lines(lore: &Lore) -> Vec<(&'static str, Vec<String>)> {
    let list = |items: Vec<String>| {
        if items.is_empty() {
            vec!["none".to_string()]
        } else {
            items
        }
    };
    vec![
        (
            "Level",
            vec![format!(
                "depth {}{}",
                lore.depth,
                lore.past
                    .as_ref()
                    .map_or(String::new(), |p| format!(", once a {p}"))
            )],
        ),
        ("Factions", list(lore.factions.clone())),
        (
            "Hates",
            list(
                lore.hates
                    .iter()
                    .map(|f| format!("{} \u{2192} {}", f.faction, f.thing.many))
                    .collect(),
            ),
        ),
        (
            "Likes",
            list(
                lore.likes
                    .iter()
                    .map(|f| format!("{} \u{2192} {}", f.faction, f.thing.many))
                    .collect(),
            ),
        ),
        (
            "Creatures",
            list(lore.creatures.iter().map(|c| c.many.clone()).collect()),
        ),
        ("Rooms", list(lore.places.clone())),
        (
            "Traders",
            list(
                lore.traders
                    .iter()
                    .map(|t| format!("{}, {} of {}", t.name, t.trade, t.faction))
                    .collect(),
            ),
        ),
        (
            "Books",
            list(
                lore.books
                    .iter()
                    .map(|b| {
                        format!(
                            "{} in the {}",
                            fb_items::lesson::long_name(b.teaches),
                            b.place.as_deref().unwrap_or("dark")
                        )
                    })
                    .collect(),
            ),
        ),
    ]
}

/// A kind as its tag reads.
fn kind_tag(kind: NoteKind) -> &'static str {
    kind.key().trim_start_matches("note_")
}

#[derive(Component)]
struct Page;

fn setup(mut commands: Commands) {
    commands.spawn((Camera2d, bevy::ui::IsDefaultUiCamera));
}

fn keys(input: Res<ButtonInput<KeyCode>>, mut view: ResMut<NotesView>) {
    if input.just_pressed(KeyCode::ArrowUp) {
        view.depth += 1;
    }
    if input.just_pressed(KeyCode::ArrowDown) && view.depth > 1 {
        view.depth -= 1;
    }
    if input.just_pressed(KeyCode::ArrowRight) {
        view.world = view.world.wrapping_add(1);
    }
    if input.just_pressed(KeyCode::ArrowLeft) {
        view.world = view.world.wrapping_sub(1);
    }
    if input.just_pressed(KeyCode::KeyR) {
        view.reroll();
    }
}

fn show(
    mut commands: Commands,
    view: Res<NotesView>,
    pages: Query<Entity, With<Page>>,
    fonts: Res<UiFonts>,
) {
    for page in &pages {
        commands.entity(page).despawn();
    }
    let lore = view.lore();
    let heading = |text: String| (Text::new(text), UiFonts::style(&fonts.display, 20.0, GOLD));
    commands
        .spawn((
            Page,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(24)),
                row_gap: px(14),
                ..default()
            },
        ))
        .with_children(|page| {
            page.spawn((
                Text::new(format!(
                    "Notes \u{b7} world {} \u{b7} depth {} \u{b7} notes from seed {}",
                    view.world, view.depth, view.seed
                )),
                UiFonts::style(&fonts.display, 26.0, INK),
            ));
            page.spawn((
                Text::new("Up/Down depth \u{b7} Left/Right world \u{b7} R reroll the notes"),
                UiFonts::style(&fonts.mono, 12.0, MUTED),
            ));
            page.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(28),
                flex_grow: 1.0,
                min_height: px(0),
                ..default()
            })
            .with_children(|columns| {
                columns
                    .spawn((Node {
                        width: percent(34),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(10),
                        overflow: Overflow::clip(),
                        ..default()
                    },))
                    .with_children(|left| {
                        left.spawn(heading("Lore".into()));
                        for (title, lines) in lore_lines(&lore) {
                            left.spawn(Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: px(2),
                                ..default()
                            })
                            .with_children(|section| {
                                section.spawn((
                                    Text::new(title),
                                    UiFonts::style(&fonts.body_medium, 14.0, GOLD),
                                ));
                                for line in lines {
                                    section.spawn((
                                        Text::new(line),
                                        UiFonts::style(&fonts.body, 13.5, INK),
                                    ));
                                }
                            });
                        }
                    });
                columns
                    .spawn((Node {
                        flex_grow: 1.0,
                        flex_basis: px(0),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        overflow: Overflow::clip(),
                        ..default()
                    },))
                    .with_children(|right| {
                        right.spawn(heading("Notes".into()));
                        for seed in view.seeds() {
                            right
                                .spawn((
                                    Node {
                                        flex_direction: FlexDirection::Row,
                                        column_gap: px(10),
                                        padding: UiRect::axes(px(10), px(6)),
                                        ..default()
                                    },
                                    BackgroundColor(BG.mix(&GOLD, 0.12)),
                                    BorderColor::all(FAINT),
                                ))
                                .with_children(|row| {
                                    row.spawn((
                                        Node {
                                            width: px(70),
                                            flex_shrink: 0.0,
                                            ..default()
                                        },
                                        Text::new(kind_tag(note_kind(seed, &lore))),
                                        UiFonts::style(&fonts.mono, 12.0, MUTED),
                                    ));
                                    row.spawn((
                                        Node {
                                            flex_grow: 1.0,
                                            flex_basis: px(0),
                                            ..default()
                                        },
                                        Text::new(write_note(seed, &lore)),
                                        UiFonts::style(&fonts.body_italic, 15.0, INK),
                                    ));
                                });
                        }
                    });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn the_command_line_sets_world_depth_and_seed() {
        assert_eq!(
            NotesView::from_args(args("--notes")).unwrap(),
            NotesView::default()
        );
        assert_eq!(
            NotesView::from_args(args("--notes --world 9 --depth=3 --seed 40")).unwrap(),
            NotesView {
                world: 9,
                depth: 3,
                seed: 40
            }
        );
        assert_eq!(
            NotesView::from_args(args("--depth 0")).unwrap().depth,
            1,
            "the first level is the shallowest"
        );
        assert!(NotesView::from_args(args("--depth deep")).is_err());
        assert!(NotesView::from_args(args("--colour red")).is_err());
    }

    #[test]
    fn rerolling_changes_the_notes_not_the_lore() {
        let mut view = NotesView::default();
        let lore = view.lore();
        let before: Vec<String> = view.seeds().map(|s| write_note(s, &lore)).collect();
        view.reroll();
        assert_eq!(view.lore(), lore);
        let after: Vec<String> = view.seeds().map(|s| write_note(s, &lore)).collect();
        assert_ne!(before, after);
    }

    #[test]
    fn the_lore_column_lists_every_heading_with_something_under_it() {
        let lines = lore_lines(&NotesView::default().lore());
        assert_eq!(
            lines.iter().map(|(t, _)| *t).collect::<Vec<_>>(),
            [
                "Level",
                "Factions",
                "Hates",
                "Likes",
                "Creatures",
                "Rooms",
                "Traders",
                "Books"
            ]
        );
        assert!(lines.iter().all(|(_, l)| !l.is_empty()));
    }
}
