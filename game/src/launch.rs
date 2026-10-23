//! How the game opens. A plain `cargo run -p forever_below` opens on the title screen
//! ([`crate::title`]); `--seed N`, or any debug switch that looks at the level
//! ([`LEVEL_SWITCHES`]), opens straight into the delve, so captures keep working as they did.
//! `--title` opens on the title whatever else is given, and New delve then plays `--seed`'s
//! level if one was given.

use bevy::prelude::*;

/// Where the game is: on the title screen, or delving.
#[derive(States, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Title,
    Delving,
}

/// The debug switches that open the game straight into the level: each shows or holds
/// something in it (README, "Debug switches").
pub const LEVEL_SWITCHES: [&str; 13] =
    ["FB_DEBUG_POSE", "FB_DEBUG_CAMERA", "FB_DEBUG_HOVER", "FB_DEBUG_KIT", "FB_DROP", "FB_OPEN_INVENTORY", "FB_OPEN_CONTAINER", "FB_OPEN_MENU", "FB_OPEN_MAP", "FB_COMMISSION", "FB_READ", "FB_TEACH", "FB_TEACH_READ"];

/// What the command line asks of a run of the level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    /// `--seed N` or `--seed=N`: the level to play.
    pub seed: Option<u32>,
    /// `--title`: open on the title screen even with a seed or a debug switch.
    pub title: bool,
}

impl Launch {
    /// The level a run plays when nothing names one.
    pub const DEFAULT_SEED: u32 = 1207;

    /// Reads `--seed N`, `--seed=N` and `--title`, in any order.
    pub fn parse(args: &[String]) -> Result<Launch, String> {
        let mut launch = Launch::default();
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            let value = match arg.as_str() {
                "--title" => {
                    launch.title = true;
                    continue;
                }
                "--seed" => args.next().ok_or("--seed needs a number")?.as_str(),
                other => other.strip_prefix("--seed=").ok_or_else(|| format!("unknown argument {other}"))?,
            };
            launch.seed = Some(value.parse().map_err(|_| format!("--seed {value} is not a number"))?);
        }
        Ok(launch)
    }

    /// The state the game opens in. `switch_set` says whether an environment variable is set.
    pub fn opening(&self, switch_set: impl Fn(&str) -> bool) -> AppState {
        let straight_in = self.seed.is_some() || LEVEL_SWITCHES.iter().any(|name| switch_set(name));
        if straight_in && !self.title { AppState::Delving } else { AppState::Title }
    }
}

/// A seed from the clock, for what each run draws afresh: the music's order, a new delve's
/// level. Whatever is generated from it is still a pure function of the seed it got.
pub fn clock_seed() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() ^ d.as_secs() as u32)
}

/// Whether the environment variable `name` is set to anything but nothing.
pub fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    fn none(_: &str) -> bool {
        false
    }

    #[test]
    fn a_plain_run_opens_on_the_title() {
        let launch = Launch::parse(&[]).unwrap();
        assert_eq!(launch, Launch::default());
        assert_eq!(launch.opening(none), AppState::Title);
    }

    #[test]
    fn a_seed_opens_straight_into_the_level() {
        for line in ["--seed 42", "--seed=42"] {
            let launch = Launch::parse(&args(line)).unwrap();
            assert_eq!(launch.seed, Some(42));
            assert_eq!(launch.opening(none), AppState::Delving, "{line}");
        }
    }

    #[test]
    fn debug_switches_skip_the_title() {
        for switch in LEVEL_SWITCHES {
            assert_eq!(Launch::default().opening(|name| name == switch), AppState::Delving, "{switch}");
        }
        // Switches that only change how things look or sound leave the title in place.
        for other in ["FB_SCREENSHOT", "FB_SETTINGS", "FB_FOLK"] {
            assert_eq!(Launch::default().opening(|name| name == other), AppState::Title, "{other}");
        }
    }

    #[test]
    fn title_wins_over_a_seed_and_switches() {
        let launch = Launch::parse(&args("--seed 7 --title")).unwrap();
        assert_eq!(launch, Launch { seed: Some(7), title: true });
        assert_eq!(launch.opening(|name| name == "FB_OPEN_MENU"), AppState::Title);
    }

    #[test]
    fn bad_arguments_are_refused() {
        assert!(Launch::parse(&args("--seed")).is_err());
        assert!(Launch::parse(&args("--seed twelve")).is_err());
        assert!(Launch::parse(&args("--fly")).is_err());
    }
}
