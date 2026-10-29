//! A cleric's devotion, offered when their deeds earn it.
//!
//! - The deed tally ([`crate::deeds`]) counts kills by family and weapon. Enough kills of one
//!   family of creature, folk or denizen (`creature_kills` in `[devotion]` of `data/rules.toml`)
//!   offer a favored foe against it ("Goblin Slayer") or an alignment with it; enough with one
//!   kind of weapon (`weapon_kills`) offer to dedicate themselves to it
//!   ([`fb_character::devotion::due`]). The offer is a modal screen ([`Screen::Devotion`]) over
//!   the level, made to a cleric only.
//! - The cleric may decline (the Not yet button, or Esc); the same offer comes again at double
//!   the count. A cleric who already follows something is offered a switch the same way, and
//!   accepting drops the old one's bonuses: gear and standing are worked out from the choice
//!   ([`crate::character::Character::loadout`], [`apply_favour`]), never stored.
//! - Aligned with a creature group they take its trait. Aligned with a folk or a denizen people,
//!   the factions of that people think better of them and forgive more; against one, they think
//!   less ([`fb_character::devotion::Devotion::favour`], [`fb_faction::Favour`]).
//! - The character sheet only shows the devotion they follow.
//! - The screen is laid out for the 1920 by 1080 reference and scales with the window
//!   ([`crate::ui`]); the mouse is read in the interface's own pixels ([`ui_cursor`]).
//! - `FB_DEBUG_DEVOTION=weapon|creature|goblin` gives a cleric (with `FB_CLASS=cleric`) the kills
//!   that earn that offer a few frames into the delve, for captures.

use std::collections::BTreeMap;

use bevy::{
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_character::devotion::{self, Devotion, Offer};
use fb_character::kin::Kin;
use fb_faction::{Favour, World};

use crate::carry::ViewModelCamera;
use crate::character::{Character, Class};
use crate::combat::feedback::Corpse;
use crate::creatures::Critter;
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, Screen, ScreenOpen, show_screen};
use crate::record::WorldRecord;
use crate::rules::GameRules;
use crate::travel::Abyss;
use crate::ui::{ACCENT, BG, BRIGHT, CELL, FAINT, INK, LINE, MUTED, UiFonts, node_rect, ui_cursor};

/// The width of the panel at the reference size, logical pixels.
const WIDTH: f32 = 760.0;

pub struct DevotionPlugin;

impl Plugin for DevotionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Offered>().add_systems(OnExit(crate::launch::AppState::Delving), |mut offered: ResMut<Offered>| offered.0 = None);
        if std::env::var("FB_DEBUG_DEVOTION").is_ok_and(|v| !v.is_empty()) {
            app.add_systems(Update, earn_for_capture.in_set(DelveSystems).before(offer_devotion));
        }
        app.add_systems(Update, (offer_devotion, click_offer, answer_closed, hover_offer, build_offer).chain().in_set(DelveSystems).after(crate::deeds::RecordsDeeds));
        app.add_systems(Update, apply_favour.in_set(DelveSystems));
    }
}

/// The offer the open screen is making, if any.
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct Offered(pub Option<Offer>);

/// What a button on the screen does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answer {
    Take(Devotion),
    Decline,
}

#[derive(Component, Debug)]
struct OfferButton(Answer);

/// The screen's root.
#[derive(Component, Debug)]
pub struct OfferScreen;

/// A cleric accepts `taking` or, with `None`, declines `offer`: either way it is answered and the
/// next comes at double the count. Taking one replaces the one they had.
pub fn answer(character: &mut Character, offer: Offer, taking: Option<Devotion>) {
    devotion::answer(&mut character.deeds, offer);
    if let Some(devotion) = taking {
        character.devotion = Some(devotion);
    }
}

/// What the offer lets the cleric choose: its choices, less the one they already follow.
pub fn choices(character: &Character, rules: &devotion::DevotionRules, offer: Offer) -> Vec<Devotion> {
    offer.choices(rules).into_iter().filter(|d| character.devotion != Some(*d)).collect()
}

/// Opens the screen when a cleric's deeds have earned an offer and nothing else is open.
fn offer_devotion(
    mut character: ResMut<Character>,
    rules: Option<Res<GameRules>>,
    mut open: ResMut<ScreenOpen>,
    mut offered: ResMut<Offered>,
    journey: Option<Res<crate::travel::Journey>>,
    players: Query<(&Transform, Option<&crate::combat::Health>), With<crate::play::Player>>,
    creatures: Query<&Transform, (With<Critter>, Without<Corpse>)>,
    folk: Query<(&Transform, &crate::hostility::Hostility), (With<crate::folk::Denizen>, Without<Corpse>)>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
) {
    if character.class != Class::Cleric || offered.0.is_some() || open.0.is_some() || journey.is_some() {
        return;
    }
    let rules = &GameRules::of(&rules).devotion;
    // A due offer waits, pending, for a quiet moment: the delver up, and nothing alive and
    // hostile near.
    if let Ok((at, health)) = players.single() {
        let me = at.translation.xz();
        let near = |t: &Transform| t.translation.xz().distance(me) <= rules.quiet_distance;
        if health.is_some_and(|h| h.is_down()) || creatures.iter().any(near) || folk.iter().any(|(t, h)| h.is_hostile() && near(t)) {
            return;
        }
    }
    // An offer that would change nothing is answered without being made.
    while let Some(offer) = devotion::due(&character.deeds, rules) {
        if choices(&character, rules, offer).is_empty() {
            devotion::answer(&mut character.deeds, offer);
            continue;
        }
        offered.0 = Some(offer);
        show_screen(&mut open, Some(Screen::Devotion), cursor.as_deref_mut().map(|c| &mut **c));
        return;
    }
}

/// A click on a button.
fn click_offer(
    input: ActionInput,
    mut offered: ResMut<Offered>,
    mut character: ResMut<Character>,
    mut screen: ResMut<ScreenOpen>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    buttons: Query<(&OfferButton, &ComputedNode, &UiGlobalTransform)>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
) {
    if !screen.is(Screen::Devotion) || !input.just_pressed(Action::Choose) {
        return;
    }
    let Some(offer) = offered.0 else { return };
    let Some(point) = windows.iter().next().and_then(|w| ui_cursor(w, ui_scale.as_deref())) else { return };
    let Some((button, ..)) = buttons.iter().find(|(_, node, at)| node_rect(node, at).contains(point)) else { return };
    answer(&mut character, offer, if let Answer::Take(d) = button.0 { Some(d) } else { None });
    offered.0 = None;
    show_screen(&mut screen, None, cursor.as_deref_mut().map(|c| &mut **c));
}

/// An offer whose screen was closed some other way (Esc) is declined.
fn answer_closed(open: Res<ScreenOpen>, mut offered: ResMut<Offered>, mut character: ResMut<Character>) {
    if let Some(offer) = offered.0
        && !open.is(Screen::Devotion)
    {
        answer(&mut character, offer, None);
        offered.0 = None;
    }
}

/// Lights the button under the mouse.
fn hover_offer(
    open: Res<ScreenOpen>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    mut buttons: Query<(&OfferButton, &ComputedNode, &UiGlobalTransform, &mut BackgroundColor, &mut BorderColor)>,
) {
    if !open.is(Screen::Devotion) {
        return;
    }
    let point = windows.iter().next().and_then(|w| ui_cursor(w, ui_scale.as_deref()));
    for (button, node, at, mut fill, mut border) in &mut buttons {
        let over = point.is_some_and(|p| node_rect(node, at).contains(p));
        let (back, line) = match (over, button.0) {
            (true, _) => (Color::srgb(0.20, 0.17, 0.10), ACCENT),
            (false, Answer::Take(_)) => (CELL, ACCENT.with_alpha(0.55)),
            (false, Answer::Decline) => (CELL, LINE),
        };
        fill.set_if_neq(BackgroundColor(back));
        border.set_if_neq(BorderColor::all(line));
    }
}

/// Draws the screen while an offer is open, and clears it when it is not.
fn build_offer(
    mut commands: Commands,
    open: Res<ScreenOpen>,
    offered: Res<Offered>,
    character: Res<Character>,
    rules: Option<Res<GameRules>>,
    fonts: Option<Res<UiFonts>>,
    cameras: Query<Entity, With<ViewModelCamera>>,
    roots: Query<Entity, With<OfferScreen>>,
) {
    let existing = roots.iter().next();
    let (true, Some(offer), Some(fonts)) = (open.is(Screen::Devotion), offered.0, fonts) else {
        if let Some(root) = existing {
            commands.entity(root).despawn();
        }
        return;
    };
    if existing.is_some() {
        return;
    }
    let Some(camera) = cameras.iter().next() else { return };
    let rules = &GameRules::of(&rules).devotion;
    let headline = offer.headline(&character.deeds);
    let ask = match offer {
        Offer::Weapon(family) => format!("Your {} have done holy work. Dedicate yourself to them?", devotion::plural(family)),
        Offer::Creature(kin) => format!("You have made {} your business. Turn that into a devotion?", kin.plural()),
    };
    let again = devotion::threshold(&character.deeds, rules, offer).saturating_mul(2);
    let replaces = character.devotion.map(|d| format!("Taking one of these gives up {}, and what it gave you.", d.label(character.body_seed)));
    commands
        .spawn((
            Name::new("Devotion offer"),
            OfferScreen,
            DespawnOnExit(crate::launch::AppState::Delving),
            UiTargetCamera(camera),
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            GlobalZIndex(12),
        ))
        .with_children(|root| {
            root.spawn((
                Name::new("Devotion panel"),
                Node {
                    width: px(WIDTH),
                    max_width: percent(94),
                    max_height: percent(94),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(14),
                    padding: UiRect::axes(px(30), px(24)),
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::all(px(14)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(BG),
                BorderColor::all(LINE),
                BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.55), px(0), px(14), px(0), px(34)),
            ))
            .with_children(|panel| {
                panel.spawn((Text::new("A devotion"), UiFonts::style(&fonts.display, 36.0, INK)));
                panel.spawn((Text::new(headline), UiFonts::style(&fonts.mono, 16.0, ACCENT)));
                panel.spawn((Text::new(ask), UiFonts::style(&fonts.body_italic, 19.0, MUTED)));
                panel.spawn((Node { height: px(1), flex_shrink: 0.0, ..default() }, BackgroundColor(FAINT)));
                for devotion in choices(&character, rules, offer) {
                    panel
                        .spawn((
                            OfferButton(Answer::Take(devotion)),
                            Node { flex_direction: FlexDirection::Column, row_gap: px(3), padding: UiRect::axes(px(16), px(10)), border: UiRect::all(px(1.5)), border_radius: BorderRadius::all(px(8)), ..default() },
                            BackgroundColor(CELL),
                            BorderColor::all(ACCENT.with_alpha(0.55)),
                        ))
                        .with_children(|button| {
                            button.spawn((Text::new(devotion.label(character.body_seed)), UiFonts::style(&fonts.display, 24.0, BRIGHT)));
                            button.spawn((Text::new(devotion.effect_text(rules)), UiFonts::style(&fonts.body, 16.0, MUTED)));
                        });
                }
                if let Some(replaces) = replaces {
                    panel.spawn((Text::new(replaces), UiFonts::style(&fonts.body, 15.0, MUTED)));
                }
                panel
                    .spawn((
                        OfferButton(Answer::Decline),
                        Node { flex_direction: FlexDirection::Column, row_gap: px(3), padding: UiRect::axes(px(16), px(10)), border: UiRect::all(px(1.5)), border_radius: BorderRadius::all(px(8)), ..default() },
                        BackgroundColor(CELL),
                        BorderColor::all(LINE),
                    ))
                    .with_children(|button| {
                        button.spawn((Text::new("Not yet"), UiFonts::style(&fonts.display, 22.0, BRIGHT)));
                        button.spawn((Text::new(format!("You will be offered again at {again} kills.")), UiFonts::style(&fonts.body, 16.0, MUTED)));
                    });
            });
        });
}

/// The favour a cleric's devotion puts on each faction of `world`: those whose folk or denizens
/// are the family they are aligned with (more) or devoted against (less). None for any other
/// class or devotion.
pub fn favour_of(character: &Character, rules: &devotion::DevotionRules, world: &World) -> BTreeMap<fb_faction::Faction, Favour> {
    let Some((kin, bonus, loss_percent)) = character.devotion.filter(|_| character.class == Class::Cleric).and_then(|d| d.favour(rules)) else { return BTreeMap::new() };
    world
        .factions()
        .iter()
        .chain(world.denizen_factions())
        .filter(|spec| spec.denizen.map_or_else(|| Kin::of_folk(spec.folk), Kin::Denizen) == kin)
        .map(|spec| (spec.id, Favour { bonus, loss_percent }))
        .collect()
}

/// Keeps the factions' favour toward the delver true to their devotion: computed from it, so
/// switching drops the old favour and nothing is reversed by hand.
fn apply_favour(character: Res<Character>, abyss: Option<Res<Abyss>>, rules: Option<Res<GameRules>>, record: Option<ResMut<WorldRecord>>, mut world: Local<Option<(u32, World)>>) {
    let (Some(abyss), Some(mut record)) = (abyss, record) else { return };
    let rules = &GameRules::of(&rules).devotion;
    let wants = character.devotion.filter(|_| character.class == Class::Cleric).and_then(|d| d.favour(rules)).is_some();
    if !wants && record.standings.favours().is_empty() {
        return;
    }
    if !world.as_ref().is_some_and(|(seed, _)| *seed == abyss.run_seed) {
        *world = Some((abyss.run_seed, World::new(abyss.run_seed)));
    }
    let Some((_, world)) = world.as_ref() else { return };
    let favour = favour_of(&character, rules, world);
    if *record.standings.favours() != favour {
        record.standings.set_favour(favour);
    }
}

/// `FB_DEBUG_DEVOTION=weapon|creature|goblin`: the kills that earn that offer, given once.
fn earn_for_capture(mut character: ResMut<Character>, rules: Option<Res<GameRules>>, mut done: Local<bool>, mut frames: Local<u32>) {
    *frames += 1;
    if *done || *frames < 20 {
        return;
    }
    *done = true;
    let rules = &GameRules::of(&rules).devotion;
    match std::env::var("FB_DEBUG_DEVOTION").unwrap_or_default().to_lowercase().as_str() {
        "weapon" => {
            for _ in 0..rules.weapon_kills {
                character.deeds.kill("creature:skeleton", Kin::Foe(fb_items::Foe::Undead), Some(armory::Family::Mace));
            }
        }
        "goblin" => {
            for _ in 0..rules.creature_kills {
                character.deeds.kill("folk:goblin", Kin::Folk(fb_body::Folk::Goblin), None);
            }
        }
        _ => {
            for _ in 0..rules.creature_kills {
                character.deeds.kill("creature:skeleton", Kin::Foe(fb_items::Foe::Undead), None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_body::Folk;
    use fb_character::devotion::DevotionRules;
    use fb_items::Foe;

    const UNDEAD: Kin = Kin::Foe(Foe::Undead);
    const SPIDER: Kin = Kin::Foe(Foe::Spider);

    fn cleric() -> Character {
        Character { class: Class::Cleric, devotion: None, deeds: Default::default(), ..Character::roll(3) }
    }

    fn app(character: Character) -> App {
        let mut app = App::new();
        app.insert_resource(character).init_resource::<ScreenOpen>().init_resource::<Offered>().add_systems(Update, (offer_devotion, answer_closed).chain());
        app
    }

    fn slay(character: &mut Character, kin: Kin, n: u32) {
        for _ in 0..n {
            character.deeds.kill(format!("kin:{}", kin.key()), kin, None);
        }
    }

    #[test]
    fn the_threshold_opens_the_screen_with_the_offer() {
        let need = DevotionRules::default().creature_kills;
        let mut c = cleric();
        slay(&mut c, UNDEAD, need - 1);
        let mut app = app(c);
        app.update();
        assert_eq!(app.world().resource::<ScreenOpen>().0, None, "one short");
        slay(&mut app.world_mut().resource_mut::<Character>(), UNDEAD, 1);
        app.update();
        assert!(app.world().resource::<ScreenOpen>().is(Screen::Devotion));
        assert_eq!(app.world().resource::<Offered>().0, Some(Offer::Creature(UNDEAD)));
    }

    #[test]
    fn a_weapon_threshold_offers_the_dedication() {
        let need = DevotionRules::default().weapon_kills;
        let mut c = cleric();
        for _ in 0..need {
            c.deeds.kill("creature:rat", Kin::Foe(Foe::Vermin), Some(armory::Family::Mace));
        }
        let mut app = app(c);
        app.update();
        assert_eq!(app.world().resource::<Offered>().0, Some(Offer::Weapon(armory::Family::Mace)));
    }

    #[test]
    fn an_offer_due_mid_fight_waits_until_it_is_quiet() {
        let need = DevotionRules::default().creature_kills;
        let mut c = cleric();
        slay(&mut c, UNDEAD, need);
        let mut app = app(c);
        app.world_mut().spawn((crate::play::Player, Transform::default(), crate::combat::Health::full(10)));
        let spot = fb_level::generate(1207, 1).creatures[0];
        let foe = app.world_mut().spawn((Critter::new(spot, spot.creature()), Transform::from_xyz(20.0, 0.0, 0.0))).id();
        app.update();
        assert_eq!(app.world().resource::<ScreenOpen>().0, None, "a creature is near");
        app.world_mut().entity_mut(foe).insert(Corpse { since: 0.0, fell: Transform::default() });
        app.update();
        assert!(app.world().resource::<ScreenOpen>().is(Screen::Devotion), "it opens once the creature is down");
    }

    #[test]
    fn an_offer_waits_while_the_delver_is_down() {
        let need = DevotionRules::default().creature_kills;
        let mut c = cleric();
        slay(&mut c, UNDEAD, need);
        let mut app = app(c);
        let player = app.world_mut().spawn((crate::play::Player, Transform::default(), crate::combat::Health { hp: 0, max: 10 })).id();
        app.update();
        assert_eq!(app.world().resource::<ScreenOpen>().0, None);
        app.world_mut().entity_mut(player).insert(crate::combat::Health::full(10));
        app.update();
        assert!(app.world().resource::<ScreenOpen>().is(Screen::Devotion));
    }

    #[test]
    fn only_a_cleric_is_offered() {
        let mut c = cleric();
        c.class = Class::Fighter;
        slay(&mut c, UNDEAD, 50);
        let mut app = app(c);
        app.update();
        assert_eq!(app.world().resource::<ScreenOpen>().0, None);
    }

    #[test]
    fn closing_the_screen_declines_and_the_offer_returns_at_double() {
        let need = DevotionRules::default().creature_kills;
        let mut c = cleric();
        slay(&mut c, SPIDER, need);
        let mut app = app(c);
        app.update();
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
        app.update();
        assert_eq!(app.world().resource::<Offered>().0, None);
        assert_eq!(app.world().resource::<Character>().devotion, None, "declined");
        app.update();
        assert_eq!(app.world().resource::<ScreenOpen>().0, None, "not again at once");
        slay(&mut app.world_mut().resource_mut::<Character>(), SPIDER, need - 1);
        app.update();
        assert_eq!(app.world().resource::<ScreenOpen>().0, None, "short of double");
        slay(&mut app.world_mut().resource_mut::<Character>(), SPIDER, 1);
        app.update();
        assert!(app.world().resource::<ScreenOpen>().is(Screen::Devotion), "at double");
    }

    #[test]
    fn accepting_a_switch_drops_the_old_bonuses() {
        let rules = DevotionRules::default();
        let mut c = cleric();
        c.devotion = Some(Devotion::FavoredFoe(UNDEAD));
        let inventory = crate::inventory::Inventory::default();
        let ward = |c: &Character| c.gear(&inventory).ward_against(UNDEAD);
        assert_eq!(ward(&c), rules.favored_ward);
        slay(&mut c, SPIDER, rules.creature_kills);
        let offer = devotion::due(&c.deeds, &rules).unwrap();
        assert_eq!(choices(&c, &rules, offer), vec![Devotion::FavoredFoe(SPIDER), Devotion::Aligned(SPIDER)]);
        answer(&mut c, offer, Some(Devotion::Aligned(SPIDER)));
        assert_eq!(ward(&c), 0.0);
        assert_eq!(c.gear(&inventory).bane_against(UNDEAD), None);
        assert!(c.gear(&inventory).web_walker, "spiders give web immunity");
        assert_eq!(devotion::due(&c.deeds, &rules), None, "answered");
    }

    #[test]
    fn an_offer_of_what_they_already_follow_is_not_made() {
        let rules = DevotionRules::default();
        let mut c = cleric();
        c.devotion = Some(Devotion::Dedicated(armory::Family::Mace));
        let kins = Kin::all();
        for i in 0..rules.weapon_kills as usize {
            c.deeds.kill("x", kins[i % kins.len()], Some(armory::Family::Mace));
        }
        let mut app = app(c);
        app.update();
        assert_eq!(app.world().resource::<ScreenOpen>().0, None);
        assert_eq!(devotion::due(&app.world().resource::<Character>().deeds, &rules), None);
    }

    #[test]
    fn devotion_and_deeds_survive_a_save_and_load() {
        let mut c = cleric();
        c.devotion = Some(Devotion::Aligned(Kin::Folk(Folk::Dwarf)));
        slay(&mut c, Kin::Foe(Foe::Vermin), 4);
        c.deeds.kill("creature:rat", Kin::Foe(Foe::Vermin), Some(armory::Family::Axe));
        c.deeds.hit_taken([armory::Family::Garment]);
        devotion::answer(&mut c.deeds, Offer::Creature(UNDEAD));
        let text = serde_json::to_string(&crate::title::last_delver::Saved::from(&c)).unwrap();
        let back: Character = serde_json::from_str::<crate::title::last_delver::Saved>(&text).unwrap().into();
        assert_eq!((back.devotion, back.deeds.clone()), (c.devotion, c.deeds.clone()));
        assert_eq!(back.deeds.kills_of(Kin::Foe(Foe::Vermin)), 5);
    }

    #[test]
    fn a_save_from_before_devotions_loads_with_none() {
        let c = cleric();
        let mut value = serde_json::to_value(crate::title::last_delver::Saved::from(&c)).unwrap();
        value.as_object_mut().unwrap().remove("devotion");
        value.as_object_mut().unwrap().remove("deeds");
        let back: Character = serde_json::from_value::<crate::title::last_delver::Saved>(value).unwrap().into();
        assert_eq!((back.devotion, back.deeds), (None, Default::default()));
    }

    /// A world with a faction of `folk` (not a denizen faction) and its id.
    fn world_with(folk: Folk) -> (World, fb_faction::Faction) {
        (0..500)
            .map(World::new)
            .find_map(|w| w.factions().iter().find(|f| f.folk == folk && f.denizen.is_none()).map(|f| f.id).map(|id| (w, id)))
            .expect("a world with such a faction")
    }

    fn standings_for(world: &World, character: &Character) -> fb_faction::Standings {
        let mut standings = fb_faction::Standings::start_from_alignment(Class::Cleric.alignment(), world);
        standings.set_favour(favour_of(character, &DevotionRules::default(), world));
        standings
    }

    #[test]
    fn aligned_with_dwarves_their_factions_think_better_and_forgive_more() {
        let (world, dwarves) = world_with(Folk::Dwarf);
        let plain = standings_for(&world, &cleric());
        let mut devout = cleric();
        devout.devotion = Some(Devotion::Aligned(Kin::Folk(Folk::Dwarf)));
        let mut aligned = standings_for(&world, &devout);
        let rules = DevotionRules::default();
        assert_eq!(aligned.get(dwarves).value(), (plain.get(dwarves).value() + rules.aligned_standing).clamp(-100, 100));
        let other = world.factions().iter().find(|f| f.folk != Folk::Dwarf && f.denizen.is_none()).unwrap().id;
        assert_eq!(aligned.get(other), plain.get(other), "only the dwarves' factions");
        let (mut before, mut after) = (plain.clone(), aligned.clone());
        before.shift(dwarves, -20);
        after.shift(dwarves, -20);
        assert_eq!(plain.get(dwarves).value() - before.get(dwarves).value(), 20);
        assert_eq!(aligned.get(dwarves).value() - after.get(dwarves).value(), 10, "half the loss");
        aligned.set_favour(Default::default());
        assert_eq!(aligned.get(dwarves), plain.get(dwarves));
    }

    #[test]
    fn goblin_slayer_costs_standing_with_goblin_factions_and_switching_removes_it() {
        let (world, goblins) = world_with(Folk::Goblin);
        let plain = standings_for(&world, &cleric());
        let mut slayer = cleric();
        slayer.devotion = Some(Devotion::FavoredFoe(Kin::Folk(Folk::Goblin)));
        let lowered = standings_for(&world, &slayer);
        assert_eq!(lowered.get(goblins).value(), (plain.get(goblins).value() + DevotionRules::default().favored_standing).clamp(-100, 100));
        slayer.devotion = Some(Devotion::Aligned(SPIDER));
        assert_eq!(standings_for(&world, &slayer).get(goblins), plain.get(goblins), "a creature alignment puts nothing on factions");
        slayer.devotion = Some(Devotion::FavoredFoe(Kin::Folk(Folk::Goblin)));
        slayer.class = Class::Fighter;
        assert_eq!(standings_for(&world, &slayer).get(goblins), plain.get(goblins), "only a cleric's");
    }

    #[test]
    fn the_bonus_applies_against_a_folk_family_in_a_blow() {
        let mut c = cleric();
        c.devotion = Some(Devotion::FavoredFoe(Kin::Folk(Folk::Goblin)));
        let gear = c.gear(&crate::inventory::Inventory::default());
        let devout = gear.favored.zip(gear.favored_bane);
        let mut rng = procgen_rng::Mulberry32::new(1);
        let goblin = Some(Kin::of_folk(Folk::Goblin));
        assert!((0..20).any(|_| crate::combat::devout_damage(&devout, goblin, &mut rng) > 0));
        assert_eq!(crate::combat::devout_damage(&devout, Some(Kin::of_folk(Folk::Human)), &mut rng), 0);
        assert_eq!(crate::combat::devout_damage(&devout, Some(UNDEAD), &mut rng), 0);
    }
}
