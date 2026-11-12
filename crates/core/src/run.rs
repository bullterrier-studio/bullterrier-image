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

mod assets;
mod band;
mod cut;
mod each;
mod frame;
mod laid;
mod many;
mod maps;
mod regions;
mod size;
mod step;
mod trace;

use assets::Assets;
use cut::Cut;
pub use maps::Finder;
use maps::{Found, Maps};
use regions::Evaluator;
pub use size::Options;
use size::Working;
use step::{Prepared, prepare};
pub use trace::Stage;

use crate::ops::par;
use crate::{Error, Image, Layer, Lut, Picture, Presets, Recipe};

/// A picture a recipe is rendered on, with the maps of it that the recipe names (`subject`, `depth`, a region): handed in
/// or found by a [`Finder`]. It borrows the picture: rendering never copies or changes it.
///
/// A map is found the first time a render needs it and kept, so rendering several recipes on one photo finds each map
/// once. A photo can be shared between threads.
pub struct Photo<'a> {
    picture: Picture<'a>,
    maps: BTreeMap<String, Arc<Layer>>,
    finders: BTreeMap<String, Found>,
    luts: BTreeMap<String, Arc<Lut>>,
    pictures: BTreeMap<String, Arc<Image>>,
}

impl fmt::Debug for Photo<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Photo")
            .field("height", &self.picture.height())
            .field("width", &self.picture.width())
            .field("maps", &self.maps.keys().collect::<Vec<_>>())
            .field("finders", &self.finders.keys().collect::<Vec<_>>())
            .field("luts", &self.luts.keys().collect::<Vec<_>>())
            .field("pictures", &self.pictures.keys().collect::<Vec<_>>())
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
            luts: BTreeMap::new(),
            pictures: BTreeMap::new(),
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

    /// This photo with `lut` for the name `name`: a step's parameter of the kind `lut` (such as `lut`'s `file`) that holds
    /// `name` uses this table. The table is shared, not copied: render many photos, or many frames, with the same one.
    /// A table of the same name given before is replaced; one no step names is not used. A name that is no given table
    /// is read as the path of a `.cube` file (once per render, however many steps name it), and a path that cannot be
    /// read is a render error naming it, before the first step runs.
    #[must_use]
    pub fn with_lut(mut self, name: impl Into<String>, lut: Arc<Lut>) -> Self {
        self.luts.insert(name.into(), lut);
        self
    }

    /// This photo with `image` for the name `name`: a step's parameter of the kind `picture` (such as `blend`'s `image`)
    /// that holds `name` uses this picture, an RGB or RGBA [`Image`] (its alpha, straight, is how much of it covers the
    /// photo). The picture is shared, not copied. One of the same name given before is replaced; one no step names is not
    /// used. A name that is no given picture is read as the path of an image file (once per render, however many steps
    /// name it, within the core's pixel limit), and a path that cannot be read is a render error naming it, before the
    /// first step runs. (The photo itself is a [`Picture`]; this is a second picture a step lays over it.)
    #[must_use]
    pub fn with_image(mut self, name: impl Into<String>, image: Arc<Image>) -> Self {
        self.pictures.insert(name.into(), image);
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
    /// does not make yet (`shards`, `cuts`, `svg`) is refused before anything runs, and so is one
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
    /// [`Error::Foreign`] if an effect that runs elsewhere fails or returns a picture of another size, and [`Error::Lut`]
    /// for a lookup table a step names that is neither given to the photo nor a readable `.cube` file.
    pub fn render(&self, recipe: &Recipe, presets: &Presets, options: Options) -> Result<Image, Error> {
        par::install(options.threads(), || {
            let mut run = self.start(recipe, presets, options)?;
            match options.band() {
                Some(rows) => run.advance_banded(rows.get())?,
                None => while run.advance()?.is_some() {},
            }
            run.finish()
        })?
    }

    /// `recipe` checked and every step of it prepared, with all the maps they use found: ready to run.
    fn start(&self, recipe: &Recipe, presets: &Presets, options: Options) -> Result<Run<'a>, Error> {
        let reach = recipe.check_rules(presets)?.reach(&recipe.regions);
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
        let mut assets = Assets::new(&self.luts, &self.pictures);
        let steps = recipe
            .steps
            .iter()
            .enumerate()
            .map(|(index, entry)| prepare(presets, ratio, &mut assets, entry, &format!("step {}", index + 1)))
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
    /// Runs the next step on the whole picture, and shows the image it made; none once all have run.
    fn advance(&mut self) -> Result<Option<&Image>, Error> {
        let Some(step) = self.steps.next() else {
            return Ok(None);
        };
        let before = self
            .current
            .as_ref()
            .map_or_else(|| self.working.picture(), Image::as_picture);
        let after = step.apply(&before, self.ratio, &mut self.regions, &self.working.picture(), 0)?;
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
