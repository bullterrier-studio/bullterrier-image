//! Every effect with a GPU kernel matches its CPU render within the bound the kernel states: seeded pictures (with exact
//! 0s and 1s among their values), each parameter at its minimum, default, maximum and two values between, every choice
//! and switch, at sizes that are and are not multiples of a workgroup.

use bullterrier_image::params::{Kind, Scalar};
use bullterrier_image::{Options, Photo, Picture, Presets, Recipe, Step, effect, effects};
use serde_json::Value;

use super::gpu;
use crate::common::pictures::varied_seeded;

const SIZES: [(usize, usize); 3] = [(1, 1), (17, 13), (64, 48)];

/// The effects that have a kernel.
fn with_kernels() -> Vec<String> {
    effects()
        .into_iter()
        .filter(|name| effect(name).is_ok_and(|effect| effect.kernel().is_some()))
        .collect()
}

/// A picture of `height` x `width` with a different value in nearly every channel, every seventh value 0 and every
/// eleventh 1 (the ends a power or a clip treats apart).
pub(crate) fn seeded(height: usize, width: usize, seed: usize) -> Vec<f32> {
    let mut values = varied_seeded(height, width, seed);
    for (index, value) in values.iter_mut().enumerate() {
        if index % 7 == 0 {
            *value = 0.0;
        } else if index % 11 == 0 {
            *value = 1.0;
        }
    }
    values
}

/// The values a parameter is tried at, beside its default: its ends and two between for a number, the other state of a
/// switch, every choice, three colours.
fn tried(kind: Kind, min: Option<&Scalar>, max: Option<&Scalar>, choices: &[&str]) -> Vec<Value> {
    let number = |scalar: Option<&Scalar>| match scalar {
        Some(Scalar::Float(value)) => Some(*value),
        #[expect(clippy::cast_precision_loss, reason = "parameter ends are small")]
        Some(Scalar::Int(value)) => Some(*value as f64),
        _ => None,
    };
    match kind {
        Kind::Float => match (number(min), number(max)) {
            (Some(low), Some(high)) => [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0]
                .map(|share| Value::from(low + (high - low) * share))
                .to_vec(),
            _ => Vec::new(),
        },
        Kind::Bool => vec![Value::from(true), Value::from(false)],
        Kind::Choice => choices.iter().map(|&choice| Value::from(choice)).collect(),
        Kind::Color => ["#000000", "#ffffff", "#2a9d8f"].map(Value::from).to_vec(),
        _ => Vec::new(),
    }
}

/// The largest difference between `cpu` and `gpu`, value by value, and where it is.
fn largest_gap(cpu: &[f32], gpu: &[f32]) -> (f32, usize) {
    assert_eq!(cpu.len(), gpu.len());
    cpu.iter()
        .zip(gpu)
        .map(|(one, other)| (one - other).abs())
        .enumerate()
        .fold((0.0, 0), |(gap, at), (index, here)| {
            if here > gap || here.is_nan() {
                (here, index)
            } else {
                (gap, at)
            }
        })
}

#[test]
fn the_effects_with_kernels_are_the_five_per_pixel_colour_effects() {
    let mut names = with_kernels();
    names.sort();
    assert_eq!(names, ["channel_mixer", "color_balance", "curves", "grade", "levels"]);
}

#[test]
fn every_kernel_names_a_bound_of_at_most_an_eighth_of_an_8_bit_level() -> crate::common::Outcome {
    for name in with_kernels() {
        let bound = effect(&name)?.kernel().map(bullterrier_image::GpuKernel::bound);
        assert!(
            bound.is_some_and(|bound| bound > 0.0 && bound <= 1.0 / 255.0 / 8.0),
            "{name}: {bound:?}"
        );
    }
    Ok(())
}

#[test]
fn every_kernel_matches_its_cpu_render_within_its_bound_across_its_parameters() -> crate::common::Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let presets = Presets::new();
    for name in with_kernels() {
        let found = effect(&name)?;
        let bound = found.kernel().map_or(0.0, bullterrier_image::GpuKernel::bound);
        let mut cases = vec![Step::new(name.as_str())];
        for field in found.params().fields {
            for value in tried(field.kind, field.min.as_ref(), field.max.as_ref(), field.choices) {
                cases.push(Step::new(name.as_str()).param(field.name, value));
            }
        }
        let mut compared = 0;
        for (seed, step) in cases.into_iter().enumerate() {
            let recipe = Recipe::new([step.clone()]);
            for &(height, width) in &SIZES {
                let values = seeded(height, width, seed);
                let photo = Photo::new(Picture::new(height, width, &values)?);
                // a value the table refuses with the others at their defaults (a point past another) is no case
                let Ok(cpu) = photo.render(&recipe, &presets, Options::default()) else {
                    continue;
                };
                let on_gpu = photo.render(&recipe, &presets, Options::default().with_gpu(&gpu))?;
                let (cpu, on_gpu) = (cpu.into_values(), on_gpu.into_values());
                let (gap, at) = largest_gap(&cpu, &on_gpu);
                assert!(
                    gap <= bound,
                    "{name} {:?} at {height} x {width}: value {at} is {} on the GPU, {} on the CPU (gap {gap:e}, bound {bound:e}, {})",
                    step.params,
                    on_gpu[at],
                    cpu[at],
                    gpu.adapter()
                );
                compared += 1;
            }
        }
        assert!(compared >= 3 * 5, "{name}: only {compared} renders compared");
    }
    Ok(())
}
