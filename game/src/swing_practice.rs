//! Swing practice (`--swings`): the level, with keys for trying out every melee weapon's
//! swings. A panel at the top left names what is held and the strike last thrown.
//!
//! - `[` and `]` hold the previous or next melee base, `N` rolls another of the same base.
//! - `F1` to `F7` throw every swing as one strike (overhead, high right, high left, from the
//!   right, from the left, rising, stab), whatever the weapon; `F8` rolls them again. They are
//!   function keys so they never collide with the digits, which are the quickslots.
//! - `G` swings over and over; `T` slows time to a quarter, to follow the arc.
//!
//! For captures, `FB_STRIKE=<1 to 7>` starts with that strike pinned, as its `F` key would, and
//! `FB_SWING=1` swings over and over.
//!
//! Blows head for a creature in reach as in play, so walk up to a rat to see them bend.

use armory::Family;
use bevy::prelude::*;
use fb_items::Slot;

use crate::carry::ViewModelCamera;
use crate::combat::swing::{Strike, Swing, SwingPractice};
use crate::inventory::{Inventory, hold_rolled};
use crate::play::{DelveSystems, Player};
use crate::ui::UiFonts;

pub struct SwingPracticePlugin;

impl Plugin for SwingPracticePlugin {
    fn build(&self, app: &mut App) {
        let strike = std::env::var("FB_STRIKE")
            .ok()
            .and_then(|n| n.trim().parse::<usize>().ok())
            .and_then(|n| STRIKES.get(n.wrapping_sub(1)).copied());
        app.insert_resource(SwingPractice {
            strike,
            repeat: false,
        })
        .init_resource::<Practising>()
        .add_systems(
            Update,
            (spawn_panel, (choose, show).chain().in_set(DelveSystems)),
        );
    }
}

/// The families swung in practice, and so every base of them.
const MELEE: [Family; 7] = [
    Family::Sword,
    Family::Mace,
    Family::Axe,
    Family::Club,
    Family::Dagger,
    Family::Spear,
    Family::Staff,
];

/// The strikes on `F1` to `F7`.
const STRIKES: [Strike; 7] = [
    Strike::Overhead,
    Strike::HighRight,
    Strike::HighLeft,
    Strike::FromRight,
    Strike::FromLeft,
    Strike::Rising,
    Strike::Stab,
];

/// The keys for [`STRIKES`], in order, and the one that goes back to rolled strikes.
const STRIKE_KEYS: [KeyCode; 7] = [
    KeyCode::F1,
    KeyCode::F2,
    KeyCode::F3,
    KeyCode::F4,
    KeyCode::F5,
    KeyCode::F6,
    KeyCode::F7,
];
const ROLLED_KEY: KeyCode = KeyCode::F8;

/// Every melee base, in family order.
pub fn melee_bases() -> Vec<&'static str> {
    MELEE
        .into_iter()
        .flat_map(fb_items::armory_base::bases_of)
        .collect()
}

/// What practice holds: the base (an index into [`melee_bases`]), the seed it was rolled at,
/// and the name of what was rolled, once it is in hand.
#[derive(Resource, Clone, Debug, Default)]
struct Practising {
    base: usize,
    seed: u32,
    held: Option<String>,
    last: Option<Strike>,
}

/// The panel's text.
#[derive(Component)]
struct PracticePanel;

fn spawn_panel(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    cameras: Query<Entity, Added<ViewModelCamera>>,
) {
    for camera in &cameras {
        commands.spawn((
            Name::new("swing practice"),
            PracticePanel,
            UiTargetCamera(camera),
            Text::default(),
            UiFonts::style(&fonts.mono, 14.0, Color::srgb(0.95, 0.9, 0.8)),
            TextShadow::default(),
            Node {
                position_type: PositionType::Absolute,
                left: px(16),
                top: px(16),
                ..default()
            },
            Pickable::IGNORE,
        ));
    }
}

/// Reads the practice keys: the weapon in hand, the strike, repeating and slow time.
fn choose(
    keys: Res<ButtonInput<KeyCode>>,
    mut practising: ResMut<Practising>,
    mut practice: ResMut<SwingPractice>,
    mut inventory: ResMut<Inventory>,
    mut time: ResMut<Time<Virtual>>,
) {
    let bases = melee_bases();
    let mut reroll = practising.held.is_none();
    if keys.just_pressed(KeyCode::BracketRight) {
        practising.base = (practising.base + 1) % bases.len();
        reroll = true;
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        practising.base = (practising.base + bases.len() - 1) % bases.len();
        reroll = true;
    }
    if keys.just_pressed(KeyCode::KeyN) {
        practising.seed = practising.seed.wrapping_add(1);
        reroll = true;
    }
    if reroll {
        practising.held = hold_rolled(
            &mut inventory,
            Slot::Mainhand,
            bases[practising.base],
            practising.seed,
        );
    }
    for (key, strike) in STRIKE_KEYS.into_iter().zip(STRIKES) {
        if keys.just_pressed(key) {
            practice.strike = Some(strike);
        }
    }
    if keys.just_pressed(ROLLED_KEY) {
        practice.strike = None;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        practice.repeat = !practice.repeat;
    }
    if keys.just_pressed(KeyCode::KeyT) {
        let slow = time.relative_speed() < 1.0;
        time.set_relative_speed(if slow { 1.0 } else { 0.25 });
    }
}

/// Writes the panel: what is held, the strike, and the keys.
fn show(
    mut practising: ResMut<Practising>,
    practice: Res<SwingPractice>,
    time: Res<Time<Virtual>>,
    swings: Query<&Swing, With<Player>>,
    mut panels: Query<&mut Text, With<PracticePanel>>,
) {
    if let Some(swing) = swings.iter().next() {
        practising.last = Some(swing.strike);
    }
    let bases = melee_bases();
    let held = practising.held.as_deref().unwrap_or("nothing");
    let strike = |s: Option<Strike>| s.map_or("rolled".to_string(), |s| format!("{s:?}"));
    let text = format!(
        "{held}  ({} {}/{}, seed {})\nlast strike  {}\nthrown as    {}{}{}\n\n[ ]  weapon    N  another\nF1-F7  strike   F8  rolled\nG  repeat     T  slow",
        bases[practising.base],
        practising.base + 1,
        bases.len(),
        practising.seed,
        strike(practising.last).replace("rolled", "-"),
        strike(practice.strike),
        if practice.repeat { "   repeating" } else { "" },
        if time.relative_speed() < 1.0 {
            "   slow"
        } else {
            ""
        },
    );
    for mut panel in &mut panels {
        if panel.0 != text {
            panel.0 = text.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strike_keys_are_not_the_quickslot_digits_and_pin_and_unpin_a_strike() {
        let digits = [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
            KeyCode::Digit0,
        ];
        assert!(
            STRIKE_KEYS
                .iter()
                .chain([&ROLLED_KEY])
                .all(|k| !digits.contains(k))
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Practising>()
            .init_resource::<SwingPractice>()
            .init_resource::<Inventory>()
            .add_systems(Update, choose);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Digit3);
        app.update();
        assert_eq!(
            app.world().resource::<SwingPractice>().strike,
            None,
            "a digit is a quickslot, not a strike"
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F3);
        app.update();
        assert_eq!(
            app.world().resource::<SwingPractice>().strike,
            Some(Strike::HighLeft)
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(ROLLED_KEY);
        app.update();
        assert_eq!(app.world().resource::<SwingPractice>().strike, None);
    }

    #[test]
    fn practice_offers_every_melee_base_and_each_rolls() {
        let bases = melee_bases();
        for base in [
            "Sword", "Mace", "Hand axe", "Club", "Dagger", "Spear", "Staff",
        ] {
            assert!(bases.contains(&base), "{base}");
        }
        for base in bases {
            let mut inventory = Inventory::default();
            assert!(
                hold_rolled(&mut inventory, Slot::Mainhand, base, 0).is_some(),
                "{base}"
            );
            assert!(inventory.worn(Slot::Mainhand).is_some(), "{base} in hand");
        }
    }
}
