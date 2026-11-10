//! Colour as effects meet it: perceived brightness, and a sampled colour made vivid.

/// The Rec. 709 weights of red, green and blue in perceived brightness.
const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722]; // constant: Rec. 709 weights of red, green, blue in brightness

/// The perceived brightness of `pixel` (Rec. 709 weights), 0..1 for channels in 0..1.
pub(crate) fn luminance(pixel: [f32; 3]) -> f32 {
    pixel[0] * LUMA[0] + pixel[1] * LUMA[1] + pixel[2] * LUMA[2]
}

/// `colour`, sampled from a photo, brightened so thin marks drawn in it stay readable (never darkened): scaled until its
/// strongest channel is full, but by at most 1 / `floor` (`floor` in 0..=1, above 0): a colour whose strongest channel is
/// below `floor` is scaled by 1 / `floor`, so near-black stays dark. A floor of 1 leaves every colour as it is.
pub(crate) fn vivid(colour: [f32; 3], floor: f32) -> [f32; 3] {
    let strongest = colour[0].max(colour[1]).max(colour[2]).max(floor);
    colour.map(|level| (level / strongest).clamp(0.0, 1.0))
}
