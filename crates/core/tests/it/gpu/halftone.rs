//! `halftone` on the GPU at a size where its cells are several pixels wide (the generic parity test's pictures are small, so
//! its cells are about a pixel): every separation, shape and screen the kernel runs, the blur the CPU makes for a long reach, and
//! what is left to the CPU.

use std::sync::Arc;

use bullterrier_image::{Image, Options, Photo, Picture, Presets, Recipe, Step, Transfers};
use serde_json::{Value, json};

use super::{fresh, largest_gap, rendered};
use crate::common::pictures::varied_seeded;

const SIZE: (usize, usize) = (270, 480);

/// A recipe of one `halftone` step with `settings` over a photo of the size of the design (a cell of 5 is 5 px).
fn halftone(settings: &[(&str, Value)]) -> Recipe {
    Recipe::new([settings.iter().fold(Step::new("halftone"), |step, (name, value)| {
        step.param(*name, value.clone())
    })])
}

/// The CPU's gap from the GPU's picture of `recipe`, and what the GPU moved (uploads, downloads).
fn both(recipe: &Recipe) -> Result<(f32, (u64, u64)), Box<dyn std::error::Error>> {
    moved_by(recipe).map(|(gap, moved)| (gap, (moved.uploads, moved.downloads)))
}

/// As [`both`], and everything the GPU was sent.
fn moved_by(recipe: &Recipe) -> Result<(f32, Transfers), Box<dyn std::error::Error>> {
    let Some(gpu) = fresh() else {
        // no GPU: as before, one each way
        let mut skipped = Transfers::default();
        (skipped.uploads, skipped.tables, skipped.downloads) = (1, 1, 1);
        return Ok((0.0, skipped));
    };
    // a picture with some structure (a ramp across, noise) so cells of every tone appear
    let noise = varied_seeded(SIZE.0, SIZE.1, 5);
    let values: Vec<f32> = noise
        .iter()
        .enumerate()
        .map(|(at, noise)| 0.6 * noise + 0.4 * f32::from(u16::try_from((at / 3) % SIZE.1).unwrap_or(0)) / 480.0)
        .collect();
    let greys: Vec<f32> = [9_u8, 3, 12, 0, 7, 11, 4, 8, 13, 1, 14, 2, 5, 15, 6, 10]
        .iter()
        .flat_map(|&level| [f32::from(level) / 15.0; 3])
        .collect();
    let photo = Photo::new(Picture::new(SIZE.0, SIZE.1, &values)?).with_image("m", Arc::new(Image::new(4, 4, &greys)?));
    let cpu = photo.render(recipe, &Presets::new(), Options::default())?.into_values();
    let on_gpu = rendered(&gpu, &photo, recipe, Options::default())?.into_values();
    let moved = gpu.transfers();
    Ok((largest_gap(&cpu, &on_gpu).0, moved))
}

/// Says a gap when `BT_GPU_GAPS` is set (the bounds measured, for the docs).
#[expect(
    clippy::print_stderr,
    reason = "the measured gaps are the output of a run that asks for them"
)]
fn report(what: &str, gap: f32) {
    if std::env::var_os("BT_GPU_GAPS").is_some() {
        eprintln!("gap halftone, {what}: {gap:e}");
    }
}

fn bound() -> f32 {
    bullterrier_image::effect("halftone")
        .ok()
        .and_then(|effect| effect.kernel().map(bullterrier_image::GpuKernel::bound))
        .unwrap_or(0.0)
}

/// What a case is called and the settings of its step.
type Case = (&'static str, Vec<(&'static str, Value)>);

/// The separations, shapes, screens, realism and edges the kernel runs: a table, not logic.
#[expect(clippy::too_many_lines, reason = "a table of cases")]
fn cases() -> Vec<Case> {
    let spot = json!(["#e63946", "#2a9d8f", "#264de4"]);
    vec![
        ("one ink", vec![("cell", json!(5.0)), ("angle", json!(30.0))]),
        (
            "an ink of its own",
            vec![
                ("cell", json!(6.0)),
                ("ink", json!("#3050ff")),
                ("paper", json!("#f0e8d0")),
            ],
        ),
        (
            "cmyk",
            vec![
                ("cell", json!(6.0)),
                ("separation", json!("cmyk")),
                ("paper", json!("#fffbe8")),
            ],
        ),
        ("rgb", vec![("cell", json!(5.0)), ("separation", json!("rgb"))]),
        (
            "spot inks",
            vec![
                ("cell", json!(5.0)),
                ("separation", json!("spot")),
                ("inks", spot),
                ("paper", json!("#ffffff")),
                ("offset", json!(3.0)),
            ],
        ),
        ("a line", vec![("cell", json!(5.0)), ("shape", json!("line"))]),
        (
            "a square",
            vec![("cell", json!(5.0)), ("shape", json!("square")), ("angle", json!(0.0))],
        ),
        ("a diamond", vec![("cell", json!(5.0)), ("shape", json!("diamond"))]),
        (
            "an ellipse",
            vec![
                ("cell", json!(5.0)),
                ("shape", json!("ellipse")),
                ("aspect", json!(2.5)),
            ],
        ),
        ("euclidean", vec![("cell", json!(5.0)), ("shape", json!("euclidean"))]),
        (
            "a matrix",
            vec![("cell", json!(5.0)), ("shape", json!("matrix")), ("matrix", json!("m"))],
        ),
        (
            "a wave",
            vec![
                ("cell", json!(5.0)),
                ("screen", json!("wave")),
                ("amplitude", json!(0.6)),
                ("wavelength", json!(7.0)),
            ],
        ),
        (
            "dot gain and uneven ink",
            vec![
                ("cell", json!(5.0)),
                ("separation", json!("cmyk")),
                ("paper", json!("#ffffff")),
                ("dot_gain", json!(0.2)),
                ("ink_noise", json!(0.6)),
                ("seed", json!(11)),
            ],
        ),
        (
            "a large cell of cmyk with dot gain and uneven ink",
            vec![
                ("cell", json!(24.0)),
                ("sigma", json!(0.0)),
                ("separation", json!("cmyk")),
                ("paper", json!("#ffffff")),
                ("dot_gain", json!(0.2)),
                ("ink_noise", json!(0.6)),
                ("seed", json!(11)),
            ],
        ),
        (
            "spot inks in a large cell",
            vec![
                ("cell", json!(40.0)),
                ("sigma", json!(0.0)),
                ("separation", json!("spot")),
                ("inks", json!(["#e63946", "#2a9d8f", "#264de4"])),
                ("paper", json!("#ffffff")),
            ],
        ),
        ("no blur", vec![("cell", json!(5.0)), ("sigma", json!(0.0))]),
        (
            "a wrapped edge",
            vec![("cell", json!(5.0)), ("sigma", json!(0.6)), ("edge", json!("wrap"))],
        ),
        (
            "a constant edge",
            vec![
                ("cell", json!(5.0)),
                ("sigma", json!(0.6)),
                ("edge", json!("constant")),
                ("constant_value", json!(0.9)),
            ],
        ),
        (
            "a mirrored edge",
            vec![("cell", json!(5.0)), ("sigma", json!(0.6)), ("edge", json!("mirror"))],
        ),
        (
            "a nearest edge",
            vec![("cell", json!(5.0)), ("sigma", json!(0.6)), ("edge", json!("nearest"))],
        ),
    ]
}

#[test]
fn the_separations_shapes_and_screens_run_on_the_device_within_the_bound() -> crate::common::Outcome {
    for (what, settings) in cases() {
        let (gap, moved) = both(&halftone(&settings))?;
        assert_eq!(moved, (1, 1), "{what}: left the device");
        assert!(gap <= bound(), "{what}: gap {gap:e}");
        report(what, gap);
    }
    Ok(())
}

#[test]
fn a_blur_too_long_to_sum_is_the_cpus_from_a_counted_readback_and_upload() -> crate::common::Outcome {
    // the defaults at the design's size: a cell of 14 and a blur of 7 px, which the CPU makes with the Fourier transform
    let (gap, moved) = moved_by(&halftone(&[]))?;
    // up, down for the blur; the blurred picture goes up as the kernel's table; the result down
    assert_eq!((moved.uploads, moved.tables, moved.downloads), (1, 1, 2));
    assert!(gap <= bound(), "gap {gap:e}");
    Ok(())
}

#[test]
fn what_the_kernel_does_not_do_runs_on_the_cpu_counted_and_gives_the_cpus_picture() -> crate::common::Outcome {
    for (what, settings) in [
        ("rings", vec![("screen", json!("rings")), ("cell", json!(5.0))]),
        ("a spiral", vec![("screen", json!("spiral")), ("cell", json!(5.0))]),
        ("rays", vec![("screen", json!("rays")), ("cell", json!(5.0))]),
        ("specks", vec![("specks", json!(2.0)), ("cell", json!(5.0))]),
    ] {
        let (gap, moved) = both(&halftone(&settings))?;
        assert_eq!(moved, (2, 2), "{what}");
        assert!(gap == 0.0, "{what}: gap {gap:e}");
    }
    Ok(())
}
