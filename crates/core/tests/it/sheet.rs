//! The contact sheet through the public API: results side by side with labels; a result with transparency over a
//! checkerboard. Expected values are worked out by hand from the layout (a 14 px gap, 30 px under each tile for its label).

use bullterrier_image::{Image, Tile, contact_sheet};

use crate::common::Outcome;

const GAP: usize = 14;
const LABEL: usize = 30;

fn flat(side: usize, rgb: [f32; 3]) -> Result<Image, bullterrier_image::Error> {
    Image::new(side, side, &rgb.repeat(side * side))
}

fn clear_red(side: usize) -> Result<Image, bullterrier_image::Error> {
    Image::rgba(side, side, &[1.0, 0.0, 0.0, 0.0].repeat(side * side))
}

/// The pixel `(row, column)` of a sheet as `[r, g, b]`.
fn at(sheet: &Image, row: usize, column: usize) -> Vec<f32> {
    let channels = sheet.channels();
    let values = sheet.clone().into_values();
    values[(row * sheet.width() + column) * channels..][..channels].to_vec()
}

#[test]
fn a_sheet_is_a_gap_then_each_tile_with_its_label_row_and_a_gap() -> Outcome {
    let (one, two) = (flat(40, [0.5; 3])?, flat(40, [0.5; 3])?);
    let tiles = [Tile::new("a", &one), Tile::new("b", &two), Tile::new("c", &one)];
    let sheet = contact_sheet(&tiles, 2, 64)?;
    // 3 tiles in 2 columns: 2 rows; the width is a gap and 2 x (64 + a gap), the height a gap and 2 x (64 + label + gap)
    assert_eq!(
        (sheet.width(), sheet.height()),
        (GAP + 2 * (64 + GAP), GAP + 2 * (64 + LABEL + GAP))
    );
    assert_eq!(sheet.channels(), 3);
    // the gutter is the sheet's background, #242424
    for value in at(&sheet, 0, 0) {
        assert!((value - 36.0 / 255.0).abs() < 1e-6, "{value}");
    }
    Ok(())
}

#[test]
fn a_tile_is_scaled_to_fit_and_centred_in_its_square() -> Outcome {
    // 20 x 40 (rows x columns) at 64: 32 x 64, centred vertically, so 16 px of background above and below it
    let wide = Image::new(20, 40, &[1.0; 20 * 40 * 3])?;
    let sheet = contact_sheet(&[Tile::new("", &wide)], 3, 64)?;
    assert!(
        (at(&sheet, GAP + 8, GAP + 30)[0] - 36.0 / 255.0).abs() < 1e-6,
        "above the picture"
    );
    assert!((at(&sheet, GAP + 32, GAP + 30)[0] - 1.0).abs() < 1e-6, "in the picture");
    assert!(
        (at(&sheet, GAP + 64 - 8, GAP + 30)[0] - 36.0 / 255.0).abs() < 1e-6,
        "below the picture"
    );
    Ok(())
}

#[test]
fn a_picture_with_transparency_is_shown_over_a_checkerboard_so_the_colour_under_it_never_shows() -> Outcome {
    let clear = clear_red(8)?;
    let sheet = contact_sheet(&[Tile::new("clear", &clear)], 3, 64)?;
    let mut seen: Vec<[i32; 3]> = Vec::new();
    for row in GAP..GAP + 64 {
        for column in GAP..GAP + 64 {
            let pixel = at(&sheet, row, column);
            assert!(pixel[0] - pixel[1] < 0.1, "red shows at {row},{column}");
            let rounded = [0, 1, 2].map(|channel| (pixel[channel] * 1000.0).round() as i32);
            if !seen.contains(&rounded) {
                seen.push(rounded);
            }
        }
    }
    assert_eq!(seen.len(), 2, "a light and a dark square: {seen:?}");
    // the squares are 8 px: the first (light, 0.8) and the next one across (dark, 0.6)
    assert!((at(&sheet, GAP, GAP)[0] - 0.8).abs() < 1e-6 && (at(&sheet, GAP, GAP + 8)[0] - 0.6).abs() < 1e-6);
    Ok(())
}

#[test]
fn an_opaque_four_channel_picture_is_its_colour_and_a_half_clear_one_a_mix_of_colour_and_board() -> Outcome {
    let colour = [0.2_f32, 0.4, 0.6];
    let (plain, opaque) = (flat(8, colour)?, Image::rgba(8, 8, &[0.2, 0.4, 0.6, 1.0].repeat(64))?);
    let (one, other) = (
        contact_sheet(&[Tile::new("a", &plain)], 3, 64)?,
        contact_sheet(&[Tile::new("a", &opaque)], 3, 64)?,
    );
    for (a, b) in one.clone().into_values().iter().zip(other.into_values()) {
        assert!((a - b).abs() < 1e-6);
    }
    let half = Image::rgba(8, 8, &[0.2, 0.4, 0.6, 0.5].repeat(64))?;
    let (mixed, board) = (
        contact_sheet(&[Tile::new("a", &half)], 3, 64)?,
        contact_sheet(&[Tile::new("a", &clear_red(8)?)], 3, 64)?,
    );
    for (row, column) in [(GAP, GAP), (GAP, GAP + 8), (GAP + 20, GAP + 30)] {
        for channel in 0..3 {
            let expected = 0.5 * colour[channel] + 0.5 * at(&board, row, column)[channel];
            assert!((at(&mixed, row, column)[channel] - expected).abs() < 1e-5);
        }
    }
    Ok(())
}

/// Whether any pixel column of `sheet` differs from `blank`'s (the same sheet with no labels), over `from..to`.
fn ink_between(sheet: &Image, blank: &Image, from: usize, to: usize) -> bool {
    let (a, b) = (sheet.clone().into_values(), blank.clone().into_values());
    (0..sheet.height()).any(|row| {
        (from..to).any(|column| {
            let at = (row * sheet.width() + column) * 3;
            a[at..at + 3] != b[at..at + 3]
        })
    })
}

#[test]
fn a_label_never_runs_past_its_tile_into_the_gutter() -> Outcome {
    let long = "split=9.50 bands=8 seed=1234 ".repeat(4);
    let sets: [(&str, [&str; 3]); 4] = [
        ("short labels fit as they are", ["a=1", "b=2", "c=3"]),
        (
            "a long label in the middle column",
            ["a=1", "split=9.50 bands=8", "c=3"],
        ),
        (
            "a long label in the last column",
            ["a=1", "b=2", "split=9.50 bands=8 seed=1234"],
        ),
        ("a label too long for any size", ["a=1", "b=2", long.as_str()]),
    ];
    let tile = 120;
    let picture = flat(tile, [0.5; 3])?;
    for (name, labels) in sets {
        let tiles: Vec<Tile<'_>> = labels.iter().map(|label| Tile::new(label, &picture)).collect();
        let blanks: Vec<Tile<'_>> = labels.iter().map(|_| Tile::new("", &picture)).collect();
        let (sheet, blank) = (contact_sheet(&tiles, 3, tile)?, contact_sheet(&blanks, 3, tile)?);
        for column in 0..3 {
            let left = GAP + column * (tile + GAP);
            assert!(
                ink_between(&sheet, &blank, left, left + tile),
                "{name}: the label of tile {column} is missing"
            );
            assert!(
                !ink_between(&sheet, &blank, left + tile, left + tile + GAP),
                "{name}: tile {column} runs into the gutter"
            );
        }
    }
    Ok(())
}

#[test]
fn a_sheet_needs_a_tile_a_column_and_a_tile_size() -> Outcome {
    let picture = flat(8, [0.5; 3])?;
    let one = [Tile::new("a", &picture)];
    for (tiles, columns, size, word) in [
        (&[][..], 3, 64, "tile"),
        (&one[..], 0, 64, "column"),
        (&one[..], 3, 0, "size"),
    ] {
        let refused = contact_sheet(tiles, columns, size);
        assert!(
            matches!(&refused, Err(error) if error.to_string().contains(word)),
            "{word}: {refused:?}"
        );
    }
    Ok(())
}

#[test]
fn a_label_too_long_for_any_size_is_cut_at_the_smallest_size_so_it_fills_the_tile() -> Outcome {
    // at the smallest type size a character is 6 px: in a 115 px tile 18 characters and the `…` fill 114 px, where the
    // largest size (10 px a character) would stop at 110: ink in the last 5 px says the smallest size was used
    let (tile, long) = (115, "split=9.50 bands=8 seed=1234 ".repeat(4));
    let picture = flat(tile, [0.5; 3])?;
    let (sheet, blank) = (
        contact_sheet(&[Tile::new(&long, &picture)], 1, tile)?,
        contact_sheet(&[Tile::new("", &picture)], 1, tile)?,
    );
    assert!(ink_between(&sheet, &blank, GAP + tile - 5, GAP + tile));
    Ok(())
}
