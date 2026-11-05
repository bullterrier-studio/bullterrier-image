//! `gradient` and the conic layout it adds to the core's gradients: positions worked out by hand from the documented
//! directions, how `repeat` and `reflect` fold the colours along them, and that only the picture's size is read.

use bullterrier_image::Error;
use bullterrier_image::effects::gradient::{self, Kind};
use bullterrier_image::params::Color;
use proptest::prelude::*;

use crate::common::compare::assert_close;
use crate::common::pixels::run_sized;

const TOLERANCE: f32 = 1e-5;
const RED: [f32; 3] = [1.0, 0.0, 0.0];
const BLUE: [f32; 3] = [0.0, 0.0, 1.0];

fn red_to_blue(kind: Kind) -> gradient::Params {
    gradient::Params {
        kind,
        colors: vec![Color(RED), Color(BLUE)],
        ..gradient::Params::default()
    }
}

/// The colour a share `t` of the way from red to blue.
fn mix(t: f32) -> [f32; 3] {
    [1.0 - t, 0.0, t]
}

#[test]
fn a_conic_gradient_turns_clockwise_from_its_start_angle_and_is_at_its_start_on_the_start_ray() -> Result<(), Error> {
    // a 101 px square has its centre pixel at (50, 50): the pixels on the axes are whole quarter turns round it
    let at = |angle: f32, x: f32, y: f32, row: usize, column: usize| -> Result<[f32; 3], Error> {
        let params = gradient::Params {
            angle,
            x,
            y,
            ..red_to_blue(Kind::Conic)
        };
        Ok(run_sized(&params, 101, 101, 0.0)?[row * 101 + column])
    };
    for (angle, row, column, turn) in [
        (0.0, 50, 100, 0.0),
        (0.0, 100, 50, 0.25),
        (0.0, 50, 0, 0.5),
        (0.0, 0, 50, 0.75),
        (90.0, 100, 50, 0.0),
        (90.0, 50, 0, 0.25),
        (180.0, 50, 0, 0.0),
        (270.0, 0, 50, 0.0),
    ] {
        assert_close(
            &at(angle, 0.5, 0.5, row, column)?,
            &mix(turn),
            TOLERANCE,
            &format!("angle {angle} at ({row}, {column})"),
        );
    }
    // the centre moves with x and y: at (0.25, 0.75) of 101 px it is at (25.25, 75.75), and the pixel centre (40.5, 75.5)
    // is 15.25 right and 0.25 above it, a turn of atan(0.25 / 15.25) / (2 pi) = 0.002608 short of the start ray
    assert_close(
        &at(0.0, 0.25, 0.75, 75, 40)?,
        &mix(1.0 - 0.002_608),
        TOLERANCE,
        "centre at (0.25, 0.75)",
    );
    Ok(())
}

#[test]
fn repeat_fits_the_colours_in_again_and_reflect_runs_every_second_repeat_backwards() -> Result<(), Error> {
    // one row of 8 px, angle 0: the pixel centres are at 1/16, 3/16 ... of the width
    let row = |repeat, reflect| {
        run_sized(
            &gradient::Params {
                angle: 0.0,
                repeat,
                reflect,
                ..red_to_blue(Kind::Linear)
            },
            1,
            8,
            0.0,
        )
    };
    // the pixel centre at (2 column + 1) / 16 of the width, times `repeat`, in sixteenths: whole sixteenths are the repeat
    // it is in, the rest how far through it
    let shares = |repeat: u8, reflect: bool| -> Vec<f32> {
        (0..8_u8)
            .map(|column| {
                let sixteenths = (2 * column + 1) * repeat;
                let within = f32::from(sixteenths % 16) / 16.0;
                if reflect && (sixteenths / 16) % 2 == 1 {
                    1.0 - within
                } else {
                    within
                }
            })
            .collect()
    };
    for (repeat, reflect) in [(1_u8, false), (2, false), (2, true), (3, true), (4, false)] {
        let expected: Vec<[f32; 3]> = shares(repeat, reflect).into_iter().map(mix).collect();
        let actual = row(i32::from(repeat), reflect)?;
        for (column, (a, e)) in actual.iter().zip(&expected).enumerate() {
            assert_close(
                a,
                e,
                TOLERANCE,
                &format!("repeat {repeat} reflect {reflect} column {column}"),
            );
        }
    }
    Ok(())
}

#[test]
fn a_radial_gradient_beyond_its_radius_stays_on_the_last_colour_however_often_it_repeats() -> Result<(), Error> {
    // the far corner of a 11 px square is past a radius of a tenth of the side; the end of the last repeat is the last colour
    for repeat in [1, 2, 3, 4] {
        let params = gradient::Params {
            radius: 0.1,
            repeat,
            ..red_to_blue(Kind::Radial)
        };
        let corner = run_sized(&params, 11, 11, 0.0)?[0];
        assert_close(&corner, &BLUE, TOLERANCE, &format!("repeat {repeat}"));
    }
    Ok(())
}

proptest! {
    #[test]
    fn only_the_size_of_the_picture_decides_the_gradient_and_every_pixel_is_between_its_colours(
        height in 1_usize..12, width in 1_usize..12, kind in 0_usize..3, angle in 0.0_f32..=360.0,
        x in 0.0_f32..=1.0, y in 0.0_f32..=1.0, repeat in 1_i32..=5, reflect in any::<bool>(),
    ) {
        let params = gradient::Params {
            angle, x, y, repeat, reflect,
            ..red_to_blue([Kind::Linear, Kind::Radial, Kind::Conic][kind])
        };
        let dark = run_sized(&params, height, width, 0.0)?;
        prop_assert_eq!(&dark, &run_sized(&params, height, width, 0.7)?);
        // red falls and blue rises along the ramp, and nothing else is ever in it
        for pixel in dark {
            prop_assert!(pixel[1] == 0.0 && (pixel[0] + pixel[2] - 1.0).abs() <= 1e-6 && pixel[2] >= 0.0 && pixel[0] >= 0.0);
        }
    }
}
