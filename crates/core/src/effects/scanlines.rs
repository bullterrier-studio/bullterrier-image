//! `scanlines`: dark horizontal lines laid over the picture at a regular spacing, like a CRT screen. What it does is
//! documented once, on `Params`.

use std::f64::consts::PI;

use serde::Deserialize;

use crate::effect::{Category, Input, Meta};
use crate::params::Params;
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `scanlines` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Texture,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// What `scanlines` reads around a pixel.
pub(crate) fn footprint(_params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Pixel
}

/// Dark horizontal lines every `spacing` px, each `thickness` x `spacing` thick, darkening by `darkness`.
///
/// The lines start at row `offset` and repeat every `spacing` px. A pixel row is multiplied by 1 - `darkness` x the
/// share of the row a line covers, so whole rows under a line are exactly 1 - `darkness` times the picture and rows no
/// line touches are unchanged; the same gain applies to every column and channel.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Distance in px from the top of one line to the top of the next. A spacing that is not a whole number drifts
    /// against the pixel rows and the lines are anti-aliased. Larger = fewer, wider-set lines.
    #[param(default = 3.0, unit = px, min = 1, max = 2000)]
    pub spacing: f32,
    /// How thick each line is, as a fraction of `spacing`: 0 = no line (the picture is unchanged), 0.5 = lines and gaps
    /// equally wide, 1 = the lines touch and the whole picture is darkened alike. A line that covers only part of a
    /// pixel row darkens that row in proportion.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub thickness: f32,
    /// How much a line darkens what is under it: 0 = nothing (the picture is unchanged), 0.35 is a visible screen, 1 =
    /// black lines. A line multiplies the picture by 1 - darkness, so colours keep their ratios.
    #[param(default = 0.35, unit = fraction, min = 0, max = 1)]
    pub darkness: f32,
    /// How far down in px the first line starts from the top of the picture (the middle of its soft edge, when
    /// `softness` is above 0). Offsets of more than `spacing` repeat the pattern: the lines are at offset + k x
    /// spacing.
    #[param(default = 0.0, unit = px, min = 0, max = 2000)]
    pub offset: f32,
    /// How softly a line fades in and out: 0 = hard edges, each row darkened by the share of it a line covers; 1 = as
    /// soft as the lines and gaps allow, so with a `thickness` of 0.5 the darkness swells and fades along sin^2,
    /// darkest in the middle of a line and nothing in the middle of a gap, as on a CRT. In between, each edge fades
    /// over `softness` x the narrower of a line and a gap, either side of the hard edge. It keeps the average
    /// darkening of the picture: only its shape changes.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1)]
    pub softness: f32,
}

/// The area under a line's profile (1 up to `plateau`, then a raised-cosine fall over 2 x `reach` to 0) from its middle
/// out to `away`, in periods.
fn half_area(away: f64, plateau: f64, reach: f64) -> f64 {
    let fall = (away - plateau).clamp(0.0, 2.0 * reach); // how far into the fall `away` is
    let share = if reach > 0.0 { fall / (2.0 * reach) } else { 0.0 }; // 0..1 across the fall
    // the fall is 0.5 (1 + cos(pi x share)), whose area from its start is fall / 2 + reach sin(pi x share) / pi
    away.min(plateau) + fall / 2.0 + reach / PI * (PI * share).sin()
}

/// The multiplier of each of the `count` rows from canvas row `first`.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "a row number is far below 2**52; the gain is applied to f32 pictures"
)]
fn gains(first: usize, count: usize, params: &Params) -> Vec<f32> {
    let (spacing, thickness, offset) = (
        f64::from(params.spacing),
        f64::from(params.thickness),
        f64::from(params.offset),
    );
    let half = thickness / 2.0; // a line's half width, in periods; its middle is that far after its start
    let reach = f64::from(params.softness) * half.min(0.5 - half); // each edge fades this far either side of the hard edge
    // Length of line above each row edge, counting from the first line: whole periods, plus the part of this period,
    // measured from the middle of the nearest line (the profile is the same either side of it).
    let above: Vec<f64> = (first..=first + count)
        .map(|edge| {
            let turns = (edge as f64 - offset) / spacing; // in periods from the first line
            let periods = (turns - half + 0.5).floor();
            let away = turns - half - periods;
            periods * thickness + half + away.signum() * half_area(away.abs(), half - reach, reach)
        })
        .collect();
    above
        .windows(2)
        .map(|edges| (1.0 - f64::from(params.darkness) * (edges[1] - edges[0]) * spacing) as f32)
        .collect()
}

/// The picture with its rows darkened.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    // the lines run from the top of the canvas, whatever rows of it the picture holds
    let gain = gains(inputs.place(picture).first_row, picture.height(), params);
    picture.map_at(|row, _, pixel| pixel.map(|value| value * gain[row]))
}
