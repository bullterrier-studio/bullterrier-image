//! `Picture` and `Image`, the pictures an effect reads and makes, through the public API alone.

use bullterrier_image::frame::Transparent;
use bullterrier_image::{Error, ErrorKind, Frame, Image, Options, Photo, Picture, Presets, Recipe, Step};

#[test]
fn map_at_hands_each_pixel_with_its_row_and_column_in_reading_order() -> Result<(), Error> {
    // 2 rows of 3 pixels, each pixel's channels all its own number 0 to 5
    let values: Vec<f32> = (0_u8..6).flat_map(|pixel| [f32::from(pixel); 3]).collect();
    let picture = Picture::new(2, 3, &values)?;
    // the new pixel records where it was found (row, column) and the value it held
    let image = picture.map_at(|row, column, pixel| {
        let place = |index: usize| u8::try_from(index).map_or(f32::NAN, f32::from);
        [place(row), place(column), pixel[0]]
    });
    assert_eq!((image.height(), image.width()), (2, 3));
    assert_eq!(
        image.into_values(),
        [0., 0., 0., 0., 1., 1., 0., 2., 2., 1., 0., 3., 1., 1., 4., 1., 2., 5.]
    );
    Ok(())
}

#[test]
fn the_values_of_a_picture_and_of_an_image_made_from_them_are_the_ones_given() -> Result<(), Error> {
    let values = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
    let picture = Picture::new(1, 2, &values)?;
    assert_eq!(picture.values(), values);
    let image = Image::new(1, 2, picture.values())?;
    assert_eq!((image.height(), image.width()), (1, 2));
    assert_eq!(image.into_values(), values);
    Ok(())
}

#[test]
fn an_image_read_as_a_picture_has_its_size_and_its_values() -> Result<(), Error> {
    let values = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
    let image = Image::new(2, 1, &values)?;
    let picture = image.as_picture();
    assert_eq!((picture.height(), picture.width()), (2, 1));
    assert_eq!(picture.values(), values);
    Ok(())
}

#[test]
fn an_image_with_alpha_has_four_channels_per_pixel_and_is_read_by_an_effect_as_its_colour_alone() -> Result<(), Error> {
    // an image gets its alpha from a cut-out frame: here the circle inscribed in a 4 x 4 picture
    let colour: Vec<f32> = (0_u8..48).map(|index| f32::from(index) / 50.0).collect();
    let photo = Photo::new(Picture::new(4, 4, &colour)?);
    let render = |transparent: Transparent| {
        let mut recipe = Recipe::new([Step::new("grade").opacity(0.0)]);
        recipe.frame = Frame {
            transparent,
            ..Frame::default()
        };
        photo.render(&recipe, &Presets::new(), Options::default())
    };
    let (plain, cut) = (render(Transparent::None)?, render(Transparent::Circle)?);
    assert_eq!(plain.channels(), 3);
    assert_eq!(cut.channels(), 4);
    assert_eq!(cut.as_picture().values(), colour);
    let values = cut.into_values();
    assert_eq!(values.len(), 4 * 4 * 4);
    let interleaved: Vec<f32> = values.chunks(4).flat_map(|pixel| pixel[..3].to_vec()).collect();
    assert_eq!(interleaved, colour);
    assert!(values.chunks(4).all(|pixel| (0.0..=1.0).contains(&pixel[3])));
    Ok(())
}

#[test]
fn a_picture_with_a_level_that_is_no_number_is_refused_naming_the_first_such_pixel() {
    for (rule, level) in [("NaN", f32::NAN), ("+inf", f32::INFINITY), ("-inf", f32::NEG_INFINITY)] {
        // 2 x 3 pixels; the bad level sits in the blue channel of row 1, column 2 and again in the last pixel
        let mut values = [0.5_f32; 18];
        values[(3 + 2) * 3 + 2] = level;
        values[17] = level;
        let refused = Picture::new(2, 3, &values);
        assert!(
            matches!(&refused, Err(error @ Error::NonFinite(_)) if error.to_string().contains("row 1, column 2")),
            "{rule}: {refused:?}"
        );
    }
    assert!(Picture::new(2, 3, &[1.0_f32; 18]).is_ok());
}

#[test]
fn a_picture_or_image_with_no_row_or_no_column_is_refused_and_one_pixel_is_enough() {
    for (height, width) in [(0, 5), (5, 0), (0, 0)] {
        let picture = Picture::new(height, width, &[]);
        let image = Image::new(height, width, &[]);
        for refused in [picture.map(|_| ()), image.map(|_| ())] {
            assert!(
                matches!(&refused, Err(error @ Error::Size { .. }) if error.to_string().contains("at least one")),
                "{height} x {width}: {refused:?}"
            );
        }
    }
    assert!(Picture::new(1, 1, &[0.1, 0.2, 0.3]).is_ok());
    assert!(Image::new(1, 1, &[0.1, 0.2, 0.3]).is_ok());
}

#[test]
fn an_image_of_values_that_do_not_make_its_size_is_refused() {
    assert!(matches!(
        Image::new(1, 2, &[0.0; 5]),
        Err(Error::Size { values: 5, .. })
    ));
    assert!(matches!(
        Image::new(2, 2, &[0.0; 6]),
        Err(Error::Size { values: 6, .. })
    ));
}

#[test]
fn an_rgba_image_keeps_its_four_channels_as_given() -> Result<(), Error> {
    // two pixels, every channel its own number; the colour under alpha 0 stays
    let values = [0.1, 0.2, 0.3, 0.0, 0.4, 0.5, 0.6, 1.0];
    let image = Image::rgba(1, 2, &values)?;
    assert_eq!((image.height(), image.width(), image.channels()), (1, 2, 4));
    assert_eq!(image.as_picture().values(), [0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
    assert_eq!(image.into_values(), values);
    Ok(())
}

#[test]
fn an_rgba_image_is_refused_for_a_wrong_size_a_non_number_or_an_alpha_outside_0_to_1() {
    let nan = [0.1, 0.2, 0.3, 1.0, 0.4, f32::NAN, 0.6, 1.0];
    let rows: [(&str, Result<Image, Error>, ErrorKind, &str); 5] = [
        (
            "a value too few",
            Image::rgba(1, 2, &[0.5; 7]),
            ErrorKind::InvalidImage,
            "7 values",
        ),
        ("no column", Image::rgba(1, 0, &[]), ErrorKind::InvalidImage, "1 x 0"),
        (
            "a NaN",
            Image::rgba(1, 2, &nan),
            ErrorKind::NonFiniteImage,
            "row 0, column 1",
        ),
        (
            "alpha above 1",
            Image::rgba(1, 2, &[0.1, 0.2, 0.3, 1.0, 0.4, 0.5, 0.6, 1.5]),
            ErrorKind::InvalidImage,
            "row 0, column 1",
        ),
        (
            "negative alpha",
            Image::rgba(1, 1, &[0.1, 0.2, 0.3, -0.5]),
            ErrorKind::InvalidImage,
            "-0.5",
        ),
    ];
    for (id, result, kind, says) in rows {
        let error = result.err();
        assert_eq!(error.as_ref().map(Error::kind), Some(kind), "{id}");
        assert!(error.is_some_and(|error| error.to_string().contains(says)), "{id}");
    }
}

#[test]
fn over_white_lays_a_transparent_image_on_white_so_a_hidden_colour_never_shows() -> Result<(), Error> {
    // pixels: opaque red, clear green (hidden), half blue
    let image = Image::rgba(1, 3, &[1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.5])?;
    let flat = image.over_white();
    assert_eq!((flat.channels(), flat.height(), flat.width()), (3, 1, 3));
    assert_eq!(flat.into_values(), [1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.5, 0.5, 1.0]);
    let opaque = Image::new(1, 1, &[0.2, 0.4, 0.6])?;
    assert_eq!(opaque.over_white().into_values(), [0.2, 0.4, 0.6], "no alpha: as it is");
    Ok(())
}
