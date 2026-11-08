//! `dust_scratches`: what the marks do to a mid-grey picture, read off the output. Painting both colours white makes the
//! change a coverage (grey 0.5 mixed with white by c is 0.5 + 0.5 c), whose sums are compared with areas worked out by hand:
//! a vertical scratch of width w covers w px in every row it crosses, a hair its length plus its two rounded ends, a speck
//! of radius r the area pi r^2 (averaged over the random radius).

#![expect(clippy::cast_precision_loss, reason = "small picture sizes and counts")]

use bullterrier_image::Error;
use bullterrier_image::effects::dust_scratches::Params;
use bullterrier_image::params::Color;

use crate::common::compare::{first_bit_difference, first_difference};
use crate::common::pictures::varied;
use crate::common::pixels::{is_pixel, run_picture};

const WHITE: Color = Color([1.0; 3]);

/// No dust and no scratches.
fn bare() -> Params {
    Params {
        density: 0.0,
        scratches: 0,
        ..Params::default()
    }
}

fn grey(size: usize) -> Vec<f32> {
    vec![0.5; size * size * 3]
}

/// Which pixels of a mid-grey `size` square picture the marks touch.
fn changed(params: &Params, size: usize) -> Result<Vec<bool>, Error> {
    let out = run_picture(params, size, size, &grey(size))?;
    Ok(out
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| !is_pixel(*pixel, [0.5; 3]))
        .collect())
}

/// How far each pixel of a mid-grey `size` square picture is covered by marks, 0..1, when both colours are white.
fn coverage(params: &Params, size: usize) -> Result<Vec<f64>, Error> {
    let params = Params {
        light: WHITE,
        dark: WHITE,
        ..params.clone()
    };
    let out = run_picture(&params, size, size, &grey(size))?;
    Ok(out
        .iter()
        .step_by(3)
        .map(|level| (f64::from(*level) - 0.5) * 2.0)
        .collect())
}

/// How many 8-connected groups of touched pixels there are.
fn marks(mask: &[bool], size: usize) -> usize {
    let mut seen = vec![false; mask.len()];
    let mut groups = 0;
    for start in 0..mask.len() {
        if !mask[start] || seen[start] {
            continue;
        }
        groups += 1;
        let mut open = vec![start];
        seen[start] = true;
        while let Some(at) = open.pop() {
            let (row, column) = (at / size, at % size);
            for next_row in row.saturating_sub(1)..=(row + 1).min(size - 1) {
                for next_column in column.saturating_sub(1)..=(column + 1).min(size - 1) {
                    let next = next_row * size + next_column;
                    if mask[next] && !seen[next] {
                        seen[next] = true;
                        open.push(next);
                    }
                }
            }
        }
    }
    groups
}

#[test]
fn no_dust_and_no_scratches_leave_the_picture_as_it_is() -> Result<(), Error> {
    let picture = varied(13, 17);
    for light in [WHITE, Color([0.0625, 0.125, 0.1875])] {
        let out = run_picture(&Params { light, ..bare() }, 13, 17, &picture)?;
        assert_eq!(first_bit_difference(&out, &picture), None);
    }
    Ok(())
}

#[test]
fn more_dust_touches_more_of_the_picture_and_keeps_the_marks_it_had() -> Result<(), Error> {
    let masks: Vec<Vec<bool>> = [0.1, 0.3, 1.0, 3.0]
        .into_iter()
        .map(|density| {
            changed(
                &Params {
                    density,
                    seed: 4,
                    ..bare()
                },
                160,
            )
        })
        .collect::<Result<_, _>>()?;
    let touched: Vec<usize> = masks.iter().map(|mask| mask.iter().filter(|on| **on).count()).collect();
    assert!(touched.windows(2).all(|pair| pair[0] < pair[1]), "{touched:?}");
    for pair in masks.windows(2) {
        // a mark at a lower density is still there at a higher one
        assert!(pair[0].iter().zip(&pair[1]).all(|(fewer, more)| !fewer || *more));
    }
    Ok(())
}

#[test]
fn the_number_of_marks_is_the_density_per_1000_square_pixels() -> Result<(), Error> {
    // hairs of size 1 are separate tiny marks: density 0.5 on 300 x 300 px = 90 000 px^2 is 45 marks, density 1.5 is 135
    // (a few merge where they touch)
    for (density, expected) in [(0.5, 45.0), (1.5, 135.0)] {
        let mask = changed(
            &Params {
                density,
                hairs: 1.0,
                size: 1.0,
                seed: 6,
                ..bare()
            },
            300,
        )?;
        let found = marks(&mask, 300) as f64;
        assert!((found / expected - 1.0).abs() < 0.1, "density {density}: {found} marks");
    }
    Ok(())
}

#[test]
fn the_number_of_marks_is_the_nearest_whole_number_halves_to_the_even_one() -> Result<(), Error> {
    // marks = density x px^2 / 1000, hairs of size 1 being tiny groups kept apart at this seed: 0.36 marks is none, 0.9 is
    // one; on 32 x 32 = 1024 px^2 the densities 625/256 and 875/256 (exact in a float) make exactly 2.5 and 3.5 marks,
    // which round to the even numbers 2 and 4
    for (density, side, expected) in [
        (0.4, 30, 0),
        (1.0, 30, 1),
        (625.0 / 256.0, 32, 2),
        (875.0 / 256.0, 32, 4),
    ] {
        let mask = changed(
            &Params {
                density,
                hairs: 1.0,
                size: 1.0,
                seed: 6,
                ..bare()
            },
            side,
        )?;
        assert_eq!(marks(&mask, side), expected, "density {density} on {side} px");
    }
    Ok(())
}

#[test]
fn a_hair_is_a_pixel_wide_line_of_up_to_size_long() -> Result<(), Error> {
    // hairs run from half of `size` to `size` long (mean 0.75 size, 9 px at size 12) and a pixel wide: each covers its
    // length plus the two half-discs of its rounded ends (pi / 4 px^2): 45 hairs cover 45 x (9 + 0.785) px^2
    let covered: f64 = coverage(
        &Params {
            density: 0.5,
            hairs: 1.0,
            size: 12.0,
            seed: 6,
            ..bare()
        },
        300,
    )?
    .iter()
    .sum();
    let expected = 45.0 * (9.0 + std::f64::consts::FRAC_PI_4);
    assert!((covered / expected - 1.0).abs() < 0.1, "{covered} vs {expected}");
    Ok(())
}

#[test]
fn a_speck_is_a_disc_of_random_radius_up_to_half_the_size() -> Result<(), Error> {
    // the radius is size / 2 x u^2 for u evenly random (many tiny specks, few big): the mean disc area is
    // pi (size / 2)^2 E[u^4] = pi x 36 / 5 = 22.6 px^2 at size 12, so 270 specks (density 3) cover about 6100 px^2
    let covered: f64 = coverage(
        &Params {
            density: 3.0,
            hairs: 0.0,
            size: 12.0,
            seed: 6,
            ..bare()
        },
        300,
    )?
    .iter()
    .sum();
    let expected = 270.0 * std::f64::consts::PI * 36.0 / 5.0;
    assert!((covered / expected - 1.0).abs() < 0.2, "{covered} vs {expected}");
    Ok(())
}

#[test]
fn larger_specks_cover_more_of_the_picture() -> Result<(), Error> {
    for size in [2.0, 6.0, 12.0] {
        let covered = |size| -> Result<f64, Error> {
            Ok(coverage(
                &Params {
                    density: 0.5,
                    size,
                    seed: 1,
                    ..bare()
                },
                160,
            )?
            .iter()
            .sum())
        };
        assert!(covered(size * 2.0)? > 1.5 * covered(size)?, "size {size}");
    }
    Ok(())
}

#[test]
fn a_straight_scratch_covers_its_width_in_every_row() -> Result<(), Error> {
    // a vertical scratch of width w covers, summed across a row, exactly w pixels (the antialiased line is a box of width w
    // seen through the one-pixel pixel box, and sampling that at whole pixels keeps its area)
    for (width, ends) in [(1.0_f32, 3_usize), (2.5, 4), (4.0, 6)] {
        let cover = coverage(
            &Params {
                scratches: 1,
                angle: 0.0,
                width,
                seed: 3,
                ..bare()
            },
            160,
        )?;
        let sums: Vec<f64> = cover.chunks(160).map(|row| row.iter().sum()).collect();
        let marked: Vec<f64> = sums.into_iter().filter(|sum| *sum > 0.0).collect();
        assert!(
            marked.len() >= 80,
            "width {width}: runs over at least half of the 160 rows"
        );
        // `ends`: the rows of the rounded ends, which are not a full width
        for sum in &marked[ends..marked.len() - ends] {
            assert!((sum - f64::from(width)).abs() < 1e-3, "width {width}: {sum}");
        }
    }
    Ok(())
}

/// The rows a lone scratch of `seed` covers and its sideways position (coverage-weighted column) in each.
fn one_scratch(seed: u32, angle: f32) -> Result<(Vec<usize>, Vec<f64>), Error> {
    let cover = coverage(
        &Params {
            scratches: 1,
            angle,
            width: 1.0,
            seed,
            ..bare()
        },
        160,
    )?;
    let rows: Vec<usize> = (0..160)
        .filter(|row| cover[row * 160..(row + 1) * 160].iter().sum::<f64>() > 0.0)
        .collect();
    let columns = rows
        .iter()
        .map(|row| {
            let line = &cover[row * 160..(row + 1) * 160];
            line.iter()
                .enumerate()
                .map(|(column, level)| column as f64 * level)
                .sum::<f64>()
                / line.iter().sum::<f64>()
        })
        .collect();
    Ok((rows, columns))
}

/// The least-squares slope of `values` against `rows`.
fn slope(rows: &[usize], values: &[f64]) -> f64 {
    let xs: Vec<f64> = rows.iter().map(|row| *row as f64).collect();
    let (mean_x, mean_y) = (
        xs.iter().sum::<f64>() / xs.len() as f64,
        values.iter().sum::<f64>() / values.len() as f64,
    );
    let covariance: f64 = xs.iter().zip(values).map(|(x, y)| (x - mean_x) * (y - mean_y)).sum();
    covariance / xs.iter().map(|x| (x - mean_x).powi(2)).sum::<f64>()
}

#[test]
fn scratches_lean_to_either_side_by_up_to_the_angle() -> Result<(), Error> {
    // each scratch leans by a random angle up to 40 degrees off vertical, i.e. moves sideways at most tan(40) = 0.84 px per
    // row, to the left or to the right; across 40 scratches the slopes spread over nearly that whole range
    let slopes: Vec<f64> = (0..40)
        .map(|seed| {
            let (rows, columns) = one_scratch(seed, 40.0)?;
            Ok(slope(&rows, &columns))
        })
        .collect::<Result<_, Error>>()?;
    let limit = 40.0_f64.to_radians().tan();
    assert!(slopes.iter().all(|s| s.abs() <= limit + 0.005), "{slopes:?}");
    assert!(
        slopes.iter().any(|s| *s < -0.8 * limit) && slopes.iter().any(|s| *s > 0.8 * limit),
        "{slopes:?}"
    );
    Ok(())
}

#[test]
fn scratches_run_over_half_to_all_of_the_height_at_places_that_differ() -> Result<(), Error> {
    let spans: Vec<(usize, usize)> = (0..16)
        .map(|seed| {
            let (rows, _) = one_scratch(seed, 0.0)?;
            Ok((rows[0], rows[rows.len() - 1]))
        })
        .collect::<Result<_, Error>>()?;
    let lengths: Vec<usize> = spans.iter().map(|(first, last)| last - first + 1).collect();
    assert!(lengths.iter().all(|length| (80..=160).contains(length)), "{lengths:?}"); // half of the 160 rows, and the ends
    assert!(lengths.iter().any(|length| *length > 140)); // some run nearly all the way
    assert!(spans.iter().any(|(first, _)| *first < 10) && spans.iter().any(|(first, _)| *first > 10)); // start at various heights
    assert!(spans.iter().any(|(_, last)| *last < 150)); // and not all reach the bottom
    Ok(())
}

#[test]
fn more_scratches_cover_more() -> Result<(), Error> {
    let covered = |scratches| -> Result<f64, Error> {
        Ok(coverage(
            &Params {
                scratches,
                seed: 1,
                ..bare()
            },
            160,
        )?
        .iter()
        .sum())
    };
    assert!(covered(8)? > 2.0 * covered(2)?);
    Ok(())
}

#[test]
fn light_marks_take_the_light_colour_and_dark_marks_the_dark_colour() -> Result<(), Error> {
    let params = Params {
        density: 1.0,
        size: 20.0,
        scratches: 0,
        seed: 1,
        light: Color([1.0, 0.0, 0.0]),
        dark: Color([0.0, 0.0, 1.0]),
        ..Params::default()
    };
    let out = run_picture(&params, 160, 160, &grey(160))?;
    let pixels = out.as_chunks::<3>().0;
    assert!(pixels.contains(&[1.0, 0.0, 0.0])); // a fully covered light pixel
    assert!(pixels.contains(&[0.0, 0.0, 1.0])); // a fully covered dark pixel
    Ok(())
}

#[test]
fn where_a_light_mark_crosses_a_dark_one_the_light_one_lies_on_top() -> Result<(), Error> {
    // light and dark marks are equally many and cover about the same share c of a dense picture, independently: with the
    // light on top the fully light pixels are c of the picture and the fully dark ones only c (1 - c), the dark ones that
    // the light ones leave; with the dark on top it is the other way round. At c near 0.4 that is 1.7 against 0.6.
    let params = Params {
        density: 5.0,
        size: 50.0,
        hairs: 0.0,
        scratches: 0,
        seed: 3,
        light: Color([1.0, 0.0, 0.0]),
        dark: Color([0.0, 0.0, 1.0]),
        ..Params::default()
    };
    let out = run_picture(&params, 160, 160, &grey(160))?;
    let count = |colour: [f32; 3]| {
        out.as_chunks::<3>()
            .0
            .iter()
            .filter(|pixel| is_pixel(**pixel, colour))
            .count()
    };
    let (light, dark) = (count([1.0, 0.0, 0.0]), count([0.0, 0.0, 1.0]));
    assert!(
        dark > 1000 && light as f64 > 1.3 * dark as f64,
        "{light} light and {dark} dark pixels"
    );
    Ok(())
}

/// Whether some 3 x 3 block of a `size` square mask is wholly set.
fn has_covered_block(mask: &[bool], size: usize) -> bool {
    (0..size - 2).any(|row| (0..size - 2).any(|column| (0..9).all(|at| mask[(row + at / 3) * size + column + at % 3])))
}

#[test]
fn hairs_are_thin_and_specks_are_blobs() -> Result<(), Error> {
    // with every mark a hair (one pixel wide) no 3 x 3 block is fully covered; with specks up to 20 px, one is
    let base = Params {
        density: 0.3,
        size: 20.0,
        seed: 2,
        ..bare()
    };
    let solid = |hairs| -> Result<Vec<bool>, Error> {
        Ok(coverage(&Params { hairs, ..base.clone() }, 160)?
            .iter()
            .map(|level| *level >= 0.99)
            .collect())
    };
    assert!(has_covered_block(&solid(0.0)?, 160));
    assert!(
        coverage(
            &Params {
                hairs: 1.0,
                ..base.clone()
            },
            160
        )?
        .iter()
        .sum::<f64>()
            > 0.0
    );
    assert!(!has_covered_block(&solid(1.0)?, 160));
    Ok(())
}

#[test]
fn the_same_seed_gives_the_same_marks_and_another_seed_other_marks() -> Result<(), Error> {
    let picture = varied(60, 60);
    let run = |seed| {
        run_picture(
            &Params {
                seed,
                ..Params::default()
            },
            60,
            60,
            &picture,
        )
    };
    assert_eq!(first_bit_difference(&run(1)?, &run(1)?), None);
    assert!(first_bit_difference(&run(1)?, &run(2)?).is_some());
    assert!(first_bit_difference(&run(u32::MAX)?, &run(u32::MAX - 1)?).is_some());
    for settings in [
        Params {
            scratches: 3,
            density: 0.0,
            ..Params::default()
        },
        Params {
            scratches: 0,
            density: 0.5,
            ..Params::default()
        },
    ] {
        let at = |seed| {
            changed(
                &Params {
                    seed,
                    ..settings.clone()
                },
                160,
            )
        };
        assert!(first_difference(&at(1)?, &at(2)?).is_some());
    }
    Ok(())
}
