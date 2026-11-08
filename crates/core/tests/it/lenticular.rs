//! `lenticular`: vertical strips, the 1st, 3rd... as the picture is and the 2nd, 4th... as the picture slid sideways.
//! Checked by the parity of each column's strip against the picture and its slid copy from the definition, and by a row
//! worked out by hand.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "small picture sizes"
)]

use bullterrier_image::effects::lenticular::Params;
use bullterrier_image::{Edge, Error};

use crate::common::compare::{first_difference, first_difference_within};
use crate::common::pictures::varied;
use crate::common::pixels::{channel, run_picture};
use crate::common::sampling::moved;

const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];

#[test]
fn the_odd_strips_show_the_picture_and_the_even_ones_the_slid_picture_at_their_own_columns() -> Result<(), Error> {
    for (height, width) in [(1, 1), (1, 13), (4, 1), (5, 9), (12, 40)] {
        let picture = varied(height, width);
        for edge in EDGES {
            // strips of one column, ones that do not divide the width, ones as wide as the picture and wider; shifts both
            // ways, fractional, and beyond the width
            for (strip, shift) in [
                (1, 1.0),
                (3, -2.5),
                (4, 7.25),
                (width as i32, 3.0),
                (500, 3.0),
                (2, 90.0),
            ] {
                let params = Params { strip, shift, edge };
                let out = run_picture(&params, height, width, &picture)?;
                let slid = moved(&picture, height, width, edge, 0.0, f64::from(shift));
                let expected: Vec<f32> = (0..height * width * 3)
                    .map(|at| {
                        let column = at / 3 % width;
                        if (column / strip as usize) % 2 == 1 {
                            slid[at]
                        } else {
                            picture[at]
                        }
                    })
                    .collect();
                let at = first_difference_within(&out, &expected, 1e-5);
                assert_eq!(at, None, "{height} x {width}, {params:?}: first differs at {at:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn a_row_is_cut_into_strips_and_the_second_fourth_strips_slide_by_hand() -> Result<(), Error> {
    // a b c d e f g h, strips of 2, slid 1 right (wrap): the 2nd strip (columns 2, 3) shows the columns 1, 2, the 4th (6, 7)
    // the columns 5, 6
    let row = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
    let grey: Vec<f32> = row.iter().flat_map(|&level| [level; 3]).collect();
    let params = Params {
        strip: 2,
        shift: 1.0,
        edge: Edge::Wrap,
    };
    let out = run_picture(&params, 1, 8, &grey)?;
    let red = channel(&out, 0);
    assert_eq!(first_difference(&red, &[0.1, 0.2, 0.2, 0.3, 0.5, 0.6, 0.6, 0.7]), None);
    Ok(())
}

#[test]
fn no_slide_and_a_strip_as_wide_as_the_picture_leave_the_picture_as_it_is() -> Result<(), Error> {
    let picture = varied(6, 17);
    for params in [
        Params {
            shift: 0.0,
            ..Params::default()
        },
        Params {
            strip: 17,
            ..Params::default()
        },
        Params {
            strip: 500,
            shift: -500.0,
            ..Params::default()
        },
    ] {
        assert_eq!(
            first_difference(&run_picture(&params, 6, 17, &picture)?, &picture),
            None,
            "{params:?}"
        );
    }
    Ok(())
}

#[test]
fn a_strip_slides_sideways_only_so_a_row_depends_on_its_own_row() -> Result<(), Error> {
    // changing one row of the picture changes only that row of the result
    let picture = varied(7, 20);
    let mut other = picture.clone();
    for level in &mut other[3 * 20 * 3..4 * 20 * 3] {
        *level = 1.0 - *level;
    }
    let (one, two) = (
        run_picture(&Params::default(), 7, 20, &picture)?,
        run_picture(&Params::default(), 7, 20, &other)?,
    );
    for row in 0..7 {
        let same = first_difference(&one[row * 60..(row + 1) * 60], &two[row * 60..(row + 1) * 60]).is_none();
        assert_eq!(same, row != 3, "row {row}");
    }
    Ok(())
}
