//! Running a recipe on a photo: [`Photo::render`].
//!
//! ```text
//! let picture = Picture::new(height, width, &values)?;
//! let look = Recipe::new([Step::new("grade").param("exposure", 0.5), Step::new("pixel_grid")]);
//! let image = Photo::new(picture).render(&look, &Presets::new(), Options::default())?;
//! ```

use std::sync::Arc;

use serde_json::Value;

use crate::effect::{self, Effect};
use crate::registry::Registry;
use crate::{BlendMode, Entry, Error, Image, Inputs, Picture, Presets, Recipe};

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

/// A picture a recipe is rendered on. It borrows the picture: rendering never copies or changes it.
#[derive(Clone, Copy, Debug)]
pub struct Photo<'a> {
    picture: Picture<'a>,
}

impl<'a> Photo<'a> {
    /// The photo whose picture is `picture`.
    #[must_use]
    pub const fn new(picture: Picture<'a>) -> Self {
        Self { picture }
    }

    /// The picture `recipe` makes of this photo: its steps run in order, each on the previous one's output, and only
    /// that output is kept. `presets` holds the presets the steps name.
    ///
    /// The recipe is checked first by the rules of [`Recipe::checked`], so nothing runs for a recipe that breaks one;
    /// each step's effect then receives its complete parameters: the step's values over its preset's, the declared
    /// defaults for the rest. A step with an `opacity`, `mask` or `blend_mode`, an `each` step, and a recipe with a
    /// `frame` or a `size` are refused until the core runs them.
    ///
    /// # Errors
    /// Whatever [`Recipe::checked`] refuses the recipe for, [`Error::Recipe`] for a step or recipe the core does not
    /// run yet, and [`Error::Foreign`] if an effect that runs elsewhere fails or returns a picture of another size.
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
            .map(|(index, entry)| prepare(registry, presets, ratio, entry, &format!("step {}", index + 1)))
            .collect::<Result<Vec<_>, _>>()?;
        let offered = Inputs::default().with_size_ratio(ratio);
        let mut current: Option<Image> = None;
        for step in &steps {
            let before = current.as_ref().map_or(self.picture, Image::as_picture);
            let after = effect::run(&*step.effect, &before, step.params.clone(), &offered)?;
            current = Some(after);
        }
        current.ok_or_else(|| Error::Recipe("a recipe needs at least one step".to_owned()))
    }
}

/// A step ready to run: its effect and the complete parameters it receives.
struct Prepared {
    effect: Arc<dyn Effect>,
    params: Value,
}

/// The step `entry` (at `place`, as `step 2`) resolved: its parameters complete, or why the core does not run it yet.
fn prepare(registry: &Registry, presets: &Presets, ratio: f64, entry: &Entry, place: &str) -> Result<Prepared, Error> {
    let Entry::Step(step) = entry else {
        return Err(Error::Recipe(format!(
            "{place}: an `each` step is not run by the core yet"
        )));
    };
    let effect = registry.effect(&step.effect)?;
    let opacity = step.opacity.unwrap_or_else(|| effect.opacity());
    let unsupported = |what: String| Error::Recipe(format!("{place}: {what} is not run by the core yet"));
    if opacity < 1.0 {
        return Err(unsupported(format!("`opacity` {opacity}")));
    }
    if step.mask.is_some() {
        return Err(unsupported("`mask`".to_owned()));
    }
    if step.blend_mode != BlendMode::Normal {
        return Err(unsupported("`blend_mode`".to_owned()));
    }
    let given = presets.under(step, place)?;
    let params = match effect.params() {
        Some(table) => table.scaled(
            table.check(&step.effect, &given).map_err(|error| error.at(place))?,
            ratio,
        ),
        None => given,
    };
    Ok(Prepared {
        effect,
        params: Value::Object(params),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::Each;
    use crate::Step;
    use crate::params::Declared;
    use crate::params::probe::Probe;
    use crate::testing::{Paint, Recorder, Shrinks, registry};

    /// The values of a 4 x 6 picture, a different one in every channel.
    fn values() -> Vec<f32> {
        (0_u8..72).map(|index| f32::from(index) / 80.0).collect()
    }

    /// `recipe` rendered on the 4 x 6 picture by the registry `registry`.
    fn rendered(registry: &Registry, recipe: &Recipe, presets: &Presets) -> Result<Image, Error> {
        let values = values();
        Photo::new(Picture::new(4, 6, &values)?).render_in(registry, recipe, presets, Options::default())
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
        // `paint` declares 0.25; this is not run until opacity is
        let result = rendered(&registry, &Recipe::new([paint()]), &Presets::new());
        assert!(
            matches!(&result, Err(error @ Error::Recipe(_)) if error.to_string().contains("`opacity` 0.25")),
            "{result:?}"
        );
        let full = rendered(&registry, &Recipe::new([paint().opacity(1.0)]), &Presets::new())?;
        assert_eq!(full.into_values(), vec![0.5; 72]);
        Ok(())
    }

    #[test]
    fn a_step_that_is_not_run_yet_is_refused_naming_what_and_where() -> Result<(), Error> {
        let registry = registry()?;
        let mut recipe_with_frame = Recipe::new([Step::new("probe").opacity(1.0)]);
        recipe_with_frame.frame = json!({"flip": "horizontal"}).as_object().cloned().unwrap_or_default();
        let mut recipe_with_size = Recipe::new([Step::new("probe").opacity(1.0)]);
        recipe_with_size.size = Some(480);
        let mut recipe_with_each = Recipe::new([Each::new("panes", [None])]);
        recipe_with_each.regions = json!({"panes": {"kind": "grid"}})
            .as_object()
            .cloned()
            .unwrap_or_default();
        for (recipe, said) in [
            (Recipe::new([Step::new("probe").opacity(0.5)]), "step 1: `opacity` 0.5"),
            (
                Recipe::new([Step::new("probe").opacity(1.0).mask("subject")]),
                "step 1: `mask`",
            ),
            (
                Recipe::new([
                    Step::new("probe").opacity(1.0),
                    Step::new("probe").opacity(1.0).blend_mode(BlendMode::Screen),
                ]),
                "step 2: `blend_mode`",
            ),
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
