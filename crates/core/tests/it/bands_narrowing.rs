//! A number written in a recipe (`0.3`, a double) is compared with the single-precision values of a map after being
//! narrowed to single precision itself: a map value that is the `f32` nearest to a bound written in the recipe is on the
//! bound. In double precision the `f32` nearest to 0.7 is just below 0.7, and the one nearest to 0.3 just above it.

use crate::common::ops::probe;
use bullterrier_image::region::Band;
use bullterrier_image::{Error, Options};

/// The selection `band` makes of a one-row map of `values`.
fn chosen(values: Vec<f32>, band: Band) -> Result<Vec<f32>, Error> {
    let seen = probe(
        (1, values.len()),
        Some(values),
        Options::default(),
        vec![("r", band.into())],
        (None, Some("r")),
    )?;
    seen.near
        .ok_or_else(|| Error::Recipe("the spy was handed no selection".to_owned()))
}

fn band(low: f64, high: f64, feather: f64) -> Band {
    Band {
        of: "subject".to_owned(),
        low,
        high,
        feather,
    }
}

#[test]
fn a_value_that_is_the_nearest_single_to_a_bound_is_on_the_bound() -> Result<(), Error> {
    // 0.7f32 is 0.69999998..: below a low bound of 0.7 in double precision, on it narrowed. 0.3f32 is 0.30000001..: above
    // a high bound of 0.3 in double precision, on it narrowed. The values just beyond are out in both
    assert_eq!(chosen(vec![0.7, 0.69, 0.8], band(0.7, 1.0, 0.0))?, [1.0, 0.0, 1.0]);
    assert_eq!(chosen(vec![0.3, 0.31, 0.2], band(0.0, 0.3, 0.0))?, [1.0, 0.0, 1.0]);
    // a whole number and a half are exact: a band of one value chooses it, and not the values beside it
    assert_eq!(chosen(vec![0.5, 0.4, 0.6], band(0.5, 0.5, 0.0))?, [1.0, 0.0, 0.0]);
    Ok(())
}

#[test]
fn a_feather_is_narrowed_alike() -> Result<(), Error> {
    // the value at the bound is wholly in; one feather beyond it is wholly out; halfway, half
    let made = chosen(vec![0.7, 0.65, 0.6, 0.5], band(0.7, 1.0, 0.1))?;
    assert!((made[0] - 1.0).abs() < 1e-6 && (made[1] - 0.5).abs() < 1e-5 && made[2].abs() < 1e-5 && made[3] == 0.0);
    Ok(())
}
