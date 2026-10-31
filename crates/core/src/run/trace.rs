//! Seeing a render step by step: [`Photo::trace`].

use super::Photo;
use crate::registry::Registry;
use crate::{Error, Image, Layer, Options, Presets, Recipe};

/// One picture, or map, from inside a render.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Stage {
    /// The photo, as the recipe receives it.
    Input(Image),
    /// The subject as the steps work on it (framed and sized), if the recipe uses it (a mask, a field, a drive or
    /// `background`) or its `frame` needs it (`anchor` `Subject`, `transparent` `Background`): white where it is.
    Subject(Layer),
    /// The depth, if the recipe uses it: 1 where it is nearest.
    Depth(Layer),
    /// The picture after the step at `number` (from 1).
    Step {
        /// The step's place in the recipe, counting from 1.
        number: usize,
        /// The picture after it.
        image: Image,
    },
    /// The picture after the last step, cut out as the recipe's `frame.transparent` says: four channels, the last the
    /// alpha. What [`render`](Photo::render) returns; there is no such stage for a recipe that cuts nothing out.
    CutOut(Image),
}

impl Photo<'_> {
    /// Every stage of rendering `recipe`: the photo, the subject and depth if the recipe uses them, the picture after
    /// each step and, if the recipe's frame cuts the picture out, the cut-out picture, in that order. It runs exactly the code [`render`](Self::render) does, so the last stage is what
    /// `render` returns, byte for byte, and it fails where `render` would. Where `render` keeps only the newest
    /// picture, a trace keeps one picture per step, as many as the recipe has steps.
    ///
    /// ```text
    /// for stage in photo.trace(&look, &Presets::new(), Options::default())? {
    ///     match stage {
    ///         Stage::Step { number, image } => save(number, image),
    ///         _ => {}
    ///     }
    /// }
    /// ```
    ///
    /// # Errors
    /// As [`render`](Self::render).
    pub fn trace(&self, recipe: &Recipe, presets: &Presets, options: Options) -> Result<Vec<Stage>, Error> {
        self.trace_in(crate::registry::registry(), recipe, presets, options)
    }

    /// As [`trace`](Self::trace), with the effects of `registry`.
    pub(crate) fn trace_in(
        &self,
        registry: &Registry,
        recipe: &Recipe,
        presets: &Presets,
        options: Options,
    ) -> Result<Vec<Stage>, Error> {
        let mut run = self.start(registry, recipe, presets, options)?;
        let picture = run.working.picture();
        let mut stages = vec![Stage::Input(Image::new(
            picture.height(),
            picture.width(),
            picture.values(),
        )?)];
        stages.extend(run.subject.take().map(|layer| Stage::Subject(Layer::clone(&layer))));
        stages.extend(run.depth.take().map(|layer| Stage::Depth(Layer::clone(&layer))));
        let mut number = 0;
        while let Some(image) = run.advance()? {
            number += 1;
            stages.push(Stage::Step {
                number,
                image: image.clone(),
            });
        }
        if run.cut.is_some() {
            stages.push(Stage::CutOut(run.finish()?));
        }
        Ok(stages)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::registry::Registry;
    use crate::testing::{Finds, Paint, Recorder, registry};
    use crate::{BlendMode, Drive, Picture, Step};

    /// The values of a 4 x 6 picture, a different one in every channel.
    fn values() -> Vec<f32> {
        (0_u8..72).map(|index| f32::from(index) / 80.0).collect()
    }

    /// A 4 x 6 map whose values step through `pattern`, repeated.
    fn map(pattern: [f32; 4]) -> Result<Layer, Error> {
        Layer::new(4, 6, (0..24).map(|index| pattern[index % 4]).collect())
    }

    /// A registry holding `paint` and a `recorder` with the field parameter `near`.
    fn rig() -> Result<Registry, Error> {
        let registry = registry()?;
        registry.register(Arc::new(Paint { name: "paint" }))?;
        registry.register(Arc::new(Recorder::new("recorder", &[], &["near"])))?;
        Ok(registry)
    }

    fn paint(level: f64) -> Step {
        Step::new("paint").param("level", level)
    }

    /// The kinds of `stages` in order: `input`, `subject`, `depth`, `step 2`.
    fn kinds(stages: &[Stage]) -> Vec<String> {
        stages
            .iter()
            .map(|stage| match stage {
                Stage::Input(_) => "input".to_owned(),
                Stage::Subject(_) => "subject".to_owned(),
                Stage::Depth(_) => "depth".to_owned(),
                Stage::Step { number, .. } => format!("step {number}"),
                Stage::CutOut(_) => "cut out".to_owned(),
            })
            .collect()
    }

    /// The photo of `values` with a handed-in subject and depth.
    fn photo(values: &[f32]) -> Result<Photo<'_>, Error> {
        Photo::new(Picture::new(4, 6, values)?)
            .with_map("subject", map([0.0, 0.25, 1.0, 0.5])?)?
            .with_map("depth", map([1.0, 0.5, 0.0, 0.75])?)
    }

    /// Recipes that between them take every path of a step, each with a name for the failure message.
    fn recipes() -> Vec<(&'static str, Recipe)> {
        let driven = paint(0.0).param("level", Drive::new("depth", 0.2, 0.8));
        vec![
            ("plain", Recipe::new([paint(0.2).opacity(1.0), paint(0.7)])),
            (
                "masked",
                Recipe::new([paint(0.9).mask("subject"), paint(0.4).opacity(0.5)]),
            ),
            (
                "blended",
                Recipe::new([paint(0.6).blend_mode(BlendMode::Multiply).opacity(0.5)]),
            ),
            ("driven", Recipe::new([driven.opacity(1.0)])),
            (
                "background",
                Recipe::new([paint(0.3).mask("background"), paint(0.5).mask("light")]),
            ),
        ]
    }

    fn rendered(rig: &Registry, photo: &Photo<'_>, recipe: &Recipe) -> Result<Image, Error> {
        photo.render_in(rig, recipe, &Presets::new(), Options::default())
    }

    fn traced(rig: &Registry, photo: &Photo<'_>, recipe: &Recipe) -> Result<Vec<Stage>, Error> {
        photo.trace_in(rig, recipe, &Presets::new(), Options::default())
    }

    #[test]
    fn the_last_stage_of_a_trace_is_exactly_what_render_returns() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        let photo = photo(&values)?;
        for (name, recipe) in recipes() {
            let stages = traced(&rig, &photo, &recipe)?;
            let last = stages.last();
            let expected = rendered(&rig, &photo, &recipe)?;
            assert!(
                matches!(last, Some(Stage::Step { number, image }) if *number == recipe.steps.len() && *image == expected),
                "{name}: {last:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn the_picture_after_each_step_is_what_the_recipe_cut_off_after_that_step_renders() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        let photo = photo(&values)?;
        for (name, recipe) in recipes() {
            let stages = traced(&rig, &photo, &recipe)?;
            let steps: Vec<&Stage> = stages
                .iter()
                .filter(|stage| matches!(stage, Stage::Step { .. }))
                .collect();
            assert_eq!(steps.len(), recipe.steps.len(), "{name}");
            for (index, stage) in steps.into_iter().enumerate() {
                let mut prefix = recipe.clone();
                prefix.steps.truncate(index + 1);
                let image = rendered(&rig, &photo, &prefix)?;
                assert_eq!(
                    *stage,
                    Stage::Step {
                        number: index + 1,
                        image
                    },
                    "{name}, step {}",
                    index + 1
                );
            }
        }
        Ok(())
    }

    #[test]
    fn the_stages_are_the_input_the_maps_the_recipe_uses_and_one_picture_per_step() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        let photo = photo(&values)?;
        let near = |map: &str| Step::new("recorder").param("near", map);
        // (the recipe, the stages it makes)
        let rows = [
            (Recipe::new([paint(0.5), paint(0.6)]), vec!["input", "step 1", "step 2"]),
            (
                Recipe::new([paint(0.5).mask("subject")]),
                vec!["input", "subject", "step 1"],
            ),
            (Recipe::new([near("depth")]), vec!["input", "depth", "step 1"]),
            (
                Recipe::new([near("depth"), near("subject")]),
                vec!["input", "subject", "depth", "step 1", "step 2"],
            ),
            // `background` is made from the subject, `light` from the picture
            (Recipe::new([near("background")]), vec!["input", "subject", "step 1"]),
            (Recipe::new([near("light")]), vec!["input", "step 1"]),
        ];
        for (recipe, expected) in rows {
            assert_eq!(kinds(&traced(&rig, &photo, &recipe)?), expected, "{:?}", recipe.steps);
        }
        // what the stages hold: the photo, the maps as handed in
        let stages = traced(&rig, &photo, &Recipe::new([near("depth"), near("subject")]))?;
        assert_eq!(stages[0], Stage::Input(Image::new(4, 6, &values)?));
        assert_eq!(stages[1], Stage::Subject(map([0.0, 0.25, 1.0, 0.5])?));
        assert_eq!(stages[2], Stage::Depth(map([1.0, 0.5, 0.0, 0.75])?));
        Ok(())
    }

    #[test]
    fn a_trace_finds_its_maps_like_a_render_does_and_fails_where_it_would() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        let finder = Arc::new(Finds::flat(0.5));
        let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", finder.clone());
        let uses_subject = Recipe::new([Step::new("recorder").param("near", "subject")]);
        traced(&rig, &photo, &uses_subject)?;
        rendered(&rig, &photo, &uses_subject)?;
        assert_eq!(finder.calls(), 1);
        // no depth: the same error as a render's
        let uses_depth = Recipe::new([Step::new("recorder").param("near", "depth")]);
        let (trace, render) = (traced(&rig, &photo, &uses_depth), rendered(&rig, &photo, &uses_depth));
        assert!(
            matches!((&trace, &render), (Err(a), Err(b)) if a.kind() == b.kind() && a.to_string() == b.to_string())
        );
        Ok(())
    }
}
