//! `dither` on the GPU: the ordered and noise methods run on the device, what they leave to the CPU is counted, and a run the
//! effect refuses is refused on both. The generic parity test holds every parameter; these hold the routing and the settings
//! it cannot name (a picture for `matrix`, the photo palette's readback, a cell of several pixels).

use std::sync::Arc;

use bullterrier_image::{Error, ErrorKind, Image, Options, Photo, Picture, Presets, Recipe, Step};
use serde_json::{Value, json};

use super::{fresh, largest_gap, rendered};
use crate::common::pictures::varied_seeded;

const SIDE: (usize, usize) = (37, 29);

// A dot exactly as far from two palette colours (a grey of 0.25 between the levels 0 and 0.5 of a `cube` in rgb) is a tie the CPU
// breaks to the lower index and the device's f32 can break either way, so no test here draws one: the parity pictures'
// values are multiples of 0.001, and the rgb palettes below do not have dyadic levels.

/// The photo the tests dither: noisy, with its exact 0s and 1s, so every palette colour is near some dot.
fn photo(values: &[f32]) -> Result<Photo<'_>, Error> {
    let greys: Vec<f32> = [9_u8, 3, 12, 0, 7, 11, 4, 8, 13, 1, 14, 2, 5, 15, 6, 10]
        .iter()
        .flat_map(|&level| [f32::from(level) / 15.0; 3])
        .collect();
    Ok(Photo::new(Picture::new(SIDE.0, SIDE.1, values)?).with_image("m", Arc::new(Image::new(4, 4, &greys)?)))
}

fn dither(settings: &[(&str, Value)]) -> Recipe {
    Recipe::new([settings.iter().fold(Step::new("dither"), |step, (name, value)| {
        step.param(*name, value.clone())
    })])
}

/// The CPU's picture and the GPU's of `recipe`, the GPU's transfers (uploads, downloads), and their gap.
fn both(recipe: &Recipe) -> Result<((u64, u64), f32), Box<dyn std::error::Error>> {
    let Some(gpu) = fresh() else { return Ok(((1, 1), 0.0)) };
    let values = varied_seeded(SIDE.0, SIDE.1, 3);
    let photo = photo(&values)?;
    let cpu = photo.render(recipe, &Presets::new(), Options::default())?.into_values();
    let on_gpu = rendered(&gpu, &photo, recipe, Options::default())?.into_values();
    let moved = gpu.transfers();
    Ok(((moved.uploads, moved.downloads), largest_gap(&cpu, &on_gpu).0))
}

#[test]
fn the_ordered_and_noise_methods_run_on_the_device_within_the_bound() -> crate::common::Outcome {
    let bound = bullterrier_image::effect("dither")?
        .kernel()
        .map_or(0.0, bullterrier_image::GpuKernel::bound);
    for method in [
        "bayer2",
        "bayer4",
        "bayer",
        "clustered",
        "clustered45",
        "blue_noise",
        "white_noise",
        "gameboy_camera",
        "matrix",
    ] {
        for (palette, space, cell, phase, pattern) in [
            ("gameboy", "oklab", 0.0, 0.0, 1.0),
            ("gameboy", "rgb", 3.0, 0.37, 1.0),
            // evenly spaced levels in Oklab: the dots are not exactly between two colours there
            ("cube3", "oklab", 2.0, 0.6, 0.7),
            ("custom", "oklab", 5.0, 0.8, 0.4),
        ] {
            let recipe = dither(&[
                ("method", json!(method)),
                ("matrix", json!("m")),
                ("palette", json!(palette)),
                ("custom", json!(["#102030", "#e0d0a0", "#a03020", "#40a060"])),
                ("space", json!(space)),
                ("cell", json!(cell)),
                ("phase", json!(phase)),
                ("pattern", json!(pattern)),
                ("seed", json!(7)),
            ]);
            let (moved, gap) = both(&recipe)?;
            assert_eq!(moved, (1, 1), "{method}, {palette}: left the device");
            assert!(gap <= bound, "{method}, {palette}, {space}, cell {cell}: gap {gap:e}");
        }
    }
    Ok(())
}

#[test]
fn a_palette_found_in_the_photo_is_the_cpus_from_one_counted_readback() -> crate::common::Outcome {
    let recipe = dither(&[
        ("method", json!("bayer4")),
        ("palette", json!("photo")),
        ("colors", json!(5)),
        ("cell", json!(2.0)),
        ("seed", json!(9)),
    ]);
    let (moved, gap) = both(&recipe)?;
    // the picture up once, down once for the palette, and the result down
    assert_eq!(
        moved,
        (1, 2),
        "the palette is found from a readback, not a second upload"
    );
    assert!(gap <= 1e-4, "gap {gap:e}");
    Ok(())
}

#[test]
fn a_serial_method_runs_on_the_cpu_counted_and_gives_the_cpus_picture() -> crate::common::Outcome {
    for method in ["floyd_steinberg", "riemersma", "dot_diffusion"] {
        let (moved, gap) = both(&dither(&[("method", json!(method)), ("palette", json!("gameboy"))]))?;
        assert_eq!(
            moved,
            (2, 2),
            "{method}: a step of the CPU reads the picture back and sends it again"
        );
        assert!(gap == 0.0, "{method}: gap {gap:e}");
    }
    Ok(())
}

#[test]
fn a_run_the_effect_refuses_is_refused_on_the_gpu_as_on_the_cpu() -> crate::common::Outcome {
    let Some(gpu) = fresh() else { return Ok(()) };
    let values = varied_seeded(SIDE.0, SIDE.1, 3);
    // method 'matrix' with no matrix: `check` names it, and the GPU runs the same check
    let recipe = dither(&[("method", json!("matrix"))]);
    let plain = Photo::new(Picture::new(SIDE.0, SIDE.1, &values)?);
    let cpu = plain.render(&recipe, &Presets::new(), Options::default());
    let on_gpu = rendered(&gpu, &plain, &recipe, Options::default());
    let (Err(cpu), Err(on_gpu)) = (cpu, on_gpu) else {
        return Err("a matrix method with no matrix rendered".into());
    };
    assert_eq!(on_gpu.kind(), ErrorKind::Recipe, "{on_gpu}");
    assert_eq!(on_gpu.to_string(), cpu.to_string());
    Ok(())
}
