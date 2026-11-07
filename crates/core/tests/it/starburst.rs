//! `starburst`: spikes of light worked out by hand and from their definition, the direction they point in (the kernel
//! is applied flipped, as a convolution is), which points send them, and the properties a screen keeps.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "small picture sizes and kernel reaches"
)]

use bullterrier_image::effects::starburst::Params;
use bullterrier_image::params::{Color, Declared, EffectParams};
use bullterrier_image::{Error, Inputs, Picture, apply};
use proptest::prelude::*;
use serde_json::Value;

use crate::common::compare::first_difference_within;
use crate::common::convolution::{Beyond, Tap, convolved};
use crate::common::pictures::{impulse, varied};
use crate::common::pixels::run_picture;

/// The pictures are compared at this distance: sums in f32 or by the transform, and the reference in f64.
const TOLERANCE: f32 = 1e-4;

const WHITE: Color = Color([1.0, 1.0, 1.0]);

/// Spikes as the hand work below has them: one hairline spike, from every point above 0, screened at full strength.
fn hairlines(rays: i32, angle: f32, length: f32) -> Params {
    Params {
        rays,
        angle,
        length,
        width: 1.0,
        threshold: 0.0,
        amount: 1.0,
        color: WHITE,
        ..Params::default()
    }
}

fn spikes(params: &Params, height: usize, width: usize, picture: &[f32]) -> Result<Vec<f32>, Error> {
    run_picture(params, height, width, picture)
}

/// `params` applied with `size_ratio` working px per design px.
fn at_ratio(params: &Params, ratio: f64, height: usize, width: usize, picture: &[f32]) -> Result<Vec<f32>, Error> {
    let image = apply(
        Params::EFFECT,
        &Picture::new(height, width, picture)?,
        Value::Object(params.values()),
        &Inputs::default().with_size_ratio(ratio),
    )?;
    Ok(image.into_values())
}

/// The red value at (`row`, `column`) of a picture of `width` columns.
fn red(picture: &[f32], width: usize, row: usize, column: usize) -> f32 {
    picture[(row * width + column) * 3]
}

#[test]
fn a_spike_leaves_the_point_in_the_direction_of_the_angle_and_fades_linearly() -> Result<(), Error> {
    // by hand: length 4, one hairline: weights 1, 3/4, 1/2, 1/4, 0 along the spike (the middle weighs the point itself,
    // which is white already), and 0 beside and behind it. The result is the weights, whatever the picture's size: the
    // spike is the kernel applied flipped, so that it goes where the angle points and not the opposite way.
    let (size, middle) = (15, 7);
    let point = impulse(size, size, middle, middle);
    let fade = [1.0_f32, 0.75, 0.5, 0.25, 0.0];
    for (angle, down, right) in [(0.0, 0, 1), (90.0, 1, 0), (180.0, 0, -1), (270.0, -1, 0)] {
        let out = spikes(&hairlines(1, angle, 4.0), size, size, &point)?;
        let mut expected = vec![0.0_f32; out.len()];
        for (step, light) in fade.iter().enumerate() {
            let (row, column) = (
                middle.wrapping_add_signed(down * step as isize),
                middle.wrapping_add_signed(right * step as isize),
            );
            expected[(row * size + column) * 3..][..3].fill(*light);
        }
        assert_eq!(first_difference_within(&out, &expected, 1e-6), None, "angle {angle}");
    }
    Ok(())
}

#[test]
fn four_spikes_make_a_cross_and_the_point_is_lit_once() -> Result<(), Error> {
    let (size, middle) = (15, 7);
    let out = spikes(
        &hairlines(4, 0.0, 4.0),
        size,
        size,
        &impulse(size, size, middle, middle),
    )?;
    for (step, light) in [1.0_f32, 0.75, 0.5, 0.25, 0.0].into_iter().enumerate() {
        for (down, right) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (row, column) = (
                middle.wrapping_add_signed(down * step as isize),
                middle.wrapping_add_signed(right * step as isize),
            );
            assert!(
                (red(&out, size, row, column) - light).abs() < 1e-6,
                "step {step}, {down} {right}"
            );
        }
    }
    // beside the spokes there is nothing
    assert!(red(&out, size, middle + 1, middle + 1) < 1e-6);
    assert!(red(&out, size, middle - 2, middle + 3) < 1e-6);
    Ok(())
}

#[test]
fn the_spikes_are_what_the_definition_says_whatever_lies_beyond_the_edge_which_is_black() -> Result<(), Error> {
    // Spikes of a few and of many weights (summed tap by tap, and filtered by the Fourier transform) over pictures from
    // one pixel to a few, against a convolution worked out from the documented spike: a line `width` px wide, soft at its
    // sides, fading linearly to nothing at `length`; the brightest of the rays where they cross.
    let shapes = [(1, 1), (1, 9), (9, 1), (5, 7), (23, 31), (40, 45)];
    for (rays, angle, length, width) in [
        (3, 20.0, 6.0, 2.0),
        (1, 200.0, 4.5, 1.0),
        (5, 33.0, 25.0, 3.0),
        (4, 45.0, 30.0, 1.0),
    ] {
        let params = Params {
            rays,
            angle,
            length,
            width,
            threshold: 0.9, // few highlights: with many the light saturates to white and says little
            amount: 0.4,
            color: Color([1.0, 0.6, 0.3]),
            ..Params::default()
        };
        let taps = spike_taps(&params);
        for (height, width) in shapes {
            let picture = varied(height, width);
            let out = spikes(&params, height, width, &picture)?;
            let at = first_difference_within(&out, &screened(&params, &taps, height, width, &picture), TOLERANCE);
            assert_eq!(
                at, None,
                "{rays} rays, angle {angle}, length {length}, {height} x {width}: first differs at {at:?}"
            );
        }
    }
    Ok(())
}

/// The weights of the spikes of `params` (not scaled to sum to 1), from the documented shape.
fn spike_taps(params: &Params) -> Vec<Tap> {
    let (length, width) = (f64::from(params.length), f64::from(params.width));
    let reach = length.max(width).ceil() as i64;
    let mut taps = Vec::new();
    for down in -reach..=reach {
        for right in -reach..=reach {
            let best = (0..params.rays)
                .map(|spike| {
                    let theta = f64::from(params.angle).to_radians()
                        + std::f64::consts::TAU * f64::from(spike) / f64::from(params.rays);
                    let (along, across) = (
                        right as f64 * theta.cos() + down as f64 * theta.sin(),
                        (down as f64 * theta.cos() - right as f64 * theta.sin()).abs(),
                    );
                    let thin = (width / 2.0 + 0.5 - across).clamp(0.0, 1.0);
                    if along < 0.0 {
                        0.0
                    } else {
                        thin * (1.0 - along / length).clamp(0.0, 1.0)
                    }
                })
                .fold(0.0, f64::max);
            taps.push((down, right, best));
        }
    }
    taps
}

/// `picture` with the light of `taps` over the highlights of it screened on, as `params` says.
fn screened(params: &Params, taps: &[Tap], height: usize, width: usize, picture: &[f32]) -> Vec<f32> {
    let lit: Vec<f32> = picture
        .iter()
        .map(|value| ((value - params.threshold) / (1.0 - params.threshold)).clamp(0.0, 1.0))
        .collect();
    let light = convolved(taps, Beyond::Black, height, width, &lit);
    picture
        .iter()
        .zip(&light)
        .enumerate()
        .map(|(index, (base, light))| {
            let strength = params.amount * params.color.0[index % 3] * light;
            base + strength.clamp(0.0, 1.0) * (1.0 - base)
        })
        .collect()
}

#[test]
fn the_comparison_tells_spikes_that_differ_apart() -> Result<(), Error> {
    // the known-bad rows: another angle, another length and a wider spike are not these spikes
    let picture = varied(23, 31);
    let params = Params {
        rays: 3,
        angle: 20.0,
        length: 25.0,
        width: 2.0,
        threshold: 0.9,
        amount: 0.4,
        ..Params::default()
    };
    let ours = spikes(&params, 23, 31, &picture)?;
    let lit_not_white = ours
        .iter()
        .zip(&picture)
        .filter(|(out, base)| out > base && **out < 0.99)
        .count();
    assert!(
        lit_not_white > 100,
        "the light does not saturate, so differences show: {lit_not_white}"
    );
    for other in [
        Params {
            angle: 25.0,
            ..params.clone()
        },
        Params {
            length: 20.0,
            ..params.clone()
        },
        Params {
            width: 3.0,
            ..params.clone()
        },
    ] {
        let expected = screened(&other, &spike_taps(&other), 23, 31, &picture);
        assert!(first_difference_within(&ours, &expected, TOLERANCE).is_some());
    }
    Ok(())
}

#[test]
fn no_light_comes_in_from_beyond_the_edge() -> Result<(), Error> {
    // a point on the edge whose spike points off the picture lights nothing on it (a repeated or mirrored edge would)
    let (size, edge) = (9, 0);
    let out = spikes(&hairlines(1, 180.0, 6.0), size, size, &impulse(size, size, 4, edge))?;
    for column in 1..size {
        assert!(red(&out, size, 4, column) < 1e-6, "column {column}");
    }
    assert!((red(&out, size, 4, 0) - 1.0).abs() < 1e-6);
    Ok(())
}

#[test]
fn amount_scales_the_light_and_color_tints_it() -> Result<(), Error> {
    let (size, middle) = (15, 7);
    let mut grey = vec![0.0_f32; size * size * 3];
    grey[(middle * size + middle) * 3..][..3].fill(0.5); // a point at half brightness
    let params = Params {
        threshold: 0.0,
        length: 6.0,
        ..Params::default()
    };
    let at = (middle * size + middle + 2) * 3;
    let light = |amount: f32, color: Color| -> Result<[f32; 3], Error> {
        let out = spikes(
            &Params {
                amount,
                color,
                ..params.clone()
            },
            size,
            size,
            &grey,
        )?;
        Ok([out[at], out[at + 1], out[at + 2]])
    };
    let [one, _, _] = light(0.5, WHITE)?;
    let [two, _, _] = light(1.0, WHITE)?;
    assert!(one > 0.01 && (two / one - 2.0).abs() < 1e-4, "{one} {two}");
    let [red, green, blue] = light(1.0, Color([0.2, 0.5, 1.0]))?;
    assert!(blue > green && green > red && red > 0.0);
    assert!((green / blue - 0.5).abs() < 1e-4 && (red / blue - 0.2).abs() < 1e-4);
    Ok(())
}

#[test]
fn no_amount_no_limit_and_black_leave_the_picture_as_it_is() -> Result<(), Error> {
    let picture = varied(9, 11);
    for params in [
        Params {
            amount: 0.0,
            threshold: 0.0,
            ..Params::default()
        },
        Params {
            limit: 0.0,
            threshold: 0.0,
            ..Params::default()
        },
        Params {
            threshold: 0.99,
            ..Params::default()
        },
    ] {
        let mut dim = picture.clone();
        for value in &mut dim {
            *value *= 0.9;
        }
        let out = spikes(&params, 9, 11, &dim)?;
        assert_eq!(first_difference_within(&out, &dim, 0.0), None, "{params:?}");
    }
    let black = vec![0.0_f32; 9 * 11 * 3];
    assert_eq!(
        first_difference_within(&spikes(&Params::default(), 9, 11, &black)?, &black, 0.0),
        None
    );
    Ok(())
}

#[test]
fn limit_lets_only_that_share_of_the_pixels_radiate_the_brightest_first() -> Result<(), Error> {
    // ten greys from 0.50 to 0.95 in a row; spikes too short to leave the point (length 0.5): a pixel brightens only if it
    // radiates. A limit of 0.3 allows three, the brightest (0.85, 0.90, 0.95), by raising the threshold to the fourth's 0.80.
    let row: Vec<f32> = (0..10).flat_map(|step| [0.5 + 0.05 * step as f32; 3]).collect();
    let params = Params {
        length: 0.5,
        threshold: 0.0,
        amount: 1.0,
        limit: 0.3,
        ..Params::default()
    };
    let out = spikes(&params, 1, 10, &row)?;
    for step in 0..10 {
        let base = 0.5 + 0.05 * step as f32;
        let expected = if base > 0.8 + 1e-3 {
            let light: f32 = (base - 0.8) / 0.2;
            base + light * (1.0 - base)
        } else {
            base
        };
        assert!(
            (red(&out, 10, 0, step) - expected).abs() < 1e-5,
            "pixel {step}: {} against {expected}",
            red(&out, 10, 0, step)
        );
    }
    // with no limit every pixel above the threshold radiates
    let all = spikes(&Params { limit: 1.0, ..params }, 1, 10, &row)?;
    assert!(red(&all, 10, 0, 0) > 0.5 + 0.1);
    Ok(())
}

#[test]
fn a_limit_never_raises_the_threshold_above_the_highest_so_near_white_still_shines() -> Result<(), Error> {
    // every pixel is 0.995: with a limit the threshold would rise to it and nothing would shine; it stops at 0.99
    let grey = vec![0.995_f32; 4 * 4 * 3];
    let params = Params {
        length: 0.5,
        threshold: 0.0,
        amount: 1.0,
        limit: 0.5,
        ..Params::default()
    };
    let out = spikes(&params, 4, 4, &grey)?;
    let expected = 0.995 + 0.5 * (1.0 - 0.995); // (0.995 - 0.99) / (1 - 0.99) = 0.5 of the light
    assert!((out[0] - expected).abs() < 1e-5, "{} against {expected}", out[0]);
    Ok(())
}

/// How many pixels of the picture changed, and whether each did.
fn lit(out: &[f32], picture: &[f32]) -> Vec<bool> {
    out.as_chunks::<3>()
        .0
        .iter()
        .zip(picture.as_chunks::<3>().0)
        .map(|(a, b)| a.iter().zip(b).any(|(a, b)| a.to_bits() != b.to_bits()))
        .collect()
}

#[test]
fn each_bright_pixel_radiates_with_the_chance_one_over_spacing_squared_from_its_seed() -> Result<(), Error> {
    // a flat grey of 60 x 60 and spikes too short to leave their point: the pixels that brighten are the ones that radiate
    let grey = vec![0.6_f32; 60 * 60 * 3];
    let base = Params {
        length: 0.5,
        threshold: 0.0,
        amount: 0.5,
        seed: 4,
        ..Params::default()
    };
    let share = |spacing: f32, seed: u32| -> Result<f32, Error> {
        let out = spikes(
            &Params {
                spacing,
                seed,
                ..base.clone()
            },
            60,
            60,
            &grey,
        )?;
        Ok(lit(&out, &grey).iter().filter(|lit| **lit).count() as f32 / 3600.0)
    };
    for (spacing, expected) in [(2.0, 0.25), (4.0, 1.0 / 16.0), (1.5, 0.4444)] {
        assert!(
            (share(spacing, 4)? - expected).abs() < 0.04,
            "spacing {spacing}: {}",
            share(spacing, 4)?
        );
    }
    // at one design pixel or less every bright point radiates, and at once
    assert!((share(1.0, 4)? - 1.0).abs() < 1e-6 && (share(0.0, 4)? - 1.0).abs() < 1e-6);
    Ok(())
}

#[test]
fn raising_spacing_only_drops_points_and_the_seed_picks_which() -> Result<(), Error> {
    let grey = vec![0.6_f32; 60 * 60 * 3];
    let params = |spacing, seed| Params {
        length: 0.5,
        threshold: 0.0,
        amount: 0.5,
        spacing,
        seed,
        ..Params::default()
    };
    let by = |spacing, seed| Ok::<_, Error>(lit(&spikes(&params(spacing, seed), 60, 60, &grey)?, &grey));
    let (near, far) = (by(2.0, 4)?, by(4.0, 4)?);
    assert!(
        far.iter().zip(&near).all(|(far, near)| !far || *near),
        "every point at 4 also radiates at 2"
    );
    assert!(near.iter().filter(|lit| **lit).count() > far.iter().filter(|lit| **lit).count());
    assert_eq!(by(2.0, 4)?, near, "the same seed picks the same points");
    assert_ne!(by(2.0, 5)?, near, "another seed picks others");
    Ok(())
}

#[test]
fn another_seed_picks_other_places_and_not_the_same_places_moved_about() -> Result<(), Error> {
    // a seed xored into a place would make seed 1's draw at pixel i the draw of seed 0 at the pixel next to it (i xor 1)
    let grey = vec![0.6_f32; 60 * 60 * 3];
    let by = |seed| -> Result<Vec<bool>, Error> {
        Ok(lit(
            &spikes(
                &Params {
                    length: 0.5,
                    threshold: 0.0,
                    amount: 0.5,
                    spacing: 2.0,
                    seed,
                    ..Params::default()
                },
                60,
                60,
                &grey,
            )?,
            &grey,
        ))
    };
    for (one, other) in [(0, 1), (4, 9), (2, 7)] {
        let (first, second) = (by(one)?, by(other)?);
        let moved = (0..first.len())
            .filter(|index| first[*index] != second[index ^ (one ^ other) as usize])
            .count();
        assert!(
            moved > 500,
            "seeds {one} and {other}: only {moved} of 3600 places differ from the same places moved about"
        );
    }
    Ok(())
}

#[test]
fn a_point_keeps_its_draw_while_the_picture_around_it_changes() -> Result<(), Error> {
    let (height, width) = (30, 30);
    let params = Params {
        length: 0.5,
        threshold: 0.0,
        amount: 0.5,
        spacing: 3.0,
        variation: 0.5,
        seed: 9,
        ..Params::default()
    };
    let mut picture = vec![0.6_f32; height * width * 3];
    let before = spikes(&params, height, width, &picture)?;
    for pixel in picture.as_chunks_mut::<3>().0.iter_mut().step_by(2) {
        pixel.fill(0.9); // half the picture is brighter
    }
    let after = spikes(&params, height, width, &picture)?;
    // pixels that did not change (0.6): their light is the same as before
    for (index, pixel) in picture.as_chunks::<3>().0.iter().enumerate() {
        if (pixel[0] - 0.6).abs() < 1e-6 {
            assert_eq!(before[index * 3].to_bits(), after[index * 3].to_bits(), "pixel {index}");
        }
    }
    Ok(())
}

#[test]
fn variation_dims_points_by_a_random_share_between_one_minus_variation_and_one() -> Result<(), Error> {
    // of every point that radiates, with every point radiating and with one in four doing so (the two draws are not alike)
    let grey = vec![0.6_f32; 60 * 60 * 3];
    for spacing in [0.0, 2.0] {
        let params = Params {
            length: 0.5,
            threshold: 0.0,
            amount: 0.5,
            spacing,
            seed: 2,
            ..Params::default()
        };
        let light = |variation: f32| -> Result<Vec<f32>, Error> {
            Ok(spikes(
                &Params {
                    variation,
                    ..params.clone()
                },
                60,
                60,
                &grey,
            )?
            .iter()
            .step_by(3)
            .map(|value| (value - 0.6) / (1.0 - 0.6))
            .collect())
        };
        let (plain, varied) = (light(0.0)?, light(0.6)?);
        let ratios: Vec<f32> = varied
            .iter()
            .zip(&plain)
            .filter(|(_, plain)| **plain > 0.0)
            .map(|(varied, plain)| varied / plain)
            .collect();
        assert!(ratios.len() > 800, "spacing {spacing}: {} points", ratios.len());
        assert!(
            ratios.iter().all(|ratio| (0.4 - 1e-4..=1.0 + 1e-4).contains(ratio)),
            "spacing {spacing}: within 0.4..1"
        );
        assert!(
            ratios.iter().any(|ratio| *ratio < 0.5) && ratios.iter().any(|ratio| *ratio > 0.9),
            "spacing {spacing}: and spread over it"
        );
        let mean = ratios.iter().sum::<f32>() / ratios.len() as f32;
        assert!((mean - 0.7).abs() < 0.03, "spacing {spacing}: {mean}");
    }
    Ok(())
}

#[test]
fn a_bright_area_gives_the_same_light_at_any_size_and_glints_keep_their_number() -> Result<(), Error> {
    // working px per design px: at 2, a bright pixel sends a quarter of a design pixel's light (spikes too short to leave
    // it); sparse points, further apart than a design px, send all of it.
    let grey = vec![0.6_f32; 60 * 60 * 3];
    let params = Params {
        length: 0.5,
        threshold: 0.0,
        amount: 0.5,
        ..Params::default()
    };
    let out = at_ratio(&params, 2.0, 60, 60, &grey)?;
    let full = 0.6 + 0.5 * 0.6 * (1.0 - 0.6);
    let quarter = 0.6 + 0.25 * 0.5 * 0.6 * (1.0 - 0.6);
    assert!(
        out.iter().all(|value| (value - quarter).abs() < 1e-5),
        "a quarter of the light"
    );
    assert!((spikes(&params, 60, 60, &grey)?[0] - full).abs() < 1e-5);
    // a spacing of 2 working px is one design px: no thinning, every pixel with its quarter
    let under = at_ratio(
        &Params {
            spacing: 2.0,
            ..params.clone()
        },
        2.0,
        60,
        60,
        &grey,
    )?;
    assert!(
        under.iter().all(|value| (value - quarter).abs() < 1e-5),
        "a spacing of one design px"
    );
    // 6 working px apart is 3 design px: the chance 1 / 6^2 of a working pixel
    let sparse = at_ratio(
        &Params {
            spacing: 6.0,
            seed: 3,
            ..params
        },
        2.0,
        60,
        60,
        &grey,
    )?;
    let share = lit(&sparse, &grey).iter().filter(|lit| **lit).count() as f32 / 3600.0;
    assert!((share - 1.0 / 36.0).abs() < 0.01, "{share}");
    assert!(
        sparse
            .iter()
            .step_by(3)
            .filter(|v| (**v - 0.6).abs() > 1e-6)
            .all(|v| (v - full).abs() < 1e-5),
        "each with all of its light"
    );
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn the_picture_only_gets_brighter_and_stays_in_range(
        height in 1_usize..9,
        width in 1_usize..9,
        picture in prop::collection::vec(0.0_f32..1.0, 81 * 3),
        rays in 1_i32..8,
        angle in 0.0_f32..360.0,
        length in prop_oneof![0.5_f32..5.0, 30.0_f32..60.0],
        threshold in 0.0_f32..0.99,
        limit in prop_oneof![Just(1.0_f32), 0.0_f32..1.0],
    ) {
        let picture = &picture[..height * width * 3];
        let params = Params { rays, angle, length, threshold, limit, amount: 3.0, ..Params::default() };
        let out = spikes(&params, height, width, picture)?;
        prop_assert_eq!(out.len(), picture.len());
        prop_assert!(out.iter().zip(picture).all(|(out, base)| out >= base && *out <= 1.0), "brighter, at most white");
    }
}
