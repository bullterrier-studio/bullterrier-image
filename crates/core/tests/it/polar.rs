//! `polar`: the picture compared with a point-by-point computation from the definition (both ways, every reading and edge
//! rule), hand-worked planets of ramps, the seam of the angle, and the round trip of the two ways.

#![expect(
    clippy::cast_precision_loss,
    reason = "small picture sizes; the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::effects::polar::{Direction, Params};
use bullterrier_image::{Edge, Error, Interpolation};
use proptest::prelude::*;

use crate::common::compare::first_difference_within;
use crate::common::pictures::varied;
use crate::common::pixels::run_picture;
use crate::common::sampling::warped;

/// The pictures are compared at this distance: the reference reads in f64 and the core stores f32.
const TOLERANCE: f32 = 1e-5;

const INTERPOLATIONS: [Interpolation; 3] = [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic];
const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];
const DIRECTIONS: [Direction; 2] = [Direction::To, Direction::From];
/// The shapes the comparison runs on: a pixel, a row, a column, a few pixels, odd and even sides.
const SHAPES: [(usize, usize); 6] = [(1, 1), (1, 9), (9, 1), (5, 7), (23, 31), (16, 10)];

/// `picture` converted as `params` says, worked out from the definition. The planet's radius `reach` is half of the shorter
/// side, first pixel to last. 'to': a pixel `d` px from the centre shows the row `(1 - d / reach)` of the way down the
/// picture (first row to last) and the column that is the angle clockwise from straight up, as a fraction of a turn, of
/// the width; `edge` rules the rows, and the columns wrap round. 'from': a pixel shows the point of the planet that its row
/// names as the distance from the centre (0 on the last row, `reach` on the first) and its column as the angle (a turn
/// over the width), read with `edge` beyond the picture.
fn by_definition(params: &Params, height: usize, width: usize, picture: &[f32]) -> Vec<f32> {
    let reach = (height.min(width) - 1) as f64 / 2.0;
    if reach == 0.0 {
        return picture.to_vec();
    }
    let (last, wide) = ((height - 1) as f64, width as f64);
    let centre = (f64::from(params.y) * last, f64::from(params.x) * (width - 1) as f64);
    let turn = std::f64::consts::TAU;
    match params.mode {
        Direction::To => warped(
            picture,
            height,
            width,
            params.interpolation,
            (params.edge, Edge::Wrap),
            |row, column| {
                let (row, column) = (row as f64, column as f64);
                let (down, across) = (row - centre.0, column - centre.1);
                (
                    (1.0 - down.hypot(across) / reach) * last,
                    across.atan2(-down) / turn * wide,
                )
            },
        ),
        Direction::From => warped(
            picture,
            height,
            width,
            params.interpolation,
            (params.edge, params.edge),
            |row, column| {
                let (row, column) = (row as f64, column as f64);
                let out_of = (1.0 - row / last) * reach;
                let angle = column / wide * turn;
                (centre.0 - out_of * angle.cos(), centre.1 + out_of * angle.sin())
            },
        ),
    }
}

#[test]
fn the_picture_is_what_a_point_by_point_computation_from_the_definition_gives() -> Result<(), Error> {
    // (x, y): the centre in the middle, at a corner and off the pixel grid
    let centres = [(0.5, 0.5), (0.0, 0.0), (0.37, 0.81), (1.0, 0.4)];
    for mode in DIRECTIONS {
        for interpolation in INTERPOLATIONS {
            for edge in EDGES {
                for (x, y) in centres {
                    let params = Params {
                        mode,
                        x,
                        y,
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
fn the_comparison_tells_conversions_that_differ_apart() -> Result<(), Error> {
    // the known-bad rows: another direction, centre, reading or edge rule is not this conversion; and the conversion is
    // not nothing, so the differences show
    let picture = varied(23, 31);
    let params = Params {
        mode: Direction::To,
        x: 0.3,
        y: 0.6,
        interpolation: Interpolation::Bilinear,
        edge: Edge::Mirror,
    };
    let ours = run_picture(&params, 23, 31, &picture)?;
    assert!(
        first_difference_within(&ours, &picture, 0.05).is_some(),
        "the conversion changes the picture"
    );
    for other in [
        Params {
            mode: Direction::From,
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
            interpolation: Interpolation::Nearest,
            ..params.clone()
        },
        Params {
            edge: Edge::Reflect,
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
fn a_picture_with_no_distance_is_left_exactly_as_it_is() -> Result<(), Error> {
    // a picture one pixel thick has no planet
    for mode in DIRECTIONS {
        for (height, width) in [(1, 9), (9, 1), (1, 1)] {
            let picture = varied(height, width);
            let params = Params {
                mode,
                x: 0.3,
                ..Params::default()
            };
            assert_eq!(
                run_picture(&params, height, width, &picture)?,
                picture,
                "{height} x {width}, {params:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_flat_picture_stays_as_flat() -> Result<(), Error> {
    let flat = vec![0.3_f32; 5 * 7 * 3];
    for mode in DIRECTIONS {
        for interpolation in INTERPOLATIONS {
            for edge in EDGES {
                let params = Params {
                    mode,
                    x: 0.2,
                    y: 0.9,
                    interpolation,
                    edge,
                };
                let at = first_difference_within(&run_picture(&params, 5, 7, &flat)?, &flat, TOLERANCE);
                assert_eq!(at, None, "{params:?}: first differs at {at:?}");
            }
        }
    }
    Ok(())
}

/// A 21 x 21 grey picture whose value is `value(row, column)`.
fn grey(value: impl Fn(usize, usize) -> f32) -> Vec<f32> {
    (0..21 * 21).flat_map(|at| [value(at / 21, at % 21); 3]).collect()
}

#[test]
fn a_planet_of_a_ramp_down_the_picture_is_brightest_at_the_centre_and_black_from_the_rim_out() -> Result<(), Error> {
    // by hand: the value is row / 20, so the last row (the planet's centre) is 1 and the first (its rim) is 0. The planet
    // has radius 10 px, and a pixel d px from the centre shows row (1 - d / 10) x 20, value 1 - d / 10 (a linear reading is
    // exact on a ramp); from d = 10 on the nearest edge rule repeats the first row, 0. So the centre pixel is 1, a pixel
    // 5 px from it 0.5 in every direction, one 8 px from it 0.2, and one 10 or more px from it (a corner of the picture,
    // 14.1) 0.
    let down = grey(|row, _| row as f32 / 20.0);
    let out = run_picture(&Params::default(), 21, 21, &down)?;
    let at = |row: usize, column: usize| out[(row * 21 + column) * 3];
    for (row, column, expected) in [
        (10, 10, 1.0),
        (10, 15, 0.5),
        (5, 10, 0.5),
        (10, 5, 0.5),
        (15, 10, 0.5),
        (10, 18, 0.2),
        (10, 20, 0.0),
        (0, 0, 0.0),
        (20, 20, 0.0),
    ] {
        assert!(
            (at(row, column) - expected).abs() < TOLERANCE,
            "({row}, {column}): {}",
            at(row, column)
        );
    }
    Ok(())
}

#[test]
fn the_columns_run_clockwise_round_the_planet_from_straight_up_and_wrap_at_the_seam() -> Result<(), Error> {
    // by hand: the value is column / 21 on 21 columns, so a pixel shows the column that is its angle clockwise from
    // straight up as a fraction of the 21: straight up is column 0, value 0; a quarter turn, to the right, is column 5.25,
    // 0.25; straight down column 10.5, 0.5; to the left, three quarters, column 15.75, 0.75 (a linear reading is exact on
    // a ramp). Just left of straight up, 1 px over and 10 px up from the centre, the angle is -atan(1/10) = -0.0997 rad,
    // that is -0.3331 of a column (times 21 over a turn): it reads between the last column (20 / 21) and the first (0) --
    // the columns wrap round -- with the weight 0.3331 on the last: 0.3331 x 20 / 21 = 0.3173, not 0, which an edge rule
    // at the seam would give.
    let across = grey(|_, column| column as f32 / 21.0);
    #[expect(clippy::cast_possible_truncation, reason = "narrowed to the f32 picture")]
    let seam = ((0.1_f64.atan() * 21.0 / std::f64::consts::TAU) * 20.0 / 21.0) as f32;
    let out = run_picture(&Params::default(), 21, 21, &across)?;
    let at = |row: usize, column: usize| out[(row * 21 + column) * 3];
    for (row, column, expected) in [(0, 10, 0.0), (10, 15, 0.25), (20, 10, 0.5), (10, 5, 0.75), (0, 9, seam)] {
        assert!(
            (at(row, column) - expected).abs() < TOLERANCE,
            "({row}, {column}): {}",
            at(row, column)
        );
    }
    Ok(())
}

#[test]
fn unwrapping_a_planet_gives_back_the_inside_of_the_picture_it_was_wrapped_from() -> Result<(), Error> {
    // 'from' undoes 'to': each point of the planet is the point of the picture at that distance and angle, so unwrapping it
    // finds the picture again, up to two readings between pixels. Away from the planet's rim and from its centre (where
    // many columns share a few pixels) a smooth picture comes back within 0.03; going 'to' twice does not undo it.
    let size = 65;
    let smooth: Vec<f32> = (0..size * size)
        .flat_map(|at| {
            let (row, column) = ((at / size) as f32, (at % size) as f32);
            [0.5 + 0.4 * (row / 9.0).sin() * (column / 11.0).cos(); 3]
        })
        .collect();
    let there = Params {
        mode: Direction::To,
        interpolation: Interpolation::Bicubic,
        ..Params::default()
    };
    let back = Params {
        mode: Direction::From,
        ..there.clone()
    };
    let round = run_picture(&back, size, size, &run_picture(&there, size, size, &smooth)?)?;
    let twice = run_picture(&there, size, size, &run_picture(&there, size, size, &smooth)?)?;
    for row in 3..40 {
        for column in 0..size {
            let at = (row * size + column) * 3;
            assert!((round[at] - smooth[at]).abs() < 0.03, "({row}, {column})");
        }
    }
    let worst = (3..40)
        .flat_map(|row| (0..size).map(move |column| (row * size + column) * 3))
        .map(|at| (twice[at] - smooth[at]).abs())
        .fold(0.0_f32, f32::max);
    assert!(worst > 0.1, "going 'to' twice is not a round trip: {worst}");
    Ok(())
}

/// Parameters over their whole ranges, the ends included.
fn parameters() -> impl Strategy<Value = Params> {
    let fraction = prop_oneof![Just(0.0_f32), Just(1.0_f32), 0.0_f32..=1.0];
    (
        0..DIRECTIONS.len(),
        fraction.clone(),
        fraction,
        0..INTERPOLATIONS.len(),
        0..EDGES.len(),
    )
        .prop_map(|(mode, x, y, interpolation, edge)| Params {
            mode: DIRECTIONS[mode],
            x,
            y,
            interpolation: INTERPOLATIONS[interpolation],
            edge: EDGES[edge],
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn at_every_end_of_every_range_the_picture_is_the_definitions_and_stays_in_range(
        params in parameters(),
        height in 1_usize..6,
        width in 1_usize..9,
        picture in prop::collection::vec(0.0_f32..=1.0, 5 * 8 * 3),
    ) {
        let picture = &picture[..height * width * 3];
        let out = run_picture(&params, height, width, picture)
            .map_err(|error: Error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(out.len(), picture.len());
        prop_assert!(out.iter().all(|value| (0.0..=1.0).contains(value)), "within 0..1");
        let at = first_difference_within(&out, &by_definition(&params, height, width, picture), TOLERANCE);
        prop_assert_eq!(at, None, "{:?}: first differs at {:?}", params, at);
    }
}
