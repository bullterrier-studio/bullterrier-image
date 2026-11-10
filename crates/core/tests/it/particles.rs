//! `particles`: dots and streaks of light screened over the picture. On a black picture the output is the light itself, so
//! the sums of the output are areas, compared with the area of a disc or a stadium worked out by hand. A picture of 100 x
//! 100 px at density 0.1 holds exactly one particle.

use std::f64::consts::PI;

use bullterrier_image::Error;
use bullterrier_image::effects::particles::Params;
use bullterrier_image::params::Color;

use crate::common::compare::first_bit_difference;
use crate::common::pictures::varied;
use crate::common::pixels::{is_pixel, run_picture, run_sized};

const SIDE: usize = 100;

/// One crisp, full-brightness white particle of `size` px across and `length` long, in the 100 x 100 picture of `seed`.
fn lone(size: f32, length: f32, angle: f32, seed: u32) -> Params {
    Params {
        density: 0.1,
        size,
        length,
        angle,
        softness: 0.0,
        layers: 1,
        seed,
        ..Params::default()
    }
}

/// The red channel of `params` over a black picture of `SIDE` x `SIDE`: the light alone.
fn light(params: &Params) -> Result<Vec<f32>, Error> {
    Ok(run_sized(params, SIDE, SIDE, 0.0)?
        .iter()
        .map(|pixel| pixel[0])
        .collect())
}

/// Whether any lit pixel lies on the border of the picture (the particle is cut off).
fn touches_border(light: &[f32]) -> bool {
    (0..SIDE).any(|at| {
        [at, (SIDE - 1) * SIDE + at, at * SIDE, at * SIDE + SIDE - 1]
            .iter()
            .any(|&pixel| light[pixel] > 0.0)
    })
}

/// The sum of `light` over every seed whose particle lies wholly inside, and how many seeds those were, of the first 40.
fn whole_areas(size: f32, length: f32, angle: f32) -> Result<Vec<f64>, Error> {
    let mut areas = Vec::new();
    for seed in 0..40 {
        let cover = light(&lone(size, length, angle, seed))?;
        if !touches_border(&cover) {
            areas.push(cover.iter().map(|level| f64::from(*level)).sum());
        }
    }
    Ok(areas)
}

#[test]
fn no_particles_leave_the_picture_as_it_is() -> Result<(), Error> {
    let picture = varied(13, 17);
    let out = run_picture(
        &Params {
            density: 0.0,
            ..Params::default()
        },
        13,
        17,
        &picture,
    )?;
    assert_eq!(first_bit_difference(&out, &picture), None);
    Ok(())
}

#[test]
fn a_round_particle_is_a_disc_of_its_size_across() -> Result<(), Error> {
    let areas = whole_areas(8.0, 0.0, 90.0)?;
    assert!(areas.len() >= 20, "{} seeds kept the particle inside", areas.len());
    for area in areas {
        assert!((area - PI * 16.0).abs() < 2.5, "{area} vs {}", PI * 16.0);
    }
    Ok(())
}

#[test]
fn a_streak_is_a_disc_swept_along_its_length() -> Result<(), Error> {
    // a stadium: a rectangle of length x size and two half discs. Pointing right (0) and down (90) it fits inside
    // the picture for some seeds; the area is the same
    for angle in [0.0, 90.0, 225.0] {
        let areas = whole_areas(4.0, 20.0, angle)?;
        assert!(areas.len() >= 10, "angle {angle}: {} seeds", areas.len());
        let expected = PI * 4.0 + 4.0 * 20.0;
        for area in areas {
            assert!((area - expected).abs() < 3.5, "angle {angle}: {area} vs {expected}");
        }
    }
    Ok(())
}

#[test]
fn a_streak_runs_from_its_head_in_the_direction_of_the_angle() -> Result<(), Error> {
    // angle 0 points right: the lit box is wider than tall; 90 points down: taller than wide
    for (angle, wide) in [(0.0, true), (180.0, true), (90.0, false), (270.0, false)] {
        let mut checked = 0;
        for seed in 0..40 {
            let cover = light(&lone(4.0, 30.0, angle, seed))?;
            if touches_border(&cover) {
                continue;
            }
            let rows: Vec<usize> = (0..SIDE)
                .filter(|r| cover[r * SIDE..(r + 1) * SIDE].iter().any(|l| *l > 0.0))
                .collect();
            let columns: Vec<usize> = (0..SIDE)
                .filter(|c| (0..SIDE).any(|r| cover[r * SIDE + c] > 0.0))
                .collect();
            let (tall, across) = (rows.len(), columns.len());
            assert_eq!(across > tall, wide, "angle {angle}: {across} wide, {tall} tall");
            checked += 1;
        }
        assert!(checked >= 5, "angle {angle}");
    }
    Ok(())
}

#[test]
fn particles_only_lighten_and_screen_over_what_is_there() -> Result<(), Error> {
    let params = Params {
        density: 3.0,
        seed: 3,
        ..Params::default()
    };
    let on_black = light(&params)?;
    assert!(on_black.iter().any(|level| *level > 0.4), "there is light to screen");
    // screen over grey 0.5 with light l is 0.5 + 0.5 l; over white it stays white
    let grey = run_sized(&params, SIDE, SIDE, 0.5)?;
    for (got, l) in grey.iter().zip(&on_black) {
        assert!((got[0] - (0.5 + 0.5 * l)).abs() < 1e-6);
    }
    let white = run_sized(&params, SIDE, SIDE, 1.0)?;
    assert!(white.iter().all(|pixel| is_pixel(*pixel, [1.0; 3])));
    let picture = varied(SIDE, SIDE);
    let out = run_picture(&params, SIDE, SIDE, &picture)?;
    assert!(out.iter().zip(&picture).all(|(after, before)| after >= before));
    Ok(())
}

#[test]
fn the_colour_tints_the_light() -> Result<(), Error> {
    let params = Params {
        density: 3.0,
        color: Color([1.0, 0.0, 0.5]),
        seed: 1,
        ..Params::default()
    };
    let out = run_sized(&params, SIDE, SIDE, 0.0)?;
    assert!(
        out.iter()
            .all(|pixel| pixel[1] == 0.0 && (pixel[2] - pixel[0] * 0.5).abs() < 1e-6)
    );
    assert!(out.iter().any(|pixel| pixel[0] > 0.4));
    Ok(())
}

#[test]
fn the_same_seed_gives_the_same_particles_and_another_seed_others() -> Result<(), Error> {
    let params = |seed| Params {
        density: 2.0,
        seed,
        ..Params::default()
    };
    let (first, again, other) = (light(&params(5))?, light(&params(5))?, light(&params(6))?);
    assert_eq!(first_bit_difference(&first, &again), None);
    assert!(first_bit_difference(&first, &other).is_some());
    Ok(())
}

#[test]
fn a_higher_density_keeps_the_particles_of_a_lower_one() -> Result<(), Error> {
    let params = |density| Params {
        density,
        seed: 2,
        ..Params::default()
    };
    let (fewer, more) = (light(&params(1.0))?, light(&params(4.0))?);
    assert!(fewer.iter().zip(&more).all(|(few, many)| few <= many));
    assert!(fewer.iter().sum::<f32>() < more.iter().sum::<f32>());
    Ok(())
}

#[test]
fn the_number_of_particles_is_the_density_per_1000_square_pixels() -> Result<(), Error> {
    // tiny crisp discs of 2 px across, apart from each other on a big picture: each covers about pi px^2
    let params = Params {
        density: 0.5,
        size: 2.0,
        softness: 0.0,
        layers: 1,
        seed: 4,
        ..Params::default()
    };
    let side = 300;
    let out = run_sized(&params, side, side, 0.0)?;
    let area: f64 = out.iter().map(|pixel| f64::from(pixel[0])).sum();
    let count = area / PI;
    assert!((count / 45.0 - 1.0).abs() < 0.15, "{count} particles, wanted 45");
    Ok(())
}

#[test]
fn depth_planes_change_the_particles_and_a_single_plane_is_all_nearest() -> Result<(), Error> {
    let params = |layers| Params {
        density: 2.0,
        layers,
        seed: 7,
        ..Params::default()
    };
    let (one, two) = (light(&params(1))?, light(&params(2))?);
    assert!(first_bit_difference(&one, &two).is_some());
    // the nearest are the largest and brightest: with every particle at the nearest depth there is the most light
    assert!(one.iter().sum::<f32>() > two.iter().sum::<f32>());
    Ok(())
}

#[test]
fn farther_particles_are_smaller_and_fainter() -> Result<(), Error> {
    let area = |size_falloff, brightness_falloff| -> Result<f32, Error> {
        let params = Params {
            density: 1.0,
            size_falloff,
            brightness_falloff,
            seed: 9,
            ..Params::default()
        };
        Ok(light(&params)?.iter().sum())
    };
    assert!(area(0.0, 0.0)? > area(0.8, 0.0)?);
    assert!(area(0.0, 0.0)? > area(0.0, 0.8)?);
    Ok(())
}

#[test]
fn particles_are_spread_over_the_whole_of_a_wide_or_a_tall_picture() -> Result<(), Error> {
    // every third of the rows, and of the columns, of the picture holds some light
    let params = Params {
        density: 3.0,
        seed: 1,
        ..Params::default()
    };
    for (height, width) in [(60, 240), (240, 60)] {
        let out = run_sized(&params, height, width, 0.0)?;
        let lit = |row: usize, column: usize| out[row * width + column][0] > 0.0;
        for third in 0..3 {
            let rows = third * height / 3..(third + 1) * height / 3;
            let columns = third * width / 3..(third + 1) * width / 3;
            assert!(
                rows.clone().any(|row| (0..width).any(|column| lit(row, column))),
                "{height}x{width}"
            );
            assert!(
                columns.clone().any(|column| (0..height).any(|row| lit(row, column))),
                "{height}x{width}"
            );
        }
    }
    Ok(())
}
