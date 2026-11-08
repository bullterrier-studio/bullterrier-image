//! `aurora`: a curtain of light over a sky. With a level base (`waviness` 0) and no rays the picture is a function of the row
//! alone and every value is worked out by hand from the documented brightness `(1 - d)^falloff` at `d` of the way up. The
//! wobble of the base has the deviation `waviness` and the rays' brightness is the normal distribution of a standardised noise,
//! which statistics over a wide picture at a fixed seed show.

use bullterrier_image::Error;
use bullterrier_image::effects::aurora::Params;
use bullterrier_image::params::Color;

use crate::common::compare::{assert_close, first_bit_difference};
use crate::common::numbers::float;
use crate::common::pixels::run_sized;
use crate::common::stats::{correlation, deviation, mean, neighbour_correlation, normal_quantile};

const RED: [f32; 3] = [1.0, 0.0, 0.0];
const BLUE: [f32; 3] = [0.0, 0.0, 1.0];

/// A level, ray-less curtain 40 px tall whose base is at row 75 of 100, brightness falling with the square of the way down,
/// red at its base turning blue at its top, over a dark blue-grey sky.
fn curtain() -> Params {
    Params {
        colors: vec![Color(RED), Color(BLUE)],
        sky: Color([0.1, 0.2, 0.3]),
        base: 0.75,
        height: 40.0,
        falloff: 2.0,
        waviness: 0.0,
        rays: 0.0,
        ..Params::default()
    }
}

#[test]
fn a_level_curtain_has_the_brightness_and_colour_the_documentation_works_out_row_by_row() -> Result<(), Error> {
    let out = run_sized(&curtain(), 100, 7, 0.0)?;
    let sky = [0.1, 0.2, 0.3];
    // (row, brightness, how far up the curtain): the base row is half covered (the edge is anti-aliased over 1 px),
    // above it the light fades as (1 - d)^2 to nothing at d = 1, below it is sky
    let rows: [(usize, f32, f32); 6] = [
        (80, 0.0, 0.0),
        (76, 0.0, 0.0),
        (75, 0.5, 0.0),
        (60, 0.390_625, 0.375),
        (45, 0.0625, 0.75),
        (35, 0.0, 1.0),
    ];
    for (row, brightness, up) in rows {
        let colour = [1.0 - up, 0.0, up];
        let expected = [0, 1, 2].map(|channel| sky[channel] + brightness * (colour[channel] - sky[channel]));
        for column in 0..7 {
            assert_close(
                &out[row * 7 + column],
                &expected,
                1e-5,
                &format!("row {row}, column {column}"),
            );
        }
    }
    // above the curtain's top: sky again
    assert_close(&out[34 * 7], &sky, 1e-6, "row 34");
    assert_close(&out[0], &sky, 1e-6, "row 0");
    Ok(())
}

#[test]
fn colours_are_spread_evenly_up_the_curtain() -> Result<(), Error> {
    // three colours: red at the base, green half way up, blue at the top; the sky is black and the falloff 0.2 keeps the
    // brightness near 1 low in the curtain, so the colour shows as it is
    let params = Params {
        colors: vec![Color(RED), Color([0.0, 1.0, 0.0]), Color(BLUE)],
        sky: Color([0.0; 3]),
        height: 4000.0,
        falloff: 0.2,
        base: 0.9,
        ..curtain()
    };
    let out = run_sized(&params, 1000, 3, 0.0)?;
    // row 900 is the base (up = 0), row 500 is 400 px up of 4000 (up = 0.1, 20 % of the way to the middle colour)
    let brightness = |up: f32| (1.0 - up).powf(0.2);
    let at = |row: usize, up: f32, colour: [f32; 3]| {
        let expected = colour.map(|level| level * brightness(up));
        assert_close(&out[row * 3], &expected, 1e-4, &format!("row {row}"));
    };
    at(500, 0.1, [0.8, 0.2, 0.0]);
    at(100, 0.2, [0.6, 0.4, 0.0]);
    Ok(())
}

#[test]
fn the_base_wanders_with_the_waviness_as_its_deviation_around_the_base_line() -> Result<(), Error> {
    // a long curtain of one grey colour reaching far up, so a column is lit down to its base and dark below it: the number of
    // lit rows in a column is the base's row, give or take the half-lit one
    let params = Params {
        colors: vec![Color([1.0; 3]), Color([1.0; 3])],
        sky: Color([0.0; 3]),
        base: 0.5,
        height: 4000.0,
        falloff: 0.2,
        waviness: 20.0,
        scale: 30.0,
        octaves: 2,
        seed: 3,
        rays: 0.0,
        ..Params::default()
    };
    let (rows, width) = (200, 1500);
    let out = run_sized(&params, rows, width, 0.0)?;
    let bases: Vec<f64> = (0..width)
        .map(|column| float((0..rows).filter(|&row| out[row * width + column][0] > 0.0).count()) - 0.5)
        .collect();
    assert!((mean(&bases) - 100.0).abs() < 4.0, "mean base row {}", mean(&bases));
    assert!(
        (deviation(&bases) / 20.0 - 1.0).abs() < 0.2,
        "deviation {}",
        deviation(&bases)
    );
    Ok(())
}

#[test]
fn a_ray_is_as_dim_as_rays_says_and_the_columns_brightness_is_the_normal_distribution_of_a_standardised_noise()
-> Result<(), Error> {
    // a level base, a grey curtain, reading one row just above the base where the curtain is at full brightness: a column
    // shows 1 - rays x (1 - rank), the rank being ndtr of a noise of unit deviation
    let width = 4000;
    let params = |rays| Params {
        colors: vec![Color([1.0; 3]), Color([1.0; 3])],
        sky: Color([0.0; 3]),
        base: 0.75,
        height: 4000.0,
        falloff: 0.2,
        waviness: 0.0,
        ray_width: 6.0,
        octaves: 2,
        seed: 8,
        rays,
        ..Params::default()
    };
    let read = |rays| -> Result<Vec<f64>, Error> {
        let out = run_sized(&params(rays), 100, width, 0.0)?;
        // 10 px above the base the vertical fade is (1 - 10 / 4000)^0.2
        let fade = (1.0_f64 - 10.0 / 4000.0).powf(0.2);
        Ok((0..width)
            .map(|column| f64::from(out[65 * width + column][0]) / fade)
            .collect())
    };
    // rays 1: the column brightness is the rank itself, its normal quantile standardised noise
    let full = read(1.0)?;
    let z: Vec<f64> = full
        .iter()
        .map(|rank| normal_quantile(rank.clamp(1e-7, 1.0 - 1e-7)))
        .collect();
    assert!(mean(&z).abs() < 0.01, "mean {}", mean(&z));
    assert!((deviation(&z) - 1.0).abs() < 0.02, "deviation {}", deviation(&z));
    // a weaker ray keeps the same ranking, dimmed to between 1 - rays and 1
    let half = read(0.5)?;
    for (rank, dim) in full.iter().zip(&half) {
        assert!((dim - (1.0 - 0.5 * (1.0 - rank))).abs() < 1e-5);
    }
    // no rays: an even curtain
    let none = read(0.0)?;
    assert!(none.iter().all(|level| (level - 1.0).abs() < 1e-5));
    Ok(())
}

#[test]
fn the_wobble_of_the_base_and_the_rays_are_independent_noises() -> Result<(), Error> {
    // the same lattice spacing for both, so only the seed's streams keep them apart: down a column, how high the base is (the
    // lit rows) tells nothing of how bright the ray there is (a row well above the base, where only the ray dims it)
    let (rows, width) = (200, 1500);
    let params = Params {
        colors: vec![Color([1.0; 3]), Color([1.0; 3])],
        sky: Color([0.0; 3]),
        base: 0.75,
        height: 4000.0,
        falloff: 0.2,
        waviness: 20.0,
        scale: 20.0,
        ray_width: 20.0,
        rays: 1.0,
        octaves: 1,
        seed: 6,
        ..Params::default()
    };
    let out = run_sized(&params, rows, width, 0.0)?;
    let bases: Vec<f64> = (0..width)
        .map(|column| float((0..rows).filter(|&row| out[row * width + column][0] > 0.0).count()))
        .collect();
    let rays: Vec<f64> = (0..width)
        .map(|column| f64::from(out[50 * width + column][0]))
        .collect();
    let alike = correlation(&bases, &rays);
    assert!(alike.abs() < 0.3, "correlation of base and ray {alike}");
    Ok(())
}

#[test]
fn broader_rays_are_alike_over_a_longer_stretch() -> Result<(), Error> {
    let at = |ray_width| -> Result<f64, Error> {
        let params = Params {
            colors: vec![Color([1.0; 3]), Color([1.0; 3])],
            sky: Color([0.0; 3]),
            waviness: 0.0,
            rays: 1.0,
            octaves: 1,
            ray_width,
            seed: 2,
            ..Params::default()
        };
        let out = run_sized(&params, 100, 2000, 0.0)?;
        let row: Vec<f64> = (0..2000).map(|column| f64::from(out[65 * 2000 + column][0])).collect();
        Ok(neighbour_correlation(&row, 2000))
    };
    let (fine, medium, broad) = (at(2.0)?, at(8.0)?, at(40.0)?);
    assert!(
        fine < medium && medium < broad && broad > 0.9,
        "{fine} {medium} {broad}"
    );
    Ok(())
}

#[test]
fn the_same_seed_gives_the_same_curtain_and_another_seed_another() -> Result<(), Error> {
    let run = |seed| {
        run_sized(
            &Params {
                seed,
                ..Params::default()
            },
            60,
            90,
            0.0,
        )
        .map(|pixels| pixels.into_iter().flatten().collect::<Vec<f32>>())
    };
    assert_eq!(first_bit_difference(&run(1)?, &run(1)?), None);
    assert!(first_bit_difference(&run(1)?, &run(2)?).is_some());
    assert!(first_bit_difference(&run(u32::MAX)?, &run(u32::MAX - 1)?).is_some());
    Ok(())
}
