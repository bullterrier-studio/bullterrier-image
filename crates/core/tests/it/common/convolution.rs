//! A convolution worked out from its definition, for the effects that are one: every output sample the double sum of the
//! weights and the samples they fall on, with `common::edge`'s way of finding what lies beyond the edge, not the core's.

use bullterrier_image::params::EffectParams;
use bullterrier_image::{Edge, Error, Inputs, Picture, apply};
use serde_json::Value;

use super::edge::folded;

/// What a convolution sees beyond the picture's edge: one of the four rules, or black.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Beyond {
    Rule(Edge),
    Black,
}

/// A weight at (rows below, columns right of) the middle of a kernel.
pub(crate) type Tap = (i64, i64, f64);

/// `params` applied to the `height` x `width` picture `values` (row after row, three channels a pixel).
pub(crate) fn render<P: EffectParams>(
    params: &P,
    height: usize,
    width: usize,
    values: &[f32],
) -> Result<Vec<f32>, Error> {
    let image = apply(
        P::EFFECT,
        &Picture::new(height, width, values)?,
        Value::Object(params.values()),
        &Inputs::default(),
    )?;
    Ok(image.into_values())
}

/// A different value in nearly every channel of every pixel, 0..1.
pub(crate) fn values(height: usize, width: usize) -> Vec<f32> {
    (0..height * width * 3)
        .map(|index| f32::from(u16::try_from((index * 7919 + index / 13) % 1000).unwrap_or(0)) / 1000.0)
        .collect()
}

/// A black `height` x `width` picture with a white pixel at (`row`, `column`).
pub(crate) fn impulse(height: usize, width: usize, row: usize, column: usize) -> Vec<f32> {
    let mut dark = vec![0.0; height * width * 3];
    let at = (row * width + column) * 3;
    dark[at..at + 3].fill(1.0);
    dark
}

/// Where the sample at `index` of a line of `len` samples comes from; none for black beyond the line.
fn source(beyond: Beyond, index: i64, len: i64) -> Option<usize> {
    match beyond {
        Beyond::Black => (0..len).contains(&index).then(|| folded(Edge::Nearest, index, len)),
        Beyond::Rule(rule) => Some(folded(rule, index, len)),
    }
}

/// `values` convolved with `taps` as the definition has it: output[y, x] is the sum of weight x sample[y - down, x -
/// right], the sample being black or what the edge rule folds it to beyond the picture.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "small picture sizes, and the f64 sums narrowed to the f32 picture"
)]
pub(crate) fn convolved(taps: &[Tap], beyond: Beyond, height: usize, width: usize, values: &[f32]) -> Vec<f32> {
    let (h, w) = (height as i64, width as i64);
    let mut out = vec![0.0_f32; values.len()];
    for y in 0..h {
        for x in 0..w {
            for channel in 0..3 {
                let mut sum = 0.0;
                for &(down, right, weight) in taps {
                    let at = source(beyond, y - down, h).zip(source(beyond, x - right, w));
                    sum += at.map_or(0.0, |(row, column)| {
                        weight * f64::from(values[(row * width + column) * 3 + channel])
                    });
                }
                out[((y * w + x) * 3) as usize + channel] = sum as f32;
            }
        }
    }
    out
}
