//! Sharing a position between evenly spaced levels: the weights that mix runs made at a few values into one picture that
//! varies pixel by pixel.

/// The share of level `level` at `position`, which counts levels: 2.25 lies a quarter of the way from level 2 to level 3.
/// The weight is 1 on the level, falls linearly to 0 at the neighbouring levels and is 0 beyond them (a tent), so the
/// weights of all levels add up to 1 wherever `position` lies within the levels.
pub(crate) fn weight(position: f32, level: u8) -> f32 {
    (1.0 - (position - f32::from(level)).abs()).clamp(0.0, 1.0)
}
