//! Healers: the clerics a faction keeps who tend the delver for coin ([`fb_faction::Profession::Healer`]).
//!
//! This is where a healer's Heal topic is handled. The topic itself ([`Service::Heal`]) is listed
//! by [`crate::talk::lines::topics`] for a healer and by [`crate::talk::lines::parley_topics`] for
//! one who will not talk, and [`crate::talk`] asks [`refusal`] first whenever it is chosen.
//!
//! The healing itself (#635, #872) extends this module: it flips [`crate::talk::service_ready`]
//! for [`Service::Heal`], so a friendly healer's choice is sent on as a `ServiceChosen` to a
//! screen of its own, and [`offer`] stays as what a healer says when they have nothing to tend.
//! Until then a friendly healer only says what they can tend.

use fb_faction::{Band, Profession};

use crate::talk::lines::{self, Service, Speaker};

/// Whether `speaker` is a healer. Only clerics are ([`fb_level::FolkSpot::class`]).
pub fn is_healer(speaker: &Speaker) -> bool {
    speaker.denizen.is_none() && speaker.trade.is_some_and(|t| t.profession == Profession::Healer)
}

/// Why this healer will not tend the delver, if they will not: a Hostile or Wary faction's healer
/// refuses, and so does any healer who has turned on the delver (`parley`, the talk is only a
/// parley). `None` for a healer who will tend, and for anyone who is not a healer.
pub fn refusal(speaker: &Speaker, parley: bool) -> Option<String> {
    if !is_healer(speaker) {
        return None;
    }
    let symbol = match speaker.band {
        Band::Hostile => "heal_refuse_hostile",
        Band::Wary => "heal_refuse_wary",
        _ if parley => "heal_refuse_hostile",
        _ => return None,
    };
    Some(lines::say(speaker, symbol, 0x60 + Service::Heal as u32, &[]))
}

/// What a willing healer says of what they can tend.
pub fn offer(speaker: &Speaker) -> String {
    lines::say(speaker, "heal_offer", 0x61, &[])
}

/// The answer to the Heal topic: the refusal, else the offer.
pub fn answer(speaker: &Speaker, parley: bool) -> String {
    refusal(speaker, parley).unwrap_or_else(|| offer(speaker))
}
