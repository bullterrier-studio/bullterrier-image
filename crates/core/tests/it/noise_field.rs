//! `noise_field`: a generated picture judged by what its documentation promises. Run through a black-to-white ramp, a pixel's
//! level is its place on the ramp, which is the normal distribution of the standardised noise: inverting it (with a formula of
//! the tests' own) gives the noise back, whose mean is 0 and deviation 1. The rest are structural properties of each kind:
//! gradient noise is zero on its lattice, cellular noise has one minimum for each cell, a layer weighted 0 or finer than the
//! pixel grid adds nothing, and `warp` leaves a single sine layer alone.

use bullterrier_image::Error;
use bullterrier_image::effects::noise_field::{Kind, Params};
use bullterrier_image::params::Color;

use crate::common::compare::{first_bit_difference, first_difference};
use crate::common::numbers::float;
use crate::common::pictures::varied;
use crate::common::pixels::{channel, run_picture, run_sized};
use crate::common::stats::{correlation, deviation, mean, neighbour_correlation, normal_quantile};

const KINDS: [Kind; 4] = [Kind::Value, Kind::Perlin, Kind::Worley, Kind::Sine];
const SIZE: usize = 128;

/// `params` with a black-to-white ramp, so that a pixel's level is its place on the ramp.
fn grey(params: &Params) -> Params {
    Params {
        colors: vec![Color([0.0; 3]), Color([1.0; 3])],
        ..params.clone()
    }
}

/// The level of every pixel of a `height` x `width` picture (red channel), as f64.
fn levels(params: &Params, height: usize, width: usize) -> Result<Vec<f64>, Error> {
    let out = run_sized(&grey(params), height, width, 0.0)?;
    Ok(out.iter().map(|pixel| f64::from(pixel[0])).collect())
}

/// The standardised noise behind the levels of a `SIZE` square picture: the normal quantile of each level.
fn noise(params: &Params) -> Result<Vec<f64>, Error> {
    noise_of(params, SIZE)
}

/// The same for a `side` square picture.
fn noise_of(params: &Params, side: usize) -> Result<Vec<f64>, Error> {
    Ok(levels(params, side, side)?
        .into_iter()
        .map(|level| normal_quantile(level.clamp(1e-7, 1.0 - 1e-7)))
        .collect())
}

fn kind(kind: Kind) -> Params {
    Params {
        kind,
        ..Params::default()
    }
}

#[test]
fn the_noise_is_standardised_before_it_is_ranked_whatever_the_kind() -> Result<(), Error> {
    for kind in KINDS {
        for (scale, octaves) in [(40.0, 1), (40.0, 4), (9.0, 3)] {
            let params = Params {
                scale,
                octaves,
                seed: 5,
                ..self::kind(kind)
            };
            if kind == Kind::Value {
                // 'value' noise has mean 0 and deviation 1 in how it is made (a pixel needs no picture to be scaled), so it
                // shows both over seeds, not in one picture of a few lattice cells
                let mut z = Vec::new();
                for seed in 0..48 {
                    z.extend(noise(&Params {
                        seed,
                        scale: scale / 5.0,
                        ..params.clone()
                    })?);
                }
                assert!(mean(&z).abs() < 0.03, "{kind:?} scale {scale}: mean {}", mean(&z));
                assert!(
                    (deviation(&z) - 1.0).abs() < 0.05,
                    "{kind:?} scale {scale}: deviation {}",
                    deviation(&z)
                );
            } else {
                let z = noise(&params)?;
                assert!(mean(&z).abs() < 0.01, "{kind:?} scale {scale}: mean {}", mean(&z));
                assert!(
                    (deviation(&z) - 1.0).abs() < 0.02,
                    "{kind:?} scale {scale}: deviation {}",
                    deviation(&z)
                );
            }
        }
    }
    Ok(())
}

#[test]
fn only_the_size_of_the_picture_is_read() -> Result<(), Error> {
    for kind in KINDS {
        let params = Params {
            warp: 6.0,
            ..self::kind(kind)
        };
        let one = run_picture(&params, 40, 52, &varied(40, 52))?;
        let other = run_picture(&params, 40, 52, &vec![0.3; 40 * 52 * 3])?;
        assert_eq!(first_bit_difference(&one, &other), None, "{kind:?}");
    }
    Ok(())
}

#[test]
fn the_same_seed_gives_the_same_picture_and_another_seed_another() -> Result<(), Error> {
    for kind in KINDS {
        for warp in [0.0, 8.0] {
            let run = |seed| {
                levels(
                    &Params {
                        warp,
                        seed,
                        ..self::kind(kind)
                    },
                    48,
                    48,
                )
            };
            let first = run(1)?;
            assert_eq!(first_difference(&first, &run(1)?), None, "{kind:?} warp {warp}");
            assert!(first_difference(&first, &run(2)?).is_some(), "{kind:?} warp {warp}");
            assert!(
                first_difference(&first, &run(u32::MAX)?).is_some(),
                "{kind:?} warp {warp}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_larger_scale_makes_broader_features_whatever_the_kind() -> Result<(), Error> {
    for kind in KINDS {
        let at = |scale| -> Result<f64, Error> {
            let z = noise(&Params {
                scale,
                octaves: 1,
                seed: 3,
                ..self::kind(kind)
            })?;
            Ok(neighbour_correlation(&z, SIZE))
        };
        let (fine, medium, coarse) = (at(4.0)?, at(16.0)?, at(64.0)?);
        assert!(fine < medium && medium < coarse, "{kind:?}: {fine} {medium} {coarse}");
        assert!(coarse > 0.9, "{kind:?}: {coarse}");
    }
    Ok(())
}

#[test]
fn a_layer_weighted_nothing_or_finer_than_the_pixel_grid_adds_nothing() -> Result<(), Error> {
    for kind in KINDS {
        // sine layers need two pixels, the others one: a scale of 4 (sine) or 3 halves under the grid after two layers (not 2:
        // gradient noise is zero at every pixel of a lattice one pixel apart)
        let scale = if kind == Kind::Sine { 4.0 } else { 3.0 };
        let run = |octaves, persistence| {
            run_sized(
                &Params {
                    scale,
                    octaves,
                    persistence,
                    seed: 4,
                    ..self::kind(kind)
                },
                33,
                41,
                0.0,
            )
        };
        let flat = |pixels: Vec<[f32; 3]>| pixels.into_iter().flatten().collect::<Vec<f32>>();
        assert_eq!(
            first_bit_difference(&flat(run(2, 0.5)?), &flat(run(8, 0.5)?)),
            None,
            "{kind:?}: finer than a pixel"
        );
        assert_eq!(
            first_bit_difference(&flat(run(1, 0.5)?), &flat(run(2, 0.0)?)),
            None,
            "{kind:?}: persistence 0"
        );
        assert!(
            first_bit_difference(&flat(run(1, 0.5)?), &flat(run(2, 0.5)?)).is_some(),
            "{kind:?}: a second layer does show"
        );
    }
    Ok(())
}

#[test]
fn a_layer_is_kept_down_to_exactly_the_size_the_pixel_grid_shows() -> Result<(), Error> {
    // value and cellular layers show at 1 px, sine waves at 2 px: the layer at exactly that size is the last one that counts
    for (kind, scale) in [(Kind::Value, 4.0), (Kind::Worley, 4.0), (Kind::Sine, 8.0)] {
        let run = |octaves| {
            run_sized(
                &Params {
                    scale,
                    octaves,
                    seed: 9,
                    ..self::kind(kind)
                },
                33,
                41,
                0.0,
            )
        };
        let flat = |pixels: Vec<[f32; 3]>| pixels.into_iter().flatten().collect::<Vec<f32>>();
        assert!(
            first_bit_difference(&flat(run(2)?), &flat(run(3)?)).is_some(),
            "{kind:?}: the last visible layer"
        );
        assert_eq!(
            first_bit_difference(&flat(run(3)?), &flat(run(4)?)),
            None,
            "{kind:?}: the first invisible one"
        );
    }
    Ok(())
}

#[test]
fn neighbouring_pixels_never_jump_whatever_the_kind() -> Result<(), Error> {
    // the noise is continuous: at a feature size of 32 px no step from one pixel to the next is anywhere near a deviation
    for kind in KINDS {
        let z = noise(&Params {
            scale: 32.0,
            octaves: 1,
            seed: 12,
            ..self::kind(kind)
        })?;
        let jump = z
            .as_chunks::<SIZE>()
            .0
            .iter()
            .flat_map(|line| line.windows(2).map(|pair| (pair[1] - pair[0]).abs()))
            .fold(0.0, f64::max);
        assert!(jump < 0.5, "{kind:?}: a step of {jump}");
    }
    Ok(())
}

#[test]
fn gradient_noise_is_the_same_at_every_lattice_point() -> Result<(), Error> {
    // one octave of spacing 8: each lattice point reads 0 before standardising, so it reads the same afterwards
    let params = Params {
        scale: 8.0,
        octaves: 1,
        ..kind(Kind::Perlin)
    };
    let out = levels(&params, 64, 64)?;
    let at_points: Vec<f64> = (0..64)
        .step_by(8)
        .flat_map(|row| (0..64).step_by(8).map(move |column| row * 64 + column))
        .map(|at| out[at])
        .collect();
    assert!(
        at_points.iter().all(|level| (level - at_points[0]).abs() < 1e-6),
        "{at_points:?}"
    );
    let elsewhere = out.iter().filter(|level| (**level - at_points[0]).abs() > 0.01).count();
    assert!(elsewhere > 64 * 64 * 9 / 10);
    Ok(())
}

#[test]
fn cellular_noise_has_one_dark_point_for_each_cell() -> Result<(), Error> {
    // a 120 px square of cells 12 px across: 10 x 10 cells, the distance to the nearest point dipping to a minimum at each
    let params = Params {
        scale: 12.0,
        octaves: 1,
        seed: 6,
        ..kind(Kind::Worley)
    };
    let out = levels(&params, 120, 120)?;
    let at = |row: usize, column: usize| out[row * 120 + column];
    let mut minima = 0;
    for row in 1..119 {
        for column in 1..119 {
            let here = at(row, column);
            let lowest = [
                (-1_isize, -1_isize),
                (-1, 0),
                (-1, 1),
                (0, -1),
                (0, 1),
                (1, -1),
                (1, 0),
                (1, 1),
            ]
            .iter()
            .all(|&(down, across)| here <= at(row.wrapping_add_signed(down), column.wrapping_add_signed(across)));
            minima += usize::from(lowest);
        }
    }
    assert!((90..=130).contains(&minima), "{minima} local minima for 100 cells");
    Ok(())
}

/// The correlation of each pixel of a `side` square field with the one `lag` pixels to its right.
fn lag_correlation(field: &[f64], side: usize, lag: usize) -> f64 {
    let pairs =
        (0..side).flat_map(|row| (0..side - lag).map(move |column| (row * side + column, row * side + column + lag)));
    let (here, there): (Vec<f64>, Vec<f64>) = pairs.map(|(a, b)| (field[a], field[b])).unzip();
    correlation(&here, &there)
}

#[test]
fn cellular_layers_of_equal_weight_count_equally_whatever_their_cell_size() -> Result<(), Error> {
    // each layer is measured in its own cells, so with persistence 1 two layers have the same spread and the pair is as smooth
    // as the average of the two alone (the layers' scatters are independent)
    let at = |scale, octaves| {
        noise_of(
            &Params {
                scale,
                octaves,
                persistence: 1.0,
                seed: 11,
                ..kind(Kind::Worley)
            },
            384,
        )
    };
    let (coarse, fine, both) = (at(8.0, 1)?, at(4.0, 1)?, at(8.0, 2)?);
    let expected = f64::midpoint(lag_correlation(&coarse, 384, 3), lag_correlation(&fine, 384, 3));
    let found = lag_correlation(&both, 384, 3);
    assert!((found - expected).abs() < 0.05, "{found} against {expected}");
    Ok(())
}

#[test]
fn cellular_noise_is_alike_at_the_border_and_inside() -> Result<(), Error> {
    // the scatter of points goes on past the border, so the top row and left column have a nearest point as the rows and
    // columns inside do. A jittered grid is not the same at every phase of its cells, so compare a row at a cell edge (cells
    // of 6 px: row 0) with the rows that are also at a cell edge (12, 24, 36), likewise columns: both average a standardised 0.1
    let side = 48;
    let (mut border, mut inside) = (Vec::new(), Vec::new());
    for seed in 100..160 {
        let z = noise_of(
            &Params {
                scale: 6.0,
                octaves: 1,
                seed,
                ..kind(Kind::Worley)
            },
            side,
        )?;
        for place in 0..side {
            border.extend([z[place], z[place * side]]);
            for edge in [12, 24, 36] {
                inside.extend([z[edge * side + place], z[place * side + edge]]);
            }
        }
    }
    assert!(
        (mean(&border) - mean(&inside)).abs() < 0.12,
        "border {}, inside {}",
        mean(&border),
        mean(&inside)
    );
    Ok(())
}

#[test]
fn a_single_sine_layer_is_not_pushed_so_warp_changes_nothing_until_there_is_a_second() -> Result<(), Error> {
    let run = |octaves, warp| {
        run_sized(
            &Params {
                kind: Kind::Sine,
                scale: 30.0,
                octaves,
                warp,
                seed: 2,
                ..Params::default()
            },
            40,
            40,
            0.0,
        )
    };
    let flat = |pixels: Vec<[f32; 3]>| pixels.into_iter().flatten().collect::<Vec<f32>>();
    assert_eq!(first_bit_difference(&flat(run(1, 0.0)?), &flat(run(1, 40.0)?)), None);
    assert!(first_bit_difference(&flat(run(2, 0.0)?), &flat(run(2, 40.0)?)).is_some());
    Ok(())
}

#[test]
fn warp_moves_the_pattern_about_by_about_that_many_pixels() -> Result<(), Error> {
    // a little warp is nearly the same picture, a large one a different one; the levels change by less the less it moves
    for kind in [Kind::Value, Kind::Perlin, Kind::Worley] {
        let base = Params {
            scale: 40.0,
            seed: 8,
            ..self::kind(kind)
        };
        let still = levels(&base, SIZE, SIZE)?;
        let moved = |warp| -> Result<f64, Error> {
            let out = levels(&Params { warp, ..base.clone() }, SIZE, SIZE)?;
            Ok(mean(
                &still.iter().zip(&out).map(|(a, b)| (a - b).abs()).collect::<Vec<_>>(),
            ))
        };
        let (zero, little, much) = (moved(0.0)?, moved(0.5)?, moved(40.0)?);
        assert!(zero < 1e-9, "{kind:?}");
        assert!(0.0 < little && little < 0.03, "{kind:?}: {little}");
        assert!(much > 0.1 && much > 4.0 * little, "{kind:?}: {much}");
    }
    Ok(())
}

#[test]
fn the_colours_lie_evenly_along_the_ramp_and_each_is_used_about_equally() -> Result<(), Error> {
    // red, green, blue: the lower half of the places mixes red into green, the upper half green into blue
    let params = Params {
        kind: Kind::Perlin,
        colors: vec![Color([1.0, 0.0, 0.0]), Color([0.0, 1.0, 0.0]), Color([0.0, 0.0, 1.0])],
        scale: 12.0,
        seed: 1,
        ..Params::default()
    };
    let out = run_sized(&params, SIZE, SIZE, 0.0)?;
    let (mut lower, mut upper) = (0, 0);
    for [red, green, blue] in out {
        if blue == 0.0 {
            assert!((red + green - 1.0).abs() < 1e-5);
            lower += 1;
        } else {
            assert!(red == 0.0 && (green + blue - 1.0).abs() < 1e-5);
            upper += 1;
        }
    }
    let share = float(lower) / float(lower + upper);
    assert!((share - 0.5).abs() < 0.05, "{share} of the pixels in the lower half");
    // and the same ramp read as the level of channel 0 of the black-to-white one: red falls as the place rises
    let reds = channel(&run_picture(&params, SIZE, SIZE, &vec![0.0; SIZE * SIZE * 3])?, 0);
    assert!(reds.iter().any(|red| *red > 0.9) && reds.contains(&0.0));
    Ok(())
}
