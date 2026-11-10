//! `shatter`: the picture broken into pieces. A piece shows as one flat colour at `fill` 1, so the pieces are read back
//! from the output by their colour, and each flat colour is checked against the mean of the input over its own pixels,
//! worked out here pixel by pixel.

use std::collections::HashMap;

use bullterrier_image::effects::shatter::{Params, Shape};
use bullterrier_image::params::Color;
use bullterrier_image::{Error, Interpolation};

use crate::common::compare::first_difference;
use crate::common::numbers::float;
use crate::common::pictures::{step, varied};
use crate::common::pixels::run_picture;

const HEIGHT: usize = 30;
const WIDTH: usize = 40;

fn shatter(params: &Params) -> Result<Vec<f32>, Error> {
    run_picture(params, HEIGHT, WIDTH, &varied(HEIGHT, WIDTH))
}

/// Pieces in place and flat: nothing moves, each piece one colour.
fn flat(pieces: usize, shape: Shape, seed: u32) -> Params {
    Params {
        pieces,
        shape,
        fill: 1.0,
        offset: 0.0,
        angle: 0.0,
        seed,
        ..Params::default()
    }
}

/// The pixels (indices) of each distinct colour of `output`, in order of first appearance.
fn groups(output: &[f32]) -> Vec<Vec<usize>> {
    let mut found: HashMap<[u32; 3], usize> = HashMap::new();
    let mut all: Vec<Vec<usize>> = Vec::new();
    for (at, pixel) in output.as_chunks::<3>().0.iter().enumerate() {
        let key = pixel.map(f32::to_bits);
        let slot = *found.entry(key).or_insert_with(|| {
            all.push(Vec::new());
            all.len() - 1
        });
        all[slot].push(at);
    }
    all
}

fn same_pixels(left: &[f32], right: &[f32]) -> bool {
    first_difference(
        &left.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        &right.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
    )
    .is_none()
}

#[test]
fn with_no_move_no_turn_and_no_lines_the_picture_is_unchanged() -> Result<(), Error> {
    let picture = varied(HEIGHT, WIDTH);
    for shape in [Shape::Voronoi, Shape::Triangles] {
        for pieces in [4, 24] {
            let out = shatter(&Params {
                pieces,
                shape,
                offset: 0.0,
                angle: 0.0,
                gap: 0.0,
                ..Params::default()
            })?;
            let worst = out.iter().zip(&picture).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
            assert!(worst < 1e-5, "{shape:?}, {pieces} pieces: off by {worst}");
        }
    }
    Ok(())
}

#[test]
fn a_voronoi_picture_has_as_many_pieces_as_pieces_asked_for() -> Result<(), Error> {
    for pieces in [4, 12, 24] {
        let out = shatter(&flat(pieces, Shape::Voronoi, 3))?;
        assert_eq!(groups(&out).len(), pieces, "{pieces} pieces");
    }
    Ok(())
}

#[test]
fn triangles_make_two_pieces_per_point_plus_the_edge_points_less_two() -> Result<(), Error> {
    // 4 points on a 40 x 30 picture: the edge points are 2 + 2 + 2 + 2 (spacing sqrt(1200 / 4) = 17.3 px, so 2 parts on
    // each side), and 4 + 8 points with 8 on the hull make 2 x 12 - 2 - 8 = 14 triangles
    let out = shatter(&flat(4, Shape::Triangles, 3))?;
    assert_eq!(groups(&out).len(), 14);
    Ok(())
}

#[test]
fn each_flat_piece_is_the_mean_of_the_picture_over_its_pixels_and_is_convex_along_rows_and_columns() -> Result<(), Error>
{
    let picture = varied(HEIGHT, WIDTH);
    for shape in [Shape::Voronoi, Shape::Triangles] {
        let out = shatter(&flat(16, shape, 5))?;
        for piece in groups(&out) {
            for channel in 0..3 {
                let mean = piece
                    .iter()
                    .map(|&at| f64::from(picture[at * 3 + channel]))
                    .sum::<f64>()
                    / float(piece.len());
                let shown = f64::from(out[piece[0] * 3 + channel]);
                assert!(
                    (mean - shown).abs() < 1e-5,
                    "{shape:?}, channel {channel}: {mean} against {shown}"
                );
            }
            // a piece is convex, so along any row (and any column) its pixels are one run
            for row in 0..HEIGHT {
                let columns: Vec<usize> = piece
                    .iter()
                    .filter(|&&at| at / WIDTH == row)
                    .map(|&at| at % WIDTH)
                    .collect();
                assert!(
                    columns.windows(2).all(|pair| pair[1] == pair[0] + 1),
                    "{shape:?}, row {row}: {columns:?}"
                );
            }
            for column in 0..WIDTH {
                let rows: Vec<usize> = piece
                    .iter()
                    .filter(|&&at| at % WIDTH == column)
                    .map(|&at| at / WIDTH)
                    .collect();
                assert!(
                    rows.windows(2).all(|pair| pair[1] == pair[0] + 1),
                    "{shape:?}, column {column}: {rows:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_gap_of_2_paints_the_pixels_beside_a_border_in_the_colour_and_leaves_the_others() -> Result<(), Error> {
    let apart = flat(12, Shape::Voronoi, 7);
    let without = shatter(&apart)?;
    let with = shatter(&Params {
        gap: 2.0,
        color: Color([1.0, 0.0, 0.0]),
        ..apart
    })?;
    let mut painted = 0;
    for at in 0..HEIGHT * WIDTH {
        let (row, column) = (at / WIDTH, at % WIDTH);
        let piece = |other: usize| without[other * 3..][..3] != without[at * 3..][..3];
        // by hand: a pixel with a 4-neighbour of another piece lies half a px from a border, which a line 2 px wide covers
        let beside = (column > 0 && piece(at - 1))
            || (column + 1 < WIDTH && piece(at + 1))
            || (row > 0 && piece(at - WIDTH))
            || (row + 1 < HEIGHT && piece(at + WIDTH));
        let wanted = if beside {
            [1.0, 0.0, 0.0]
        } else {
            [without[at * 3], without[at * 3 + 1], without[at * 3 + 2]]
        };
        painted += usize::from(beside);
        assert!(
            with[at * 3..][..3] == wanted,
            "pixel {at}: {:?} against {wanted:?}",
            &with[at * 3..][..3]
        );
    }
    assert!(painted > 20);
    Ok(())
}

#[test]
fn the_same_seed_gives_the_same_break_and_another_seed_another() -> Result<(), Error> {
    let at = |seed: u32| {
        shatter(&Params {
            seed,
            gap: 1.0,
            ..Params::default()
        })
    };
    assert!(same_pixels(&at(4)?, &at(4)?));
    assert!(!same_pixels(&at(4)?, &at(5)?));
    Ok(())
}

#[test]
fn reading_the_nearest_pixel_keeps_every_value_of_the_picture() -> Result<(), Error> {
    let picture = varied(HEIGHT, WIDTH);
    let out = shatter(&Params {
        interpolation: Interpolation::Nearest,
        offset: 6.0,
        angle: 10.0,
        ..Params::default()
    })?;
    let mut present: Vec<[u32; 3]> = picture.as_chunks::<3>().0.iter().map(|p| p.map(f32::to_bits)).collect();
    present.sort_unstable();
    assert!(
        out.as_chunks::<3>()
            .0
            .iter()
            .all(|p| present.binary_search(&p.map(f32::to_bits)).is_ok())
    );
    assert!(!same_pixels(&out, &picture), "the pieces did not move");
    Ok(())
}

#[test]
fn detail_gathers_more_pieces_along_the_picture_s_edge() -> Result<(), Error> {
    // a dark left half and a bright right half: the one edge is between columns 19 and 20
    let picture = step(HEIGHT, WIDTH, 0.1, 0.9);
    let across_edge = |detail: f32, seed: u32| -> Result<usize, Error> {
        let out = run_picture(
            &Params {
                detail,
                ..flat(24, Shape::Voronoi, seed)
            },
            HEIGHT,
            WIDTH,
            &picture,
        )?;
        let band: Vec<usize> = (0..HEIGHT)
            .flat_map(|row| (18..22).map(move |column| row * WIDTH + column))
            .collect();
        // the pieces that reach the band
        let mut seen: Vec<[u32; 3]> = band
            .iter()
            .map(|&at| {
                out[at * 3..][..3]
                    .try_into()
                    .map_or([0; 3], |p: [f32; 3]| p.map(f32::to_bits))
            })
            .collect();
        seen.sort_unstable();
        seen.dedup();
        Ok(seen.len())
    };
    let (mut even, mut following) = (0, 0);
    for seed in 0..6 {
        even += across_edge(0.0, seed)?;
        following += across_edge(1.0, seed)?;
    }
    assert!(following > even + 12, "{following} against {even}");
    Ok(())
}

#[test]
fn more_pieces_than_pixels_still_work() -> Result<(), Error> {
    for shape in [Shape::Voronoi, Shape::Triangles] {
        let out = run_picture(
            &Params {
                pieces: 500,
                fill: 1.0,
                gap: 1.0,
                shape,
                ..Params::default()
            },
            3,
            4,
            &varied(3, 4),
        )?;
        assert!(
            out.iter().all(|level| (0.0..=1.0).contains(level)),
            "{shape:?}: {out:?}"
        );
    }
    Ok(())
}
