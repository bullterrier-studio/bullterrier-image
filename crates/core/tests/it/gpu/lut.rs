//! `lut` on the GPU with a table the generic parity test's own does not make: a larger size, a domain narrower than 0 to 1
//! (colours outside it take the edge entry) and entries outside 0 to 1 (the result is kept in range).

use std::sync::Arc;

use bullterrier_image::{Lut, Options, Photo, Picture, Presets, Recipe, Step};

use super::{fresh, largest_gap, rendered};
use crate::common::pictures::varied_seeded;

#[test]
fn a_table_with_its_own_domain_and_entries_outside_the_range_grades_as_on_the_cpu() -> crate::common::Outcome {
    let Some(gpu) = fresh() else { return Ok(()) };
    let size = 5_u16;
    let entries = (0..size.pow(3))
        .map(|at| [0, 1, 2].map(|channel| f32::from((at * 5 + channel * 17) % 23) / 11.0 - 0.3))
        .collect();
    let lut = Lut::new(usize::from(size), [0.1, 0.0, 0.2], [0.9, 1.0, 0.7], entries)?;
    let values = varied_seeded(23, 19, 4);
    let photo = Photo::new(Picture::new(23, 19, &values)?).with_lut("grade", Arc::new(lut));
    for amount in [1.0, 0.35] {
        let recipe = Recipe::new([Step::new("lut").param("file", "grade").param("amount", amount)]);
        let cpu = photo
            .render(&recipe, &Presets::new(), Options::default())?
            .into_values();
        gpu.reset_transfers();
        let on_gpu = rendered(&gpu, &photo, &recipe, Options::default())?.into_values();
        assert_eq!((gpu.transfers().uploads, gpu.transfers().downloads), (1, 1));
        let (gap, at) = largest_gap(&cpu, &on_gpu);
        assert!(gap <= 1e-4, "amount {amount}: value {at} differs by {gap:e}");
    }
    Ok(())
}
