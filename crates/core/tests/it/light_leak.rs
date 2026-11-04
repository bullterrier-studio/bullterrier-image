//! `light_leak`: seeded patches of light that come in from the side `angle` points to. The random numbers are the
//! core's own stream, so what is tested is what the doc promises: the same seed gives the same leak, another seed
//! another, the light enters from the right side, and it is screened over the picture.

use bullterrier_image::Error;
use bullterrier_image::effects::light_leak;
use bullterrier_image::params::Color;

use crate::common::pixels::run_sized;

const HEIGHT: usize = 90;
const WIDTH: usize = 120;

fn white() -> light_leak::Params {
    light_leak::Params {
        color: Color([1.0; 3]),
        ..light_leak::Params::default()
    }
}

/// The leak alone: `params` on a black picture, as the red channel, row after row.
fn leak(params: &light_leak::Params) -> Result<Vec<f32>, Error> {
    Ok(run_sized(params, HEIGHT, WIDTH, 0.0)?
        .iter()
        .map(|pixel| pixel[0])
        .collect())
}

/// How far the light's centre of mass lies from the picture's centre toward `angle` degrees clockwise from right, in px:
/// the brute-force weighted mean of every pixel's offset.
#[expect(clippy::cast_precision_loss, reason = "test pictures are tiny")]
fn centroid_toward(light: &[f32], angle: f32) -> f32 {
    let (sin, cos) = angle.to_radians().sin_cos();
    let (mut moment, mut total) = (0.0, 0.0);
    for (index, &value) in light.iter().enumerate() {
        let right = (index % WIDTH) as f32 - (WIDTH - 1) as f32 / 2.0;
        let down = (index / WIDTH) as f32 - (HEIGHT - 1) as f32 / 2.0;
        moment += value * (right * cos + down * sin);
        total += value;
    }
    moment / total
}

#[test]
fn the_same_seed_gives_the_same_leak_and_another_seed_another() -> Result<(), Error> {
    let seeded = |seed| leak(&light_leak::Params { seed, ..white() });
    assert_eq!(seeded(3)?, seeded(3)?);
    let (first, second) = (seeded(3)?, seeded(4)?);
    let biggest = first
        .iter()
        .zip(&second)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    assert!(biggest > 0.05, "seeds 3 and 4 differ by at most {biggest}");
    Ok(())
}

#[test]
fn the_light_enters_from_the_side_angle_points_to() -> Result<(), Error> {
    for angle in [0.0, 45.0, 90.0, 180.0, 270.0] {
        for seed in 0..4 {
            let light = leak(&light_leak::Params {
                angle,
                seed,
                amount: 1.0,
                size: 30.0,
                ..white()
            })?;
            let toward = centroid_toward(&light, angle);
            assert!(
                toward > 5.0,
                "angle {angle}, seed {seed}: the light's centre is {toward} px toward the side"
            );
        }
    }
    Ok(())
}

#[test]
fn light_is_screened_over_the_picture_and_scaled_by_amount() -> Result<(), Error> {
    // over a grey of 0.5 each pixel is 1 - (1 - 0.5)(1 - light), light being what the leak adds to black
    let params = light_leak::Params {
        size: 30.0,
        seed: 5,
        ..white()
    };
    let light = leak(&params)?;
    let over_grey = run_sized(&params, HEIGHT, WIDTH, 0.5)?;
    for (index, (pixel, light)) in over_grey.iter().zip(&light).enumerate() {
        assert!((pixel[0] - (1.0 - 0.5 * (1.0 - light))).abs() <= 1e-5, "pixel {index}");
    }
    // where nothing saturates, 3 times the amount is 3 times the light
    let weak = leak(&light_leak::Params {
        amount: 0.1,
        ..params.clone()
    })?;
    let strong = leak(&light_leak::Params { amount: 0.3, ..params })?;
    assert!(weak.iter().copied().fold(0.0, f32::max) > 0.01);
    for (weak, strong) in weak.iter().zip(&strong) {
        assert!((strong - 3.0 * weak).abs() <= 1e-4 * strong.max(1e-2));
    }
    Ok(())
}

#[test]
fn the_color_tints_the_light_and_an_amount_of_zero_changes_nothing() -> Result<(), Error> {
    let tinted = light_leak::Params {
        color: Color([1.0, 0.5, 0.25]),
        amount: 1.0,
        size: 30.0,
        ..light_leak::Params::default()
    };
    let out = run_sized(&tinted, HEIGHT, WIDTH, 0.0)?;
    let lit: Vec<_> = out.iter().filter(|pixel| pixel[0] > 0.01).collect();
    assert!(lit.len() > 100);
    for pixel in lit {
        assert!((pixel[1] / pixel[0] - 0.5).abs() <= 1e-3);
        assert!((pixel[2] / pixel[0] - 0.25).abs() <= 1e-3);
    }
    let off = run_sized(&light_leak::Params { amount: 0.0, ..tinted }, HEIGHT, WIDTH, 0.3)?;
    assert!(off.iter().flatten().all(|value| value.to_bits() == 0.3_f32.to_bits()));
    Ok(())
}

#[test]
fn the_picture_only_gets_brighter_and_stays_in_range() -> Result<(), Error> {
    let params = light_leak::Params {
        amount: 2.0,
        size: 45.0,
        blobs: 16,
        ..light_leak::Params::default()
    };
    let out = run_sized(&params, HEIGHT, WIDTH, 0.3)?;
    assert!(out.iter().flatten().all(|&value| (0.3..=1.0).contains(&value)));
    assert!(out.iter().flatten().any(|&value| value > 0.31));
    Ok(())
}

#[test]
fn overlapping_patches_blend_instead_of_clipping_to_a_flat_plateau() -> Result<(), Error> {
    let light = leak(&light_leak::Params {
        blobs: 8,
        size: 45.0,
        amount: 1.0,
        ..white()
    })?;
    let top = light.iter().copied().fold(0.0, f32::max);
    let flat = light.iter().filter(|&&value| value > top - 1e-6).count();
    assert!(flat * 100 < light.len(), "{flat} pixels share the maximum {top}");
    Ok(())
}

/// The spread of the pixels above half the peak: (along the side, in from it), as standard deviations in px.
#[expect(clippy::cast_precision_loss, reason = "test pictures are tiny")]
fn spread_of_the_bright_part(light: &[f32]) -> (f32, f32) {
    let half = light.iter().copied().fold(0.0, f32::max) / 2.0;
    let bright: Vec<(f32, f32)> = (0..light.len())
        .filter(|&index| light[index] > half)
        .map(|index| ((index / WIDTH) as f32, (index % WIDTH) as f32))
        .collect();
    let deviation = |value: fn(&(f32, f32)) -> f32| {
        let mean = bright.iter().map(value).sum::<f32>() / bright.len() as f32;
        (bright.iter().map(|point| (value(point) - mean).powi(2)).sum::<f32>() / bright.len() as f32).sqrt()
    };
    (deviation(|point| point.0), deviation(|point| point.1))
}

#[test]
fn a_patch_is_stretched_along_the_side_by_up_to_twice_and_never_across_it() -> Result<(), Error> {
    // from the right side: a lone patch well inside the picture, so that nothing cuts it off, reaches `sigma` in and
    // `sigma` x (1 + stretch) along the side, stretch in 0..1
    let mut stretches = Vec::new();
    for seed in 0..60 {
        let light = leak(&light_leak::Params {
            blobs: 1,
            size: 30.0,
            amount: 0.5,
            seed,
            ..white()
        })?;
        let peak = (0..light.len())
            .max_by(|&a, &b| light[a].total_cmp(&light[b]))
            .unwrap_or_default();
        let (row, column) = (peak / WIDTH, peak % WIDTH);
        if column + 20 < WIDTH && row > 27 && row < 63 {
            let (along, in_from) = spread_of_the_bright_part(&light);
            stretches.push(along / in_from);
        }
    }
    assert!(
        stretches.len() >= 4,
        "only {} patches lie clear of the edges",
        stretches.len()
    );
    assert!(stretches.iter().all(|&stretch| stretch > 0.9), "{stretches:?}");
    assert!(stretches.iter().any(|&stretch| stretch > 1.4), "{stretches:?}");
    Ok(())
}
