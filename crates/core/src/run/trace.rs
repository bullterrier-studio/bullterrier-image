//! Seeing a render step by step: [`Photo::trace`].

use super::Photo;
use crate::ops::par;
use crate::registry::Registry;
use crate::{Error, Image, Labels, Layer, Options, Presets, Recipe};

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
    /// A region of the recipe that is a selection, as the steps work with it: how much each pixel of the working picture
    /// is chosen. One stage for each region that a step uses, or that a region a step uses is made from, in the order the
    /// recipe declares them (a `given` region too, as it was handed in).
    Selection {
        /// The region's name in the recipe.
        name: String,
        /// How much each pixel is chosen.
        layer: Layer,
    },
    /// A region of the recipe that is a partition, as the steps work with it: the piece each pixel belongs to. Shown
    /// like [`Selection`](Self::Selection), among them in the order of declaration.
    Partition {
        /// The region's name in the recipe.
        name: String,
        /// The piece of each pixel, from 0.
        labels: Labels,
    },
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
    /// Every stage of rendering `recipe`: the photo, the subject and depth if the recipe uses them, each region of the
    /// recipe that the steps use (or that a region they use is made from) in the order the recipe declares them, the
    /// picture after each step and, if the recipe's frame cuts the picture out, the cut-out picture, in that order. It
    /// runs exactly the code [`render`](Self::render) does, so the last stage is what
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
        par::install(options.threads(), || {
            let mut run = self.start(registry, recipe, presets, options)?;
            let picture = run.working.picture();
            let mut stages = vec![Stage::Input(Image::new(
                picture.height(),
                picture.width(),
                picture.values(),
            )?)];
            stages.extend(
                run.regions
                    .held("subject")
                    .map(|layer| Stage::Subject(Layer::clone(&layer))),
            );
            stages.extend(
                run.regions
                    .held("depth")
                    .map(|layer| Stage::Depth(Layer::clone(&layer))),
            );
            stages.extend(run.regions.shown(&picture)?);
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
        })?
    }
}
