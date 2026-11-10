//! `stretch`: one row or column of the picture repeated over the lines beside it. What it does is documented once, on
//! `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::params::{Choice, Params};
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `stretch` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// What `stretch` reads around a pixel.
pub(crate) fn footprint(_params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Whole
}

/// Which way the line is smeared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum Axis {
    /// One pixel row is repeated up and down over the rows beside it: streaks run down the picture.
    Vertical,
    /// One pixel column is repeated left and right over the columns beside it: streaks run across the picture.
    Horizontal,
}

/// Which sides of the line are smeared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum Side {
    /// Outward each way.
    Both,
    /// Down, or right.
    After,
    /// Up, or left.
    Before,
}

/// A pixel stretch: one line of the picture repeated over the lines beside it, the way a glitched file smears.
///
/// Repeat one row (or column) of the picture over the `span` of rows (columns) beside it. The line sits at `at` and keeps
/// its place; the lines within `span` of it on the chosen `side` become copies of it, the rest of the picture is
/// untouched. Every output pixel is an input pixel, so no colour is invented.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Which way the line is smeared: 'vertical' repeats one pixel row up and down over the rows beside it (streaks run
    /// down the picture), 'horizontal' repeats one pixel column left and right (streaks run across).
    #[param(default = Vertical)]
    pub axis: Axis,
    /// Where the line that is smeared lies, as a fraction 0 to 1 of the way across the picture (down it for 'vertical',
    /// right for 'horizontal'): 0 = the first row or column, 0.5 = the middle, 1 = the last. Moving it chooses which
    /// colours the smear carries.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub at: f32,
    /// How far the line is smeared, in px, 0 to 2000, tuned for a picture about 480 px high, on each side that `side`
    /// names, rounded to whole pixels. 0 = nothing is smeared; the picture's height (width for 'horizontal') or more = the
    /// line covers the whole picture. Raising it hides more of the picture behind the smear.
    #[param(default = 72.0, unit = px, min = 0, max = 2000)]
    pub span: f32,
    /// Which sides of the line are smeared: 'both' (outward each way), 'after' (down, or right) or 'before' (up, or left).
    /// The other side keeps the picture as it is.
    #[param(default = Both)]
    pub side: Side,
}

/// The line `from` reads for each of `size` lines along the stretched axis: itself, or the stretched line when it lies
/// within `reach` lines of it on a smeared side.
fn sources(size: usize, params: &Params) -> Vec<usize> {
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    let last = (size - 1) as f64;
    // rounded half to even, as NumPy and Python round
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "0..=size - 1 after the rounding"
    )]
    let line = (f64::from(params.at) * last).round_ties_even() as usize;
    #[expect(clippy::cast_possible_truncation, reason = "a span of at most a few thousand px")]
    let reach = f64::from(params.span).round_ties_even() as i64;
    (0..size)
        .map(|place| {
            #[expect(clippy::cast_possible_wrap, reason = "a picture side is far below 2**63")]
            let offset = place as i64 - line as i64;
            let before = params.side != Side::After && offset < 0 && offset >= -reach;
            let after = params.side != Side::Before && offset > 0 && offset <= reach;
            if before || after { line } else { place }
        })
        .collect()
}

/// The picture with the line smeared.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let vertical = params.axis == Axis::Vertical;
    let from = sources(if vertical { height } else { width }, params);
    let pixels = picture.pixels();
    picture.map_at(|row, column, _| {
        if vertical {
            pixels[from[row] * width + column]
        } else {
            pixels[row * width + from[column]]
        }
    })
}
