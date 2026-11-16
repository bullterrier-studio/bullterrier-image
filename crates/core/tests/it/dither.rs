//! `dither`: the picture redrawn in a few colours. The oracles are values worked by hand (error passed along a short
//! column, the Bayer matrix from its closed form, the sRGB grey whose Oklab lightness is 0.5), the published colours of the
//! fixed palettes typed in the test, and properties: only palette colours appear, each pixel takes the nearest one, a
//! flat area keeps its mean, an ordered pattern repeats.

use std::sync::Arc;

use bullterrier_image::{Error, Image, Options, Photo, Picture, Presets, Recipe, Step};
use serde_json::{Value, json};
use bullterrier_image::effects::dither::{Init, Method, Palette, Params, Separation, Space};
use bullterrier_image::params::Color;

use crate::common::pictures::varied;
use crate::common::pixels::run_picture;

const BLACK: [f32; 3] = [0.0; 3];
const WHITE: [f32; 3] = [1.0; 3];

/// Black and white, matched in sRGB itself (so a grey splits at 0.5), with the dot pattern `pattern` of `method`.
fn ink(method: Method, pattern: f32) -> Params {
    Params {
        method,
        palette: Palette::Custom,
        custom: vec![Color(BLACK), Color(WHITE)],
        space: Space::Rgb,
        pattern,
        ..Params::default()
    }
}

/// A grey picture, one value after another, `width` wide.
fn greys(values: &[f32], width: usize) -> (usize, usize, Vec<f32>) {
    (
        values.len() / width,
        width,
        values.iter().flat_map(|&grey| [grey; 3]).collect(),
    )
}

/// The red channel of every pixel of `picture`.
fn reds(picture: &[f32]) -> Vec<f32> {
    picture.as_chunks::<3>().0.iter().map(|pixel| pixel[0]).collect()
}

/// The pixels of `picture`.
fn pixels(picture: &[f32]) -> Vec<[f32; 3]> {
    picture
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect()
}

fn run(params: &Params, (height, width, picture): &(usize, usize, Vec<f32>)) -> Result<Vec<f32>, Error> {
    run_picture(params, *height, *width, picture)
}

fn near(pixel: [f32; 3], colour: [f32; 3]) -> bool {
    pixel.iter().zip(colour).all(|(a, b)| (a - b).abs() < 1e-5)
}

fn rgb(hex: u32) -> [f32; 3] {
    [hex >> 16, (hex >> 8) & 0xff, hex & 0xff].map(|byte| f32::from(u8::try_from(byte).unwrap_or(0)) / 255.0)
}

// ---- the colours --------------------------------------------------------------------------------------------------------

#[test]
fn a_fixed_palette_paints_with_its_published_colours_only() -> Result<(), Error> {
    let gameboy = [0x0f_380f, 0x30_6230, 0x8b_ac0f, 0x9b_bc0f];
    let cga = [0x00_0000, 0x55_ffff, 0xff_55ff, 0xff_ffff];
    let ega = [
        0x00_0000, 0x00_00aa, 0x00_aa00, 0x00_aaaa, 0xaa_0000, 0xaa_00aa, 0xaa_5500, 0xaa_aaaa, 0x55_5555, 0x55_55ff,
        0x55_ff55, 0x55_ffff, 0xff_5555, 0xff_55ff, 0xff_ff55, 0xff_ffff,
    ];
    let pico8 = [
        0x00_0000, 0x1d_2b53, 0x7e_2553, 0x00_8751, 0xab_5236, 0x5f_574f, 0xc2_c3c7, 0xff_f1e8, 0xff_004d, 0xff_a300,
        0xff_ec27, 0x00_e436, 0x29_adff, 0x83_769c, 0xff_77a8, 0xff_ccaa,
    ];
    let picture = (20, 20, varied(20, 20));
    for (palette, colours) in [
        (Palette::Gameboy, gameboy.to_vec()),
        (Palette::Cga, cga.to_vec()),
        (Palette::Ega, ega.to_vec()),
        (Palette::Pico8, pico8.to_vec()),
    ] {
        let colours: Vec<[f32; 3]> = colours.into_iter().map(rgb).collect();
        for method in [
            Method::FloydSteinberg,
            Method::Atkinson,
            Method::Bayer,
            Method::BlueNoise,
        ] {
            let params = Params {
                method,
                palette,
                ..Params::default()
            };
            let out = pixels(&run(&params, &picture)?);
            assert!(
                out.iter()
                    .all(|&pixel| colours.iter().any(|&colour| near(pixel, colour))),
                "{palette:?} {method:?}: a colour outside the palette"
            );
        }
    }
    Ok(())
}

#[test]
fn a_cube_palette_has_evenly_spaced_levels_in_every_channel_and_uses_them_all() -> Result<(), Error> {
    let picture = (16, 16, varied(16, 16));
    for (palette, levels) in [
        (Palette::Cube2, 2_u8),
        (Palette::Cube3, 3),
        (Palette::Cube4, 4),
        (Palette::Cube5, 5),
        (Palette::Cube6, 6),
    ] {
        let params = Params {
            palette,
            space: Space::Rgb,
            pattern: 0.0,
            ..Params::default()
        };
        let out = run(&params, &picture)?;
        let allowed: Vec<f32> = (0..levels)
            .map(|step| f32::from(step) / f32::from(levels - 1))
            .collect();
        assert!(
            out.iter()
                .all(|&value| allowed.iter().any(|&level| (level - value).abs() < 1e-6)),
            "{palette:?}: a level off the grid"
        );
        // snapped on its own, each channel is the level nearest to the input
        for (got, &given) in out.iter().zip(&picture.2) {
            let best = allowed
                .iter()
                .map(|&level| (level - given).abs())
                .fold(f32::INFINITY, f32::min);
            assert!((got - given).abs() <= best + 1e-6, "{palette:?}: {given} became {got}");
        }
    }
    Ok(())
}

#[test]
fn your_own_colours_are_all_there_is_whatever_the_method_and_pattern() -> Result<(), Error> {
    let own = [[0.04, 0.06, 0.15], [1.0, 0.35, 0.21], [0.97, 0.9, 0.78]];
    let picture = (24, 24, varied(24, 24));
    for method in [
        Method::FloydSteinberg,
        Method::Atkinson,
        Method::Bayer,
        Method::BlueNoise,
    ] {
        for pattern in [0.0, 0.3, 1.0] {
            for space in [Space::Oklab, Space::Rgb] {
                let params = Params {
                    method,
                    palette: Palette::Custom,
                    custom: own.iter().map(|&colour| Color(colour)).collect(),
                    pattern,
                    space,
                    ..Params::default()
                };
                let out = pixels(&run(&params, &picture)?);
                assert!(
                    out.iter().all(|&pixel| own.iter().any(|&colour| near(pixel, colour))),
                    "{method:?} {pattern} {space:?}: a colour outside the list"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn with_no_pattern_every_dot_takes_the_nearest_colour_of_the_list_in_the_space() -> Result<(), Error> {
    let own = [[0.1, 0.1, 0.1], [0.9, 0.2, 0.2], [0.2, 0.8, 0.3], [0.3, 0.3, 0.95]];
    let picture = (20, 20, varied(20, 20));
    let params = Params {
        palette: Palette::Custom,
        custom: own.iter().map(|&colour| Color(colour)).collect(),
        space: Space::Rgb,
        pattern: 0.0,
        ..Params::default()
    };
    for (got, given) in pixels(&run(&params, &picture)?).iter().zip(pixels(&picture.2)) {
        let distance = |colour: &[f32; 3]| colour.iter().zip(given).map(|(a, b)| (a - b) * (a - b)).sum::<f32>();
        let best = own.iter().map(distance).fold(f32::INFINITY, f32::min);
        assert!(distance(got) <= best + 1e-6, "{given:?} became {got:?}");
    }
    Ok(())
}

#[test]
fn oklab_splits_black_from_white_at_the_grey_of_lightness_one_half_and_rgb_at_one_half() -> Result<(), Error> {
    // Oklab lightness is the cube root of linear light for a grey: 0.5 is a linear 0.125, an sRGB grey of about 0.389
    let picture = greys(&[0.38, 0.40, 0.49, 0.51], 4);
    let split = |space| Params {
        space,
        ..ink(Method::FloydSteinberg, 0.0)
    };
    assert_eq!(reds(&run(&split(Space::Oklab), &picture)?), [0.0, 1.0, 1.0, 1.0]);
    assert_eq!(reds(&run(&split(Space::Rgb), &picture)?), [0.0, 0.0, 0.0, 1.0]);
    Ok(())
}

// ---- error diffusion ----------------------------------------------------------------------------------------------------

#[test]
fn floyd_steinberg_passes_seven_sixteenths_right_and_five_sixteenths_down() -> Result<(), Error> {
    let diffuse = ink(Method::FloydSteinberg, 1.0);
    // 0.6 becomes white and leaves an error of -0.4; 7/16 of it makes the pixel to the right 0.62 - 0.175 = 0.445: black
    // (an eighth of it, or 3/16, would leave it above one half)
    assert_eq!(reds(&run(&diffuse, &greys(&[0.6, 0.62], 2))?), [1.0, 0.0]);
    // 5/16 of it makes the pixel below 0.6 - 0.125 = 0.475: black
    assert_eq!(reds(&run(&diffuse, &greys(&[0.6, 0.6], 1))?), [1.0, 0.0]);
    // 0.45 -> black (error 0.45); right 0.6 + 7/16 x 0.45 = 0.797 -> white (error -0.203); below 0.6 + 5/16 x 0.45 - 3/16 x
    // 0.203 = 0.7025 -> white (error -0.2975); the last 0.45 + 1/16 x 0.45 - 5/16 x 0.203 - 7/16 x 0.2975 = 0.284 -> black
    assert_eq!(
        reds(&run(&diffuse, &greys(&[0.45, 0.6, 0.6, 0.45], 2))?),
        [0.0, 1.0, 1.0, 0.0]
    );
    Ok(())
}

#[test]
fn atkinson_passes_an_eighth_to_each_of_six_pixels_including_two_rows_down() -> Result<(), Error> {
    // 0.6 -> white, error -0.4, an eighth (-0.05) to each of the next two rows; 0.5 - 0.05 = 0.45 -> black, error 0.45, an
    // eighth (0.05625) to each of the two rows below; 0.5 - 0.05 + 0.05625 = 0.50625 -> white, error -0.49375; the last
    // 0.5 + 0.05625 - 0.0617 = 0.4945 -> black
    let column = greys(&[0.6, 0.5, 0.5, 0.5], 1);
    assert_eq!(reds(&run(&ink(Method::Atkinson, 1.0), &column)?), [1.0, 0.0, 1.0, 0.0]);
    // 0.6 -> white (-0.05 to each of the next two rows); 0.5 -> 0.45 black (+0.05625 to the next two); 0.45 - 0.05 + 0.05625
    // = 0.456 stays black, where the first row's tap two rows down missing would leave 0.506 and make it white
    let two_down = greys(&[0.6, 0.5, 0.45], 1);
    assert_eq!(reds(&run(&ink(Method::Atkinson, 1.0), &two_down)?), [1.0, 0.0, 0.0]);
    Ok(())
}

#[test]
fn no_pattern_passes_no_error_on() -> Result<(), Error> {
    let picture = greys(&[0.6, 0.5, 0.45, 0.55], 4);
    for method in [
        Method::FloydSteinberg,
        Method::Atkinson,
        Method::Bayer,
        Method::BlueNoise,
    ] {
        assert_eq!(
            reds(&run(&ink(method, 0.0), &picture)?),
            [1.0, 0.0, 0.0, 1.0],
            "{method:?}"
        );
    }
    Ok(())
}

#[test]
fn floyd_steinberg_keeps_the_mean_of_a_flat_area() -> Result<(), Error> {
    for grey in [0.1_f32, 0.3, 0.5, 0.77] {
        let picture = (40, 40, vec![grey; 40 * 40 * 3]);
        let out = reds(&run(&ink(Method::FloydSteinberg, 1.0), &picture)?);
        let mean = out.iter().sum::<f32>() / 1600.0;
        assert!((mean - grey).abs() < 0.02, "grey {grey}: mean {mean}");
        assert!(
            out.iter()
                .all(|&value| value.abs() < 1e-6 || (value - 1.0).abs() < 1e-6)
        );
    }
    Ok(())
}

#[test]
fn atkinson_drops_a_quarter_of_the_error_so_a_light_grey_comes_out_lighter_than_it_is() -> Result<(), Error> {
    let picture = (40, 40, vec![0.9_f32; 40 * 40 * 3]);
    let mean =
        |method| -> Result<f32, Error> { Ok(reds(&run(&ink(method, 1.0), &picture)?).iter().sum::<f32>() / 1600.0) };
    assert!(mean(Method::Atkinson)? > mean(Method::FloydSteinberg)? + 0.03);
    Ok(())
}

// ---- ordered dithering ------------------------------------------------------------------------------------------------

/// The rank of the 8 x 8 Bayer matrix at (`row`, `column`) from its closed form: bit k of `row ^ column` is bit 2 (2 - k) + 1
/// of the rank, bit k of `row` bit 2 (2 - k).
fn bayer_rank(row: usize, column: usize) -> usize {
    bayer_rank_of(3, row, column)
}

/// The rank of the Bayer matrix of side 2^`bits` at (`row`, `column`), by the same closed form.
fn bayer_rank_of(bits: usize, row: usize, column: usize) -> usize {
    let mixed = row ^ column;
    (0..bits)
        .map(|bit| {
            (((mixed >> bit) & 1) << (2 * (bits - 1 - bit) + 1)) | (((row >> bit) & 1) << (2 * (bits - 1 - bit)))
        })
        .sum()
}

#[test]
fn bayer_lights_the_pixels_of_a_flat_grey_whose_rank_is_in_the_top_share() -> Result<(), Error> {
    // a grey of k/16 mixes k whites with 16 - k blacks; a pixel is white when floor(16 x level) >= 16 - k, and a level is
    // (rank + 0.5) / 64
    for whites in [1_usize, 4, 8, 12, 15] {
        #[expect(clippy::cast_precision_loss, reason = "a small count")]
        let grey = whites as f32 / 16.0;
        let picture = (8, 8, vec![grey; 8 * 8 * 3]);
        let out = reds(&run(&ink(Method::Bayer, 1.0), &picture)?);
        for row in 0..8 {
            for column in 0..8 {
                let lit = bayer_rank(row, column) / 4 >= 16 - whites;
                assert_eq!(out[row * 8 + column] > 0.5, lit, "{whites}/16 at ({row}, {column})");
            }
        }
    }
    Ok(())
}

#[test]
fn an_ordered_pattern_is_deterministic_and_repeats_with_its_tile() -> Result<(), Error> {
    for (method, side) in [(Method::Bayer, 8), (Method::BlueNoise, 64)] {
        let size = side + 3;
        let flat = (size, size, vec![0.37_f32; size * size * 3]);
        let out = reds(&run(&ink(method, 1.0), &flat)?);
        assert_eq!(
            out,
            reds(&run(&ink(method, 1.0), &flat)?),
            "{method:?} is deterministic"
        );
        for row in 0..size - side {
            for column in 0..size {
                assert_eq!(
                    out[row * size + column],
                    out[(row + side) * size + column],
                    "{method:?}"
                );
                assert_eq!(out[column * size + row], out[column * size + row + side], "{method:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn blue_noise_lights_an_eighth_of_the_tile_with_almost_no_lit_dots_touching() -> Result<(), Error> {
    let flat = (64, 64, vec![0.125_f32; 64 * 64 * 3]);
    let out = reds(&run(&ink(Method::BlueNoise, 1.0), &flat)?);
    assert_eq!(out.iter().filter(|&&value| value > 0.5).count(), 512);
    let mut touching = 0;
    for row in 0..64_usize {
        for column in 0..64_usize {
            for (down, across) in [(0_isize, 1_isize), (1, -1), (1, 0), (1, 1)] {
                let (next_row, next_column) = (row.wrapping_add_signed(down), column.wrapping_add_signed(across));
                if out[row * 64 + column] > 0.5
                    && next_row < 64
                    && next_column < 64
                    && out[next_row * 64 + next_column] > 0.5
                {
                    touching += 1;
                }
            }
        }
    }
    // a random scatter of 512 dots in 4096 has about 4096 x 4 x (1/8)^2 = 256 touching pairs
    assert!(touching <= 8, "{touching} touching pairs");
    Ok(())
}

#[test]
fn the_pattern_amount_sets_how_many_colours_a_dot_is_mixed_of() -> Result<(), Error> {
    // at 0 a grey of 0.3 is the nearest colour, black, everywhere; with 16 candidates about 5 of 16 are white
    let picture = (8, 8, vec![0.3_f32; 8 * 8 * 3]);
    let lit = |pattern| -> Result<usize, Error> {
        Ok(reds(&run(&ink(Method::Bayer, pattern), &picture)?)
            .iter()
            .filter(|&&v| v > 0.5)
            .count())
    };
    assert_eq!(lit(0.0)?, 0);
    // 5 of 16 candidates are white, so a pixel is lit when floor(rank / 4) >= 11: the 20 ranks from 44 up
    assert_eq!(lit(1.0)?, 20);
    Ok(())
}

// ---- finding the colours --------------------------------------------------------------------------------------------------

/// Four flat quarters of the colours `colours` (`size` x `size`, `size` even).
fn quarters(colours: [[f32; 3]; 4], size: usize) -> (usize, usize, Vec<f32>) {
    let values = (0..size * size)
        .flat_map(|at| colours[usize::from(at / size >= size / 2) * 2 + usize::from(at % size >= size / 2)])
        .collect();
    (size, size, values)
}

#[test]
fn as_many_colours_as_the_picture_has_are_found_exactly() -> Result<(), Error> {
    let picture = quarters(
        [[0.9, 0.1, 0.1], [0.1, 0.8, 0.2], [0.2, 0.2, 0.9], [0.95, 0.9, 0.1]],
        16,
    );
    for init in [Init::Spread, Init::Random] {
        for space in [Space::Oklab, Space::Rgb] {
            let params = Params {
                palette: Palette::Photo,
                colors: 4,
                init,
                space,
                pattern: 0.0,
                ..Params::default()
            };
            let out = run(&params, &picture)?;
            crate::common::compare::assert_close(&out, &picture.2, 1e-6, &format!("{init:?} {space:?}"));
        }
    }
    Ok(())
}

#[test]
fn more_colours_than_the_picture_has_leave_it_as_it_is() -> Result<(), Error> {
    let picture = quarters([[0.9, 0.1, 0.1], [0.9, 0.1, 0.1], [0.2, 0.2, 0.9], [0.2, 0.2, 0.9]], 16);
    let params = Params {
        colors: 8,
        pattern: 0.0,
        ..Params::default()
    };
    crate::common::compare::assert_close(&run(&params, &picture)?, &picture.2, 1e-6, "two colours of eight");
    Ok(())
}

#[test]
fn found_colours_are_at_most_as_many_as_asked_and_each_dot_takes_the_nearest_of_them() -> Result<(), Error> {
    let picture = (24, 24, varied(24, 24));
    for init in [Init::Spread, Init::Random] {
        let params = Params {
            colors: 5,
            init,
            space: Space::Rgb,
            pattern: 0.0,
            ..Params::default()
        };
        let out = pixels(&run(&params, &picture)?);
        let mut palette: Vec<[f32; 3]> = Vec::new();
        for &pixel in &out {
            if !palette.iter().any(|&colour| near(pixel, colour)) {
                palette.push(pixel);
            }
        }
        assert!(palette.len() <= 5, "{init:?}: {} colours", palette.len());
        for (got, given) in out.iter().zip(pixels(&picture.2)) {
            let distance = |colour: &[f32; 3]| colour.iter().zip(given).map(|(a, b)| (a - b) * (a - b)).sum::<f32>();
            let best = palette.iter().map(distance).fold(f32::INFINITY, f32::min);
            assert!(distance(got) <= best + 1e-6, "{init:?}: {given:?} became {got:?}");
        }
    }
    Ok(())
}

#[test]
fn the_same_seed_finds_the_same_colours_whatever_part_of_the_dots_is_searched() -> Result<(), Error> {
    let picture = (24, 24, varied(24, 24));
    let params = Params {
        colors: 6,
        sample: 100,
        init: Init::Random,
        seed: 5,
        ..Params::default()
    };
    assert_eq!(run(&params, &picture)?, run(&params, &picture)?);
    Ok(())
}

// ---- dots ---------------------------------------------------------------------------------------------------------------------

#[test]
fn a_dot_is_the_mean_of_the_pixels_under_it_and_the_last_ones_are_smaller() -> Result<(), Error> {
    // 3 x 3 with dots of 2 px: a block of four, two of two, one of one pixel
    let picture = greys(&[0.9, 0.9, 1.0, 0.9, 0.1, 0.6, 1.0, 0.6, 0.4], 3);
    let params = Params {
        cell: 2.0,
        ..ink(Method::FloydSteinberg, 0.0)
    };
    // block means: 0.7 (white), 0.8 (white), 0.8 (white), 0.4 (black)
    assert_eq!(
        reds(&run(&params, &picture)?),
        [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]
    );
    Ok(())
}

#[test]
fn a_dot_is_one_flat_colour_even_with_a_pattern() -> Result<(), Error> {
    let picture = (14, 11, varied(14, 11));
    let params = Params {
        cell: 4.0,
        ..ink(Method::Bayer, 1.0)
    };
    let out = reds(&run(&params, &picture)?);
    for row in 0..14 {
        for column in 0..11 {
            assert_eq!(
                out[row * 11 + column],
                out[(row / 4 * 4) * 11 + column / 4 * 4],
                "({row}, {column})"
            );
        }
    }
    Ok(())
}

#[test]
fn an_ordered_dither_does_not_depend_on_the_order_the_colours_are_listed_in() -> Result<(), Error> {
    // candidates are put in order of the first channel, so the same set of colours in any order gives the same picture
    let (dark, mid, light) = ([0.1, 0.3, 0.2], [0.5, 0.1, 0.6], [0.9, 0.8, 0.7]);
    let picture = (8, 8, vec![0.4_f32; 8 * 8 * 3]);
    let with = |order: [[f32; 3]; 3]| -> Result<Vec<f32>, Error> {
        let params = Params {
            palette: Palette::Custom,
            custom: order.iter().map(|&colour| Color(colour)).collect(),
            space: Space::Rgb,
            ..ink(Method::Bayer, 1.0)
        };
        run(&params, &picture)
    };
    let sorted = with([dark, mid, light])?;
    assert_eq!(sorted, with([light, dark, mid])?);
    assert_eq!(sorted, with([mid, light, dark])?);
    // and it really mixes: at least two of the colours appear
    assert!(
        sorted
            .as_chunks::<3>()
            .0
            .iter()
            .any(|pixel| near([pixel[0], pixel[1], pixel[2]], mid))
    );
    Ok(())
}

#[test]
fn blue_noise_uses_every_level_of_its_tile_once_so_a_grey_of_k_sixteenths_lights_k_sixteenths() -> Result<(), Error> {
    for whites in 1..16_u16 {
        let grey = f32::from(whites) / 16.0;
        let flat = (64, 64, vec![grey; 64 * 64 * 3]);
        let lit = reds(&run(&ink(Method::BlueNoise, 1.0), &flat)?)
            .iter()
            .filter(|&&v| v > 0.5)
            .count();
        assert_eq!(lit, usize::from(whites) * 256, "{whites}/16");
    }
    Ok(())
}

// ---- threshold matrices -------------------------------------------------------------------------------------------------

/// The lit (white) pixels of a flat grey `grey` painted by `method` on a `side` x `side` picture, row after row.
fn lit_on_flat(method: Method, grey: f32, side: usize) -> Result<Vec<bool>, Error> {
    let flat = (side, side, vec![grey; side * side * 3]);
    Ok(reds(&run(&ink(method, 1.0), &flat)?).iter().map(|&v| v > 0.5).collect())
}

#[test]
fn every_ordered_matrix_lights_a_grey_of_k_levels_in_exactly_k_cells_of_its_n() -> Result<(), Error> {
    // each rank once: a grey of k/n lights exactly k n-ths of the cells. n is 16 (the candidates), or 4 for the 2 x 2.
    let cases = [
        (Method::Bayer2, 2, 4_usize),
        (Method::Bayer4, 4, 16),
        (Method::Bayer, 8, 16),
        (Method::Clustered, 8, 16),
        (Method::Clustered45, 8, 16),
        (Method::GameboyCamera, 4, 16),
    ];
    for (method, side, levels) in cases {
        for k in 1..levels {
            #[expect(clippy::cast_precision_loss, reason = "a small count")]
            let grey = k as f32 / levels as f32;
            let lit = lit_on_flat(method, grey, side)?.iter().filter(|&&on| on).count();
            assert_eq!(lit, k * side * side / levels, "{method:?} {k}/{levels}");
        }
    }
    Ok(())
}

#[test]
fn the_small_bayer_matrices_light_the_pixels_of_the_closed_form_in_the_top_share() -> Result<(), Error> {
    for (method, bits, levels) in [(Method::Bayer2, 1_usize, 4_usize), (Method::Bayer4, 2, 16)] {
        let side = 1 << bits;
        let cells = side * side;
        for k in 1..levels {
            #[expect(clippy::cast_precision_loss, reason = "a small count")]
            let out = lit_on_flat(method, k as f32 / levels as f32, side)?;
            for (at, &on) in out.iter().enumerate() {
                // a rank is lit when the share of the cells at or above it is within the grey's: the top k n-ths
                assert_eq!(on, bayer_rank_of(bits, at / side, at % side) >= cells - k * cells / levels, "{method:?} {k}");
            }
        }
    }
    Ok(())
}

#[test]
fn the_game_boy_camera_matrix_lights_its_cells_in_the_order_of_their_register_thresholds() -> Result<(), Error> {
    // the thresholds of the first of the camera's dither patterns (registers A006 on, the lowest of each cell's three, a
    // cell x + 4 y), as dither-pattern-gen's reference/patterns.mjs lists them; a cell with a lower threshold is lit sooner
    let thresholds = [
        128_u16, 143, 131, 146, 138, 133, 141, 136, 130, 145, 129, 144, 140, 135, 139, 134,
    ];
    for k in 1..16_usize {
        #[expect(clippy::cast_precision_loss, reason = "a small count")]
        let out = lit_on_flat(Method::GameboyCamera, k as f32 / 16.0, 4)?;
        let mut sorted = thresholds;
        sorted.sort_unstable();
        for (at, &on) in out.iter().enumerate() {
            assert_eq!(on, thresholds[at] <= sorted[k - 1], "{k}/16 at {at}");
        }
    }
    Ok(())
}

/// The lit set at `k` of 16 levels of `method`'s 8 x 8 tile, as (row, column) cells.
fn lit_cells(method: Method, k: usize) -> Result<Vec<(usize, usize)>, Error> {
    #[expect(clippy::cast_precision_loss, reason = "a small count")]
    let out = lit_on_flat(method, k as f32 / 16.0, 8)?;
    Ok((0..64).filter(|&at| out[at]).map(|at| (at / 8, at % 8)).collect())
}

/// How many pieces `cells` fall into on the wrapped 8 x 8 tile, cells touching along a side being one piece.
fn pieces(cells: &[(usize, usize)]) -> usize {
    let mut seen = std::collections::BTreeSet::new();
    let mut count = 0;
    for &start in cells {
        if seen.contains(&start) {
            continue;
        }
        count += 1;
        let mut stack = vec![start];
        while let Some((row, column)) = stack.pop() {
            if !seen.insert((row, column)) {
                continue;
            }
            for (down, right) in [(1, 0), (7, 0), (0, 1), (0, 7)] {
                let next = ((row + down) % 8, (column + right) % 8);
                if cells.contains(&next) && !seen.contains(&next) {
                    stack.push(next);
                }
            }
        }
    }
    count
}

#[test]
fn clustered_dots_keep_every_lit_pixel_lit_and_stay_in_one_dot_per_cluster() -> Result<(), Error> {
    // the lit pixels at a level stay lit at every higher one, and they gather: one dot in the square screen, two (one
    // each way along the diagonal) in the 45 degree one
    for (method, most_dots) in [(Method::Clustered, 1), (Method::Clustered45, 2)] {
        let mut before: Vec<(usize, usize)> = Vec::new();
        for k in 1..16 {
            let lit = lit_cells(method, k)?;
            assert!(before.iter().all(|cell| lit.contains(cell)), "{method:?} {k}: a lit pixel went out");
            let dots = pieces(&lit);
            assert!((1..=most_dots).contains(&dots), "{method:?} {k}/16: {dots} dots");
            before = lit;
        }
    }
    Ok(())
}

#[test]
fn white_noise_lights_the_share_of_its_tone_changes_with_the_seed_and_keeps_with_it() -> Result<(), Error> {
    let tone = 5.0_f32 / 16.0;
    let flat = (128, 128, vec![tone; 128 * 128 * 3]);
    let with = |seed| Params {
        seed,
        ..ink(Method::WhiteNoise, 1.0)
    };
    let first = run(&with(3), &flat)?;
    let lit = reds(&first).iter().filter(|&&v| v > 0.5).count();
    #[expect(clippy::cast_precision_loss, reason = "a count below 16384")]
    let share = lit as f32 / (128.0 * 128.0);
    assert!((share - tone).abs() < 0.02, "{share} lit of {tone}");
    assert_eq!(run(&with(3), &flat)?, first, "the same seed, the same picture");
    assert_ne!(run(&with(4), &flat)?, first, "another seed, another picture");
    Ok(())
}

/// `method` (with `extra` parameters) on `picture`, black and white in sRGB, given the picture `matrix` as `"m"`.
fn with_matrix(
    method: &str,
    matrix: Option<&Image>,
    extra: &[(&str, Value)],
    picture: &(usize, usize, Vec<f32>),
) -> Result<Vec<f32>, Error> {
    let mut step = Step::new("dither")
        .param("method", method)
        .param("palette", "custom")
        .param("custom", json!(["#000000", "#ffffff"]))
        .param("space", "rgb");
    for (key, value) in extra {
        step = step.param(*key, value.clone());
    }
    let mut photo = Photo::new(Picture::new(picture.0, picture.1, &picture.2)?);
    if let Some(matrix) = matrix {
        photo = photo.with_image("m", Arc::new(matrix.clone()));
    }
    Ok(photo
        .render(&Recipe::new([step]), &Presets::new(), Options::default())?
        .into_values())
}

/// A grey picture of `rows` x `columns` whose cells are `greys`, row after row.
fn grey_matrix(rows: usize, columns: usize, greys: &[f32]) -> Result<Image, Error> {
    Image::new(rows, columns, &greys.iter().flat_map(|&grey| [grey; 3]).collect::<Vec<_>>())
}

#[test]
fn a_matrix_that_is_bayer_four_gives_the_picture_of_bayer4() -> Result<(), Error> {
    #[expect(clippy::cast_precision_loss, reason = "a rank below 16")]
    let greys: Vec<f32> = (0..16)
        .map(|at| (bayer_rank_of(2, at / 4, at % 4) as f32 + 0.5) / 16.0)
        .collect();
    let matrix = grey_matrix(4, 4, &greys)?;
    let picture = (13, 11, varied(13, 11));
    let theirs = with_matrix("matrix", Some(&matrix), &[("matrix", json!("m"))], &picture)?;
    assert_eq!(theirs, with_matrix("bayer4", None, &[], &picture)?);
    Ok(())
}

#[test]
fn a_matrix_lights_a_grey_of_k_levels_in_k_cells_brightest_first_with_ties_in_reading_order() -> Result<(), Error> {
    // 2 x 2: the brightest cell (0, 1) is lit first, then the tied ones in reading order, (1, 0) before (1, 1), the
    // dimmest (0, 0) last
    let matrix = grey_matrix(2, 2, &[0.0, 0.9, 0.5, 0.5])?;
    let lit_at = |k: f32| -> Result<Vec<bool>, Error> {
        let flat = (2, 2, vec![k / 4.0; 12]);
        Ok(with_matrix("matrix", Some(&matrix), &[("matrix", json!("m"))], &flat)?
            .chunks(3)
            .map(|pixel| pixel[0] > 0.5)
            .collect())
    };
    assert_eq!(lit_at(1.0)?, [false, true, false, false]);
    assert_eq!(lit_at(2.0)?, [false, true, true, false]);
    assert_eq!(lit_at(3.0)?, [false, true, true, true]);
    // 4 x 4 (a non-Bayer one: ranks by reading order): k of 16 lit
    let ramp: Vec<f32> = (0..16).map(|at| f32::from(u8::try_from(at).unwrap_or(0)) / 16.0).collect();
    let matrix = grey_matrix(4, 4, &ramp)?;
    for k in 1..16_u8 {
        let flat = (4, 4, vec![f32::from(k) / 16.0; 48]);
        let out = with_matrix("matrix", Some(&matrix), &[("matrix", json!("m"))], &flat)?;
        assert_eq!(out.chunks(3).filter(|pixel| pixel[0] > 0.5).count(), usize::from(k), "{k}/16");
    }
    Ok(())
}

#[test]
fn method_matrix_with_no_matrix_or_an_empty_name_is_refused_naming_matrix() {
    let picture = (4, 4, vec![0.5_f32; 48]);
    for extra in [vec![], vec![("matrix", json!(""))]] {
        let refused = with_matrix("matrix", None, &extra, &picture);
        assert!(
            matches!(&refused, Err(error) if error.to_string().contains("matrix")),
            "{extra:?}: {refused:?}"
        );
    }
}

// ---- inks ---------------------------------------------------------------------------------------------------------------

const PAPER: [f32; 3] = [0.95, 0.93, 0.88];
const PINK: [f32; 3] = [1.0, 0.3, 0.45];
const BLUE: [f32; 3] = [0.17, 0.35, 1.0];
const YELLOW: [f32; 3] = [1.0, 0.9, 0.1];

fn inked(method: Method, inks: &[[f32; 3]]) -> Params {
    Params {
        method,
        separation: Separation::Inks,
        inks: inks.iter().map(|&ink| Color(ink)).collect(),
        paper: Color(PAPER),
        ..Params::default()
    }
}

/// The colour of the inks whose bits are set in `mask` laid over the paper: each full ink multiplies a channel by its ratio
/// to the paper (an ink lighter than the paper in a channel leaves it as it is).
fn laid(inks: &[[f32; 3]], mask: usize) -> [f32; 3] {
    [0, 1, 2].map(|c| {
        (0..inks.len())
            .filter(|ink| mask >> ink & 1 == 1)
            .fold(PAPER[c], |light, ink| light * (inks[ink][c] / PAPER[c]).min(1.0))
    })
}

/// Which inks (as bits) landed on each pixel, read from its colour; none of the combinations may be missed.
fn landed(inks: &[[f32; 3]], picture: &[f32]) -> Vec<usize> {
    pixels(picture)
        .into_iter()
        .map(|pixel| {
            (0..1 << inks.len())
                .find(|&mask| near(pixel, laid(inks, mask)))
                .unwrap_or(usize::MAX)
        })
        .collect()
}

#[test]
fn every_pixel_is_the_paper_under_some_combination_of_the_full_inks() -> Result<(), Error> {
    let picture = (32, 32, varied(32, 32));
    for inks in [vec![PINK], vec![PINK, BLUE], vec![PINK, BLUE, YELLOW]] {
        for method in [
            Method::FloydSteinberg,
            Method::Atkinson,
            Method::Bayer,
            Method::BlueNoise,
        ] {
            let params = Params {
                cell: 2.0,
                ..inked(method, &inks)
            };
            let out = run(&params, &picture)?;
            let layers = landed(&inks, &out);
            assert!(
                layers.iter().all(|&mask| mask != usize::MAX),
                "{} inks, {method:?}",
                inks.len()
            );
            // and the combinations are used: every ink lands somewhere
            for ink in 0..inks.len() {
                assert!(layers.iter().any(|mask| mask >> ink & 1 == 1), "ink {ink} never lands");
            }
        }
    }
    Ok(())
}

#[test]
fn the_share_of_pixels_an_ink_lands_on_is_its_unmixed_coverage() -> Result<(), Error> {
    // 0.3 of pink and 0.6 of blue, printed as the flat colour to redraw
    let flat =
        [0, 1, 2].map(|c| PAPER[c] * (PINK[c] / PAPER[c]).min(1.0).powf(0.3) * (BLUE[c] / PAPER[c]).min(1.0).powf(0.6));
    let picture = (64, 64, flat.repeat(64 * 64));
    // Atkinson drops a quarter of the error, so it is left out: its tones are lighter or darker by design. The ordered
    // methods work in sixteenths, so they are off by up to a thirty-second.
    for method in [Method::FloydSteinberg, Method::Bayer, Method::BlueNoise] {
        let out = run(&inked(method, &[PINK, BLUE]), &picture)?;
        let layers = landed(&[PINK, BLUE], &out);
        for (ink, coverage) in [(0, 0.3), (1, 0.6)] {
            let landing = layers.iter().filter(|mask| *mask >> ink & 1 == 1).count();
            let share = f32::from(u16::try_from(landing).unwrap_or(0)) / 4096.0;
            assert!((share - coverage).abs() < 0.04, "{method:?} ink {ink}: {share}");
        }
    }
    Ok(())
}

#[test]
fn paper_stays_paper_and_a_full_ink_stays_that_ink() -> Result<(), Error> {
    for method in [
        Method::FloydSteinberg,
        Method::Atkinson,
        Method::Bayer,
        Method::BlueNoise,
    ] {
        let on = |colour: [f32; 3]| -> Result<Vec<[f32; 3]>, Error> {
            let picture = (16, 16, colour.repeat(256));
            Ok(pixels(&run(&inked(method, &[PINK, BLUE]), &picture)?))
        };
        assert!(on(PAPER)?.iter().all(|&pixel| pixel == PAPER), "{method:?}");
        assert!(
            on(PINK)?.iter().all(|&pixel| near(pixel, laid(&[PINK, BLUE], 1))),
            "{method:?}"
        );
    }
    Ok(())
}

#[test]
fn the_inks_matter_only_with_a_separation() -> Result<(), Error> {
    let picture = (24, 24, varied(24, 24));
    let plain = run(&Params::default(), &picture)?;
    let with_unused = Params {
        inks: vec![Color(YELLOW)],
        paper: Color(PAPER),
        offset: 9.0,
        offset_angle: 33.0,
        ..Params::default()
    };
    assert_eq!(plain, run(&with_unused, &picture)?);
    Ok(())
}

#[test]
fn ink_k_lands_k_offsets_away_along_the_angle_and_no_offset_leaves_the_angle_unused() -> Result<(), Error> {
    let picture = (30, 30, varied(30, 30));
    let inks = [PINK, BLUE, YELLOW];
    let base = inked(Method::FloydSteinberg, &inks);
    let in_register = landed(&inks, &run(&base, &picture)?);
    let turned = Params {
        offset_angle: 70.0,
        ..base.clone()
    };
    assert_eq!(in_register, landed(&inks, &run(&turned, &picture)?), "offset 0");
    // (right, down) of one offset, as whole pixels
    for (offset, angle, step) in [
        (3.0, 0.0, (3, 0)),
        (2.0, 90.0, (0, 2)),
        (2.0, 180.0, (-2, 0)),
        (4.0, 360.0, (4, 0)),
    ] {
        let moved = Params {
            offset,
            offset_angle: angle,
            ..base.clone()
        };
        let got = landed(&inks, &run(&moved, &picture)?);
        for row in 0..30_isize {
            for column in 0..30_isize {
                for ink in 0..3_isize {
                    let (from_row, from_column) = (row - step.1 * ink, column - step.0 * ink);
                    let expected = if (0..30).contains(&from_row) && (0..30).contains(&from_column) {
                        in_register[(from_row * 30 + from_column).cast_unsigned()] >> ink & 1
                    } else {
                        0
                    };
                    assert_eq!(
                        got[(row * 30 + column).cast_unsigned()] >> ink & 1,
                        expected,
                        "ink {ink} at {row},{column}, offset {offset} at {angle}"
                    );
                }
            }
        }
    }
    Ok(())
}

// ---- the diffusion kernels and the serpentine scan ------------------------------------------------------------------------

/// The picture as rows of '0' (black) and '1' (white), a space between rows.
fn bits(picture: &[f32], width: usize) -> String {
    let marks: Vec<String> = reds(picture)
        .chunks(width)
        .map(|row| row.iter().map(|&red| if red > 0.5 { '1' } else { '0' }).collect())
        .collect();
    marks.join(" ")
}

#[test]
fn each_diffusion_kernel_spreads_the_error_by_its_published_weights() -> Result<(), Error> {
    // A flat grey of 0.39 (6 x 4; no pixel lands within 4e-4 of the 0.5 threshold, so f32 rounding cannot decide one) in black and white. The pictures are worked pixel by pixel with each kernel's published
    // weights (Floyd-Steinberg 7 3 5 1 / 16, Atkinson 1 1 1 1 1 1 / 8, Jarvis-Judice-Ninke / 48, Stucki / 42, Burkes / 32,
    // Sierra / 32, two-row Sierra / 16, Sierra Lite 2 1 1 / 4, Stevenson-Arce / 200), by a separate exact-fraction
    // script; every kernel gives a picture of its own, so a wrong weight shows.
    let picture = greys(&[0.39; 24], 6);
    let rows = [
        (Method::FloydSteinberg, "010010 010101 010000 001011"),
        (Method::Atkinson, "000100 010010 011001 000100"),
        (Method::JarvisJudiceNinke, "000100 101001 010010 001100"),
        (Method::Stucki, "001001 010010 101001 001001"),
        (Method::Burkes, "001001 100100 010101 001000"),
        (Method::Sierra, "000100 101001 010100 001010"),
        (Method::SierraTwoRow, "001001 010100 010010 010010"),
        (Method::SierraLite, "010010 010101 010001 001010"),
        (Method::StevensonArce, "000000 011001 011001 000110"),
    ];
    for (method, expected) in rows {
        let out = run(&ink(method, 1.0), &picture)?;
        assert_eq!(bits(&out, 6), expected, "{method:?}");
    }
    Ok(())
}

#[test]
fn a_serpentine_scan_turns_the_kernel_round_on_odd_rows() -> Result<(), Error> {
    // Sierra Lite (2 right, 1 below left, 1 below, over 4). Row 0 is black and sends 0.1 down to the two pixels under and
    // left of its 0.4. Row 1 is 0.45 each: left to right, 0.45 + 0 is black and passes 0.225 on, so the middle is white
    // (0.775) and the right black (0.4375): "010". Right to left, the right is 0.55, white, passing -0.225 left, so the
    // middle (0.325) is black and passes 0.1625 on, the left (0.6125) white: "101".
    let picture = greys(&[0.0, 0.0, 0.4, 0.45, 0.45, 0.45], 3);
    let plain = ink(Method::SierraLite, 1.0);
    let turned = Params {
        serpentine: true,
        ..plain.clone()
    };
    assert_eq!(bits(&run(&plain, &picture)?, 3), "000 010");
    assert_eq!(bits(&run(&turned, &picture)?, 3), "000 101");
    Ok(())
}

#[test]
fn a_serpentine_row_passes_its_error_down_on_the_mirrored_side() -> Result<(), Error> {
    // Sierra Lite again (2 right, 1 below left, 1 below, over 4), three rows of three; exact fractions, worked by script.
    // Row 1 is scanned right to left and its 0.4 at the right sends a share of its error to the below-right side of
    // each pixel, so row 2 (0.43 0.35 0.35) comes out "010"; with the below tap not turned round it would be "101".
    let picture = greys(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.4, 0.43, 0.35, 0.35], 3);
    let turned = Params {
        serpentine: true,
        ..ink(Method::SierraLite, 1.0)
    };
    assert_eq!(bits(&run(&turned, &picture)?, 3), "000 000 010");
    Ok(())
}

#[test]
fn a_serpentine_row_is_the_plain_scan_of_that_row_turned_round() -> Result<(), Error> {
    // Row 0 is pure, so passes nothing down: row 1 starts clean, and scanned right to left it must equal the plain scan
    // of its mirror image, mirrored back.
    let row = [0.3, 0.55, 0.42, 0.8, 0.15, 0.5, 0.65, 0.35, 0.47, 0.9];
    let width = row.len();
    let mut pure_row = vec![0.0; width];
    pure_row[3] = 1.0;
    for method in [Method::FloydSteinberg, Method::JarvisJudiceNinke, Method::StevensonArce] {
        let plain = ink(method, 1.0);
        let turned = Params {
            serpentine: true,
            ..plain.clone()
        };
        let mirror: Vec<f32> = row.iter().rev().copied().collect();
        let flat = |first: &[f32], second: &[f32]| greys(&[first, second].concat(), width);
        let mut expected = reds(&run(&plain, &flat(&pure_row, &mirror))?)[width..].to_vec();
        expected.reverse();
        let got = reds(&run(&turned, &flat(&pure_row, &row))?)[width..].to_vec();
        assert_eq!(got, expected, "{method:?}");
    }
    Ok(())
}

#[test]
fn serpentine_changes_every_diffusion_method_and_no_ordered_one() -> Result<(), Error> {
    // `parameters_are_read` cannot hold a row for this: the default method is a diffusion one, so `serpentine` changes the
    // picture alone there.
    let picture = (20, 20, varied(20, 20));
    let diffusion = [
        Method::FloydSteinberg,
        Method::Atkinson,
        Method::JarvisJudiceNinke,
        Method::Stucki,
        Method::Burkes,
        Method::Sierra,
        Method::SierraTwoRow,
        Method::SierraLite,
        Method::StevensonArce,
    ];
    for method in diffusion.into_iter().chain([Method::Bayer, Method::BlueNoise]) {
        let plain = ink(method, 1.0);
        let turned = Params {
            serpentine: true,
            ..plain.clone()
        };
        let same = run(&plain, &picture)? == run(&turned, &picture)?;
        assert_eq!(same, !diffusion.contains(&method), "{method:?}");
    }
    Ok(())
}
