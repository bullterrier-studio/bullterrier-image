//! `vignette`, `scanlines` and `equalize`: values worked out by hand from their documented formulas.

use bullterrier_image::effects::{equalize, scanlines, vignette};
use bullterrier_image::params::{Color, EffectParams};
use bullterrier_image::{Error, Inputs, Picture, apply};
use proptest::prelude::*;
use serde_json::Value;

use crate::common::pixels::run;

const TOLERANCE: f32 = 1e-5;

/// `params` applied to the `height` x `width` grey picture of `greys` (one value per pixel); one channel's values.
fn grid_of<P: EffectParams>(params: &P, height: usize, width: usize, greys: &[f32]) -> Result<Vec<f32>, Error> {
    let values: Vec<f32> = greys.iter().flat_map(|&grey| [grey; 3]).collect();
    let image = apply(
        P::EFFECT,
        &Picture::new(height, width, &values)?,
        Value::Object(params.values()),
        &Inputs::default(),
    )?;
    Ok(image.into_values().chunks(3).map(|pixel| pixel[0]).collect())
}

/// `params` applied to a picture of `height` rows of `width` pixels, each `grey` in all channels; one channel's values.
fn grid<P: EffectParams>(params: &P, height: usize, width: usize, grey: f32) -> Result<Vec<f32>, Error> {
    grid_of(params, height, width, &vec![grey; height * width])
}

fn assert_close(actual: &[f32], expected: &[f32], context: &str) {
    assert!(
        actual.len() == expected.len() && actual.iter().zip(expected).all(|(a, e)| (a - e).abs() <= TOLERANCE),
        "{context}: {actual:?}, not {expected:?}"
    );
}

// --- vignette ----------------------------------------------------------------------------------------------------

/// Black fading to white from 0.5 out over 0.5, so a pixel's value is its smoothstep share.
fn to_white(roundness: f32) -> vignette::Params {
    vignette::Params {
        amount: 1.0,
        radius: 0.5,
        feather: 0.5,
        roundness,
        color: Color([1.0; 3]),
    }
}

#[test]
fn a_rectangle_fades_by_the_larger_offset_along_a_smoothstep() -> Result<(), Error> {
    // one row of 4 pixels: offsets 0.75, 0.25, 0.25, 0.75; 0.75 is halfway through the fade (smoothstep 0.5), 0.25 before it
    let out = grid(&to_white(0.0), 1, 4, 0.0)?;
    assert_close(&out, &[0.5, 0.0, 0.0, 0.5], "a rectangle, one row");
    Ok(())
}

#[test]
fn an_ellipse_reaches_a_corner_less_far_than_a_rectangle() -> Result<(), Error> {
    // distance 0.75 x sqrt(1/2) = 0.53033 (offset 0.75 across, 0 down), so t = 0.06066 and 3t^2 - 2t^3 = 0.0105926
    let out = grid(&to_white(1.0), 1, 4, 0.0)?;
    assert_close(&out, &[0.010_592_55, 0.0, 0.0, 0.010_592_55], "an ellipse, one row");
    Ok(())
}

#[test]
fn a_rounded_rectangle_takes_the_norm_with_exponent_two_over_roundness() -> Result<(), Error> {
    // 2 rows of 4: the corner pixel is 0.5 down and 0.75 across; exponent 4: 0.75 x ((0.5 / 0.75)^4 + 1)^(1/4) / 2^(1/4)
    // = 0.65974, so t = 0.31949 and the share is 0.240996
    let out = grid(&to_white(0.5), 2, 4, 0.0)?;
    assert_close(&out[..1], &[0.240_995_8], "the corner of a rounded rectangle");
    Ok(())
}

#[test]
fn the_middle_pixel_of_an_odd_picture_is_left_alone_even_with_a_hard_edge_at_radius_zero() -> Result<(), Error> {
    let params = vignette::Params {
        radius: 0.0,
        feather: 0.0,
        amount: 1.0,
        ..vignette::Params::default()
    };
    let out = grid(&params, 3, 3, 0.7)?;
    assert!(out.iter().all(|value| value.is_finite()), "{out:?}");
    assert_close(&out[4..5], &[0.7], "the middle of 3 x 3");
    assert_close(&out[..1], &[0.0], "a corner, fully faded to black");
    Ok(())
}

proptest! {
    #[test]
    fn a_pixel_ends_between_its_own_value_and_the_colour(
        value in 0.0_f32..=1.0, amount in 0.0_f32..=1.0, radius in 0.0_f32..=1.0, feather in 0.0_f32..=1.0,
        roundness in 0.0_f32..=1.0, tint in 0.0_f32..=1.0,
    ) {
        let params = vignette::Params { amount, radius, feather, roundness, color: Color([tint; 3]) };
        let out = run(&params, &[[value; 3]; 7])?;
        let (low, high) = (value.min(tint) - 1e-6, value.max(tint) + 1e-6);
        prop_assert!(out.iter().flatten().all(|v| (low..=high).contains(v)), "{:?}", out);
    }
}

// --- scanlines ---------------------------------------------------------------------------------------------------

fn lines(spacing: f32, thickness: f32, offset: f32, softness: f32) -> scanlines::Params {
    scanlines::Params {
        spacing,
        thickness,
        darkness: 1.0,
        offset,
        softness,
    }
}

#[test]
fn a_hard_line_blackens_the_rows_it_covers_and_a_row_it_half_covers_by_half() -> Result<(), Error> {
    // lines [1, 3) and [5, 7) of a period of 4
    let whole = grid(&lines(4.0, 0.5, 1.0, 0.0), 8, 1, 1.0)?;
    assert_close(&whole, &[1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0], "whole rows");
    // lines [0.5, 2.5) and [4.5, 6.5)
    let half = grid(&lines(4.0, 0.5, 0.5, 0.0), 8, 1, 1.0)?;
    assert_close(&half, &[0.5, 0.0, 0.5, 1.0, 0.5, 0.0, 0.5, 1.0], "rows half covered");
    Ok(())
}

#[test]
fn a_fully_soft_line_swells_along_sin_squared_and_the_rows_add_up_to_the_hard_ones() -> Result<(), Error> {
    // the darkness is 0.5 (1 + cos(pi (x - 1) / 2)): its integral over each row is 0.5 +- 1/pi
    let swell = 0.5 + std::f32::consts::FRAC_1_PI;
    let dip = 0.5 - std::f32::consts::FRAC_1_PI;
    let out = grid(&lines(4.0, 0.5, 0.0, 1.0), 4, 1, 1.0)?;
    assert_close(&out, &[1.0 - swell, 1.0 - swell, 1.0 - dip, 1.0 - dip], "a soft line");
    Ok(())
}

#[test]
fn a_zero_thickness_or_darkness_changes_nothing_and_a_full_one_darkens_everything_alike() -> Result<(), Error> {
    assert_close(&grid(&lines(3.0, 0.0, 0.7, 0.4), 6, 1, 0.8)?, &[0.8; 6], "no line");
    let none = scanlines::Params {
        darkness: 0.0,
        ..lines(3.0, 0.5, 0.0, 0.0)
    };
    assert_close(&grid(&none, 6, 1, 0.8)?, &[0.8; 6], "no darkness");
    let touching = scanlines::Params {
        darkness: 0.5,
        ..lines(2.5, 1.0, 0.3, 0.7)
    };
    assert_close(&grid(&touching, 6, 1, 0.8)?, &[0.4; 6], "lines that touch");
    Ok(())
}

proptest! {
    #[test]
    fn the_average_darkening_over_whole_periods_is_darkness_times_thickness_whatever_the_softness(
        period in 1_u8..=9, periods in 1_u8..=4, thickness in 0.0_f32..=1.0, offset in 0.0_f32..=40.0,
        softness in 0.0_f32..=1.0, darkness in 0.0_f32..=1.0,
    ) {
        let params = scanlines::Params { spacing: f32::from(period), thickness, darkness, offset, softness };
        let height = usize::from(period) * usize::from(periods);
        let out = grid(&params, height, 1, 1.0)?;
        let darkening: f32 = out.iter().map(|gain| 1.0 - gain).sum();
        let expected = darkness * thickness * f32::from(period) * f32::from(periods);
        prop_assert!((darkening - expected).abs() <= 1e-3, "{darkening} not {expected}");
        prop_assert!(out.iter().all(|gain| (-1e-6..=1.0 + 1e-6).contains(gain)), "{:?}", out);
    }
}

// --- equalize ----------------------------------------------------------------------------------------------------

#[test]
fn one_unclipped_tile_maps_a_pixel_to_the_share_of_pixels_at_or_below_its_bin() -> Result<(), Error> {
    // 4 bins; the greys sit in bins 0, 0, 2, 3, so the counts are 2, 0, 1, 1 and the cumulative shares 0.5, 0.5, 0.75, 1
    let params = equalize::Params {
        tiles: 1,
        limit: 4.0,
        bins: 4,
    };
    let pixels = [0.125, 0.125, 0.625, 0.875].map(|grey| [grey; 3]);
    let out = run(&params, &pixels)?;
    assert_close(
        &out.iter().map(|pixel| pixel[0]).collect::<Vec<_>>(),
        &[0.5, 0.5, 0.75, 1.0],
        "classic equalisation",
    );
    Ok(())
}

#[test]
fn a_limit_of_one_flattens_the_histogram_so_the_mapping_is_the_uniform_ramp() -> Result<(), Error> {
    let params = equalize::Params {
        tiles: 1,
        limit: 1.0,
        bins: 4,
    };
    let pixels = [0.125, 0.125, 0.625, 0.875].map(|grey| [grey; 3]);
    let out = run(&params, &pixels)?;
    assert_close(
        &out.iter().map(|pixel| pixel[0]).collect::<Vec<_>>(),
        &[0.25, 0.25, 0.75, 1.0],
        "a flat histogram",
    );
    Ok(())
}

#[test]
#[expect(clippy::cast_precision_loss, reason = "a position is below 32")]
fn neighbouring_tiles_blend_by_the_distance_to_their_centres() -> Result<(), Error> {
    // 16 rows by 32 columns in 2 x 2 tiles of 8 x 16: the left half is one dark grey (bin 64 of 256), the right half
    // one bright grey (bin 192). The left tiles map the dark grey to 1; the right tiles map it (below all their pixels)
    // to 0. So a left pixel at column x is 1 - w, w the share of the way from the left tile's centre (column 7.5) to the
    // right tile's (23.5), (x - 7.5) / 16 held to 0..1; right pixels map to 1 in both tiles. Also turned on its side,
    // and with the grey halves swapped, where a dark pixel at x is w itself.
    let (dark, bright) = (64.5_f32 / 256.0, 192.5_f32 / 256.0);
    let params = equalize::Params {
        tiles: 2,
        limit: 256.0,
        bins: 256,
    };
    for (turned, dark_first) in [(false, true), (true, true), (false, false)] {
        let (height, width) = if turned { (32, 16) } else { (16, 32) };
        let along = |index: usize| if turned { index / width } else { index % width };
        let greys: Vec<f32> = (0..height * width)
            .map(|index| {
                if (along(index) < 16) == dark_first {
                    dark
                } else {
                    bright
                }
            })
            .collect();
        let out = grid_of(&params, height, width, &greys)?;
        let share = |position: usize| ((position as f32 - 7.5) / 16.0).clamp(0.0, 1.0);
        let expected: Vec<f32> = (0..height * width)
            .map(|index| match (along(index), dark_first) {
                (position @ 0..16, true) => 1.0 - share(position),
                (position @ 16.., false) => share(position),
                _ => 1.0,
            })
            .collect();
        assert_close(&out, &expected, &format!("turned {turned}, dark first {dark_first}"));
    }
    Ok(())
}

proptest! {
    #[test]
    fn equalising_keeps_a_colours_hue_and_never_leaves_0_to_1(
        values in prop::collection::vec(0.0_f32..=1.0, 3 * 40), tiles in 1_i32..=6, limit in 1.0_f32..=8.0, bins in 2_i32..=64,
    ) {
        let pixels: Vec<[f32; 3]> = values.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
        let out = run(&equalize::Params { tiles, limit, bins }, &pixels)?;
        for (before, after) in pixels.iter().zip(&out) {
            prop_assert!(after.iter().all(|v| (0.0..=1.0 + 1e-6).contains(v)), "{:?}", after);
            // the channels are scaled alike: before x after[i] == after x before[i]
            for i in 0..3 {
                for j in 0..3 {
                    prop_assert!((after[i] * before[j] - after[j] * before[i]).abs() <= 1e-5);
                }
            }
        }
    }
}
