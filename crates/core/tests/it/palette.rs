//! `palette`: a picture's dominant colours, largest share first. The pictures are flat blocks of known colours and sizes, so
//! the expected palette is known by counting.

use crate::common::pictures::varied;
use bullterrier_image::{Error, ErrorKind, Picture, palette};

const RED: [f32; 3] = [0.8, 0.1, 0.1];
const GREEN: [f32; 3] = [0.1, 0.7, 0.2];
const BLUE: [f32; 3] = [0.1, 0.2, 0.9];

/// A picture 10 pixels wide whose rows are `blocks`: each colour for as many rows as it is given.
fn blocks(rows: &[(usize, [f32; 3])]) -> (usize, Vec<f32>) {
    let values = rows
        .iter()
        .flat_map(|&(count, colour)| std::iter::repeat_n(colour, count * 10).flatten())
        .collect::<Vec<f32>>();
    (values.len() / 30, values)
}

fn dominant(rows: &[(usize, [f32; 3])], colors: i64, seed: i128) -> Result<Vec<[f32; 3]>, Error> {
    let (height, values) = blocks(rows);
    palette(&Picture::new(height, 10, &values)?, colors, seed)
}

#[test]
fn three_flat_colours_come_back_exactly_largest_share_first_whatever_the_layout() -> Result<(), Error> {
    // 5 rows red, 3 green, 2 blue, in every order of the blocks
    let (red, green, blue) = ((5, RED), (3, GREEN), (2, BLUE));
    for layout in [[red, green, blue], [blue, green, red], [green, blue, red]] {
        for colors in [3, 5] {
            assert_eq!(
                dominant(&layout, colors, 0)?,
                vec![RED, GREEN, BLUE],
                "{layout:?} {colors}"
            );
        }
    }
    Ok(())
}

#[test]
fn equal_shares_put_the_lighter_colour_first() -> Result<(), Error> {
    let (light, dark) = ([0.9, 0.9, 0.8], [0.1, 0.1, 0.2]);
    assert_eq!(dominant(&[(4, dark), (4, light)], 2, 0)?, vec![light, dark]);
    assert_eq!(dominant(&[(4, light), (4, dark)], 2, 0)?, vec![light, dark]);
    Ok(())
}

#[test]
fn one_colour_is_the_mean_of_the_picture() -> Result<(), Error> {
    // 6 rows of black and 2 of white: the mean level is a quarter
    let got = dominant(&[(6, [0.0; 3]), (2, [1.0; 3])], 1, 0)?;
    assert_eq!(got.len(), 1);
    assert!(got[0].iter().all(|level| (level - 0.25).abs() < 1e-6), "{got:?}");
    Ok(())
}

#[test]
fn the_same_seed_gives_the_same_palette_on_a_picture_larger_than_the_search_sample() -> Result<(), Error> {
    let values = varied(150, 100); // 15000 pixels, more than the search looks at
    let picture = Picture::new(150, 100, &values)?;
    let first = palette(&picture, 6, 3)?;
    assert_eq!(first, palette(&picture, 6, 3)?);
    assert_eq!(first.len(), 6);
    assert!(first.iter().flatten().all(|level| (0.0..=1.0).contains(level)));
    Ok(())
}

#[test]
fn a_number_of_colours_outside_1_to_256_is_refused_naming_the_value() -> Result<(), Error> {
    let values = varied(4, 4);
    let picture = Picture::new(4, 4, &values)?;
    for bad in [0, -3] {
        let refused = palette(&picture, bad, 0).err();
        assert!(
            matches!(&refused, Some(error) if error.kind() == ErrorKind::Recipe
                && error.to_string().contains("colors") && error.to_string().contains(&bad.to_string())),
            "{bad}: {refused:?}"
        );
    }
    assert!(palette(&picture, 1, 0).is_ok());
    Ok(())
}

#[test]
fn a_seed_outside_0_to_4294967295_is_refused_naming_the_value() -> Result<(), Error> {
    let values = varied(4, 4);
    let picture = Picture::new(4, 4, &values)?;
    for bad in [-1_i128, 1 << 32, 1 << 70] {
        let refused = palette(&picture, 2, bad).err();
        assert!(
            matches!(&refused, Some(error) if error.kind() == ErrorKind::Recipe
                && error.to_string().contains("seed")),
            "{bad}: {refused:?}"
        );
    }
    let negative = palette(&picture, 2, -1).err().map(|error| error.to_string());
    assert_eq!(
        negative.as_deref(),
        Some("effect 'palette': `seed` must be >= 0, got -1")
    );
    for good in [0, i128::from(u32::MAX)] {
        assert!(palette(&picture, 2, good).is_ok(), "{good}");
    }
    Ok(())
}
