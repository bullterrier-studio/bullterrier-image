//! `blend`: a second picture laid over the photo with a blend mode. What it does is documented once, on `Params`.

#![expect(
    clippy::doc_markdown,
    reason = "the choices 'soft_light', 'top_left'... are spelt as the user writes them, in the help text"
)]

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::blend;
use crate::ops::place::{self, Anchor, Fit, Placement, Resample};
use crate::params::{Params, PictureName};
use crate::{BlendMode, Footprint, Image, Inputs, Picture};

/// Where `blend` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Compositing,
    opacity: 1.0,
    inputs: &[],
};

/// What `blend` reads around a pixel: the picture is placed by the photo's size, so any pixel can depend on the whole.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Lay a second picture over the photo with a blend mode (overlay a logo, double exposure), fitted by cover, contain,
/// stretch, width or tile.
///
/// The picture's own transparency is respected: where it is transparent the photo shows. Without a picture the photo is
/// left as it is.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The picture laid over the photo (a texture, a light leak, a logo): the path of an image file in a recipe file
    /// (relative to the file) or in `fx`, or, in Python, a Picture or an array; in Rust, the name of a picture the photo
    /// was given or the path of an image file. Its own transparency is respected. None leaves the photo as it is.
    #[param()]
    pub image: PictureName,
    /// How the picture combines with the photo, as the W3C Compositing and Blending Level 1 modes: 'normal' shows the
    /// picture, 'multiply' darkens, 'screen' lightens, 'overlay' deepens contrast by the photo's tones, 'soft_light' does
    /// so gently by the picture's, 'difference' subtracts, 'color' gives the photo the picture's hue and saturation and
    /// keeps its brightness. Mid grey leaves the photo unchanged under 'overlay' and 'soft_light'.
    #[param(default = Multiply)]
    pub mode: BlendMode,
    /// How the picture is fitted to the photo: 'cover' fills it keeping the aspect and crops the overhang, 'contain' fits
    /// inside it keeping the aspect (the rest of the photo is untouched), 'stretch' fills it ignoring the aspect, 'width'
    /// keeps the aspect and makes the picture as wide as the photo times `scale` (a logo or sticker), 'tile' repeats that
    /// picture over the photo.
    #[param(default = Cover)]
    pub fit: Fit,
    /// Where the fitted picture sits, or which tile sits there: 'top_left', 'top', 'top_right', 'left', 'center',
    /// 'right', 'bottom_left', 'bottom' or 'bottom_right'. It decides what 'cover' crops. Needs a picture.
    #[param(default = Center)]
    pub at: Anchor,
    /// The fitted picture's size as a multiple of the size `fit` gives it: 1 as fitted, 0.5 half as wide and tall. With
    /// 'width' it is the picture's width as a share of the photo's (0.2 a fifth); with 'tile', 0.25 puts four tiles across
    /// the photo. Needs a picture.
    #[param(default = 1.0, unit = ratio, min = 0.01, max = 2)]
    pub scale: f32,
    /// Space kept clear around the fitted picture on every side, 0 to 216 px (never more than leaves a pixel of the
    /// shorter side). Raising it shrinks what 'cover', 'contain' and 'stretch' fill and moves a smaller picture at an edge
    /// or corner inward. Needs a picture.
    #[param(default = 0.0, unit = px, min = 0, max = 216)]
    pub margin: f32,
    /// The filter that sizes the picture: 'nearest' keeps hard pixels, 'bilinear' is soft, 'bicubic' is sharper, 'lanczos'
    /// is sharpest and the best for shrinking. Needs a picture.
    #[param(default = Lanczos, advanced)]
    pub resample: Resample,
}

/// The photo with the picture the step names (`image`) laid over it; the photo itself without one.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let Some(asset) = inputs.picture("image") else {
        return picture.map(|pixel| pixel);
    };
    let (height, width) = (picture.height(), picture.width());
    let placed = place::place(
        asset,
        (height, width),
        &Placement {
            fit: params.fit,
            at: params.at,
            scale: f64::from(params.scale),
            margin: f64::from(params.margin),
            resample: params.resample,
        },
    );
    blend::lay_rgba(picture, &placed, params.mode)
}
