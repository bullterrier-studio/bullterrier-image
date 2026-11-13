//! `components` regions: the connected pieces of a selection, seen as the partition an effect is handed. Pictures are
//! drawn (`#` selected, `.` not), so the pieces are known by construction; a brute-force labelling (propagating the least
//! pixel index among neighbours until nothing changes) is the oracle for pictures too irregular to draw.

use bullterrier_image::region::{Components, Pick};
use bullterrier_image::{Error, ErrorKind, Options};

use crate::common::ops::probe;
use crate::common::pieces::{drawn, numbered_by_first_pixel};

/// The map of a drawing: 1 where `#`, 0 elsewhere, and its height and width.
fn map(art: &[&str]) -> (Vec<f32>, (usize, usize)) {
    let values = art
        .iter()
        .flat_map(|row| row.chars().map(|cell| f32::from(u8::from(cell == '#'))))
        .collect();
    (values, (art.len(), art[0].len()))
}

fn components(min_area: f64) -> Components {
    Components {
        of: "subject".to_owned(),
        min_area,
        ..Components::default()
    }
}

/// The labels and number of pieces `region` makes of the selection `values` (the map `subject`).
fn pieces(values: Vec<f32>, size: (usize, usize), region: Components) -> Result<(Vec<i32>, usize), Error> {
    let seen = probe(
        size,
        Some(values),
        Options::default(),
        vec![("s", region.into())],
        (Some("s"), None),
    )?;
    Ok((seen.labels, seen.pieces))
}

fn drawn_pieces(art: &[&str], region: Components) -> Result<String, Error> {
    let (values, size) = map(art);
    let (labels, _) = pieces(values, size, region)?;
    Ok(drawn(&labels, size.1))
}

#[test]
fn each_separate_shape_is_a_piece_numbered_by_its_first_pixel_and_the_rest_is_one_more_piece() -> Result<(), Error> {
    let art = ["........", ".##..##.", ".##..##.", "........"];
    // the rest is numbered by its first pixel like the others: it begins the picture
    assert_eq!(
        drawn_pieces(&art, components(0.0))?,
        "00000000/01100220/01100220/00000000"
    );
    // a picture that starts with a shape numbers it first, and the rest after it
    let art = ["##..#", "##..#", "....."];
    assert_eq!(drawn_pieces(&art, components(0.0))?, "00112/00112/11111");
    Ok(())
}

#[test]
fn pieces_touching_only_at_a_corner_are_two_pieces_and_a_tilted_bar_is_one() -> Result<(), Error> {
    let corner = ["##..", "##..", "..##", "..##"];
    assert_eq!(drawn_pieces(&corner, components(0.0))?, "0011/0011/1122/1122");
    // a bar seen at an angle, as a facade is: each row's pixels share an edge with the next row's
    let tilted = ["##....", ".##...", "..##..", "...##.", "....##"];
    assert_eq!(
        drawn_pieces(&tilted, components(0.0))?,
        "001111/100111/110011/111001/111100"
    );
    Ok(())
}

#[test]
fn a_pixel_is_in_from_the_threshold_up_and_a_selection_of_everything_or_nothing_is_one_piece() -> Result<(), Error> {
    let at = |threshold: f64, level: f32| {
        let region = Components {
            threshold,
            ..components(0.0)
        };
        pieces(vec![level; 6], (2, 3), region).map(|(labels, count)| (labels[0], count))
    };
    assert_eq!(
        at(0.5, 0.5)?,
        (0, 1),
        "at the threshold: in, so all of it is the one piece"
    );
    assert_eq!(at(0.5, 0.25)?, (0, 1), "below it: all rest, one piece");
    assert_eq!(at(0.25, 0.25)?, (0, 1));
    // a shape beside the rest shows which side of the threshold decides: the shape is at 0.5
    let (values, size) = (vec![0.5, 0.5, 0.0, 0.0], (1, 4));
    let at = |threshold: f64| {
        let region = Components {
            threshold,
            ..components(0.0)
        };
        pieces(values.clone(), size, region).map(|(labels, _)| drawn(&labels, 4))
    };
    assert_eq!(at(0.5)?, "0011");
    assert_eq!(at(0.5001)?, "0000");
    Ok(())
}

#[test]
fn a_piece_under_min_area_of_the_frame_joins_the_rest_and_one_of_exactly_that_area_stays() -> Result<(), Error> {
    // 4 x 4 pixels: the square is 4 of 16, a quarter; the lone pixel a sixteenth
    let art = ["##.#", "##..", "....", "...."];
    assert_eq!(drawn_pieces(&art, components(0.0))?, "0012/0011/1111/1111");
    assert_eq!(
        drawn_pieces(&art, components(0.0625))?,
        "0012/0011/1111/1111",
        "a sixteenth is kept"
    );
    assert_eq!(drawn_pieces(&art, components(0.07))?, "0011/0011/1111/1111");
    assert_eq!(
        drawn_pieces(&art, components(0.25))?,
        "0011/0011/1111/1111",
        "a quarter is kept"
    );
    assert_eq!(
        drawn_pieces(&art, components(0.26))?,
        "0000/0000/0000/0000",
        "everything dropped"
    );
    Ok(())
}

#[test]
fn more_pieces_than_max_pieces_is_refused_naming_the_limit_and_the_count_and_dropped_ones_are_not_counted()
-> Result<(), Error> {
    let art = ["#.#.#", "....."];
    let (values, size) = map(&art);
    let with = |max_pieces: u32, min_area: f64| {
        let region = Components {
            max_pieces,
            ..components(min_area)
        };
        pieces(values.clone(), size, region)
    };
    assert_eq!(with(3, 0.0)?.1, 4, "three shapes and the rest");
    let refused = with(2, 0.0)
        .err()
        .ok_or_else(|| Error::Recipe("not refused".to_owned()))?;
    let said = refused.to_string();
    assert!(
        refused.kind() == ErrorKind::Recipe && said.contains("max_pieces") && said.contains('2') && said.contains('3'),
        "{said}"
    );
    // each shape is a tenth of the frame: under a share of 0.2 none counts
    assert_eq!(with(1, 0.2)?.1, 1);
    Ok(())
}

/// The selection of the pixels of `values` from 0.5 up, labelled by brute force: every selected pixel starts as its own
/// index and takes the least index among its selected neighbours until nothing changes; the pixels that are not selected
/// are labelled by their least index together. Numbered by first pixel, which is the order of the least indices.
fn reference(values: &[f32], (height, width): (usize, usize)) -> Vec<i32> {
    let inside: Vec<bool> = values.iter().map(|&value| value >= 0.5).collect();
    let mut label: Vec<usize> = (0..values.len()).collect();
    let rest = inside.iter().position(|&on| !on);
    loop {
        let mut changed = false;
        for at in 0..label.len() {
            let (row, column) = (at / width, at % width);
            let neighbours = [
                (row > 0).then(|| at - width),
                (row + 1 < height).then(|| at + width),
                (column > 0).then(|| at - 1),
                (column + 1 < width).then(|| at + 1),
            ];
            for other in neighbours.into_iter().flatten() {
                if inside[other] == inside[at] && inside[at] && label[other] < label[at] {
                    label[at] = label[other];
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut least: Vec<usize> = (0..label.len())
        .filter(|&at| if inside[at] { label[at] == at } else { Some(at) == rest })
        .collect();
    least.sort_unstable();
    (0..label.len())
        .map(|at| {
            let own = if inside[at] { label[at] } else { rest.unwrap_or(0) };
            i32::try_from(least.iter().position(|&first| first == own).unwrap_or(0)).unwrap_or(0)
        })
        .collect()
}

#[test]
fn the_pieces_are_those_of_a_brute_force_labelling_on_irregular_pictures() -> Result<(), Error> {
    let (height, width) = (21, 27);
    for (seed, density) in [(1_u32, 20_u32), (2, 45), (3, 60), (4, 75), (5, 90)] {
        // a deterministic scatter at each density: from many specks to a few large shapes
        let mut state = seed;
        let mut cell = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 16) % 100 < density
        };
        let values: Vec<f32> = (0..height * width).map(|_| f32::from(u8::from(cell()))).collect();
        let (labels, count) = pieces(values.clone(), (height, width), components(0.0))?;
        let wanted = reference(&values, (height, width));
        assert_eq!(
            crate::common::compare::first_difference(&labels, &wanted),
            None,
            "density {density}"
        );
        assert!(numbered_by_first_pixel(&labels, count));
    }
    Ok(())
}

#[test]
fn a_pick_chooses_the_pixels_of_one_piece_and_the_other_pieces_stay_out() -> Result<(), Error> {
    let (values, size) = map(&["#...#", "#.#.#", "..#.."]);
    // pieces: 0 the left bar, 1 the rest, 2 the right bar, 3 the middle shape (by first pixel)
    let region = Components {
        of: "subject".to_owned(),
        ..Components::default()
    };
    let pick = Pick {
        of: "s".to_owned(),
        pieces: vec![3.into()],
    };
    let seen = probe(
        size,
        Some(values),
        Options::default(),
        vec![("s", region.into()), ("p", pick.into())],
        (None, Some("p")),
    )?;
    let chosen = seen.near.ok_or_else(|| Error::Recipe("no selection".to_owned()))?;
    assert_eq!(
        chosen
            .iter()
            .map(|&value| if value > 0.5 { '1' } else { '0' })
            .collect::<String>(),
        "000000010000100"
    );
    Ok(())
}
