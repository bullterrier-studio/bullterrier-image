//! `radial_blur`: the picture compared with the average of points along each arc or ray worked out from the definition
//! (every reading, every edge rule), hand-worked smears, and the properties a smear about a centre keeps.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "small picture sizes and counts, and the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::effects::radial_blur::Params;
use bullterrier_image::{Edge, Error, Interpolation, Motion};
use proptest::prelude::*;

use crate::common::compare::first_difference_within;
use crate::common::pictures::{ramp, varied};
use crate::common::pixels::run_picture;
use crate::common::sampling::read_at;

/// The pictures are compared at this distance: the reference sums in f64 and the core stores f32.
const TOLERANCE: f32 = 1e-5;

const MOTIONS: [Motion; 2] = [Motion::Zoom, Motion::Spin];
const INTERPOLATIONS: [Interpolation; 3] = [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic];
const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];

/// The lowest and highest value of the picture.
fn range(picture: &[f32]) -> (f32, f32) {
    picture
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), &value| {
            (low.min(value), high.max(value))
        })
}

/// `picture` smeared as `params` says, worked out from the definition: the centre is at fraction (`x`, `y`) of the way
/// from the first to the last pixel; a pixel is the average of `samples` points that lie, the middles of that many equal
/// parts of the smear, at scales `1 + offset` of its offset from the centre ('zoom', the offsets spread over `amount`) or
/// at angles `offset` round the centre ('spin', spread over `amount` turns); each point is read by `interpolation` with
/// `edge` beyond the picture, and the average is held to the picture's own range.
fn by_definition(params: &Params, height: usize, width: usize, picture: &[f32]) -> Vec<f32> {
    let centre = (
        f64::from(params.y) * (height - 1) as f64,
        f64::from(params.x) * (width - 1) as f64,
    );
    let count = f64::from(params.samples);
    let reach = match params.mode {
        Motion::Zoom => f64::from(params.amount),
        Motion::Spin => std::f64::consts::TAU * f64::from(params.amount),
    };
    let (low, high) = range(picture);
    let mut out = Vec::with_capacity(picture.len());
    for at in 0..height * width {
        let (down, across) = ((at / width) as f64 - centre.0, (at % width) as f64 - centre.1);
        let mut sum = [0.0; 3];
        for k in 0..params.samples {
            let offset = reach * ((f64::from(k) + 0.5) / count - 0.5);
            let (row, column) = match params.mode {
                Motion::Zoom => (centre.0 + (1.0 + offset) * down, centre.1 + (1.0 + offset) * across),
                Motion::Spin => (
                    centre.0 + offset.cos() * down + offset.sin() * across,
                    centre.1 - offset.sin() * down + offset.cos() * across,
                ),
            };
            let read = read_at(picture, height, width, params.interpolation, params.edge, row, column);
            for (total, level) in sum.iter_mut().zip(read) {
                *total += level / count;
            }
        }
        out.extend(sum.map(|total| (total as f32).clamp(low, high)));
    }
    out
}

/// The shapes the comparison runs on: a pixel, a row, a column, a few pixels, a picture wider than the smear is long.
const SHAPES: [(usize, usize); 5] = [(1, 1), (1, 9), (9, 1), (5, 7), (23, 31)];

#[test]
fn the_picture_is_what_a_point_by_point_computation_from_the_definition_gives() -> Result<(), Error> {
    // (amount, x, y, samples): a centre in the middle, at two corners and off the pixel grid; a smear short and a whole
    // turn; two readings to a dozen
    let sets = [
        (0.3, 0.5, 0.5, 8),
        (1.0, 0.0, 0.0, 5),
        (0.65, 1.0, 0.3, 12),
        (0.1, 0.37, 1.0, 3),
        (0.8, 0.5, 0.5, 2),
    ];
    for mode in MOTIONS {
        for interpolation in INTERPOLATIONS {
            for edge in EDGES {
                for (amount, x, y, samples) in sets {
                    let params = Params {
                        mode,
                        amount,
                        x,
                        y,
                        samples,
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
    }
    Ok(())
}

#[test]
fn the_comparison_tells_smears_that_differ_apart() -> Result<(), Error> {
    // the known-bad rows: another amount, centre, number of readings, direction or edge rule is not this smear; and the
    // smear is not nothing, so the differences show
    let picture = varied(23, 31);
    let params = Params {
        mode: Motion::Zoom,
        amount: 0.4,
        x: 0.3,
        y: 0.6,
        samples: 6,
        interpolation: Interpolation::Bilinear,
        edge: Edge::Wrap,
    };
    let ours = run_picture(&params, 23, 31, &picture)?;
    assert!(
        first_difference_within(&ours, &picture, 0.05).is_some(),
        "the smear changes the picture"
    );
    for other in [
        Params {
            amount: 0.35,
            ..params.clone()
        },
        Params {
            x: 0.4,
            ..params.clone()
        },
        Params {
            y: 0.5,
            ..params.clone()
        },
        Params {
            samples: 7,
            ..params.clone()
        },
        Params {
            mode: Motion::Spin,
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
            first_difference_within(&ours, &by_definition(&other, 23, 31, &picture), TOLERANCE).is_some(),
            "{other:?}"
        );
    }
    Ok(())
}

#[test]
fn no_amount_and_a_single_reading_leave_the_picture_exactly_as_it_is() -> Result<(), Error> {
    let picture = varied(6, 5);
    for mode in MOTIONS {
        for interpolation in INTERPOLATIONS {
            for edge in EDGES {
                for (amount, samples) in [(0.0, 8), (0.7, 1), (0.0, 1)] {
                    let params = Params {
                        mode,
                        amount,
                        samples,
                        interpolation,
                        edge,
                        ..Params::default()
                    };
                    assert_eq!(run_picture(&params, 6, 5, &picture)?, picture, "{params:?}");
                }
            }
        }
    }
    Ok(())
}

#[test]
fn a_flat_picture_stays_exactly_as_flat() -> Result<(), Error> {
    let flat = vec![0.3_f32; 5 * 7 * 3];
    for mode in MOTIONS {
        for interpolation in INTERPOLATIONS {
            for edge in EDGES {
                let params = Params {
                    mode,
                    amount: 1.0,
                    x: 0.2,
                    y: 0.9,
                    samples: 9,
                    interpolation,
                    edge,
                };
                assert_eq!(run_picture(&params, 5, 7, &flat)?, flat, "{params:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn a_zoom_leaves_a_linear_ramp_as_it_is_wherever_every_reading_lies_inside_the_picture() -> Result<(), Error> {
    // by hand: a point at scale s reads the ramp at centre + s x offset, which is the ramp's value there plus (s - 1) x its
    // slope x the offset; the scales average to 1, so the average is the pixel's own value. A pixel within 8 of the middle
    // of a 21 x 21 picture reads at most 8 x 1.1 = 8.8 away from it, inside. (A cubic and a linear reading are exact on a
    // ramp; the nearest is not.)
    let size = 21;
    let across = ramp(size, size);
    let down: Vec<f32> = (0..size)
        .flat_map(|row| (0..size).flat_map(move |_| [row as f32 / (size - 1) as f32; 3]))
        .collect();
    for picture in [across, down] {
        for interpolation in [Interpolation::Bilinear, Interpolation::Bicubic] {
            let params = Params {
                mode: Motion::Zoom,
                amount: 0.2,
                samples: 7,
                interpolation,
                ..Params::default()
            };
            let out = run_picture(&params, size, size, &picture)?;
            for row in 2..19 {
                for column in 2..19 {
                    let at = (row * size + column) * 3;
                    let (down, across) = (row.abs_diff(10), column.abs_diff(10));
                    if down <= 8 && across <= 8 {
                        assert!(
                            (out[at] - picture[at]).abs() < TOLERANCE,
                            "{interpolation:?} ({row}, {column}): {} for {}",
                            out[at],
                            picture[at]
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn a_zoom_keeps_the_neighbourhood_of_the_centre_sharp_and_blurs_the_rim() -> Result<(), Error> {
    // by hand: columns 0..21 alternate black (even) and white (odd), one row, centre at column 10. Amount 0.4 with four
    // readings are the scales 0.85, 0.95, 1.05 and 1.15, read by the nearest pixel. Next to the centre every reading falls
    // on the pixel itself: column 11 (offset 1) reads 10.85, 10.95, 11.05 and 11.15, all column 11, and column 12 reads
    // 11.7, 11.9, 12.1 and 12.3, all column 12. At offset 8 they are 16.8, 17.6, 18.4 and 19.2: columns 17, 18, 18 and 19,
    // white, black, black and white: grey 1/2. On the other side 3.2, 2.4, 1.6 and 0.8: columns 3, 2, 2, 1: grey 1/2 too.
    let stripes: Vec<f32> = (0..21).flat_map(|column| [(column % 2) as f32; 3]).collect();
    let params = Params {
        mode: Motion::Zoom,
        amount: 0.4,
        samples: 4,
        interpolation: Interpolation::Nearest,
        ..Params::default()
    };
    let out = run_picture(&params, 1, 21, &stripes)?;
    let red = |column: usize| out[column * 3];
    for column in 8..=12 {
        assert!(
            (red(column) - (column % 2) as f32).abs() < 1e-6,
            "column {column} is sharp: {}",
            red(column)
        );
    }
    for column in [2, 18] {
        assert!(
            (red(column) - 0.5).abs() < 1e-6,
            "column {column} is blurred: {}",
            red(column)
        );
    }
    Ok(())
}

#[test]
fn the_centre_pixel_never_moves() -> Result<(), Error> {
    let size = 21;
    let picture = varied(size, size);
    let centre = (size / 2 * size + size / 2) * 3;
    for mode in MOTIONS {
        for interpolation in INTERPOLATIONS {
            for edge in EDGES {
                let params = Params {
                    mode,
                    amount: 0.9,
                    samples: 11,
                    interpolation,
                    edge,
                    ..Params::default()
                };
                let out = run_picture(&params, size, size, &picture)?;
                assert_eq!(out[centre..centre + 3], picture[centre..centre + 3], "{params:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn a_spin_round_a_whole_turn_of_four_readings_looks_the_same_turned_by_a_quarter() -> Result<(), Error> {
    // the readings are 45 degrees past a quarter-turn grid and a quarter turn carries each to the next, so a pixel and the
    // pixel a quarter turn round the middle of an odd square average the same four points, whatever the picture is
    // and whatever lies beyond its edge
    let size = 21;
    let picture = varied(size, size);
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            let params = Params {
                mode: Motion::Spin,
                amount: 1.0,
                samples: 4,
                interpolation,
                edge,
                ..Params::default()
            };
            let out = run_picture(&params, size, size, &picture)?;
            let middle = size / 2;
            for row in 0..size {
                for column in 0..size {
                    // the pixel a quarter turn round: (down, across) becomes (across, -down)
                    let (down, across) = (row as isize - middle as isize, column as isize - middle as isize);
                    let turned = ((middle as isize + across) as usize * size + (middle as isize - down) as usize) * 3;
                    let at = (row * size + column) * 3;
                    assert!(
                        (out[at] - out[turned]).abs() < TOLERANCE,
                        "{interpolation:?} {edge:?} ({row}, {column}): {} and {}",
                        out[at],
                        out[turned]
                    );
                }
            }
        }
    }
    Ok(())
}

/// Parameters over their whole ranges, the ends included.
fn parameters() -> impl Strategy<Value = Params> {
    let fraction = prop_oneof![Just(0.0_f32), Just(1.0_f32), 0.0_f32..=1.0];
    (
        0..MOTIONS.len(),
        fraction.clone(),
        fraction.clone(),
        fraction,
        prop_oneof![Just(1), Just(2), Just(256), 1..=256],
        0..INTERPOLATIONS.len(),
        0..EDGES.len(),
    )
        .prop_map(|(mode, amount, x, y, samples, interpolation, edge)| Params {
            mode: MOTIONS[mode],
            amount,
            x,
            y,
            samples,
            interpolation: INTERPOLATIONS[interpolation],
            edge: EDGES[edge],
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn at_every_end_of_every_range_the_picture_is_the_definitions_and_stays_in_its_own_range(
        params in parameters(),
        height in 1_usize..6,
        width in 1_usize..9,
        picture in prop::collection::vec(0.0_f32..=1.0, 5 * 8 * 3),
    ) {
        let picture = &picture[..height * width * 3];
        let out = run_picture(&params, height, width, picture)
            .map_err(|error: Error| TestCaseError::fail(error.to_string()))?;
        let (low, high) = range(picture);
        prop_assert_eq!(out.len(), picture.len());
        prop_assert!(out.iter().all(|value| (low..=high).contains(value)), "within the picture's range");
        let at = first_difference_within(&out, &by_definition(&params, height, width, picture), TOLERANCE);
        prop_assert_eq!(at, None, "{:?}: first differs at {:?}", params, at);
    }
}
