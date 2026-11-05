//! Light laid over a picture: the steps every glowing effect shares, picking out the highlights and screening light over.

/// The part of each channel of `pixel` above `threshold`, rescaled so that white is 1: `(value - threshold) / (1 -
/// threshold)`.
///
/// `pixel` is in 0..1 and every channel is judged on its own, so coloured light keeps its colour. The result lies in
/// 0..1, is 0 wherever a channel is at or below `threshold` and rises linearly to 1 at white. `threshold` is in 0..1 and
/// below 1 (a threshold of 1 has nothing above it).
pub(crate) fn highlights(pixel: [f32; 3], threshold: f32) -> [f32; 3] {
    pixel.map(|value| ((value - threshold) / (1.0 - threshold)).clamp(0.0, 1.0))
}

/// `light` screened over `base`: `1 - (1 - base) * (1 - light)`, written `base + light * (1 - base)`, per channel.
///
/// `base` is a pixel in 0..1 and `light` the light's brightness per channel. Light is clipped to 0..1 first, so the
/// result never lies below `base` and never above 1: black light leaves `base` exactly as it is, full light gives
/// white, and light adds up without clipping harshly.
pub(crate) fn screen(base: [f32; 3], light: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|channel| base[channel] + light[channel].clamp(0.0, 1.0) * (1.0 - base[channel]))
}
