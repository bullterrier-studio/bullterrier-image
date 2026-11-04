//! Colour ramps: a position from 0 to 1 looked up in a list of colours, the same way for every effect that colours by a
//! scalar (the Rust twin of `bullterrier_image.ops.ramp`).

/// The colour at position `t` (0..1) along `stops`, a list of at least one colour.
///
/// The colours sit evenly spaced: with n of them, the k-th at `t = k / (n - 1)`, and between two neighbours the colour is
/// their linear mix. A single colour is returned everywhere. `t = 1` is the last colour.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a stop count is tiny, and a position 0..1 times it is a small non-negative number"
)]
pub(crate) fn at(stops: &[[f32; 3]], t: f32) -> [f32; 3] {
    let last = stops.len() - 1;
    if last == 0 {
        return stops[0];
    }
    let position = t * last as f32;
    let below = (position as usize).min(last - 1);
    let share = position - below as f32;
    let (from, to) = (stops[below], stops[below + 1]);
    [0, 1, 2].map(|channel| from[channel] * (1.0 - share) + to[channel] * share)
}
