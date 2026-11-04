//! `selective_color`: shift hue, saturation and lightness for one range of colours only. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::colorspace::{hsv_to_rgb, rgb_to_hsv};
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `selective_color` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Colour,
    opacity: 1.0,
    inputs: &[],
};

/// The smallest feather; only keeps a hard-edged range (feather 0) from dividing by zero, a pixel past the edge still
/// gets weight 0.
const NO_FEATHER: f32 = 1e-6; // constant: only guards the division, a hard edge is then exact to a millionth of a degree
const TURN: f32 = 360.0; // constant: degrees in a full turn of the colour wheel
const HALF_TURN: f32 = 180.0; // constant: degrees in half a turn of the colour wheel

/// Shift the hue, saturation and lightness of the colours in one hue range, leaving every other pixel untouched; a range
/// as wide as the wheel is a hue shift.
///
/// A range of hues (a centre, a width and a feather) and the changes made to the colours inside it. Works in HSV on the
/// gamma-encoded values. A pixel's weight is its place in the range (1 within `width` of the `hue` centre, falling
/// linearly to 0 over `feather` degrees; the wheel wraps round) times its HSV saturation, so a grey, which has no hue,
/// is never selected and a pale colour is changed less than a vivid one. Pixels of weight 0 come back bit for bit. A
/// selected pixel moves by weight times each change: `hue_shift` degrees, saturation times `saturation` and value
/// toward black or full by `lightness`. Saturation and value stay inside 0..1 by construction, so the result needs no
/// clipping.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Centre of the range on the colour wheel, 0..360 degrees: 0 red, 60 yellow, 120 green, 180 cyan, 240 blue, 300
    /// magenta; 360 is red again.
    #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub hue: f32,
    /// Full width of the range, 0..360 degrees, centred on `hue`; colours inside it are changed at full strength. 360
    /// takes in every colour; raising it widens the range.
    #[param(default = 60.0, unit = degrees, min = 0, max = 360)]
    pub width: f32,
    /// How far beyond each edge of the range the change fades out, 0..180 degrees: the weight falls in a straight line
    /// from full at the edge to none that far away. 0 gives a hard edge; raising it blends the range into its
    /// neighbours.
    #[param(default = 30.0, unit = degrees, min = 0, max = 180)]
    pub feather: f32,
    /// How far the selected hues are turned round the colour wheel, -180..180 degrees; positive goes red toward yellow,
    /// green, blue. 0 keeps the hue. With `width` 360 and `feather` 0 every colour turns (preset `hue_shift`).
    #[param(default = 0.0, unit = degrees, min = -180, max = 180, drivable)]
    pub hue_shift: f32,
    /// Gain on the saturation of the selected colours: 1 keeps it, 0 turns them grey, above 1 makes them more vivid
    /// (stopping at full saturation).
    #[param(default = 1.0, unit = ratio, min = 0, max = 3)]
    pub saturation: f32,
    /// Brightness (HSV value) of the selected colours, -1..1: -1 takes them all the way to black, +1 all the way to the
    /// full value, 0 keeps it; a value of 0.5 closes half the gap in that direction.
    #[param(default = 0.0, unit = fraction, min = -1, max = 1)]
    pub lightness: f32,
}

/// The picture with the colours of the range changed.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    picture.map(|pixel| {
        let [hue, saturation, value] = rgb_to_hsv(pixel);
        let degrees = hue * TURN;
        let distance = ((degrees - params.hue + HALF_TURN).rem_euclid(TURN) - HALF_TURN).abs();
        let outside_by = distance - params.width / 2.0;
        let weight = (1.0 - outside_by / params.feather.max(NO_FEATHER)).clamp(0.0, 1.0) * saturation;
        if weight <= 0.0 {
            return pixel;
        }
        // room to move toward full value, or toward black
        let gap = if params.lightness > 0.0 { 1.0 - value } else { value };
        hsv_to_rgb([
            (degrees + weight * params.hue_shift) / TURN,
            (saturation * (1.0 + weight * (params.saturation - 1.0))).clamp(0.0, 1.0),
            value + weight * params.lightness * gap,
        ])
    })
}
