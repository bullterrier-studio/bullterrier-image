//! `melt`: every pixel shows its own column higher up by `amount` x the column's drip length x its brightness. The drip
//! lengths are read back through a grey ramp down the picture (what a column shows at the last row is worked out by hand),
//! and the documented laws are checked on them: the normal cumulative distribution of noise of mean 0 and deviation 1,
//! smooth over `scale`; brightness is Rec. 709 luminance. A picture of any other kind is then compared with a
//! point-by-point computation from the definition, every reading and every edge rule.

#![expect(
    clippy::cast_precision_loss,
    reason = "small picture sizes; the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::effects::melt::Params;
use bullterrier_image::{Edge, Error, Interpolation};

use crate::common::compare::{first_bit_difference, first_difference_within};
use crate::common::pictures::varied;
use crate::common::pixels::run_picture;
use crate::common::sampling::warped;
use crate::common::stats::{correlation, deviation, mean, normal_quantile};

const INTERPOLATIONS: [Interpolation; 3] = [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic];
const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];
/// The rows of a ramp picture: the value at row r is r / 64, exact in a float.
const RAMP: usize = 65;
/// Rec. 709 luminance weights of red, green and blue (the standard's, not the core's).
const LUMA: [f64; 3] = [0.2126, 0.7152, 0.0722];

/// A `RAMP` x `width` picture whose channel `channel` rises from 0 on the first row to 1 on the last, the others 0 (all three
/// if `channel` is 3).
fn ramp(width: usize, channel: usize) -> Vec<f32> {
    (0..RAMP * width)
        .flat_map(|at| {
            let level = (at / width) as f32 / 64.0;
            [0, 1, 2].map(|which| if channel == 3 || which == channel { level } else { 0.0 })
        })
        .collect()
}

/// What `params` makes of a ramp in `channel` at its last row, from the definition: that pixel's brightness is `weight` x 1, it
/// is pulled `amount x drip x weight` px and shows the ramp at row `64 - pulled`, which is `(64 - pulled) / 64` as long as
/// that row is on the picture. So a column's `drip x weight` is `(1 - shown) x 64 / amount`; `weight` is 1 for a grey ramp,
/// the `channel`'s luminance weight for a ramp in one channel (the read-back is of that one channel).
fn pull_at_last_row(params: &Params, width: usize, channel: usize) -> Result<Vec<f64>, Error> {
    let params = Params {
        interpolation: Interpolation::Bilinear,
        edge: Edge::Nearest,
        ..params.clone()
    };
    let out = run_picture(&params, RAMP, width, &ramp(width, channel))?;
    let last = (RAMP - 1) * width;
    Ok((0..width)
        .map(|column| {
            let shown = f64::from(out[(last + column) * 3 + channel.min(2)]);
            (1.0 - shown) * 64.0 / f64::from(params.amount)
        })
        .collect())
}

/// The drip length of each column of a `width` picture, read back from a grey ramp (every channel the ramp, brightness 1 at the
/// last row).
fn drips(params: &Params, width: usize) -> Result<Vec<f64>, Error> {
    let lengths = pull_at_last_row(params, width, 3)?;
    // the other tests take these as their oracle: a melt that pulled nothing would make them agree with anything
    assert!(
        mean(&lengths) > 0.2,
        "the ramp is pulled: mean length {}",
        mean(&lengths)
    );
    Ok(lengths)
}

#[test]
fn a_picture_one_column_wide_has_the_middle_drip_length_by_hand() -> Result<(), Error> {
    // one column: the noise has no variation, so it is all zeros; the normal distribution of 0 is 1/2. amount 20 pulls the
    // white last row 10 px, so it shows the ramp at row 54: 54 / 64
    let params = Params {
        amount: 20.0,
        ..Params::default()
    };
    let out = run_picture(&params, RAMP, 1, &ramp(1, 3))?;
    let shown = out[(RAMP - 1) * 3];
    assert!((shown - 54.0 / 64.0).abs() < 1e-6, "{shown}");
    Ok(())
}

#[test]
fn the_drip_lengths_are_the_normal_distribution_of_noise_of_mean_zero_and_deviation_one() -> Result<(), Error> {
    let params = Params {
        amount: 40.0,
        scale: 6.0,
        seed: 3,
        ..Params::default()
    };
    let lengths = drips(&params, 2000)?;
    assert!(lengths.iter().all(|u| (0.0..=1.0).contains(u)), "lengths lie in 0..1");
    // the noise is standardised over the row, so the quantiles of the lengths are exactly mean 0 and deviation 1
    let z: Vec<f64> = lengths
        .iter()
        .map(|u| normal_quantile(u.clamp(1e-7, 1.0 - 1e-7)))
        .collect();
    assert!(mean(&z).abs() < 0.01, "mean {}", mean(&z));
    assert!((deviation(&z) - 1.0).abs() < 0.02, "deviation {}", deviation(&z));
    // and so the lengths spread evenly
    for quarter in [0.25, 0.5, 0.75] {
        let below = lengths.iter().filter(|u| **u < quarter).count() as f64 / 2000.0;
        assert!(
            (below - quarter).abs() < 0.06,
            "{below} of the columns are under {quarter}"
        );
    }
    Ok(())
}

#[test]
fn neighbouring_columns_drip_alike_over_about_the_scale() -> Result<(), Error> {
    let neighbours = |scale: f32| -> Result<f64, Error> {
        let params = Params {
            amount: 40.0,
            scale,
            seed: 6,
            ..Params::default()
        };
        let lengths = drips(&params, 2000)?;
        Ok(correlation(&lengths[..1999], &lengths[1..]))
    };
    let (broad, fine) = (neighbours(60.0)?, neighbours(1.0)?);
    assert!(broad > 0.98, "broad {broad}");
    assert!(fine < 0.5, "fine {fine}");
    Ok(())
}

#[test]
fn the_seed_decides_the_drips() -> Result<(), Error> {
    let params = |seed| Params {
        amount: 40.0,
        scale: 5.0,
        seed,
        ..Params::default()
    };
    let one = drips(&params(1), 200)?;
    assert_eq!(one, drips(&params(1), 200)?);
    assert!(correlation(&one, &drips(&params(2), 200)?).abs() < 0.5);
    Ok(())
}

#[test]
fn brightness_is_rec_709_luminance() -> Result<(), Error> {
    // a ramp in one channel is pulled by that channel's weight of the grey ramp's pull
    let params = Params {
        amount: 30.0,
        scale: 8.0,
        seed: 1,
        ..Params::default()
    };
    let grey = drips(&params, 40)?;
    for (channel, weight) in LUMA.into_iter().enumerate() {
        let one = pull_at_last_row(&params, 40, channel)?;
        for (column, (alone, all)) in one.iter().zip(&grey).enumerate() {
            assert!(
                (alone - weight * all).abs() < 1e-4,
                "channel {channel}, column {column}: {alone} for {weight} x {all}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_pixel_is_pulled_in_proportion_to_its_own_brightness() -> Result<(), Error> {
    // on the grey ramp the brightness at row r is r / 64, so row r shows (r - pulled x r / 64) / 64: the same drip length
    // read at any row
    let params = Params {
        amount: 24.0,
        scale: 5.0,
        seed: 9,
        ..Params::default()
    };
    let lengths = drips(&params, 30)?;
    let out = run_picture(
        &Params {
            interpolation: Interpolation::Bilinear,
            edge: Edge::Nearest,
            ..params.clone()
        },
        RAMP,
        30,
        &ramp(30, 3),
    )?;
    for row in [16, 32, 48] {
        for (column, length) in lengths.iter().enumerate() {
            let shown = f64::from(out[(row * 30 + column) * 3]);
            let expected = (row as f64 - 24.0 * length * row as f64 / 64.0) / 64.0;
            assert!((shown - expected).abs() < 1e-5, "row {row}, column {column}");
        }
    }
    Ok(())
}

/// `picture` melted by the definition, given the drip length of each column: pixel (row, column) shows the picture at
/// row `row - amount x length x luminance` of the same column.
fn by_definition(params: &Params, lengths: &[f64], height: usize, width: usize, picture: &[f32]) -> Vec<f32> {
    warped(
        picture,
        height,
        width,
        params.interpolation,
        (params.edge, params.edge),
        |row, column| {
            let at = (row * width + column) * 3;
            let bright: f64 = LUMA
                .iter()
                .zip(&picture[at..at + 3])
                .map(|(w, v)| w * f64::from(*v))
                .sum();
            (
                row as f64 - f64::from(params.amount) * lengths[column] * bright,
                column as f64,
            )
        },
    )
}

#[test]
fn the_picture_is_what_a_point_by_point_computation_from_the_definition_gives() -> Result<(), Error> {
    for (amount, scale, seed) in [(20.0, 5.0, 4), (500.0, 1.0, 0), (7.5, 200.0, 4_294_967_295)] {
        let reference = Params {
            amount,
            scale,
            seed,
            ..Params::default()
        };
        for (height, width) in [(1, 9), (17, 1), (17, 24), (40, 13)] {
            // the drip lengths of this width, read back once from a ramp (they do not depend on the amount, the
            // interpolation or the edge; a pull must stay on the ramp for the read-back, so not 500 px)
            let lengths = drips(
                &Params {
                    amount: 20.0,
                    ..reference.clone()
                },
                width,
            )?;
            for interpolation in INTERPOLATIONS {
                for edge in EDGES {
                    let params = Params {
                        interpolation,
                        edge,
                        ..reference.clone()
                    };
                    let picture = varied(height, width);
                    let at = first_difference_within(
                        &run_picture(&params, height, width, &picture)?,
                        &by_definition(&params, &lengths, height, width, &picture),
                        1e-4,
                    );
                    assert_eq!(at, None, "{params:?}, {height} x {width}: first differs at {at:?}");
                }
            }
        }
    }
    Ok(())
}

#[test]
fn the_comparison_tells_melts_that_differ_apart() -> Result<(), Error> {
    let params = Params {
        amount: 20.0,
        scale: 5.0,
        seed: 4,
        interpolation: Interpolation::Bilinear,
        edge: Edge::Wrap,
    };
    let picture = varied(17, 24);
    let ours = run_picture(&params, 17, 24, &picture)?;
    let lengths = drips(&params, 24)?;
    for other in [
        Params {
            amount: 19.0,
            ..params.clone()
        },
        Params {
            interpolation: Interpolation::Nearest,
            ..params.clone()
        },
        Params {
            edge: Edge::Mirror,
            ..params.clone()
        },
    ] {
        assert!(
            first_difference_within(&ours, &by_definition(&other, &lengths, 17, 24, &picture), 1e-4).is_some(),
            "{other:?}"
        );
    }
    assert!(
        first_difference_within(&ours, &picture, 0.05).is_some(),
        "the melt changes the picture"
    );
    Ok(())
}

#[test]
fn no_amount_and_black_pixels_leave_the_picture_where_it_is() -> Result<(), Error> {
    let picture = varied(9, 11);
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            let params = Params {
                amount: 0.0,
                interpolation,
                edge,
                ..Params::default()
            };
            assert_eq!(
                first_bit_difference(&run_picture(&params, 9, 11, &picture)?, &picture),
                None
            );
        }
    }
    // a black picture has no brightness to pull by, whatever the amount
    let black = vec![0.0_f32; 9 * 11 * 3];
    let out = run_picture(&Params::default(), 9, 11, &black)?;
    assert_eq!(first_bit_difference(&out, &black), None);
    Ok(())
}

#[test]
fn every_pixel_shows_its_own_column_and_every_value_stays_in_range() -> Result<(), Error> {
    // nearest reading: every output pixel is a pixel of the same column of the input
    let (height, width) = (23, 14);
    let picture = varied(height, width);
    let params = Params {
        amount: 500.0,
        scale: 3.0,
        interpolation: Interpolation::Nearest,
        ..Params::default()
    };
    let out = run_picture(&params, height, width, &picture)?;
    for row in 0..height {
        for column in 0..width {
            let shown = &out[(row * width + column) * 3..][..3];
            let found = (0..height).any(|source| picture[(source * width + column) * 3..][..3] == *shown);
            assert!(found, "({row}, {column}) shows a pixel of another column");
        }
    }
    for interpolation in INTERPOLATIONS {
        let out = run_picture(
            &Params {
                interpolation,
                ..params.clone()
            },
            height,
            width,
            &picture,
        )?;
        assert!(out.iter().all(|value| (0.0..=1.0).contains(value)), "{interpolation:?}");
    }
    Ok(())
}
