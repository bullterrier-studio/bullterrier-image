//! A recipe's `frame.transparent`, through the public API alone: after the last step the picture is cut out, which gives
//! it a fourth channel, alpha, and leaves its colour as the steps made it.

use bullterrier_image::frame::Transparent;
use bullterrier_image::{Error, ErrorKind, Frame, Image, Layer, Options, Photo, Picture, Presets, Recipe, Stage, Step};

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

/// A scene with a different value in every channel and place (up to 200 columns).
fn scene(height: usize, width: usize) -> Vec<f32> {
    picture_of(height, width, |row, column| {
        let value = f32::from(u16::try_from(row * 200 + column).unwrap_or(0)) / 40000.0;
        [value, value + 0.0001, value + 0.0002]
    })
}

/// A recipe of one step that changes nothing, with `frame`.
fn recipe_of(frame: Frame) -> Recipe {
    let mut recipe = Recipe::new([Step::new("grade").opacity(0.0)]);
    recipe.frame = frame;
    recipe
}

/// The photo of `values`, a `height` x `width` picture, with `subject` as its subject map if there is one.
fn photo(values: &[f32], height: usize, width: usize, subject: Option<Layer>) -> Result<Photo<'_>, Error> {
    let photo = Photo::new(Picture::new(height, width, values)?);
    match subject {
        Some(subject) => photo.with_map("subject", subject),
        None => Ok(photo),
    }
}

/// The recipe of `frame` rendered on `photo` at the longest side `size` (none: as it is).
fn render(photo: &Photo<'_>, frame: Frame, size: Option<u32>) -> Result<Image, Error> {
    let options = size.map_or_else(Options::default, |size| Options::default().with_size(size));
    photo.render(&recipe_of(frame), &Presets::new(), options)
}

fn cut(transparent: Transparent, corner_radius: f64) -> Frame {
    Frame {
        transparent,
        corner_radius,
        ..Frame::default()
    }
}

/// The alpha channel of a four-channel image, row after row.
fn alpha_of(image: &Image) -> Vec<f32> {
    assert_eq!(image.channels(), 4);
    image.clone().into_values().chunks(4).map(|pixel| pixel[3]).collect()
}

/// The colour channels of an image of three or four, pixel after pixel.
fn colour_of(image: &Image) -> Vec<f32> {
    let channels = image.channels();
    image
        .clone()
        .into_values()
        .chunks(channels)
        .flat_map(|pixel| pixel[..3].to_vec())
        .collect()
}

/// The alpha of a `height` x `width` picture by brute force: for each pixel, the distance `signed` from its centre to
/// the edge of the shape (negative inside), turned into a coverage by a ramp one pixel wide across the edge.
fn reference(height: usize, width: usize, signed: impl Fn(f64, f64) -> f64) -> Vec<f64> {
    let centred = |index: usize, length: usize| {
        f64::from(u32::try_from(index).unwrap_or(0)) + 0.5 - f64::from(u32::try_from(length).unwrap_or(0)) / 2.0
    };
    (0..height)
        .flat_map(|row| (0..width).map(move |column| (row, column)))
        .map(|(row, column)| (0.5 - signed(centred(row, height), centred(column, width))).clamp(0.0, 1.0))
        .collect()
}

/// The circle inscribed in the picture: the distance from the middle, less the radius, which is half the shorter side.
fn circle(height: usize, width: usize) -> Vec<f64> {
    let radius = f64::from(u32::try_from(height.min(width)).unwrap_or(0)) / 2.0;
    reference(height, width, |dy, dx| dy.hypot(dx) - radius)
}

/// The rectangle with corners of `radius` px, by the standard signed distance of a rounded box (Quilez): the distance
/// from the pixel's centre to the box shrunk by the radius, less the radius.
fn rounded(height: usize, width: usize, radius: f64) -> Vec<f64> {
    let half = |length: usize| f64::from(u32::try_from(length).unwrap_or(0)) / 2.0;
    reference(height, width, |dy, dx| {
        let (qy, qx) = (dy.abs() - (half(height) - radius), dx.abs() - (half(width) - radius));
        qy.max(0.0).hypot(qx.max(0.0)) + qy.max(qx).min(0.0) - radius
    })
}

/// `actual` is `expected` to within what an f32 distance loses.
fn assert_alpha(actual: &[f32], expected: &[f64], row: &str) {
    assert_eq!(actual.len(), expected.len(), "{row}");
    for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!((f64::from(*a) - e).abs() < 1e-5, "{row}: pixel {index}: {a} is not {e}");
    }
}

/// The alpha of `frame` on a plain `height` x `width` picture rendered at `size`.
fn alpha(height: usize, width: usize, frame: Frame, size: Option<u32>) -> Result<Vec<f32>, Error> {
    let values = scene(height, width);
    Ok(alpha_of(&render(&photo(&values, height, width, None)?, frame, size)?))
}

#[test]
fn a_circle_is_the_one_inscribed_in_the_picture_with_an_edge_one_pixel_wide() -> Result<(), Error> {
    // squares of even and odd sides, a wide and a tall picture, a single row
    for (height, width) in [(8, 8), (9, 9), (31, 31), (6, 14), (14, 6), (1, 5)] {
        let made = alpha(height, width, cut(Transparent::Circle, 0.2), None)?;
        assert_alpha(&made, &circle(height, width), &format!("{height} x {width}"));
    }
    Ok(())
}

#[test]
fn a_circle_is_opaque_in_the_middle_clear_in_the_corners_and_the_same_on_both_sides_of_each_axis() -> Result<(), Error>
{
    for side in [8_usize, 9, 20] {
        let made = alpha(side, side, cut(Transparent::Circle, 0.2), None)?;
        let at = |row: usize, column: usize| made[row * side + column];
        assert_eq!(at(side / 2, side / 2), 1.0, "{side}: the middle");
        for corner in [(0, 0), (0, side - 1), (side - 1, 0), (side - 1, side - 1)] {
            assert_eq!(at(corner.0, corner.1), 0.0, "{side}: corner {corner:?}");
        }
        for row in 0..side {
            for column in 0..side {
                assert_eq!(
                    at(row, column),
                    at(side - 1 - row, column),
                    "{side}: mirrored top to bottom"
                );
                assert_eq!(
                    at(row, column),
                    at(row, side - 1 - column),
                    "{side}: mirrored left to right"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn rounded_corners_are_a_share_of_the_shorter_side() -> Result<(), Error> {
    // (height, width, corner_radius, the radius in px): 0.25 of the shorter side, the whole 0.5, none
    for (height, width, share, radius) in [
        (16, 16, 0.25, 4.0),
        (6, 14, 0.25, 1.5),
        (14, 6, 0.2, 1.2),
        (10, 10, 0.5, 5.0),
        (7, 9, 0.0, 0.0),
    ] {
        let made = alpha(height, width, cut(Transparent::Rounded, share), None)?;
        assert_alpha(
            &made,
            &rounded(height, width, radius),
            &format!("{height} x {width} at {share}"),
        );
    }
    Ok(())
}

#[test]
fn rounded_corners_of_no_radius_cut_nothing_and_of_half_the_side_are_the_circle() -> Result<(), Error> {
    for side in [5_usize, 8, 33] {
        let square = alpha(side, side, cut(Transparent::Rounded, 0.0), None)?;
        assert_eq!(square, vec![1.0; side * side], "{side}");
        let round = alpha(side, side, cut(Transparent::Rounded, 0.5), None)?;
        assert_eq!(round, alpha(side, side, cut(Transparent::Circle, 0.2), None)?, "{side}");
    }
    Ok(())
}

#[test]
fn a_cut_is_made_at_the_size_of_the_picture_the_steps_worked_on() -> Result<(), Error> {
    // 4 x 6 at a longest side of 12 is 8 x 12; framed to a square first it is 12 x 12 (a crop to the aspect 1)
    let made = alpha(4, 6, cut(Transparent::Circle, 0.2), Some(12))?;
    assert_alpha(&made, &circle(8, 12), "resized");
    let frame = Frame {
        aspect: Some(1.0),
        ..cut(Transparent::Circle, 0.2)
    };
    assert_alpha(&alpha(4, 6, frame, Some(12))?, &circle(12, 12), "cropped and resized");
    Ok(())
}

#[test]
fn the_background_cut_is_the_subject_as_the_steps_work_on_it() -> Result<(), Error> {
    // a 4 x 6 subject with a different value at each place
    let subject = (0_u8..24).map(|index| f32::from(index) / 24.0).collect::<Vec<_>>();
    let values = scene(4, 6);
    let photo = photo(&values, 4, 6, Some(Layer::new(4, 6, subject.clone())?))?;
    let unframed = render(&photo, cut(Transparent::Background, 0.2), None)?;
    assert_eq!(alpha_of(&unframed), subject);
    // flipped left to right the picture and the subject move together: each row of the subject is reversed
    let flipped = Frame {
        flip: bullterrier_image::frame::Flip::X,
        ..cut(Transparent::Background, 0.2)
    };
    let expected: Vec<f32> = subject.chunks(6).flat_map(|row| row.iter().rev().copied()).collect();
    assert_eq!(alpha_of(&render(&photo, flipped, None)?), expected);
    // at another size it is the subject resampled with the picture: what a trace shows as the subject
    let resized = Options::default().with_size(12);
    let recipe = recipe_of(cut(Transparent::Background, 0.2));
    let stages = photo.trace(&recipe, &Presets::new(), resized)?;
    let shown = stages.iter().find_map(|stage| match stage {
        Stage::Subject(layer) => Some(layer.values().to_vec()),
        _ => None,
    });
    let cut = photo.render(&recipe, &Presets::new(), resized)?;
    assert_eq!(Some(alpha_of(&cut)), shown);
    assert_eq!((cut.height(), cut.width()), (8, 12));
    Ok(())
}

#[test]
fn the_colour_of_a_cut_picture_is_the_colour_of_the_uncut_one_and_an_uncut_one_has_no_alpha() -> Result<(), Error> {
    let values = scene(5, 9);
    let subject = Layer::new(5, 9, vec![0.25; 45])?;
    let photo = photo(&values, 5, 9, Some(subject))?;
    let uncut = render(&photo, cut(Transparent::None, 0.2), None)?;
    assert_eq!(uncut.channels(), 3);
    assert_eq!(uncut.clone().into_values().len(), 5 * 9 * 3);
    for transparent in [Transparent::Circle, Transparent::Rounded, Transparent::Background] {
        let made = render(&photo, cut(transparent, 0.3), None)?;
        assert_eq!(made.channels(), 4, "{transparent:?}");
        assert_eq!(made.clone().into_values().len(), 5 * 9 * 4, "{transparent:?}");
        assert_eq!(colour_of(&made), colour_of(&uncut), "{transparent:?}");
    }
    Ok(())
}

#[test]
fn a_trace_ends_with_the_cut_picture_that_render_returns_after_the_picture_of_the_last_step() -> Result<(), Error> {
    let values = scene(4, 6);
    let subject = Layer::new(4, 6, (0_u8..24).map(|index| f32::from(index) / 24.0).collect())?;
    let photo = photo(&values, 4, 6, Some(subject))?;
    for transparent in [Transparent::Circle, Transparent::Rounded, Transparent::Background] {
        let recipe = recipe_of(cut(transparent, 0.3));
        let stages = photo.trace(&recipe, &Presets::new(), Options::default())?;
        let rendered = photo.render(&recipe, &Presets::new(), Options::default())?;
        let uncut = render(&photo, cut(Transparent::None, 0.3), None)?;
        // the stages end with the last step's picture, then the cut-out one
        assert!(
            matches!(stages.as_slice(), [.., Stage::Step { number: 1, image: step }, Stage::CutOut(last)]
                if *step == uncut && *last == rendered),
            "{transparent:?}: {stages:?}"
        );
    }
    // nothing is cut: the trace ends with the last step
    let stages = photo.trace(&recipe_of(Frame::default()), &Presets::new(), Options::default())?;
    assert!(
        matches!(stages.last(), Some(Stage::Step { number: 1, .. })),
        "{stages:?}"
    );
    Ok(())
}

#[test]
fn a_background_cut_needs_the_subject_and_a_circle_does_not() -> Result<(), Error> {
    let values = scene(4, 6);
    let photo = photo(&values, 4, 6, None)?;
    let background = recipe_of(cut(Transparent::Background, 0.2));
    let render = photo.render(&background, &Presets::new(), Options::default());
    let trace = photo.trace(&background, &Presets::new(), Options::default());
    for (what, kind) in [
        ("render", render.err().map(|error| error.kind())),
        ("trace", trace.err().map(|error| error.kind())),
    ] {
        assert_eq!(kind, Some(ErrorKind::SubjectNotFound), "{what}");
    }
    assert!(
        photo
            .render(
                &recipe_of(cut(Transparent::Circle, 0.2)),
                &Presets::new(),
                Options::default()
            )
            .is_ok()
    );
    Ok(())
}
