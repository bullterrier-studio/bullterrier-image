//! Running a recipe on a photo: [`Photo::render`].
//!
//! ```text
//! let picture = Picture::new(height, width, &values)?;
//! let look = Recipe::new([Step::new("grade").param("exposure", 0.5), Step::new("pixel_grid")]);
//! let photo = Photo::new(picture).with_finder("subject", Arc::new(my_subject_finder));
//! let image = photo.render(&look, &Presets::new(), Options::default())?;
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

mod maps;
mod trace;

pub use maps::Finder;
use maps::{Found, Maps};
pub use trace::Stage;

use serde_json::{Map, Value};

use crate::effect::{self, Effect};
use crate::ops::{blend, levels};
use crate::recipe::drive;
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

/// A picture a recipe is rendered on, with the maps of it that the recipe names (`subject`, `depth`, a region): handed in
/// or found by a [`Finder`]. It borrows the picture: rendering never copies or changes it.
///
/// A map is found the first time a render needs it and kept, so rendering several recipes on one photo finds each map
/// once. A photo can be shared between threads.
pub struct Photo<'a> {
    picture: Picture<'a>,
    maps: BTreeMap<String, Arc<Layer>>,
    finders: BTreeMap<String, Found>,
}

impl fmt::Debug for Photo<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Photo")
            .field("height", &self.picture.height())
            .field("width", &self.picture.width())
            .field("maps", &self.maps.keys().collect::<Vec<_>>())
            .field("finders", &self.finders.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl<'a> Photo<'a> {
    /// The photo whose picture is `picture`, with no maps.
    #[must_use]
    pub const fn new(picture: Picture<'a>) -> Self {
        Self {
            picture,
            maps: BTreeMap::new(),
            finders: BTreeMap::new(),
        }
    }

    /// This photo with `map` for the name `name`: `subject` (white = subject), `depth` (1 = nearest), or a region of kind
    /// `given` the recipe declares. A step's `mask`, a drive's `by` and an effect's field parameters find it there. A
    /// map of the same name given before is replaced. A render refuses a photo with a map that is no map of its recipe,
    /// used or not.
    ///
    /// # Errors
    /// [`Error::MapNotFound`] (of the kind of `name`, see [`Error::kind`]) if `map` is not of the picture's size.
    pub fn with_map(mut self, name: impl Into<String>, map: Layer) -> Result<Self, Error> {
        let name = name.into();
        let map = maps::fits(&name, map, &self.picture)?;
        self.maps.insert(name, Arc::new(map));
        Ok(self)
    }

    /// This photo with `finder` to find the map `name` (as for [`with_map`](Self::with_map)). It is asked, with the picture
    /// as it was given, when a recipe rendered on the photo uses the map and none was handed in under that name, and not
    /// again: its answer is kept for every later render. Share one finder between photos with an [`Arc`].
    #[must_use]
    pub fn with_finder(mut self, name: impl Into<String>, finder: Arc<dyn Finder>) -> Self {
        self.finders.insert(name.into(), Found::new(finder));
        self
    }

    /// The picture `recipe` makes of this photo: its steps run in order, each on the previous one's output, and only
    /// that output is kept. `presets` holds the presets the steps name.
    ///
    /// The recipe is checked first by the rules of [`Recipe::checked`], so nothing runs for a recipe that breaks one;
    /// each step's effect then receives its complete parameters: the step's values over its preset's, the declared
    /// defaults for the rest, scaled to `options`. The step's result is laid over its input as an image editor lays a
    /// layer: by its blend mode, mixed in by its opacity (none: the effect's own) and, where it names a `mask`, only
    /// as far as the mask says. Every map the steps use is looked up, and found if need be, before the first step runs:
    /// the maps the photo was given and the ones it is made to find, plus `background` (1 - the subject) and `light`
    /// (the Rec. 709 brightness of the photo as it is given). An `each` step and a recipe with a `frame` or a `size` are
    /// refused until the core runs them.
    ///
    /// # Errors
    /// Whatever [`Recipe::checked`] refuses the recipe for, [`Error::Recipe`] for a step or recipe the core does not
    /// run yet or a photo given a map or finder that is no map of the recipe, [`Error::MapNotFound`] for a map a step
    /// uses that the photo has neither of nor a finder for (or whose finder fails or answers one of another size), and
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
        let mut run = self.start(registry, recipe, presets, options)?;
        while run.advance()?.is_some() {}
        run.finish()
    }

    /// `recipe` checked and every step of it prepared, with all the maps they use found: ready to run.
    fn start(
        &self,
        registry: &Registry,
        recipe: &Recipe,
        presets: &Presets,
        options: Options,
    ) -> Result<Run<'a>, Error> {
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
        let maps = Maps::new(self, recipe)?;
        let steps = recipe
            .steps
            .iter()
            .enumerate()
            .map(|(index, entry)| prepare(registry, presets, ratio, &maps, entry, &format!("step {}", index + 1)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Run {
            picture: self.picture,
            ratio,
            steps: steps.into_iter(),
            current: None,
            subject: maps.resolved("subject"),
            depth: maps.resolved("depth"),
        })
    }
}

/// A recipe being run, the one engine of `render` and `trace`: the steps still to run and the newest image. Each step
/// takes the image before it and drops it once its own is made, so no earlier image is kept; whoever wants them (a
/// trace) copies what [`advance`](Self::advance) shows.
struct Run<'a> {
    picture: Picture<'a>,
    ratio: f64,
    steps: std::vec::IntoIter<Prepared>,
    current: Option<Image>,
    /// The subject, if some step uses it (a mask, a field, a drive, `background`).
    subject: Option<Arc<Layer>>,
    /// The depth, if some step uses it.
    depth: Option<Arc<Layer>>,
}

impl Run<'_> {
    /// Runs the next step, and shows the image it made; none once all have run.
    fn advance(&mut self) -> Result<Option<&Image>, Error> {
        let Some(step) = self.steps.next() else {
            return Ok(None);
        };
        let before = self.current.as_ref().map_or(self.picture, Image::as_picture);
        let mut offered = Inputs::default().with_size_ratio(self.ratio);
        for (parameter, layer) in &step.fields {
            offered = offered.with_field(parameter.as_str(), layer);
        }
        let after = step.run(&before, &offered)?;
        self.current = Some(step.lay(&before, after));
        Ok(self.current.as_ref())
    }

    /// The image after the last step.
    fn finish(self) -> Result<Image, Error> {
        self.current
            .ok_or_else(|| Error::Recipe("a recipe needs at least one step".to_owned()))
    }
}

/// The step `entry` (at `place`, as `step 2`) resolved: its parameters complete and scaled, the maps it names
/// found, or why the core does not run it yet.
fn prepare(
    registry: &Registry,
    presets: &Presets,
    ratio: f64,
    maps: &Maps<'_, '_>,
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
        fields.push((
            field.parameter.clone(),
            maps.get(name, &format!("{place}, parameter `{}`", field.parameter))?,
        ));
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
            by: maps.get(&first.by, &format!("{place}, drive `by`"))?,
            top: first.levels - 1,
            levels,
        }
    } else {
        Runs::One(complete(&*effect, &step.effect, given, ratio, place)?)
    };
    Ok(Prepared {
        opacity: step.opacity.unwrap_or_else(|| effect.opacity()),
        blend_mode: step.blend_mode,
        mask: step
            .mask
            .as_deref()
            .map(|name| maps.get(name, &format!("{place}, `mask`")))
            .transpose()?,
        fields,
        effect,
        runs,
    })
}

/// The complete parameters a step's effect is run with.
enum Runs {
    /// A plain step: one run.
    One(Value),
    /// A step with driven parameters: one run for each level, `levels[n]` for level `n` (0 to `top`), mixed by the map.
    Driven {
        by: Arc<Layer>,
        top: u8,
        levels: Vec<Value>,
    },
}

/// A step ready to run: its effect, the complete parameters it receives, the maps it works from and how its result is
/// laid over its input.
struct Prepared {
    effect: Arc<dyn Effect>,
    runs: Runs,
    /// The field parameters of the effect, each with its map.
    fields: Vec<(String, Arc<Layer>)>,
    opacity: f64,
    blend_mode: BlendMode,
    mask: Option<Arc<Layer>>,
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
    /// The effect run on `before`. A driven step runs it once for each level that some pixel is on or next to, and each
    /// pixel mixes the runs of the two levels either side of its map value, linearly; a level none is near is not run.
    fn run(&self, before: &Picture<'_>, offered: &Inputs<'_>) -> Result<Image, Error> {
        let (by, top, levels) = match &self.runs {
            Runs::One(params) => return effect::run(&*self.effect, before, params.clone(), offered),
            Runs::Driven { by, top, levels } => (by, *top, levels),
        };
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

    /// `after`, the effect's result for `before`, laid over `before` as the step says. A plain step (blend mode
    /// normal, opacity 1, no mask) is its result as it is, uncopied.
    fn lay(&self, before: &Picture<'_>, after: Image) -> Image {
        if self.opacity >= 1.0 && self.mask.is_none() && self.blend_mode == BlendMode::Normal {
            after
        } else {
            blend::lay(
                before,
                &after.as_picture(),
                self.blend_mode,
                self.opacity,
                self.mask.as_deref(),
            )
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
    use crate::testing::{Finds, Measured, Paint, Recorder, Shrinks, registry};
    use crate::{Drive, Each, ErrorKind, Step};

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
    fn a_field_parameter_receives_the_map_its_step_names_or_the_effects_default() -> Result<(), Error> {
        let registry = registry()?;
        let recorder = Arc::new(Recorder::new("recorder", &[], &["near", "far"]));
        registry.register(recorder.clone())?;
        let values = values();
        let photo = Photo::new(Picture::new(4, 6, &values)?)
            .with_map("subject", flat(0.25)?)?
            .with_map("NEAR", flat(0.75)?)?
            .with_map("FAR", flat(0.5)?)?;
        let render = |step: Step| {
            let mut recipe = Recipe::new([step]);
            recipe.regions = json!({"NEAR": {"kind": "given"}, "FAR": {"kind": "given"}})
                .as_object()
                .cloned()
                .unwrap_or_default();
            photo.render_in(&registry, &recipe, &Presets::new(), Options::default())
        };
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

    /// The map `subject` that an effect with the field parameter `near` receives when `photo` is rendered with a step
    /// that sets `near` to `subject`.
    fn subject_received(registry: &Registry, recorder: &Recorder, photo: &Photo<'_>) -> Result<Vec<f32>, Error> {
        let step = Step::new("recorder").param("near", "subject");
        photo.render_in(registry, &Recipe::new([step]), &Presets::new(), Options::default())?;
        let received = recorder.received();
        Ok(received[received.len() - 1].fields["near"].clone())
    }

    #[test]
    fn a_map_given_again_under_the_same_name_replaces_the_first() -> Result<(), Error> {
        let registry = registry()?;
        let recorder = Arc::new(Recorder::new("recorder", &[], &["near"]));
        registry.register(recorder.clone())?;
        let values = values();
        let photo = Photo::new(Picture::new(4, 6, &values)?)
            .with_map("subject", flat(0.25)?)?
            .with_map("subject", flat(0.75)?)?;
        assert_eq!(subject_received(&registry, &recorder, &photo)?, vec![0.75; 24]);
        Ok(())
    }

    #[test]
    fn a_finder_given_again_under_the_same_name_replaces_the_first() -> Result<(), Error> {
        let registry = registry()?;
        let recorder = Arc::new(Recorder::new("recorder", &[], &["near"]));
        registry.register(recorder.clone())?;
        let values = values();
        let (first, second) = (Arc::new(Finds::flat(0.25)), Arc::new(Finds::flat(0.75)));
        let photo = Photo::new(Picture::new(4, 6, &values)?)
            .with_finder("subject", first.clone())
            .with_finder("subject", second.clone());
        assert_eq!(subject_received(&registry, &recorder, &photo)?, vec![0.75; 24]);
        assert_eq!((first.calls(), second.calls()), (0, 1));
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
        // `length` is px with a minimum of 2: from 3 (map 0) to 8 (map 1) at half size: 1.5 is held at 2, 4 stays 4
        let step = Step::new("measure").param("length", Drive::new("subject", 3.0, 8.0).levels(2));
        let (values, subject) = banded(&[0.0, 1.0])?;
        Photo::new(Picture::new(1, 2, &values)?)
            .with_map("subject", subject)?
            .render_in(
                &registry,
                &Recipe::new([step]),
                &Presets::new(),
                Options::default().with_size_ratio(0.5),
            )?;
        let lengths: Vec<Value> = recorder
            .received()
            .iter()
            .map(|received| received.params["length"].clone())
            .collect();
        assert_eq!(lengths, [json!(2.0), json!(4.0)]);
        Ok(())
    }

    #[test]
    fn a_drive_the_photo_has_no_map_for_or_that_breaks_a_rule_refuses_the_render_and_nothing_runs() -> Result<(), Error>
    {
        let registry = registry()?;
        let recorder = Arc::new(Recorder::new("recorder", &[], &[]));
        registry.register(recorder.clone())?;
        let drive = |by: &str| Drive::new(by, 0.0, 1.0);
        let values = values();
        let photo = Photo::new(Picture::new(4, 6, &values)?);
        for (step, said) in [
            (Step::new("recorder").param("x", drive("depth")), "'depth'"),
            (
                Step::new("recorder")
                    .param("x", drive("depth"))
                    .param("y", drive("light")),
                "one field",
            ),
            (
                Step::new("recorder")
                    .param("x", drive("depth"))
                    .param("y", drive("depth").levels(3)),
                "one number of levels",
            ),
            (
                Step::new("recorder").param("x", drive("depth").levels(17)),
                "from 2 to 16",
            ),
        ] {
            let result = photo.render_in(&registry, &Recipe::new([step]), &Presets::new(), Options::default());
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
