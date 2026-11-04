//! Tone remaps: how a brightness value is moved onto another, the same way for every effect that reshapes tones
//! (`levels`, `auto_levels`, `curves`). Every map takes a value in 0..1 and gives one in 0..1.

/// The smallest span between the input points of a [`Remap`]; keeps equal or crossed points from dividing by zero.
const MIN_SPAN: f32 = 1e-6; // constant: only guards the division; the picture is then cut at the black point (a threshold)

/// Photoshop's Levels for one channel: normalise between the input points, apply the midtone gamma, place between the
/// output points.
///
/// A value at the black point or below goes to the output black, one at the white point or above to the output white,
/// and between them out = output black + ((value - black) / (white - black)) ** (1 / gamma) * (output white - output
/// black). So a gamma above 1 brightens the midtones. Input points that are equal or crossed cut the values at the
/// black point into the two output ends instead of dividing by zero. Points are in 0..1 and the gamma is above 0.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Remap {
    black: f32,
    span: f32,
    exponent: f32,
    output_black: f32,
    reach: f32,
}

impl Remap {
    /// The remap with these points and midtone `gamma`.
    pub(crate) fn new(black: f32, white: f32, gamma: f32, output_black: f32, output_white: f32) -> Self {
        Self {
            black,
            span: (white - black).max(MIN_SPAN),
            exponent: 1.0 / gamma,
            output_black,
            reach: output_white - output_black,
        }
    }

    /// `value` remapped.
    pub(crate) fn apply(&self, value: f32) -> f32 {
        let normalised = ((value - self.black) / self.span).clamp(0.0, 1.0);
        // the clamp only removes rounding error, so the result is in 0..1 exactly
        (self.output_black + normalised.powf(self.exponent) * self.reach).clamp(0.0, 1.0)
    }
}
