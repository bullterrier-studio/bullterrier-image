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

mod cut;
mod each;
mod frame;
mod laid;
mod maps;
mod regions;
mod size;
mod step;
mod trace;

use cut::Cut;
pub use maps::Finder;
use maps::{Found, Maps};
use regions::Evaluator;
pub use size::Options;
use size::Working;
use step::{Prepared, prepare};
pub use trace::Stage;

use crate::registry::Registry;
use crate::{Error, Image, Layer, Picture, Presets, Recipe};

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
    /// again: its answer is kept for every later render. (A recipe with a `subject` region found `per` piece also asks
    /// the finder of `subject`, whatever was handed in, about the crop of each piece, and those answers are not kept.)
    /// Share one finder between photos with an [`Arc`].
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
    /// (the Rec. 709 brightness of the picture the steps work on). A region made from them (`band`, `bands`, `combine`,
    /// the gradients, `grid`, `pick`, `subject`) is made for the working picture when its step comes and dropped after
    /// the last step that needs it; one declared and not used costs nothing. A step that takes a `partition` receives
    /// the pieces of the region it names (a selection is split into the pixels it chooses, above one half, and the
    /// others). A `subject` region found `per` piece needs the photo's `subject` finder (a handed in `subject` map does
    /// not do) and asks it about each piece's crop of the working picture. A recipe whose steps use a region the core
    /// does not make yet (`shards`, `cuts`, `path`, `svg`) is refused before anything runs, and so is one
    /// that needs the subject finder of a photo that has none.
    ///
    /// An `each` step gives each piece of its partition a look (see [`Each`](crate::Each)): every look that has a piece is run once
    /// over the whole working picture and shown by the weights of its pieces, which are 1 inside them and 0 outside,
    /// softened across the borders by a Gaussian of `feather` design pixels; a piece with no look keeps what the step
    /// received. The step's `mask`, `opacity` (none: 1) and `blend_mode` lay the result over its input.
    ///
    /// The steps work on the photo as the recipe's `frame` frames it (flipped, cropped, given borders: the photo and every
    /// map move together; a crop around the subject needs the `subject` map), at the size `options` ask for, else the
    /// recipe's `size`, else the framed picture's own: the photo and every map are resampled to it with the aspect kept
    /// (see [`Options::with_size`]). A finder is still given the photo as it was handed in. The recipe's pixel values are
    /// scaled by the working picture's longest side over 480.
    ///
    /// After the last step a `frame.transparent` cut gives the picture its alpha channel (see [`Image`]): the inscribed
    /// circle, corners rounded by `corner_radius` of the shorter side, or the subject map of the working picture,
    /// which this render then needs. The colour is as the steps made it.
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
        let reach = recipe.check_in(registry, presets)?.reach(&recipe.regions);
        regions::check_makeable(
            &recipe.regions,
            reach
                .iter()
                .map(|reached| (reached.name.as_str(), reached.place.as_str())),
            self.finders.contains_key("subject"),
        )?;
        // the subject the frame needs (a crop around it, a cut by it), found before anything is made
        let needs_subject = recipe.frame.needs_subject();
        let given_subject = needs_subject
            .then(|| maps::stored(self, recipe, "subject", "`frame`"))
            .transpose()?;
        let working = Working::new(
            self.picture,
            &recipe.frame,
            options.size().or(recipe.size),
            given_subject.as_deref(),
        )?;
        let ratio = working.ratio();
        let maps = Maps::new(self, recipe, &working)?;
        // as the working picture has it, which a trace shows like a map a step uses
        let framed_subject = needs_subject.then(|| maps.get("subject", "`frame`")).transpose()?;
        let cut = Cut::plan(&recipe.frame, &working, framed_subject.clone())?;
        let steps = recipe
            .steps
            .iter()
            .enumerate()
            .map(|(index, entry)| prepare(registry, presets, ratio, entry, &format!("step {}", index + 1)))
            .collect::<Result<Vec<_>, _>>()?;
        // every map the photo gives that the steps use is found now, so that none is missing when its step runs
        let mut held = BTreeMap::new();
        for reached in reach.iter().filter(|reached| maps.gives(&reached.name)) {
            held.insert(reached.name.clone(), maps.get(&reached.name, &reached.place)?);
        }
        if let Some(subject) = framed_subject {
            held.entry("subject".to_owned()).or_insert(subject);
        }
        let last = reach.into_iter().map(|reached| (reached.name, reached.last)).collect();
        Ok(Run {
            working,
            ratio,
            steps: steps.into_iter(),
            ran: 0,
            current: None,
            cut,
            regions: Evaluator::new(
                &recipe.regions,
                held,
                last,
                self.finders.get("subject").map(Found::finder),
            ),
        })
    }
}

/// A recipe being run, the one engine of `render` and `trace`: the steps still to run and the newest image. Each step
/// takes the image before it and drops it once its own is made, so no earlier image is kept; whoever wants them (a
/// trace) copies what [`advance`](Self::advance) shows.
struct Run<'a> {
    /// The picture the steps work on: the photo, or the copy of it at the size asked for.
    working: Working<'a>,
    ratio: f64,
    steps: std::vec::IntoIter<Prepared>,
    /// How many steps have run.
    ran: usize,
    current: Option<Image>,
    /// How the image after the last step is cut out, if the recipe's frame says so.
    cut: Option<Cut>,
    /// The selections the steps work from, each dropped once the last step that needs it has run.
    regions: Evaluator,
}

impl Run<'_> {
    /// Runs the next step, and shows the image it made; none once all have run.
    fn advance(&mut self) -> Result<Option<&Image>, Error> {
        let Some(step) = self.steps.next() else {
            return Ok(None);
        };
        let before = self
            .current
            .as_ref()
            .map_or_else(|| self.working.picture(), Image::as_picture);
        let after = step.apply(&before, self.ratio, &mut self.regions, &self.working.picture())?;
        self.regions.release(self.ran);
        self.ran += 1;
        self.current = Some(after);
        Ok(self.current.as_ref())
    }

    /// The image after the last step, cut out if the recipe's frame says so.
    fn finish(self) -> Result<Image, Error> {
        let image = self
            .current
            .ok_or_else(|| Error::Recipe("a recipe needs at least one step".to_owned()))?;
        Ok(match &self.cut {
            Some(cut) => cut.apply(image),
            None => image,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::region::Given;
    use crate::testing::{Finds, Paint, Recorder, Shrinks, registry, with_recorder};
    use crate::{Drive, ErrorKind, Step};

    /// The values of a 4 x 6 picture, a different one in every channel.
    fn values() -> Vec<f32> {
        (0_u8..72).map(|index| f32::from(index) / 80.0).collect()
    }

    /// `recipe` rendered on the 4 x 6 picture by the registry `registry`.
    fn rendered(registry: &Registry, recipe: &Recipe, presets: &Presets) -> Result<Image, Error> {
        let values = values();
        Photo::new(Picture::new(4, 6, &values)?).render_in(registry, recipe, presets, Options::default())
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
            recipe.regions.insert("NEAR", Given {});
            recipe.regions.insert("FAR", Given {});
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
