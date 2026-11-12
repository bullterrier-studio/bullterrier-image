//! Blend modes, seen through a step's `blend_mode` and `opacity` and its `mask`: the pixels a source laid over a base
//! comes to, worked by hand from W3C Compositing and Blending Level 1. The source is an effect of the test's own that
//! ignores the picture and returns fixed values.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bullterrier_image::{
    BlendMode, Category, Effect, Error, Image, Inputs, Layer, Options, Photo, Picture, Presets, Recipe, Step, register,
};

use crate::common::compare::assert_close;

/// A foreign effect that ignores the picture and returns `values` (pixel after pixel, row after row) instead: the source a
/// blend mode is laid over the photo with.
struct Replaces {
    name: String,
    height: usize,
    width: usize,
    values: Vec<f32>,
}

impl Effect for Replaces {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn apply(&self, _picture: &Picture<'_>, _params: &serde_json::Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        Image::new(self.height, self.width, &self.values)
    }
}

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

/// `source` laid over `backdrop` (one row of pixels) by `lay` (a blend mode and an opacity), with `mask` as the map
/// `subject` if given.
fn laid(backdrop: &[f32], source: &[f32], lay: (BlendMode, f64), mask: Option<&[f32]>) -> Result<Vec<f32>, Error> {
    static SOURCES: AtomicUsize = AtomicUsize::new(0);
    let width = backdrop.len() / 3;
    let name = format!("blend_source_{}", SOURCES.fetch_add(1, Ordering::Relaxed));
    register(Arc::new(Replaces {
        name: name.clone(),
        height: 1,
        width,
        values: source.to_vec(),
    }))?;
    let step = Step::new(name).blend_mode(lay.0).opacity(lay.1);
    let mut photo = Photo::new(Picture::new(1, width, backdrop)?);
    if let Some(mask) = mask {
        photo = photo.with_map("subject", Layer::new(1, width, mask.to_vec())?)?;
    }
    let step = if mask.is_some() { step.mask("subject") } else { step };
    let image = photo.render(&Recipe::new([step]), &Presets::new(), Options::default())?;
    Ok(image.into_values())
}

/// A blend mode and an opacity.
const fn source(mode: BlendMode, opacity: f64) -> (BlendMode, f64) {
    (mode, opacity)
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
        assert_close(&out, expected.as_flattened(), 1e-6, mode.name());
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
        1e-6,
        "normal 0.25",
    );
    assert_close(
        &laid(&backdrop, &solid, source(BlendMode::Multiply, 0.5), None)?,
        &[0.16; 3],
        1e-6,
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
    assert_close(&normal, &channels([0.2, 0.6, 0.4, 0.3]), 1e-6, "mask");
    // outside the mask (value 0) the pixel is the backdrop's exactly
    assert_eq!(normal[..3], backdrop[..3]);
    let half = laid(&backdrop, &solid, source(BlendMode::Normal, 0.5), Some(&mask))?;
    assert_close(&half, &channels([0.2, 0.4, 0.3, 0.25]), 1e-6, "mask at 0.5");
    // multiply: B = 0.12; Co = 0.2 - alpha x 0.08
    let multiplied = laid(&backdrop, &solid, source(BlendMode::Multiply, 1.0), Some(&mask))?;
    assert_close(
        &multiplied,
        &channels([0.2, 0.12, 0.16, 0.18]),
        1e-6,
        "mask with multiply",
    );
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
