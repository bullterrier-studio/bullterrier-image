//! Resizing the working picture to a longest side (`Options::with_size`), seen through the public API alone: the kernels
//! of the resampling (a shrink and a growth) worked by hand, the size of the other side, and what a constant or a hard
//! edge becomes. A map is resampled by the same code as a picture; the properties are checked on both.

use bullterrier_image::{Error, ErrorKind, Image, Layer, Options, Photo, Picture, Presets, Recipe, Stage, Step};
use proptest::prelude::*;

type Outcome = Result<(), Box<dyn std::error::Error>>;

/// A recipe of one step that changes nothing: the photo is only resized.
fn nothing() -> Recipe {
    Recipe::new([Step::new("grade").opacity(0.0)])
}

/// The picture of `height` x `width` whose pixel `at` is grey `grey(at)` in all three channels, resized to the longest
/// side `longest`.
fn resized(height: usize, width: usize, longest: u32, grey: impl Fn(usize) -> f32) -> Result<Image, Error> {
    let values: Vec<f32> = (0..height * width).flat_map(|at| [grey(at); 3]).collect();
    Photo::new(Picture::new(height, width, &values)?).render(
        &nothing(),
        &Presets::new(),
        Options::default().with_size(longest),
    )
}

/// The first channel of `image`, pixel after pixel.
fn first_channel(image: Image) -> Vec<f32> {
    image
        .into_values()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| pixel[0])
        .collect()
}

/// `actual` is `expected` to within `tolerance`.
fn assert_close(actual: &[f32], expected: &[f64], tolerance: f64) {
    assert_eq!(actual.len(), expected.len());
    for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!((f64::from(*a) - e).abs() <= tolerance, "sample {index}: {a} is not {e}");
    }
}

#[test]
fn two_pixels_shrunk_to_one_are_their_mean_in_every_channel() -> Outcome {
    // the kernel is symmetric about the output pixel's centre, which lies between the two input pixels
    let values = [0.2, 0.9, 0.0, 0.6, 0.1, 1.0];
    let image = Photo::new(Picture::new(1, 2, &values)?).render(
        &nothing(),
        &Presets::new(),
        Options::default().with_size(1),
    )?;
    assert_eq!((image.height(), image.width()), (1, 1));
    assert_close(&image.into_values(), &[0.4, 0.5, 0.5], 1e-7);
    Ok(())
}

#[test]
fn a_growth_from_two_pixels_to_four_follows_the_cubic_kernel_worked_by_hand() -> Outcome {
    // 2 -> 4 pixels: output pixel `x` is centred at (x + 0.5) / 2 in input pixels, so the input pixels in reach lie
    // at distances 0.25 and 1.25 from out 0, 0.25 and 0.75 from out 1, and so on mirrored. Keys' kernel (a = -0.5)
    // weighs a distance of 0.25 by 111/128, 0.75 by 29/128 and 1.25 by -9/128. Scaled to sum to 1 the weights are
    // 37/34 and -3/34 for out 0, 111/140 and 29/140 for out 1; out 2 and out 3 mirror them.
    let image = resized(1, 2, 4, |at| [0.2, 0.6][at])?;
    assert_eq!((image.height(), image.width()), (2, 4));
    let row = [
        (37.0 * 0.2 - 3.0 * 0.6) / 34.0,
        (111.0 * 0.2 + 29.0 * 0.6) / 140.0,
        (29.0 * 0.2 + 111.0 * 0.6) / 140.0,
        (-3.0 * 0.2 + 37.0 * 0.6) / 34.0,
    ];
    // the single input row makes both output rows: one tap of weight 1
    let expected: Vec<f64> = row.iter().chain(&row).copied().collect();
    assert_close(&first_channel(image), &expected, 1e-7);
    Ok(())
}

/// A ramp over a picture one row high: `0.05 + 0.03 x` at pixel `x`.
fn ramp(at: usize) -> f32 {
    0.05 + 0.03 * f32::from(u8::try_from(at).unwrap_or(0))
}

#[test]
fn a_ramp_grown_stays_the_same_ramp_wherever_the_kernel_is_whole() -> Outcome {
    // the output pixel `x` of a growth from 10 to 25 pixels looks at the input at (x + 0.5) 0.4 - 0.5, in pixels;
    // the cubic kernel is exact for a straight line, so the line's value there is the output. Within 2 pixels of
    // either end the kernel is cut off and renormalised, which is allowed to bend the line.
    let image = resized(1, 10, 25, ramp)?;
    assert_eq!((image.height(), image.width()), (2, 25));
    let out = first_channel(image);
    for x in 5..20_u8 {
        let at = (f64::from(x) + 0.5) * 0.4 - 0.5;
        let expected = 0.05 + 0.03 * at;
        for row in 0..2 {
            assert!(
                (f64::from(out[row * 25 + usize::from(x)]) - expected).abs() < 2e-6,
                "x {x}: {} is not {expected}",
                out[row * 25 + usize::from(x)]
            );
        }
    }
    Ok(())
}

#[test]
fn a_ramp_shrunk_stays_the_same_ramp_wherever_the_kernel_is_whole() -> Outcome {
    // 30 to 8 pixels: output pixel `x` looks at the input at (x + 0.5) 3.75 - 0.5. The sharp kernel sampled at whole
    // pixels is not exactly symmetric about that point, so the line is met to 1e-5 (3.5e-6 measured), not exactly.
    let image = resized(1, 30, 8, ramp)?;
    assert_eq!((image.height(), image.width()), (1, 8));
    let out = first_channel(image);
    for x in 3..5_u8 {
        let at = (f64::from(x) + 0.5) * 3.75 - 0.5;
        let expected = 0.05 + 0.03 * at;
        assert!((f64::from(out[usize::from(x)]) - expected).abs() < 1e-5, "x {x}");
    }
    Ok(())
}

/// `shorter` scaled by `longest / longer` and rounded to the nearest whole number, halves to the even one, by integer
/// arithmetic: the rounding of Python's `round` without a float in it.
fn proportional(shorter: usize, longest: usize, longer: usize) -> usize {
    let (whole, rest) = (shorter * longest / longer, shorter * longest % longer);
    let up = match (2 * rest).cmp(&longer) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Equal => whole % 2 == 1,
        std::cmp::Ordering::Less => false,
    };
    (whole + usize::from(up)).max(1)
}

#[test]
fn the_other_side_is_rounded_to_the_nearest_pixel_halves_to_the_even_one_and_is_at_least_one() -> Outcome {
    // (height, width, longest side, the new height and width): 3 x 4 at 2 is 1.5 x 2, a half, to the even 2; 5 x 4
    // at 2 is 2 x 1.6 -> 2; 3 x 8 at 4 is 1.5 x 4 -> 2; 5 x 8 at 4 is 2.5 -> 2 (even); 7 x 8 at 4 is 3.5 -> 4; a
    // very long picture keeps a pixel: 1 x 100 at 10 is 0.1 -> 1; 85 x 170 at 455 is exactly 227.5 (85 * 455 / 170),
    // which a float product lands below (227.49999999999997), so it is the integer arithmetic that gives the even 228
    for (height, width, longest, expected) in [
        (3, 4, 2, (2, 2)),
        (5, 4, 2, (2, 2)),
        (3, 8, 4, (2, 4)),
        (5, 8, 4, (2, 4)),
        (7, 8, 4, (4, 4)),
        (1, 100, 10, (1, 10)),
        (100, 1, 10, (10, 1)),
        (85, 170, 455, (228, 455)),
        (170, 85, 455, (455, 228)),
    ] {
        let image = resized(height, width, longest, |_| 0.5)?;
        assert_eq!(
            (image.height(), image.width()),
            expected,
            "{height} x {width} at {longest}"
        );
    }
    Ok(())
}

#[test]
fn a_picture_that_already_has_its_longest_side_is_not_resized() -> Outcome {
    for (height, width) in [(4, 6), (6, 4), (1, 1)] {
        let longest = u32::try_from(height.max(width))?;
        let values: Vec<f32> = (0..height * width * 3)
            .map(|at| f32::from(u8::try_from(at).unwrap_or(0)) / 100.0)
            .collect();
        let image = Photo::new(Picture::new(height, width, &values)?).render(
            &nothing(),
            &Presets::new(),
            Options::default().with_size(longest),
        )?;
        assert_eq!(image.into_values(), values, "{height} x {width}");
    }
    Ok(())
}

#[test]
fn a_size_that_makes_more_pixels_than_a_picture_may_have_is_refused_naming_the_size_and_the_limit() -> Outcome {
    // the limit is 2^26 = 67,108,864 pixels (8192 x 8192): a square of side 8193 has 67,125,249, a picture of two
    // columns grown to 33,554,433 columns has two more than the limit
    for (side, longest) in [(1, 8_193), (1, 2_000_000_000), (1, u32::MAX), (2, 33_554_433)] {
        let values = vec![0.5; side * side * 3];
        let refused = Photo::new(Picture::new(side, side, &values)?).render(
            &nothing(),
            &Presets::new(),
            Options::default().with_size(longest),
        );
        assert!(
            matches!(&refused, Err(error) if error.kind() == ErrorKind::Recipe
                && error.to_string().contains("`size`") && error.to_string().contains("67108864")),
            "{side} x {side} at {longest}: {refused:?}"
        );
    }
    Ok(())
}

/// Whether every value of `out` is `level` to within a rounding.
fn all_level(out: &[f32], level: f32) -> bool {
    out.iter().all(|sample| (sample - level).abs() < 1e-6)
}

/// The values of a map resampled with the picture it belongs to, as a trace shows them as the subject.
fn map_resized(height: usize, width: usize, longest: u32, values: Vec<f32>) -> Result<Vec<f32>, Error> {
    let pixels = vec![0.5; height * width * 3];
    let photo =
        Photo::new(Picture::new(height, width, &pixels)?).with_map("subject", Layer::new(height, width, values)?)?;
    // the step needs the subject, so the trace shows it
    let recipe = Recipe::new([Step::new("grade").opacity(0.0).mask("subject")]);
    let stages = photo.trace(&recipe, &Presets::new(), Options::default().with_size(longest))?;
    stages
        .into_iter()
        .find_map(|stage| match stage {
            Stage::Subject(layer) => Some(layer.values().to_vec()),
            _ => None,
        })
        .ok_or_else(|| Error::Recipe("the trace shows no subject".to_owned()))
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn a_constant_stays_that_constant_at_any_size_in_any_number_of_channels(
        height in 1_usize..24, width in 1_usize..24, longest in 1_u32..60, level in 0.0_f32..=1.0,
    ) {
        let image = resized(height, width, longest, |_| level)?;
        prop_assert_eq!(image.channels(), 3);
        let out = image.into_values();
        prop_assert!(all_level(&out, level), "{:?}", out);
        // one channel: a map is resampled alike
        let map = map_resized(height, width, longest, vec![level; height * width])?;
        prop_assert!(all_level(&map, level), "{:?}", map);
    }

    #[test]
    fn the_result_stays_within_0_and_1_though_the_kernel_overshoots_at_an_edge(
        height in 1_usize..12, width in 1_usize..12, longest in 1_u32..30,
        seed in prop::collection::vec(any::<bool>(), 144),
    ) {
        // hard edges: every sample 0 or 1
        let on = |at: usize| f32::from(u8::from(seed[at % seed.len()]));
        let out = resized(height, width, longest, on)?.into_values();
        prop_assert!(out.iter().all(|sample| (0.0..=1.0).contains(sample)), "{:?}", out);
        let values: Vec<f32> = (0..height * width).map(on).collect();
        let map = map_resized(height, width, longest, values)?;
        prop_assert!(map.iter().all(|sample| (0.0..=1.0).contains(sample)), "{:?}", map);
    }

    #[test]
    fn the_longest_side_is_exact_and_the_other_is_the_aspect_rounded(
        height in 1_usize..120, width in 1_usize..120, longest in 1_u32..240,
    ) {
        let longest_side = usize::try_from(longest)?;
        let (long, short) = (height.max(width), height.min(width));
        let expected_short = proportional(short, longest_side, long);
        let expected = if height >= width { (longest_side, expected_short) } else { (expected_short, longest_side) };
        let image = resized(height, width, longest, |_| 0.5)?;
        prop_assert_eq!((image.height(), image.width()), expected);
    }
}
