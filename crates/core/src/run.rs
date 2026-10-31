//! Running a recipe on a photo: [`Photo::render`].
//!
//! ```text
//! let picture = Picture::new(height, width, &values)?;
//! let look = Recipe::new([Step::new("grade").param("exposure", 0.5), Step::new("pixel_grid")]);
//! let image = Photo::new(picture).render(&look, &Presets::new(), Options::default())?;
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use crate::effect::{self, Effect};
use crate::ops::blend;
use crate::registry::Registry;
use crate::{BlendMode, Entry, Error, Image, Inputs, Layer, Picture, Presets, Recipe};

/// What a render is asked for besides the recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Options {
    size_ratio: f64,
}

impl Default for Options {
    /// A ratio of 1: the picture is the size the recipe's pixel values are written for.
    fn default() -> Self {
        Self { size_ratio: 1.0 }
    }
}

impl Options {
    /// These options with `ratio` working pixels per design pixel: the working size over 480, the longest side a
    /// recipe's pixel values (`px`) are written for. A render multiplies them by it, so the look is the same at any
    /// size. A finite number above 0, else the render is refused.
    #[must_use]
    pub const fn with_size_ratio(mut self, ratio: f64) -> Self {
        self.size_ratio = ratio;
        self
    }
}

/// A picture a recipe is rendered on, with the maps of it that the recipe names (`subject`, `depth`, a region). It
/// borrows the picture: rendering never copies or changes it.
#[derive(Clone, Debug)]
pub struct Photo<'a> {
    picture: Picture<'a>,
    maps: BTreeMap<String, Layer>,
}

impl<'a> Photo<'a> {
    /// The photo whose picture is `picture`, with no maps.
    #[must_use]
    pub const fn new(picture: Picture<'a>) -> Self {
        Self {
            picture,
            maps: BTreeMap::new(),
        }
    }

    /// This photo with `map` for the name `name` (`subject`, `depth`, or a region the recipe declares as given): a
    /// step's `mask`, a drive's `by` and an effect's field parameters find it there. A map of the same name given
    /// before is replaced. It must be of the picture's size, which a render checks when it uses the map.
    #[must_use]
    pub fn with_map(mut self, name: impl Into<String>, map: Layer) -> Self {
        self.maps.insert(name.into(), map);
        self
    }

    /// The picture `recipe` makes of this photo: its steps run in order, each on the previous one's output, and only
    /// that output is kept. `presets` holds the presets the steps name.
    ///
    /// The recipe is checked first by the rules of [`Recipe::checked`], so nothing runs for a recipe that breaks one;
    /// each step's effect then receives its complete parameters: the step's values over its preset's, the declared
    /// defaults for the rest, scaled to `options`. The step's result is laid over its input as an image editor lays a
    /// layer: by its blend mode, mixed in by its opacity (none: the effect's own) and, where it names a `mask`, only
    /// as far as the mask says. An `each` step and a recipe with a `frame` or a `size` are refused until the core
    /// runs them.
    ///
    /// # Errors
    /// Whatever [`Recipe::checked`] refuses the recipe for, [`Error::Recipe`] for a step or recipe the core does not
    /// run yet, [`Error::MapNotFound`] for a mask or field the photo has no map of (or of another size), and
    /// [`Error::Foreign`] if an effect that runs elsewhere fails or returns a picture of another size.
    pub fn render(&self, recipe: &Recipe, presets: &Presets, options: Options) -> Result<Image, Error> {
        self.render_in(crate::registry::registry(), recipe, presets, options)
    }

    /// As [`render`](Self::render), with the effects of `registry`.
    pub(crate) fn render_in(
        &self,
        registry: &Registry,
        recipe: &Recipe,
        presets: &Presets,
        options: Options,
    ) -> Result<Image, Error> {
        let ratio = options.size_ratio;
        if !(ratio.is_finite() && ratio > 0.0) {
            return Err(Error::Inputs(format!(
                "`size_ratio` must be a finite number above 0, got {ratio}"
            )));
        }
        recipe.check_in(registry, presets)?;
        if !recipe.frame.is_empty() {
            return Err(Error::Recipe("`frame` is not run by the core yet".to_owned()));
        }
        if recipe.size.is_some() {
            return Err(Error::Recipe("`size` is not run by the core yet".to_owned()));
        }
        let steps = recipe
            .steps
            .iter()
            .enumerate()
            .map(|(index, entry)| self.prepare(registry, presets, ratio, entry, &format!("step {}", index + 1)))
            .collect::<Result<Vec<_>, _>>()?;
        let mut current: Option<Image> = None;
        for step in &steps {
            let before = current.as_ref().map_or(self.picture, Image::as_picture);
            let mut offered = Inputs::default().with_size_ratio(ratio);
            for (parameter, layer) in &step.fields {
                offered = offered.with_field(parameter.as_str(), layer);
            }
            let after = effect::run(&*step.effect, &before, step.params.clone(), &offered)?;
            current = Some(step.lay(&before, after));
        }
        current.ok_or_else(|| Error::Recipe("a recipe needs at least one step".to_owned()))
    }

    /// The step `entry` (at `place`, as `step 2`) resolved: its parameters complete and scaled, the maps it names
    /// found, or why the core does not run it yet.
    fn prepare(
        &self,
        registry: &Registry,
        presets: &Presets,
        ratio: f64,
        entry: &Entry,
        place: &str,
    ) -> Result<Prepared<'_>, Error> {
        let Entry::Step(step) = entry else {
            return Err(Error::Recipe(format!(
                "{place}: an `each` step is not run by the core yet"
            )));
        };
        let effect = registry.effect(&step.effect)?;
        let given = presets.under(step, place)?;
        let mut fields = Vec::new();
        for field in effect.fields() {
            let name = match given.get(&field.parameter) {
                None => &field.default,
                Some(Value::String(name)) => name,
                Some(other) => {
                    return Err(Error::Recipe(format!(
                        "{place}: `{}` names a field, got {other}",
                        field.parameter
                    )));
                }
            };
            fields.push((field.parameter.clone(), self.map(name)?));
        }
        let params = match effect.params() {
            Some(table) => table.scaled(
                table.check(&step.effect, &given).map_err(|error| error.at(place))?,
                ratio,
            ),
            None => given,
        };
        Ok(Prepared {
            opacity: step.opacity.unwrap_or_else(|| effect.opacity()),
            blend_mode: step.blend_mode,
            mask: step.mask.as_deref().map(|name| self.map(name)).transpose()?,
            fields,
            effect,
            params: Value::Object(params),
        })
    }

    /// The map handed in as `name`, if it is of the picture's size.
    fn map(&self, name: &str) -> Result<&Layer, Error> {
        let layer = self.maps.get(name).ok_or_else(|| {
            Error::map_not_found(
                name,
                format!("no map '{name}' was given to the photo (Photo::with_map)"),
            )
        })?;
        let (height, width) = (self.picture.height(), self.picture.width());
        if (layer.height(), layer.width()) != (height, width) {
            return Err(Error::map_not_found(
                name,
                format!(
                    "the map '{name}' is {} x {} pixels, but the picture is {height} x {width}",
                    layer.height(),
                    layer.width()
                ),
            ));
        }
        Ok(layer)
    }
}

/// A step ready to run: its effect, the complete parameters it receives, the maps it works from and how its result is
/// laid over its input.
struct Prepared<'p> {
    effect: Arc<dyn Effect>,
    params: Value,
    /// The field parameters of the effect, each with its map.
    fields: Vec<(String, &'p Layer)>,
    opacity: f64,
    blend_mode: BlendMode,
    mask: Option<&'p Layer>,
}

impl Prepared<'_> {
    /// `after`, the effect's result for `before`, laid over `before` as the step says. A plain step (blend mode
    /// normal, opacity 1, no mask) is its result as it is, uncopied.
    fn lay(&self, before: &Picture<'_>, after: Image) -> Image {
        if self.opacity >= 1.0 && self.mask.is_none() && self.blend_mode == BlendMode::Normal {
            after
        } else {
            blend::lay(before, &after.as_picture(), self.blend_mode, self.opacity, self.mask)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::params::Declared;
    use crate::params::probe::Probe;
    use crate::testing::{Paint, Recorder, Shrinks, registry};
    use crate::{Each, ErrorKind, Step};

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

    /// The test registry with a recorder of the probe's parameters, called `recorder`.
    fn with_recorder() -> Result<(Registry, Arc<Recorder>), Error> {
        let registry = registry()?;
        let recorder = Arc::new(Recorder::new("recorder", &[], &[]).with_table(Probe::table()));
        registry.register(recorder.clone())?;
        Ok((registry, recorder))
    }

    #[test]
    fn a_step_reaches_its_effect_with_the_preset_under_its_values_and_the_defaults_for_the_rest() -> Result<(), Error> {
        let (registry, recorder) = with_recorder()?;
        let mut presets = Presets::new();
        let table = json!({"wide": {"description": "Wide cells.", "cell": 8, "cells": 7}});
        presets.add_in(&registry, "recorder", &table)?;
        let mut step = Step::new("recorder").param("cells", 9);
        step.preset = Some("wide".to_owned());
        rendered(&registry, &Recipe::new([step]), &presets)?;
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
    fn what_the_core_does_not_run_yet_is_refused_naming_it() -> Result<(), Error> {
        let registry = registry()?;
        let mut recipe_with_frame = Recipe::new([Step::new("probe")]);
        recipe_with_frame.frame = json!({"flip": "horizontal"}).as_object().cloned().unwrap_or_default();
        let mut recipe_with_size = Recipe::new([Step::new("probe")]);
        recipe_with_size.size = Some(480);
        let mut recipe_with_each = Recipe::new([Each::new("panes", [None])]);
        recipe_with_each.regions = json!({"panes": {"kind": "grid"}})
            .as_object()
            .cloned()
            .unwrap_or_default();
        for (recipe, said) in [
            (recipe_with_each, "step 1: an `each` step"),
            (recipe_with_frame, "`frame`"),
            (recipe_with_size, "`size`"),
        ] {
            let result = rendered(&registry, &recipe, &Presets::new());
            assert!(
                matches!(&result, Err(error @ Error::Recipe(_)) if error.to_string().contains(said)),
                "{said}: {result:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_recipe_that_breaks_a_rule_of_a_look_is_refused_before_anything_runs() -> Result<(), Error> {
        let (registry, recorder) = with_recorder()?;
        // the first step is fine; the second names no effect, a value out of range or a mask that is no region
        for (recipe, said) in [
            (
                Recipe::new([Step::new("recorder"), Step::new("no_such_effect")]),
                "no_such_effect",
            ),
            (
                Recipe::new([Step::new("recorder"), Step::new("recorder").param("cells", 99)]),
                "step 2",
            ),
            (
                Recipe::new([Step::new("recorder").mask("nowhere")]),
                "which is no region",
            ),
            (Recipe::new(Vec::<Step>::new()), "at least one step"),
        ] {
            let result = rendered(&registry, &recipe, &Presets::new());
            assert!(
                matches!(&result, Err(error) if error.to_string().contains(said)),
                "{said}: {result:?}"
            );
        }
        assert_eq!(recorder.received().len(), 0);
        Ok(())
    }

    /// A map of the 4 x 6 picture whose every value is `level`.
    fn flat(level: f32) -> Result<Layer, Error> {
        Layer::new(4, 6, vec![level; 24])
    }

    #[test]
    fn a_mask_the_photo_has_no_map_of_refuses_the_render_as_that_maps_error_and_nothing_runs() -> Result<(), Error> {
        let registry = registry()?;
        let recorder = Arc::new(Recorder::new("recorder", &[], &[]));
        registry.register(recorder.clone())?;
        let recipe = |mask: &str| {
            let mut recipe = Recipe::new([Step::new("recorder"), Step::new("recorder").mask(mask)]);
            recipe.regions = json!({"cutout": {"kind": "path"}})
                .as_object()
                .cloned()
                .unwrap_or_default();
            recipe
        };
        // (mask, the kind of error the package raises it as); the maps handed in are `depth`, of the wrong size, and
        // `other`
        let small = Layer::new(2, 2, vec![0.5; 4])?;
        let values = values();
        let photo = Photo::new(Picture::new(4, 6, &values)?)
            .with_map("depth", small)
            .with_map("other", flat(0.5)?);
        for (mask, kind, said) in [
            ("subject", ErrorKind::SubjectNotFound, "'subject'"),
            ("cutout", ErrorKind::MapNotFound, "'cutout'"),
            ("light", ErrorKind::MapNotFound, "'light'"),
            ("depth", ErrorKind::DepthNotFound, "2 x 2"),
        ] {
            let result = photo.render_in(&registry, &recipe(mask), &Presets::new(), Options::default());
            assert!(
                matches!(&result, Err(error) if error.kind() == kind && error.to_string().contains(said)),
                "{mask}: {result:?}"
            );
        }
        assert_eq!(recorder.received().len(), 0);
        Ok(())
    }

    #[test]
    fn a_field_parameter_receives_the_map_its_step_names_or_the_effects_default() -> Result<(), Error> {
        let registry = registry()?;
        let recorder = Arc::new(Recorder::new("recorder", &[], &["near", "far"]));
        registry.register(recorder.clone())?;
        let values = values();
        let photo = Photo::new(Picture::new(4, 6, &values)?)
            .with_map("subject", flat(0.25)?)
            .with_map("NEAR", flat(0.75)?)
            .with_map("FAR", flat(0.5)?);
        let render = |step: Step| photo.render_in(&registry, &Recipe::new([step]), &Presets::new(), Options::default());
        render(Step::new("recorder").param("near", "subject"))?;
        render(Step::new("recorder"))?; // the recorder's default field is the parameter upcased
        let received = recorder.received();
        // each parameter under its own name, the named map or the default one
        assert_eq!(received[0].fields["near"], vec![0.25; 24]);
        assert_eq!(received[0].fields["far"], vec![0.5; 24]);
        assert_eq!(received[1].fields["near"], vec![0.75; 24]);
        assert_eq!(received[1].fields["far"], vec![0.5; 24]);
        let missing = render(Step::new("recorder").param("far", "depth"));
        assert!(
            matches!(&missing, Err(error) if error.kind() == ErrorKind::DepthNotFound),
            "{missing:?}"
        );
        let mistyped = render(Step::new("recorder").param("near", 3));
        assert!(
            matches!(&mistyped, Err(error @ Error::Recipe(_)) if error.to_string().contains("`near` names a field")),
            "{mistyped:?}"
        );
        Ok(())
    }

    #[test]
    fn an_effect_that_changes_the_pictures_shape_fails_naming_it() -> Result<(), Error> {
        let registry = registry()?;
        registry.register(Arc::new(Shrinks))?;
        let result = rendered(&registry, &Recipe::new([Step::new("shrinks")]), &Presets::new());
        assert!(
            matches!(&result, Err(error @ Error::Foreign { effect, .. })
                if effect == "shrinks" && error.to_string().contains("4 x 6") && error.to_string().contains("1 x 1")),
            "{result:?}"
        );
        Ok(())
    }

    #[test]
    fn each_step_runs_on_the_output_of_the_one_before_and_the_photo_is_left_as_it_was() -> Result<(), Error> {
        let registry = registry()?;
        registry.register(Arc::new(Paint { name: "paint" }))?;
        let recorder = Arc::new(Recorder::new("recorder", &[], &[]));
        registry.register(recorder.clone())?;
        let recipe = Recipe::new([
            Step::new("paint").param("level", 0.75).opacity(1.0),
            Step::new("recorder"),
        ]);
        let before = values();
        let photo = Photo::new(Picture::new(4, 6, &before)?);
        let image = photo.render_in(&registry, &recipe, &Presets::new(), Options::default())?;
        assert_eq!(image.into_values(), vec![0.75; 72]);
        assert_eq!(before, values());
        // what the second step was handed is the first one's picture, not the photo
        assert_eq!(recorder.received()[0].picture, vec![0.75; 72]);
        Ok(())
    }
}
