//! `pattern`: stripes, checks, dots and a grid worked out by hand for a 10 px cell, how they repeat and turn, the soft edge
//! and the defaults.

use bullterrier_image::Error;
use bullterrier_image::effects::pattern::{self, Kind};
use bullterrier_image::params::Color;

use crate::common::compare::assert_close;
use crate::common::pixels::run_sized;

const RED: [f32; 3] = [1.0, 0.0, 0.0];
const BLUE: [f32; 3] = [0.0, 0.0, 1.0];

/// `kind` in red on blue, with a hard edge, in cells of 10 px.
fn red_on_blue(kind: Kind) -> pattern::Params {
    pattern::Params {
        kind,
        cell: 10.0,
        foreground: Color(RED),
        background: Color(BLUE),
        feather: 0.0,
        ..pattern::Params::default()
    }
}

/// A picture of `params`, `width` pixels across, row after row.
struct Drawn {
    width: usize,
    pixels: Vec<[f32; 3]>,
}

impl Drawn {
    fn new(params: &pattern::Params, height: usize, width: usize) -> Result<Self, Error> {
        Ok(Self {
            width,
            pixels: run_sized(params, height, width, 0.0)?,
        })
    }

    fn at(&self, row: usize, column: usize) -> [f32; 3] {
        self.pixels[row * self.width + column]
    }

    /// Whether each pixel is the foreground colour (red); none if a pixel is neither colour.
    fn foreground(&self) -> Option<Vec<bool>> {
        self.pixels
            .iter()
            .map(|&pixel| match pixel {
                RED => Some(true),
                BLUE => Some(false),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn each_kind_draws_the_pattern_worked_out_by_hand_on_a_10_px_cell() -> Result<(), Error> {
    // whether the pixel in column x, row y is the foreground
    type Inside = fn(i32, i32) -> bool;
    let cases: [(Kind, f32, Inside); 4] = [
        // a band 4 px wide in the middle of every cell: pixel centres 3.5 to 6.5
        (Kind::Stripes, 0.4, |x, _| (3..=6).contains(&(x % 10))),
        // lines 2 px wide centred on the cell borders: the last and the first pixel of a cell
        (Kind::Grid, 0.2, |x, y| {
            [0, 9].contains(&(x % 10)) || [0, 9].contains(&(y % 10))
        }),
        // 10 px squares alternate, the top-left one in the foreground colour
        (Kind::Checks, 0.2, |x, y| (x / 10 + y / 10) % 2 == 0),
        // a disc of 3 px round the cell centre (5, 5): a pixel centre is within 3 when twice its offset is within 6
        (Kind::Dots, 0.2, |x, y| {
            (2 * (x % 10) - 9).pow(2) + (2 * (y % 10) - 9).pow(2) < 36
        }),
    ];
    for (kind, thickness, inside) in cases {
        let params = pattern::Params {
            thickness,
            radius: 0.3,
            ..red_on_blue(kind)
        };
        let expected: Vec<bool> = (0..40 * 40).map(|index| inside(index % 40, index / 40)).collect();
        assert_eq!(Drawn::new(&params, 40, 40)?.foreground(), Some(expected), "{kind:?}");
    }
    Ok(())
}

#[test]
fn the_angle_turns_the_pattern_clockwise_about_the_top_left_corner() -> Result<(), Error> {
    // at 90 the stripes are horizontal: the same along each row, different between rows
    let params = pattern::Params {
        angle: 90.0,
        ..red_on_blue(Kind::Stripes)
    };
    let horizontal = Drawn::new(&params, 40, 40)?;
    for row in 0..40 {
        assert!(
            (0..40)
                .all(|column| horizontal.at(row, column).map(f32::to_bits) == horizontal.at(row, 0).map(f32::to_bits)),
            "row {row}"
        );
    }
    assert_ne!(horizontal.at(0, 0), horizontal.at(5, 0));
    // at 45 the stripes run along the other diagonal: one down and one left is the same place on the pattern
    let params = pattern::Params {
        angle: 45.0,
        feather: 3.0,
        ..red_on_blue(Kind::Stripes)
    };
    let diagonal = Drawn::new(&params, 60, 60)?;
    for (row, column) in (1..60).flat_map(|row| (0..59).map(move |column| (row, column))) {
        assert_close(
            &diagonal.at(row, column),
            &diagonal.at(row - 1, column + 1),
            1e-3,
            &format!("({row}, {column})"),
        );
    }
    // a quarter turn moves the checks one square sideways, so the top-left square shows the neighbour's colour
    let turned = pattern::Params {
        angle: 90.0,
        ..red_on_blue(Kind::Checks)
    };
    let upright = Drawn::new(&red_on_blue(Kind::Checks), 40, 40)?.foreground();
    let swapped = upright
        .as_ref()
        .map(|colours| colours.iter().map(|&colour| !colour).collect());
    assert_eq!(Drawn::new(&turned, 40, 40)?.foreground(), swapped);
    Ok(())
}

#[test]
fn feather_blends_the_edge_over_that_many_px_and_zero_gives_hard_edges() -> Result<(), Error> {
    // stripes 5 px wide in a 10 px cell have their edges at x = 2.5 and 7.5: column 2 has its centre on the edge (half and
    // half), column 3 is 1 px inside it, so its coverage is 0.5 + 1 / 4
    let params = pattern::Params {
        thickness: 0.5,
        feather: 4.0,
        ..red_on_blue(Kind::Stripes)
    };
    let soft = Drawn::new(&params, 1, 10)?;
    assert_close(&soft.at(0, 2), &[0.5, 0.0, 0.5], 1e-5, "on the edge");
    assert_close(&soft.at(0, 3), &[0.75, 0.0, 0.25], 1e-5, "1 px inside the edge");
    assert_close(&soft.at(0, 5), &RED, 1e-6, "the middle of the stripe");
    Ok(())
}

#[test]
fn with_no_settings_it_draws_white_on_near_black_with_a_cell_of_32_px_and_a_feather_of_1_px() -> Result<(), Error> {
    // the coverage of the foreground at (row, column) of a 40 x 40 picture, worked out by hand from the documentation
    let dot = 0.5 + (9.6 - (9.5_f32.powi(2) + 0.5_f32.powi(2)).sqrt());
    let cases = [
        // a line 0.2 x 32 = 6.4 px wide, so 3.2 px each side of a border, fully covers the pixel centred 2.5 px in
        (Kind::Grid, 16, 2, 1.0),
        (Kind::Grid, 16, 3, 0.2), // 0.3 px beyond the line, feathered over 1 px: 0.5 - 0.3
        (Kind::Grid, 16, 4, 0.0),
        (Kind::Grid, 16, 16, 0.0),   // the middle of the cell is far from every line
        (Kind::Stripes, 5, 18, 1.0), // centred on the middle of the cell, the same in every row
        (Kind::Stripes, 0, 18, 1.0),
        (Kind::Stripes, 9, 19, 0.2), // 3.5 px from the middle, 0.3 px beyond the stripe
        (Kind::Stripes, 0, 0, 0.0),
        (Kind::Dots, 16, 25, dot), // the dot is 0.3 x 32 = 9.6 px in radius
        (Kind::Dots, 16, 26, 0.0),
        (Kind::Checks, 15, 15, 1.0), // the top-left square is the foreground
    ];
    let ground = [16.0 / 255.0, 16.0 / 255.0, 24.0 / 255.0];
    for (kind, row, column, coverage) in cases {
        let params = pattern::Params {
            kind,
            ..pattern::Params::default()
        };
        let expected = ground.map(|level| level + (1.0 - level) * coverage);
        assert_close(
            &Drawn::new(&params, 40, 40)?.at(row, column),
            &expected,
            1e-6,
            &format!("{kind:?} at ({row}, {column})"),
        );
    }
    Ok(())
}
