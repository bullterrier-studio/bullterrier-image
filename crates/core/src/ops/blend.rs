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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::testing::{Replaces, registry};
    use crate::{Error, Options, Photo, Presets, Recipe, Step};

    /// Seven pixels of backdrop and source, chosen so that every branch of every mode is taken: the backdrop's channels
    /// lie on both sides of 0.25 (0.2, 0.24 and 0.28) and of 0.5, the source's on both sides of 0.5, pairs 3 to 5 make
    /// the `color` mode clip below 0, above 1, and not at all, and the last is black under grey, where the moved colour
    /// lands on 0 within a rounding (it must stay black, not become NaN).
    const BACKDROP: [[f32; 3]; 7] = [
        [0.2, 0.5, 0.8],
        [0.2, 0.5, 0.8],
        [0.1, 0.1, 0.1],
        [0.2, 0.5, 0.8],
        [0.9, 0.9, 0.9],
        [0.28, 0.24, 0.28],
        [0.0, 0.0, 0.0],
    ];
    const SOURCE: [[f32; 3]; 7] = [
        [0.3, 0.6, 0.9],
        [0.6, 0.3, 0.3],
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.6, 0.6, 0.6],
        [0.7, 0.7, 0.7],
    ];

    /// `source` laid over `backdrop` (one row of pixels) by `step`, with `masks` as the map `subject` if given.
    fn laid(backdrop: &[f32], source: &[f32], step: Step, mask: Option<&[f32]>) -> Result<Vec<f32>, Error> {
        let width = backdrop.len() / 3;
        let registry = registry()?;
        registry.register(Arc::new(Replaces {
            name: "source",
            height: 1,
            width,
            values: source.to_vec(),
        }))?;
        let mut photo = Photo::new(Picture::new(1, width, backdrop)?);
        if let Some(mask) = mask {
            photo = photo.with_map("subject", Layer::new(1, width, mask.to_vec())?)?;
        }
        let step = if mask.is_some() { step.mask("subject") } else { step };
        let image = photo.render_in(&registry, &Recipe::new([step]), &Presets::new(), Options::default())?;
        Ok(image.into_values())
    }

    /// A step of the source effect with a blend mode and an opacity.
    fn source(mode: BlendMode, opacity: f64) -> Step {
        Step::new("source").blend_mode(mode).opacity(opacity)
    }

    fn assert_close(actual: &[f32], expected: &[f32], said: &str) {
        assert_eq!(actual.len(), expected.len(), "{said}");
        for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
            assert!((a - e).abs() < 1e-6, "{said}: value {index} is {a}, not {e}");
        }
    }

    #[test]
    fn each_mode_gives_the_value_the_w3c_formula_gives_for_known_pixels() -> Result<(), Error> {
        // Worked by hand from W3C Compositing and Blending Level 1 (separable modes, section 10.1; non-separable, 10.2),
        // with Cb the backdrop and Cs the source of the first pixel, channel by channel (0.2 / 0.3, 0.5 / 0.6, 0.8 / 0.9):
        //   multiply  Cb x Cs                        0.06  0.3   0.72
        //   screen    Cb + Cs - Cb x Cs              0.44  0.8   0.98
        //   overlay   HardLight(Cs, Cb): Cb <= 0.5 -> multiply(Cs, 2 Cb) = 0.3 x 0.4 = 0.12; Cb > 0.5 -> screen(Cs, 2 Cb - 1)
        //             0.12, then 0.5 <= 0.5 -> 0.6 x 1.0 = 0.6, then 0.8 -> screen(0.9, 0.6) = 0.9 + 0.6 - 0.54 = 0.96
        //   soft light Cs <= 0.5: Cb - (1 - 2 Cs) Cb (1 - Cb) = 0.2 - 0.4 x 0.2 x 0.8 = 0.136
        //             Cs > 0.5: Cb + (2 Cs - 1)(D(Cb) - Cb), D = ((16 x - 12) x + 4) x if x <= 0.25 else sqrt(x):
        //             0.5 + 0.2 x (sqrt(0.5) - 0.5) = 0.5414214; second pixel channel 0, Cs = 0.6, Cb = 0.2:
        //             D(0.2) = 0.448, 0.2 + 0.2 x (0.448 - 0.2) = 0.2496
        //   difference |Cb - Cs|                     0.1   0.1   0.1
        //   color     SetLum(Cs, Lum(Cb)) with Lum = 0.3 R + 0.59 G + 0.11 B and ClipColor; the pixel (0.1 x3) with
        //             source (1, 0, 0): Lum(Cb) = 0.1, Lum(Cs) = 0.3, moved by -0.2 to (0.8, -0.2, -0.2), min -0.2 < 0:
        //             C = 0.1 + (C - 0.1) x 0.1 / (0.1 + 0.2) -> (0.3333333, 0, 0)
        let rows: [(BlendMode, [[f32; 3]; 7]); 7] = [
            (BlendMode::Normal, SOURCE),
            (
                BlendMode::Multiply,
                [
                    [0.06, 0.3, 0.72],
                    [0.12, 0.15, 0.24],
                    [0.1, 0.0, 0.0],
                    [0.2, 0.0, 0.0],
                    [0.0, 0.0, 0.9],
                    [0.168, 0.144, 0.168],
                    [0.0, 0.0, 0.0],
                ],
            ),
            (
                BlendMode::Screen,
                [
                    [0.44, 0.8, 0.98],
                    [0.68, 0.65, 0.86],
                    [1.0, 0.1, 0.1],
                    [1.0, 0.5, 0.8],
                    [0.9, 0.9, 1.0],
                    [0.712, 0.696, 0.712],
                    [0.7, 0.7, 0.7],
                ],
            ),
            (
                BlendMode::Overlay,
                [
                    [0.12, 0.6, 0.96],
                    [0.24, 0.3, 0.72],
                    [0.2, 0.0, 0.0],
                    [0.4, 0.0, 0.6],
                    [0.8, 0.8, 1.0],
                    [0.336, 0.288, 0.336],
                    [0.0, 0.0, 0.0],
                ],
            ),
            (
                BlendMode::SoftLight,
                [
                    [0.136, 0.541_421_4, 0.875_541_8],
                    [0.2496, 0.4, 0.736],
                    [0.296, 0.01, 0.01],
                    [0.448, 0.25, 0.64],
                    [0.81, 0.81, 0.948_683_3],
                    [0.329_830_1, 0.289_996_8, 0.329_830_1],
                    [0.0, 0.0, 0.0],
                ],
            ),
            (
                BlendMode::Difference,
                [
                    [0.1, 0.1, 0.1],
                    [0.4, 0.2, 0.5],
                    [0.9, 0.1, 0.1],
                    [0.8, 0.5, 0.8],
                    [0.9, 0.9, 0.1],
                    [0.32, 0.36, 0.32],
                    [0.7, 0.7, 0.7],
                ],
            ),
            (
                BlendMode::Color,
                [
                    [0.2, 0.5, 0.8],
                    [0.653, 0.353, 0.353],
                    [0.333_333_3, 0.0, 0.0],
                    [1.0, 0.204_285_7, 0.204_285_7],
                    [0.887_640_4, 0.887_640_4, 1.0],
                    [0.2564, 0.2564, 0.2564],
                    [0.0, 0.0, 0.0],
                ],
            ),
        ];
        // a new mode needs a row
        assert_eq!(rows.map(|(mode, _)| mode), BlendMode::ALL);
        for (mode, expected) in rows {
            let out = laid(BACKDROP.as_flattened(), SOURCE.as_flattened(), source(mode, 1.0), None)?;
            assert_close(&out, expected.as_flattened(), mode.name());
        }
        Ok(())
    }

    #[test]
    fn opacity_mixes_the_blend_in_from_nothing_at_0_to_all_of_it_at_1() -> Result<(), Error> {
        let (backdrop, solid) = ([0.2_f32; 3], [0.6_f32; 3]);
        for mode in BlendMode::ALL {
            let none = laid(&backdrop, &solid, source(*mode, 0.0), None)?;
            assert_eq!(none, backdrop, "{} at 0", mode.name());
        }
        let full = laid(&backdrop, &solid, source(BlendMode::Normal, 1.0), None)?;
        assert_eq!(full, solid);
        // Co = Cb + alpha (B(Cb, Cs) - Cb): normal at 0.25: 0.2 + 0.25 x (0.6 - 0.2) = 0.3; multiply at 0.5: B = 0.12,
        // 0.2 + 0.5 x (0.12 - 0.2) = 0.16
        assert_close(
            &laid(&backdrop, &solid, source(BlendMode::Normal, 0.25), None)?,
            &[0.3; 3],
            "normal 0.25",
        );
        assert_close(
            &laid(&backdrop, &solid, source(BlendMode::Multiply, 0.5), None)?,
            &[0.16; 3],
            "multiply 0.5",
        );
        Ok(())
    }

    #[test]
    fn a_mask_limits_the_step_to_where_it_is_above_0_and_weights_it_by_its_value() -> Result<(), Error> {
        let (backdrop, solid) = ([0.2_f32; 12], [0.6_f32; 12]);
        let mask = [0.0, 1.0, 0.5, 0.25];
        // alpha = opacity x mask; Co = 0.2 + alpha x 0.4
        let normal = laid(&backdrop, &solid, source(BlendMode::Normal, 1.0), Some(&mask))?;
        let channels = |pixels: [f32; 4]| pixels.iter().flat_map(|pixel| [*pixel; 3]).collect::<Vec<f32>>();
        assert_close(&normal, &channels([0.2, 0.6, 0.4, 0.3]), "mask");
        // outside the mask (value 0) the pixel is the backdrop's exactly
        assert_eq!(normal[..3], backdrop[..3]);
        let half = laid(&backdrop, &solid, source(BlendMode::Normal, 0.5), Some(&mask))?;
        assert_close(&half, &channels([0.2, 0.4, 0.3, 0.25]), "mask at 0.5");
        // multiply: B = 0.12; Co = 0.2 - alpha x 0.08
        let multiplied = laid(&backdrop, &solid, source(BlendMode::Multiply, 1.0), Some(&mask))?;
        assert_close(&multiplied, &channels([0.2, 0.12, 0.16, 0.18]), "mask with multiply");
        Ok(())
    }

    #[test]
    fn whatever_the_mode_opacity_and_pixels_the_result_stays_within_0_and_1() -> Result<(), Error> {
        // every backdrop and every source whose channels are 0, 0.25, 0.5, 0.75 or 1: the ends of every branch
        let levels = [0.0_f32, 0.1, 0.25, 0.3, 0.5, 0.7, 0.75, 0.9, 1.0];
        let mut colours = Vec::new();
        for red in levels {
            for green in levels {
                for blue in levels {
                    colours.push([red, green, blue]);
                }
            }
        }
        let mut pairs = Vec::new();
        for backdrop in &colours {
            for source in &colours {
                pairs.push((*backdrop, *source));
            }
        }
        let backdrop: Vec<f32> = pairs.iter().flat_map(|(backdrop, _)| *backdrop).collect();
        let source_values: Vec<f32> = pairs.iter().flat_map(|(_, source)| *source).collect();
        for mode in BlendMode::ALL {
            for opacity in [0.3, 1.0] {
                let out = laid(&backdrop, &source_values, source(*mode, opacity), None)?;
                let bad = out.iter().position(|value| !(0.0..=1.0).contains(value));
                assert!(
                    bad.is_none(),
                    "{} at {opacity}: {:?} of {:?} over {:?}",
                    mode.name(),
                    bad.map(|i| out[i]),
                    bad.map(|i| &backdrop[i / 3 * 3..i / 3 * 3 + 3]),
                    bad.map(|i| &source_values[i / 3 * 3..i / 3 * 3 + 3])
                );
            }
        }
        Ok(())
    }
}
