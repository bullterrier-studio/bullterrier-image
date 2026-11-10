//! Apple Vision on the bull terrier photo, for real (macOS 14 and later has it; no download).

use std::path::Path;

use bullterrier_image::{Error, Finder};
use bullterrier_image_finders::Vision;

type Outcome = Result<(), Box<dyn std::error::Error>>;

#[test]
fn vision_finds_the_dog_and_leaves_the_window() -> Outcome {
    let image =
        bullterrier_image::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/photos/bullterrier.jpeg"))?;
    let picture = image.as_picture();
    let (height, width) = (picture.height(), picture.width());
    let matte = Vision::new()?.find(&picture)?;
    assert_eq!((matte.height(), matte.width()), (height, width));
    assert!(
        matte.values()[(height / 2) * width + width / 2] > 0.9,
        "the dog's chest"
    );
    let corner = (0..height / 10).flat_map(|row| (0..width / 10).map(move |column| row * width + column));
    assert!(
        corner.map(|at| matte.values()[at]).fold(0.0, f32::max) < 0.1,
        "the window, top left"
    );
    Ok(())
}

#[test]
fn a_flat_grey_picture_has_no_subject() -> Outcome {
    let values = vec![0.5_f32; 64 * 64 * 3];
    let picture = bullterrier_image::Picture::new(64, 64, &values)?;
    let refused = Vision::new()?.find(&picture);
    assert!(matches!(refused, Err(Error::MapNotFound { .. })), "{refused:?}");
    Ok(())
}
