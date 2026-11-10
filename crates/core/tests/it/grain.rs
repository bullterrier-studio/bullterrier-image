//! `grain`: noise added to the picture, judged by its statistics at a fixed seed against hand-worked values: the
//! deviation follows `amount` and the brightness `balance` picks, `size` clumps without changing the deviation, the
//! distributions have their known shapes, and the result is clipped.

use bullterrier_image::effects::grain::{Distribution, Params};
use bullterrier_image::{Edge, Error};

use crate::common::compare::first_bit_difference;
use crate::common::numbers::float;
use crate::common::pictures::varied;
use crate::common::pixels::{run_picture, run_rows};
use crate::common::stats::{correlation, deviation, mean, neighbour_correlation};

const SIDE: usize = 128;

/// A `SIDE` x `SIDE` picture of one colour.
fn patch(colour: [f32; 3]) -> Vec<f32> {
    colour.repeat(SIDE * SIDE)
}

/// What the grain adds to `picture` (`SIDE` square), channel `which`, as f64.
fn added(params: &Params, picture: &[f32], which: usize) -> Result<Vec<f64>, Error> {
    let out = run_picture(params, SIDE, SIDE, picture)?;
    Ok(out
        .iter()
        .zip(picture)
        .skip(which)
        .step_by(3)
        .map(|(out, input)| f64::from(out - input))
        .collect())
}

/// A weak grain at mid-grey and below, so that nothing of a flat patch up to 0.5 is clipped.
fn weak() -> Params {
    Params {
        amount: 0.013,
        balance: -7.0 / 13.0,
        ..Params::default()
    }
}

#[test]
fn an_amount_of_zero_leaves_the_picture_as_it_is_at_any_balance() -> Result<(), Error> {
    let picture = varied(9, 11);
    for distribution in [Distribution::Gaussian, Distribution::Uniform] {
        for balance in [-1.0, 0.0, 1.0] {
            for color in [false, true] {
                let params = Params {
                    amount: 0.0,
                    balance,
                    color,
                    distribution,
                    ..Params::default()
                };
                let out = run_picture(&params, 9, 11, &picture)?;
                assert_eq!(first_bit_difference(&out, &picture), None, "{params:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn the_same_seed_gives_the_same_grain_and_another_seed_other_grain() -> Result<(), Error> {
    let picture = varied(30, 40);
    let run = |seed| {
        run_picture(
            &Params {
                seed,
                ..Params::default()
            },
            30,
            40,
            &picture,
        )
    };
    assert_eq!(first_bit_difference(&run(1)?, &run(1)?), None);
    assert!(first_bit_difference(&run(1)?, &run(2)?).is_some());
    // the largest seed is a seed like any other
    assert!(first_bit_difference(&run(u32::MAX)?, &run(u32::MAX - 1)?).is_some());
    Ok(())
}

#[test]
fn the_deviation_is_the_amount_at_mid_grey_and_follows_the_balance_across_brightness() -> Result<(), Error> {
    // (balance, brightness, expected deviation) with an amount of 0.01, worked out by hand from
    // amount x (1 + balance x (2 x brightness - 1))
    let rows = [
        (0.0, 0.5, 0.01),
        (0.0, 0.2, 0.01),
        (-0.5, 0.5, 0.01),
        (-1.0, 0.2, 0.016),
        (1.0, 0.8, 0.016),
        (1.0, 0.2, 0.004),
        (-0.5, 0.8, 0.007),
    ];
    for distribution in [Distribution::Gaussian, Distribution::Uniform] {
        for color in [false, true] {
            for (balance, brightness, expected) in rows {
                let params = Params {
                    amount: 0.01,
                    balance,
                    color,
                    distribution,
                    seed: 1,
                    ..Params::default()
                };
                let grain = added(&params, &patch([brightness; 3]), 0)?;
                let context = format!("balance {balance}, brightness {brightness}, {distribution:?}, color {color}");
                assert!(
                    (deviation(&grain) / expected - 1.0).abs() < 0.03,
                    "{context}: {}",
                    deviation(&grain)
                );
                assert!(mean(&grain).abs() < 0.0005, "{context}: mean {}", mean(&grain));
            }
        }
    }
    Ok(())
}

#[test]
fn brightness_is_the_luminance_of_the_pixel_for_all_three_channels() -> Result<(), Error> {
    // Rec. 709 luminance of (0.2, 0.4, 0.8) is 0.2126 x 0.2 + 0.7152 x 0.4 + 0.0722 x 0.8 = 0.38626 (the mean of the
    // channels would be 0.467): with a balance of -1 the deviation is 0.01 x (1 - (2 x 0.38626 - 1)) in every channel
    let params = Params {
        amount: 0.01,
        balance: -1.0,
        color: true,
        seed: 1,
        ..Params::default()
    };
    let expected = 0.01 * (1.0 - (2.0 * 0.38626 - 1.0));
    for channel in 0..3 {
        let grain = added(&params, &patch([0.2, 0.4, 0.8]), channel)?;
        assert!((deviation(&grain) / expected - 1.0).abs() < 0.03, "channel {channel}");
    }
    Ok(())
}

#[test]
fn each_pixel_follows_its_own_brightness() -> Result<(), Error> {
    // a dark left half (0.15) and a bright right half (0.85) with shadow grain only: the deviation is
    // 0.02 x (1 - brightness) in each half
    let picture: Vec<f32> = (0..SIDE * SIDE)
        .flat_map(|at| [if at % SIDE < SIDE / 2 { 0.15 } else { 0.85 }; 3])
        .collect();
    let params = Params {
        amount: 0.01,
        balance: -1.0,
        seed: 3,
        ..Params::default()
    };
    let grain = added(&params, &picture, 0)?;
    let half = |first: bool| -> Vec<f64> {
        grain
            .iter()
            .enumerate()
            .filter(|(at, _)| (at % SIDE < SIDE / 2) == first)
            .map(|(_, value)| *value)
            .collect()
    };
    assert!((deviation(&half(true)) / (0.02 * 0.85) - 1.0).abs() < 0.05);
    assert!((deviation(&half(false)) / (0.02 * 0.15) - 1.0).abs() < 0.05);
    Ok(())
}

#[test]
fn grey_grain_is_the_same_in_every_channel_and_colour_grain_is_independent_per_channel() -> Result<(), Error> {
    let flat = patch([0.5; 3]);
    let grey = Params { seed: 2, ..weak() };
    let channels: Vec<Vec<f64>> = (0..3)
        .map(|which| added(&grey, &flat, which))
        .collect::<Result<_, _>>()?;
    assert_eq!(channels[0], channels[1]);
    assert_eq!(channels[0], channels[2]);
    let colour = Params {
        color: true,
        seed: 2,
        ..weak()
    };
    let channels: Vec<Vec<f64>> = (0..3)
        .map(|which| added(&colour, &flat, which))
        .collect::<Result<_, _>>()?;
    for (one, other) in [(0, 1), (0, 2), (1, 2)] {
        assert!(
            correlation(&channels[one], &channels[other]).abs() < 0.03,
            "{one} and {other}"
        );
    }
    Ok(())
}

#[test]
fn size_above_one_pixel_clumps_the_grain_and_one_pixel_does_not() -> Result<(), Error> {
    let flat = patch([0.5; 3]);
    let correlation_at = |size| -> Result<f64, Error> {
        let grain = added(
            &Params {
                size,
                seed: 1,
                ..weak()
            },
            &flat,
            0,
        )?;
        Ok(neighbour_correlation(&grain, SIDE))
    };
    let (one, three, six) = (correlation_at(1.0)?, correlation_at(3.0)?, correlation_at(6.0)?);
    assert!(one.abs() < 0.03, "{one}");
    assert!(three > 0.5, "{three}");
    assert!(six > three, "{six} vs {three}");
    Ok(())
}

#[test]
fn clumping_keeps_the_standard_deviation() -> Result<(), Error> {
    for distribution in [Distribution::Gaussian, Distribution::Uniform] {
        for size in [1.0, 4.0] {
            let params = Params {
                amount: 0.03,
                size,
                distribution,
                seed: 5,
                ..Params::default()
            };
            let grain = added(&params, &patch([0.5; 3]), 0)?;
            assert!(
                (deviation(&grain) / 0.03 - 1.0).abs() < 0.02,
                "{distribution:?} at {size}"
            );
        }
    }
    Ok(())
}

#[test]
fn uniform_grain_stays_within_its_bound_and_gaussian_grain_does_not() -> Result<(), Error> {
    // a uniform distribution of deviation s spans +-sqrt(3) s, evenly; a Gaussian's tail goes further
    let bound = 0.03 * 3.0_f64.sqrt();
    let flat = patch([0.5; 3]);
    let uniform = added(
        &Params {
            amount: 0.03,
            distribution: Distribution::Uniform,
            seed: 1,
            ..Params::default()
        },
        &flat,
        0,
    )?;
    let largest = uniform.iter().fold(0.0_f64, |most, value| most.max(value.abs()));
    assert!(largest <= bound * 1.0001, "{largest}"); // float32 rounding of the picture
    assert!(largest > 0.99 * bound, "{largest}"); // and it fills the range
    let inner = uniform.iter().filter(|value| value.abs() < bound / 2.0).count();
    assert!((float(inner) / float(uniform.len()) - 0.5).abs() < 0.01); // evenly: half within half the bound
    let gaussian = added(
        &Params {
            amount: 0.03,
            seed: 1,
            ..Params::default()
        },
        &flat,
        0,
    )?;
    assert!(gaussian.iter().any(|value| value.abs() > 1.8 * bound));
    Ok(())
}

#[test]
fn edge_and_truncate_decide_what_the_clumping_blur_sees_beyond_the_border() -> Result<(), Error> {
    // the blur (size 8: sigma 4) reaches truncate x 4 px in from the border: 4 px at 1, 16 px at 4
    let flat = vec![0.5; 96 * 96 * 3];
    let grain = |truncate, edge| -> Result<Vec<f64>, Error> {
        let params = Params {
            size: 8.0,
            seed: 2,
            truncate,
            edge,
            ..weak()
        };
        let out = run_picture(&params, 96, 96, &flat)?;
        Ok(out.iter().step_by(3).map(|value| f64::from(value - 0.5)).collect())
    };
    let inside = |field: &[f64]| -> Vec<f64> {
        (8..88)
            .flat_map(|row| (8..88).map(move |column| row * 96 + column))
            .map(|at| field[at])
            .collect()
    };
    let (reflect_near, wrap_near) = (grain(1.0, Edge::Reflect)?, grain(1.0, Edge::Wrap)?);
    let (reflect_far, wrap_far) = (grain(4.0, Edge::Reflect)?, grain(4.0, Edge::Wrap)?);
    assert!(correlation(&inside(&reflect_near), &inside(&wrap_near)) > 1.0 - 1e-6); // out of reach
    assert!(correlation(&inside(&reflect_far), &inside(&wrap_far)) < 1.0 - 1e-6); // reached
    assert_ne!(reflect_near[..2], wrap_near[..2]); // at the border
    Ok(())
}

#[test]
fn the_result_is_clipped_to_the_picture_range() -> Result<(), Error> {
    let out = run_picture(
        &Params {
            amount: 0.5,
            ..Params::default()
        },
        SIDE,
        SIDE,
        &patch([1.0; 3]),
    )?;
    let largest = out.iter().fold(0.0_f32, |most, value| most.max(*value));
    let smallest = out.iter().fold(1.0_f32, |least, value| least.min(*value));
    assert!((largest - 1.0).abs() < f32::EPSILON); // never above white
    assert!(smallest < 0.9); // darkened where the noise is negative
    Ok(())
}

#[test]
fn unclumped_grain_on_rows_of_the_canvas_told_their_place_is_the_same_rows_of_the_whole() -> Result<(), Error> {
    // odd and even sides that differ, so that a row and a column cannot be swapped
    let (height, width) = (37, 20);
    let picture = varied(height, width);
    for distribution in [Distribution::Gaussian, Distribution::Uniform] {
        for (size, color) in [(0.0, false), (1.0, true)] {
            let params = Params {
                amount: 0.2,
                size,
                color,
                distribution,
                seed: 5,
                ..Params::default()
            };
            let whole = run_picture(&params, height, width, &picture)?;
            for (from, to) in [(0, 9), (13, 30), (30, 37), (36, 37)] {
                let band = run_rows(&params, (height, width), &picture, (from, to), true)?;
                assert_eq!(
                    first_bit_difference(&band, &whole[from * width * 3..to * width * 3]),
                    None,
                    "rows {from}..{to} told their place, {params:?}"
                );
            }
            // known bad: not told where it lies, the band draws the grain of the top of the canvas
            let alone = run_rows(&params, (height, width), &picture, (13, 30), false)?;
            assert!(
                first_bit_difference(&alone, &whole[13 * width * 3..30 * width * 3]).is_some(),
                "a band not told its place matched the whole: the test proves nothing, {params:?}"
            );
        }
    }
    Ok(())
}
