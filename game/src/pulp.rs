//! SPIKE: `FB_LOOK=pulp` switches on an 80s pulp fantasy look, painterly shading
//! (`render/toon.wesl`), lighter tinted ink (`render/item.rs`), heroic proportions
//! (`fb_body::Look`) and a warm studio light for the doll view. Read once; unset, nothing
//! changes. Call [`on`] first thing in `main` so bodies are built with the look from the start.

use std::sync::OnceLock;

static ON: OnceLock<bool> = OnceLock::new();

/// Whether `FB_LOOK=pulp` is set for this run.
pub fn on() -> bool {
    *ON.get_or_init(|| {
        let on = std::env::var("FB_LOOK").is_ok_and(|v| v.trim().eq_ignore_ascii_case("pulp"));
        if on {
            fb_body::set_look(fb_body::Look::PULP);
        }
        on
    })
}
