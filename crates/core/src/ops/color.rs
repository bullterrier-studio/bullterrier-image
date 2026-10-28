//! Colour as effects meet it: perceived brightness.

/// The Rec. 709 weights of red, green and blue in perceived brightness.
const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722]; // constant: Rec. 709 weights of red, green, blue in brightness

/// The perceived brightness of `pixel` (Rec. 709 weights), 0..1 for channels in 0..1.
pub(crate) fn luminance(pixel: [f32; 3]) -> f32 {
    pixel[0] * LUMA[0] + pixel[1] * LUMA[1] + pixel[2] * LUMA[2]
}

#[cfg(test)]
mod tests {
    use super::luminance;
    use proptest::prelude::*;

    #[test]
    fn the_primaries_weigh_as_rec_709_publishes() {
        let published = [0.2126, 0.7152, 0.0722];
        for (channel, weight) in published.into_iter().enumerate() {
            let mut pixel = [0.0; 3];
            pixel[channel] = 1.0;
            assert!((luminance(pixel) - weight).abs() < 1e-7, "channel {channel}");
        }
    }

    proptest! {
        #[test]
        fn a_grey_is_as_bright_as_its_value(value in 0.0_f32..=1.0) {
            prop_assert!((luminance([value; 3]) - value).abs() < 1e-6);
        }

        #[test]
        fn raising_any_channel_never_darkens(pixel in prop::array::uniform3(0.0_f32..=1.0), channel in 0_usize..3,
                                             lift in 0.0_f32..=1.0) {
            let mut brighter = pixel;
            brighter[channel] += lift;
            prop_assert!(luminance(brighter) >= luminance(pixel));
        }
    }
}
