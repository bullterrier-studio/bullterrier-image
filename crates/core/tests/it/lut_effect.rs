//! The `lut` effect, built from its typed `Params` and run by a render: the table a photo is given grades the picture, and
//! `amount` mixes the grade with the picture. Expectations are hand arithmetic on tables whose lookups are known.

use std::sync::Arc;

use bullterrier_image::effects::lut;
use bullterrier_image::params::LutName;
use bullterrier_image::{Error, Lut, Options, Photo, Picture, Presets, Recipe, Step};
use proptest::prelude::*;

/// A table of two entries per axis whose entry for the corner (red, green, blue) is `map` of the corner, each 0 or 1.
fn corners(map: impl Fn([f32; 3]) -> [f32; 3]) -> Result<Lut, Error> {
    let entries = (0..8_usize)
        .map(|entry| map([entry % 2, entry / 2 % 2, entry / 4].map(|corner| f32::from(u8::from(corner == 1)))))
        .collect();
    Lut::new(2, [0.0; 3], [1.0; 3], entries)
}

/// `values` (a picture one pixel high) graded by the step `step` with `table` given as `film`.
fn graded(values: &[f32], step: Step, table: Lut) -> Result<Vec<f32>, Error> {
    let photo = Photo::new(Picture::new(1, values.len() / 3, values)?).with_lut("film", Arc::new(table));
    Ok(photo
        .render(&Recipe::new([step]), &Presets::new(), Options::default())?
        .into_values())
}

fn film(amount: f32) -> Step {
    Step::of(lut::Params {
        file: LutName::new("film"),
        amount,
    })
}

fn close(actual: &[f32], expected: &[f32]) -> bool {
    actual.len() == expected.len() && actual.iter().zip(expected).all(|(a, e)| (a - e).abs() <= 1e-5)
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn a_negative_table_gives_the_negative_and_amount_is_the_share_of_the_way(
        pixel in prop::array::uniform3(0.0_f32..=1.0),
        amount in 0.0_f32..=1.0,
    ) {
        let made = graded(&pixel, film(amount), corners(|corner| corner.map(|level| 1.0 - level))?)?;
        let expected = pixel.map(|level| (1.0 - amount) * level + amount * (1.0 - level));
        prop_assert!(close(&made, &expected), "{made:?}, not {expected:?}");
    }

    #[test]
    fn an_identity_table_changes_nothing_at_any_amount(
        pixel in prop::array::uniform3(0.0_f32..=1.0),
        amount in 0.0_f32..=1.0,
        size in 2_usize..=9,
    ) {
        let made = graded(&pixel, film(amount), Lut::identity(size)?)?;
        prop_assert!(close(&made, &pixel), "{made:?}, not {pixel:?}");
    }
}

#[test]
fn an_amount_of_nothing_leaves_the_picture_whatever_the_table() -> Result<(), Error> {
    let pixel = [0.2, 0.5, 0.9];
    let made = graded(&pixel, film(0.0), corners(|_| [1.0, 0.0, 0.5])?)?;
    assert!(close(&made, &pixel), "{made:?}");
    Ok(())
}

#[test]
fn a_table_that_leaves_the_range_gives_a_picture_inside_it() -> Result<(), Error> {
    let made = graded(&[0.3, 0.6, 0.9], film(1.0), corners(|_| [1.4, -0.2, 0.5])?)?;
    assert!(close(&made, &[1.0, 0.0, 0.5]), "{made:?}");
    Ok(())
}

#[test]
fn a_step_with_no_table_is_the_identity() -> Result<(), Error> {
    let pixel = [0.2, 0.5, 0.9];
    let photo = Photo::new(Picture::new(1, 1, &pixel)?);
    let step = Step::of(lut::Params::default());
    let made = photo.render(&Recipe::new([step]), &Presets::new(), Options::default())?;
    assert!(close(&made.into_values(), &pixel));
    Ok(())
}

#[test]
fn the_step_of_typed_params_is_the_step_written_by_hand() {
    assert_eq!(film(0.75), Step::new("lut").param("file", "film").param("amount", 0.75));
    assert_eq!(
        Step::of(lut::Params::default()),
        Step::new("lut")
            .param("file", serde_json::Value::Null)
            .param("amount", 1.0)
    );
}
