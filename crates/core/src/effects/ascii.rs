//! `ascii`: the picture redrawn as a grid of text characters. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::textart::{Style, text_art};
use crate::params::{Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `ascii` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Text,
    opacity: 1.0,
    inputs: &[],
};

/// What `ascii` reads around a pixel: the grid of characters is anchored at the picture's corner, so a row is not the same
/// in another frame of reference.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Redraw the picture as text: a grid of characters `cell` px wide, each chosen by the brightness under it.
///
/// Each character covers one cell of the grid and is picked from `chars` by how much ink it puts down, so the picture's
/// tones survive as denser and lighter characters. The glyphs are drawn from one bundled monospaced font (`JetBrains Mono`
/// Regular), never a system font, so a look is the same on every platform.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Width of one character, 1 to 200 px; the height follows from the font. Smaller = finer detail, more characters.
    #[param(default = 10, unit = px, min = 1, max = 200)]
    pub cell: i32,
    /// The character palette, in any order, at least two different characters. It is sorted by how much ink each glyph puts
    /// down, so the densest is used for the brightest areas on dark paper and the darkest on light paper (see `paper`). Try
    /// ' .:-=+*#%@', ' .oO0@', '01', or blocks like ' ░▒▓█': the bundled font has the Latin letters and the block elements,
    /// not braille.
    #[param(default = " .:-=+*#%@", min_length = 2, max_length = 128, min_distinct = 2)]
    pub chars: String,
    /// The ink colour of every character ('#rrggbb', '#rgb' or [r, g, b] 0..1); none (the default) colours each character
    /// from the average colour of the photo under it, brightened so thin strokes stay readable: scaled until its strongest
    /// channel is full, by at most 1 / `ink_floor` (never darkened).
    #[param()]
    pub ink: Option<Color>,
    /// With no `ink`: the most a character's colour is brightened, as a floor 0.01 to 1 on its strongest channel: a colour
    /// whose strongest channel is below it is scaled by 1 / `ink_floor` instead of up to full, so dark areas stay dark.
    /// Lowering it brightens dark colours more (and their noise); 1 = colours as they are.
    #[param(default = 0.35, unit = fraction, min = 0.01, max = 1)]
    pub ink_floor: f32,
    /// The paper colour behind the characters. Dark paper (Rec. 709 luminance below `paper_threshold`) puts dense characters
    /// on bright areas; lighter paper puts them on dark areas.
    #[param(default = "#000000")]
    pub paper: Color,
    /// The Rec. 709 luminance, 0 to 1, below which `paper` counts as dark (dense characters on bright areas) rather than
    /// light (dense characters on dark areas); 0.5 = mid grey. Raising it lets lighter paper count as dark: 0 treats every
    /// paper as light, 1 every paper but white as dark.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub paper_threshold: f32,
    /// Exponent on the amount of ink each character puts down, 0.1 to 10: 1 = linear, above 1 lighter midtones (less ink),
    /// below 1 more ink.
    #[param(default = 1.0, unit = ratio, min = 0.1, max = 10)]
    pub gamma: f32,
    /// How much of the original picture shows through the paper behind the characters, 0 to 1: 0 = text on flat `paper`
    /// (fine detail is lost to the grid), 1 = text laid over the untouched photo.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1)]
    pub photo: f32,
}

/// The picture redrawn in characters. The bundled font is read here for the cell grid, so a font that cannot be read (which
/// a test excludes) leaves the picture as it is.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let style = Style {
        cell: usize::try_from(params.cell).unwrap_or(1),
        chars: &params.chars,
        ink: params.ink.map(|ink| ink.0),
        ink_floor: params.ink_floor,
        paper: params.paper.0,
        paper_threshold: params.paper_threshold,
        gamma: params.gamma,
        photo: params.photo,
    };
    text_art(picture, &style).unwrap_or_else(|_| picture.map(|pixel| pixel))
}
