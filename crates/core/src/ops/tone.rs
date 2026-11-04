//! Tone remaps: how a brightness value is moved onto another, the same way for every effect that reshapes tones
//! (`levels`, `auto_levels`, `curves`). Every map takes a value in 0..1 and gives one in 0..1.

use crate::ops::cast::single;

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

/// A smooth tone curve through control points: the monotone piecewise cubic Hermite interpolant (PCHIP) with the
/// Fritsch-Carlson slopes (Fritsch and Carlson, SIAM J. Numer. Anal. 17, 1980; as in Moler's `pchiptx`).
///
/// It passes exactly through every point and never overshoots: between two points it stays between their outputs, so
/// where the outputs rise the curve rises (a brighter value is never mapped darker) and a bump or dip is never
/// invented. Below the first input and above the last the curve is held at that point's output.
#[derive(Debug, Clone)]
pub(crate) struct Curve {
    inputs: Vec<f64>,
    /// For each stretch between two points, the cubic's coefficients in (x - its first input), highest power first.
    pieces: Vec<[f64; 4]>, // constant: a cubic has four coefficients
}

impl Curve {
    /// The curve through (`inputs[i]`, `outputs[i]`). There are at least three points, `inputs` strictly increase, and
    /// both are as long as each other.
    pub(crate) fn through(inputs: &[f64], outputs: &[f64]) -> Self {
        let widths: Vec<f64> = inputs.windows(2).map(|pair| pair[1] - pair[0]).collect();
        let chords: Vec<f64> = outputs
            .windows(2)
            .zip(&widths)
            .map(|(pair, width)| (pair[1] - pair[0]) / width)
            .collect();
        let last = chords.len() - 1;
        // the slope at each point: zero where the chords change direction or flatten, else their weighted harmonic mean
        let mut slopes = vec![0.0; inputs.len()];
        for point in 1..=last {
            let (before, after) = (chords[point - 1], chords[point]);
            if before * after > 0.0 {
                let weight_before = 2.0 * widths[point] + widths[point - 1];
                let weight_after = widths[point] + 2.0 * widths[point - 1];
                slopes[point] = (weight_before + weight_after) / (weight_before / before + weight_after / after);
            }
        }
        slopes[0] = end_slope(widths[0], widths[1], chords[0], chords[1]);
        slopes[last + 1] = end_slope(widths[last], widths[last - 1], chords[last], chords[last - 1]);
        let pieces = (0..=last)
            .map(|piece| {
                let (width, chord) = (widths[piece], chords[piece]);
                let (start, end) = (slopes[piece], slopes[piece + 1]);
                let twist = (start + end - 2.0 * chord) / width;
                [twist / width, (chord - start) / width - twist, start, outputs[piece]]
            })
            .collect();
        Self {
            inputs: inputs.to_vec(),
            pieces,
        }
    }

    /// The curve's output at `value`.
    pub(crate) fn apply(&self, value: f32) -> f32 {
        let first = self.inputs[0];
        let x = f64::from(value).clamp(first, self.inputs[self.inputs.len() - 1]);
        let piece = (self.inputs.partition_point(|&input| input <= x) - 1).min(self.pieces.len() - 1);
        let [cubic, square, slope, start] = self.pieces[piece];
        let along = x - self.inputs[piece];
        single(((cubic * along + square) * along + slope) * along + start)
    }
}

/// The sign of `number`: -1, 0 or 1.
fn sign(number: f64) -> i8 {
    i8::from(number > 0.0) - i8::from(number < 0.0)
}

const END_SLOPE_CAP: f64 = 3.0; // constant: Fritsch and Butland's cap of an end slope at three chords

/// The slope at an end point from the stretch beside it (`width`, `chord`) and the next one (`next_width`,
/// `next_chord`): the one-sided three-point estimate, set to zero if it points away from the chord and to three times
/// the chord if the chords change direction and it is steeper than that.
fn end_slope(width: f64, next_width: f64, chord: f64, next_chord: f64) -> f64 {
    let estimate = ((2.0 * width + next_width) * chord - width * next_chord) / (width + next_width);
    if sign(estimate) != sign(chord) {
        0.0
    } else if sign(chord) != sign(next_chord) && estimate.abs() > END_SLOPE_CAP * chord.abs() {
        END_SLOPE_CAP * chord
    } else {
        estimate
    }
}
