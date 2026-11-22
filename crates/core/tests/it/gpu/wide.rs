//! Kernels that place something by a position in single precision (a line's offset, the curtain's edge, a bump's reach): on a picture
//! thousands of pixels across the position has the precision of an f32 of that size, and the result stays within the kernel's
//! bound. The generic parity test's pictures are tens of pixels across; these are 3000 px long in each direction.

use bullterrier_image::{Options, Photo, Picture, Presets, Recipe, Step};

use super::{fresh, largest_gap, rendered};
use crate::common::pictures::varied_seeded;

/// Recipes whose position reads are big numbers on a long picture, and the effects they run.
fn recipes() -> Vec<(&'static str, Recipe)> {
    let step = |name: &str| Step::new(name);
    vec![
        ("crosshatch", Recipe::new([step("crosshatch").param("angle", 33.0).param("spacing", 2.0)])),
        ("engraving", Recipe::new([step("engraving").param("bend", 0.0).param("angle", 71.0).param("spacing", 2.0)])),
        ("aurora", Recipe::new([step("aurora").param("height", 3000.0).param("waviness", 400.0)])),
        ("lens_flare", Recipe::new([step("lens_flare").param("x", 0.9).param("y", 0.8)])),
        ("light_leak", Recipe::new([step("light_leak").param("angle", 37.0).param("size", 80.0)])),
    ]
}

#[test]
fn a_picture_thousands_of_pixels_long_stays_within_each_kernels_bound() -> crate::common::Outcome {
    let Some(gpu) = fresh() else { return Ok(()) };
    for (height, width) in [(8, 3000), (3000, 8)] {
        let values = varied_seeded(height, width, 3);
        let photo = Photo::new(Picture::new(height, width, &values)?);
        for (name, recipe) in recipes() {
            let bound = bullterrier_image::effect(name)?
                .kernel()
                .map_or(0.0, bullterrier_image::GpuKernel::bound);
            let cpu = photo
                .render(&recipe, &Presets::new(), Options::default())?
                .into_values();
            let on_gpu = rendered(&gpu, &photo, &recipe, Options::default())?.into_values();
            let (gap, at) = largest_gap(&cpu, &on_gpu);
            assert!(
                gap <= bound,
                "{name} at {height} x {width}: value {at} differs by {gap:e}, bound {bound:e}"
            );
        }
    }
    Ok(())
}
