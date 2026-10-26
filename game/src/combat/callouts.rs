//! What each of the delver's blows did, written for a moment over the head of what it was aimed
//! at: "Miss", "Blocked", "Hit 5", "Critical 9". The same outcomes the log prints
//! (`attack.line()`, a folk's "takes 4"), so a fight reads without the console.
//!
//! - **Where**: just above the target's head as it stood when struck (a folk's head joint, a
//!   creature's highest bone), so a felled body's last blow stays where it fell. Each callout
//!   rises from there and fades; the next one on the same target steps aside a little, so a
//!   quick pair does not print over each other.
//! - **Look**: words in Almendra, numbers in Plex Mono, as the HUD writes them, each with a dark
//!   shadow to read on lit stone. A miss is muted italic, a shield's block steel blue, a hit
//!   cream, a natural 20 gold and larger, popping in harder. The number of a killing blow is red.
//!
//! The timings, sizes and colours are our own.

use bevy::prelude::*;

use crate::creatures::behaviour::body_radius;
use crate::creatures::{Bones, Critter, HitShape};
use crate::folk::Denizen;
use crate::folk_pose::{FolkAnim, FolkJoint};
use crate::play::{DelveSystems, PlayerCamera};
use crate::ui::{BAD, BG, BRIGHT, FRAME_GOLD, INK, MUTED, UiFonts};

pub struct CalloutPlugin;

impl Plugin for CalloutPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Callout>().add_systems(Update, (spawn_callouts, float_callouts).chain().in_set(DelveSystems));
    }
}

/// What a blow at a creature or a folk came to, to write over them.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Callout {
    pub target: Entity,
    pub outcome: Outcome,
}

/// How a blow went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The attack roll fell short of the target's Armor Class.
    Miss,
    /// A shield turned it: no harm.
    Blocked,
    /// It landed for `damage`, as taken (a skeleton's double from a bludgeon included); a
    /// `critical` is a natural 20, and a `fatal` one brought the target to 0.
    Hit { damage: u32, critical: bool, fatal: bool },
    /// It missed but grazed for `damage` ([`fb_character::combat::graze`]); a `fatal` one brought
    /// the target to 0.
    Graze { damage: u32, fatal: bool },
}

impl Outcome {
    /// What `blow` did, having taken `damage` and brought its target to 0 if `fatal`.
    pub fn of(blow: &super::feedback::Struck, damage: u32, fatal: bool) -> Outcome {
        if blow.graze { Outcome::Graze { damage, fatal } } else { Outcome::Hit { damage, critical: blow.critical, fatal } }
    }
}

/// Which of the HUD's faces a run of a callout is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Word,
    Italic,
    Number,
}

/// One run of a callout's text.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub face: Face,
    pub size: f32,
    pub colour: Color,
}

/// How a callout is written and moves: its runs, how long it stays, and how far larger than
/// its size it pops in.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub runs: Vec<Run>,
    pub life: f32,
    pub pop: f32,
}

/// A shield's block: a pale steel blue, a cooler kin of the HUD's cream.
const STEEL: Color = Color::srgb(0.68, 0.77, 0.9);

/// How `outcome` is written.
pub fn look(outcome: Outcome) -> Look {
    let run = |text: &str, face, size, colour| Run { text: text.to_string(), face, size, colour };
    match outcome {
        Outcome::Miss => Look { runs: vec![run("Miss", Face::Italic, 24.0, MUTED)], life: 0.9, pop: 1.0 },
        Outcome::Blocked => Look { runs: vec![run("Blocked", Face::Word, 26.0, STEEL)], life: 1.0, pop: 1.2 },
        Outcome::Graze { damage, fatal } => Look { runs: vec![run("Graze ", Face::Italic, 22.0, MUTED), run(&damage.to_string(), Face::Number, 26.0, if fatal { BAD } else { INK })], life: 1.0, pop: 1.1 },
        Outcome::Hit { damage, critical, fatal } => {
            let number = |colour| run(&damage.to_string(), Face::Number, if critical { 42.0 } else { 32.0 }, if fatal { BAD } else { colour });
            if critical {
                Look { runs: vec![run("Critical ", Face::Word, 26.0, FRAME_GOLD), number(FRAME_GOLD)], life: 1.4, pop: 1.7 }
            } else {
                Look { runs: vec![run("Hit ", Face::Word, 22.0, INK), number(BRIGHT)], life: 1.1, pop: 1.3 }
            }
        }
    }
}

/// How high over its anchor a callout has risen, logical pixels, at the end of its life.
const RISE: f32 = 46.0;
/// How long a callout takes to settle from its pop to its size, seconds.
const POP_TIME: f32 = 0.14;
/// The share of its life a callout holds fully opaque before it fades.
const HOLD: f32 = 0.55;
/// How far up from the top of a head a callout starts, world units (15 cm).
const HEAD_ROOM: f32 = 1.5;
/// How far a folk's head reaches over its head joint, world units: a human's head is about 2.3
/// units tall from the neck.
const HEAD_HEIGHT: f32 = 2.3;
/// How wide a callout's box is, logical pixels; the text is centred in it.
const WIDTH: f32 = 240.0;
/// About how tall a callout's line is, logical pixels: it sits on its anchor, not across it.
const TEXT_HEIGHT: f32 = 40.0;
/// The steps aside, logical pixels, that callouts on the same target take in turn.
const STEPS: [f32; 3] = [0.0, 22.0, -22.0];

/// Where a callout anchored at `at` on a view of `size` (logical pixels) is written: there,
/// but low enough that it can rise its whole way and stay on the screen, and in from the
/// sides. Close up, a head is often above the top of the view, and the callout waits at the top.
pub fn kept_in_view(at: Vec2, size: Vec2) -> Vec2 {
    let top = TEXT_HEIGHT + RISE + EDGE;
    let side = 0.5 * WIDTH;
    Vec2::new(at.x.clamp(side, (size.x - side).max(side)), at.y.clamp(top, (size.y - EDGE).max(top)))
}

/// How far in from the edges of the view a callout stays, logical pixels.
const EDGE: f32 = 24.0;

/// Where a callout `age` seconds old is with `look`'s life and pop: how far it has risen
/// (logical pixels), its scale, and its opacity.
pub fn motion(age: f32, life: f32, pop: f32) -> (f32, f32, f32) {
    let t = (age / life).clamp(0.0, 1.0);
    let rise = RISE * (1.0 - (1.0 - t) * (1.0 - t));
    let settle = (age / POP_TIME).clamp(0.0, 1.0);
    let scale = pop + (1.0 - pop) * (1.0 - (1.0 - settle).powi(3));
    let fade = ((t - HOLD) / (1.0 - HOLD)).clamp(0.0, 1.0);
    (rise, scale, 1.0 - fade * fade)
}

/// A callout on screen: where over the world it hangs, how old it is, and the step aside it took.
#[derive(Component, Clone, Debug)]
pub struct Floating {
    pub anchor: Vec3,
    pub age: f32,
    pub life: f32,
    pub pop: f32,
    pub aside: f32,
    /// Each run's colour at full, to fade from.
    colours: Vec<Color>,
}

/// Where the top of a struck body is: a folk's head, a creature's highest bone sphere
/// (its root and the circle it keeps clear without shaped bones).
#[derive(bevy::ecs::system::SystemParam)]
pub struct Heads<'w, 's> {
    creatures: Query<'w, 's, (&'static Critter, &'static Transform, Option<&'static Bones>)>,
    folk: Query<'w, 's, (&'static Denizen, &'static Transform, Option<&'static Children>)>,
    anims: Query<'w, 's, &'static FolkAnim>,
    joints: Query<'w, 's, &'static GlobalTransform, With<FolkJoint>>,
    shapes: Query<'w, 's, (&'static GlobalTransform, &'static HitShape)>,
}

impl Heads<'_, '_> {
    /// The point just over `target`'s head, world space.
    pub fn above(&self, target: Entity) -> Option<Vec3> {
        if let Ok((denizen, root, children)) = self.folk.get(target) {
            let anim = children.into_iter().flat_map(|c| c.iter()).find_map(|c| self.anims.get(c).ok());
            let head = anim.and_then(|a| self.joints.get(a.joint(fb_body::Joint::Head)).ok()).map(GlobalTransform::translation);
            // Their bounds take in what they hold, a spear standing well over their head.
            return Some(head.map_or_else(|| root.transform_point(Vec3::Y * denizen.max.y), |h| h + Vec3::Y * HEAD_HEIGHT) + Vec3::Y * HEAD_ROOM);
        }
        let (critter, root, bones) = self.creatures.get(target).ok()?;
        let top = bones.iter().flat_map(|b| b.0.iter()).filter_map(|&bone| self.shapes.get(bone).ok()).flat_map(|(at, shape)| shape.spheres(at)).map(|(c, r)| c.y + r).reduce(f32::max);
        let top = top.unwrap_or(root.translation.y + 2.0 * body_radius(critter));
        Some(Vec3::new(root.translation.x, top + HEAD_ROOM, root.translation.z))
    }
}

/// Writes each new callout over its target.
fn spawn_callouts(mut commands: Commands, mut callouts: MessageReader<Callout>, fonts: Option<Res<UiFonts>>, heads: Heads, mut turn: Local<usize>) {
    for callout in callouts.read() {
        let Some(anchor) = heads.above(callout.target) else { continue };
        let look = look(callout.outcome);
        let aside = STEPS[*turn % STEPS.len()];
        *turn += 1;
        let font = |face: Face, size: f32| {
            let handle = fonts.as_deref().map(|f| match face {
                Face::Word => f.display_bold.clone(),
                Face::Italic => f.body_italic.clone(),
                Face::Number => f.mono_bold.clone(),
            });
            handle.map_or_else(|| TextFont::from_font_size(size), |h| TextFont::from_font_size(size).with_font(h))
        };
        let colours = look.runs.iter().map(|r| r.colour).collect();
        let (first, rest) = look.runs.split_first().expect("a callout has words");
        commands
            .spawn((
                Name::new("callout"),
                Floating { anchor, age: 0.0, life: look.life, pop: look.pop, aside, colours },
                Text::new(first.text.clone()),
                font(first.face, first.size),
                TextColor(first.colour),
                TextShadow { offset: Vec2::new(2.0, 2.0), color: BG.with_alpha(0.85) },
                TextLayout::justify(Justify::Center),
                Node { position_type: PositionType::Absolute, width: px(WIDTH), ..default() },
                UiTransform::from_scale(Vec2::splat(look.pop)),
                // Off screen until placed, under every other part of the HUD and the screens.
                Visibility::Hidden,
                GlobalZIndex(-10),
                Pickable::IGNORE,
                DespawnOnExit(crate::launch::AppState::Delving),
            ))
            .with_children(|text| {
                for run in rest {
                    text.spawn((TextSpan::new(run.text.clone()), font(run.face, run.size), TextColor(run.colour)));
                }
            });
    }
}

/// Moves each callout on: pinned over its anchor as the view turns, rising, popping in and
/// fading out, then gone. Hidden while its anchor is behind the eye. The anchor's place in the
/// view is carried into the interface's pixels, which `UiScale` sizes
/// ([`crate::ui::InterfaceScalePlugin`]).
#[allow(clippy::type_complexity)]
fn float_callouts(
    mut commands: Commands,
    time: Res<Time>,
    ui_scale: Option<Res<UiScale>>,
    cameras: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    mut callouts: Query<(Entity, &mut Floating, &mut Node, &mut UiTransform, &mut Visibility, &mut TextColor, &mut TextShadow, Option<&Children>)>,
    mut spans: Query<&mut TextColor, (With<TextSpan>, Without<Floating>)>,
) {
    let camera = cameras.single().ok();
    let per_view = 1.0 / ui_scale.map_or(1.0, |s| s.0.max(f32::EPSILON));
    for (entity, mut floating, mut node, mut transform, mut visibility, mut colour, mut shadow, children) in &mut callouts {
        floating.age += time.delta_secs();
        if floating.age >= floating.life {
            commands.entity(entity).despawn();
            continue;
        }
        let Some((at, view)) = camera.and_then(|(camera, eye)| Some((camera.world_to_viewport(eye, floating.anchor).ok()?, camera.logical_viewport_size()?))) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let at = kept_in_view(at * per_view, view * per_view);
        let (rise, scale, alpha) = motion(floating.age, floating.life, floating.pop);
        node.left = px(at.x - 0.5 * WIDTH + floating.aside);
        node.top = px(at.y - rise - TEXT_HEIGHT);
        transform.scale = Vec2::splat(scale);
        colour.0 = floating.colours[0].with_alpha(alpha);
        shadow.color = BG.with_alpha(0.85 * alpha);
        for (span, base) in children.into_iter().flat_map(|c| c.iter()).zip(floating.colours.iter().skip(1)) {
            if let Ok(mut span) = spans.get_mut(span) {
                span.0 = base.with_alpha(alpha);
            }
        }
        visibility.set_if_neq(Visibility::Inherited);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(outcome: Outcome) -> String {
        look(outcome).runs.iter().map(|r| r.text.as_str()).collect()
    }

    #[test]
    fn each_outcome_reads_as_the_log_says_it() {
        assert_eq!(text(Outcome::Miss), "Miss");
        assert_eq!(text(Outcome::Blocked), "Blocked");
        assert_eq!(text(Outcome::Graze { damage: 1, fatal: false }), "Graze 1");
        assert_eq!(text(Outcome::Hit { damage: 5, critical: false, fatal: false }), "Hit 5");
        assert_eq!(text(Outcome::Hit { damage: 14, critical: true, fatal: false }), "Critical 14");
    }

    #[test]
    fn a_critical_is_bigger_and_stays_longer_and_a_killing_blow_is_red() {
        let hit = look(Outcome::Hit { damage: 5, critical: false, fatal: false });
        let critical = look(Outcome::Hit { damage: 5, critical: true, fatal: false });
        assert!(critical.runs[1].size > hit.runs[1].size);
        assert!(critical.life > hit.life && critical.pop > hit.pop);
        let fatal = look(Outcome::Hit { damage: 5, critical: false, fatal: true });
        assert_eq!(fatal.runs[1].colour, BAD);
        assert_eq!(fatal.runs[1].face, Face::Number);
    }

    #[test]
    fn a_head_above_the_view_is_called_out_at_its_top() {
        let view = Vec2::new(1920.0, 1080.0);
        // On screen, it stays where it is.
        assert_eq!(kept_in_view(Vec2::new(900.0, 400.0), view), Vec2::new(900.0, 400.0));
        // Above the view, held down so the whole rise shows.
        let held = kept_in_view(Vec2::new(1430.0, -77.0), view);
        assert_eq!(held.x, 1430.0);
        assert!(held.y - RISE - TEXT_HEIGHT >= 0.0);
        // Off a side, in far enough for the whole box.
        assert!(kept_in_view(Vec2::new(1910.0, 400.0), view).x + 0.5 * WIDTH <= view.x);
    }

    #[test]
    fn a_callout_pops_in_rises_and_fades() {
        let (rise, scale, alpha) = motion(0.0, 1.0, 1.5);
        assert_eq!((rise, scale, alpha), (0.0, 1.5, 1.0));
        // Settled to its size after the pop, still opaque through the hold.
        let (_, scale, alpha) = motion(POP_TIME, 1.0, 1.5);
        assert!((scale - 1.0).abs() < 1e-5);
        assert_eq!(alpha, 1.0);
        // Risen all the way and faded out at the end.
        let (rise, _, alpha) = motion(1.0, 1.0, 1.5);
        assert!((rise - RISE).abs() < 1e-4);
        assert!(alpha.abs() < 1e-5);
        // Always rising.
        assert!(motion(0.3, 1.0, 1.0).0 < motion(0.6, 1.0, 1.0).0);
    }
}
