//! One step of a recipe: [`prepare`] resolves it (its parameters complete and scaled, the regions it names), a
//! [`Prepared`] runs its effect on a picture and lays the result over it.

use std::sync::Arc;

use serde_json::{Map, Value};

use super::regions::Evaluator;
use crate::effect::{self, Effect};
use crate::ops::{blend, levels};
use crate::recipe::drive;
use crate::registry::Registry;
use crate::{BlendMode, Entry, Error, Image, Inputs, Layer, Picture, Presets};

/// The step `entry` (at `place`, as `step 2`) resolved: its parameters complete and scaled, the regions it names, or why
/// the core does not run it yet.
pub(super) fn prepare(
    registry: &Registry,
    presets: &Presets,
    ratio: f64,
    entry: &Entry,
    place: &str,
) -> Result<Prepared, Error> {
    let Entry::Step(step) = entry else {
        return Err(Error::Recipe(format!(
            "{place}: an `each` step is not run by the core yet"
        )));
    };
    let effect = registry.effect(&step.effect)?;
    let given = presets.under(step, place)?;
    let mut fields = Vec::new();
    for field in effect.fields() {
        let name = field
            .field_in(&given)
            .map_err(|why| Error::Recipe(format!("{place}: {why}")))?;
        fields.push((field.parameter.clone(), name.to_owned()));
    }
    let drives = drive::in_params(&step.effect, &given, place)?;
    let runs = if let Some((_, first)) = drives.first() {
        // one run for each level of the drives, each with its values checked and scaled like a plain step's
        let levels = (0..first.levels)
            .map(|level| {
                let mut values = given.clone();
                for (name, drive) in &drives {
                    values.insert(name.clone(), Value::from(drive.value(level)));
                }
                complete(&*effect, &step.effect, values, ratio, place)
            })
            .collect::<Result<_, _>>()?;
        Runs::Driven {
            by: first.by.clone(),
            top: first.levels - 1,
            levels,
        }
    } else {
        Runs::One(complete(&*effect, &step.effect, given, ratio, place)?)
    };
    Ok(Prepared {
        opacity: step.opacity.unwrap_or_else(|| effect.opacity()),
        blend_mode: step.blend_mode,
        mask: step.mask.clone(),
        fields,
        effect,
        runs,
    })
}

/// The complete parameters a step's effect is run with.
enum Runs {
    /// A plain step: one run.
    One(Value),
    /// A step with driven parameters: one run for each level, `levels[n]` for level `n` (0 to `top`), mixed by the
    /// selection called `by`.
    Driven { by: String, top: u8, levels: Vec<Value> },
}

/// A step ready to run: its effect, the complete parameters it receives, the selections it works from (by region name)
/// and how its result is laid over its input.
pub(super) struct Prepared {
    effect: Arc<dyn Effect>,
    runs: Runs,
    /// The field parameters of the effect, each with the name of its region.
    fields: Vec<(String, String)>,
    opacity: f64,
    blend_mode: BlendMode,
    mask: Option<String>,
}

/// The parameters `effect` (called `name`) is run with for the values `given`: for an effect that describes them, checked
/// and scaled to `ratio`, else as given.
fn complete(
    effect: &dyn Effect,
    name: &str,
    given: Map<String, Value>,
    ratio: f64,
    place: &str,
) -> Result<Value, Error> {
    Ok(Value::Object(match effect.params() {
        Some(table) => table.scaled(table.check(name, &given).map_err(|error| error.at(place))?, ratio),
        None => given,
    }))
}

impl Prepared {
    /// This step's effect run on `before`, with the selections it names made by `regions` for `working`, the picture the
    /// render works on, and the result laid over `before`.
    ///
    /// # Errors
    /// Why a region could not be made, or the effect failed.
    pub(super) fn apply(
        &self,
        before: &Picture<'_>,
        ratio: f64,
        regions: &mut Evaluator,
        working: &Picture<'_>,
    ) -> Result<Image, Error> {
        let mut fields = Vec::with_capacity(self.fields.len());
        for (parameter, region) in &self.fields {
            fields.push((parameter, regions.selection(region, working)?));
        }
        let mut offered = Inputs::default().with_size_ratio(ratio);
        for (parameter, layer) in &fields {
            offered = offered.with_field(parameter.as_str(), layer);
        }
        let after = match &self.runs {
            Runs::One(params) => effect::run(&*self.effect, before, params.clone(), &offered)?,
            Runs::Driven { by, top, levels } => {
                let by = regions.selection(by, working)?;
                self.driven(before, &offered, &by, *top, levels)?
            }
        };
        let mask = self
            .mask
            .as_ref()
            .map(|mask| regions.selection(mask, working))
            .transpose()?;
        Ok(self.lay(before, after, mask.as_deref()))
    }

    /// The effect run on `before` for each level of the drive that some pixel is on or next to, and each pixel mixes the
    /// runs of the two levels either side of its value in `by`, linearly; a level none is near is not run.
    fn driven(
        &self,
        before: &Picture<'_>,
        offered: &Inputs<'_>,
        by: &Layer,
        top: u8,
        levels: &[Value],
    ) -> Result<Image, Error> {
        let positions: Vec<f32> = by.values().iter().map(|value| value * f32::from(top)).collect();
        let mut mixed = vec![[0.0_f32; 3]; positions.len()];
        for (level, params) in (0..=top).zip(levels) {
            let weights: Vec<f32> = positions
                .iter()
                .map(|&position| levels::weight(position, level))
                .collect();
            if weights.iter().all(|&weight| weight <= 0.0) {
                continue;
            }
            let run = effect::run(&*self.effect, before, params.clone(), offered)?;
            for ((out, pixel), weight) in mixed.iter_mut().zip(run.as_picture().pixels()).zip(&weights) {
                *out = std::array::from_fn(|channel| out[channel] + weight * pixel[channel]);
            }
        }
        Ok(Image::from_pixels(before.height(), before.width(), mixed))
    }

    /// `after`, the effect's result for `before`, laid over `before` as the step says: by its blend mode, mixed in by its
    /// opacity and, where `mask` is given, only as far as the mask says. A plain step (blend mode normal, opacity 1, no
    /// mask) is its result as it is, uncopied.
    fn lay(&self, before: &Picture<'_>, after: Image, mask: Option<&Layer>) -> Image {
        if self.opacity >= 1.0 && mask.is_none() && self.blend_mode == BlendMode::Normal {
            after
        } else {
            blend::lay(before, &after.as_picture(), self.blend_mode, self.opacity, mask)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::params::Declared;
    use crate::region::Grid;
    use crate::testing::{Measured, Paint, Recorder, registry, with_recorder};
    use crate::{Drive, Each, Options, Photo, Recipe, Step};

    /// The values of a 4 x 6 picture, a different one in every channel.
    fn values() -> Vec<f32> {
        (0_u8..72).map(|index| f32::from(index) / 80.0).collect()
    }

    /// `recipe` rendered on the 4 x 6 picture by the registry `registry`.
    fn rendered(registry: &Registry, recipe: &Recipe, presets: &Presets) -> Result<Image, Error> {
        let values = values();
        Photo::new(Picture::new(4, 6, &values)?).render_in(registry, recipe, presets, Options::default())
    }

    /// `actual` is `expected` to within what an f32 sum loses.
    fn assert_close(actual: &[f32], expected: &[f32]) {
        assert_eq!(actual.len(), expected.len());
        for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
            assert!((a - e).abs() < 1e-6, "value {index}: {a} is not {e}");
        }
    }

    #[test]
    fn a_step_reaches_its_effect_with_the_preset_under_its_values_and_the_defaults_for_the_rest() -> Result<(), Error> {
        let (registry, recorder) = with_recorder()?;
        let mut presets = Presets::new();
        let table = json!({"wide": {"description": "Wide cells.", "cell": 8, "cells": 7}});
        presets.add_in(&registry, "recorder", &table)?;
        let mut step = Step::new("recorder").param("cells", 9);
        step.preset = Some("wide".to_owned());
        // a picture of the design size, so that px values are not scaled
        let values = vec![0.5_f32; 480 * 3];
        Photo::new(Picture::new(1, 480, &values)?).render_in(
            &registry,
            &Recipe::new([step]),
            &presets,
            Options::default(),
        )?;
        // `cell` from the preset (a decimal, though written whole), `cells` from the step over the preset's 7, the rest
        // the probe's declared defaults, a colour as three numbers
        let expected = json!({
            "cell": 8.0, "cells": 9, "angle": 0.5, "shape": "fourth_way", "keep": true,
            "tint": [1.0, 128.0 / 255.0, 0.0],
        });
        let received = recorder.received();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].params, expected);
        Ok(())
    }

    #[test]
    fn an_effect_that_describes_no_parameters_gets_them_as_the_step_writes_them() -> Result<(), Error> {
        let registry = registry()?;
        let recorder = Arc::new(Recorder::new("loose", &[], &[]));
        registry.register(recorder.clone())?;
        let step = Step::new("loose").param("anything", json!([1, "two"]));
        rendered(&registry, &Recipe::new([step]), &Presets::new())?;
        assert_eq!(recorder.received()[0].params, json!({"anything": [1, "two"]}));
        Ok(())
    }

    #[test]
    fn a_step_that_sets_no_opacity_has_the_opacity_its_effect_declares() -> Result<(), Error> {
        let registry = registry()?;
        registry.register(Arc::new(Paint { name: "paint" }))?;
        let paint = || Step::new("paint").param("level", 0.5);
        // `paint` declares an opacity of 0.25: a quarter of the way from the photo (value v) to 0.5 is v + 0.25 (0.5 - v)
        let default = rendered(&registry, &Recipe::new([paint()]), &Presets::new())?;
        let expected: Vec<f32> = values().iter().map(|v| v + 0.25 * (0.5 - v)).collect();
        assert_close(&default.into_values(), &expected);
        let own = rendered(&registry, &Recipe::new([paint().opacity(0.5)]), &Presets::new())?;
        let expected: Vec<f32> = values().iter().map(|v| v + 0.5 * (0.5 - v)).collect();
        assert_close(&own.into_values(), &expected);
        let full = rendered(&registry, &Recipe::new([paint().opacity(1.0)]), &Presets::new())?;
        assert_eq!(full.into_values(), vec![0.5; 72]);
        Ok(())
    }

    #[test]
    fn an_each_step_is_refused_naming_it_because_the_core_does_not_run_it_yet() -> Result<(), Error> {
        let registry = registry()?;
        let mut recipe_with_each = Recipe::new([Each::new("panes", [None])]);
        recipe_with_each.regions.insert("panes", Grid::default());
        let result = rendered(&registry, &recipe_with_each, &Presets::new());
        assert!(
            matches!(&result, Err(error @ Error::Recipe(_)) if error.to_string().contains("not run by the core yet")),
            "{result:?}"
        );
        Ok(())
    }

    /// The 1 x `width` photo of `width` grey pixels, with `field` as its map `subject` (one value per pixel).
    fn banded(field: &[f32]) -> Result<(Vec<f32>, Layer), Error> {
        Ok((vec![0.5; field.len() * 3], Layer::new(1, field.len(), field.to_vec())?))
    }

    /// `step` rendered on a grey photo as wide as `field` that has `field` as its map `subject`.
    fn driven_by(registry: &Registry, step: Step, field: &[f32]) -> Result<Image, Error> {
        let (values, subject) = banded(field)?;
        Photo::new(Picture::new(1, field.len(), &values)?)
            .with_map("subject", subject)?
            .render_in(registry, &Recipe::new([step]), &Presets::new(), Options::default())
    }

    #[test]
    fn a_driven_parameter_mixes_the_runs_either_side_of_the_maps_value() -> Result<(), Error> {
        let registry = registry()?;
        registry.register(Arc::new(Paint { name: "paint" }))?;
        // paints the grey `level`, here 0.2 where the map is 0 and 0.8 where it is 1: a pixel at v mixes (1 - v) of the
        // first run with v of the second
        let step = Step::new("paint")
            .param("level", Drive::new("subject", 0.2, 0.8).levels(2))
            .opacity(1.0);
        let field = [0.0, 1.0, 0.25, 0.5, 0.75, 0.1];
        let painted = driven_by(&registry, step, &field)?.into_values();
        let expected: Vec<f32> = [0.2, 0.8, 0.35, 0.5, 0.65, 0.26]
            .iter()
            .flat_map(|level| [*level; 3])
            .collect();
        assert_close(&painted, &expected);
        Ok(())
    }

    #[test]
    fn a_driven_effect_with_the_map_on_a_level_is_the_plain_effect_at_that_levels_value() -> Result<(), Error> {
        // grade's exposure from 0 to 2 in 5 levels (0, 0.5, 1, 1.5, 2); a map of 0.5 is on level 2 (exposure 1), 0.25 on
        // level 1 (0.5), 0.375 halfway between levels 1 and 2
        let grade = |exposure: f64| Step::new("grade").param("exposure", exposure);
        let driven = Step::new("grade").param("exposure", Drive::new("subject", 0.0, 2.0));
        let registry = registry()?;
        let rendered_with = |step: Step, field: f32| -> Result<Vec<f32>, Error> {
            Ok(driven_by(&registry, step, &[field; 6])?.into_values())
        };
        let plain = |exposure: f64| rendered_with(grade(exposure), 0.0);
        assert_eq!(rendered_with(driven.clone(), 0.5)?, plain(1.0)?);
        assert_eq!(rendered_with(driven.clone(), 0.25)?, plain(0.5)?);
        let (below, above) = (plain(0.5)?, plain(1.0)?);
        let halfway: Vec<f32> = below
            .iter()
            .zip(&above)
            .map(|(low, high)| 0.5 * low + 0.5 * high)
            .collect();
        assert_close(&rendered_with(driven, 0.375)?, &halfway);
        Ok(())
    }

    #[test]
    fn only_the_levels_a_pixel_is_on_or_next_to_are_run_and_the_last_is_exactly_the_end() -> Result<(), Error> {
        let registry = registry()?;
        let recorder = Arc::new(Recorder::new("recorder", &[], &[]));
        registry.register(recorder.clone())?;
        // x from 10 to 50 in 5 levels: 10, 20, 30, 40, 50
        let step = Step::new("recorder").param("x", Drive::new("subject", 10.0, 50.0));
        // (the map's values, the x of the runs in order)
        let rows: [(&[f32], &[f64]); 5] = [
            (&[0.0, 0.0], &[10.0]),
            (&[0.5, 0.5], &[30.0]),
            (&[0.3, 0.3], &[20.0, 30.0]),
            (&[0.0, 1.0], &[10.0, 50.0]),
            (&[0.9], &[40.0, 50.0]),
        ];
        for (field, expected) in rows {
            driven_by(&registry, step.clone(), field)?;
            let runs: Vec<f64> = recorder
                .received()
                .iter()
                .filter_map(|received| received.params["x"].as_f64())
                .collect();
            assert_eq!(runs, expected, "{field:?}");
            recorder.clear();
        }
        // 0.3 + 0.6 x 3 / 3 is 0.9000000000000001: the run of the last level gets `end` itself
        let end = Step::new("recorder").param("x", Drive::new("subject", 0.3, 0.9).levels(4));
        driven_by(&registry, end, &[1.0])?;
        let received = recorder.received();
        assert_eq!(received.len(), 1);
        assert_eq!(
            received[0].params["x"].as_f64().map(f64::to_bits),
            Some(0.9_f64.to_bits())
        );
        Ok(())
    }

    #[test]
    fn the_weights_of_a_pixel_add_up_to_one_so_a_constant_stays_that_constant() -> Result<(), Error> {
        let registry = registry()?;
        registry.register(Arc::new(Paint { name: "paint" }))?;
        // every level paints 1: the result is the sum of the weights, for every map value from 0 to 1
        let field: Vec<f32> = (0..1001_u16).map(|index| f32::from(index) / 1000.0).collect();
        for levels in [2, 3, 5, 16] {
            let step = Step::new("paint")
                .param("level", Drive::new("subject", 1.0, 1.0).levels(levels))
                .opacity(1.0);
            let painted = driven_by(&registry, step, &field)?.into_values();
            assert!(
                painted.iter().all(|value| (1.0 - 1e-6..=1.0).contains(value)),
                "{levels} levels"
            );
        }
        Ok(())
    }

    #[test]
    fn a_linear_effect_driven_by_a_map_is_its_parameter_at_the_maps_value_whatever_the_levels() -> Result<(), Error> {
        let registry = registry()?;
        registry.register(Arc::new(Paint { name: "paint" }))?;
        // paint's grey is its `level`, so a driven pixel is 0.2 + 0.6 v at map value v, however many levels mix it
        let field: Vec<f32> = (0..=100_u8).map(|index| f32::from(index) / 100.0).collect();
        let expected: Vec<f32> = field.iter().flat_map(|v| [0.2 + 0.6 * v; 3]).collect();
        for levels in [2, 3, 5, 16] {
            let step = Step::new("paint")
                .param("level", Drive::new("subject", 0.2, 0.8).levels(levels))
                .opacity(1.0);
            let painted = driven_by(&registry, step, &field)?.into_values();
            assert_close(&painted, &expected);
        }
        Ok(())
    }

    #[test]
    fn every_driven_parameter_of_a_step_moves_with_its_level_and_the_others_stay_as_they_are() -> Result<(), Error> {
        let registry = registry()?;
        let recorder = Arc::new(Recorder::new("recorder", &[], &[]));
        registry.register(recorder.clone())?;
        let step = Step::new("recorder")
            .param("x", Drive::new("subject", 10.0, 50.0).levels(2))
            .param("y", Drive::new("subject", 1.0, 5.0).levels(2))
            .param("z", 7);
        driven_by(&registry, step, &[0.0, 1.0])?;
        let runs: Vec<Value> = recorder
            .received()
            .iter()
            .map(|received| received.params.clone())
            .collect();
        assert_eq!(
            runs,
            [
                json!({"x": 10.0, "y": 1.0, "z": 7}),
                json!({"x": 50.0, "y": 5.0, "z": 7})
            ]
        );
        Ok(())
    }

    #[test]
    fn each_level_is_scaled_to_the_picture_on_its_own() -> Result<(), Error> {
        let registry = registry()?;
        let recorder = Arc::new(Recorder::new("measure", &[], &[]).with_table(Measured::table()));
        registry.register(recorder.clone())?;
        // `length` is px with a minimum of 2: from 3 (map 0) to 8 (map 1) at half size, a picture of 240 px: 1.5 is held
        // at 2, 4 stays 4
        let step = Step::new("measure").param("length", Drive::new("subject", 3.0, 8.0).levels(2));
        let field: Vec<f32> = (0..240).map(|index| f32::from(u8::from(index > 0))).collect();
        driven_by(&registry, step, &field)?;
        let lengths: Vec<Value> = recorder
            .received()
            .iter()
            .map(|received| received.params["length"].clone())
            .collect();
        assert_eq!(lengths, [json!(2.0), json!(4.0)]);
        Ok(())
    }
}
