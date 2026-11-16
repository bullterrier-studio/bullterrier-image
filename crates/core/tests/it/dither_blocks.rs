//! `dither` with `block` and `block_height`: every block of dots draws from two palette colours only, as the colour
//! attributes of a Spectrum or a C64 do. The oracles are hand-worked pictures (a block on the line between two colours uses
//! those two), the plain dither when every block would pick the same pair (error then must carry across the borders), and
//! counts of the colours in a block.

use std::collections::HashSet;

use bullterrier_image::{Error, Options, Photo, Picture, Presets, Recipe, Step};
use serde_json::{Value, json};

use crate::common::dither::{BLACK, WHITE, near, pixels, rgb};
use crate::common::pictures::{ramp, varied};

const RED: [f32; 3] = [1.0, 0.0, 0.0];
const GREEN: [f32; 3] = [0.0, 1.0, 0.0];
const BLUE: [f32; 3] = [0.0, 0.0, 1.0];

type Picture3 = (usize, usize, Vec<f32>);

/// `picture` dithered with `method` between `colours` (matched in sRGB) with the `extra` parameters.
fn drawn(
    method: &str,
    colours: &[[f32; 3]],
    extra: &[(&str, Value)],
    (height, width, picture): &Picture3,
) -> Result<Vec<[f32; 3]>, Error> {
    let mut step = Step::new("dither")
        .param("method", method)
        .param("palette", "custom")
        .param("custom", json!(colours))
        .param("space", "rgb");
    for (key, value) in extra {
        step = step.param(*key, value.clone());
    }
    let out = Photo::new(Picture::new(*height, *width, picture)?)
        .render(&Recipe::new([step]), &Presets::new(), Options::default())?
        .into_values();
    Ok(pixels(&out))
}

fn flat(colour: [f32; 3], height: usize, width: usize) -> Picture3 {
    (height, width, colour.repeat(height * width))
}

/// A picture whose pixels before `split` (a row, or a column when `by_column`) are red and blue mixed, and the others green
/// and blue mixed: each lies on the line between two palette colours, a different pair for each side.
fn two_sides((height, width): (usize, usize), split: usize, by_column: bool) -> Picture3 {
    let pixels = (0..height * width)
        .flat_map(|at| {
            let first = if by_column { at % width } else { at / width } < split;
            if first { [0.5, 0.0, 0.5] } else { [0.0, 0.5, 0.5] }
        })
        .collect();
    (height, width, pixels)
}

/// The distinct colours (as bit patterns) of the block of `rows` x `columns` pixels whose top left is `(top, left)`.
fn colours_in(
    out: &[[f32; 3]],
    width: usize,
    (top, left): (usize, usize),
    (rows, columns): (usize, usize),
) -> HashSet<[u32; 3]> {
    let height = out.len() / width;
    (top..(top + rows).min(height))
        .flat_map(|row| (left..(left + columns).min(width)).map(move |column| (row, column)))
        .map(|(row, column)| out[row * width + column].map(f32::to_bits))
        .collect()
}

fn set(colours: &[[f32; 3]]) -> HashSet<[u32; 3]> {
    colours.iter().map(|colour| colour.map(f32::to_bits)).collect()
}

const METHODS: [&str; 7] = [
    "floyd_steinberg",
    "ostromoukhov",
    "riemersma",
    "dot_diffusion",
    "bayer",
    "clustered45",
    "white_noise",
];

#[test]
fn every_block_has_two_colours_at_most_all_of_them_from_the_palette() -> Result<(), Error> {
    let palette = [BLACK, RED, GREEN, BLUE, WHITE];
    let picture = (21, 30, varied(21, 30)); // blocks of 8 x 4 leave cut ones at the right and bottom
    for method in METHODS {
        let out = drawn(
            method,
            &palette,
            &[("block", json!(8)), ("block_height", json!(4))],
            &picture,
        )?;
        for top in (0..21).step_by(4) {
            for left in (0..30).step_by(8) {
                let seen = colours_in(&out, 30, (top, left), (4, 8));
                assert!(
                    seen.len() <= 2,
                    "{method}: block ({top}, {left}) has {} colours",
                    seen.len()
                );
                assert!(
                    seen.is_subset(&set(&palette)),
                    "{method}: block ({top}, {left}) has a colour off the palette"
                );
            }
        }
        // the unblocked picture is not that poor, or the check above proves nothing
        let free = drawn(method, &palette, &[], &picture)?;
        assert!(
            colours_in(&free, 30, (0, 0), (4, 8)).len() > 2,
            "{method}: a free block has 3 or more colours"
        );
    }
    Ok(())
}

#[test]
fn blocks_that_all_pick_one_pair_are_the_plain_dither_so_error_carries_across_their_borders() -> Result<(), Error> {
    let picture = (19, 27, ramp(19, 27));
    for method in METHODS {
        let plain = drawn(method, &[BLACK, WHITE], &[], &picture)?;
        for (block, height) in [(256, 256), (4, 3), (1, 1)] {
            let blocked = drawn(
                method,
                &[BLACK, WHITE],
                &[("block", json!(block)), ("block_height", json!(height))],
                &picture,
            )?;
            assert!(
                blocked == plain,
                "{method}: blocks of {block} x {height} differ from the plain dither"
            );
        }
    }
    Ok(())
}

#[test]
fn a_block_on_the_line_between_two_colours_is_drawn_with_those_two_in_their_share() -> Result<(), Error> {
    // [0.5, 0, 0.5] lies on the segment red-blue (its distance is 0) and farther from every other pair of the palette
    let picture = flat([0.5, 0.0, 0.5], 8, 8);
    for method in ["bayer", "floyd_steinberg"] {
        let out = drawn(
            method,
            &[BLACK, RED, GREEN, BLUE, WHITE],
            &[("block", json!(8)), ("block_height", json!(8))],
            &picture,
        )?;
        let reds = out.iter().filter(|&&pixel| near(pixel, RED)).count();
        let blues = out.iter().filter(|&&pixel| near(pixel, BLUE)).count();
        assert_eq!(reds + blues, 64, "{method}: only red and blue");
        // 64 dots at half red: Bayer's sixteenths and Floyd's error carried across 64 dots both land within one dot of 32
        assert!(reds.abs_diff(32) <= 1, "{method}: {reds} red of 64");
    }
    Ok(())
}

#[test]
fn the_block_height_is_8_dots_unless_given() -> Result<(), Error> {
    let picture = (16, 16, varied(16, 16));
    let palette = [BLACK, RED, GREEN, BLUE, WHITE];
    let by_default = drawn("bayer", &palette, &[("block", json!(16))], &picture)?;
    let given = drawn(
        "bayer",
        &palette,
        &[("block", json!(16)), ("block_height", json!(8))],
        &picture,
    )?;
    assert!(by_default.iter().eq(&given));
    for top in [0, 8] {
        assert!(
            colours_in(&by_default, 16, (top, 0), (8, 16)).len() <= 2,
            "block at row {top}"
        );
    }
    // two blocks on two different lines: red and blue above, green and blue below
    let sides = drawn(
        "bayer",
        &palette,
        &[("block", json!(16))],
        &two_sides((16, 16), 8, false),
    )?;
    assert_eq!(
        colours_in(&sides, 16, (0, 0), (16, 16)).len(),
        3,
        "two blocks, one pair each"
    );
    assert!(colours_in(&sides, 16, (0, 0), (8, 16)).is_subset(&set(&[RED, BLUE])));
    Ok(())
}

#[test]
fn a_block_counts_dots_so_with_a_cell_it_covers_that_many_times_the_pixels() -> Result<(), Error> {
    // 480 px wide, the design size, so a cell of 2 is 2 px
    let palette = [BLACK, RED, GREEN, BLUE, WHITE];
    let picture = (16, 480, varied(16, 480));
    let extra = [("cell", json!(2)), ("block", json!(4)), ("block_height", json!(4))];
    let out = drawn("floyd_steinberg", &palette, &extra, &picture)?;
    for top in [0, 8] {
        for left in (0..480).step_by(8) {
            assert!(
                colours_in(&out, 480, (top, left), (8, 8)).len() <= 2,
                "block ({top}, {left})"
            );
        }
    }
    // columns 0..4 and 4..8 lie on different lines: one 8 px block takes one pair, two 4 px blocks would take three colours
    let sides = drawn("bayer", &palette, &extra, &two_sides((16, 480), 4, true))?;
    assert!(
        colours_in(&sides, 480, (0, 0), (8, 8)).len() <= 2,
        "a block is 8 pixels wide, not 4"
    );
    Ok(())
}

#[test]
fn a_group_keeps_the_pair_inside_one_run_of_the_palette() -> Result<(), Error> {
    // [0.5, 0, 0.5] is exactly between red and blue; with groups of 2 (red green | blue white) that pair is not allowed
    let picture = flat([0.5, 0.0, 0.5], 8, 8);
    let palette = [RED, GREEN, BLUE, WHITE];
    let out = drawn(
        "bayer",
        &palette,
        &[("block", json!(8)), ("block_group", json!(2))],
        &picture,
    )?;
    let seen = colours_in(&out, 8, (0, 0), (8, 8));
    assert!(
        seen.is_subset(&set(&[RED, GREEN])) || seen.is_subset(&set(&[BLUE, WHITE])),
        "{seen:?}"
    );
    // and without the group the pair is red-blue
    let free = drawn("bayer", &palette, &[("block", json!(8))], &picture)?;
    assert!(free.iter().all(|&pixel| near(pixel, RED) || near(pixel, BLUE)));
    Ok(())
}

#[test]
fn a_palette_longer_than_the_search_keeps_still_gives_every_block_a_pair() -> Result<(), Error> {
    let picture = (16, 16, varied(16, 16));
    let colours: Vec<[f32; 3]> = (0..64_u32).map(|at| rgb(at * 0x04_04_04 + at)).collect();
    let out = drawn("bayer", &colours, &[("block", json!(8))], &picture)?;
    for top in [0, 8] {
        for left in [0, 8] {
            assert!(colours_in(&out, 16, (top, left), (8, 8)).len() <= 2);
        }
    }
    // dots that alternate between the greys 5 and 60 of 64: their pair is found among all 64, so the picture is kept
    let grey = |at: u32| [f32::from(u16::try_from(at).unwrap_or(0)) / 63.0; 3];
    let greys: Vec<[f32; 3]> = (0..64).map(grey).collect();
    let alternate: Vec<f32> = (0..16 * 16)
        .flat_map(|at| grey(if at % 2 == 0 { 5 } else { 60 }))
        .collect();
    let kept = drawn("bayer", &greys, &[("block", json!(8))], &(16, 16, alternate.clone()))?;
    assert!(kept == pixels(&alternate), "the pair of the two greys is chosen");
    Ok(())
}

#[test]
fn spot_inks_tiles_and_a_group_of_one_are_refused_naming_the_cause() {
    let picture = flat([0.5; 3], 8, 8);
    for (method, extra, says) in [
        (
            "bayer",
            vec![("block", json!(4)), ("separation", json!("spot"))],
            "block",
        ),
        ("tiles", vec![("block", json!(4))], "block"),
        (
            "bayer",
            vec![("block", json!(4)), ("block_group", json!(1))],
            "block_group",
        ),
    ] {
        let refused = drawn(method, &[BLACK, WHITE], &extra, &picture);
        assert!(
            matches!(&refused, Err(error) if error.to_string().contains(says)),
            "{method} {says}: {refused:?}"
        );
    }
}

/// `preset` of dither on `picture`.
fn by_preset(preset: &str, (height, width, picture): &Picture3) -> Result<Vec<[f32; 3]>, Error> {
    let mut step = Step::new("dither");
    step.preset = Some(preset.to_owned());
    let out = Photo::new(Picture::new(*height, *width, picture)?)
        .render(&Recipe::new([step]), &Presets::shipped()?, Options::default())?
        .into_values();
    Ok(pixels(&out))
}

#[test]
fn the_spectrum_preset_gives_each_8_by_8_block_two_colours_of_one_brightness() -> Result<(), Error> {
    // 256 px wide: a design size of 480 makes the preset's cell 1.875 px a dot of 1 px
    let out = by_preset("zx_spectrum", &(192, 256, varied(192, 256)))?;
    for top in (0..192).step_by(8) {
        for left in (0..256).step_by(8) {
            let seen = colours_in(&out, 256, (top, left), (8, 8));
            assert!(seen.len() <= 2, "block ({top}, {left}): {} colours", seen.len());
            // a lit channel is 0xd7 (normal) or 0xff (bright) of 255
            let bright: HashSet<bool> = seen
                .iter()
                .flat_map(|bits| bits.map(f32::from_bits))
                .filter(|&channel| channel > 0.0)
                .map(|channel| channel > 0.9)
                .collect();
            assert!(
                bright.len() <= 1,
                "block ({top}, {left}) mixes normal and bright colours"
            );
        }
    }
    Ok(())
}

#[test]
fn the_msx_and_c64_presets_give_each_block_two_colours() -> Result<(), Error> {
    for (preset, width, height, block) in [("msx", 256, 192, (1, 8)), ("c64_hires", 320, 200, (8, 8))] {
        let out = by_preset(preset, &(height, width, varied(height, width)))?;
        for top in (0..height).step_by(block.0) {
            for left in (0..width).step_by(block.1) {
                let seen = colours_in(&out, width, (top, left), block);
                assert!(seen.len() <= 2, "{preset} ({top}, {left}): {} colours", seen.len());
            }
        }
    }
    Ok(())
}
