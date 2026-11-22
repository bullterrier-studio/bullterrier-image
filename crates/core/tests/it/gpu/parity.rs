//! Every effect with a GPU kernel matches its CPU render within the bound the kernel states: seeded pictures (with exact
//! 0s and 1s among their values), each parameter at its minimum, default, maximum and two values between, every choice
//! and switch, each drivable parameter driven by a map and all of them by a field the recipe makes (a driven step runs on
//! the CPU today: those cases hold the executor to that, and a later GPU drive to the bound), at sizes that are and are
//! not multiples of a workgroup. A new kernel inherits every case; every case must be compared at least once.

use bullterrier_image::params::{Kind, Scalar};
use bullterrier_image::region::LinearGradient;
use bullterrier_image::{
    Drive, Error, Image, Input, Layer, Lut, Options, Photo, Picture, Presets, Recipe, Step, effect, effects,
};
use std::sync::Arc;

use serde_json::Value;

use super::{assert_finite, gpu, largest_gap, rendered};
use crate::common::pictures::varied_seeded;

const SIZES: [(usize, usize); 3] = [(1, 1), (17, 13), (64, 48)];

/// The effects whose default step moves more than the picture up once and back once, as (uploads, downloads), and why: `dither`'s
/// default is a serial method, which the CPU runs between a readback and an upload; `gradient_map` reads the picture back for
/// its clip points (`reading_picture`); `engraving`'s default bends its lines by the smoothed picture (a neighbourhood), which the
/// kernel leaves to the CPU.
const DEFAULT_TRANSFERS: [(&str, (u64, u64)); 3] =
    [("dither", (2, 2)), ("engraving", (2, 2)), ("gradient_map", (1, 2))];

/// The name of the picture every photo of this test is given (a 4 x 4 one: a matrix of 16 distinct greys, and a strip of one
/// tile), and of its lookup table (3 entries a side, not the identity).
const PICTURE: &str = "m";
const TABLE: &str = "table";

/// The picture and table named above.
fn given() -> Result<(Arc<Image>, Arc<Lut>), Error> {
    let greys: Vec<f32> = [9_u8, 3, 12, 0, 7, 11, 4, 8, 13, 1, 14, 2, 5, 15, 6, 10]
        .iter()
        .flat_map(|&level| [f32::from(level) / 15.0; 3])
        .collect();
    let entries = (0..27_u16)
        .map(|at| [0, 1, 2].map(|channel| f32::from((at * 7 + channel * 11) % 13) / 12.0))
        .collect();
    Ok((
        Arc::new(Image::new(4, 4, &greys)?),
        Arc::new(Lut::new(3, [0.0; 3], [1.0; 3], entries)?),
    ))
}

/// Compound cases the effect's own table refuses by design, so the CPU renders none (`dither`'s last choices are `tiles` with
/// `spot` separation and a `block`; `halftone`'s third are a teal paper under inks that print over it): the case is held by the
/// others.
const REFUSED_BY_DESIGN: [(&str, &str); 2] = [
    ("dither", "every parameter at its last value"),
    ("halftone", "every parameter at its third value"),
];

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
        // the names of what `given` gives every photo
        Kind::Picture => vec![Value::from(PICTURE)],
        Kind::Lut => vec![Value::from(TABLE)],
        _ => Vec::new(),
    }
}

#[test]
fn the_foundation_effects_have_kernels_and_a_foreign_one_has_none() {
    let names = with_kernels();
    for name in ["channel_mixer", "color_balance", "curves", "grade", "levels"] {
        assert!(names.iter().any(|have| have == name), "{name} has no kernel: {names:?}");
    }
    // a kernel is an effect's own business: the catalogue names what has one (`gpu` in `fx`, tested in Python)
    assert!(names.iter().all(|name| effect(name).is_ok()), "{names:?}");
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

/// A case of the parity test: the recipe, what it compares (a parameter's name, or how a step is driven), and whether the
/// photo carries the depth ramp its drives read.
struct Case {
    what: String,
    recipe: Recipe,
    depth: bool,
}

impl Case {
    fn of(what: impl Into<String>, step: Step) -> Self {
        Self {
            what: what.into(),
            recipe: Recipe::new([step]),
            depth: false,
        }
    }
}

/// A depth map of `height` x `width` rising from 0 at the first pixel to 1 at the last, so a drive takes every level.
fn ramp(height: usize, width: usize) -> Vec<f32> {
    let last = (height * width).saturating_sub(1).max(1);
    #[expect(clippy::cast_precision_loss, reason = "a small picture's pixel index")]
    (0..height * width).map(|at| at as f32 / last as f32).collect()
}

/// The cases of the effect `name`: its defaults; each parameter alone at each value it is tried at; every parameter at its
/// first value, then its last, then each of the two between, at once (the extremes and the middles compound); each drivable parameter driven by a map (the depth ramp);
/// and every drivable parameter at once driven by a field the recipe makes (a linear gradient region).
fn cases_of(name: &str, fields: &[bullterrier_image::params::Field]) -> Vec<Case> {
    let mut cases = vec![Case::of("defaults", Step::new(name))];
    let (mut lows, mut highs) = (Step::new(name), Step::new(name));
    // the parameters that only matter together (a reflect with a repeat of one does nothing) meet in the values between
    let (mut firsts_between, mut seconds_between) = (Step::new(name), Step::new(name));
    let mut by_field = Step::new(name);
    let mut drivable = false;
    for field in fields {
        let values = tried(field.kind, field.min.as_ref(), field.max.as_ref(), field.choices);
        if let (Some(low), Some(high)) = (values.first(), values.last()) {
            lows = lows.param(field.name, low.clone());
            highs = highs.param(field.name, high.clone());
            firsts_between = firsts_between.param(field.name, values.get(1).unwrap_or(low).clone());
            seconds_between = seconds_between.param(field.name, values.get(2).unwrap_or(low).clone());
        }
        // a drive from the parameter's first interior value to its second: inside every rule another parameter sets
        if field.drivable
            && let (Some(start), Some(end)) = (
                values.get(1).and_then(Value::as_f64),
                values.get(2).and_then(Value::as_f64),
            )
        {
            drivable = true;
            cases.push(Case {
                what: format!("{} driven by a map", field.name),
                recipe: Recipe::new([Step::new(name).param(field.name, Drive::new("depth", start, end).levels(4))]),
                depth: true,
            });
            by_field = by_field.param(field.name, Drive::new("field", end, start).levels(3));
        }
        for value in values {
            cases.push(Case::of(field.name, Step::new(name).param(field.name, value)));
        }
    }
    cases.extend([
        Case::of("every parameter at its first value", lows),
        Case::of("every parameter at its last value", highs),
        Case::of("every parameter at its second value", firsts_between),
        Case::of("every parameter at its third value", seconds_between),
    ]);
    if drivable {
        cases.push(Case {
            what: "every drivable parameter driven by a field".to_owned(),
            recipe: Recipe::new([by_field]).with_region("field", LinearGradient::default()),
            depth: false,
        });
    }
    cases
}

/// Says the largest gap a kernel showed against its bound, when `BT_GPU_GAPS` is set (the bounds measured, for the docs).
#[expect(
    clippy::print_stderr,
    reason = "the measured gaps are the output of a run that asks for them"
)]
fn report_gap(name: &str, worst: f32, bound: f32) {
    if std::env::var_os("BT_GPU_GAPS").is_some() {
        eprintln!("gap {name}: {worst:e} of {bound:e}");
    }
}

#[test]
fn every_kernel_matches_its_cpu_render_within_its_bound_across_its_parameters_and_drives() -> crate::common::Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let presets = Presets::new();
    for name in with_kernels() {
        let found = effect(&name)?;
        let bound = found.kernel().map_or(0.0, bullterrier_image::GpuKernel::bound);
        // a kernel's effect that read a field or a partition would need its own case here
        assert!(
            found.fields().is_empty() && !found.inputs().contains(&Input::Partition),
            "{name}: give this harness a case for its field or partition"
        );
        let fields = found.params().fields;
        let cases = cases_of(&name, &fields);
        let mut compared = std::collections::BTreeMap::<&str, usize>::new();
        let mut worst = 0.0_f32;
        for (seed, case) in cases.iter().enumerate() {
            for &(height, width) in &SIZES {
                let values = seeded(height, width, seed);
                let (picture, table) = given()?;
                let mut photo = Photo::new(Picture::new(height, width, &values)?)
                    .with_image(PICTURE, picture)
                    .with_lut(TABLE, table);
                if case.depth {
                    photo = photo.with_map("depth", Layer::new(height, width, ramp(height, width))?)?;
                }
                // a value the table refuses with the others at their defaults (a point past another) is no case
                let Ok(cpu) = photo.render(&case.recipe, &presets, Options::default()) else {
                    continue;
                };
                gpu.reset_transfers();
                let on_gpu = rendered(&gpu, &photo, &case.recipe, Options::default())?;
                if case.what == "defaults" && (height, width) == SIZES[SIZES.len() - 1] {
                    // on the largest picture (a one-pixel one is flat, which `gradient_map` leaves to the CPU) the kernel ran on the
                    // device: a step left to the CPU would read the picture back and send it again
                    let moved = gpu.transfers();
                    let expected = DEFAULT_TRANSFERS
                        .iter()
                        .find(|(effect, _)| *effect == name)
                        .map_or((1, 1), |&(_, moved)| moved);
                    assert_eq!(
                        (moved.uploads, moved.downloads),
                        expected,
                        "{name} with its defaults ran on the CPU inside the GPU render, or not as its table says"
                    );
                }
                let (cpu, on_gpu) = (cpu.into_values(), on_gpu.into_values());
                // named first, so a NaN says which effect and case wrote it
                let context = format!("{name} {}: {:?} at {height} x {width}", case.what, case.recipe.steps);
                assert_finite(&on_gpu, &format!("the GPU's picture of {context}"));
                let (gap, at) = largest_gap(&cpu, &on_gpu);
                assert!(
                    gap <= bound,
                    "{name} {}: {:?} at {height} x {width}: value {at} is {} on the GPU, {} on the CPU (gap {gap:e}, bound {bound:e}, {})",
                    case.what,
                    case.recipe.steps,
                    on_gpu[at],
                    cpu[at],
                    gpu.adapter()
                );
                worst = worst.max(gap);
                *compared.entry(case.what.as_str()).or_default() += 1;
            }
        }
        report_gap(&name, worst, bound);
        // the floor: every parameter, every drive and every compound case was compared at least once
        let never: Vec<&str> = cases
            .iter()
            .map(|case| case.what.as_str())
            .filter(|what| !compared.contains_key(what))
            .filter(|what| !REFUSED_BY_DESIGN.contains(&(name.as_str(), what)))
            .collect();
        assert!(never.is_empty(), "{name}: never compared: {never:?}");
    }
    Ok(())
}
