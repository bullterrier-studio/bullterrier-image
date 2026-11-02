//! Pixel positions and sizes are rounded as Python's `round` is, halves to the even whole number, seen through where a
//! centred crop of a picture starts: the free room before the crop is half of what is left over, and a half of an odd
//! number goes to the even pixel.

use bullterrier_image::{Error, Frame, Options, Photo, Picture, Presets, Recipe, Step};

/// The first channel of the columns of a 2-row picture of `width` columns, each column's value its number over 10, after a
/// centred crop to a square.
fn cropped_columns(width: usize) -> Result<Vec<f32>, Error> {
    let values: Vec<f32> = (0..2 * width)
        .flat_map(|at| [f32::from(u8::try_from(at % width).unwrap_or(0)) / 10.0; 3])
        .collect();
    let mut recipe = Recipe::new([Step::new("grade").opacity(0.0)]);
    recipe.frame = Frame {
        aspect: Some(1.0),
        ..Frame::default()
    };
    let image = Photo::new(Picture::new(2, width, &values)?).render(&recipe, &Presets::new(), Options::default())?;
    assert_eq!((image.height(), image.width()), (2, 2));
    Ok(image
        .into_values()
        .as_chunks::<3>()
        .0
        .iter()
        .take(2)
        .map(|pixel| pixel[0])
        .collect())
}

#[test]
fn a_half_pixel_of_room_goes_to_the_even_whole_number() -> Result<(), Error> {
    // (width, the first column of the crop): room 5 is 2.5 before the crop, to the even 2 (not 3); room 3 is 1.5, to 2;
    // room 1 is 0.5, to 0; room 2 is exactly 1; room 7 is 3.5, to 4
    for (width, first) in [(7_u8, 2_u8), (5, 2), (3, 0), (4, 1), (9, 4)] {
        let columns = cropped_columns(usize::from(width))?;
        let expected = [f32::from(first) / 10.0, f32::from(first + 1) / 10.0];
        assert_eq!(columns, expected, "width {width}");
    }
    Ok(())
}
