//! `halftone` with `separation`: CMYK, spot inks and RGB light, their offsets and screen angles, and the tone each
//! separation keeps. The oracles are Murray-Davies mixes worked by hand and comparisons between renders.

#![expect(clippy::cast_precision_loss, reason = "small picture sizes")]

use bullterrier_image::Error;
use bullterrier_image::effects::halftone::{Params, Screen, Separation};
use bullterrier_image::params::Color;

use crate::common::compare::{assert_close, first_difference};
use crate::common::halftone::{mean_colour, mixed};
use crate::common::pictures::varied;
use crate::common::pixels::{is_pixel, run_picture};
use crate::halftone::*;

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
pub(crate) fn rgb() -> Params {
    Params {
        cell: 10.0,
        separation: Separation::Rgb,
        sigma: 0.0,
        feather: 0.1,
        ..Params::default()
    }
}

/// The fraction of pixels of the channel `which` that are lit (above half).
pub(crate) fn lit(out: &[f32], which: usize) -> f32 {
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

// ---- separations keep the tone ------------------------------------------------------------------------------------------

fn separated(separation: Separation, inks: &[[f32; 3]]) -> Params {
    Params {
        cell: 12.0,
        sigma: 0.0,
        feather: 1.0,
        separation,
        inks: inks.iter().map(|&ink| Color(ink)).collect(),
        paper: WHITE,
        ..Params::default()
    }
}

fn assert_mean(params: &Params, photo: [f32; 3], want: [f32; 3], tolerance: f32) -> Result<(), Error> {
    let got = mean_colour(params, photo)?;
    assert!(
        (0..3).all(|c| (got[c] - want[c]).abs() <= tolerance),
        "photo {photo:?}: printed {got:?}, wanted {want:?} within {tolerance}"
    );
    Ok(())
}

// The bound: a dot's area is its ink amount within 0.01 (a cell's soft edge and pixel grid, see the shape tests), and a
// channel of the mix moves by at most the sum of the areas' errors, so 0.02 for one or two inks.

#[test]
fn one_black_spot_ink_prints_a_grey_at_its_own_level() -> Result<(), Error> {
    let params = separated(Separation::Spot, &[[0.0; 3]]);
    for grey in [0.2, 0.5, 0.8] {
        assert_mean(&params, [grey; 3], [grey; 3], 0.02)?;
    }
    Ok(())
}

#[test]
fn two_spot_inks_print_the_colours_they_can_mix_at_those_colours() -> Result<(), Error> {
    let inks = [PINK, BLUE];
    // concentric screens: `halftone_concentric.rs`
    for screen in [Screen::Grid, Screen::Wave] {
        let params = Params {
            screen,
            ..separated(Separation::Spot, &inks)
        };
        for areas in [[0.3, 0.4], [0.6, 0.1], [0.2, 0.7], [0.5, 0.5]] {
            let photo = mixed([1.0; 3], &inks, &areas);
            assert_mean(&params, photo, photo, 0.02)?;
        }
    }
    Ok(())
}

#[test]
fn cmyk_prints_greys_at_their_level_in_the_shadows_too() -> Result<(), Error> {
    let params = separated(Separation::Cmyk, &[]);
    for grey in [0.2, 0.5, 0.8] {
        assert_mean(&params, [grey; 3], [grey; 3], 0.03)?;
    }
    Ok(())
}

#[test]
fn subtractive_separations_refuse_a_dark_paper_naming_paper_and_print_on_a_light_one() {
    let photo = flat(16, 16, 0.5);
    for separation in [Separation::Cmyk, Separation::Spot] {
        let on = |paper: [f32; 3]| {
            run_picture(
                &Params {
                    separation,
                    paper: Color(paper),
                    ..spot()
                },
                16,
                16,
                &photo,
            )
        };
        let refused = on([0.0; 3]);
        assert!(
            matches!(&refused, Err(error) if error.to_string().contains("paper")),
            "{separation:?}: {refused:?}"
        );
        assert!(on(PAPER).is_ok(), "{separation:?}");
    }
    // rgb adds light on black and ignores the paper
    let rgb = Params {
        separation: Separation::Rgb,
        paper: Color([0.0; 3]),
        ..spot()
    };
    assert!(run_picture(&rgb, 16, 16, &photo).is_ok());
}
