//! `auto_levels`: stretch the picture to the full tonal range, like Photoshop's Auto Levels. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::tone::Remap;
use crate::params::{Choice, Params};
use crate::{Image, Inputs, Picture};

/// Where `auto_levels` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Colour,
    opacity: 1.0,
    inputs: &[],
};

/// What `auto_levels` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// What is stretched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Mode {
    /// One black and one white point over all channels together, so colours keep their relationships.
    Linked,
    /// Red, green and blue each get their own points, which also removes a colour cast.
    PerChannel,
}

/// Stretch the tones so the darkest values become black and the brightest white, ignoring `clip` at each end.
///
/// The black and white points are read from the picture itself: the `clip` share of the darkest values and of the
/// brightest values (counted per channel, or over all channels together when `mode` is `linked`) end up saturated,
/// and the rest is spread linearly between. A channel with no range (every value equal) is left as it is.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The share of values (0..0.45) that end up saturated at each end, pure black and pure white: 0.005 is 0.5
    /// percent, 0 stretches exactly from the darkest to the brightest value. Raising it gives more contrast and ignores
    /// stray dark or bright pixels, at the cost of detail in the extremes.
    #[param(default = 0.005, unit = fraction, min = 0, max = 0.45)]
    pub clip: f32,
    /// What is stretched: `per_channel` stretches red, green and blue separately, which also removes a colour cast but
    /// shifts colours (Photoshop's Enhance Per Channel Contrast); `linked` finds one black and one white point over all
    /// channels together and stretches them alike, so colours keep their relationships (Enhance Monochromatic
    /// Contrast).
    #[param(default = PerChannel)]
    pub mode: Mode,
}

/// The stretched picture.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let remaps = match params.mode {
        Mode::PerChannel => {
            [0, 1, 2].map(|channel| stretch(picture.pixels().iter().map(|pixel| pixel[channel]), params.clip))
        }
        Mode::Linked => [stretch(picture.values().iter().copied(), params.clip); 3],
    };
    picture.map(|pixel| [0, 1, 2].map(|channel| remaps[channel].apply(pixel[channel])))
}

/// The remap that sends the `clip` share of `values` at each end to black and white and spreads the rest between; none
/// (the identity) if the values have no range.
fn stretch(values: impl Iterator<Item = f32>, clip: f32) -> Remap {
    let mut ordered: Vec<f32> = values.collect();
    let count = ordered.len();
    let cut = dropped(clip, count);
    let high = count - 1 - cut;
    let (below, white, _) = ordered.select_nth_unstable_by(high, f32::total_cmp);
    let white = *white;
    let black = if cut < high {
        *below.select_nth_unstable_by(cut, f32::total_cmp).1
    } else {
        white
    };
    if white <= black {
        Remap::new(0.0, 1.0, 1.0, 0.0, 1.0)
    } else {
        Remap::new(black, white, 1.0, 0.0, 1.0)
    }
}

/// How many values are dropped at each end of `count` values for a `clip` share: whole values only.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "a share (0..0.45) of a count is a count; a picture has far fewer than 2**52 values"
)]
fn dropped(clip: f32, count: usize) -> usize {
    (f64::from(clip) * count as f64) as usize
}
