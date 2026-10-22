//! SPIKE: `FB_LOOK=pulp` switches on an 80s pulp fantasy look: the inked toon kept, with warm
//! light and violet shadow, a cool rim and shiny metal (`render/toon.wesl`), tinted thin ink
//! (`render/item.rs`), heroic proportions, anatomy, faces and standing poses (`fb_body`), and a
//! dramatic doll light. `FB_LOOK=paint` is the earlier painterly variant (soft bands, brush
//! noise). Read once; unset, nothing changes. Call [`on`] first thing in `main` so bodies are
//! built with the look from the start.

use std::sync::OnceLock;

static MODE: OnceLock<u32> = OnceLock::new();

/// 0 off, 1 `pulp`, 2 `paint`.
pub fn mode() -> u32 {
    *MODE.get_or_init(|| {
        let v = std::env::var("FB_LOOK").unwrap_or_default();
        let mode = if v.trim().eq_ignore_ascii_case("pulp") {
            1
        } else if v.trim().eq_ignore_ascii_case("paint") {
            2
        } else {
            0
        };
        if mode != 0 {
            fb_body::set_look(fb_body::Look::PULP);
        }
        mode
    })
}

/// Whether either spike look is on.
pub fn on() -> bool {
    mode() != 0
}
