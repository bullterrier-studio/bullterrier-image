//! `fog`: the picture mixed toward a colour by an opacity that is cloudy and heavier toward the bottom. Without clouds
//! (`floor` 1) the opacity is a function of the row that the documentation gives, so every value is worked out by hand; the
//! clouds are the highest `coverage` of a standardised noise, whose normal quantile statistics at a fixed seed show; `rise`
//! draws them out upward.

use bullterrier_image::Error;
use bullterrier_image::effects::fog::Params;
use bullterrier_image::params::Color;
use bullterrier_image::{Edge, Interpolation};

use crate::common::compare::{assert_close, first_bit_difference, first_difference};
use crate::common::numbers::float;
use crate::common::pictures::varied;
use crate::common::pixels::{run_picture, run_sized};
use crate::common::stats::{correlation, deviation, mean, normal_quantile};

const SIDE: usize = 128;

/// White fog over black, so a pixel's level is the fog's opacity there.
fn opacity(params: &Params, height: usize, width: usize) -> Result<Vec<f64>, Error> {
    let out = run_sized(
        &Params {
            color: Color([1.0; 3]),
            ..params.clone()
        },
        height,
        width,
        0.0,
    )?;
    Ok(out.iter().map(|pixel| f64::from(pixel[0])).collect())
}

/// The inverse of smoothstep `3t^2 - 2t^3` on 0..1, by bisection.
fn unsmooth(y: f64) -> f64 {
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..60 {
        let middle = f64::midpoint(low, high);
        if middle * middle * (3.0 - 2.0 * middle) < y {
            low = middle;
        } else {
            high = middle;
        }
    }
    f64::midpoint(low, high)
}

#[test]
fn without_clouds_the_opacity_falls_in_a_straight_line_up_the_frame() -> Result<(), Error> {
    // floor 1: no clouds, the opacity is amount x (1 - gradient x (1 - t)) with t = (row + 1/2) / height from the top
    let params = Params {
        amount: 0.8,
        gradient: 0.6,
        floor: 1.0,
        ..Params::default()
    };
    let out = opacity(&params, 20, 5)?;
    for row in 0..20 {
        let down = (float(row) + 0.5) / 20.0;
        let expected = 0.8 * (1.0 - 0.6 * (1.0 - down));
        for column in 0..5 {
            assert!((out[row * 5 + column] - expected).abs() < 1e-6, "row {row}");
        }
    }
    Ok(())
}

#[test]
fn the_picture_moves_toward_the_colour_by_the_opacity_in_every_channel() -> Result<(), Error> {
    let picture = [0.2_f32, 0.4, 0.6].repeat(9 * 11);
    let params = Params {
        amount: 0.5,
        color: Color([1.0, 0.0, 0.5]),
        floor: 1.0,
        gradient: 0.0,
        ..Params::default()
    };
    let out = run_picture(&params, 9, 11, &picture)?;
    // opacity 0.5 everywhere: halfway from each channel to the colour
    for pixel in out.as_chunks::<3>().0 {
        assert_close(pixel, &[0.6, 0.2, 0.55], 1e-6, "halfway to the colour");
    }
    Ok(())
}

#[test]
fn no_amount_or_no_cloud_and_no_floor_leaves_the_picture_as_it_is() -> Result<(), Error> {
    let picture = varied(24, 31);
    for params in [
        Params {
            amount: 0.0,
            ..Params::default()
        },
        Params {
            coverage: 0.0,
            floor: 0.0,
            ..Params::default()
        },
    ] {
        let out = run_picture(&params, 24, 31, &picture)?;
        assert_eq!(first_bit_difference(&out, &picture), None, "{params:?}");
    }
    Ok(())
}

#[test]
fn the_cloud_shape_is_smoothstep_of_the_noise_ranked_by_the_normal_distribution() -> Result<(), Error> {
    // coverage 1, feather 1, no floor, no gradient, amount 1: opacity = smoothstep(u), u = ndtr(standardised noise). Undoing
    // the smoothstep and the normal distribution must give noise of mean 0 and deviation 1
    // the noise has mean 0 and deviation 1 in how it is made, not in one picture, so it shows both over seeds
    let mut z: Vec<f64> = Vec::new();
    for seed in 0..24 {
        let params = Params {
            amount: 1.0,
            gradient: 0.0,
            coverage: 1.0,
            feather: 1.0,
            floor: 0.0,
            scale: 12.0,
            seed,
            ..Params::default()
        };
        z.extend(
            opacity(&params, SIDE, SIDE)?
                .into_iter()
                .map(|shape| normal_quantile(unsmooth(shape).clamp(1e-7, 1.0 - 1.0e-7))),
        );
    }
    assert!(mean(&z).abs() < 0.03, "mean {}", mean(&z));
    assert!((deviation(&z) - 1.0).abs() < 0.05, "deviation {}", deviation(&z));
    Ok(())
}

#[test]
fn about_coverage_of_the_picture_has_cloud_in_it() -> Result<(), Error> {
    // also when the noise is read between pixels (stretched upward, pushed about), which smooths it: it is ranked again after,
    // so a thin share of cloud is still a thin share (without that, smoothed noise would cover only about half of 0.03)
    for (coverage, rise, turbulence, tolerance) in [
        (0.2_f32, 1.0_f32, 0.0_f32, 0.07),
        (0.45, 1.0, 0.0, 0.07),
        (0.8, 1.0, 0.0, 0.07),
        (0.03, 6.0, 60.0, 0.006),
        (0.97, 6.0, 60.0, 0.006),
    ] {
        let params = Params {
            amount: 1.0,
            gradient: 0.0,
            coverage,
            floor: 0.0,
            scale: 3.0,
            rise,
            turbulence,
            seed: 9,
            ..Params::default()
        };
        let cloudy = opacity(&params, 2 * SIDE, 2 * SIDE)?
            .iter()
            .filter(|level| **level > 0.0)
            .count();
        let share = float(cloudy) / float(4 * SIDE * SIDE);
        assert!(
            (share - f64::from(coverage)).abs() < tolerance,
            "coverage {coverage} rise {rise} turbulence {turbulence}: {share}"
        );
    }
    Ok(())
}

#[test]
fn the_floor_is_the_opacity_between_the_clouds_as_a_share_of_the_amount() -> Result<(), Error> {
    let params = |floor| Params {
        amount: 0.8,
        gradient: 0.0,
        coverage: 0.3,
        feather: 0.1,
        floor,
        scale: 10.0,
        seed: 2,
        ..Params::default()
    };
    for floor in [0.0_f32, 0.25, 0.5] {
        let out = opacity(&params(floor), SIDE, SIDE)?;
        let least = out.iter().copied().fold(f64::INFINITY, f64::min);
        let most = out.iter().copied().fold(0.0, f64::max);
        assert!(
            (least - 0.8 * f64::from(floor)).abs() < 1e-6,
            "floor {floor}: clear air {least}"
        );
        assert!(
            most > 0.8 * 0.99 && most <= 0.8 + 1e-6,
            "floor {floor}: thickest {most}"
        );
    }
    Ok(())
}

#[test]
fn a_smaller_feather_makes_crisper_clouds() -> Result<(), Error> {
    // the share of pixels that are neither clear nor full strength shrinks with the feather
    let soft = |feather| -> Result<f64, Error> {
        let out = opacity(
            &Params {
                amount: 1.0,
                gradient: 0.0,
                coverage: 0.5,
                floor: 0.0,
                feather,
                scale: 8.0,
                seed: 6,
                ..Params::default()
            },
            SIDE,
            SIDE,
        )?;
        Ok(float(out.iter().filter(|level| **level > 0.01 && **level < 0.99).count()) / float(out.len()))
    };
    let (crisp, medium, soft_edge) = (soft(0.05)?, soft(0.25)?, soft(1.0)?);
    assert!(crisp < medium && medium < soft_edge, "{crisp} {medium} {soft_edge}");
    assert!(crisp < 0.1 && soft_edge > 0.3);
    Ok(())
}

/// The correlation of each pixel of a `SIDE` square field with the one `lag` pixels below it (`down`) or to its right.
fn lag_correlation(field: &[f64], lag: usize, down: bool) -> f64 {
    let pairs = (0..SIDE - lag).flat_map(|near| (0..SIDE).map(move |along| (near, along)));
    let (here, there): (Vec<f64>, Vec<f64>) = pairs
        .map(|(near, along)| {
            let (first, second) = if down {
                (near * SIDE + along, (near + lag) * SIDE + along)
            } else {
                (along * SIDE + near, along * SIDE + near + lag)
            };
            (field[first], field[second])
        })
        .unzip();
    correlation(&here, &there)
}

#[test]
fn rise_draws_the_clouds_out_upward() -> Result<(), Error> {
    let clouds = |rise| {
        opacity(
            &Params {
                amount: 1.0,
                gradient: 0.0,
                coverage: 1.0,
                floor: 0.0,
                scale: 8.0,
                rise,
                seed: 1,
                ..Params::default()
            },
            SIDE,
            SIDE,
        )
    };
    let round = clouds(1.0)?;
    let (down, across) = (lag_correlation(&round, 4, true), lag_correlation(&round, 4, false));
    assert!(
        (down - across).abs() < 0.15,
        "round puffs: {down} down, {across} across"
    );
    let tall = clouds(4.0)?;
    let (down, across) = (lag_correlation(&tall, 4, true), lag_correlation(&tall, 4, false));
    assert!(down > across + 0.2, "drawn out: {down} down, {across} across");
    Ok(())
}

/// The ranks (0 for the smallest) of `values`, as the numbers a rank correlation is taken over.
fn ranks(values: &[f64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|&a, &b| values[a].total_cmp(&values[b]));
    let mut ranks = vec![0.0; values.len()];
    for (rank, &at) in order.iter().enumerate() {
        ranks[at] = float(rank);
    }
    ranks
}

#[test]
fn the_clouds_are_drawn_out_from_the_bottom_edge_so_the_bottom_row_stays_as_it_is() -> Result<(), Error> {
    // rise stretches the noise away from the bottom edge: the bottom row reads the same row of noise whatever the rise, and a
    // row higher up reads a row that is rise times nearer the bottom. Opacity is monotone in the noise (coverage 1, no floor), so
    // the order of the pixels along a row says which row of noise it reads; the wrap rule makes a read past the edge show
    let bottom_order = |rise, row| -> Result<Vec<f64>, Error> {
        let out = opacity(
            &Params {
                amount: 1.0,
                gradient: 0.0,
                coverage: 1.0,
                floor: 0.0,
                scale: 6.0,
                rise,
                edge: Edge::Wrap,
                seed: 3,
                ..Params::default()
            },
            SIDE,
            SIDE,
        )?;
        Ok(ranks(&out[row * SIDE..(row + 1) * SIDE]))
    };
    let alike = |one: &[f64], other: &[f64]| correlation(one, other);
    // the bottom row: the same noise at rise 1 and 4
    assert!(alike(&bottom_order(1.0, SIDE - 1)?, &bottom_order(4.0, SIDE - 1)?) > 0.98);
    // 40 rows up at rise 4 it reads the noise 10 rows up, which is not the noise of the row 40 up at rise 1, but is that of row
    // 10 up at rise 1
    assert!(alike(&bottom_order(4.0, SIDE - 41)?, &bottom_order(1.0, SIDE - 11)?) > 0.9);
    assert!(alike(&bottom_order(4.0, SIDE - 41)?, &bottom_order(1.0, SIDE - 41)?) < 0.5);
    Ok(())
}

#[test]
fn how_the_noise_is_read_matters_only_where_something_moves_it() -> Result<(), Error> {
    let all = |params: &Params| -> Result<Vec<Vec<f32>>, Error> {
        let mut pictures = Vec::new();
        for resampling in [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic] {
            for edge in [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap] {
                let params = Params {
                    resampling,
                    edge,
                    ..params.clone()
                };
                pictures.push(run_picture(&params, 40, 40, &varied(40, 40))?);
            }
        }
        Ok(pictures)
    };
    // nothing moves: every way of reading gives the picture of the first
    let still = all(&Params {
        turbulence: 0.0,
        rise: 1.0,
        ..Params::default()
    })?;
    for other in &still {
        assert_eq!(first_bit_difference(&still[0], other), None);
    }
    // turbulence moves the noise: each way of reading gives another picture
    let moved = all(&Params {
        turbulence: 12.0,
        ..Params::default()
    })?;
    for (one, first) in moved.iter().enumerate() {
        for (two, second) in moved.iter().enumerate().skip(one + 1) {
            assert!(
                first_bit_difference(first, second).is_some(),
                "readings {one} and {two}"
            );
        }
    }
    Ok(())
}

#[test]
fn fog_is_thicker_toward_the_bottom_and_the_same_seed_gives_the_same_clouds() -> Result<(), Error> {
    let params = Params {
        gradient: 1.0,
        floor: 0.5,
        seed: 3,
        ..Params::default()
    };
    let out = opacity(&params, SIDE, SIDE)?;
    let (top, bottom) = (&out[..SIDE * SIDE / 4], &out[SIDE * SIDE * 3 / 4..]);
    assert!(mean(top) < 0.5 * mean(bottom), "{} against {}", mean(top), mean(bottom));
    let again = opacity(&params, SIDE, SIDE)?;
    let other = opacity(&Params { seed: 4, ..params }, SIDE, SIDE)?;
    assert_eq!(first_difference(&out, &again), None);
    assert!(first_difference(&out, &other).is_some());
    Ok(())
}
