//! A picture sent to the device and read back is the picture that was sent, at sizes that are not multiples of a
//! workgroup.

use bullterrier_image::{Image, Picture};

use pollster::block_on;

use super::gpu;
use crate::common::compare::first_bit_difference;
use crate::common::pictures::varied_seeded;

const SIZES: [(usize, usize); 3] = [(1, 1), (7, 5), (301, 199)];

#[test]
fn a_picture_comes_back_as_it_was_sent() -> crate::common::Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    for (index, (height, width)) in SIZES.into_iter().enumerate() {
        let values = varied_seeded(height, width, index);
        let sent = block_on(gpu.upload(&Picture::new(height, width, &values)?))?;
        assert_eq!((sent.height(), sent.width(), sent.channels()), (height, width, 3));
        let back = block_on(gpu.download(&sent))?;
        assert_eq!(back.channels(), 3);
        let back = back.into_values();
        assert_eq!(
            first_bit_difference(&back, &values),
            None,
            "{height} x {width} on {}",
            gpu.adapter()
        );
    }
    Ok(())
}

#[test]
fn an_image_with_alpha_comes_back_with_its_alpha() -> crate::common::Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    for (index, (height, width)) in SIZES.into_iter().enumerate() {
        let values: Vec<f32> = varied_seeded(height, width * 4, index + 7)[..height * width * 4].to_vec();
        let image = Image::rgba(height, width, &values)?;
        let sent = block_on(gpu.upload_image(&image))?;
        assert_eq!(sent.channels(), 4);
        let back = block_on(gpu.download(&sent))?;
        assert_eq!(back.channels(), 4);
        assert_eq!(
            first_bit_difference(&back.into_values(), &values),
            None,
            "{height} x {width}"
        );
    }
    Ok(())
}
