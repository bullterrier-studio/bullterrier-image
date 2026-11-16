//! `dither` with method `tiles`: every dot drawn as one tile of a strip, picked by its tone, between the two palette
//! colours that bracket it. The oracles are strips worked by hand (an all-dark, a half and an all-light tile), the share
//! of lit pixels of a tile against the tone that picks it, and a one-tile strip, which is a fixed pattern.

use std::sync::Arc;

use bullterrier_image::{Error, Image, Options, Photo, Picture, Presets, Recipe, Step};
use serde_json::{Value, json};

use crate::common::dither::{BLACK, WHITE, grey_matrix, near, pixels};

/// `tiles` (as the picture `t`) on `picture`, between `colours` matched in sRGB, with the `extra` parameters.
fn tiled(
    strip: Option<&Image>,
    colours: &[[f32; 3]],
    extra: &[(&str, Value)],
    picture: &(usize, usize, Vec<f32>),
) -> Result<Vec<f32>, Error> {
    let mut step = Step::new("dither")
        .param("method", "tiles")
        .param("palette", "custom")
        .param("custom", json!(colours))
        .param("space", "rgb");
    for (key, value) in extra {
        step = step.param(*key, value.clone());
    }
    let mut photo = Photo::new(Picture::new(picture.0, picture.1, &picture.2)?);
    if let Some(strip) = strip {
        photo = photo.with_image("t", Arc::new(strip.clone()));
        step = step.param("tiles", "t");
    }
    Ok(photo
        .render(&Recipe::new([step]), &Presets::new(), Options::default())?
        .into_values())
}

fn flat(grey: f32, height: usize, width: usize) -> (usize, usize, Vec<f32>) {
    (height, width, vec![grey; height * width * 3])
}

/// A strip of square tiles of `side`, each given as the number of its first pixels (reading order) that are lit.
fn strip(side: usize, lit: &[usize]) -> Result<Image, Error> {
    let width = side * lit.len();
    let greys: Vec<f32> = (0..side * width)
        .map(|at| {
            let (row, column) = (at / width, at % width);
            f32::from(u8::from(row * side + column % side < lit[column / side]))
        })
        .collect();
    grey_matrix(side, width, &greys)
}

fn is_white(pixel: [f32; 3]) -> bool {
    pixel[0] > 0.5
}

#[test]
fn an_all_dark_a_half_and_an_all_light_tile_come_at_the_tones_nought_a_half_and_one() -> Result<(), Error> {
    // side 4: the half tile lights its first two rows
    let tiles = strip(4, &[0, 8, 16])?;
    let drawn = |tone: f32| -> Result<Vec<bool>, Error> {
        Ok(pixels(&tiled(Some(&tiles), &[BLACK, WHITE], &[], &flat(tone, 8, 8))?)
            .into_iter()
            .map(is_white)
            .collect())
    };
    assert_eq!(drawn(0.0)?, vec![false; 64], "tone 0: the dark tile");
    assert_eq!(drawn(1.0)?, vec![true; 64], "tone 1: the light tile");
    let half: Vec<bool> = (0..64).map(|at| (at / 8) % 4 < 2).collect();
    assert_eq!(drawn(0.5)?, half, "tone 0.5: the middle tile, repeated");
    Ok(())
}

#[test]
fn the_share_of_lit_pixels_of_a_flat_tone_is_within_a_tile_step_of_the_tone() -> Result<(), Error> {
    // 5 tiles lighting 0, 4, 8, 12 and 16 of 16 pixels: tone t picks tile floor(5t), which lights k/4; the two are never
    // more than 1/5 apart ((4 - k)/20 below the next tone step, k/20 above the tile's own)
    let tiles = strip(4, &[0, 4, 8, 12, 16])?;
    for (tone, tile) in [(0.1, 0.0), (0.3, 1.0), (0.5, 2.0), (0.7, 3.0), (0.9, 4.0)] {
        let out = pixels(&tiled(Some(&tiles), &[BLACK, WHITE], &[], &flat(tone, 16, 16))?);
        let share = f32::from(u16::try_from(out.iter().filter(|&&pixel| is_white(pixel)).count()).unwrap_or(0)) / 256.0;
        assert!(
            (share - tile / 4.0).abs() < 1e-6,
            "tone {tone}: tile {tile}, share {share}"
        );
        assert!((share - tone).abs() <= 0.2 + 1e-6, "tone {tone}: share {share}");
    }
    Ok(())
}

#[test]
fn one_tile_is_a_fixed_pattern_whatever_the_picture_and_its_size() -> Result<(), Error> {
    let tiles = strip(4, &[5])?;
    let picture = (10, 10, crate::common::pictures::varied(10, 10)); // the last dots are cut
    let out = pixels(&tiled(Some(&tiles), &[BLACK, WHITE], &[], &picture)?);
    for (at, &pixel) in out.iter().enumerate() {
        let (row, column) = (at / 10 % 4, at % 10 % 4);
        assert_eq!(is_white(pixel), row * 4 + column < 5, "pixel {at}");
    }
    Ok(())
}

#[test]
fn a_dot_is_drawn_between_the_two_colours_whose_line_it_lies_on_dark_end_first() -> Result<(), Error> {
    const RED: [f32; 3] = [1.0, 0.0, 0.0];
    const GREEN: [f32; 3] = [0.0, 1.0, 0.0];
    const BLUE: [f32; 3] = [0.0, 0.0, 1.0];
    // two tiles, all dark and all light; the dark end has the lower red, so it is blue and red is the light end: a dot
    // that is `mix` of the way from red to blue lies at 1 - mix on the way to the light end, and green is never used
    let tiles = strip(2, &[0, 4])?;
    for (mix, wanted) in [(0.25, RED), (0.75, BLUE)] {
        let picture = (4, 4, [1.0 - mix, 0.0, mix].repeat(16));
        let out = tiled(Some(&tiles), &[RED, GREEN, BLUE], &[], &picture)?;
        assert!(
            pixels(&out).iter().all(|&pixel| near(pixel, wanted)),
            "{mix}: {:?}",
            &out[..3]
        );
    }
    Ok(())
}

#[test]
fn the_cell_is_unused_the_tile_side_is_the_dot() -> Result<(), Error> {
    let tiles = strip(4, &[0, 6, 16])?;
    let picture = (9, 11, crate::common::pictures::varied(9, 11));
    assert_eq!(
        tiled(Some(&tiles), &[BLACK, WHITE], &[("cell", json!(7))], &picture)?,
        tiled(Some(&tiles), &[BLACK, WHITE], &[], &picture)?
    );
    Ok(())
}

#[test]
fn a_missing_or_malformed_strip_or_spot_inks_are_refused_naming_the_cause() -> Result<(), Error> {
    let picture = flat(0.5, 8, 8);
    let ragged = grey_matrix(4, 10, &[0.5; 40])?; // 10 is not a multiple of 4
    let fine = strip(4, &[0, 16])?;
    for (strip, extra, says) in [
        (None, vec![], "tiles"),
        (Some(&ragged), vec![], "multiple"),
        (Some(&fine), vec![("separation", json!("spot"))], "spot"),
    ] {
        let refused = tiled(strip, &[BLACK, WHITE], &extra, &picture);
        assert!(
            matches!(&refused, Err(error) if error.to_string().contains(says)),
            "{says}: {refused:?}"
        );
    }
    Ok(())
}
