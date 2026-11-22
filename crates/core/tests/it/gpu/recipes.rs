//! The dither tool's recipes and demo 31's, rendered whole on the GPU: one upload, one readback, and the picture within the
//! recipe budget of the CPU's (at most 1/255 a channel on 99.9% of the values, 2/255 on all, after 8-bit rounding).

use bullterrier_image::{Options, Photo, Presets, Recipe, Step};

use super::fresh;
use super::render::{dog, levels_apart};
use super::rendered;
use crate::common::Outcome;

/// The recipes: demo 31's, a 1-bit Bayer dither, the Game Boy's palette and a CMYK halftone.
fn recipes() -> Vec<(&'static str, Recipe)> {
    vec![
        (
            "demo 31",
            Recipe::new([
                Step::new("grade").param("contrast", 1.3),
                Step::new("halftone").param("cell", 3).param("angle", 30),
                Step::new("grain").param("amount", 0.05).param("seed", 1),
            ]),
        ),
        (
            "a 1-bit Bayer dither",
            Recipe::new([
                Step::new("grade").param("contrast", 1.2),
                Step::new("dither")
                    .param("method", "bayer")
                    .param("palette", "custom")
                    .param("custom", ["#000000", "#ffffff"])
                    .param("cell", 2),
            ]),
        ),
        (
            "a Game Boy palette",
            Recipe::new([
                Step::new("grade").param("saturation", 0.6),
                Step::new("dither")
                    .param("method", "bayer4")
                    .param("palette", "gameboy")
                    .param("cell", 3),
            ]),
        ),
        (
            "a CMYK halftone",
            Recipe::new([
                Step::new("grade").param("contrast", 1.1),
                Step::new("halftone")
                    .param("cell", 12)
                    .param("separation", "cmyk")
                    .param("paper", "#fffbe8")
                    .param("dot_gain", 0.1),
            ]),
        ),
    ]
}

#[test]
fn each_recipe_runs_whole_on_the_device_with_one_upload_and_one_readback_within_the_budget() -> Outcome {
    let Some(gpu) = fresh() else { return Ok(()) };
    let photo_image = dog()?;
    let photo = Photo::new(photo_image.as_picture());
    let options = Options::default().with_size(240);
    for (what, recipe) in recipes() {
        let cpu = photo.render(&recipe, &Presets::new(), options)?.into_values();
        gpu.reset_transfers();
        let on_gpu = rendered(&gpu, &photo, &recipe, options)?.into_values();
        let moved = gpu.transfers();
        assert_eq!((moved.uploads, moved.downloads), (1, 1), "{what}: left the device");
        let (share, largest) = levels_apart(&cpu, &on_gpu);
        assert!(
            share <= 0.001 && largest <= 2,
            "{what}: {share} of the values over a level, at most {largest}"
        );
    }
    Ok(())
}
