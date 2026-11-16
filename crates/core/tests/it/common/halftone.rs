//! What the halftone tests share: the Murray-Davies mix of inks and the mean colour a screen prints.

use bullterrier_image::Error;
use bullterrier_image::effects::halftone::Params;

use crate::common::pixels::run_picture;

/// The colour of `paper` under inks of `colours` at screen areas `areas`: a pixel is inked where its screens overlap, the
/// screens do not line up, so each channel is `paper * prod (1 - area * (1 - ink / paper))` (Murray-Davies per ink).
pub(crate) fn mixed(paper: [f32; 3], colours: &[[f32; 3]], areas: &[f32]) -> [f32; 3] {
    [0, 1, 2].map(|c| {
        colours.iter().zip(areas).fold(paper[c], |light, (ink, area)| {
            light * (1.0 - area * (1.0 - (ink[c] / paper[c]).min(1.0)))
        })
    })
}

/// The mean colour of `params` printing a flat `rgb` over 240 x 240 px.
#[expect(clippy::cast_precision_loss, reason = "small picture sizes")]
pub(crate) fn mean_colour(params: &Params, rgb: [f32; 3]) -> Result<[f32; 3], Error> {
    let out = run_picture(params, 240, 240, &rgb.repeat(240 * 240))?;
    let pixels = out.as_chunks::<3>().0;
    Ok([0, 1, 2].map(|c| pixels.iter().map(|pixel| pixel[c]).sum::<f32>() / pixels.len() as f32))
}
