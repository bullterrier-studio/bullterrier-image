//! A convolution worked out from its definition, for the effects that are one: every output sample the double sum of the
//! weights and the samples they fall on, with a different way of finding what lies beyond the edge than the core's.

use bullterrier_image::params::EffectParams;
use bullterrier_image::{Edge, Error, Inputs, Picture, apply};
use serde_json::Value;

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

/// Where the sample at `index` of a line of `len` samples comes from, found by folding the index back into the line one
/// bounce at a time; none for black beyond the line. (The core finds it by modular arithmetic.)
fn source(beyond: Beyond, mut index: i64, len: i64) -> Option<usize> {
    let inside = |index: i64| (0..len).contains(&index);
    match beyond {
        Beyond::Black => inside(index).then_some(index),
        Beyond::Rule(Edge::Nearest) => Some(index.clamp(0, len - 1)),
        Beyond::Rule(Edge::Wrap) => {
            while !inside(index) {
                index += if index < 0 { len } else { -len };
            }
            Some(index)
        }
        Beyond::Rule(Edge::Reflect) => {
            while !inside(index) {
                index = if index < 0 { -index - 1 } else { 2 * len - 1 - index };
            }
            Some(index)
        }
        Beyond::Rule(Edge::Mirror) => {
            while len > 1 && !inside(index) {
                index = if index < 0 { -index } else { 2 * (len - 1) - index };
            }
            Some(if len > 1 { index } else { 0 })
        }
    }
    .and_then(|index| usize::try_from(index).ok())
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

/// The first value where `a` and `b` are further apart than `tolerance`, or none; a mismatch is reported by this, not
/// printed (lesson L1: no whole pictures in a failure message).
pub(crate) fn first_difference(a: &[f32], b: &[f32], tolerance: f32) -> Option<usize> {
    (a.len() != b.len())
        .then(|| a.len().min(b.len()))
        .or_else(|| a.iter().zip(b).position(|(a, b)| (a - b).abs() > tolerance))
}
