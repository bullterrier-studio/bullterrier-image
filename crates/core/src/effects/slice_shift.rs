//! `slice_shift`: horizontal strips of the picture slid sideways, wrapping around. What it does is documented once, on
//! `Params`.

use serde::Deserialize;

use crate::effect::{Category, Input, Meta};
use crate::ops::random::Rng;
use crate::ops::shift::{Band, slide_bands};
use crate::params::Params;
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `slice_shift` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// What `slice_shift` reads around a pixel: a row reads only its own row; the strips and bands are those of the canvas.
pub(crate) fn footprint(_params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Pixel
}

/// Glitch tearing: the picture cut into strips and bands that slide sideways by random distances, wrapping around.
///
/// Cut the picture into horizontal strips at random rows and slide each sideways by a random distance, then slide `bands`
/// more strips of random height placed at random rows. A strip that slides out of the picture on one side comes back in
/// on the other (it wraps around), and the strip moves by whole pixels, so every row keeps exactly the pixels it had, only
/// in a different order. The cuts, the bands and the slides depend only on `seed`, `slices`, `bands`, `amount` and the
/// picture's size.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How many horizontal strips the whole picture is cut into, 0 to 500 (never more than it has rows). The cuts fall at
    /// random rows, so the strips differ in height; 1 slides the whole picture as one; 0 = no tear, only the bands move.
    /// More strips make a finer, busier tear.
    #[param(default = 30, unit = count, min = 0, max = 500)]
    pub slices: i32,
    /// The furthest a strip or band slides, in px, 0 to 2000, tuned for a picture about 480 px high: each slides by a
    /// random whole number of pixels between that far left and that far right, so one may also stay put. 0 leaves the
    /// picture unchanged; the picture's width or more can slide one by a whole picture width.
    #[param(default = 24.0, unit = px, min = 0, max = 2000)]
    pub amount: f32,
    /// Random seed for where the cuts and the bands fall and how far each slides: the same seed always gives the same tear,
    /// any other seed gives a different one.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
    /// How many extra strips slide, 0 to 500, on top of the tear: each is placed at a random row, so they may overlap (a
    /// row in two slides by both) and the rest of the picture stays; 0 = none. A few bands with `slices` 0 make a
    /// corrupted video frame.
    #[param(default = 0, unit = count, min = 0, max = 500)]
    pub bands: i32,
    /// The lowest a band can be, in px, 0 to 2000, tuned for a picture about 480 px high (never below 1 px). Each band's
    /// height is drawn at random between this and `band_max_height`, which it may not exceed.
    #[param(default = 4.8, unit = px, min = 0, max = 2000, at_most = band_max_height)]
    pub band_min_height: f32,
    /// The tallest a band can be, in px, 0 to 2000, tuned for a picture about 480 px high. Raising it gives taller bands.
    #[param(default = 28.8, unit = px, min = 0, max = 2000)]
    pub band_max_height: f32,
}

/// The strips of a picture `height` rows high: cut at `strips` - 1 random rows, each with a random whole slide of at most
/// `furthest` px either way.
fn strips(rng: &mut Rng, height: usize, count: usize, furthest: usize) -> Vec<Band> {
    let cuts = rng.sample(height - 1, count.min(height) - 1);
    // every strip's first row but the top's is one past a cut
    let tops: Vec<usize> = std::iter::once(0).chain(cuts.iter().map(|cut| cut + 1)).collect();
    let ends = tops.iter().skip(1).copied().chain(std::iter::once(height));
    let mut bands = Vec::with_capacity(tops.len());
    for (top, end) in tops.iter().copied().zip(ends) {
        #[expect(clippy::cast_possible_wrap, reason = "a slide is at most 2000 px")]
        let distance = rng.below(2 * furthest + 1) as i64 - furthest as i64;
        bands.push(Band {
            top,
            rows: end - top,
            distance,
        });
    }
    bands
}

/// A band drawn at random: a height between `lowest` and `tallest` (at least 1, at most the picture's), a row for its top
/// that keeps it inside the picture, and a slide of up to `amount` px either way, cut to whole pixels toward 0.
fn band(rng: &mut Rng, height: usize, lowest: f64, tallest: f64, amount: f64) -> Band {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a height in 0..=2000 * ratio, cut to whole px"
    )]
    let rows = ((lowest + rng.uniform() * (tallest - lowest)) as usize).clamp(1, height);
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a row of the picture"
    )]
    let top = (rng.uniform() * (height - rows) as f64) as usize;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a slide of at most 2000 px, cut toward 0"
    )]
    let distance = (-amount + rng.uniform() * 2.0 * amount) as i64;
    Band { top, rows, distance }
}

/// The picture with strips and bands slid sideways.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let place = inputs.place(picture);
    // the strips and bands (at most 500 of each, the parameters' maximum) are those of the canvas, whichever rows are drawn
    let canvas = place.canvas_height;
    let mut rng = Rng::seeded(params.seed);
    let mut moves = Vec::new();
    if params.slices > 0 {
        #[expect(
            clippy::cast_sign_loss,
            clippy::cast_possible_truncation,
            reason = "positive, and a slide of at most 2000 px"
        )]
        let (count, furthest) = (
            params.slices as usize,
            f64::from(params.amount).round_ties_even() as usize,
        );
        moves = strips(&mut rng, canvas, count, furthest);
    }
    let (lowest, tallest, amount) = (
        f64::from(params.band_min_height),
        f64::from(params.band_max_height),
        f64::from(params.amount),
    );
    for _ in 0..params.bands {
        moves.push(band(&mut rng, canvas, lowest, tallest, amount));
    }
    Image::from_pixels(
        height,
        width,
        slide_bands(picture.pixels(), (height, width), place.first_row, &moves),
    )
}
