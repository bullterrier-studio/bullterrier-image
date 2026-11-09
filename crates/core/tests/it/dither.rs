//! `dither`: the picture redrawn in a few colours. The oracles are values worked by hand (error passed along a short
//! column, the Bayer matrix from its closed form, the sRGB grey whose Oklab lightness is 0.5), the published colours of the
//! fixed palettes typed in the test, and properties: only palette colours appear, each pixel takes the nearest one, a
//! flat area keeps its mean, an ordered pattern repeats.

use bullterrier_image::Error;
use bullterrier_image::effects::dither::{Init, Method, Palette, Params, Space};
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
    let mixed = row ^ column;
    (0..3)
        .map(|bit| (((mixed >> bit) & 1) << (2 * (2 - bit) + 1)) | (((row >> bit) & 1) << (2 * (2 - bit))))
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
