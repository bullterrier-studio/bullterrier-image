//! The `color_transfer` effect: the photo takes the mean and spread of the reference's Oklab channels. The oracle is the
//! test's own Oklab (Ottosson's published definition, in f64) and plain loops for mean and deviation, not the effect's code.

#![expect(
    clippy::unreadable_literal,
    clippy::many_single_char_names,
    reason = "Ottosson's published coefficients and cone names, spelt as published"
)]

use std::sync::Arc;

use crate::common::compare::assert_close;
use crate::common::pictures::{varied, varied_seeded};
use bullterrier_image::{Error, Image, Options, Photo, Picture, Presets, Recipe, Step};

const HEIGHT: usize = 12;
const WIDTH: usize = 16;

/// Ottosson's Oklab (2020) of one sRGB pixel, from the published definition.
fn oklab(pixel: &[f32]) -> [f64; 3] {
    let linear = pixel.iter().map(|&level| {
        let level = f64::from(level);
        if level <= 0.04045 {
            level / 12.92
        } else {
            ((level + 0.055) / 1.055).powf(2.4)
        }
    });
    let [r, g, b]: [f64; 3] = linear.collect::<Vec<_>>().try_into().unwrap_or_default();
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

/// The mean and population standard deviation of each Oklab channel of `values` (RGB pixels), by plain loops.
fn statistics(values: &[f32]) -> ([f64; 3], [f64; 3]) {
    let labs: Vec<[f64; 3]> = values.chunks(3).map(oklab).collect();
    #[expect(clippy::cast_precision_loss, reason = "a test picture is small")]
    let count = labs.len() as f64;
    let mut mean = [0.0; 3];
    for lab in &labs {
        for channel in 0..3 {
            mean[channel] += lab[channel] / count;
        }
    }
    let mut deviation = [0.0; 3];
    for lab in &labs {
        for channel in 0..3 {
            deviation[channel] += (lab[channel] - mean[channel]).powi(2) / count;
        }
    }
    (mean, deviation.map(f64::sqrt))
}

/// A muted reference picture: every level between 0.35 and 0.65, so a transfer onto it stays inside the gamut.
fn muted(seed: usize) -> Vec<f32> {
    varied_seeded(HEIGHT, WIDTH, seed)
        .iter()
        .map(|level| 0.35 + 0.3 * level)
        .collect()
}

/// A photo of the tests' size, rendered with `color_transfer` given `reference` (none: no reference) and `params`.
fn transfer(photo: &[f32], reference: Option<Image>, params: &[(&str, serde_json::Value)]) -> Result<Vec<f32>, Error> {
    let mut step = Step::new("color_transfer");
    let mut photo = Photo::new(Picture::new(HEIGHT, WIDTH, photo)?);
    if let Some(reference) = reference {
        photo = photo.with_image("ref", Arc::new(reference));
        step = step.param("reference", "ref");
    }
    for (key, value) in params {
        step = step.param(*key, value.clone());
    }
    Ok(photo
        .render(&Recipe::new([step]), &Presets::new(), Options::default())?
        .into_values())
}

fn rgb(height: usize, width: usize, values: &[f32]) -> Result<Image, Error> {
    Image::new(height, width, values)
}

fn assert_stats_close(got: &[f32], want: &[f32], tolerance: f64, axes: std::ops::Range<usize>) {
    let ((got_mean, got_deviation), (want_mean, want_deviation)) = (statistics(got), statistics(want));
    for axis in axes {
        assert!(
            (got_mean[axis] - want_mean[axis]).abs() < tolerance,
            "mean {axis}: {got_mean:?} vs {want_mean:?}"
        );
        assert!(
            (got_deviation[axis] - want_deviation[axis]).abs() < tolerance,
            "deviation {axis}: {got_deviation:?} vs {want_deviation:?}"
        );
    }
}

#[test]
fn no_reference_and_amount_zero_leave_the_photo() -> Result<(), Error> {
    let photo = varied(HEIGHT, WIDTH);
    assert_close(&transfer(&photo, None, &[])?, &photo, 0.0, "no reference");
    let none = transfer(&photo, Some(rgb(HEIGHT, WIDTH, &muted(1))?), &[("amount", 0.into())])?;
    assert_close(&none, &photo, 0.0, "amount 0");
    Ok(())
}

#[test]
fn the_photo_as_its_own_reference_is_unchanged() -> Result<(), Error> {
    let photo = varied(HEIGHT, WIDTH);
    let same = transfer(&photo, Some(rgb(HEIGHT, WIDTH, &photo)?), &[])?;
    assert_close(&same, &photo, 1e-4, "own reference");
    Ok(())
}

#[test]
fn full_transfer_gives_the_photo_the_references_mean_and_spread_in_every_channel() -> Result<(), Error> {
    let reference = muted(3);
    let made = transfer(&varied(HEIGHT, WIDTH), Some(rgb(HEIGHT, WIDTH, &reference)?), &[])?;
    assert_stats_close(&made, &reference, 2e-3, 0..3);
    Ok(())
}

#[test]
fn amount_one_half_lands_halfway_between_the_photo_and_the_full_transfer() -> Result<(), Error> {
    let photo = varied(HEIGHT, WIDTH);
    let reference = rgb(HEIGHT, WIDTH, &muted(3))?;
    let full = transfer(&photo, Some(reference.clone()), &[])?;
    let half = transfer(&photo, Some(reference), &[("amount", 0.5.into())])?;
    let average: Vec<f32> = photo
        .iter()
        .zip(&full)
        .map(|(photo, full)| (photo + full) / 2.0)
        .collect();
    assert_close(&half, &average, 1e-6, "halfway");
    Ok(())
}

#[test]
fn preserve_luminance_keeps_each_pixels_lightness_and_matches_a_and_b() -> Result<(), Error> {
    let photo = varied(HEIGHT, WIDTH);
    let reference = muted(5);
    let made = transfer(
        &photo,
        Some(rgb(HEIGHT, WIDTH, &reference)?),
        &[("preserve_luminance", true.into())],
    )?;
    for (made, photo) in made.chunks(3).zip(photo.chunks(3)) {
        assert!((oklab(made)[0] - oklab(photo)[0]).abs() < 2e-3, "{made:?} vs {photo:?}");
    }
    assert_stats_close(&made, &reference, 2e-2, 1..3);
    Ok(())
}

#[test]
fn a_flat_reference_paints_the_photo_in_its_colour_and_a_flat_photo_takes_the_references_mean() -> Result<(), Error> {
    let colour = [0.8, 0.3, 0.2];
    let flat: Vec<f32> = colour.repeat(HEIGHT * WIDTH);
    let painted = transfer(&varied(HEIGHT, WIDTH), Some(rgb(HEIGHT, WIDTH, &flat)?), &[])?;
    assert_close(&painted, &flat, 1e-4, "flat reference");

    let reference = muted(7);
    let made = transfer(&[0.5; HEIGHT * WIDTH * 3], Some(rgb(HEIGHT, WIDTH, &reference)?), &[])?;
    let (mean, _) = statistics(&made);
    let (want, _) = statistics(&reference);
    for channel in 0..3 {
        assert!((mean[channel] - want[channel]).abs() < 1e-4, "{mean:?} vs {want:?}");
    }
    Ok(())
}

#[test]
fn black_white_and_other_extreme_references_give_finite_levels_in_range() -> Result<(), Error> {
    let size = HEIGHT * WIDTH;
    let extremes = [
        vec![0.0; size * 3],
        vec![1.0; size * 3],
        [1.0, 0.0, 0.0].repeat(size),
        [0.0, 1.0, 1.0].repeat(size),
        varied(HEIGHT, WIDTH),
    ];
    let photos = [
        varied(HEIGHT, WIDTH),
        vec![0.0; size * 3],
        vec![1.0; size * 3],
        [0.2, 0.9, 0.1].repeat(size),
    ];
    for reference in &extremes {
        for photo in &photos {
            for preserve in [false, true] {
                let made = transfer(
                    photo,
                    Some(rgb(HEIGHT, WIDTH, reference)?),
                    &[("preserve_luminance", preserve.into())],
                )?;
                assert!(made.iter().all(|level| (0.0..=1.0).contains(level)), "{reference:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn transparent_reference_pixels_do_not_count() -> Result<(), Error> {
    // left half opaque muted colours; right half fully transparent with wild colours under the clear
    let opaque = muted(9);
    let mut rgba = Vec::new();
    let mut kept = Vec::new();
    for (index, pixel) in opaque.chunks(3).enumerate() {
        if index % 2 == 0 {
            rgba.extend([pixel[0], pixel[1], pixel[2], 1.0]);
            kept.extend(pixel);
        } else {
            rgba.extend([1.0, 0.0, 1.0, 0.0]);
        }
    }
    let photo = varied(HEIGHT, WIDTH);
    let with_clear = transfer(&photo, Some(Image::rgba(HEIGHT, WIDTH, &rgba)?), &[])?;
    let only_visible = transfer(&photo, Some(rgb(HEIGHT / 2, WIDTH, &kept)?), &[])?;
    assert_close(&with_clear, &only_visible, 1e-5, "transparent pixels ignored");

    let nothing = Image::rgba(1, 2, &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0])?;
    assert_close(&transfer(&photo, Some(nothing), &[])?, &photo, 0.0, "no visible pixel");
    Ok(())
}
