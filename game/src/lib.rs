//! Forever Below: the Bevy side of the game. Generators live in the plain-Rust crates under
//! `crates/`; this crate draws and plays them.

pub mod ambient;
pub mod audio;
pub mod barter;
pub mod belongings;
pub mod breath;
pub mod camp;
pub mod capture;
pub mod carry;
pub mod character;
pub mod dazzle;
pub mod dice;
pub mod door_view;
pub mod doors;
pub mod combat;
pub mod crest;
pub mod creature_view;
pub mod creatures;
pub mod doll_view;
pub mod eat;
pub mod effects;
pub mod deeds;
pub mod devotion;
pub mod enchanter;
pub mod enchanting;
pub mod exhaustion;
pub mod exposure;
pub mod figure;
pub mod fitting;
pub mod folk;
pub mod folk_combat;
pub mod folk_regard;
pub mod hostility;
pub mod blessing;
pub mod identify;
pub mod kills;
pub mod folk_pose;
pub mod folk_routine;
pub mod footsteps;
pub mod commission;
pub mod forge_view;
pub mod frame_time;
pub mod game_clock;
pub mod inspect;
pub mod interact;
pub mod inventory;
pub mod keybindings;
pub mod keymap;
pub mod kindle;
pub mod lab_chart;
pub mod lab_fight;
pub mod lab_figure;
pub mod lab_rules;
pub mod lab_sheet;
pub mod lab_strike;
pub mod lab_time;
pub mod lab_timeline;
pub mod lab_view;
pub mod launch;
pub mod loading;
pub mod level;
pub mod level_view;
pub mod map;
pub mod map_pairs;
pub mod light;
pub mod menu;
pub mod play;
pub mod player;
pub mod niches;
pub mod props;
pub mod book;
pub mod casting;
pub mod casting_influence;
pub mod quickslots;
pub mod reading;
pub mod record;
pub mod remains;
pub mod save;
pub mod shelf;
pub mod render;
pub mod settings;
pub mod sight;
pub mod simulate;
pub mod soak;
pub mod smoke;
pub mod sparkle;
pub mod time_left;
pub mod staged;
pub mod status;
pub mod swing_practice;
pub mod talk;
pub mod title;
pub mod travel;
pub mod webs;
pub mod rules;
pub mod tuning;
pub mod notes_view;
pub mod scenery_view;
pub mod turntable;
pub mod tweak_panel;
pub mod ui;
pub mod view_model;

#[cfg(test)]
/// Seconds of CPU this thread has used. Timing checks read it rather than the wall clock, so a
/// machine busy with other tests (or anything else) does not fail them.
pub(crate) fn thread_cpu_seconds() -> f64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `t` is a valid timespec for the call to write into.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
    t.tv_sec as f64 + t.tv_nsec as f64 * 1e-9
}
