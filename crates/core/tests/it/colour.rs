//! The colour arithmetic `grade` rests on, seen through `grade` alone: the sRGB transfer function (IEC 61966-2-1) that a
//! colour balance works through, and the Rec. 709 brightness that saturation 0 reduces a colour to.
//!
//! A tint of 1 halves the green gain (`2^-tint`) and leaves red and blue alone, so the green channel comes out as
//! `encode(decode(green) / 2)` and the others as `encode(decode(v))`.

use bullterrier_image::{Error, Inputs, Picture, apply};
use proptest::prelude::*;
use serde_json::json;

/// `grade` with `tint` and `saturation` (the rest at their defaults) applied to the pixels `pixels`.
fn graded(pixels: &[[f32; 3]], tint: f64, saturation: f64) -> Result<Vec<[f32; 3]>, Error> {
    let values: Vec<f32> = pixels.iter().flatten().copied().collect();
    let params = json!({"temperature": 0.0, "tint": tint, "exposure": 0.0, "contrast": 1.0, "pivot": 0.5,
                        "saturation": saturation, "vibrance": 0.0});
    let image = apply(
        "grade",
        &Picture::new(1, pixels.len(), &values)?,
        params,
        &Inputs::default(),
    )?;
    Ok(image
        .into_values()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect())
}

/// (encoded, encoded after halving the linear light), worked by hand in double precision from the IEC 61966-2-1
/// definitions: linear = encoded / 12.92 up to 0.04045, above it ((encoded + 0.055) / 1.055) ** 2.4, and back by
/// 12.92 x linear up to 0.0031308, above it 1.055 x linear ** (1 / 2.4) - 0.055. Mid grey 0.5 is 21.4 % linear light
/// (published: 0.2140), half of it 0.1070.
const HALVED: [(f32, f32); 7] = [
    (0.0, 0.0),           // black
    (0.02, 0.01),         // inside the linear segment: half the value
    (0.04045, 0.020_225), // the top of the linear segment
    (0.2, 0.136_034_15),  // a dark tone, across the break
    (0.5, 0.360_780_2),   // mid grey
    (0.7, 0.510_610_9),   // a light tone
    (1.0, 0.735_357),     // white
];

#[test]
fn a_colour_balance_works_through_the_published_transfer_function() -> Result<(), Error> {
    let inputs: Vec<[f32; 3]> = HALVED.iter().map(|(encoded, _)| [*encoded; 3]).collect();
    let out = graded(&inputs, 1.0, 1.0)?;
    for ((encoded, halved), pixel) in HALVED.iter().zip(out) {
        assert!(
            (pixel[1] - halved).abs() < 1e-6,
            "green {encoded} became {}, not {halved}",
            pixel[1]
        );
        // red and blue have the gain 1: decoded and encoded again they are what they were
        assert!(
            (pixel[0] - encoded).abs() < 1e-6 && (pixel[2] - encoded).abs() < 1e-6,
            "{encoded}"
        );
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn encoding_undoes_decoding_over_the_range(value in 0.0_f32..=1.0) {
        let out = graded(&[[value, value, value]], 1.0, 1.0)?;
        prop_assert!((out[0][0] - value).abs() < 1e-5 && (out[0][2] - value).abs() < 1e-5, "{:?}", out);
    }

    #[test]
    fn halving_the_light_halves_a_value_inside_the_linear_segment(value in 0.0_f32..=0.04045) {
        // both ends of the round trip are on the straight segment, the same slope: half the value
        let out = graded(&[[value, value, value]], 1.0, 1.0)?;
        prop_assert!((out[0][1] - value / 2.0).abs() < 1e-6, "{:?}", out);
    }

    #[test]
    fn both_directions_keep_the_order_of_values(low in 0.0_f32..=1.0, step in 0.0_f32..=1.0) {
        let high = (low + step).min(1.0);
        let out = graded(&[[low, low, low], [high, high, high]], 1.0, 1.0)?;
        prop_assert!(out[0][1] <= out[1][1] && out[0][0] <= out[1][0], "{:?}", out);
    }

    #[test]
    fn a_grey_is_as_bright_as_its_value(value in 0.0_f32..=1.0) {
        let out = graded(&[[value, value, value]], 0.0, 0.0)?;
        prop_assert!(out[0].iter().all(|channel| (channel - value).abs() < 1e-6), "{:?}", out);
    }

    #[test]
    fn raising_any_channel_never_darkens(
        pixel in prop::array::uniform3(0.0_f32..=1.0), channel in 0_usize..3, lift in 0.0_f32..=1.0,
    ) {
        let mut brighter = pixel;
        brighter[channel] = (brighter[channel] + lift).min(1.0);
        let out = graded(&[pixel, brighter], 0.0, 0.0)?;
        prop_assert!(out[1][0] >= out[0][0] - 1e-6, "{:?}", out);
    }
}

#[test]
fn the_primaries_weigh_as_rec_709_publishes() -> Result<(), Error> {
    // saturation 0 gives every channel the brightness of the pixel
    let published = [0.2126, 0.7152, 0.0722];
    let primaries: Vec<[f32; 3]> = (0..3)
        .map(|channel| {
            let mut pixel = [0.0; 3];
            pixel[channel] = 1.0;
            pixel
        })
        .collect();
    for ((channel, weight), pixel) in published.into_iter().enumerate().zip(graded(&primaries, 0.0, 0.0)?) {
        assert!(
            pixel.iter().all(|value| (value - weight).abs() < 1e-6),
            "channel {channel}: {pixel:?}"
        );
    }
    Ok(())
}
