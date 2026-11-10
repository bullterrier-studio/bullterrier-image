//! `gradient`: a colour gradient (linear, radial or conic), or one flat colour, in place of the picture. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Input, Meta};
use crate::ops::{gradient, ramp};
use crate::params::{Choice, Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `gradient` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Backgrounds,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// What `gradient` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Pixel
}

/// The layouts of a gradient.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Kind {
    /// The colours run in a straight band across the whole picture, at `angle`.
    Linear,
    /// Rings round the centre `x`, `y`, the last colour `radius` away.
    Radial,
    /// A clockwise sweep round the centre, starting at `angle`; the last colour meets the first at a hard seam.
    Conic,
}

/// Replace the picture with a gradient (linear, radial or conic), or with a flat colour fill (preset `fill`: black; set
/// colors for another); to replace the background: mask = "background".
///
/// A generated background: the input is used only for its height and width, its content is ignored. A gradient through
/// a list of colours, laid out as a straight band, rings round a centre or a sweep round it. Sizes are fractions of the
/// picture, so the look does not depend on its resolution. For example `colors` = "#12002b" and "#ff2bd6" with `kind` =
/// "radial" is a magenta glow on deep violet. A single colour is a flat fill.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The layout: 'linear' (colours run in a straight band across the whole picture, at `angle`), 'radial' (rings round
    /// the centre `x`, `y`, the last colour `radius` away) or 'conic' (a clockwise sweep round the centre, starting at
    /// `angle`; the last colour meets the first at a hard seam).
    #[param(default = Linear)]
    pub kind: Kind,
    /// The gradient's colours in order, evenly spaced; at least one. Linear: from the start of the band to its end.
    /// Radial: from the centre outwards. Conic: round the turn. One colour is a flat fill of the whole picture, and
    /// every other parameter is then ignored.
    #[param(default = ["#12002b", "#ff2bd6"], min_length = 1)]
    pub colors: Vec<Color>,
    /// Linear: the direction the colours run, measured clockwise on screen from the right: 0 = left to right, 90 = top
    /// to bottom. Conic: where the sweep starts, on the same scale (0 = right of the centre, 90 = below it). Ignored by
    /// 'radial'.
    #[param(default = 90.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// Horizontal position of the centre as a fraction of the width (0 = left edge, 1 = right edge). Used by 'radial' and
    /// 'conic'; 'linear' is always centred.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub x: f32,
    /// Vertical position of the centre as a fraction of the height (0 = top edge, 1 = bottom edge). Used by 'radial' and
    /// 'conic'; 'linear' is always centred.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub y: f32,
    /// Radial only: the distance from the centre at which the last colour is reached, as a fraction of the picture's
    /// shorter side; farther out stays on the last colour. Larger = a wider, softer glow. Ignored by 'linear' and
    /// 'conic'.
    #[param(default = 0.75, unit = fraction, min = 0.01, max = 4)]
    pub radius: f32,
    /// How many times the list of colours is fitted along the gradient (the band, the radius, one turn): 1 = once, 2 =
    /// twice as fast. Radial stays on the last colour beyond the radius.
    #[param(default = 1, unit = count, min = 1, max = 64)]
    pub repeat: i32,
    /// Run every second repeat backwards, so neighbouring repeats meet on the same colour instead of jumping from the
    /// last colour back to the first. Has no effect with `repeat` 1.
    #[param(default = false)]
    pub reflect: bool,
}

/// The position along the colours, 0..1, of a pixel at `t` (0..1) along the whole gradient when the colours are fitted
/// in `repeat` times (every second one backwards if `reflect`).
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "a repeat is at most 64, and a position 0..1 times it is a small non-negative number"
)]
fn along_colours(t: f32, repeat: i32, reflect: bool) -> f32 {
    let cycles = t * repeat as f32;
    // which repeat it is in; the very end belongs to the last one
    let run = (cycles as i32).min(repeat - 1);
    let within = cycles - run as f32;
    if reflect && run % 2 == 1 { 1.0 - within } else { within }
}

/// The gradient, in the picture's size: the picture is used only for its height and width.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let width = picture.width();
    // the gradient spans the canvas, of which the picture holds the rows from `first_row`
    let place = inputs.place(picture);
    let canvas = (place.canvas_height, place.canvas_width);
    let rows = place.first_row..place.first_row + picture.height();
    let (x, y) = (f64::from(params.x), f64::from(params.y));
    let t = match params.kind {
        Kind::Linear => gradient::linear(canvas, rows, f64::from(params.angle)),
        Kind::Radial => gradient::radial(canvas, rows, x, y, f64::from(params.radius)),
        Kind::Conic => gradient::conic(canvas, rows, x, y, f64::from(params.angle)),
    };
    let stops: Vec<[f32; 3]> = params.colors.iter().map(|colour| colour.0).collect();
    picture.map_at(|row, column, _| {
        ramp::at(
            &stops,
            along_colours(t[row * width + column], params.repeat, params.reflect),
        )
    })
}
