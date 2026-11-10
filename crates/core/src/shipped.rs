//! The presets and looks that ship with the crate: TOML text by name, from `data/presets/` and `data/looks/`.

include!(concat!(env!("OUT_DIR"), "/shipped.rs"));

/// The preset files that ship with the crate: `(effect, text)`, by effect name. Each text has one `[name]` table per
/// preset (see [`Presets::add_toml`](crate::Presets::add_toml)).
#[must_use]
pub fn presets() -> &'static [(&'static str, &'static str)] {
    PRESETS
}

/// The looks that ship with the crate: `(name, text)`, by name. Each text is a recipe file with a top-level `description`
/// (see [`Recipe::checked_look`](crate::Recipe::checked_look)).
#[must_use]
pub fn looks() -> &'static [(&'static str, &'static str)] {
    LOOKS
}
