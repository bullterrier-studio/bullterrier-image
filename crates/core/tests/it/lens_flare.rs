//! `lens_flare`: the bumps of brightness the doc promises, worked out by hand: (1 - t^2)^2 of a halo or a ghost at t
//! = distance / radius, the ring the same round its radius, ghosts at light + i * spacing * (centre - light) fading
//! linearly, and the flare screened over the picture.

use bullterrier_image::Error;
use bullterrier_image::effects::lens_flare;
use bullterrier_image::params::Color;

use crate::common::pixels::run_sized;

const HEIGHT: usize = 101;
const WIDTH: usize = 201;
const LIGHT: (usize, usize) = (30, 40); // (row, column): x = 40 / 200 = 0.2, y = 30 / 100 = 0.3
const WHITE: Color = Color([1.0; 3]);
const TOLERANCE: f32 = 1e-5;

/// One feature at a time: white light at `LIGHT`, full amount, the halo, the ring and the ghosts switched off.
fn bare() -> lens_flare::Params {
    lens_flare::Params {
        x: 0.2,
        y: 0.3,
        color: WHITE,
        amount: 1.0,
        halo: 0.0,
        ring: 0.0,
        ghosts: 0,
        ..lens_flare::Params::default()
    }
}

/// The flare of `params` over a picture of grey `fill`, as the red channel of each pixel, row after row.
fn red_channel(params: &lens_flare::Params, fill: f32) -> Result<Vec<f32>, Error> {
    Ok(run_sized(params, HEIGHT, WIDTH, fill)?
        .iter()
        .map(|pixel| pixel[0])
        .collect())
}

fn at(values: &[f32], row: usize, column: usize) -> f32 {
    values[row * WIDTH + column]
}

#[expect(clippy::cast_precision_loss, reason = "test distances are tiny")]
fn distance(row: usize, column: usize, to: (usize, usize)) -> f32 {
    ((row as f32 - to.0 as f32).powi(2) + (column as f32 - to.1 as f32).powi(2)).sqrt()
}

#[test]
fn the_halo_falls_from_one_at_the_light_to_nothing_at_its_radius() -> Result<(), Error> {
    let radius = 44.0;
    let out = red_channel(&lens_flare::Params { halo: radius, ..bare() }, 0.0)?;
    assert!((at(&out, LIGHT.0, LIGHT.1) - 1.0).abs() <= TOLERANCE);
    // 22 px out: (1 - (22 / 44)^2)^2 = 0.5625
    assert!((at(&out, LIGHT.0, LIGHT.1 + 22) - 0.5625).abs() <= TOLERANCE);
    for row in 0..HEIGHT {
        for column in 0..WIDTH {
            let away = distance(row, column, LIGHT);
            if away >= radius {
                assert!(at(&out, row, column) <= TOLERANCE, "({row}, {column}) is {away} px out");
            } else if away < 0.9 * radius {
                assert!(at(&out, row, column) > 0.0, "({row}, {column}) is {away} px out");
            }
        }
    }
    Ok(())
}

#[test]
fn the_ring_is_brightest_on_its_radius_and_dark_at_the_light() -> Result<(), Error> {
    let params = lens_flare::Params {
        ring: 34.0,
        ring_width: 8.0,
        ..bare()
    };
    let out = red_channel(&params, 0.0)?;
    assert!((at(&out, LIGHT.0, LIGHT.1 + 34) - 1.0).abs() <= TOLERANCE);
    // 4 px off the radius, half a width: (1 - 0.5^2)^2 = 0.5625; 8 px off, a whole width: nothing
    assert!((at(&out, LIGHT.0, LIGHT.1 + 38) - 0.5625).abs() <= TOLERANCE);
    assert!(at(&out, LIGHT.0, LIGHT.1 + 42) <= TOLERANCE);
    assert!(at(&out, LIGHT.0, LIGHT.1) <= TOLERANCE);
    Ok(())
}

#[test]
fn ghosts_sit_along_the_line_from_the_light_through_the_centre_each_dimmer_than_the_last() -> Result<(), Error> {
    // from the light (30, 40) to the centre (50, 100) is (20, 60); at spacing 0.5 the ghosts are at (40, 70), (50, 100)
    // and (60, 130), with brightness 3/3, 2/3 and 1/3 of an amount of 0.9
    let params = lens_flare::Params {
        ghosts: 3,
        spacing: 0.5,
        size: 7.0,
        amount: 0.9,
        ..bare()
    };
    let out = red_channel(&params, 0.0)?;
    for ((row, column), peak) in [(40, 70), (50, 100), (60, 130)].into_iter().zip([0.9, 0.6, 0.3]) {
        assert!(
            (at(&out, row, column) - peak).abs() <= TOLERANCE,
            "ghost at ({row}, {column})"
        );
        assert!(
            at(&out, row, column) > at(&out, row, column + 3),
            "ghost at ({row}, {column}) peaks there"
        );
    }
    assert!(at(&out, LIGHT.0, LIGHT.1) <= TOLERANCE, "no ghost on the light itself");
    Ok(())
}

#[test]
fn a_light_at_the_centre_gives_the_ghosts_no_line_so_they_sit_on_it() -> Result<(), Error> {
    let params = lens_flare::Params {
        x: 0.5,
        y: 0.5,
        ghosts: 1,
        spacing: 0.7,
        size: 5.0,
        ..bare()
    };
    let out = red_channel(&params, 0.0)?;
    assert!((at(&out, 50, 100) - 1.0).abs() <= TOLERANCE);
    Ok(())
}

#[test]
fn the_flare_is_screened_over_the_picture_in_the_colour() -> Result<(), Error> {
    // a grey of 0.5 under light 0.4 is 1 - (1 - 0.5)(1 - 0.4) = 0.7
    let params = lens_flare::Params {
        halo: 22.0,
        amount: 0.4,
        ..bare()
    };
    let out = red_channel(&params, 0.5)?;
    assert!((at(&out, LIGHT.0, LIGHT.1) - 0.7).abs() <= TOLERANCE);
    assert_eq!(at(&out, 100, 200), 0.5, "far from the flare the picture is untouched");

    let tinted = lens_flare::Params {
        halo: 22.0,
        amount: 1.0,
        color: Color([1.0, 0.5, 0.0]),
        ..bare()
    };
    let pixel = run_sized(&tinted, HEIGHT, WIDTH, 0.0)?[LIGHT.0 * WIDTH + LIGHT.1];
    assert_eq!(pixel, [1.0, 0.5, 0.0]);
    Ok(())
}

#[test]
fn the_picture_only_gets_brighter_and_an_amount_of_zero_changes_nothing() -> Result<(), Error> {
    let busy = lens_flare::Params {
        ghosts: 12,
        spacing: 3.0,
        x: 1.0,
        y: 1.0,
        amount: 10.0,
        halo: 600.0,
        ring: 600.0,
        size: 300.0,
        ..lens_flare::Params::default()
    };
    let out = run_sized(&busy, HEIGHT, WIDTH, 0.3)?;
    assert!(out.iter().flatten().all(|&value| (0.3..=1.0).contains(&value)));
    assert!(out.iter().flatten().any(|&value| value > 0.3 + 0.01));
    let off = run_sized(&lens_flare::Params { amount: 0.0, ..busy }, HEIGHT, WIDTH, 0.3)?;
    assert!(off.iter().flatten().all(|value| value.to_bits() == 0.3_f32.to_bits()));
    Ok(())
}
