//! `bloom`: the halo of a lone bright pixel compared with a sum of Gaussians worked out from their definition, the light
//! of an even picture worked out by hand, and the properties a screen keeps.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "small picture sizes and counts, and the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::effects::bloom::Params;
use bullterrier_image::params::Color;
use bullterrier_image::{EdgeOrConstant, Error};
use proptest::prelude::*;

use crate::common::compare::first_difference_within;
use crate::common::gaussian::weights;
use crate::common::pictures;
use crate::common::pictures::varied;
use crate::common::pixels::run_picture;

/// The pictures are compared at this distance: the sums are made in f64 and f32 is 6e-8 at 1.
const TOLERANCE: f32 = 2e-6;

fn bloomed(params: &Params, size: usize, picture: &[f32]) -> Result<Vec<f32>, Error> {
    run_picture(params, size, size, picture)
}

/// A `size` x `size` picture of one colour.
fn even(size: usize, pixel: [f32; 3]) -> Vec<f32> {
    pixel.repeat(size * size)
}

/// A black `size` x `size` picture with `spot` at its middle pixel.
fn spot(size: usize, spot: [f32; 3]) -> Vec<f32> {
    pictures::spot(size, size, size / 2, size / 2, spot)
}

/// The red channel of every pixel.
fn red(picture: &[f32]) -> Vec<f32> {
    picture.iter().step_by(3).copied().collect()
}

/// The screen blend mode from its definition.
fn screen(base: f32, light: f32) -> f32 {
    1.0 - (1.0 - base) * (1.0 - light.clamp(0.0, 1.0))
}

/// The halo of a lone highlight of 1 at the middle of a `size` x `size` picture, `amount` times the average of the
/// Gaussians of sigma `radius`, `radius / 2`... (`octaves` of them), weighted by `persistence` to the octave and the
/// weights scaled to add up to 1: the outer product of each one's one-dimensional weights.
fn halo(size: usize, radius: f64, octaves: i32, persistence: f64, amount: f64) -> Vec<f64> {
    let shares: Vec<f64> = (0..octaves).map(|octave| persistence.powi(octave)).collect();
    let total: f64 = shares.iter().sum();
    let mut halo = vec![0.0; size * size];
    for (octave, share) in shares.iter().enumerate() {
        let (line, reach) = weights(radius / 2.0_f64.powi(octave as i32), 4.0);
        let middle = size / 2;
        for (row, row_weight) in line.iter().enumerate() {
            for (column, column_weight) in line.iter().enumerate() {
                let (y, x) = (middle + row - reach, middle + column - reach);
                halo[y * size + x] += amount * share / total * row_weight * column_weight;
            }
        }
    }
    halo
}

#[test]
fn a_single_bright_pixel_spreads_as_the_weighted_sum_of_octave_gaussians() -> Result<(), Error> {
    // the picture is far larger than the widest reach (4 x 8 = 32 px), so the edge rule is not seen
    let size = 81;
    let rows = [
        (8.0, 1, 0.5, 0.9),
        (8.0, 3, 0.0, 0.9),
        (8.0, 3, 1.0, 0.9),
        (8.0, 3, 0.5, 0.9),
        (6.0, 5, 0.7, 0.4),
    ];
    for (radius, octaves, persistence, amount) in rows {
        let params = Params {
            radius,
            octaves,
            persistence,
            amount,
            threshold: 0.5,
            ..Params::default()
        };
        let out = red(&bloomed(&params, size, &spot(size, [1.0; 3]))?);
        let expected: Vec<f32> = halo(
            size,
            f64::from(radius),
            octaves,
            f64::from(persistence),
            f64::from(amount),
        )
        .into_iter()
        .map(|light| light as f32)
        .collect();
        // the bright pixel is white and the light is screened over black
        let middle = size / 2 * size + size / 2;
        assert!(
            (out[middle] - 1.0).abs() < TOLERANCE,
            "{radius} {octaves} {persistence}"
        );
        let mut out = out;
        let mut expected = expected;
        out[middle] = 0.0;
        expected[middle] = 0.0;
        assert_eq!(
            first_difference_within(&out, &expected, TOLERANCE),
            None,
            "radius {radius}, {octaves} octaves, persistence {persistence}"
        );
    }
    Ok(())
}

#[test]
fn the_comparison_tells_halos_that_differ_apart() -> Result<(), Error> {
    let size = 81;
    let params = Params {
        radius: 8.0,
        octaves: 3,
        persistence: 0.5,
        amount: 0.9,
        threshold: 0.5,
        ..Params::default()
    };
    let out = red(&bloomed(&params, size, &spot(size, [1.0; 3]))?);
    for (octaves, persistence) in [(2, 0.5), (3, 0.6), (4, 0.5)] {
        let other = halo(size, 8.0, octaves, persistence, 0.9);
        let other: Vec<f32> = other.into_iter().map(|light| light as f32).collect();
        assert!(
            first_difference_within(&out, &other, TOLERANCE).is_some(),
            "{octaves} {persistence}"
        );
    }
    Ok(())
}

#[test]
fn each_octave_adds_its_share_of_the_spread() -> Result<(), Error> {
    // The mean squared distance of the light from its source is 2 sigma^2 for a 2-D Gaussian, so for the average of the
    // octaves it is 2 x the sum of weight x sigma^2 (the cut-off at 4 sigma takes under 1% of it). The light stays far
    // below 1 so that the screen does not bend it.
    let size = 161;
    let middle = size / 2;
    let radius = 12.0_f64;
    for (octaves, persistence) in [(1, 0.5_f64), (2, 0.5), (4, 0.5), (4, 1.0), (3, 0.0)] {
        let params = Params {
            radius: radius as f32,
            octaves,
            persistence: persistence as f32,
            amount: 0.5,
            threshold: 0.5,
            ..Params::default()
        };
        // a source of 0.8 (a highlight of 0.6): the light at its own pixel is read off the screen, light = (out - base) / (1 - base)
        let mut out = red(&bloomed(&params, size, &spot(size, [0.8; 3]))?);
        let source = size / 2 * size + size / 2;
        out[source] = (out[source] - 0.8) / (1.0 - 0.8);
        let (mut mass, mut moment) = (0.0_f64, 0.0_f64);
        for (index, light) in out.iter().enumerate() {
            let (y, x) = (
                (index / size) as f64 - middle as f64,
                (index % size) as f64 - middle as f64,
            );
            mass += f64::from(*light);
            moment += f64::from(*light) * (x * x + y * y);
        }
        let shares: Vec<f64> = (0..octaves).map(|octave| persistence.powi(octave)).collect();
        let total: f64 = shares.iter().sum();
        let expected: f64 = shares
            .iter()
            .enumerate()
            .map(|(octave, share)| 2.0 * share / total * (radius / 2.0_f64.powi(octave as i32)).powi(2))
            .sum();
        let spread = moment / mass;
        assert!(
            (spread / expected - 1.0).abs() < 0.02,
            "{octaves} octaves, persistence {persistence}: {spread} against {expected}"
        );
    }
    Ok(())
}

#[test]
fn an_even_picture_is_screened_with_its_own_highlight_whatever_the_octaves() -> Result<(), Error> {
    // every octave leaves an even picture as it is and the weights add up to 1, so the halo is the highlight
    for (octaves, persistence) in [(1, 0.5), (3, 0.0), (4, 0.5), (6, 1.0)] {
        for (value, amount, threshold) in [(0.8, 0.6, 0.55), (0.9, 3.0, 0.5)] {
            let params = Params {
                radius: 4.0,
                octaves,
                persistence,
                amount,
                threshold,
                ..Params::default()
            };
            let out = bloomed(&params, 16, &even(16, [value; 3]))?;
            let highlight = (value - threshold) / (1.0 - threshold);
            let expected = vec![screen(value, amount * highlight); 16 * 16 * 3];
            assert_eq!(
                first_difference_within(&out, &expected, 1e-5),
                None,
                "{octaves} octaves, persistence {persistence}, value {value}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_colour_tints_the_light_of_the_brightest_channel() -> Result<(), Error> {
    // on an even picture the halo is the highlight of the brightest channel, and the light that times the colour
    let rows = [
        ([0.9, 0.3, 0.1], [1.0, 0.5, 0.0], 1.0, 0.5),
        ([0.2, 0.9, 0.4], [0.0, 0.0, 1.0], 2.0, 0.6),
        ([0.4, 0.45, 0.3], [1.0, 1.0, 1.0], 3.0, 0.5),
    ];
    for (pixel, color, amount, threshold) in rows {
        let params = Params {
            radius: 4.0,
            octaves: 1,
            amount,
            threshold,
            color: Some(Color(color)),
            ..Params::default()
        };
        let out = bloomed(&params, 32, &even(32, pixel))?;
        let brightest = pixel[0].max(pixel[1]).max(pixel[2]);
        let highlight = ((brightest - threshold) / (1.0 - threshold)).max(0.0);
        let expected: Vec<f32> = (0..32 * 32)
            .flat_map(|_| [0, 1, 2].map(|channel| screen(pixel[channel], amount * highlight * color[channel])))
            .collect();
        assert_eq!(
            first_difference_within(&out, &expected, 1e-5),
            None,
            "{pixel:?} tinted {color:?}"
        );
    }
    Ok(())
}

#[test]
fn without_a_colour_each_channel_blooms_on_its_own_and_a_colour_makes_the_light_its_hue() -> Result<(), Error> {
    let size = 41;
    let picture = spot(size, [1.0, 0.0, 0.0]);
    let at = (size / 2 * size + size / 2 + 7) * 3; // seven px to the right of the red spot
    let own = bloomed(
        &Params {
            radius: 4.0,
            amount: 1.0,
            ..Params::default()
        },
        size,
        &picture,
    )?;
    assert!(own[at] > 5e-4, "the red spills beyond the spot");
    assert_eq!((own[at + 1], own[at + 2]), (0.0, 0.0), "and nothing else does");
    // a colour makes the light that colour whatever the highlight's own: the added light lies along the colour
    let color = [0.2, 0.8, 0.4];
    let tinted = bloomed(
        &Params {
            radius: 4.0,
            amount: 1.0,
            color: Some(Color(color)),
            ..Params::default()
        },
        size,
        &picture,
    )?;
    let strength = tinted[at] / color[0];
    assert!(strength > 5e-4, "{strength}");
    assert!((tinted[at + 1] - strength * color[1]).abs() < 1e-6);
    assert!((tinted[at + 2] - strength * color[2]).abs() < 1e-6);
    Ok(())
}

#[test]
fn a_black_colour_gives_no_light_and_black_stays_black() -> Result<(), Error> {
    let picture = varied(12, 12);
    let black = Params {
        color: Some(Color([0.0; 3])),
        threshold: 0.0,
        ..Params::default()
    };
    assert_eq!(
        first_difference_within(&bloomed(&black, 12, &picture)?, &picture, 0.0),
        None
    );
    let dark = even(12, [0.0; 3]);
    assert_eq!(
        first_difference_within(&bloomed(&Params::default(), 12, &dark)?, &dark, 0.0),
        None
    );
    Ok(())
}

#[test]
fn nothing_above_the_threshold_and_an_amount_of_nothing_leave_the_picture_as_it_is() -> Result<(), Error> {
    let dim: Vec<f32> = varied(12, 12).iter().map(|value| value * 0.5).collect();
    let none_above = Params {
        threshold: 0.5,
        amount: 10.0,
        ..Params::default()
    };
    assert_eq!(
        first_difference_within(&bloomed(&none_above, 12, &dim)?, &dim, 0.0),
        None
    );
    let bright = varied(12, 12);
    let no_amount = Params {
        threshold: 0.0,
        amount: 0.0,
        ..Params::default()
    };
    assert_eq!(
        first_difference_within(&bloomed(&no_amount, 12, &bright)?, &bright, 0.0),
        None
    );
    Ok(())
}

#[test]
fn a_radius_of_nothing_keeps_the_light_on_the_highlights_themselves() -> Result<(), Error> {
    // with nothing to blur the light of a pixel is its own highlight, screened over it
    let picture = varied(10, 10);
    let params = Params {
        radius: 0.0,
        threshold: 0.4,
        amount: 0.8,
        ..Params::default()
    };
    let expected: Vec<f32> = picture
        .iter()
        .map(|&value| screen(value, 0.8 * ((value - 0.4) / 0.6).clamp(0.0, 1.0)))
        .collect();
    assert_eq!(
        first_difference_within(&bloomed(&params, 10, &picture)?, &expected, 1e-6),
        None
    );
    Ok(())
}

#[test]
fn truncate_sets_where_each_octave_is_cut_off() -> Result<(), Error> {
    let size = 81;
    let picture = spot(size, [1.0; 3]);
    let six_px_right = (size / 2 * size + size / 2 + 6) * 3; // 3 sigma out for the widest octave at radius 2
    let at = |truncate: f32| -> Result<f32, Error> {
        let params = Params {
            radius: 2.0,
            octaves: 1,
            threshold: 0.5,
            truncate,
            ..Params::default()
        };
        Ok(bloomed(&params, size, &picture)?[six_px_right])
    };
    assert!(at(4.0)? > 0.0);
    assert_eq!(at(2.0)?, 0.0);
    Ok(())
}

#[test]
fn the_edge_sets_what_the_blur_sees_beyond_it() -> Result<(), Error> {
    let size = 41;
    let mut picture = vec![0.0; size * size * 3];
    let left_edge = (size / 2 * size) * 3;
    picture[left_edge..left_edge + 3].fill(1.0); // a bright pixel on the left edge
    let far_side = (size / 2 * size + size - 1) * 3; // the right edge of the same row, one px away round the back
    let on = |edge| -> Result<f32, Error> {
        let params = Params {
            radius: 3.0,
            threshold: 0.5,
            edge,
            ..Params::default()
        };
        Ok(bloomed(&params, size, &picture)?[far_side])
    };
    assert!(on(EdgeOrConstant::Wrap)? > 0.01);
    assert_eq!(on(EdgeOrConstant::Reflect)?, 0.0);
    Ok(())
}

#[test]
fn constant_value_is_the_highlight_assumed_beyond_the_edge() -> Result<(), Error> {
    // beyond the edge the picture is a flat highlight of `constant_value`; far from the edge it does not matter
    let size = 81;
    let picture = even(size, [0.0; 3]);
    let with = |constant_value| -> Result<Vec<f32>, Error> {
        let params = Params {
            radius: 2.0,
            octaves: 1,
            edge: EdgeOrConstant::Constant,
            constant_value,
            ..Params::default()
        };
        bloomed(&params, size, &picture)
    };
    let lit = with(1.0)?;
    assert!(
        lit[0] > 0.4,
        "in the corner 3/4 of the blur's weight is beyond the edge: {}",
        lit[0]
    );
    assert!(lit[(size / 2 * size + size / 2) * 3].abs() < 1e-6);
    assert!(with(0.0)?.iter().all(|value| *value == 0.0));
    Ok(())
}

fn parameters() -> impl Strategy<Value = Params> {
    (
        0.0..20.0_f32,
        0.0..0.99_f32,
        0.0..10.0_f32,
        proptest::option::of([0.0..1.0_f32, 0.0..1.0, 0.0..1.0]),
        1..=8,
        0.0..=1.0_f32,
        1.0..=10.0_f32,
        (0..5_usize, 0.0..=1.0_f32),
    )
        .prop_map(
            |(radius, threshold, amount, color, octaves, persistence, truncate, (edge, constant_value))| Params {
                radius,
                threshold,
                amount,
                color: color.map(Color),
                octaves,
                persistence,
                truncate,
                edge: [
                    EdgeOrConstant::Reflect,
                    EdgeOrConstant::Constant,
                    EdgeOrConstant::Nearest,
                    EdgeOrConstant::Mirror,
                    EdgeOrConstant::Wrap,
                ][edge],
                constant_value,
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn the_picture_only_ever_brightens_and_stays_within_0_to_1(
        params in parameters(),
        height in 1..9_usize,
        width in 1..9_usize,
    ) {
        let picture = varied(height, width);
        let out = run_picture(&params, height, width, &picture).map_err(|error: Error| TestCaseError::fail(error.to_string()))?;
        for (out, base) in out.iter().zip(&picture) {
            prop_assert!(*out >= *base - 1e-6 && *out <= 1.0 + 1e-6, "{base} became {out}");
        }
    }
}
