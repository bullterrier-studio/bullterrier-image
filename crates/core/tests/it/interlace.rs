//! `interlace`: the rows of one field slid sideways, the other field left as it is. Checked by which rows move, by the
//! slid rows against the picture moved from the definition, and by a small picture worked out by hand.

use bullterrier_image::effects::interlace::{Field, Params};
use bullterrier_image::{Edge, Error};

use crate::common::compare::{first_difference, first_difference_within};
use crate::common::pictures::varied;
use crate::common::pixels::{channel, run_picture};
use crate::common::sampling::moved;

const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];

#[test]
fn only_the_rows_of_the_chosen_field_move_and_they_slide_as_the_definition_says() -> Result<(), Error> {
    for (height, width) in [(1, 1), (1, 9), (2, 5), (7, 1), (9, 16), (16, 10)] {
        let picture = varied(height, width);
        for edge in EDGES {
            for (field, slid) in [(Field::Odd, 1), (Field::Even, 0)] {
                for offset in [6.0, -2.5, 0.25, 40.0] {
                    let params = Params { offset, field, edge };
                    let out = run_picture(&params, height, width, &picture)?;
                    let by_definition = moved(&picture, height, width, edge, 0.0, f64::from(offset));
                    let expected: Vec<f32> = (0..height * width * 3)
                        .map(|at| {
                            if at / 3 / width % 2 == slid {
                                by_definition[at]
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
    }
    Ok(())
}

#[test]
fn the_odd_field_is_the_second_fourth_rows_counting_from_the_top_as_the_first() -> Result<(), Error> {
    // rows a b c d (grey, one pixel each), slid 1 right with wrap: the odd field slides, the even stays
    let rows = [[0.1, 0.2, 0.3], [0.4, 0.5, 0.6], [0.7, 0.8, 0.9]];
    let grey: Vec<f32> = rows.iter().flatten().flat_map(|&level| [level; 3]).collect();
    for (field, expected) in [
        (Field::Odd, [[0.1, 0.2, 0.3], [0.6, 0.4, 0.5], [0.7, 0.8, 0.9]]),
        (Field::Even, [[0.3, 0.1, 0.2], [0.4, 0.5, 0.6], [0.9, 0.7, 0.8]]),
    ] {
        let params = Params {
            offset: 1.0,
            field,
            edge: Edge::Wrap,
        };
        let out = run_picture(&params, 3, 3, &grey)?;
        let red = channel(&out, 0);
        let expected: Vec<f32> = expected.iter().flatten().copied().collect();
        assert_eq!(first_difference_within(&red, &expected, 1e-6), None, "{field:?}");
    }
    Ok(())
}

#[test]
fn no_offset_leaves_the_picture_exactly_as_it_is() -> Result<(), Error> {
    let picture = varied(8, 13);
    let params = Params {
        offset: 0.0,
        ..Params::default()
    };
    assert_eq!(
        first_difference(&run_picture(&params, 8, 13, &picture)?, &picture),
        None
    );
    Ok(())
}
