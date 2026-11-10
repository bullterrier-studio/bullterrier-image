//! `ascii`: the picture redrawn as a grid of characters. The oracles are the tone-to-glyph rule (a flat tone draws one
//! glyph over every cell, and a glyph further along the palette has more ink), hand-worked colours, the grid's size
//! (a cell 10 px wide is 24 px high, worked from the font's tables in `ops/glyphs.rs`) and comparisons between renders that
//! must be alike or must differ.

#![expect(clippy::cast_precision_loss, reason = "small picture sizes and indices")]

use bullterrier_image::effects::ascii::Params;
use bullterrier_image::params::Color;
use bullterrier_image::{Error, Options, Photo, Picture, Presets, Recipe, Step};

use crate::common::compare::first_difference_within;
use crate::common::pixels::run_picture;

const CHARS: &str = " .:-=+*#%@";
const WHITE: Color = Color([1.0; 3]);

/// White characters on black paper, the palette ten glyphs from blank to dense, in cells `cell` px wide.
fn white_on_black(cell: i32) -> Params {
    Params {
        cell,
        chars: CHARS.to_owned(),
        ink: Some(WHITE),
        ..Params::default()
    }
}

/// The picture of one grey `level`, `height` x `width`.
fn flat(height: usize, width: usize, level: f32) -> Vec<f32> {
    vec![level; height * width * 3]
}

/// The red channel of `values` (three channels a pixel).
fn red(values: &[f32]) -> Vec<f32> {
    values.as_chunks::<3>().0.iter().map(|pixel| pixel[0]).collect()
}

/// The ink, white on black, that `params` put down on the `height` x `width` grey `level`.
fn ink(params: &Params, height: usize, width: usize, level: f32) -> Result<f32, Error> {
    let out = run_picture(params, height, width, &flat(height, width, level))?;
    Ok(red(&out).iter().sum())
}

#[test]
fn a_flat_black_picture_is_the_lightest_glyph_and_a_flat_white_one_the_densest() -> Result<(), Error> {
    let params = white_on_black(8);
    // the lightest glyph of the palette is the space, which has no ink; the densest has the most of all the tones
    assert!(ink(&params, 36, 24, 0.0)?.abs() < f32::EPSILON);
    let white = ink(&params, 36, 24, 1.0)?;
    for tone in 0..9 {
        let level = tone as f32 / 9.0;
        assert!(ink(&params, 36, 24, level)? < white, "{level}");
    }
    // and with a palette of two, black draws the first and white the second, whatever order they are written in
    for chars in [".@", "@."] {
        let params = Params {
            chars: chars.to_owned(),
            ..white_on_black(8)
        };
        let (dark, bright) = (ink(&params, 18, 16, 0.0)?, ink(&params, 18, 16, 1.0)?);
        assert!(dark > 0.0 && dark < bright, "{chars:?}: {dark} {bright}");
    }
    Ok(())
}

#[test]
fn ink_grows_with_the_tone_one_step_of_the_palette_at_a_time() -> Result<(), Error> {
    // ten tones, one for each glyph (glyph k for the tone k / 9): each has more ink than the one before
    let params = white_on_black(10);
    let inks: Vec<f32> = (0..10)
        .map(|glyph| ink(&params, 24, 20, glyph as f32 / 9.0))
        .collect::<Result<_, _>>()?;
    assert!(inks.windows(2).all(|pair| pair[0] < pair[1]), "{inks:?}");
    Ok(())
}

#[test]
fn along_a_ramp_from_dark_to_bright_the_ink_of_each_cell_never_falls() -> Result<(), Error> {
    let (cell, width) = (8_usize, 80_usize);
    let height = 18; // one row of cells: a cell 8 px wide is 18 px high (13 px font: ascent 14, descent 4)
    let values: Vec<f32> = (0..height * width)
        .flat_map(|at| [(at % width) as f32 / (width - 1) as f32; 3])
        .collect();
    let out = run_picture(&white_on_black(8), height, width, &values)?;
    let out = red(&out);
    let per_cell: Vec<f32> = (0..width / cell)
        .map(|column| {
            (0..height)
                .map(|row| out[row * width + column * cell..][..cell].iter().sum::<f32>())
                .sum()
        })
        .collect();
    assert!(per_cell.windows(2).all(|pair| pair[0] <= pair[1]), "{per_cell:?}");
    assert!(per_cell[0] < per_cell[width / cell - 1], "{per_cell:?}");
    Ok(())
}

#[test]
fn paper_shows_exactly_where_no_ink_is_needed() -> Result<(), Error> {
    let paper = [0x33 as f32 / 255.0, 0x66 as f32 / 255.0, 0x99 as f32 / 255.0];
    let params = Params {
        paper: Color(paper),
        ..white_on_black(8)
    };
    let out = run_picture(&params, 20, 23, &flat(20, 23, 0.0))?;
    assert!(
        out.as_chunks::<3>()
            .0
            .iter()
            .all(|pixel| (0..3).all(|channel| (pixel[channel] - paper[channel]).abs() < 1e-6))
    );
    Ok(())
}

#[test]
fn the_paper_threshold_decides_whether_mid_grey_paper_is_dark_or_light() -> Result<(), Error> {
    // #808080 has the Rec. 709 luminance 0.502, just above the default threshold of 0.5: light paper, so white needs no ink
    let grey = 128.0 / 255.0;
    let params = Params {
        ink: Some(Color([1.0, 0.0, 0.0])),
        paper: Color([grey; 3]),
        ..white_on_black(8)
    };
    let light = run_picture(&params, 27, 16, &flat(27, 16, 1.0))?;
    assert!(light.iter().all(|value| (value - grey).abs() < 1e-6));
    // raised to 0.6, the same paper is dark and white gets the densest glyph: red appears
    let dark = run_picture(
        &Params {
            paper_threshold: 0.6,
            ..params
        },
        27,
        16,
        &flat(27, 16, 1.0),
    )?;
    assert!(red(&dark).iter().any(|value| *value > 0.9));
    Ok(())
}

#[test]
fn on_light_paper_the_ramp_flips_so_dark_areas_get_the_dense_glyph() -> Result<(), Error> {
    let params = Params {
        ink: Some(Color([0.0; 3])),
        paper: WHITE,
        ..white_on_black(8)
    };
    let darkness = |level: f32| -> Result<f32, Error> {
        let out = run_picture(&params, 27, 16, &flat(27, 16, level))?;
        Ok(red(&out).iter().map(|value| 1.0 - value).sum())
    };
    assert!(darkness(1.0)?.abs() < 1e-3);
    assert!(darkness(0.0)? > darkness(0.5)?);
    assert!(darkness(0.5)? > darkness(0.8)?);
    Ok(())
}

#[test]
fn an_ink_colours_every_character_and_nothing_else() -> Result<(), Error> {
    let params = Params {
        ink: Some(Color([1.0, 0.0, 0.5])),
        ..white_on_black(8)
    };
    let out = run_picture(&params, 27, 16, &flat(27, 16, 1.0))?;
    let pixels = out.as_chunks::<3>().0;
    // black paper, so a pixel is the ink times its coverage: blue is half of red and green is none, everywhere
    assert!(pixels.iter().all(|p| p[1] == 0.0 && (p[2] - p[0] / 2.0).abs() < 1e-6));
    assert!(pixels.iter().any(|p| p[0] > 0.9));
    Ok(())
}

#[test]
fn with_no_ink_a_character_takes_the_photo_colour_brightened_to_a_full_channel() -> Result<(), Error> {
    // the colour (0.9, 0.45, 0.3) is scaled until its strongest channel is full: (1, 0.5, 1/3), at every pixel of coverage
    let params = Params {
        chars: CHARS.to_owned(),
        ..Params::default()
    };
    let values: Vec<f32> = [0.9, 0.45, 0.3].repeat(27 * 16);
    let out = run_picture(&params, 27, 16, &values)?;
    let pixels = out.as_chunks::<3>().0;
    let strongest = pixels.iter().map(|p| p[0]).fold(0.0, f32::max);
    assert!(strongest > 0.9, "{strongest}");
    for p in pixels.iter().filter(|p| p[0] > 0.0) {
        assert!(
            (p[1] / p[0] - 0.5).abs() < 1e-4 && (p[2] / p[0] - 1.0 / 3.0).abs() < 1e-4,
            "{p:?}"
        );
    }
    Ok(())
}

#[test]
fn the_ink_floor_caps_how_much_a_dark_colour_is_brightened() -> Result<(), Error> {
    // the colour (0.3, 0.15, 0.06) has the strongest channel 0.3: above a floor of 0.2 it is scaled to full (red 1), below
    // a floor of 0.6 only by 1 / 0.6 (red 0.5), and with a floor of 1 it is as it is (red 0.3)
    let values: Vec<f32> = [0.3, 0.15, 0.06].repeat(27 * 16);
    for (ink_floor, red_ink) in [(0.2, 1.0), (0.6, 0.5), (1.0, 0.3)] {
        let params = Params {
            chars: CHARS.to_owned(),
            ink_floor,
            ..Params::default()
        };
        let out = run_picture(&params, 27, 16, &values)?;
        let strongest = out.as_chunks::<3>().0.iter().map(|p| p[0]).fold(0.0, f32::max);
        // a pixel inside a stroke is fully covered, so it has the ink's own red
        assert!((strongest - red_ink).abs() < 0.02, "{ink_floor}: {strongest}");
    }
    Ok(())
}

#[test]
fn photo_is_how_much_of_the_picture_shows_through_the_paper() -> Result<(), Error> {
    // a black picture on dark paper draws only blank glyphs, so every pixel is the paper's share and the picture's share
    let paper = [0.4, 0.2, 0.0];
    let at = |photo: f32, level: f32| -> Result<Vec<f32>, Error> {
        let params = Params {
            paper: Color(paper),
            photo,
            ..white_on_black(8)
        };
        run_picture(&params, 27, 16, &flat(27, 16, level))
    };
    let shown = at(0.5, 0.0)?;
    for (value, expected) in shown.as_chunks::<3>().0.iter().flat_map(|p| p.iter().zip(paper)) {
        assert!((value - expected * 0.5).abs() < 1e-6);
    }
    let nothing = at(0.0, 0.0)?;
    for (value, expected) in nothing.as_chunks::<3>().0.iter().flat_map(|p| p.iter().zip(paper)) {
        assert!((value - expected).abs() < 1e-6);
    }
    // all of it: the picture itself, here black, where no character is
    assert!(at(1.0, 0.0)?.iter().all(|value| value.abs() < 1e-6));
    Ok(())
}

#[test]
fn gamma_bends_the_ink_amount_as_an_exponent_on_the_tone() -> Result<(), Error> {
    // the amount is the tone to the power gamma, so a tone of 0.6 with a gamma of 2 draws what a tone of 0.36 draws with 1
    let params = |gamma: f32| Params {
        gamma,
        ..white_on_black(8)
    };
    let bent = run_picture(&params(2.0), 27, 16, &flat(27, 16, 0.6))?;
    let plain = run_picture(&params(1.0), 27, 16, &flat(27, 16, 0.36))?;
    assert_eq!(first_difference_within(&bent, &plain, 1e-6), None);
    let straight = run_picture(&params(1.0), 27, 16, &flat(27, 16, 0.6))?;
    assert!(first_difference_within(&bent, &straight, 1e-3).is_some());
    Ok(())
}

#[test]
fn the_grid_is_a_cell_wide_and_as_high_as_the_font_makes_it() -> Result<(), Error> {
    // a cell 10 px wide is 24 px high (17 px font: ascent 18, descent 6); a dense glyph repeats with exactly that period
    let (height, width) = (72, 40);
    let out = red(&run_picture(
        &white_on_black(10),
        height,
        width,
        &flat(height, width, 1.0),
    )?);
    let same_rows = |apart: usize| {
        (0..height - apart).all(|row| out[row * width..][..width] == out[(row + apart) * width..][..width])
    };
    let same_columns = |apart: usize| {
        (0..height).all(|row| out[row * width..][..width - apart] == out[row * width + apart..][..width - apart])
    };
    assert!(same_rows(24) && same_columns(10));
    assert!((1..24).all(|apart| !same_rows(apart)), "a shorter period in the rows");
    assert!(
        (1..10).all(|apart| !same_columns(apart)),
        "a shorter period in the columns"
    );
    Ok(())
}

#[test]
fn a_picture_that_is_no_whole_number_of_cells_is_cut_from_a_whole_one() -> Result<(), Error> {
    // cells of 10 x 24: a white picture one and a half cells each way has cut-off cells that are as white as whole ones,
    // so it is the top left of the render of the picture of 2 x 2 whole cells
    let (small, big) = ((36, 15), (48, 20));
    let cut = red(&run_picture(
        &white_on_black(10),
        small.0,
        small.1,
        &flat(small.0, small.1, 1.0),
    )?);
    let whole = red(&run_picture(
        &white_on_black(10),
        big.0,
        big.1,
        &flat(big.0, big.1, 1.0),
    )?);
    for row in 0..small.0 {
        assert_eq!(
            cut[row * small.1..][..small.1],
            whole[row * big.1..][..small.1],
            "row {row}"
        );
    }
    // pictures smaller than one cell still work
    for (height, width) in [(1, 1), (5, 3), (23, 9)] {
        let out = run_picture(&white_on_black(10), height, width, &flat(height, width, 0.5))?;
        assert_eq!(out.len(), height * width * 3);
    }
    Ok(())
}

#[test]
fn a_cell_cut_off_by_the_edge_is_measured_with_the_border_pixels_going_on() -> Result<(), Error> {
    // 15 x 36 px in cells of 10 x 24: the left cell is black, and the right one has 5 white columns of its own and 5 past
    // the picture, which are the last column again: white. So the right cell is as white as a whole white one, and the
    // bottom cell (12 rows of its 24) is as black as the top one
    let (height, width) = (36, 15);
    let values: Vec<f32> = (0..height * width)
        .flat_map(|at| [if at % width < 10 { 0.0 } else { 1.0 }; 3])
        .collect();
    let out = red(&run_picture(&white_on_black(10), height, width, &values)?);
    let white = red(&run_picture(
        &white_on_black(10),
        height,
        width,
        &flat(height, width, 1.0),
    )?);
    for row in 0..height {
        assert_eq!(
            out[row * width + 10..][..5],
            white[row * width + 10..][..5],
            "row {row}"
        );
        assert!(out[row * width..][..10].iter().all(|value| *value == 0.0), "row {row}");
    }
    Ok(())
}

#[test]
fn a_tone_between_two_glyphs_takes_the_nearer() -> Result<(), Error> {
    // with ten glyphs the tone 0.58 is 5.22 steps along the palette (glyph 5) and 0.63 is 5.67 (glyph 6)
    let params = white_on_black(8);
    let at = |level: f32| run_picture(&params, 27, 16, &flat(27, 16, level));
    assert_eq!(at(0.58)?, at(5.0 / 9.0)?);
    assert_eq!(at(0.63)?, at(6.0 / 9.0)?);
    assert_ne!(at(5.0 / 9.0)?, at(6.0 / 9.0)?);
    Ok(())
}

#[test]
fn the_palette_is_the_distinct_characters_in_any_order_and_a_space_is_one() -> Result<(), Error> {
    let at = |chars: &str, level: f32| {
        let params = Params {
            chars: chars.to_owned(),
            ..white_on_black(8)
        };
        run_picture(&params, 27, 16, &flat(27, 16, level))
    };
    for level in [0.0, 1.0] {
        assert_eq!(at(". ", level)?, at(" .", level)?);
        assert_eq!(at(" ..  .", level)?, at(" .", level)?);
    }
    // a space is a character, not padding: a dark picture is all spaces, so no ink
    assert!(at(" @", 0.0)?.iter().all(|value| *value == 0.0));
    Ok(())
}

#[test]
fn the_same_picture_and_settings_draw_the_same_picture() -> Result<(), Error> {
    let params = Params {
        photo: 0.3,
        ..Params::default()
    };
    let values: Vec<f32> = (0..40 * 37 * 3).map(|at| ((at * 37) % 101) as f32 / 100.0).collect();
    assert_eq!(
        run_picture(&params, 40, 37, &values)?,
        run_picture(&params, 40, 37, &values)?
    );
    Ok(())
}

#[test]
fn a_palette_with_one_distinct_character_is_refused_when_the_step_is_made() -> Result<(), Error> {
    let pixels = flat(8, 8, 0.5);
    let photo = Photo::new(Picture::new(8, 8, &pixels)?);
    let refused = |chars: &str| {
        let recipe = Recipe::new([Step::new("ascii").param("chars", chars)]);
        photo.render(&recipe, &Presets::new(), Options::default()).map(drop)
    };
    assert!(
        matches!(refused("aaaa"), Err(Error::Invalid { ref field, .. }) if field == "chars"),
        "{:?}",
        refused("aaaa")
    );
    assert!(refused("ab").is_ok());
    Ok(())
}
