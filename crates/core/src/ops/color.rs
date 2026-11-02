//! Colour as effects meet it: perceived brightness.

/// The Rec. 709 weights of red, green and blue in perceived brightness.
const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722]; // constant: Rec. 709 weights of red, green, blue in brightness

/// The perceived brightness of `pixel` (Rec. 709 weights), 0..1 for channels in 0..1.
pub(crate) fn luminance(pixel: [f32; 3]) -> f32 {
    pixel[0] * LUMA[0] + pixel[1] * LUMA[1] + pixel[2] * LUMA[2]
}
