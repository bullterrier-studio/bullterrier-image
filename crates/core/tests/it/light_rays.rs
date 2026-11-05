//! `light_rays`: the picture compared with highlights gathered along the line to the light and screened over it, worked
//! out from the definition (every reading, every edge rule), a hand-worked ray, and the properties a screen keeps.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "small picture sizes and counts, and the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::effects::light_rays::Params;
use bullterrier_image::{Edge, Error, Interpolation};
use proptest::prelude::*;

use crate::common::compare::first_difference_within;
use crate::common::pictures::{impulse, varied};
use crate::common::pixels::run_picture;
use crate::common::sampling::read_at;

/// The pictures are compared at this distance: the reference sums in f64 and the core stores f32.
const TOLERANCE: f32 = 1e-5;

const INTERPOLATIONS: [Interpolation; 3] = [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic];
const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];

/// The part of `value` above `threshold`, white being 1.
fn above(value: f32, threshold: f32) -> f32 {
    ((value - threshold) / (1.0 - threshold)).clamp(0.0, 1.0)
}

/// `picture` with the rays of `params` worked out from the definition: the light is at fraction (`x`, `y`) of the way from
/// the first to the last pixel; each pixel gathers, from `samples` points on the line to the light at the scales
/// `1 - length x k / samples` of its offset from it (k from 0), the highlights of the picture (each channel's part above
/// `threshold`) read by `interpolation` with `edge` beyond the picture, weighing `decay`^k, the weights scaled to add up to
/// 1; and `amount` times that is screened over the pixel.
fn by_definition(params: &Params, height: usize, width: usize, picture: &[f32]) -> Vec<f32> {
    let bright: Vec<f32> = picture.iter().map(|&value| above(value, params.threshold)).collect();
    let light = (
        f64::from(params.y) * (height - 1) as f64,
        f64::from(params.x) * (width - 1) as f64,
    );
    let count = f64::from(params.samples);
    let weights: Vec<f64> = (0..params.samples).map(|k| f64::from(params.decay).powi(k)).collect();
    let total: f64 = weights.iter().sum();
    let mut out = Vec::with_capacity(picture.len());
    for at in 0..height * width {
        let (down, across) = ((at / width) as f64 - light.0, (at % width) as f64 - light.1);
        let mut ray = [0.0; 3];
        for (k, weight) in (0..params.samples).zip(&weights) {
            let scale = 1.0 + -f64::from(params.length) * f64::from(k) / count;
            let read = read_at(
                &bright,
                height,
                width,
                params.interpolation,
                params.edge,
                light.0 + scale * down,
                light.1 + scale * across,
            );
            for (sum, level) in ray.iter_mut().zip(read) {
                *sum += weight / total * level;
            }
        }
        for (channel, level) in ray.iter().enumerate() {
            let base = picture[at * 3 + channel];
            out.push(base + (params.amount * *level as f32).clamp(0.0, 1.0) * (1.0 - base));
        }
    }
    out
}

/// The shapes the comparison runs on: a pixel, a row, a column, a few pixels, a picture wider than the rays are long.
const SHAPES: [(usize, usize); 5] = [(1, 1), (1, 9), (9, 1), (5, 7), (23, 31)];

#[test]
fn the_picture_is_what_a_point_by_point_computation_from_the_definition_gives() -> Result<(), Error> {
    // (x, y, length, threshold, amount, samples, decay): the light in the middle, in two corners and off the pixel grid; rays
    // from nothing to all the way; two readings to a dozen; weights that fall away, stay level and vanish after the first
    let sets = [
        (0.5, 0.3, 0.5, 0.7, 2.5, 8, 0.9),
        (0.0, 0.0, 1.0, 0.0, 10.0, 5, 1.0),
        (1.0, 1.0, 0.8, 0.99, 3.0, 12, 0.5),
        (0.37, 1.0, 0.2, 0.4, 1.0, 2, 0.0),
        (0.5, 0.5, 0.0, 0.6, 2.0, 4, 0.7),
    ];
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            for (x, y, length, threshold, amount, samples, decay) in sets {
                let params = Params {
                    x,
                    y,
                    length,
                    threshold,
                    amount,
                    samples,
                    decay,
                    interpolation,
                    edge,
                };
                for (height, width) in SHAPES {
                    let picture = varied(height, width);
                    let at = first_difference_within(
                        &run_picture(&params, height, width, &picture)?,
                        &by_definition(&params, height, width, &picture),
                        TOLERANCE,
                    );
                    assert_eq!(at, None, "{params:?}, {height} x {width}: first differs at {at:?}");
                }
            }
        }
    }
    Ok(())
}

#[test]
fn the_comparison_tells_rays_that_differ_apart() -> Result<(), Error> {
    // the known-bad rows: another light, length, threshold, amount, number of readings or decay is not these rays; and the
    // light does not saturate, so the differences show
    let picture = varied(23, 31);
    let params = Params {
        x: 0.3,
        y: 0.6,
        length: 0.6,
        threshold: 0.6,
        amount: 0.8,
        samples: 6,
        decay: 0.8,
        interpolation: Interpolation::Bilinear,
        edge: Edge::Reflect,
    };
    let ours = run_picture(&params, 23, 31, &picture)?;
    let unsaturated = ours
        .iter()
        .zip(&picture)
        .filter(|(out, base)| out > base && **out < 0.99)
        .count();
    assert!(unsaturated > 100, "{unsaturated}");
    for other in [
        Params {
            x: 0.4,
            ..params.clone()
        },
        Params {
            y: 0.5,
            ..params.clone()
        },
        Params {
            length: 0.5,
            ..params.clone()
        },
        Params {
            threshold: 0.5,
            ..params.clone()
        },
        Params {
            amount: 0.7,
            ..params.clone()
        },
        Params {
            samples: 7,
            ..params.clone()
        },
        Params {
            decay: 0.7,
            ..params.clone()
        },
        Params {
            interpolation: Interpolation::Nearest,
            ..params.clone()
        },
    ] {
        assert!(
            first_difference_within(&ours, &by_definition(&other, 23, 31, &picture), TOLERANCE).is_some(),
            "{other:?}"
        );
    }
    Ok(())
}

#[test]
fn a_highlight_streaks_away_from_the_light_and_never_toward_it() -> Result<(), Error> {
    // by hand: one row of 21, a white pixel at column 5, black elsewhere, the light at column 0. Two readings of equal
    // weight, at the scales 1 and 3/4 of a pixel's distance from the light, read by the nearest pixel: column p reads
    // columns p and round(3p/4). Column 5 reads 5 and 4, column 6 reads 6 and 5 (4.5 up), column 7 reads 7 and 5 (5.25):
    // half the white each; every other column reads black twice. At full amount and with white over threshold the streak is
    // grey 1/2 on columns 6 and 7, the highlight itself stays white and the rest stays black: none lies between the light
    // and the highlight.
    let params = Params {
        x: 0.0,
        y: 0.0,
        length: 0.5,
        threshold: 0.5,
        amount: 1.0,
        samples: 2,
        decay: 1.0,
        interpolation: Interpolation::Nearest,
        edge: Edge::Reflect,
    };
    let out = run_picture(&params, 1, 21, &impulse(1, 21, 0, 5))?;
    for column in 0..21 {
        let expected = match column {
            5 => 1.0,
            6 | 7 => 0.5,
            _ => 0.0,
        };
        for channel in 0..3 {
            assert!(
                (out[column * 3 + channel] - expected).abs() < 1e-6,
                "column {column}: {}",
                out[column * 3 + channel]
            );
        }
    }
    Ok(())
}

#[test]
fn without_rays_the_highlights_only_brighten_themselves() -> Result<(), Error> {
    // by hand: a single reading (samples 1), readings that reach no way toward the light (length 0) and weights that fall to
    // nothing after the first (decay 0) all read the pixel itself: it is screened with `amount` times its own highlight
    let picture = varied(9, 11);
    let base = Params {
        threshold: 0.6,
        amount: 1.5,
        ..Params::default()
    };
    let expected: Vec<f32> = picture
        .iter()
        .map(|&value| value + (1.5 * above(value, 0.6)).clamp(0.0, 1.0) * (1.0 - value))
        .collect();
    for (what, params) in [
        (
            "samples 1",
            Params {
                samples: 1,
                ..base.clone()
            },
        ),
        (
            "length 0",
            Params {
                length: 0.0,
                ..base.clone()
            },
        ),
        (
            "decay 0",
            Params {
                decay: 0.0,
                ..base.clone()
            },
        ),
    ] {
        let out = run_picture(&params, 9, 11, &picture)?;
        assert_eq!(first_difference_within(&out, &expected, TOLERANCE), None, "{what}");
    }
    Ok(())
}

#[test]
fn the_weights_add_up_to_one_so_an_even_glow_gives_its_own_light_whatever_the_readings() -> Result<(), Error> {
    // by hand: grey 0.85 over threshold 0.7 has the highlight 0.15 / 0.3 = 1/2 in every pixel, so every average of it is 1/2
    // if the weights add up to 1 and the screen at full amount gives 0.85 + 1/2 x 0.15 = 0.925
    let grey = vec![0.85_f32; 6 * 7 * 3];
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            for (samples, decay, length) in [(3, 0.3, 1.0), (8, 1.0, 0.5), (8, 0.9, 0.0), (1, 0.5, 0.7)] {
                let params = Params {
                    x: 0.2,
                    y: 0.7,
                    length,
                    threshold: 0.7,
                    amount: 1.0,
                    samples,
                    decay,
                    interpolation,
                    edge,
                };
                let out = run_picture(&params, 6, 7, &grey)?;
                assert!(
                    out.iter().all(|value| (value - 0.925).abs() < 1e-5),
                    "{params:?}: {}",
                    out[0]
                );
            }
        }
    }
    Ok(())
}

#[test]
fn black_no_amount_and_a_threshold_above_every_value_leave_the_picture_as_it_is() -> Result<(), Error> {
    let black = vec![0.0_f32; 7 * 9 * 3];
    for interpolation in INTERPOLATIONS {
        let params = Params {
            amount: 10.0,
            interpolation,
            ..Params::default()
        };
        assert_eq!(
            run_picture(&params, 7, 9, &black)?,
            black,
            "black stays black, {interpolation:?}"
        );
    }
    let dim: Vec<f32> = varied(7, 9).iter().map(|value| value * 0.5).collect();
    let high = Params {
        threshold: 0.5,
        amount: 10.0,
        ..Params::default()
    };
    assert_eq!(
        first_difference_within(&run_picture(&high, 7, 9, &dim)?, &dim, 0.0),
        None
    );
    let off = Params {
        amount: 0.0,
        ..Params::default()
    };
    assert_eq!(run_picture(&off, 7, 9, &dim)?, dim);
    Ok(())
}

/// Parameters over their whole ranges, the ends included.
fn parameters() -> impl Strategy<Value = Params> {
    let fraction = || prop_oneof![Just(0.0_f32), Just(1.0_f32), 0.0_f32..=1.0];
    (
        (fraction(), fraction(), fraction()),
        prop_oneof![Just(0.0_f32), Just(0.99_f32), 0.0_f32..=0.99],
        prop_oneof![Just(0.0_f32), Just(10.0_f32), 0.0_f32..=10.0],
        prop_oneof![Just(1), Just(2), Just(256), 1..=256],
        fraction(),
        0..INTERPOLATIONS.len(),
        0..EDGES.len(),
    )
        .prop_map(
            |((x, y, length), threshold, amount, samples, decay, interpolation, edge)| Params {
                x,
                y,
                length,
                threshold,
                amount,
                samples,
                decay,
                interpolation: INTERPOLATIONS[interpolation],
                edge: EDGES[edge],
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn at_every_end_of_every_range_the_picture_is_the_definitions_and_only_gets_brighter(
        params in parameters(),
        height in 1_usize..6,
        width in 1_usize..9,
        picture in prop::collection::vec(0.0_f32..=1.0, 5 * 8 * 3),
    ) {
        let picture = &picture[..height * width * 3];
        let out = run_picture(&params, height, width, picture)
            .map_err(|error: Error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(out.len(), picture.len());
        prop_assert!(
            out.iter().zip(picture).all(|(out, base)| out >= base && *out <= 1.0),
            "brighter, at most white"
        );
        let at = first_difference_within(&out, &by_definition(&params, height, width, picture), TOLERANCE);
        prop_assert_eq!(at, None, "{:?}: first differs at {:?}", params, at);
    }
}
