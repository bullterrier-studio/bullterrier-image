//! `halftone`: tone becomes dot size. The oracles are the screen's geometry (a dot's area is its tone times the circle
//! around its square cell, so the ink per tone of separate dots is pi / 2 of a cell's area), hand-worked colours, and
//! comparisons between renders that must be alike or must differ.

#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "small picture sizes and indices"
)]

use std::f32::consts::PI;

use bullterrier_image::effects::halftone::Params;
use bullterrier_image::params::Color;
use bullterrier_image::{EdgeOrConstant, Error};

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

#[test]
fn ink_grows_with_tone_and_is_pi_over_two_of_the_cell_per_tone_while_the_dots_stay_apart() -> Result<(), Error> {
    // angle 0 and a size of whole cells: every dot is alike, so the mean over the picture is that of one cell
    let params = Params {
        cell: 10,
        angle: 0.0,
        ..white_on_black()
    };
    let tones = [0.0, 0.001, 0.01, 0.05, 0.1, 0.25, 0.45, 0.8, 1.0];
    let inks: Vec<f32> = tones
        .iter()
        .map(|&tone| mean_ink(&params, 40, tone))
        .collect::<Result<_, _>>()?;
    assert!(inks[0].abs() < f32::EPSILON);
    assert!(inks.windows(2).all(|pair| pair[0] < pair[1]), "{inks:?}");
    // full tone is the circle around the square cell: pi / 2 of its area, so dots touch at half tone and fill the cell
    for tone in [0.1, 0.25, 0.45] {
        let ink = mean_ink(&params, 40, tone)?;
        assert!(
            (ink / tone - PI / 2.0).abs() < 0.04 * PI / 2.0,
            "tone {tone}: {}",
            ink / tone
        );
    }
    Ok(())
}

#[test]
fn a_dot_prints_about_its_own_area_however_small() -> Result<(), Error> {
    // a soft edge one pixel wide adds at most pi / 12 px^2 to a dot (a linear ramp of width f on a circle's edge adds pi f^2 / 12);
    // a dot of radius r has the tone 2 (r / cell)^2
    let cell = 20;
    let params = Params {
        cell: cell as i32,
        angle: 0.0,
        ..white_on_black()
    };
    for radius in [0.1_f32, 0.25, 0.4, 0.5, 1.0, 2.0, 3.0] {
        let tone = 2.0 * (radius / cell as f32).powi(2);
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
        cell: 6,
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
            cell: 8,
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
                cell: 8,
                ink_floor,
                ..Params::default()
            },
            24,
            24,
            &dark,
        )?;
        let by_ink = run_picture(
            &Params {
                cell: 8,
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
        cell: cell as i32,
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
                cell: 12,
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
            cell,
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
        cell: cell as i32,
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
        cell: 10,
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
            cell: 10,
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
                cell: 20,
                angle: 0.0,
                feather,
                ..white_on_black()
            },
            40,
            40,
            &flat(40, 40, 0.25),
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
