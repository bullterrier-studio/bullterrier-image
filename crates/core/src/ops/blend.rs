//! Blend modes: how a source laid over a base combines with it, as W3C Compositing and Blending Level 1 defines them.
//!
//! Every function works on one pixel, `[red, green, blue]` in 0..1. A mode's formula `B(Cb, Cs)` takes the base `Cb` and
//! the source `Cs`; the result is `Co = Cb + alpha (B(Cb, Cs) - Cb)` (section 5.1 for an opaque base), clipped to 0..1.

use super::par;
use crate::{BlendMode, Image, Layer, Picture};

/// `source` laid over `base` (both of one size) by `mode`, mixed in by `opacity` (0..1) times, where there is a `mask`,
/// the mask's value at the pixel: a new image in 0..1. Opacity 0, or a mask of 0, leaves the base exactly; opacity 1
/// and mode normal leaves the source exactly.
pub(crate) fn lay(
    base: &Picture<'_>,
    source: &Picture<'_>,
    mode: BlendMode,
    opacity: f64,
    mask: Option<&Layer>,
) -> Image {
    #[expect(clippy::cast_possible_truncation, reason = "an opacity is from 0 to 1")]
    let opacity = opacity as f32;
    let blend = |base: [f32; 3], source: [f32; 3], alpha: f32| {
        let blended = blended(mode, base, source);
        // Cb (1 - alpha) + B alpha is Cb + alpha (B - Cb), exact at alpha 0 and 1
        std::array::from_fn(|channel| (base[channel] * (1.0 - alpha) + blended[channel] * alpha).clamp(0.0, 1.0))
    };
    let width = base.width();
    let mut pixels = vec![[0.0; 3]; base.pixels().len()];
    par::rows(&mut pixels, width, |first_row, band| {
        let first = first_row * width;
        let pairs = band
            .iter_mut()
            .zip(&base.pixels()[first..])
            .zip(&source.pixels()[first..]);
        match mask {
            Some(mask) => {
                for (((out, &base), &source), &weight) in pairs.zip(&mask.values()[first..]) {
                    *out = blend(base, source, opacity * weight);
                }
            }
            None => {
                for ((out, &base), &source) in pairs {
                    *out = blend(base, source, opacity);
                }
            }
        }
    });
    Image::from_pixels(base.height(), width, pixels)
}

/// `B(Cb, Cs)` of `mode` for one pixel.
fn blended(mode: BlendMode, base: [f32; 3], source: [f32; 3]) -> [f32; 3] {
    let separable = |blend: fn(f32, f32) -> f32| std::array::from_fn(|channel| blend(base[channel], source[channel]));
    match mode {
        BlendMode::Normal => source,
        BlendMode::Multiply => separable(multiply),
        BlendMode::Screen => separable(screen),
        BlendMode::Overlay => separable(overlay),
        BlendMode::SoftLight => separable(soft_light),
        BlendMode::Difference => separable(|base, source| (base - source).abs()),
        BlendMode::Color => set_lum(source, lum(base)),
    }
}

/// Multiply: `Cb x Cs`.
fn multiply(base: f32, source: f32) -> f32 {
    base * source
}

/// Screen: `Cb + Cs - Cb x Cs`.
fn screen(base: f32, source: f32) -> f32 {
    base + source - base * source
}

/// Overlay: `HardLight(Cs, Cb)`, hard light with the two swapped: multiply below a base of 0.5, screen above.
fn overlay(base: f32, source: f32) -> f32 {
    if base <= 0.5 {
        multiply(source, 2.0 * base)
    } else {
        screen(source, 2.0 * base - 1.0)
    }
}

/// Soft light: a source below 0.5 darkens the base, one above lightens it, by `D(Cb)` (section 10.1, soft light).
fn soft_light(base: f32, source: f32) -> f32 {
    if source <= 0.5 {
        base - (1.0 - 2.0 * source) * base * (1.0 - base)
    } else {
        base + (2.0 * source - 1.0) * (soft_light_curve(base) - base)
    }
}

/// `D(x)` of soft light: a cubic up to 0.25, the square root above it.
fn soft_light_curve(base: f32) -> f32 {
    // constant: the W3C soft light formula switches from its cubic to the square root at 0.25
    const CUBIC_UP_TO: f32 = 0.25;
    if base <= CUBIC_UP_TO {
        // constant: the coefficients of the W3C soft light cubic, D(x) = ((16 x - 12) x + 4) x
        ((16.0 * base - 12.0) * base + 4.0) * base
    } else {
        base.sqrt()
    }
}

/// `Lum(C)`, the luminosity of section 10.2 (not Rec. 709's weights).
fn lum(colour: [f32; 3]) -> f32 {
    // constant: the W3C `Lum` weights of red, green and blue
    const WEIGHTS: [f32; 3] = [0.3, 0.59, 0.11];
    colour[0] * WEIGHTS[0] + colour[1] * WEIGHTS[1] + colour[2] * WEIGHTS[2]
}

/// `SetLum(C, l)`: `C` moved to luminosity `l`, then `ClipColor` into the gamut (section 10.2).
fn set_lum(colour: [f32; 3], luminosity: f32) -> [f32; 3] {
    let shift = luminosity - lum(colour);
    clip_color(colour.map(|channel| channel + shift), luminosity)
}

/// `ClipColor(C)` for a colour of luminosity `luminosity`: a colour with a channel below 0 or above 1 is pulled toward
/// that luminosity until it fits. The spec computes `Lum(C)` again here; it equals `luminosity` by construction, but in
/// floating point it can differ by a rounding, and then a black backdrop under a grey source divides 0 by 0.
fn clip_color(colour: [f32; 3], luminosity: f32) -> [f32; 3] {
    let low = colour[0].min(colour[1]).min(colour[2]);
    let high = colour[0].max(colour[1]).max(colour[2]);
    let mut clipped = colour;
    if low < 0.0 {
        clipped = clipped.map(|channel| luminosity + (channel - luminosity) * luminosity / (luminosity - low));
    }
    if high > 1.0 {
        clipped = clipped.map(|channel| luminosity + (channel - luminosity) * (1.0 - luminosity) / (high - luminosity));
    }
    clipped
}
