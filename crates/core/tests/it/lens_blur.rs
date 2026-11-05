//! `lens_blur`: the picture compared with a convolution by the disc, and with the weighted average, worked out from their
//! definitions, over every way the core applies a kernel, and the properties a mean keeps.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "small picture sizes and kernel reaches"
)]

use bullterrier_image::effects::lens_blur::Params;
use bullterrier_image::{Edge, Error};
use proptest::prelude::*;

use crate::common::compare::first_difference_within;
use crate::common::convolution::{Beyond, Tap, convolved};
use crate::common::pictures::{impulse, varied};
use crate::common::pixels::run_picture;

/// The pictures are compared at this distance: sums in f32 or by the transform, and the reference in f64.
const TOLERANCE: f32 = 1e-4;

const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];

/// The weights of the disc of `radius` px: a pixel weighs the share of it inside the disc, a linear ramp one pixel wide
/// across the rim; scaled to sum to 1.
fn disc(radius: f64) -> Vec<Tap> {
    let reach = radius.ceil() as i64;
    let taps: Vec<Tap> = (-reach..=reach)
        .flat_map(|down| (-reach..=reach).map(move |right| (down, right)))
        .map(|(down, right)| {
            let distance = ((down * down + right * right) as f64).sqrt();
            (down, right, (radius + 0.5 - distance).clamp(0.0, 1.0))
        })
        .collect();
    let total: f64 = taps.iter().map(|tap| tap.2).sum();
    taps.into_iter()
        .map(|(down, right, weight)| (down, right, weight / total))
        .collect()
}

fn blur(
    radius: f32,
    highlights: f32,
    edge: Edge,
    height: usize,
    width: usize,
    picture: &[f32],
) -> Result<Vec<f32>, Error> {
    run_picture(
        &Params {
            radius,
            highlights,
            edge,
        },
        height,
        width,
        picture,
    )
}

/// The picture blurred over the disc with each pixel counting 1 + `highlights` x its brightness times: the sum of the
/// weighted pixels over the sum of the weights, each a convolution with the disc.
fn favouring_bright(
    taps: &[Tap],
    highlights: f32,
    edge: Edge,
    height: usize,
    width: usize,
    picture: &[f32],
) -> Vec<f32> {
    let pixels = picture.as_chunks::<3>().0;
    let weight = |pixel: &[f32; 3]| 1.0 + highlights * (0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2]); // Rec. 709
    let weights: Vec<f32> = pixels.iter().map(weight).collect();
    let weighted: Vec<f32> = pixels
        .iter()
        .zip(&weights)
        .flat_map(|(pixel, w)| pixel.iter().map(move |value| value * w))
        .collect();
    let flat: Vec<f32> = weights.iter().flat_map(|w| [*w; 3]).collect();
    let (above, below) = (
        convolved(taps, Beyond::Rule(edge), height, width, &weighted),
        convolved(taps, Beyond::Rule(edge), height, width, &flat),
    );
    above.iter().zip(&below).map(|(above, below)| above / below).collect()
}

#[test]
fn the_blur_is_the_picture_averaged_over_the_disc_whatever_lies_beyond_the_edge() -> Result<(), Error> {
    // Discs of 1.5, 2, 2.6, 5 and 5.7 px are summed tap by tap (9, 21, 29, 97 and 121 weights above 0), those of 6, 9.5 and
    // 13 px (137, 305 and 577) are filtered by the Fourier transform; a radius with a fraction over a half reaches a pixel
    // further than the radius rounded down. The pictures are from one pixel to a few, smaller and larger than the discs.
    let shapes = [(1, 1), (1, 9), (9, 1), (5, 7), (23, 31)];
    for radius in [1.5, 2.0, 2.6, 5.0, 5.7, 6.0, 9.5, 13.0] {
        let taps = disc(f64::from(radius));
        for edge in EDGES {
            for (height, width) in shapes {
                let picture = varied(height, width);
                let expected = convolved(&taps, Beyond::Rule(edge), height, width, &picture);
                let at =
                    first_difference_within(&blur(radius, 0.0, edge, height, width, &picture)?, &expected, TOLERANCE);
                assert_eq!(
                    at, None,
                    "radius {radius}, {edge:?}, {height} x {width}: first differs at {at:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn with_highlights_each_pixel_counts_by_its_brightness_in_the_average() -> Result<(), Error> {
    for radius in [1.5, 5.7, 6.0, 9.5] {
        let taps = disc(f64::from(radius));
        for highlights in [0.7, 6.0, 50.0] {
            for edge in EDGES {
                for (height, width) in [(1, 1), (1, 9), (9, 1), (5, 7), (23, 31)] {
                    let picture = varied(height, width);
                    let expected = favouring_bright(&taps, highlights, edge, height, width, &picture);
                    let at = first_difference_within(
                        &blur(radius, highlights, edge, height, width, &picture)?,
                        &expected,
                        TOLERANCE,
                    );
                    assert_eq!(
                        at, None,
                        "radius {radius}, highlights {highlights}, {edge:?}, {height} x {width}: first differs at {at:?}"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn the_comparisons_tell_a_blur_that_differs_apart() -> Result<(), Error> {
    // the known-bad rows: another radius, another edge rule, and no highlights are not the blur with them
    let picture = varied(23, 31);
    let ours = blur(5.0, 6.0, Edge::Reflect, 23, 31, &picture)?;
    let favoured = |radius, highlights, edge| favouring_bright(&disc(radius), highlights, edge, 23, 31, &picture);
    for other in [
        favoured(4.0, 6.0, Edge::Reflect),
        favoured(5.0, 6.0, Edge::Wrap),
        favoured(5.0, 0.0, Edge::Reflect),
    ] {
        assert!(first_difference_within(&ours, &other, TOLERANCE).is_some());
    }
    Ok(())
}

#[test]
fn a_point_of_light_becomes_a_disc_whose_rim_pixels_weigh_their_share() -> Result<(), Error> {
    // by hand: a pixel wholly inside the disc has the weight of the middle one, one at the radius half of it, one half a
    // pixel beyond the radius none; the weights sum to 1 (a white point keeps all its light)
    for radius in [3.0, 6.0, 10.0] {
        let (size, middle) = (41, 20);
        let out = blur(
            radius,
            0.0,
            Edge::Nearest,
            size,
            size,
            &impulse(size, size, middle, middle),
        )?;
        let red = |down: usize, right: usize| f64::from(out[((middle + down) * size + middle + right) * 3]);
        let centre = red(0, 0);
        let radius_steps = radius as usize;
        assert!(
            (red(0, radius_steps) / centre - 0.5).abs() < 1e-5,
            "radius {radius}: on the rim"
        );
        assert!(
            (red(0, radius_steps - 1) / centre - 1.0).abs() < 1e-5,
            "radius {radius}: just inside"
        );
        assert!(red(0, radius_steps + 1) < 1e-6, "radius {radius}: beyond");
        assert!(
            (red(3, 4) - red(0, 5)).abs() < 1e-6,
            "radius {radius}: round, not square"
        );
        let all: f64 = out.iter().step_by(3).map(|value| f64::from(*value)).sum();
        assert!((all - 1.0).abs() < 1e-4, "radius {radius}: {all}");
    }
    Ok(())
}

#[test]
fn highlights_make_a_light_pixel_count_more_in_the_average_around_it() -> Result<(), Error> {
    // a lone white pixel on black counts 1 + g times a black one: at a pixel with the plain disc's share k of it, the
    // light reaches k (1 + g) of a total weight of 1 + k g (every other neighbour weighs 1)
    let (size, middle) = (41, 20);
    let picture = impulse(size, size, middle, middle);
    let at = (middle * size + middle + 2) * 3;
    let plain = f64::from(blur(6.0, 0.0, Edge::Nearest, size, size, &picture)?[at]);
    for highlights in [2.0, 9.0] {
        let out = f64::from(blur(6.0, highlights, Edge::Nearest, size, size, &picture)?[at]);
        let gain = f64::from(highlights);
        let expected = plain * (1.0 + gain) / (1.0 + plain * gain);
        assert!(
            (out - expected).abs() < 1e-5,
            "highlights {highlights}: {out} against {expected}"
        );
    }
    Ok(())
}

#[test]
fn a_flat_picture_stays_exactly_as_flat_with_and_without_highlights() -> Result<(), Error> {
    // 0.7 is not a sum of weights in f32: the result is held to the picture's range, which is the one value
    for (height, width) in [(1, 1), (1, 40), (40, 1), (9, 13), (30, 41)] {
        let flat = vec![0.7_f32; height * width * 3];
        for edge in EDGES {
            for (radius, highlights) in [(0.0, 0.0), (1.5, 0.0), (9.0, 0.0), (1.5, 50.0), (9.0, 50.0)] {
                let out = blur(radius, highlights, edge, height, width, &flat)?;
                assert_eq!(
                    first_difference_within(&out, &flat, 0.0),
                    None,
                    "{height} x {width}, {edge:?}, radius {radius}, highlights {highlights}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn radius_zero_and_a_disc_of_one_pixel_leave_the_picture_as_it_is() -> Result<(), Error> {
    let picture = varied(6, 5);
    for (radius, highlights) in [(0.0, 0.0), (0.0, 4.0), (0.3, 0.0)] {
        let out = blur(radius, highlights, Edge::Reflect, 6, 5, &picture)?;
        assert_eq!(
            first_difference_within(&out, &picture, 0.0),
            None,
            "radius {radius}, highlights {highlights}"
        );
    }
    Ok(())
}

#[test]
fn a_wrapped_blur_without_highlights_keeps_the_mean_of_every_channel() -> Result<(), Error> {
    let (height, width) = (23, 31);
    let picture = varied(height, width);
    let mean = |values: &[f32], channel: usize| {
        values
            .iter()
            .skip(channel)
            .step_by(3)
            .map(|value| f64::from(*value))
            .sum::<f64>()
            / (height * width) as f64
    };
    for radius in [2.0, 9.5] {
        let out = blur(radius, 0.0, Edge::Wrap, height, width, &picture)?;
        for channel in 0..3 {
            let (before, after) = (mean(&picture, channel), mean(&out, channel));
            assert!(
                (before - after).abs() < 1e-5,
                "{radius}, channel {channel}: {before}, {after}"
            );
        }
        assert!(
            first_difference_within(&out, &picture, 1e-3).is_some(),
            "{radius} blurs"
        );
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn a_flat_picture_is_not_pushed_past_its_own_level_by_rounding(
        level in 0.0_f32..=1.0,
        radius in 1.0_f32..8.0,
        highlights in 0.5_f32..50.0,
        edge in 0_usize..4,
    ) {
        let flat = vec![level; 7 * 9 * 3];
        let out = blur(radius, highlights, EDGES[edge], 7, 9, &flat)?;
        prop_assert!(out.iter().all(|&value| value.to_bits() == level.to_bits()), "level {level}");
    }

    #[test]
    fn a_blur_never_leaves_the_range_of_the_picture(
        height in 1_usize..9,
        width in 1_usize..9,
        picture in prop::collection::vec(0.2_f32..0.8, 81 * 3),
        radius in prop_oneof![0.0_f32..3.0, 6.0_f32..14.0],
        highlights in prop_oneof![Just(0.0_f32), 0.0_f32..50.0],
        edge in 0_usize..4,
    ) {
        let picture = &picture[..height * width * 3];
        let (low, high) = picture.iter().fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
        let out = blur(radius, highlights, EDGES[edge], height, width, picture)?;
        prop_assert_eq!(out.len(), picture.len());
        prop_assert!(out.iter().all(|&value| (low..=high).contains(&value)), "outside {low}..{high}");
    }
}
