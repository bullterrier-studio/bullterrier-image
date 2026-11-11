//! Resizing a picture to a longest side, and reading a colour as a person writes it.

use crate::common::pictures::varied;
use bullterrier_image::params::Color;
use bullterrier_image::pipeline::resize;
use bullterrier_image::{Error, Image};
use serde_json::json;

#[test]
fn the_longest_side_becomes_the_size_and_the_other_keeps_the_aspect() -> Result<(), Error> {
    for (height, width, longest, expected) in [
        (40, 80, 20, (10, 20)),
        (80, 40, 20, (20, 10)),
        (30, 50, 100, (60, 100)),
        (3, 100, 10, (1, 10)),
    ] {
        let out = resize(&Image::new(height, width, &varied(height, width))?, longest)?;
        assert_eq!((out.height(), out.width()), expected, "{height}x{width} to {longest}");
    }
    Ok(())
}

#[test]
fn a_picture_that_already_has_the_size_comes_back_unchanged() -> Result<(), Error> {
    let image = Image::new(12, 16, &varied(12, 16))?;
    assert_eq!(resize(&image, 16)?.into_values(), image.into_values());
    Ok(())
}

#[test]
fn a_flat_picture_stays_flat_and_a_sharp_one_never_leaves_0_to_1() -> Result<(), Error> {
    let flat = Image::new(30, 40, &[0.25; 30 * 40 * 3])?;
    for longest in [10, 100] {
        assert!(
            resize(&flat, longest)?
                .into_values()
                .iter()
                .all(|v| (v - 0.25).abs() < 1e-6)
        );
    }
    let stripes: Vec<f32> = (0..30 * 40)
        .flat_map(|i| [f32::from(u8::from((i % 40) % 2 == 0)); 3])
        .collect();
    let image = Image::new(30, 40, &stripes)?;
    for longest in [13, 160] {
        assert!(
            resize(&image, longest)?
                .into_values()
                .iter()
                .all(|v| (0.0..=1.0).contains(v))
        );
    }
    Ok(())
}

#[test]
fn a_shrink_averages_what_it_covers_and_alpha_is_resized_with_it() -> Result<(), Error> {
    // left half black, right half white: the middle column pair of a 4-column shrink to 2 are the two halves' own levels
    let values: Vec<f32> = (0..4 * 8).flat_map(|i| [f32::from(u8::from(i % 8 >= 4)); 4]).collect();
    let out = resize(&Image::rgba(4, 8, &values)?, 4)?;
    assert_eq!((out.height(), out.width(), out.channels()), (2, 4, 4));
    let pixels = out.into_values();
    assert!(pixels[0] < 0.05 && pixels[3 * 4] > 0.95, "ends keep their levels");
    assert!(
        pixels[4] < 0.4 && pixels[2 * 4] > 0.6,
        "the middle goes from dark to light"
    );
    Ok(())
}

#[test]
fn a_colour_is_read_from_hex_or_three_numbers_and_nothing_else() {
    let good = [
        (json!("#ff0000"), [1.0, 0.0, 0.0]),
        (json!(" #0AF "), [0.0, 2.0 / 3.0, 1.0]),
        (json!("#000000"), [0.0, 0.0, 0.0]),
        (json!([0.25, 0.5, 1]), [0.25, 0.5, 1.0]),
    ];
    for (written, expected) in good {
        let parsed = Color::parse(&written).map(|c| c.0);
        let close = parsed.is_some_and(|c| c.iter().zip(expected).all(|(a, b)| (f64::from(*a) - b).abs() < 1e-6));
        assert!(close, "{written}: {parsed:?}");
    }
    for bad in [
        json!("red"),
        json!("#ff00"),
        json!("#gg0000"),
        json!([0.1, 0.2]),
        json!([0.1, 0.2, 1.5]),
        json!([0.1, 0.2, -0.1]),
        json!(null),
        json!(true),
    ] {
        assert!(Color::parse(&bad).is_none(), "{bad}");
    }
}

#[test]
fn the_channels_of_a_colour_are_the_exact_decimals_of_its_bytes() {
    assert_eq!(
        Color::channels(&json!("#102030")),
        Some([16.0 / 255.0, 32.0 / 255.0, 48.0 / 255.0])
    );
    assert_eq!(Color::channels(&json!([0.1, 0.2, 0.3])), Some([0.1, 0.2, 0.3]));
    assert_eq!(Color::channels(&json!("#12")), None);
}
