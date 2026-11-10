//! Automatic quality checks: what a person would otherwise have to look at to spot a bad result. Each returns plain-English
//! warnings (none: fine). A check never fails and never blocks an export; the report shows them, so a batch of hundreds of
//! photos needs a person only for the ones that are flagged.

use crate::ops::length::float;
use crate::{Image, Layer};

/// Warns if the subject cut-out covers an implausible share of the photo (it missed the subject, or took it all): the
/// share of pixels at 0.5 or more, against `(low, high)`.
#[must_use]
pub fn matte_warnings(matte: &Layer, (low, high): (f64, f64)) -> Vec<String> {
    let values = matte.values();
    let covered = values.iter().filter(|&&value| value >= 0.5).count();
    let share = float(covered) / float(values.len().max(1));
    if share < low {
        vec![format!(
            "the cut-out covers only {:.1}% of the photo: the subject may have been missed",
            share * 100.0
        )]
    } else if share > high {
        vec![format!(
            "the cut-out covers {:.1}% of the photo: there is almost no background to separate",
            share * 100.0
        )]
    } else {
        Vec::new()
    }
}

/// The level below which the brightest value makes a picture blank, and the mean and the spread above and below which it is
/// almost white or flat.
const BLANK: f64 = 0.02; // constant: the thresholds of the check, tuned on broken results
const WHITE: f64 = 0.97; // constant: a mean this high is almost entirely white
const FLAT: f64 = 0.01; // constant: a spread this low is a flat colour with no detail

/// Warns about a result that is empty, blown out or without any detail (an [`Image`] holds only numbers, so none is broken). Only the colour is judged: a picture with
/// transparency is judged by its three colour channels.
#[must_use]
pub fn still_warnings(image: &Image) -> Vec<String> {
    let colour = image.as_picture().values();
    let count = float(colour.len().max(1));
    let (mut brightest, mut sum) = (f64::NEG_INFINITY, 0.0);
    for &value in colour {
        brightest = brightest.max(f64::from(value));
        sum += f64::from(value);
    }
    let mean = sum / count;
    let spread = (colour
        .iter()
        .map(|&value| (f64::from(value) - mean).powi(2))
        .sum::<f64>()
        / count)
        .sqrt();
    if brightest < BLANK {
        vec!["the result is blank (all black)".to_owned()]
    } else if mean > WHITE {
        vec!["the result is almost entirely white".to_owned()]
    } else if spread < FLAT {
        vec!["the result is a flat colour with no detail".to_owned()]
    } else {
        Vec::new()
    }
}
