//! The `blend` effect: a second picture laid over the photo with a blend mode, fitted by cover, contain, stretch, width
//! or tile. Every expectation is worked out by hand from the definitions: a rectangle counted in pixels, a mode's
//! formula at a corner value (white, black), alpha as a weight.

use std::sync::Arc;

use crate::common::compare::assert_close;
use crate::common::pictures::varied;
use bullterrier_image::{Error, Image, Options, Photo, Picture, Presets, Recipe, Step};
use serde_json::Value;

const HEIGHT: usize = 8;
const WIDTH: usize = 10;

/// The varied 8 x 10 photo of the tests.
fn photo_values() -> Vec<f32> {
    varied(HEIGHT, WIDTH)
}

/// A picture of `rows` x `columns` whose pixels are `pixels` (RGBA), row after row.
fn asset(rows: usize, columns: usize, pixels: &[[f32; 4]]) -> Result<Arc<Image>, Error> {
    Ok(Arc::new(Image::rgba(rows, columns, &pixels.concat())?))
}

/// The photo with the step `blend` given `image` = the asset and the `params`, rendered at opacity 1.
fn laid(asset: Arc<Image>, params: &[(&str, Value)], threads: usize) -> Result<Vec<f32>, Error> {
    let values = photo_values();
    let photo = Photo::new(Picture::new(HEIGHT, WIDTH, &values)?).with_picture("asset", asset);
    let mut step = Step::new("blend").param("image", "asset");
    for (key, value) in params {
        step = step.param(*key, value.clone());
    }
    let options = Options::default().with_threads(threads);
    Ok(photo
        .render(&Recipe::new([step]), &Presets::new(), options)?
        .into_values())
}

fn lay(asset: Arc<Image>, params: &[(&str, Value)]) -> Result<Vec<f32>, Error> {
    laid(asset, params, 1)
}

const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Whether pixel (`row`, `column`) of `made` is the photo's own.
fn untouched(made: &[f32], row: usize, column: usize) -> bool {
    let values = photo_values();
    let at = (row * WIDTH + column) * 3;
    made[at..at + 3] == values[at..at + 3]
}

/// Whether every pixel of `made` is black exactly in the rectangle rows `rows`, columns `columns` and the photo's own
/// outside it.
fn black_only_in(made: &[f32], rows: std::ops::Range<usize>, columns: std::ops::Range<usize>) -> bool {
    (0..HEIGHT).all(|row| {
        (0..WIDTH).all(|column| {
            let at = (row * WIDTH + column) * 3;
            if rows.contains(&row) && columns.contains(&column) {
                made[at..at + 3].iter().all(|&level| level == 0.0)
            } else {
                untouched(made, row, column)
            }
        })
    })
}

fn s(text: &str) -> Value {
    Value::from(text)
}

#[test]
fn normal_over_a_full_cover_opaque_picture_is_the_picture() -> Result<(), Error> {
    let values = varied(HEIGHT, WIDTH);
    let mut pixels = Vec::new();
    for rgb in values.chunks(3) {
        pixels.push([rgb[0] * 0.5, rgb[1] * 0.5 + 0.25, 1.0 - rgb[2], 1.0]);
    }
    let made = lay(
        asset(HEIGHT, WIDTH, &pixels)?,
        &[("mode", s("normal")), ("fit", s("stretch"))],
    )?;
    let expected: Vec<f32> = pixels.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    assert_close(&made, &expected, 1e-5, "the picture");
    Ok(())
}

#[test]
fn multiply_by_white_keeps_the_photo_and_by_black_blacks_it_out() -> Result<(), Error> {
    let photo = photo_values();
    let white = lay(asset(1, 1, &[WHITE])?, &[("mode", s("multiply"))])?;
    assert_close(&white, &photo, 1e-6, "multiply white");
    let black = lay(asset(1, 1, &[BLACK])?, &[("mode", s("multiply"))])?;
    assert_close(&black, &vec![0.0; photo.len()], 1e-6, "multiply black");
    Ok(())
}

#[test]
fn screen_by_black_keeps_the_photo() -> Result<(), Error> {
    let made = lay(asset(1, 1, &[BLACK])?, &[("mode", s("screen"))])?;
    assert_close(&made, &photo_values(), 1e-6, "screen black");
    Ok(())
}

#[test]
fn the_pictures_alpha_is_the_cover_of_the_blend() -> Result<(), Error> {
    let photo = photo_values();
    let clear = lay(asset(1, 1, &[[1.0, 0.0, 0.0, 0.0]])?, &[("mode", s("normal"))])?;
    assert_close(&clear, &photo, 1e-6, "alpha 0");
    let half = lay(asset(1, 1, &[[1.0, 1.0, 1.0, 0.5]])?, &[("mode", s("normal"))])?;
    let expected: Vec<f32> = photo.iter().map(|level| level * 0.5 + 0.5).collect();
    assert_close(&half, &expected, 1e-6, "alpha 0.5 mixes half");
    Ok(())
}

#[test]
fn no_picture_leaves_the_photo_unchanged() -> Result<(), Error> {
    let values = photo_values();
    let photo = Photo::new(Picture::new(HEIGHT, WIDTH, &values)?);
    let made = photo.render(&Recipe::new([Step::new("blend")]), &Presets::new(), Options::default())?;
    assert_close(&made.into_values(), &values, 0.0, "no picture");
    Ok(())
}

#[test]
fn contain_centres_the_picture_and_leaves_the_border_alone() -> Result<(), Error> {
    // a 1 x 2 picture in an 8 x 10 photo: factor min(10 / 2, 8 / 1) = 5, so 5 rows x 10 columns; the 3 spare rows split
    // 1.5 / 1.5, and the half goes to the even row: the picture starts at row 2 (rows 2..7)
    let made = lay(
        asset(1, 2, &[BLACK, WHITE])?,
        &[("mode", s("normal")), ("fit", s("contain")), ("resample", s("nearest"))],
    )?;
    for row in 0..HEIGHT {
        for column in 0..WIDTH {
            let at = (row * WIDTH + column) * 3;
            if (2..7).contains(&row) {
                let level = if column < 5 { 0.0 } else { 1.0 };
                assert_eq!(made[at..at + 3], [level; 3], "row {row}, column {column}");
            } else {
                assert!(untouched(&made, row, column), "row {row}, column {column}");
            }
        }
    }
    Ok(())
}

#[test]
fn width_with_a_scale_makes_the_picture_that_share_of_the_photos_width_at_its_anchor() -> Result<(), Error> {
    // a square picture: as wide as the photo (10), times 0.5, is 5 x 5
    let square = asset(4, 4, &[BLACK; 16])?;
    let top_left = lay(
        Arc::clone(&square),
        &[
            ("mode", s("normal")),
            ("fit", s("width")),
            ("scale", Value::from(0.5)),
            ("at", s("top_left")),
        ],
    )?;
    assert!(black_only_in(&top_left, 0..5, 0..5));
    // the anchor decides where the free space goes: all of it before a picture at the bottom right (3 rows, 5 columns)
    let bottom_right = lay(
        square,
        &[
            ("mode", s("normal")),
            ("fit", s("width")),
            ("scale", Value::from(0.5)),
            ("at", s("bottom_right")),
        ],
    )?;
    assert!(black_only_in(&bottom_right, 3..8, 5..10));
    Ok(())
}

#[test]
fn margin_keeps_that_many_pixels_clear_on_every_side() -> Result<(), Error> {
    // a margin is in design px: 96 of them are 96 x 10 / 480 = 2 px of this 10 px wide photo
    let made = lay(
        asset(4, 4, &[BLACK; 16])?,
        &[
            ("mode", s("normal")),
            ("fit", s("stretch")),
            ("margin", Value::from(96)),
        ],
    )?;
    assert!(black_only_in(&made, 2..6, 2..8));
    Ok(())
}

#[test]
fn tile_repeats_the_picture_with_the_period_of_its_size() -> Result<(), Error> {
    // a 2 x 2 checker as wide as the photo (10) times 0.5: tiles of 5 x 5 pixels, each quadrant a nearest-neighbour block
    let checker = asset(2, 2, &[BLACK, WHITE, WHITE, BLACK])?;
    let made = lay(
        checker,
        &[
            ("mode", s("normal")),
            ("fit", s("tile")),
            ("scale", Value::from(0.5)),
            ("at", s("top_left")),
            ("resample", s("nearest")),
        ],
    )?;
    // the source pixel of tile pixel `i` (0..5) along a side of 2 is floor((i + 0.5) * 2 / 5)
    let source = |index: usize| (2 * index + 1) / 5;
    for row in 0..HEIGHT {
        for column in 0..WIDTH {
            let (r, c) = (row % 5, column % 5);
            let level = if (source(r) + source(c)) % 2 == 0 { 0.0 } else { 1.0 };
            let at = (row * WIDTH + column) * 3;
            assert_eq!(made[at..at + 3], [level; 3], "row {row}, column {column}");
        }
    }
    Ok(())
}

#[test]
fn the_result_is_the_same_at_any_thread_count() -> Result<(), Error> {
    let pixels: Vec<[f32; 4]> = (0..12_u8)
        .map(|i| [f32::from(i) / 12.0, 0.5, 1.0 - f32::from(i) / 12.0, 0.75])
        .collect();
    let params = [("mode", s("overlay")), ("fit", s("cover")), ("scale", Value::from(1.5))];
    let single = lay(asset(3, 4, &pixels)?, &params)?;
    for threads in [2, 3, 10] {
        let many = laid(asset(3, 4, &pixels)?, &params, threads)?;
        assert!(single == many, "threads {threads}");
    }
    Ok(())
}

#[test]
fn a_very_long_picture_covering_the_photo_is_sized_only_where_it_shows() -> Result<(), Error> {
    // 1 x 4000 over 8 x 10: cover sizes it 80000 x 8 before the crop; only the 10 columns that show are made
    let strip = vec![[0.25, 0.5, 0.75, 1.0]; 4000];
    let made = lay(asset(1, 4000, &strip)?, &[("mode", s("normal"))])?;
    assert_close(&made, &[0.25, 0.5, 0.75].repeat(HEIGHT * WIDTH), 1e-5, "the strip");
    Ok(())
}

#[test]
fn a_margin_larger_than_the_photo_still_leaves_a_pixel_row_and_column_each_side_of_the_middle() -> Result<(), Error> {
    // 216 design px are 4.5 px here, which rounds to 4: more than the photo has room for. The most a side keeps is
    // (8 - 1) / 2 = 3 (the shorter side is 8), which leaves the area rows 3..5, columns 3..7
    let made = lay(
        asset(4, 4, &[BLACK; 16])?,
        &[
            ("mode", s("normal")),
            ("fit", s("stretch")),
            ("margin", Value::from(216)),
        ],
    )?;
    assert!(black_only_in(&made, 3..5, 3..7));
    Ok(())
}

#[test]
fn the_modes_the_effect_offers_are_the_seven_of_a_recipe_step_in_their_order() {
    let offered = <bullterrier_image::BlendMode as bullterrier_image::params::Choice>::CHOICES;
    let of_a_step: Vec<&str> = bullterrier_image::BlendMode::ALL
        .iter()
        .map(|mode| mode.name())
        .collect();
    assert_eq!(offered, of_a_step.as_slice());
}

#[test]
fn a_half_pixel_of_size_or_position_goes_to_the_even_one() -> Result<(), Error> {
    // contain a 1 x 2 picture: 5 x 10 at scale 1; at scale 0.5 the height 2.5 rounds to 2 (not 3), the width is 5
    let half = lay(
        asset(1, 2, &[BLACK, BLACK])?,
        &[
            ("mode", s("normal")),
            ("fit", s("contain")),
            ("at", s("top_left")),
            ("scale", Value::from(0.5)),
        ],
    )?;
    assert!(black_only_in(&half, 0..2, 0..5));
    // at scale 0.6 it is 3 x 6; the 5 spare rows (2.5 before it) put it at row 2 (not 3), and the 4 spare columns at 2
    let moved = lay(
        asset(1, 2, &[BLACK, BLACK])?,
        &[
            ("mode", s("normal")),
            ("fit", s("contain")),
            ("scale", Value::from(0.6)),
        ],
    )?;
    assert!(black_only_in(&moved, 2..5, 2..8));
    Ok(())
}

#[test]
fn a_tile_that_starts_inside_the_photo_continues_before_its_start() -> Result<(), Error> {
    // 5 x 5 tiles of a 2 x 2 checker centred: 5 spare columns, the 2.5 before them going to the even 2, so the tile starts
    // at column 2 and the pixels before it show the tile's end
    let checker = asset(2, 2, &[BLACK, WHITE, WHITE, BLACK])?;
    let made = lay(
        checker,
        &[
            ("mode", s("normal")),
            ("fit", s("tile")),
            ("scale", Value::from(0.5)),
            ("resample", s("nearest")),
        ],
    )?;
    let source = |index: usize| (2 * index + 1) / 5;
    // rows: 8 - 5 = 3 spare, 1.5 before them goes to the even 2: the tile starts at row 2
    for row in 0..HEIGHT {
        for column in 0..WIDTH {
            let (r, c) = ((row + 5 - 2) % 5, (column + 5 - 2) % 5);
            let level = if (source(r) + source(c)) % 2 == 0 { 0.0 } else { 1.0 };
            let at = (row * WIDTH + column) * 3;
            assert_eq!(made[at..at + 3], [level; 3], "row {row}, column {column}");
        }
    }
    Ok(())
}

#[test]
fn each_anchor_puts_the_picture_at_its_place_in_the_free_space() -> Result<(), Error> {
    // a 5 x 5 picture in 8 x 10: 3 spare rows and 5 spare columns; a half goes to the even pixel (1.5 to 2, 2.5 to 2)
    let square = asset(4, 4, &[BLACK; 16])?;
    for (anchor, top, left) in [
        ("top_left", 0, 0),
        ("top", 0, 2),
        ("top_right", 0, 5),
        ("left", 2, 0),
        ("center", 2, 2),
        ("right", 2, 5),
        ("bottom_left", 3, 0),
        ("bottom", 3, 2),
        ("bottom_right", 3, 5),
    ] {
        let made = lay(
            Arc::clone(&square),
            &[
                ("mode", s("normal")),
                ("fit", s("width")),
                ("scale", Value::from(0.5)),
                ("at", s(anchor)),
            ],
        )?;
        assert!(black_only_in(&made, top..top + 5, left..left + 5), "{anchor}");
    }
    Ok(())
}

#[test]
fn bilinear_interpolates_between_pixel_centres_and_holds_the_edge_values() -> Result<(), Error> {
    // black then white stretched from 2 to 10 columns: output pixel i is centred at (i + 0.5) / 5 input pixels, the input
    // pixels are centred at 0.5 and 1.5, and between them the value rises linearly from 0 to 1
    let made = lay(
        asset(1, 2, &[BLACK, WHITE])?,
        &[
            ("mode", s("normal")),
            ("fit", s("stretch")),
            ("resample", s("bilinear")),
        ],
    )?;
    let expected = [0.0, 0.0, 0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.0, 1.0];
    for row in 0..HEIGHT {
        let line: Vec<f32> = (0..WIDTH).map(|column| made[(row * WIDTH + column) * 3]).collect();
        assert_close(&line, &expected, 1e-5, "a row");
    }
    Ok(())
}

#[test]
fn every_filter_keeps_a_flat_colour_flat_when_it_sizes_it() -> Result<(), Error> {
    for resample in ["nearest", "bilinear", "bicubic", "lanczos"] {
        for (rows, columns) in [(3, 3), (20, 30)] {
            let flat = asset(rows, columns, &vec![[0.25, 0.5, 0.75, 1.0]; rows * columns])?;
            let made = lay(
                flat,
                &[("mode", s("normal")), ("fit", s("stretch")), ("resample", s(resample))],
            )?;
            assert_close(&made, &[0.25, 0.5, 0.75].repeat(HEIGHT * WIDTH), 1e-5, resample);
        }
    }
    Ok(())
}
