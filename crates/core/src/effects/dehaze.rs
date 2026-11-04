//! `dehaze`: haze and fog removed with the dark channel prior (He, Sun and Tang 2009). What it does is documented once,
//! on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::color::luminance;
use crate::ops::edge::Edge;
use crate::ops::guided::guided_filter;
use crate::ops::length::{float, whole};
use crate::ops::rank::minimum_filter;
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `dehaze` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// The smallest airlight channel used as a divisor, so a black picture (airlight 0) cannot divide by zero; it changes no
/// picture with a visible sky (a channel below 1/1000 is black in any 8-bit file).
const MIN_AIRLIGHT: f32 = 1e-3; // constant: below one 8-bit level, so no visible picture is changed

/// Remove haze and fog by inverting the atmospheric scattering model I = J t + A (1 - t) (He, Sun, Tang 2009).
///
/// The dark channel of a haze-free outdoor picture is nearly 0: in any patch of `patch` x `patch` px some pixel has a
/// channel near black. Haze lifts it, so the dark channel of I / A measures 1 - t, the haze at that place. The steps
/// are
/// - the airlight A: the `airlight_fraction` of pixels with the highest dark channel (the haziest) are looked at, and
///   the colour of the brightest of them (largest red + green + blue) is A, a per channel colour;
/// - the transmission t = 1 - `amount` x dark channel of I / A, refined by a guided filter (`radius`,
///   `edge_threshold`), guided by the picture's luma, so it follows the edges of the picture, then held between
///   `transmission_floor` and 1;
/// - the scene J = (I - A) / t + A, held to 0..1.
///
/// A picture without haze (a dark channel of 0) comes out as it went in. `edge` says what the patch search sees beyond
/// the picture's border. Colours are those of the sRGB-encoded picture.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How much of the haze is taken out (He, Sun and Tang's omega): 0 = off, 1 = all of it that the dark channel
    /// finds, which also flattens real distance cues; a little below 1 keeps the picture natural.
    #[param(default = 0.9, unit = fraction, min = 0, max = 1)]
    pub amount: f32,
    /// Side of the square patch in which the darkest value is taken to find the haze, in px at the recipe's size (an
    /// even size is centred half a pixel off). Larger patches estimate the haze more reliably but blur it over bigger
    /// areas and miss thin haze; 1 judges every pixel by its own darkest channel.
    #[param(default = 15, unit = px, min = 1, max = 201)]
    pub patch: i32,
    /// Radius of the guided filter that refines the haze estimate to follow the picture's edges, in px at the recipe's
    /// size (rounded to whole px). Larger gives a smoother haze with fewer halos far from edges; 0 keeps the blocky
    /// patch estimate.
    #[param(default = 30.0, unit = px, min = 0, max = 500)]
    pub radius: f32,
    /// The brightness step (0..1, Rec. 709 luma) that counts as an edge the refined haze estimate must not blur
    /// across: steps above it keep the haze of their own side, steps below it are smoothed. Raising it gives a
    /// smoother estimate and halos around edges; lowering it follows ever smaller edges.
    #[param(default = 0.03, unit = fraction, min = 0.001, max = 1, advanced)]
    pub edge_threshold: f32,
    /// The lowest transmission (He, Sun and Tang's t0), the share of a pixel's light that reaches the camera: colours
    /// are stretched by at most 1 / `transmission_floor`, so a low floor clears thick haze but amplifies noise and
    /// clips; 1 allows no stretch at all.
    #[param(default = 0.1, unit = fraction, min = 0.01, max = 1, advanced)]
    pub transmission_floor: f32,
    /// The share of the picture's pixels, those with the highest dark channel (the haziest), among which the brightest
    /// one is taken as the colour of the haze (He, Sun and Tang use 0.1 percent = 0.001; at least one pixel is always
    /// used). Raising it makes the estimate less sensitive to a single odd pixel but can pick a bright object instead of
    /// the sky.
    #[param(default = 0.001, unit = fraction, min = 0.0001, max = 0.1)]
    pub airlight_fraction: f32,
    /// What the darkest-value search sees beyond the picture's edge, which sets the haze estimate along the border
    /// (dehaze filters by local darkest values rather than warping, so the default is 'nearest', not 'reflect'):
    /// 'nearest' (the edge pixel repeated) and 'reflect' (mirrored, edge pixel repeated) look only at pixels of the
    /// patch inside the picture and give the same result; 'mirror' (mirrored about the edge pixel) also looks at pixels
    /// up to half a patch from the border; 'wrap' (the opposite edge) lets the far side's darkest values set the haze
    /// at the border.
    #[param(default = Nearest, advanced)]
    pub edge: Edge,
}

/// The smallest channel of each pixel after it is divided by `light`.
fn darkest_channel(picture: &Picture<'_>, light: [f32; 3]) -> Vec<f32> {
    picture
        .pixels()
        .iter()
        .map(|pixel| (pixel[0] / light[0]).min(pixel[1] / light[1]).min(pixel[2] / light[2]))
        .collect()
}

/// The colour of the haze: that of the brightest (largest red + green + blue) of the `fraction` of the pixels, at least
/// one, with the highest `dark` channel. The pixels tied at the lowest dark value taken are chosen in reading order, so
/// the choice does not depend on how a sort orders ties, and the first of the brightest wins.
fn airlight(picture: &Picture<'_>, dark: &[f32], fraction: f32) -> [f32; 3] {
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a count of pixels")]
    let count = ((f64::from(fraction) * float(dark.len())) as usize).max(1);
    let mut sorted = dark.to_vec();
    let (_, &mut threshold, _) = sorted.select_nth_unstable_by(dark.len() - count, f32::total_cmp);
    let above = (0..dark.len()).filter(|&at| dark[at] > threshold);
    let tied = (0..dark.len())
        .filter(|&at| dark[at].total_cmp(&threshold).is_eq())
        .take(count);
    let pixels = picture.pixels();
    let sum = |at: &usize| pixels[*at][0] + pixels[*at][1] + pixels[*at][2];
    let mut haziest = above.chain(tied).take(count);
    let first = haziest.next().unwrap_or(0);
    let brightest = haziest.fold(first, |best, at| if sum(&at) > sum(&best) { at } else { best });
    pixels[brightest].map(|channel| channel.max(MIN_AIRLIGHT))
}

/// The picture with its haze removed.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let (patch, edge) = (whole(f64::from(params.patch)), params.edge.extension());

    let dark = minimum_filter(&darkest_channel(picture, [1.0; 3]), height, width, patch, edge);
    let light = airlight(picture, &dark, params.airlight_fraction);

    // the transmission from the dark channel of the picture over the airlight, refined along the picture's edges
    let dark_over_light = minimum_filter(&darkest_channel(picture, light), height, width, patch, edge);
    let rough: Vec<f32> = dark_over_light.iter().map(|&dark| 1.0 - params.amount * dark).collect();
    let luma: Vec<f32> = picture.pixels().iter().map(|&pixel| luminance(pixel)).collect();
    let epsilon = f64::from(params.edge_threshold).powi(2);
    let refined = guided_filter(&luma, &rough, height, width, whole(f64::from(params.radius)), epsilon);

    picture.map_at(|row, column, pixel| {
        let transmission = refined[row * width + column].clamp(params.transmission_floor, 1.0);
        [0, 1, 2].map(|channel| ((pixel[channel] - light[channel]) / transmission + light[channel]).clamp(0.0, 1.0))
    })
}
