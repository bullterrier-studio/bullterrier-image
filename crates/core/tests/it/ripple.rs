//! `ripple`: the picture compared with a point-by-point computation from the definition (every reading, every edge rule),
//! hand-worked waves over ramps, and the properties a sideways wave keeps.

#![expect(
    clippy::cast_precision_loss,
    reason = "small picture sizes; the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::effects::ripple::Params;
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
/// The shapes the comparison runs on: a pixel, a row, a column, a few pixels, odd and even sides.
const SHAPES: [(usize, usize); 6] = [(1, 1), (1, 9), (9, 1), (5, 7), (23, 31), (16, 10)];

/// `picture` rippled as `params` says, worked out from the definition: a pixel `s` px from the middle of the picture
/// (between the two middle pixels of an even side) measured along the direction `angle` degrees clockwise from pointing
/// right shows the picture `amplitude x sin(2 pi s / wavelength + phase)` px behind it, at a right angle to that
/// direction (below it for angle 0, to its right for angle 90), read by `interpolation` with `edge` beyond the picture.
fn by_definition(params: &Params, height: usize, width: usize, picture: &[f32]) -> Vec<f32> {
    let (angle, phase) = (
        f64::from(params.angle).to_radians(),
        f64::from(params.phase).to_radians(),
    );
    let middle = ((height - 1) as f64 / 2.0, (width - 1) as f64 / 2.0);
    warped(
        picture,
        height,
        width,
        params.interpolation,
        (params.edge, params.edge),
        |row, column| {
            let (down, across) = (row as f64 - middle.0, column as f64 - middle.1);
            let along = across * angle.cos() + down * angle.sin();
            let moved = f64::from(params.amplitude)
                * (std::f64::consts::TAU * along / f64::from(params.wavelength) + phase).sin();
            (row as f64 - angle.cos() * moved, column as f64 + angle.sin() * moved)
        },
    )
}

#[test]
fn the_picture_is_what_a_point_by_point_computation_from_the_definition_gives() -> Result<(), Error> {
    // (wavelength, amplitude, phase, angle): long and short waves, small and large, both signs of the phase, the four
    // quarters of the angle and a slant
    let sets = [
        (40.0, 6.0, 0.0, 0.0),
        (7.5, 3.0, -120.0, 33.0),
        (2.0, 500.0, 180.0, 360.0),
        (13.0, 2.5, 90.0, 90.0),
        (9.0, 4.0, -45.0, 180.0),
        (21.0, 1.5, 150.0, 270.0),
        (4000.0, 8.0, 17.0, 123.0),
    ];
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            for (wavelength, amplitude, phase, angle) in sets {
                let params = Params {
                    wavelength,
                    amplitude,
                    phase,
                    angle,
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
fn the_comparison_tells_ripples_that_differ_apart() -> Result<(), Error> {
    // the known-bad rows: another wavelength, amplitude, phase, direction, reading or edge rule is not this ripple; and
    // the ripple is not nothing, so the differences show
    let picture = varied(23, 31);
    let params = Params {
        wavelength: 9.0,
        amplitude: 4.0,
        phase: 40.0,
        angle: 30.0,
        interpolation: Interpolation::Bilinear,
        edge: Edge::Wrap,
    };
    let ours = run_picture(&params, 23, 31, &picture)?;
    assert!(
        first_difference_within(&ours, &picture, 0.05).is_some(),
        "the ripple changes the picture"
    );
    for other in [
        Params {
            wavelength: 9.5,
            ..params.clone()
        },
        Params {
            amplitude: 3.5,
            ..params.clone()
        },
        Params {
            phase: 50.0,
            ..params.clone()
        },
        Params {
            angle: 40.0,
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
fn no_amplitude_leaves_the_picture_exactly_as_it_is() -> Result<(), Error> {
    let picture = varied(6, 5);
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            let params = Params {
                amplitude: 0.0,
                phase: 70.0,
                angle: 33.0,
                interpolation,
                edge,
                ..Params::default()
            };
            assert_eq!(run_picture(&params, 6, 5, &picture)?, picture, "{params:?}");
        }
    }
    Ok(())
}

#[test]
fn a_flat_picture_stays_as_flat() -> Result<(), Error> {
    let flat = vec![0.3_f32; 5 * 7 * 3];
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            let params = Params {
                wavelength: 3.0,
                amplitude: 500.0,
                phase: -60.0,
                angle: 123.0,
                interpolation,
                edge,
            };
            let at = first_difference_within(&run_picture(&params, 5, 7, &flat)?, &flat, TOLERANCE);
            assert_eq!(at, None, "{params:?}: first differs at {at:?}");
        }
    }
    Ok(())
}

/// A 17 x 17 grey picture whose value is `value(row, column)`.
fn grey(value: impl Fn(usize, usize) -> f32) -> Vec<f32> {
    (0..17 * 17).flat_map(|at| [value(at / 17, at % 17); 3]).collect()
}

#[test]
fn a_wave_along_the_width_moves_the_columns_up_and_down_by_hand_worked_amounts() -> Result<(), Error> {
    // by hand: a picture whose value is row / 16 (a ramp down the picture, so a linear reading is exact), the middle at
    // column 8, wavelength 16, amplitude 2, angle 0: the wave runs along the width and moves each column up or down. A
    // pixel shows the picture `moved` px behind it, so it shows row - moved: its value is (row - moved) / 16.
    // Phase 0: column 12 (4 px on, a quarter wavelength) is at the crest, moved = +2: 0.375 at row 8; column 4 is at the
    // trough, moved = -2: 0.625; column 8 is at zero: 0.5. Phase 90: the crest is at the middle: column 8 gives 0.375, and
    // columns 4 and 12 (a quarter wave each side of it, where the wave is back at zero) give 0.5.
    let down = grey(|row, _| row as f32 / 16.0);
    let rows = |phase: f32| -> Result<Vec<f32>, Error> {
        let params = Params {
            wavelength: 16.0,
            amplitude: 2.0,
            phase,
            angle: 0.0,
            ..Params::default()
        };
        let out = run_picture(&params, 17, 17, &down)?;
        Ok([4, 8, 12].map(|column| out[(8 * 17 + column) * 3]).to_vec())
    };
    for (phase, expected) in [(0.0, [0.625, 0.5, 0.375]), (90.0, [0.5, 0.375, 0.5])] {
        let got = rows(phase)?;
        let at = first_difference_within(&got, &expected, TOLERANCE);
        assert_eq!(at, None, "phase {phase}: {got:?} for {expected:?}");
    }
    Ok(())
}

#[test]
fn a_wave_down_the_picture_moves_the_rows_left_and_right_by_hand_worked_amounts() -> Result<(), Error> {
    // by hand: a picture whose value is column / 16, wavelength 16, amplitude 2, angle 90: the wave runs down the picture
    // and moves each row sideways; a pixel shows the picture to its right by `moved` (the picture moves left where the
    // wave is positive): its value is (column + moved) / 16. At column 8, phase 0: row 12 is at the crest, moved = +2:
    // 0.625; row 4 is at the trough: 0.375; row 8 is at zero: 0.5.
    let across = grey(|_, column| column as f32 / 16.0);
    let params = Params {
        wavelength: 16.0,
        amplitude: 2.0,
        phase: 0.0,
        angle: 90.0,
        ..Params::default()
    };
    let out = run_picture(&params, 17, 17, &across)?;
    let got = [4, 8, 12].map(|row| out[(row * 17 + 8) * 3]);
    let at = first_difference_within(&got, &[0.375, 0.5, 0.625], TOLERANCE);
    assert_eq!(at, None, "{got:?}");
    Ok(())
}

#[test]
fn a_picture_that_does_not_change_in_the_direction_the_wave_moves_pixels_is_left_as_it_is() -> Result<(), Error> {
    // pixels move at a right angle to the wave's direction: along the columns for angle 0 (down or up), along the rows for
    // angle 90. A picture that is the same all the way down every column, or along every row, does not show it.
    let size = 17;
    let columns = grey(|_, column| (column * column % 11) as f32 / 10.0);
    let rows = grey(|row, _| (row * row % 11) as f32 / 10.0);
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            for (angle, picture) in [(0.0, &columns), (90.0, &rows), (180.0, &columns), (270.0, &rows)] {
                let params = Params {
                    wavelength: 7.0,
                    amplitude: 9.0,
                    phase: 33.0,
                    angle,
                    interpolation,
                    edge,
                };
                let at = first_difference_within(&run_picture(&params, size, size, picture)?, picture, TOLERANCE);
                assert_eq!(at, None, "{params:?}: first differs at {at:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn a_phase_a_whole_cycle_apart_gives_the_same_picture() -> Result<(), Error> {
    // -180 and 180 degrees are a whole cycle apart (the ends of the range of the phase)
    let picture = varied(23, 31);
    for interpolation in INTERPOLATIONS {
        let params = |phase| Params {
            wavelength: 11.0,
            amplitude: 5.0,
            phase,
            angle: 40.0,
            interpolation,
            ..Params::default()
        };
        let at = first_difference_within(
            &run_picture(&params(-180.0), 23, 31, &picture)?,
            &run_picture(&params(180.0), 23, 31, &picture)?,
            TOLERANCE,
        );
        assert_eq!(at, None, "{interpolation:?}: first differs at {at:?}");
    }
    Ok(())
}

#[test]
fn a_pixel_never_moves_farther_than_the_amplitude() -> Result<(), Error> {
    // on a ramp down the picture a linear reading shows the value at (row - moved) with |moved| <= amplitude, so no
    // pixel of the inside (further than the amplitude from the edge) differs from the ramp by more than amplitude / 16
    let down = grey(|row, _| row as f32 / 16.0);
    let amplitude = 2.5;
    let params = Params {
        wavelength: 5.0,
        amplitude,
        angle: 0.0,
        ..Params::default()
    };
    let out = run_picture(&params, 17, 17, &down)?;
    for row in 3..14 {
        for column in 0..17 {
            let at = (row * 17 + column) * 3;
            assert!(
                (out[at] - down[at]).abs() <= amplitude / 16.0 + TOLERANCE,
                "({row}, {column})"
            );
        }
    }
    Ok(())
}

/// Parameters over their whole ranges, the ends included.
fn parameters() -> impl Strategy<Value = Params> {
    (
        prop_oneof![Just(2.0_f32), Just(4000.0_f32), 2.0_f32..=4000.0],
        prop_oneof![Just(0.0_f32), Just(500.0_f32), 0.0_f32..=500.0],
        prop_oneof![Just(-180.0_f32), Just(180.0_f32), -180.0_f32..=180.0],
        prop_oneof![Just(0.0_f32), Just(360.0_f32), 0.0_f32..=360.0],
        0..INTERPOLATIONS.len(),
        0..EDGES.len(),
    )
        .prop_map(|(wavelength, amplitude, phase, angle, interpolation, edge)| Params {
            wavelength,
            amplitude,
            phase,
            angle,
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
