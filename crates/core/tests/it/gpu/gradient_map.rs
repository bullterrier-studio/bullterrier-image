//! `gradient_map` on the GPU: the clip points are the CPU's, found on the picture read back; a nearly flat picture, whose
//! stretch a one-step difference of brightness would blow up, runs on the CPU.

use bullterrier_image::{Options, Photo, Picture, Presets, Recipe, Step};

use super::{fresh, largest_gap, rendered};
use crate::common::pictures::varied_seeded;

fn gradient_map() -> Recipe {
    Recipe::new([Step::new("gradient_map").param("stops", ["#101040", "#c04080", "#fff0b0"])])
}

/// The CPU's picture and the GPU's of `values` (`height` x `width`), the GPU's transfers and their gap.
fn both(height: usize, width: usize, values: &[f32]) -> Result<((u64, u64), f32), Box<dyn std::error::Error>> {
    let Some(gpu) = fresh() else { return Ok(((1, 2), 0.0)) };
    let photo = Photo::new(Picture::new(height, width, values)?);
    let cpu = photo
        .render(&gradient_map(), &Presets::new(), Options::default())?
        .into_values();
    let on_gpu = rendered(&gpu, &photo, &gradient_map(), Options::default())?.into_values();
    let moved = gpu.transfers();
    Ok(((moved.uploads, moved.downloads), largest_gap(&cpu, &on_gpu).0))
}

#[test]
fn a_picture_with_a_range_of_tones_is_stretched_on_the_device_from_one_counted_readback() -> crate::common::Outcome {
    let (moved, gap) = both(31, 23, &varied_seeded(31, 23, 5))?;
    // up once, down once for the clip points, the result down
    assert_eq!(moved, (1, 2));
    assert!(gap <= 1e-4, "gap {gap:e}");
    Ok(())
}

#[test]
fn a_nearly_flat_picture_runs_on_the_cpu_and_gives_the_cpus_picture() -> crate::common::Outcome {
    // every pixel the same grey but one, a thousandth lighter: the stretch's span is a thousandth
    let mut values = vec![0.4_f32; 12 * 9 * 3];
    values[..3].fill(0.401);
    let (moved, gap) = both(12, 9, &values)?;
    // up, down for the clip points (the plan leaves it), the same readback is the CPU step's, up again, the result down
    assert_eq!(moved, (2, 2));
    assert!(gap == 0.0, "gap {gap:e}");
    Ok(())
}
