//! Light laid over a picture: the one step every glowing effect shares.

/// `light` screened over `base`: `1 - (1 - base) * (1 - light)`, written `base + light * (1 - base)`, per channel.
///
/// `base` is a pixel in 0..1 and `light` the light's brightness per channel. Light is clipped to 0..1 first, so the
/// result never lies below `base` and never above 1: black light leaves `base` exactly as it is, full light gives
/// white, and light adds up without clipping harshly.
pub(crate) fn screen(base: [f32; 3], light: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|channel| base[channel] + light[channel].clamp(0.0, 1.0) * (1.0 - base[channel]))
}
