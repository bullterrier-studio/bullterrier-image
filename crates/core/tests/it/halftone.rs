//! `halftone`: tone becomes dot size. The oracles are the screen's geometry (every shape keeps the tone: the mean ink of
//! a flat tone is the tone), hand-worked colours, and comparisons between renders that must be alike or must differ.

#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "small picture sizes and indices"
)]

use std::f32::consts::PI;

use bullterrier_image::effects::halftone::{Params, Separation, Shape};
use bullterrier_image::params::{Color, Declared, PictureName};
use bullterrier_image::{EdgeOrConstant, Error, Image, Inputs, Picture, apply};
use serde_json::Value;

use crate::common::compare::{assert_close, first_difference};
use crate::common::pictures::varied;
use crate::common::pixels::{is_pixel, run_picture};

const WHITE: Color = Color([1.0; 3]);

/// White dots on black paper.
fn white_on_black() -> Params {
    Params {
        ink: Some(WHITE),
        ..Params::default()
    }
}

/// The picture of one grey `level`, `height` x `width`.
fn flat(height: usize, width: usize, level: f32) -> Vec<f32> {
    vec![level; height * width * 3]
}

/// The red channel of the screen `params` print of `rgb`.
fn red(params: &Params, height: usize, width: usize, rgb: &[f32]) -> Result<Vec<f32>, Error> {
    Ok(run_picture(params, height, width, rgb)?
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| pixel[0])
        .collect())
}

/// The mean ink (red channel, white on black) of an even tone.
fn mean_ink(params: &Params, size: usize, tone: f32) -> Result<f32, Error> {
    let out = red(params, size, size, &flat(size, size, tone))?;
    Ok(out.iter().sum::<f32>() / out.len() as f32)
}

#[test]
fn white_is_nearly_all_dots_black_is_blank_and_paper_shows_exactly_where_no_ink_is_needed() -> Result<(), Error> {
    assert!(mean_ink(&white_on_black(), 28, 1.0)? > 0.9);
    assert!(mean_ink(&white_on_black(), 28, 0.0)?.abs() < f32::EPSILON);
    for (level, ink, paper) in [
        (0.0, [1.0; 3], [0.125, 0.1875, 0.25]),
        (1.0, [0.0; 3], [0.9375, 0.9, 0.8125]),
    ] {
        let params = Params {
            ink: Some(Color(ink)),
            paper: Color(paper),
            ..Params::default()
        };
        let out = run_picture(&params, 20, 24, &flat(20, 24, level))?;
        assert!(
            out.as_chunks::<3>().0.iter().all(|pixel| is_pixel(*pixel, paper)),
            "{level}"
        );
    }
    Ok(())
}

/// The coverage oracle's screen: black ink on white paper, `cell` 20, `feather` 1, no blur, a square screen of whole cells.
fn oracle(shape: Shape) -> Params {
    Params {
        shape,
        cell: 20.0,
        angle: 0.0,
        sigma: 0.0,
        feather: 1.0,
        ink: Some(Color([0.0; 3])),
        paper: Color([1.0; 3]),
        ..Params::default()
    }
}

/// The ink of the oracle screen `params` at an even `tone`, one value per pixel of a picture of 80 x 80 (4 x 4 cells).
fn ink_map(params: &Params, tone: f32) -> Result<Vec<f32>, Error> {
    Ok(red(params, 80, 80, &flat(80, 80, 1.0 - tone))?
        .into_iter()
        .map(|r| 1.0 - r)
        .collect())
}

/// The coverage oracle: the mean ink of the flat grey whose luminance is `1 - tone`, for the screen `params`.
fn coverage_with(params: &Params, tone: f32) -> Result<f32, Error> {
    let ink = ink_map(params, tone)?;
    Ok(ink.iter().sum::<f32>() / ink.len() as f32)
}

fn coverage(shape: Shape, tone: f32) -> Result<f32, Error> {
    coverage_with(&oracle(shape), tone)
}

#[test]
fn ink_grows_with_tone_and_every_shape_keeps_the_tone() -> Result<(), Error> {
    for shape in [Shape::Dot, Shape::Line] {
        let mut last = -1.0;
        for tone in [0.0, 0.001, 0.01, 0.05, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let ink = coverage(shape, tone)?;
            assert!(ink > last, "{shape:?} {tone}: {ink} after {last}");
            last = ink;
        }
        // the soft edge is centred on the dot's edge, so it keeps the area but for a curve's second-order gain
        for tone in [0.1, 0.25, 0.5, 0.75, 0.9] {
            let ink = coverage(shape, tone)?;
            assert!((ink - tone).abs() < 0.008, "{shape:?} tone {tone}: ink {ink}");
        }
    }
    Ok(())
}

#[test]
fn a_dot_prints_about_its_own_area_however_small() -> Result<(), Error> {
    // a soft edge one pixel wide adds at most pi / 12 px^2 to a dot (a linear ramp of width f on a circle's edge adds pi f^2 / 12);
    // a dot of radius r has the tone pi (r / cell)^2
    let cell = 20;
    let params = Params {
        cell: cell as f32,
        angle: 0.0,
        ..white_on_black()
    };
    for radius in [0.1_f32, 0.25, 0.4, 0.5, 1.0, 2.0, 3.0] {
        let tone = PI * (radius / cell as f32).powi(2);
        let ink = mean_ink(&params, 2 * cell, tone)? * (cell * cell) as f32;
        let area = PI * radius * radius;
        assert!(
            (ink - area).abs() <= PI / 12.0 + 0.05 * area,
            "radius {radius}: {ink} against {area}"
        );
    }
    Ok(())
}

#[test]
fn on_a_light_paper_dark_tones_get_the_ink_and_paper_threshold_decides_what_is_light() -> Result<(), Error> {
    let black_on_white = Params {
        ink: Some(Color([0.0; 3])),
        paper: Color([1.0; 3]),
        ..Params::default()
    };
    assert!(
        red(&black_on_white, 28, 28, &flat(28, 28, 1.0))?
            .iter()
            .all(|&value| (value - 1.0).abs() < 1e-6),
        "white needs no ink"
    );
    assert!(mean_ink(&black_on_white, 28, 0.0)? < 0.1, "black is nearly all ink");
    // #808080 has the luminance 0.502: light at the default threshold of 0.5, dark at 0.6
    let red_on_grey = Params {
        ink: Some(Color([1.0, 0.0, 0.0])),
        paper: Color([128.0 / 255.0; 3]),
        ..Params::default()
    };
    let white = flat(28, 28, 1.0);
    let light = run_picture(&red_on_grey, 28, 28, &white)?;
    assert_close(
        &light,
        &vec![128.0 / 255.0; light.len()],
        1e-6,
        "light paper: white needs no ink",
    );
    let dark = run_picture(
        &Params {
            paper_threshold: 0.6,
            ..red_on_grey
        },
        28,
        28,
        &white,
    )?;
    let (reds, greens): (Vec<f32>, Vec<f32>) = dark.as_chunks::<3>().0.iter().map(|pixel| (pixel[0], pixel[1])).unzip();
    assert!(
        reds.iter().sum::<f32>() / reds.len() as f32 > 0.9,
        "dark paper: white is all dots"
    );
    assert!(greens.iter().sum::<f32>() / (greens.len() as f32) < 0.1);
    Ok(())
}

#[test]
fn with_one_ink_every_pixel_is_a_mix_of_the_ink_and_the_paper() -> Result<(), Error> {
    let (ink, paper) = ([1.0_f32, 0.5, 0.0], [0.0_f32, 0.0, 0.25]);
    let params = Params {
        cell: 6.0,
        ink: Some(Color(ink)),
        paper: Color(paper),
        ..Params::default()
    };
    let out = run_picture(&params, 30, 36, &varied(30, 36))?;
    let mut total = 0.0;
    for pixel in out.as_chunks::<3>().0 {
        let share = pixel[0]; // red goes from the paper's 0 to the ink's 1: the share of ink
        total += share;
        for channel in 0..3 {
            assert!((pixel[channel] - (paper[channel] + share * (ink[channel] - paper[channel]))).abs() < 1e-5);
        }
    }
    assert!(total > 0.0 && total < (out.len() / 3) as f32);
    Ok(())
}

#[test]
fn with_no_ink_the_dots_take_the_photo_colour_brightened_by_at_most_one_over_the_floor() -> Result<(), Error> {
    let red_photo: Vec<f32> = (0..24 * 24).flat_map(|_| [0.8, 0.0, 0.0]).collect();
    let out = run_picture(
        &Params {
            cell: 8.0,
            ..Params::default()
        },
        24,
        24,
        &red_photo,
    )?;
    assert!(
        out.as_chunks::<3>()
            .0
            .iter()
            .all(|pixel| pixel[1] == 0.0 && pixel[2] == 0.0),
        "only red"
    );
    assert!(
        out.as_chunks::<3>().0.iter().any(|pixel| (pixel[0] - 1.0).abs() < 1e-6),
        "a dot at full strength: 0.8 scaled up to 1"
    );
    // a colour whose strongest channel is 0.2 is scaled by 1 / floor: x4 at 0.25, left as it is at 1
    let dark: Vec<f32> = (0..24 * 24).flat_map(|_| [0.2, 0.1, 0.04]).collect();
    for (ink_floor, ink) in [(0.25, [0.8, 0.4, 0.16]), (1.0, [0.2, 0.1, 0.04])] {
        let by_floor = run_picture(
            &Params {
                cell: 8.0,
                ink_floor,
                ..Params::default()
            },
            24,
            24,
            &dark,
        )?;
        let by_ink = run_picture(
            &Params {
                cell: 8.0,
                ink: Some(Color(ink)),
                ..Params::default()
            },
            24,
            24,
            &dark,
        )?;
        assert_close(&by_floor, &by_ink, 1e-6, &format!("floor {ink_floor}"));
        assert!(by_floor.iter().any(|&value| value > ink[0] / 2.0), "dots are there");
    }
    Ok(())
}

#[test]
fn an_upright_screen_repeats_every_cell_and_not_every_half_cell() -> Result<(), Error> {
    let (size, cell) = (48, 12);
    let params = Params {
        cell: cell as f32,
        angle: 0.0,
        ..white_on_black()
    };
    let out = red(&params, size, size, &flat(size, size, 0.3))?;
    for row in 0..size {
        for column in 0..size - cell {
            assert!(
                (out[row * size + column] - out[row * size + column + cell]).abs() < 1e-5,
                "along"
            );
            assert!(
                (out[column * size + row] - out[(column + cell) * size + row]).abs() < 1e-5,
                "down"
            );
        }
    }
    let half = cell / 2;
    let worst = (0..size * (size - half))
        .map(|at| (out[at] - out[at + half]).abs())
        .fold(0.0, f32::max);
    assert!(worst > 0.5, "half a cell along, it is not the same");
    Ok(())
}

#[test]
fn a_quarter_turn_gives_the_same_square_grid_and_other_angles_do_not() -> Result<(), Error> {
    let screen = |angle: f32| {
        red(
            &Params {
                cell: 12.0,
                angle,
                ..white_on_black()
            },
            48,
            48,
            &flat(48, 48, 0.3),
        )
    };
    let mean_gap =
        |one: &[f32], other: &[f32]| one.iter().zip(other).map(|(a, b)| (a - b).abs()).sum::<f32>() / one.len() as f32;
    let (thirty, quarter_later, upright) = (screen(30.0)?, screen(120.0)?, screen(0.0)?);
    assert!(mean_gap(&thirty, &quarter_later) < 5e-3);
    assert!(mean_gap(&thirty, &upright) > 0.1);
    assert_eq!(
        first_difference(&screen(0.0)?, &screen(360.0)?),
        None,
        "a full turn is no turn"
    );
    Ok(())
}

#[test]
fn rows_of_dots_run_the_angle_clockwise_on_screen() -> Result<(), Error> {
    // from a dot, the next one along a row lies at (cell cos a, cell sin a) in (x, y), y pointing down, and not at its
    // mirror image
    let (size, cell) = (100, 20);
    for angle in [20.0_f32, 340.0, 65.0] {
        let params = Params {
            cell: cell as f32,
            angle,
            ..white_on_black()
        };
        let out = red(&params, size, size, &flat(size, size, 0.3))?;
        let bright = |at: usize| out[at] > 0.5;
        // the bright pixel nearest the middle, and the centre of mass of the dot round it (a dot is 15 px across)
        let nearest = (0..size * size)
            .filter(|&at| bright(at))
            .min_by_key(|&at| ((at % size) as i32 - 50).pow(2) + ((at / size) as i32 - 50).pow(2))
            .unwrap_or(0);
        let (px, py) = ((nearest % size) as f32, (nearest / size) as f32);
        let near: Vec<usize> = (0..size * size)
            .filter(|&at| bright(at) && ((at % size) as f32 - px).hypot((at / size) as f32 - py) < 10.0)
            .collect();
        let (cx, cy) = (
            near.iter().map(|&at| (at % size) as f32).sum::<f32>() / near.len() as f32,
            near.iter().map(|&at| (at / size) as f32).sum::<f32>() / near.len() as f32,
        );
        let (cos, sin) = (angle.to_radians().cos(), angle.to_radians().sin());
        let at = |x: f32, y: f32| out[(y.round() as usize) * size + x.round() as usize];
        assert!(
            at(cx + cell as f32 * cos, cy + cell as f32 * sin) > 0.9,
            "angle {angle}: the next dot along the row"
        );
        assert!(
            at(cx + cell as f32 * cos, cy - cell as f32 * sin) < 0.1,
            "angle {angle}: not its mirror image"
        );
    }
    Ok(())
}

#[test]
fn each_dot_stands_for_the_tone_at_its_centre() -> Result<(), Error> {
    // with no tone blur, a picture and a copy of flat cells (each the colour at its cell's centre) print alike
    let (size, cell) = (32, 8);
    let photo = varied(size, size);
    let at_centre = |index: usize| index / cell * cell + cell / 2;
    let cells: Vec<f32> = (0..size * size)
        .flat_map(|pixel| {
            let source = (at_centre(pixel / size) * size + at_centre(pixel % size)) * 3;
            photo[source..source + 3].to_vec()
        })
        .collect();
    let params = Params {
        cell: cell as f32,
        angle: 0.0,
        sigma: 0.0,
        ..Params::default()
    };
    assert_eq!(
        first_difference(
            &run_picture(&params, size, size, &photo)?,
            &run_picture(&params, size, size, &cells)?
        ),
        None
    );
    Ok(())
}

/// A picture `width` wide, black left of `edge` and white from there on, 40 rows.
fn half_and_half(width: usize, edge: usize) -> Vec<f32> {
    (0..40 * width * 3)
        .map(|index| f32::from(index / 3 % width >= edge))
        .collect()
}

/// The mean red in the block of rows and columns of a 40-row, `width`-wide screen.
fn mean_in(out: &[f32], width: usize, rows: std::ops::Range<usize>, columns: std::ops::Range<usize>) -> f32 {
    let values: Vec<f32> = rows
        .flat_map(|row| columns.clone().map(move |column| out[row * width + column]))
        .collect();
    values.iter().sum::<f32>() / values.len() as f32
}

#[test]
fn sigma_and_truncate_set_how_far_tone_spreads_into_neighbouring_cells() -> Result<(), Error> {
    // cells of 10 px, white from column 60 of 120: the black cells at columns 20 to 30 are 30 to 40 px from the white
    let base = Params {
        cell: 10.0,
        angle: 0.0,
        ..white_on_black()
    };
    let picture = half_and_half(120, 60);
    let ink =
        |params: &Params| -> Result<f32, Error> { Ok(mean_in(&red(params, 40, 120, &picture)?, 120, 10..30, 20..30)) };
    assert!(
        ink(&Params {
            sigma: 0.0,
            ..base.clone()
        })?
        .abs()
            < f32::EPSILON,
        "each dot sees only the pixel at its centre"
    );
    assert!(
        ink(&Params {
            sigma: 3.0,
            ..base.clone()
        })? > 0.02,
        "each dot sees the tone 3 cells around it"
    );
    assert!(
        ink(&Params {
            sigma: 3.0,
            truncate: 1.0,
            ..base
        })? < 1e-3, // a kernel this long is filtered by the Fourier transform, whose rounding leaves a speck
        "cut off at 1 x 3 cells = 30 px, the white is out of reach"
    );
    Ok(())
}

#[test]
fn edge_and_constant_value_set_what_lies_beyond_the_picture() -> Result<(), Error> {
    let picture = flat(40, 40, 0.0);
    let ink = |edge, constant_value| -> Result<f32, Error> {
        let params = Params {
            cell: 10.0,
            angle: 0.0,
            sigma: 2.0,
            edge,
            constant_value,
            ..white_on_black()
        };
        Ok(mean_in(&red(&params, 40, 40, &picture)?, 40, 10..30, 0..10))
    };
    assert!(ink(EdgeOrConstant::Reflect, 0.0)?.abs() < f32::EPSILON);
    assert!(
        ink(EdgeOrConstant::Constant, 1.0)? > 0.05,
        "white beyond the edge bleeds in"
    );
    assert!(ink(EdgeOrConstant::Constant, 0.0)?.abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn feather_softens_the_dot_edges_without_changing_the_ink() -> Result<(), Error> {
    let screen = |feather: f32| {
        red(
            &Params {
                cell: 20.0,
                angle: 0.0,
                feather,
                ..white_on_black()
            },
            40,
            40,
            &flat(40, 40, 0.4), // a dot of radius 7 px: a ramp of width f adds pi f^2 / 12 px^2, 2.6 % at f = 4
        )
    };
    let soft_share =
        |out: &[f32]| out.iter().filter(|&&value| value > 0.01 && value < 0.99).count() as f32 / out.len() as f32;
    let (crisp, soft) = (screen(1.0)?, screen(4.0)?);
    assert!(soft_share(&soft) > 2.0 * soft_share(&crisp));
    let mean = |out: &[f32]| out.iter().sum::<f32>() / out.len() as f32;
    assert!((mean(&soft) - mean(&crisp)).abs() < 0.03 * mean(&crisp));
    Ok(())
}

#[test]
fn a_full_turn_of_the_angle_is_no_turn() -> Result<(), Error> {
    let photo = varied(48, 48);
    let screen = |angle| {
        run_picture(
            &Params {
                cell: 12.0,
                angle,
                ..Params::default()
            },
            48,
            48,
            &photo,
        )
    };
    assert_eq!(first_difference(&screen(0.0)?, &screen(360.0)?), None);
    Ok(())
}

/// The picture of one colour, `height` x `width`.
fn colour(height: usize, width: usize, rgb: [f32; 3]) -> Vec<f32> {
    (0..height * width).flat_map(|_| rgb).collect()
}

/// The channel `channel` of every pixel of `out`.
fn channel(out: &[f32], channel: usize) -> Vec<f32> {
    out.as_chunks::<3>().0.iter().map(|pixel| pixel[channel]).collect()
}

// inks no lighter than the paper in any channel: a lighter one cannot be printed on it
const PINK: [f32; 3] = [0.9, 0.3, 0.45];
const BLUE: [f32; 3] = [0.17, 0.35, 0.85];
const PAPER: [f32; 3] = [0.95, 0.93, 0.88];

/// Pink and blue spot inks on a light paper, the screens of the first at 45 degrees.
fn spot() -> Params {
    Params {
        cell: 8.0,
        separation: Separation::Spot,
        inks: vec![Color(PINK), Color(BLUE)],
        paper: Color(PAPER),
        ..Params::default()
    }
}

#[test]
fn a_line_screen_keeps_the_tone_as_the_area_of_its_lines() -> Result<(), Error> {
    // a line as wide as the ink needs: a share `tone` of each row of cells, but for the soft edge
    let params = Params {
        cell: 10.0,
        angle: 0.0,
        shape: Shape::Line,
        ..white_on_black()
    };
    for tone in [0.1, 0.3, 0.6, 0.9] {
        let ink = mean_ink(&params, 40, tone)?;
        assert!((ink - tone).abs() < 0.03, "tone {tone}: {ink}");
    }
    Ok(())
}

#[test]
fn lines_run_along_the_screen_rows_at_the_angle() -> Result<(), Error> {
    // upright, every row of pixels is one value; turned a quarter, every column is
    let screen = |angle: f32| {
        red(
            &Params {
                cell: 10.0,
                angle,
                shape: Shape::Line,
                ..white_on_black()
            },
            40,
            40,
            &flat(40, 40, 0.4),
        )
    };
    let (upright, quarter) = (screen(0.0)?, screen(90.0)?);
    for row in 0..40 {
        for column in 1..40 {
            assert!((upright[row * 40 + column] - upright[row * 40]).abs() < 1e-5, "row");
            assert!((quarter[column * 40 + row] - quarter[row]).abs() < 1e-5, "column");
        }
    }
    assert!(upright.iter().any(|&value| value > 0.9) && upright.iter().any(|&value| value < 0.1));
    Ok(())
}

#[test]
fn cmyk_prints_no_ink_for_paper_black_alone_for_black_and_cyan_alone_for_a_paper_relative_cyan() -> Result<(), Error> {
    let params = Params {
        cell: 8.0,
        angle: 0.0,
        separation: Separation::Cmyk,
        paper: Color(PAPER),
        ..Params::default()
    };
    let print = |photo: [f32; 3]| run_picture(&params, 32, 32, &colour(32, 32, photo));
    let bare = print(PAPER)?;
    assert!(
        bare.as_chunks::<3>().0.iter().all(|pixel| is_pixel(*pixel, PAPER)),
        "paper needs no ink"
    );
    // black: the black ink alone, a dark grey (#231f20) at the middle of a dot, never a colour
    let black = print([0.0; 3])?;
    let (red_of, green_of) = (channel(&black, 0), channel(&black, 1));
    assert!(red_of.iter().copied().fold(1.0, f32::min) < 0.14 + 1e-3);
    assert!(red_of.iter().zip(&green_of).all(|(r, g)| (r - g).abs() < 0.04));
    // cyan relative to the paper: the cyan ink (green #ae = 0.682; its blue #ef is lighter than the paper, so adds none)
    // and no magenta (green 0), yellow (blue 0) or black (both low)
    let cyan = print([0.0, PAPER[1], PAPER[2]])?;
    assert!(
        channel(&cyan, 1).iter().all(|&g| g > 0.682 - 1e-3),
        "no magenta or black"
    );
    assert!(
        channel(&cyan, 2).iter().all(|&b| b > PAPER[2] - 1e-3),
        "no yellow or black"
    );
    assert!(
        channel(&cyan, 0).iter().copied().fold(1.0, f32::min) < 0.2,
        "the cyan dots are there"
    );
    Ok(())
}

#[test]
fn a_spot_ink_the_photo_does_not_need_prints_nothing() -> Result<(), Error> {
    // a photo of the first ink's own colour is that ink alone, so a second ink changes nothing
    let photo = colour(32, 32, PINK);
    let with_blue = run_picture(&spot(), 32, 32, &photo)?;
    let alone = run_picture(
        &Params {
            inks: vec![Color(PINK)],
            ..spot()
        },
        32,
        32,
        &photo,
    )?;
    assert_close(&with_blue, &alone, 1e-5, "pink alone");
    // and the dots are there: some pixel is the ink itself
    assert!(
        with_blue
            .as_chunks::<3>()
            .0
            .iter()
            .any(|pixel| { pixel.iter().zip(PINK).all(|(a, b)| (a - b).abs() < 0.02) })
    );
    Ok(())
}

#[test]
fn offset_zero_is_no_offset_whatever_its_direction() -> Result<(), Error> {
    let photo = varied(40, 40);
    for separation in [Separation::Cmyk, Separation::Spot] {
        let base = Params { separation, ..spot() };
        let turned = Params {
            offset_angle: 77.0,
            ..base.clone()
        };
        assert_eq!(
            first_difference(
                &run_picture(&base, 40, 40, &photo)?,
                &run_picture(&turned, 40, 40, &photo)?
            ),
            None
        );
    }
    Ok(())
}

#[test]
fn the_second_ink_moves_by_the_offset_and_the_first_stays() -> Result<(), Error> {
    // a flat photo of the second ink's colour is that ink alone: its pattern is the unmoved one slid by the offset
    let (size, slide) = (48, 3);
    let register = run_picture(&spot(), size, size, &colour(size, size, BLUE))?;
    let sliding = |angle: f32| {
        run_picture(
            &Params {
                offset: slide as f32,
                offset_angle: angle,
                ..spot()
            },
            size,
            size,
            &colour(size, size, BLUE),
        )
    };
    let right = sliding(0.0)?;
    let down = sliding(90.0)?;
    for row in 0..size - slide {
        for column in 0..size - slide {
            for channel in 0..3 {
                let at = |x: usize, y: usize| (y * size + x) * 3 + channel;
                assert!(
                    (right[at(column + slide, row)] - register[at(column, row)]).abs() < 1e-4,
                    "right"
                );
                assert!(
                    (down[at(column, row + slide)] - register[at(column, row)]).abs() < 1e-4,
                    "down"
                );
            }
        }
    }
    // the first ink does not move
    let first = |offset: f32| run_picture(&Params { offset, ..spot() }, size, size, &colour(size, size, PINK));
    assert_eq!(first_difference(&first(0.0)?, &first(5.0)?), None);
    Ok(())
}

#[test]
fn the_third_ink_moves_twice_as_far_as_the_second() -> Result<(), Error> {
    let (size, slide) = (48, 2);
    let green = [0.2, 0.7, 0.3];
    let params = |offset: f32| Params {
        offset,
        inks: vec![Color(PINK), Color(BLUE), Color(green)],
        ..spot()
    };
    let photo = colour(size, size, green);
    let register = run_picture(&params(0.0), size, size, &photo)?;
    let moved = run_picture(&params(slide as f32), size, size, &photo)?;
    for row in 0..size {
        for column in 0..size - 2 * slide {
            let at = |x: usize| (row * size + x) * 3;
            assert!((moved[at(column + 2 * slide)] - register[at(column)]).abs() < 1e-3);
        }
    }
    Ok(())
}

#[test]
fn the_second_spot_ink_is_screened_thirty_degrees_from_the_first() -> Result<(), Error> {
    // the blue photo is the second ink alone: it prints as a lone blue ink would at the angle thirty degrees on
    let photo = colour(40, 40, BLUE);
    let second = run_picture(&spot(), 40, 40, &photo)?;
    let lone = run_picture(
        &Params {
            angle: 75.0,
            inks: vec![Color(BLUE)],
            ..spot()
        },
        40,
        40,
        &photo,
    )?;
    assert_close(&second, &lone, 1e-4, "second ink");
    Ok(())
}

#[test]
fn the_four_cmyk_screens_each_have_their_own_angle() -> Result<(), Error> {
    // half of each ink alone against the paper (cyan takes away half the red, magenta of the green, yellow of the blue,
    // black of all) makes separate dots; where they print is the mask of that ink's screen
    let params = Params {
        cell: 8.0,
        angle: 0.0,
        separation: Separation::Cmyk,
        paper: Color(PAPER),
        ..Params::default()
    };
    let mask = |photo: [f32; 3], channel_of: usize| -> Result<Vec<bool>, Error> {
        let out = run_picture(&params, 48, 48, &colour(48, 48, photo))?;
        let values = channel(&out, channel_of);
        let lowest = values.iter().copied().fold(1.0, f32::min);
        Ok(values
            .iter()
            .map(|&value| value < f32::midpoint(lowest, PAPER[channel_of]))
            .collect())
    };
    let half = |channel: usize| {
        let mut photo = PAPER;
        photo[channel] *= 0.5;
        photo
    };
    let masks = [
        mask(half(0), 0)?,
        mask(half(1), 1)?,
        mask(half(2), 2)?,
        mask(PAPER.map(|level| level * 0.5), 0)?,
    ];
    for one in 0..4 {
        assert!(
            masks[one].iter().any(|&on| on) && masks[one].iter().any(|&on| !on),
            "screen {one} has dots"
        );
        for other in one + 1..4 {
            let apart = masks[one].iter().zip(&masks[other]).filter(|(a, b)| a != b).count();
            assert!(
                apart > 48 * 48 / 10,
                "screens {one} and {other} differ in {apart} pixels"
            );
        }
    }
    Ok(())
}

#[test]
fn the_offset_ink_reads_the_tone_where_its_dots_are() -> Result<(), Error> {
    // a render with the second ink moved by `slide` is the unmoved render of the picture seen from `slide` on, slid back
    let (size, cell, slide) = (48, 8_i32, 3);
    let blocks = |at: &dyn Fn(usize, usize) -> (usize, usize)| -> Vec<f32> {
        (0..size * size)
            .flat_map(|index| {
                let (x, y) = at(index % size, index / size);
                if (x / 12 + y / 12) % 2 == 0 { BLUE } else { PAPER }
            })
            .collect()
    };
    let params = Params {
        sigma: 0.0,
        cell: cell as f32,
        ..spot()
    };
    for (angle, along) in [(0.0, (slide, 0)), (90.0, (0, slide))] {
        let moved = run_picture(
            &Params {
                offset: slide as f32,
                offset_angle: angle,
                ..params.clone()
            },
            size,
            size,
            &blocks(&|x, y| (x, y)),
        )?;
        let seen_from = blocks(&|x, y| ((x + along.0).min(size - 1), (y + along.1).min(size - 1)));
        let register = run_picture(&params, size, size, &seen_from)?;
        for row in 0..size - slide {
            for column in 0..size - slide {
                for channel in 0..3 {
                    let at = |x: usize, y: usize| (y * size + x) * 3 + channel;
                    assert!(
                        (moved[at(column + along.0, row + along.1)] - register[at(column, row)]).abs() < 1e-3,
                        "angle {angle} at {column}, {row}"
                    );
                }
            }
        }
    }
    Ok(())
}

// ---- rgb ------------------------------------------------------------------------------------------------------------

/// Red, green and blue light on black, `cell` 10, the coverage oracle's settings: no blur, a small feather.
fn rgb() -> Params {
    Params {
        cell: 10.0,
        separation: Separation::Rgb,
        sigma: 0.0,
        feather: 0.1,
        ..Params::default()
    }
}

/// The fraction of pixels of the channel `which` that are lit (above half).
fn lit(out: &[f32], which: usize) -> f32 {
    let values = channel(out, which);
    values.iter().filter(|&&value| value > 0.5).count() as f32 / values.len() as f32
}

#[test]
fn a_flat_red_green_blue_or_white_lights_only_its_own_screens() -> Result<(), Error> {
    let size = 60;
    let print = |colour_of: [f32; 3]| run_picture(&rgb(), size, size, &colour(size, size, colour_of));
    for (name, photo, own) in [
        ("red", [1.0, 0.0, 0.0], [true, false, false]),
        ("green", [0.0, 1.0, 0.0], [false, true, false]),
        ("blue", [0.0, 0.0, 1.0], [false, false, true]),
        ("white", [1.0; 3], [true; 3]),
    ] {
        let out = print(photo)?;
        for (which, &is_own) in own.iter().enumerate() {
            let values = channel(&out, which);
            if is_own {
                // a full dot fills its cell but for the soft edge
                assert!(
                    lit(&out, which) > 0.97,
                    "{name}: channel {which} lit {}",
                    lit(&out, which)
                );
            } else {
                assert!(values.iter().all(|&value| value == 0.0), "{name}: channel {which}");
            }
        }
    }
    // a channel is its own screen whatever the others show: white's red is red's red
    let (red_only, white) = (print([1.0, 0.0, 0.0])?, print([1.0; 3])?);
    assert_eq!(first_difference(&channel(&red_only, 0), &channel(&white, 0)), None);
    // and the paper is always black: it is ignored
    let on_white = Params {
        paper: Color([1.0; 3]),
        ..rgb()
    };
    assert_eq!(
        first_difference(
            &run_picture(&on_white, size, size, &colour(size, size, [0.2, 0.5, 0.8]))?,
            &run_picture(&rgb(), size, size, &colour(size, size, [0.2, 0.5, 0.8]))?
        ),
        None
    );
    Ok(())
}

#[test]
fn each_channel_lights_the_fraction_of_the_picture_its_value_asks() -> Result<(), Error> {
    // a channel of value v covers v of the picture; the screens are turned, so the cells at the border are cut: a
    // relative bound of 8 % on a picture of 20 x 20 cells
    let size = 200;
    for tone in [0.1, 0.25, 0.4, 0.7] {
        let out = run_picture(&rgb(), size, size, &colour(size, size, [tone, 1.0 - tone, tone * 0.5]))?;
        for (which, value) in [tone, 1.0 - tone, tone * 0.5].into_iter().enumerate() {
            let mean = channel(&out, which).iter().sum::<f32>() / (size * size) as f32;
            assert!(
                (mean / value - 1.0).abs() < 0.08,
                "tone {value}, channel {which}: {mean} against {value}"
            );
        }
    }
    Ok(())
}

#[test]
fn the_red_green_and_blue_screens_are_a_single_ink_screen_at_15_45_and_75_degrees() -> Result<(), Error> {
    // a grey prints the same tone in all three channels: each is the one-ink white-on-black screen at its own angle
    let size = 80;
    let grey = colour(size, size, [0.3; 3]);
    let out = run_picture(&Params { angle: 0.0, ..rgb() }, size, size, &grey)?;
    for (which, angle) in [15.0, 45.0, 75.0].into_iter().enumerate() {
        let alone = Params {
            cell: 10.0,
            angle,
            sigma: 0.0,
            feather: 0.1,
            ..white_on_black()
        };
        let want = red(&alone, size, size, &grey)?;
        assert_eq!(first_difference(&channel(&out, which), &want), None, "channel {which}");
    }
    Ok(())
}

// A recipe's `cell` is a length in px, scaled to the picture's size as a float: the screen's pitch is what the recipe asks
// for, never a whole count. These go through `Recipe` so they hold for the field whatever its type.

/// The red channel of a one-ink black-on-white halftone of `recipe_cell` (a fractional length) over `values`, rendered at
/// `size` (the longest side the recipe's px are written for 480 of).
fn through_recipe(
    recipe_cell: f64,
    tone: f32,
    (height, width): (usize, usize),
    size: Option<u32>,
) -> Result<(Vec<f32>, usize), Box<dyn std::error::Error>> {
    use bullterrier_image::{Options, Photo, Picture, Presets, Recipe, Step};
    let step = Step::new("halftone")
        .param("cell", recipe_cell)
        .param("angle", 0.0)
        .param("sigma", 0.0)
        .param("feather", 1.0)
        .param("ink", "#000000")
        .param("paper", "#ffffff");
    let values = flat(height, width, 1.0 - tone);
    let options = size.map_or_else(Options::default, |size| Options::default().with_size(size));
    let image =
        Photo::new(Picture::new(height, width, &values)?).render(&Recipe::new([step]), &Presets::new(), options)?;
    let wide = image.width();
    let red = image
        .into_values()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| pixel[0])
        .collect();
    Ok((red, wide))
}

#[test]
fn a_fractional_cell_keeps_the_tone_within_what_its_soft_edge_adds() -> Result<(), Box<dyn std::error::Error>> {
    // The ink is the dots' area plus what the one pixel soft edge adds: pi f^2 / 12 px^2 a dot (f = 1), a share
    // pi / (12 cell^2) of its cell. Allow twice that, as a dot of a pixel or two is not a clean disc, and 0.01 for the
    // partial cells at the picture's edge (a picture of 480 px at size 480, so the cell is the recipe's).
    // Only the ramp share is derived (0.036 at cell 2.7); the factor 2 and the 0.01 are an empirical envelope, not a
    // derivation: at 2.7 px a dot is a few pixels, not a disc, and dots at 0.9 overlap and clip (measured error up to
    // 0.076 against the bound 0.082). The bound at 13.3 (0.013) is the tight one; the cell 2.7 case guards against a
    // gross error (a rounded pitch is off by 0.1 or more) and not against a small one.
    for cell in [2.7_f64, 13.3] {
        let bound = 2.0 * std::f64::consts::PI / (12.0 * cell * cell) + 0.01;
        for tone in [0.1_f32, 0.25, 0.5, 0.75, 0.9] {
            let (red, _) = through_recipe(cell, tone, (480, 480), Some(480))?;
            let ink = 1.0 - f64::from(red.iter().sum::<f32>()) / red.len() as f64;
            assert!(
                (ink - f64::from(tone)).abs() < bound,
                "cell {cell} tone {tone}: ink {ink} (bound {bound})"
            );
        }
    }
    Ok(())
}

/// The mean distance between the centres of the dots along the row of the screen's dots nearest `row`, in px.
fn dot_pitch(red: &[f32], width: usize, row: usize) -> f32 {
    let line = &red[row * width..(row + 1) * width];
    let mut centres = Vec::new();
    let mut start = None;
    for (column, value) in line.iter().enumerate() {
        match (start, *value < 0.5) {
            (None, true) => start = Some(column),
            (Some(from), false) => {
                centres.push((from + column - 1) as f32 / 2.0);
                start = None;
            }
            _ => {}
        }
    }
    (centres[centres.len() - 1] - centres[0]) / (centres.len() - 1) as f32
}

#[test]
fn a_recipe_screen_scales_with_the_picture_without_jumps() -> Result<(), Box<dyn std::error::Error>> {
    // `cell` px at 480 is `cell x size / 480` px at `size`: a whole cell of 5 is 13.33 at 1280, not 13
    for cell in [5.0_f64, 7.0] {
        let (small, small_width) = through_recipe(cell, 0.3, (24, 480), Some(480))?;
        let (large, large_width) = through_recipe(cell, 0.3, (24, 480), Some(1280))?;
        // a row through the middle of a row of dots: the centres of the dots in row 2 of the screen
        let row = |pitch: f32| (pitch * 2.5).round() as usize;
        let at_480 = dot_pitch(&small, small_width, row(cell as f32));
        let at_1280 = dot_pitch(&large, large_width, row(cell as f32 * 1280.0 / 480.0));
        assert!(
            (at_480 - cell as f32).abs() < 0.03,
            "cell {cell}: pitch {at_480} at 480"
        );
        let want = cell as f32 * 1280.0 / 480.0;
        assert!(
            (at_1280 - want).abs() < 0.03,
            "cell {cell}: pitch {at_1280} at 1280, wanted {want}"
        );
    }
    Ok(())
}

#[test]
fn a_fractional_cells_dot_stands_for_the_pixel_nearest_its_centre() -> Result<(), Error> {
    // cells of 7.5 px: the cell of column x is floor(x / 7.5) and its centre is at (k + 0.5) x 7.5, rounded to a pixel
    let (size, cell) = (28, 7.5_f64);
    let photo = varied(size, size);
    let at_centre = |index: usize| (((index as f64 / cell).floor() + 0.5) * cell).round() as usize;
    let cells: Vec<f32> = (0..size * size)
        .flat_map(|pixel| {
            let source = (at_centre(pixel / size) * size + at_centre(pixel % size)) * 3;
            photo[source..source + 3].to_vec()
        })
        .collect();
    let params = Params {
        cell: cell as f32,
        angle: 0.0,
        sigma: 0.0,
        ..Params::default()
    };
    assert_eq!(
        first_difference(
            &run_picture(&params, size, size, &photo)?,
            &run_picture(&params, size, size, &cells)?
        ),
        None
    );
    Ok(())
}

#[test]
fn the_tone_blur_is_sigma_times_the_fractional_cell_in_px() -> Result<(), Error> {
    use bullterrier_image::effects::gaussian_blur;
    // a halftone blurring by sigma cells equals a gaussian blur of sigma x cell px, then a halftone that does not blur
    let (height, width) = (24, 200);
    let photo = varied(height, width);
    let (cell, sigma) = (10.4_f32, 0.5_f32);
    let blurred = run_picture(
        &gaussian_blur::Params {
            radius: cell * sigma,
            truncate: 4.0,
            ..gaussian_blur::Params::default()
        },
        height,
        width,
        &photo,
    )?;
    let screen = Params {
        cell,
        angle: 0.0,
        ..white_on_black()
    };
    let whole = run_picture(
        &Params {
            sigma,
            truncate: 4.0,
            ..screen.clone()
        },
        height,
        width,
        &photo,
    )?;
    let after = run_picture(&Params { sigma: 0.0, ..screen }, height, width, &blurred)?;
    assert_close(&whole, &after, 1e-5, "blur of sigma x cell px");
    Ok(())
}

#[test]
fn the_ink_does_not_jump_when_the_cell_passes_a_whole_or_a_half_number() -> Result<(), Box<dyn std::error::Error>> {
    // The edge ramp and the fade-in of a tiny dot are lengths in px (fractions of the pitch): a cell a hair either side of
    // 2.5 or 3 changes the picture by a hair. A pitch rounded or ceiled there is a different ramp, a visible step in ink.
    for tone in [0.02_f32, 0.5] {
        for (below, above) in [(2.499_f64, 2.501_f64), (2.999, 3.001)] {
            let ink = |cell| -> Result<f64, Box<dyn std::error::Error>> {
                let (red, _) = through_recipe(cell, tone, (480, 480), Some(480))?;
                Ok(1.0 - f64::from(red.iter().sum::<f32>()) / red.len() as f64)
            };
            let (a, b) = (ink(below)?, ink(above)?);
            assert!(
                (a - b).abs() < 0.002,
                "tone {tone}: ink {a} at cell {below}, {b} at {above}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_whole_number_cell_written_in_a_recipe_file_still_loads() -> Result<(), Box<dyn std::error::Error>> {
    use bullterrier_image::{Options, Photo, Picture, Presets, Recipe};
    let recipe = Recipe::from_toml("[[step]]\neffect = \"halftone\"\ncell = 14\n")?;
    let values = flat(24, 24, 0.5);
    Photo::new(Picture::new(24, 24, &values)?).render(&recipe, &Presets::new(), Options::default())?;
    Ok(())
}

// ---- dot shapes -------------------------------------------------------------------------------------------------------

/// The screens of every new shape, each as the coverage oracle runs it (the ellipse at aspect 2 and the matrix at a picture
/// of its own, see `coverage_of_a_matrix`).
fn new_shapes() -> Vec<(&'static str, Params)> {
    vec![
        ("square", oracle(Shape::Square)),
        ("diamond", oracle(Shape::Diamond)),
        ("euclidean", oracle(Shape::Euclidean)),
        (
            "ellipse 2",
            Params {
                aspect: 2.0,
                ..oracle(Shape::Ellipse)
            },
        ),
        (
            "ellipse 4",
            Params {
                aspect: 4.0,
                ..oracle(Shape::Ellipse)
            },
        ),
    ]
}

#[test]
fn ink_grows_with_tone_in_every_new_shape() -> Result<(), Error> {
    for (name, params) in new_shapes() {
        let mut last = -1.0;
        for tone in [0.0, 0.001, 0.01, 0.05, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let ink = coverage_with(&params, tone)?;
            assert!(ink > last, "{name} {tone}: {ink} after {last}");
            last = ink;
        }
    }
    Ok(())
}

#[test]
fn every_new_shape_keeps_the_tone() -> Result<(), Error> {
    // A straight edge keeps its area (the ramp is centred on it and a pixel row samples it at whole pixels). A curve gains
    // pi f^2 / 12 px^2 a dot at feather f = 1 where the ramp is a pixel wide (see `a_dot_prints_about_its_own_area_however_small`),
    // 0.0013 of a cell of 20 px; a value that changes only 0.7 a cell at the edge (the diamond's diagonals, the cosine
    // screen's) widens the ramp by 1 / 0.7 and so doubles that gain, 0.0026. The level is read off a grid of 65536 places, within
    // 0.0003 of tone. About 0.003 together, and the cosine screen's flat saddles at the tips of its diamond, where the value does
    // not change and the ramp spreads, add a little more (measured 0.0041 at most): the bound is 0.005.
    for (name, params) in new_shapes() {
        for tone in [0.1, 0.25, 0.5, 0.75, 0.9] {
            let ink = coverage_with(&params, tone)?;
            assert!((ink - tone).abs() < 0.005, "{name} tone {tone}: ink {ink}");
        }
    }
    Ok(())
}

#[test]
fn a_square_at_full_tone_fills_its_cell_and_at_a_quarter_is_a_centred_half_cell_square() -> Result<(), Error> {
    let params = oracle(Shape::Square);
    assert!(ink_map(&params, 1.0)?.iter().all(|&ink| ink > 1.0 - 1e-6));
    // side sqrt(tone) cells: 10 px of a cell of 20 (pixels 5..15), its edges on whole pixels
    let ink = ink_map(&params, 0.25)?;
    for (x, y) in [(10, 10), (6, 6), (14, 14), (6, 14), (14, 6), (26, 26)] {
        assert!(ink[y * 80 + x] > 0.99, "({x}, {y}) is inside");
    }
    for (x, y) in [(0, 0), (4, 10), (10, 4), (16, 10), (10, 16), (19, 19), (20, 20)] {
        assert!(ink[y * 80 + x] < 0.01, "({x}, {y}) is outside");
    }
    Ok(())
}

#[test]
fn a_diamond_at_half_tone_touches_its_neighbours_at_the_middle_of_the_cell_sides() -> Result<(), Error> {
    // |du| + |dv| < 1 / 2 is a diamond of area 1 / 2 whose tips are the middles of the cell's sides: the diamond at (x, y)
    // = (10, 10) has its right tip at (20, 10) where the next diamond's left tip is
    let params = oracle(Shape::Diamond);
    let touching = ink_map(&params, 0.5)?;
    let apart = ink_map(&params, 0.4)?;
    let past = ink_map(&params, 0.6)?;
    let at = |map: &[f32], x: usize, y: usize| map[y * 80 + x];
    // the tip pixels on the side between two cells (x = 19 is in the left cell, 20 in the right), and a corner
    assert!(
        at(&touching, 19, 10) > 0.9 && at(&touching, 20, 10) > 0.4,
        "the tips meet"
    );
    assert!(at(&apart, 20, 10) < 0.01, "a smaller diamond leaves a gap");
    assert!(
        at(&past, 20, 10) > 0.99 && at(&past, 19, 10) > 0.99,
        "a larger one overlaps"
    );
    assert!(
        at(&touching, 20, 20) < 0.01 && at(&touching, 19, 19) < 0.01,
        "the corners stay paper"
    );
    // and the centre is ink, the square's own side middle a little further out is the paper between diamonds
    assert!(at(&touching, 10, 10) > 0.99);
    Ok(())
}

#[test]
fn an_ellipse_of_aspect_one_is_the_round_dot() -> Result<(), Error> {
    let round = oracle(Shape::Dot);
    let ellipse = Params {
        aspect: 1.0,
        ..oracle(Shape::Ellipse)
    };
    for tone in [0.02, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
        let (want, got) = (ink_map(&round, tone)?, ink_map(&ellipse, tone)?);
        // a level read off a grid differs from the circle's by 0.0003 of tone, 0.0003 / (2 pi r) cells at a dot of radius
        // r cells (the smallest here, 0.08): 0.0006 cells, 0.012 px, a coverage of that much
        let worst = want.iter().zip(&got).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(worst < 0.01, "tone {tone}: pixels differ by {worst}");
    }
    Ok(())
}

#[test]
fn an_ellipse_is_aspect_times_as_long_along_the_rows_as_across_them() -> Result<(), Error> {
    for aspect in [2.0_f32, 4.0] {
        let params = Params {
            aspect,
            ..oracle(Shape::Ellipse)
        };
        // the centre cell's ink along its middle row and its middle column (pixels 10, centred on the dot at 10)
        let ink = ink_map(&params, 0.05)?;
        let along: f32 = (0..20).map(|x| ink[10 * 80 + x]).sum();
        let across: f32 = (0..20).map(|y| ink[y * 80 + 10]).sum();
        // axes r sqrt a and r / sqrt a; the ramp adds about a pixel to each length, so allow 1.2 px on the ratio's terms
        let want = across * aspect;
        assert!(
            (along - want).abs() < 1.5 * aspect,
            "aspect {aspect}: {along} along, {across} across"
        );
        assert!(along > across * 1.5, "aspect {aspect}");
    }
    Ok(())
}

#[test]
fn a_euclidean_dot_is_round_in_highlights_half_the_cell_at_half_and_mirrors_about_half() -> Result<(), Error> {
    let params = oracle(Shape::Euclidean);
    // highlights: round but for the cosine's fourth-order squareness (the radius differs by about 10 % between axis and
    // diagonal at this size), of the radius that holds the tone (0.1: 3.57 px of 20); inside 0.85 of it all ink, past 1.25 none
    let ink = ink_map(&params, 0.1)?;
    let radius = (0.1_f32 / PI).sqrt() * 20.0;
    for y in 0..20 {
        for x in 0..20 {
            let distance = ((x as f32 - 10.0).powi(2) + (y as f32 - 10.0).powi(2)).sqrt();
            if distance < 0.85 * radius {
                assert!(ink[y * 80 + x] > 0.99, "({x}, {y}) inside");
            } else if distance > 1.25 * radius {
                assert!(ink[y * 80 + x] < 0.01, "({x}, {y}) outside");
            }
        }
    }
    // the ink lights half at 50 %
    assert!((coverage_with(&params, 0.5)? - 0.5).abs() < 0.005);
    // swapping ink and paper about 50 %: the picture of tone 1 - t is the paper of tone t, half a cell on in both ways
    for tone in [0.1, 0.3, 0.5] {
        let (a, b) = (ink_map(&params, tone)?, ink_map(&params, 1.0 - tone)?);
        for y in 0..70 {
            for x in 0..70 {
                let (here, there) = (a[y * 80 + x], 1.0 - b[(y + 10) * 80 + x + 10]);
                assert!(
                    (here - there).abs() < 0.01,
                    "tone {tone} ({x}, {y}): {here} against {there}"
                );
            }
        }
    }
    Ok(())
}

/// The round dot's spot value at each pixel of one cell of `side` px: the picture whose ranking is the dot's, the
/// centre brightest (the brightest cells are inked first).
fn dot_matrix(side: usize) -> Result<Image, Error> {
    let greys: Vec<f32> = (0..side * side)
        .flat_map(|index| {
            let (x, y) = (
                (index % side) as f32 / side as f32 - 0.5,
                (index / side) as f32 / side as f32 - 0.5,
            );
            let distance = (x * x + y * y).sqrt();
            [1.0 - distance; 3]
        })
        .collect();
    Image::new(side, side, &greys)
}

/// The ink map of the oracle screen with shape 'matrix' and the cell picture `matrix`.
fn matrix_ink(matrix: &Image, tone: f32) -> Result<Vec<f32>, Error> {
    let params = Params {
        matrix: PictureName::new("m"),
        ..oracle(Shape::Matrix)
    };
    let values = flat(80, 80, 1.0 - tone);
    let out = apply(
        "halftone",
        &Picture::new(80, 80, &values)?,
        &Value::Object(params.values()),
        &Inputs::default().with_image("matrix", matrix),
    )?;
    Ok(out
        .into_values()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| 1.0 - pixel[0])
        .collect())
}

#[test]
fn a_matrix_of_the_round_dots_values_reproduces_the_round_dot() -> Result<(), Error> {
    // a picture of 20 x 20, one place a pixel of the cell, ranks the same pixels as the dot's own distance does; the dot
    // inks the places nearer its centre than the radius, the matrix its k nearest of 400 (ties in reading order), and
    // its soft edge is a ramp of 1 / 400 of tone a rank, 0.05 of a pixel's coverage: the two differ at the edge pixels
    // alone (a ring about 2 pi r px, up to half a coverage each) and the tone each keeps is within 1 / 400
    let matrix = dot_matrix(20)?;
    let round = oracle(Shape::Dot);
    for tone in [0.1, 0.25, 0.5, 0.75] {
        let (want, got) = (ink_map(&round, tone)?, matrix_ink(&matrix, tone)?);
        let mean = |map: &[f32]| map.iter().sum::<f32>() / map.len() as f32;
        assert!(
            (mean(&want) - mean(&got)).abs() < 0.005,
            "tone {tone}: {} against {}",
            mean(&want),
            mean(&got)
        );
        let ring = 2.0 * PI * (tone / PI).sqrt() * 20.0; // px of edge a cell
        let bound = 0.5 * ring / 400.0; // half a coverage each, at most
        let mean_difference = want.iter().zip(&got).map(|(a, b)| (a - b).abs()).sum::<f32>() / want.len() as f32;
        assert!(
            mean_difference < bound,
            "tone {tone}: pixels differ by {mean_difference} (bound {bound})"
        );
    }
    Ok(())
}

#[test]
fn a_matrix_inks_a_tone_of_k_of_its_n_places_brightest_first_with_ties_in_reading_order() -> Result<(), Error> {
    // 2 x 2 places of a cell of 20 px: greys 0.2 0.9 / 0.5 0.5 rank (bright first) top right, then the two 0.5 in reading
    // order (bottom left, bottom right), then the top left; a flat tone of k / 4 inks k places, whole blocks of 10 x 10 px
    // but for the soft edge of 0.5 / 4 * 20 = 2.5 of coverage's ramp, so the middle pixel of each block is whole
    let matrix = Image::new(
        2,
        2,
        &[0.2, 0.9, 0.5, 0.5].iter().flat_map(|&g| [g; 3]).collect::<Vec<_>>(),
    )?;
    let block = |ink: &[f32], column: usize, row: usize| ink[(row * 10 + 5) * 80 + column * 10 + 5];
    for (tone, inked) in [
        (0.25, [[0, 1], [0, 0]]),
        (0.5, [[0, 1], [1, 0]]),
        (0.75, [[0, 1], [1, 1]]),
        (1.0, [[1, 1], [1, 1]]),
    ] {
        let ink = matrix_ink(&matrix, tone)?;
        for (row, line) in inked.iter().enumerate() {
            for (column, &want) in line.iter().enumerate() {
                assert!(
                    (block(&ink, column, row) - want as f32).abs() < 0.01,
                    "tone {tone} place ({column}, {row})"
                );
            }
        }
    }
    Ok(())
}

// ---- the refusals of shape 'matrix' ----------------------------------------------------------------------------------

/// `shape` 'matrix' of the picture `matrix` (none: not given) run as a recipe.
fn matrix_run(matrix: Option<&Image>) -> Result<(), Error> {
    use bullterrier_image::{Options, Photo, Presets, Recipe, Step};
    let step = Step::new("halftone").param("shape", "matrix");
    let step = if matrix.is_some() {
        step.param("matrix", "m")
    } else {
        step
    };
    let values = flat(20, 20, 0.5);
    let mut photo = Photo::new(Picture::new(20, 20, &values)?);
    if let Some(matrix) = matrix {
        photo = photo.with_image("m", std::sync::Arc::new(matrix.clone()));
    }
    photo
        .render(&Recipe::new([step]), &Presets::new(), Options::default())
        .map(drop)
}

#[test]
fn shape_matrix_with_no_picture_or_one_too_large_is_refused_naming_the_matrix() -> Result<(), Error> {
    let none = matrix_run(None);
    assert!(
        matches!(&none, Err(error) if error.to_string().contains("`matrix`") && error.to_string().contains("halftone")),
        "{none:?}"
    );
    for (rows, columns) in [(257, 1), (1, 257)] {
        let big = Image::new(rows, columns, &vec![0.4; rows * columns * 3])?;
        let past = matrix_run(Some(&big));
        assert!(
            matches!(&past, Err(error) if error.to_string().contains("`matrix`") && error.to_string().contains("257")),
            "{rows} x {columns}: {past:?}"
        );
    }
    let most = Image::new(256, 256, &vec![0.4; 256 * 256 * 3])?;
    assert!(matrix_run(Some(&most)).is_ok());
    // another shape ignores a `matrix` it was not given
    assert!(red(&oracle(Shape::Square), 20, 20, &flat(20, 20, 0.5)).is_ok());
    Ok(())
}

#[test]
fn the_ink_of_every_shape_reaches_a_full_tone_without_a_jump() -> Result<(), Error> {
    // the ramp is centred on a mark's edge, so where the paper between marks is narrower than the ramp it fades out: a tone
    // a hair under 1 prints (all but) fully, as 1 does
    for shape in [Shape::Dot, Shape::Line, Shape::Square, Shape::Diamond, Shape::Euclidean] {
        for (cell, angle) in [(20.0, 0.0), (14.0, 45.0), (7.0, 17.0)] {
            let params = Params {
                cell,
                angle,
                ..oracle(shape)
            };
            let (almost, full) = (coverage_with(&params, 0.9999)?, coverage_with(&params, 1.0)?);
            assert!(full > 1.0 - 1e-6, "{shape:?} {cell} {angle}: tone 1 prints {full}");
            assert!(
                full - almost < 0.002,
                "{shape:?} {cell} {angle}: {almost} at 0.9999, {full} at 1"
            );
        }
    }
    Ok(())
}
