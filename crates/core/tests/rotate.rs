//! A recipe's `angle`, `fit` and `resample`, through the public API alone: the photo and its maps turned about their
//! centre, clockwise for a positive angle, before the crop and the border.

use bullterrier_image::frame::{Anchor, Fit, Flip, Resample};
use bullterrier_image::params::Color;
use bullterrier_image::{Error, Frame, Layer, Options, Photo, Picture, Presets, Recipe, Stage, Step};
use proptest::prelude::*;

/// What a render of a frame made of a scene: the picture, and the subject map as the steps were handed it (empty when
/// the recipe used none).
struct Turned {
    height: usize,
    width: usize,
    values: Vec<f32>,
    subject: Vec<f32>,
}

/// The pixel at `row`, `column` of the test scene: a different value in every channel and place (up to 200 columns).
fn px(row: usize, column: usize) -> [f32; 3] {
    let value = f32::from(u16::try_from(row * 200 + column).unwrap_or(0)) / 40000.0;
    [value, value + 0.0001, value + 0.0002]
}

/// The values of a `height` x `width` picture whose pixel at `row`, `column` is `make(row, column)`.
fn picture_of(height: usize, width: usize, make: impl Fn(usize, usize) -> [f32; 3]) -> Vec<f32> {
    let mut values = Vec::with_capacity(height * width * 3);
    for row in 0..height {
        for column in 0..width {
            values.extend(make(row, column));
        }
    }
    values
}

/// `frame` rendered on the `height` x `width` picture `values` (one step that changes nothing), with `subject` as the
/// subject map if there is one, which a trace then shows as the map the frame made.
fn turn(height: usize, width: usize, values: &[f32], frame: Frame, subject: Option<Layer>) -> Result<Turned, Error> {
    let mut photo = Photo::new(Picture::new(height, width, values)?);
    let mut step = Step::new("grade").opacity(0.0);
    if let Some(subject) = subject {
        photo = photo.with_map("subject", subject)?;
        step = step.mask("subject");
    }
    let mut recipe = Recipe::new([step]);
    recipe.frame = frame;
    let stages = photo.trace(&recipe, &Presets::new(), Options::default())?;
    let mut turned = Turned {
        height: 0,
        width: 0,
        values: Vec::new(),
        subject: Vec::new(),
    };
    for stage in stages {
        match stage {
            Stage::Input(image) => {
                (turned.height, turned.width) = (image.height(), image.width());
                turned.values = image.into_values();
            }
            Stage::Subject(layer) => turned.subject = layer.values().to_vec(),
            _ => {}
        }
    }
    Ok(turned)
}

/// The scene of `height` x `width` pixels (`px`) turned by `angle` degrees.
fn scene_turned(height: usize, width: usize, angle: f64, fit: Fit, resample: Resample) -> Result<Turned, Error> {
    let frame = Frame {
        angle,
        fit,
        resample,
        ..Frame::default()
    };
    turn(height, width, &picture_of(height, width, px), frame, None)
}

fn turned_by(angle: f64, fit: Fit) -> Frame {
    Frame {
        angle,
        fit,
        ..Frame::default()
    }
}

#[test]
fn a_quarter_turn_moves_every_pixel_clockwise_when_positive() -> Result<(), Error> {
    // [[a, b, c], [d, e, f]] turned: 90 clockwise puts the left column on top, read upward; a half turn reverses both
    // ways; -90 and 270 are a quarter counter-clockwise. By hand, grey pixels 0.1 to 0.6.
    let grey = |values: &[f32]| values.iter().flat_map(|&value| [value; 3]).collect::<Vec<_>>();
    let scene = grey(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
    for (angle, shape, expected) in [
        (90.0, (3, 2), [0.4, 0.1, 0.5, 0.2, 0.6, 0.3]),
        (180.0, (2, 3), [0.6, 0.5, 0.4, 0.3, 0.2, 0.1]),
        (-90.0, (3, 2), [0.3, 0.6, 0.2, 0.5, 0.1, 0.4]),
        (270.0, (3, 2), [0.3, 0.6, 0.2, 0.5, 0.1, 0.4]),
        (-180.0, (2, 3), [0.6, 0.5, 0.4, 0.3, 0.2, 0.1]),
        (360.0, (2, 3), [0.1, 0.2, 0.3, 0.4, 0.5, 0.6]),
    ] {
        let made = turn(2, 3, &scene, turned_by(angle, Fit::Expand), None)?;
        assert_eq!((made.height, made.width), shape, "{angle}");
        assert_eq!(made.values, grey(&expected), "{angle}");
    }
    Ok(())
}

#[test]
fn a_quarter_turn_that_crops_shows_the_centred_part_of_the_turned_picture_the_photos_shape() -> Result<(), Error> {
    // 96 x 128 turned 90 is 128 x 96; the largest part of the photo's 3:4 shape it covers is 72 x 96, rows 28 to 100
    // (the pixel at (r, c) of the turned picture is the scene's at (95 - c, r))
    let made = scene_turned(96, 128, 90.0, Fit::Crop, Resample::Bicubic)?;
    assert_eq!((made.height, made.width), (72, 96));
    assert_eq!(made.values, picture_of(72, 96, |row, column| px(95 - column, 28 + row)));
    Ok(())
}

#[test]
fn a_quarter_turn_that_crops_floors_the_room_it_centres_in() -> Result<(), Error> {
    // 4 x 7 turned 90 is 7 x 4; the part that fits is 2 x 4, centred in 7 rows: 5 / 2 = 2.5 -> row 2 (the pixel at (r, c)
    // of the turned picture is the scene's at (3 - c, r)). 7 x 4 turned is 4 x 7; the part is 4 x 2, centred in 7
    // columns: column 2.
    let wide = scene_turned(4, 7, 90.0, Fit::Crop, Resample::Nearest)?;
    assert_eq!(wide.values, picture_of(2, 4, |row, column| px(3 - column, 2 + row)));
    let tall = scene_turned(7, 4, 90.0, Fit::Crop, Resample::Nearest)?;
    assert_eq!(tall.values, picture_of(4, 2, |row, column| px(6 - (2 + column), row)));
    Ok(())
}

#[test]
fn a_positive_angle_turns_clockwise_and_a_negative_one_counter_clockwise() -> Result<(), Error> {
    // a white block in the top left corner of a black 24 x 24 picture: a clockwise turn of 45 degrees takes the top left
    // corner to the top (the topmost point of the diamond) and the top right corner to the right; counter-clockwise takes
    // it to the left
    let values = picture_of(24, 24, |row, column| [f32::from(u8::from(row < 4 && column < 4)); 3]);
    let centroid = |angle: f64| -> Result<(f64, f64, usize), Error> {
        let frame = Frame {
            resample: Resample::Nearest,
            ..turned_by(angle, Fit::Expand)
        };
        let made = turn(24, 24, &values, frame, None)?;
        let white: Vec<(usize, usize)> = (0..made.height * made.width)
            .filter(|&index| made.values[index * 3] > 0.5)
            .map(|index| (index / made.width, index % made.width))
            .collect();
        let mean = |part: fn(&(usize, usize)) -> usize| {
            white
                .iter()
                .map(|place| f64::from(u32::try_from(part(place)).unwrap_or(0)))
                .sum::<f64>()
                / f64::from(u32::try_from(white.len()).unwrap_or(1))
        };
        Ok((mean(|place| place.0), mean(|place| place.1), made.width))
    };
    // the picture is 34 x 34: the middle is 17
    let (row, column, _) = centroid(45.0)?;
    assert!(
        row < 6.0 && (column - 17.0).abs() < 3.0,
        "clockwise: row {row}, column {column}"
    );
    let (row, column, _) = centroid(-45.0)?;
    assert!(
        column < 6.0 && (row - 17.0).abs() < 3.0,
        "counter-clockwise: row {row}, column {column}"
    );
    Ok(())
}

#[test]
fn the_flip_comes_before_the_turn() -> Result<(), Error> {
    // flipped across, then 90 clockwise: the pixel at (r, c) of the 3 x 2 result is the scene's at (H - 1 - c, W - 1 - r)
    let frame = Frame {
        flip: Flip::X,
        ..turned_by(90.0, Fit::Expand)
    };
    let made = turn(2, 3, &picture_of(2, 3, px), frame, None)?;
    assert_eq!(made.values, picture_of(3, 2, |row, column| px(1 - column, 2 - row)));
    Ok(())
}

proptest! {
    #[test]
    fn four_quarter_turns_are_the_picture_and_a_quarter_turn_then_its_inverse_is_exact(
        height in 1_usize..9, width in 1_usize..9, data in prop::collection::vec(0.0_f32..=1.0, 3 * 81),
    ) {
        let values: Vec<f32> = data[..height * width * 3].to_vec();
        let (mut now, mut high, mut wide) = (values.clone(), height, width);
        for _ in 0..4 {
            let made = turn(high, wide, &now, turned_by(90.0, Fit::Expand), None)?;
            (now, high, wide) = (made.values, made.height, made.width);
        }
        prop_assert_eq!((high, wide, now), (height, width, values.clone()));
        let there = turn(height, width, &values, turned_by(90.0, Fit::Expand), None)?;
        let back = turn(there.height, there.width, &there.values, turned_by(-90.0, Fit::Expand), None)?;
        prop_assert_eq!(back.values, values);
    }

    #[test]
    fn a_half_turn_is_a_flip_both_ways(
        height in 1_usize..12, width in 1_usize..12, crop in any::<bool>(), which in 0_usize..3,
    ) {
        let fit = if crop { Fit::Crop } else { Fit::Expand };
        let resample = [Resample::Nearest, Resample::Bilinear, Resample::Bicubic][which];
        let values = picture_of(height, width, px);
        let turned = turn(height, width, &values, Frame { resample, ..turned_by(180.0, fit) }, None)?;
        let flipped = turn(height, width, &values, Frame { flip: Flip::Both, ..Frame::default() }, None)?;
        prop_assert_eq!(turned.values, flipped.values);
    }
}

#[test]
fn an_angle_and_the_same_turn_a_whole_number_of_turns_away_make_one_picture() -> Result<(), Error> {
    for (angle, other) in [
        (30.0, -330.0),
        (17.5, -342.5),
        (100.0, -260.0),
        (-45.0, 315.0),
        (0.5, 360.5),
        (90.0, -270.0),
        (180.0, -180.0),
        (30.0, 750.0),
        (30.0, 3_600_000_000_030.0),
        (30.0, -3_599_999_999_970.0),
    ] {
        for fit in [Fit::Crop, Fit::Expand] {
            for resample in [Resample::Nearest, Resample::Bilinear, Resample::Bicubic] {
                let one = scene_turned(7, 9, angle, fit, resample)?;
                let two = scene_turned(7, 9, other, fit, resample)?;
                assert_eq!(
                    (one.height, one.width, one.values),
                    (two.height, two.width, two.values),
                    "{angle} and {other}, {fit:?}, {resample:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_turn_by_a_whole_number_of_turns_changes_nothing() -> Result<(), Error> {
    for fit in [Fit::Crop, Fit::Expand] {
        let made = scene_turned(5, 7, 360.0, fit, Resample::Bicubic)?;
        assert_eq!(made.values, picture_of(5, 7, px), "{fit:?}");
    }
    Ok(())
}

#[test]
fn the_size_of_a_turned_picture_is_worked_by_hand() -> Result<(), Error> {
    // (height, width, angle, fit, the turned height and width). An expanded picture is the bounding box of the turned
    // rectangle, rounded up: 40 x 20 at 30 degrees is 40 cos 30 + 20 sin 30 = 44.64 wide and 40 sin 30 + 20 cos 30 =
    // 37.32 tall; 10 x 10 at 45 is 14.14 each way. A cropped one is the largest rectangle of the photo's aspect the
    // turned picture covers, rounded down: at 45 degrees a square of side s fits one of s / (cos 45 + sin 45) = 7.07;
    // 20 x 40 at 30 degrees is the width 40 x 0.5359 = 21.4 and the height 10.7. A quarter turn swaps the sides when
    // expanded; at 180 degrees the picture is whole. A very long picture keeps a pixel.
    for (height, width, angle, fit, expected) in [
        (20, 40, 30.0, Fit::Expand, (38, 45)),
        (10, 10, 45.0, Fit::Expand, (15, 15)),
        (4, 6, 90.0, Fit::Expand, (6, 4)),
        (5, 7, -60.0, Fit::Expand, (9, 8)),
        (10, 10, 45.0, Fit::Crop, (7, 7)),
        (20, 40, 30.0, Fit::Crop, (10, 21)),
        (5, 7, 60.0, Fit::Crop, (2, 4)),
        (4, 6, 180.0, Fit::Crop, (4, 6)),
        (96, 128, 90.0, Fit::Crop, (72, 96)),
        (4, 7, 90.0, Fit::Crop, (2, 4)),
        (7, 4, 90.0, Fit::Crop, (4, 2)),
        (1, 40, 45.0, Fit::Crop, (1, 1)),
        (40, 1, 45.0, Fit::Crop, (1, 1)),
    ] {
        let made = scene_turned(height, width, angle, fit, Resample::Nearest)?;
        assert_eq!(
            (made.height, made.width),
            expected,
            "{height} x {width} at {angle}, {fit:?}"
        );
    }
    Ok(())
}

#[test]
fn a_cropped_turn_shows_none_of_the_fill() -> Result<(), Error> {
    // a constant picture of 0.3 stays 0.3 everywhere, though the fill is white
    for (height, width) in [(9, 12), (12, 9), (7, 7), (1, 30), (30, 1), (2, 2), (33, 50)] {
        for angle in [
            1.0, 10.0, 30.0, 45.0, 60.0, 89.0, 91.0, 135.0, 179.5, -20.0, -135.0, 17.3,
        ] {
            for resample in [Resample::Nearest, Resample::Bilinear, Resample::Bicubic] {
                let frame = Frame {
                    resample,
                    fill: Color([1.0, 1.0, 1.0]),
                    ..turned_by(angle, Fit::Crop)
                };
                let made = turn(height, width, &vec![0.3; height * width * 3], frame, None)?;
                assert!(
                    made.values.iter().all(|&value| (value - 0.3).abs() < 1e-6),
                    "{height} x {width} at {angle}, {resample:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn an_expanded_turn_shows_the_fill_in_the_corners_the_picture_in_the_middle_and_blends_one_pixel_between()
-> Result<(), Error> {
    // 0.6 turned 45 degrees: the corner is the fill (0.1, 0.2, 0.3) exactly, the middle is 0.6, and some pixel on the
    // edge is strictly between the two in every channel
    let frame = Frame {
        fill: Color([0.1, 0.2, 0.3]),
        resample: Resample::Bilinear,
        ..turned_by(45.0, Fit::Expand)
    };
    let made = turn(20, 20, &vec![0.6; 20 * 20 * 3], frame, None)?;
    assert_eq!((made.height, made.width), (29, 29));
    let pixel = |row: usize, column: usize| made.values[(row * 29 + column) * 3..][..3].to_vec();
    assert_eq!(pixel(0, 0), vec![0.1, 0.2, 0.3]);
    assert_eq!(pixel(28, 28), vec![0.1, 0.2, 0.3]);
    assert_eq!(pixel(14, 14), vec![0.6; 3]);
    let blended = (0..29).any(|column| {
        pixel(0, column)
            .iter()
            .zip([0.1, 0.2, 0.3])
            .all(|(&value, fill)| value > fill + 1e-3 && value < 0.6 - 1e-3)
    });
    assert!(blended, "the top row has a pixel part picture, part fill");
    // what is neither fill nor picture lies within a pixel of the edge: a row through the middle is picture all but its
    // two ends
    let middle: Vec<usize> = (0..29).filter(|&column| pixel(14, column)[0] < 0.6 - 1e-6).collect();
    assert!(middle.iter().all(|&column| column <= 3 || column >= 25), "{middle:?}");
    Ok(())
}

proptest! {
    #[test]
    fn a_map_is_turned_as_the_photo_is(
        height in 1_usize..14, width in 1_usize..14, hot in (0_usize..14, 0_usize..14),
        angle in -360.0_f64..360.0, crop in any::<bool>(), which in 0_usize..3,
    ) {
        // a one-hot photo and a one-hot map: the same pixels light up, with the same values (the turn is read pixel by
        // pixel alike), borders and corners 0 in both
        let (row, column) = (hot.0 % height, hot.1 % width);
        let values = picture_of(height, width, |r, c| [f32::from(u8::from((r, c) == (row, column))); 3]);
        let subject = Layer::new(height, width, (0..height * width).map(|i| f32::from(u8::from(i == row * width + column))).collect())?;
        let frame = Frame {
            resample: [Resample::Nearest, Resample::Bilinear, Resample::Bicubic][which],
            ..turned_by(angle, if crop { Fit::Crop } else { Fit::Expand })
        };
        let made = turn(height, width, &values, frame, Some(subject))?;
        let red: Vec<f32> = made.values.chunks(3).map(|pixel| pixel[0]).collect();
        prop_assert_eq!(made.subject.len(), made.height * made.width);
        prop_assert_eq!(red, made.subject);
    }

    #[test]
    fn a_turned_picture_stays_within_0_and_1_though_the_cubic_overshoots(
        height in 1_usize..12, width in 1_usize..12, angle in -180.0_f64..180.0, crop in any::<bool>(),
        edges in prop::collection::vec(any::<bool>(), 144),
    ) {
        let values: Vec<f32> = edges.iter().cycle().take(height * width * 3).map(|&on| f32::from(u8::from(on))).collect();
        let frame = Frame { resample: Resample::Bicubic, ..turned_by(angle, if crop { Fit::Crop } else { Fit::Expand }) };
        let made = turn(height, width, &values, frame, None)?;
        prop_assert!(made.values.iter().all(|value| (0.0..=1.0).contains(value)));
    }
}

#[test]
fn a_crop_around_the_subject_is_cut_from_the_turned_subject() -> Result<(), Error> {
    // the subject, rows 2 to 4 and columns 8 to 12 of a 10 x 20 scene, turned 90 clockwise lies at rows 8 to 12 and
    // columns 6 to 8 of the 20 x 10 result (the pixel at (r, c) is the scene's at (9 - c, r))
    let subject = Layer::new(
        10,
        20,
        (0..200)
            .map(|index| {
                f32::from(u8::from(
                    (2..4).contains(&(index / 20)) && (8..12).contains(&(index % 20)),
                ))
            })
            .collect(),
    )?;
    let frame = Frame {
        anchor: Anchor::Subject,
        ..turned_by(90.0, Fit::Expand)
    };
    let made = turn(10, 20, &picture_of(10, 20, px), frame, Some(subject))?;
    assert_eq!((made.height, made.width), (4, 2));
    assert_eq!(made.values, picture_of(4, 2, |row, column| px(3 - column, 8 + row)));
    assert_eq!(made.subject, vec![1.0; 8]);
    Ok(())
}

#[test]
fn a_crop_around_the_subject_of_an_expanded_turn_is_the_box_of_the_turned_subject_not_the_corners() -> Result<(), Error>
{
    // the corners a turn leaves are no subject: the subject rows 8 to 12, columns 10 to 20 of a 20 x 30 scene, turned 30
    // degrees, has a box smaller than the picture, and every side of the crop has a subject pixel on it
    let subject = Layer::new(
        20,
        30,
        (0..600)
            .map(|index| {
                f32::from(u8::from(
                    (8..12).contains(&(index / 30)) && (10..20).contains(&(index % 30)),
                ))
            })
            .collect(),
    )?;
    let whole = scene_turned(20, 30, 30.0, Fit::Expand, Resample::Bilinear)?;
    let frame = Frame {
        anchor: Anchor::Subject,
        resample: Resample::Bilinear,
        fill: Color([1.0, 1.0, 1.0]),
        ..turned_by(30.0, Fit::Expand)
    };
    let made = turn(20, 30, &picture_of(20, 30, px), frame, Some(subject))?;
    assert!(
        made.height < whole.height && made.width < whole.width,
        "{} x {}",
        made.height,
        made.width
    );
    let on = |row: usize, column: usize| made.subject[row * made.width + column] >= 0.5;
    assert!((0..made.width).any(|column| on(0, column)) && (0..made.width).any(|column| on(made.height - 1, column)));
    assert!((0..made.height).any(|row| on(row, 0)) && (0..made.height).any(|row| on(row, made.width - 1)));
    Ok(())
}

#[test]
fn the_border_is_laid_round_the_turned_picture() -> Result<(), Error> {
    // 2 x 3 turned a quarter is 3 x 2, the shorter side 2: a border of 0.5 is 1 pixel above, in the fill
    let frame = Frame {
        pad_top: 0.5,
        fill: Color([0.9, 0.8, 0.7]),
        ..turned_by(90.0, Fit::Expand)
    };
    let made = turn(2, 3, &picture_of(2, 3, px), frame, None)?;
    assert_eq!((made.height, made.width), (4, 2));
    assert_eq!(&made.values[..6], &[0.9, 0.8, 0.7, 0.9, 0.8, 0.7]);
    assert_eq!(
        &made.values[6..],
        &picture_of(3, 2, |row, column| px(1 - column, row))[..]
    );
    Ok(())
}

#[test]
fn a_nearest_turn_picks_the_pixel_pillows_fixed_point_does_where_a_position_is_close_to_a_border() -> Result<(), Error>
{
    // The expected pixel numbers (row * width + column of the scene) were taken from Pillow 13.0's
    // `Image.fromarray(np.arange(n * n, dtype=np.float32).reshape(n, n)).transform((side, side), Image.Transform.AFFINE,
    // matrix, Image.Resampling.NEAREST)`, with the matrix the Python engine builds for a crop (`ops.frame.rotate`). Rounding
    // the matrix to 16.16 fixed point down instead of to the nearest changes 8 of the 25 pixels of the first picture and
    // 7 of the 49 of the second, so these are the positions that tell the two apart.
    let number =
        |row: usize, column: usize, width: usize| f32::from(u8::try_from(row * width + column).unwrap_or(0)) / 100.0;
    for (size, angle, expected) in [
        (
            8,
            45.0,
            vec![
                vec![33_u8, 25, 18, 11, 12],
                vec![33, 34, 27, 20, 12],
                vec![42, 35, 36, 28, 21],
                vec![51, 44, 36, 37, 30],
                vec![52, 52, 45, 38, 38],
            ],
        ),
        (
            10,
            -45.0,
            vec![
                vec![5, 15, 26, 27, 37, 48, 59],
                vec![14, 25, 25, 36, 47, 57, 58],
                vec![23, 24, 35, 45, 56, 57, 67],
                vec![22, 33, 44, 55, 55, 66, 77],
                vec![32, 42, 53, 54, 65, 75, 76],
                vec![41, 52, 52, 63, 74, 75, 85],
                vec![50, 51, 62, 72, 73, 84, 95],
            ],
        ),
    ] {
        let scene = picture_of(size, size, |row, column| [number(row, column, size); 3]);
        let frame = Frame {
            angle,
            fit: Fit::Crop,
            resample: Resample::Nearest,
            ..Frame::default()
        };
        let made = turn(size, size, &scene, frame, None)?;
        let side = expected.len();
        assert_eq!((made.height, made.width), (side, side), "{size} at {angle}");
        let numbers: Vec<f32> = expected
            .iter()
            .flatten()
            .flat_map(|&index| [f32::from(index) / 100.0; 3])
            .collect();
        assert_eq!(made.values, numbers, "{size} at {angle}");
    }
    Ok(())
}
