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
#[cfg(feature = "gpu")]
mod gpu;
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
use size::ratio_of;
use size::{Origin, Working};
use step::{Prepared, prepare};
pub use trace::Stage;

use crate::ops::par;
use crate::{Characters, Data, Entry, Error, Frame, Image, Labels, Layer, Lut, Picture, Presets, Recipe, Spread};

impl Recipe {
    /// How far `self` paints beyond the edge of the picture it is rendered on: the room, in output pixels, that a
    /// caller gives a picture so that no halo is cut at its edge (see [`Spread`]). The steps' spreads add, as each step
    /// works on what the one before made; a step shown at opacity 0 adds none, an `each` step the widest of its looks'
    /// and a driven step the widest of its levels'. A mask only hides, so it does not lessen it.
    ///
    /// `longest_side` is the longest side in px of what a render's `px` values follow: the longer side of the canvas
    /// given to [`Options::with_canvas`], else of the picture the render works on (at the size [`Options::with_size`]
    /// or the recipe's `size` ask for, else the picture's own, as the `frame` leaves it). A `px` of the recipe is then
    /// `longest_side / 480` pixels, as in a render. Only the parameters are read: no picture or lookup table is loaded.
    ///
    /// A picture grown by this room is rendered with [`Options::with_room`], so that a step that places itself by the picture
    /// (a vignette's centre, a warp's) takes the original box, not the grown picture, as its own; the `px` then follow the box's
    /// longest side, which is the `longest_side` to give.
    ///
    /// # Errors
    /// Whatever [`Recipe::checked`] refuses the recipe for, and [`Error::Recipe`] for a `longest_side` of 0.
    pub fn spread(&self, presets: &Presets, longest_side: u32) -> Result<Spread, Error> {
        if longest_side == 0 {
            return Err(Error::Recipe("`longest_side` must be >= 1, got 0".to_owned()));
        }
        self.check_rules(presets)?;
        let ratio = ratio_of(f64::from(longest_side));
        let mut total = Spread::NONE;
        for (index, entry) in self.steps.iter().enumerate() {
            total = total.then(step::spread(presets, ratio, entry, &format!("step {}", index + 1))?);
        }
        Ok(total)
    }

    /// The rows of margin a render of a band of this recipe needs on each side, or none if the recipe cannot be rendered
    /// in bands: the answer a caller needs to cut a picture into bands and give each to [`Options::with_window`].
    ///
    /// Hand a render the rows of the picture from the band's first row less this margin to its last row plus it (fewer
    /// where the picture ends), with [`Options::with_window`] saying where they lie on the canvas, and the rows of the
    /// band in what it returns are the rows of the whole picture's render, byte for byte (unlike [`Options::with_band`],
    /// which is within float rounding, 1e-4, where a step blurs by a Fourier transform). The margins of the steps add, as
    /// each reads what the one before made.
    ///
    /// The answer is none for a recipe with a `frame` or a `size` (they change the rows), and with a step that reads the
    /// whole picture (an effect whose [`Footprint`](crate::Footprint) is `Whole`), a driven step, an `each` step, or one
    /// with a `mask`, a `partition` or a field parameter: a map or the pieces of a band are not the canvas's. It is none too
    /// for a step that sums a blur by a Fourier transform ([`Footprint::Fourier`](crate::Footprint::Fourier): a blur
    /// reaching 16 working px or more, or a kernel of more than 128 taps), whose sum depends on the rows it is given (see
    /// [`close_band_margin`](Self::close_band_margin) for a window that need only look the same). It
    /// reads parameters only and loads no picture or table. `longest_side` is as for [`spread`](Self::spread).
    ///
    /// # Errors
    /// Whatever [`Recipe::checked`] refuses the recipe for, and [`Error::Recipe`] for a `longest_side` of 0.
    pub fn band_margin(&self, presets: &Presets, longest_side: u32) -> Result<Option<usize>, Error> {
        self.window_margin(presets, longest_side, false)
    }

    /// [`band_margin`](Self::band_margin) for a window that need only look the same: the rows of the band in what a window
    /// returns are the rows of the whole picture's render to within 32 of 65,535 on a value of 0 to 1 (0.13 of an 8-bit
    /// level), not byte for byte. It also clears a step that sums a blur by a Fourier transform
    /// ([`Footprint::Fourier`](crate::Footprint::Fourier)) or rounds a position by its row
    /// ([`Footprint::Rounded`](crate::Footprint::Rounded)), which have no exact margin; where [`band_margin`](Self::band_margin)
    /// has one, this is the same number and the window is exact. The answer is none for the same recipes otherwise.
    ///
    /// The bound is for each such step: measured at most 14 of 65,535 (`liquid_metal` on a noise picture of 4.5 megapixels),
    /// 1.3 on a photo of 0.5 megapixel, and far less for a blur or a halftone. A step after it that raises contrast
    /// multiplies the difference by its gain.
    ///
    /// # Errors
    /// As [`band_margin`](Self::band_margin).
    pub fn close_band_margin(&self, presets: &Presets, longest_side: u32) -> Result<Option<usize>, Error> {
        self.window_margin(presets, longest_side, true)
    }

    fn window_margin(&self, presets: &Presets, longest_side: u32, close: bool) -> Result<Option<usize>, Error> {
        if longest_side == 0 {
            return Err(Error::Recipe("`longest_side` must be >= 1, got 0".to_owned()));
        }
        self.check_rules(presets)?;
        let ratio = ratio_of(f64::from(longest_side));
        let mut total = 0_usize;
        for (index, entry) in self.steps.iter().enumerate() {
            match step::band_margin(presets, ratio, entry, &format!("step {}", index + 1), close)? {
                Some(margin) => total = total.saturating_add(margin),
                None => return Ok(None),
            }
        }
        Ok((self.frame == Frame::default() && self.size.is_none()).then_some(total))
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
    #[expect(
        clippy::needless_pass_by_value,
        reason = "options are a value built at the call, `Options::default().with_size(..)`; a GPU in them made them no longer `Copy`"
    )]
    pub fn render(&self, recipe: &Recipe, presets: &Presets, options: Options) -> Result<Image, Error> {
        par::install(options.threads(), || {
            #[cfg(feature = "gpu")]
            if let Some(gpu) = options.gpu() {
                return pollster::block_on(gpu::render(gpu, self, recipe, presets, &options));
            }
            let mut run = self.start(recipe, presets, &options)?;
            match options.band() {
                Some(rows) => run.advance_banded(rows.get())?,
                None => while run.advance()?.is_some() {},
            }
            run.finish()
        })?
    }

    /// What the last step of `recipe` makes beside its picture: the steps before it run as for
    /// [`render`](Self::render), then the last is asked for its data on the picture they made. That is the data of the very
    /// picture the recipe makes (but for a `frame.transparent` cut, which only adds alpha), so a plain last step is
    /// required: no `mask`, no opacity below 1, no blend mode, no drive. Rendering the recipe too costs the same run again.
    ///
    /// # Errors
    /// As [`render`](Self::render) for the recipe, the photo and its maps; [`Error::Recipe`] naming the last step's effect
    /// if it makes no data, is not plain, or is an `each` step, or `options` ask for room (`Options::with_room`).
    #[expect(
        clippy::needless_pass_by_value,
        reason = "options are a value built at the call, `Options::default().with_size(..)`; a GPU in them made them no longer `Copy`"
    )]
    pub(crate) fn data(&self, recipe: &Recipe, presets: &Presets, options: Options) -> Result<Data, Error> {
        #[cfg(feature = "gpu")]
        options.cpu_only("ink_planes`, `palette_map` and `characters")?;
        par::install(options.threads(), || self.start(recipe, presets, &options)?.data())?
    }

    /// The ink coverage planes of the last step of `recipe`, one [`Layer`] per ink, 0 to 1: how
    /// much of the ink lies on each pixel. `halftone` gives its inks in order (no `separation`: the one ink; `cmyk`: cyan,
    /// magenta, yellow, black; `rgb`: red, green, blue; `spot`: `inks`), `dither` with `separation` `spot` gives its
    /// `inks`, each 0 or 1. Bare-paper specks are not on a plate, so they are in no plane.
    ///
    /// The steps before the last run as for [`render`](Self::render); the data is of the picture the last step makes, so it
    /// must be plain: no `mask`, no opacity below 1, no blend mode, no drive. Asking for it and for the picture runs the
    /// recipe twice.
    ///
    /// # Errors
    /// As [`render`](Self::render); [`Error::Recipe`] naming the last step's effect if it makes another kind of data (or
    /// none), is not plain, or is an `each` step, or `options` ask for room (`Options::with_room`).
    pub fn ink_planes(&self, recipe: &Recipe, presets: &Presets, options: Options) -> Result<Vec<Layer>, Error> {
        match self.data(recipe, presets, options)? {
            Data::Planes(planes) => Ok(planes),
            other => Err(wrong_kind(recipe, "ink planes", &other)),
        }
    }

    /// The palette-index map of the last step of `recipe` and its palette: a [`Labels`] with the
    /// index of the palette colour each pixel has, and the colours as sRGB `[red, green, blue]` in 0..1. Painting each pixel
    /// with its colour gives the picture exactly. `dither` with a palette gives it (not its `tiles` method, which draws
    /// between two colours).
    ///
    /// # Errors
    /// As [`render`](Self::render); [`Error::Recipe`] naming the last step's effect if it makes another kind of data (or
    /// none), is not plain, or is an `each` step, or `options` ask for room (`Options::with_room`).
    pub fn palette_map(
        &self,
        recipe: &Recipe,
        presets: &Presets,
        options: Options,
    ) -> Result<(Labels, Vec<[f32; 3]>), Error> {
        match self.data(recipe, presets, options)? {
            Data::Palette { map, colours } => Ok((map, colours)),
            other => Err(wrong_kind(recipe, "a palette map", &other)),
        }
    }

    /// The characters of the last step of `recipe`: `ascii`'s grid as text with the colour of each.
    ///
    /// # Errors
    /// As [`render`](Self::render); [`Error::Recipe`] naming the last step's effect if it makes another kind of data (or
    /// none), is not plain, or is an `each` step, or `options` ask for room (`Options::with_room`).
    pub fn characters(&self, recipe: &Recipe, presets: &Presets, options: Options) -> Result<Characters, Error> {
        match self.data(recipe, presets, options)? {
            Data::Text(text) => Ok(text),
            other => Err(wrong_kind(recipe, "characters", &other)),
        }
    }

    /// The map of the region `name` of `recipe`: how much each pixel of the working picture is chosen by it, a number from 0
    /// to 1, as a step that names the region (`mask`, a field) is given it. `name` is `subject`, `background`, `depth`,
    /// `light` or a region the recipe declares that is a selection, whether or not a step uses it.
    ///
    /// The working picture is the one [`render`](Self::render) works on: the photo as the recipe's `frame` frames it, at
    /// the size `options` ask for, else the recipe's `size`, else its own. No step runs and no cut of the finished
    /// picture is made; the recipe is checked as for a render. Only the maps `name` is made from (and the subject, if the
    /// frame crops around it) are needed, see [`Recipe::region_needs`].
    ///
    /// # Errors
    /// Whatever [`Recipe::checked`] refuses the recipe for, [`Error::Recipe`] if `name` is no selection (it is a
    /// partition, or no region), naming the selections there are and the closest, and [`Error::MapNotFound`] for a map
    /// the region is made from that the photo has neither of nor a finder for.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "options are a value built at the call, `Options::default().with_size(..)`; a GPU in them made them no longer `Copy`"
    )]
    pub fn region_map(&self, recipe: &Recipe, presets: &Presets, options: Options, name: &str) -> Result<Layer, Error> {
        #[cfg(feature = "gpu")]
        options.cpu_only("region_map")?;
        par::install(options.threads(), || {
            let mut run = self.begin(recipe, presets, &options, Some(name))?;
            let picture = run.working.picture();
            Ok(Layer::clone(run.regions.selection(name, &picture)?.as_ref()))
        })?
    }

    /// `recipe` checked and every step of it prepared, with all the maps they use found: ready to run.
    fn start(&self, recipe: &Recipe, presets: &Presets, options: &Options) -> Result<Run<'a>, Error> {
        self.begin(recipe, presets, options, None)
    }

    /// [`start`](Self::start), or, for `only` a selection, the run that is only the regions that selection is made from:
    /// no step prepared and no cut planned, and the subject found only if the crop of the frame needs it.
    fn begin(
        &self,
        recipe: &Recipe,
        presets: &Presets,
        options: &Options,
        only: Option<&str>,
    ) -> Result<Run<'a>, Error> {
        let uses = recipe.check_rules(presets)?;
        let reach = match only {
            Some(name) => recipe.reach_of_selection(name)?,
            None => uses.reach(&recipe.regions),
        };
        regions::check_makeable(
            &recipe.regions,
            reach
                .iter()
                .map(|reached| (reached.name.as_str(), reached.place.as_str())),
            self.finders.contains_key("subject"),
        )?;
        // the subject the frame needs (a crop around it, a cut by it), found before anything is made
        let needs_subject = if only.is_some() {
            recipe.frame.crops_by_subject()
        } else {
            recipe.frame.needs_subject()
        };
        let given_subject = needs_subject
            .then(|| maps::stored(self, recipe, "subject", "`frame`"))
            .transpose()?;
        let working = Working::new(
            self.picture,
            &recipe.frame,
            options.size().or(recipe.size),
            given_subject.as_deref(),
        )?;
        let origin = working.origin(options)?;
        let ratio = working.ratio(options)?;
        let maps = Maps::new(self, recipe, &working)?;
        // as the working picture has it, which a trace shows like a map a step uses
        let framed_subject = needs_subject.then(|| maps.get("subject", "`frame`")).transpose()?;
        let mut assets = Assets::new(&self.luts, &self.pictures);
        let (cut, steps) = if only.is_some() {
            (None, Vec::new())
        } else {
            let steps = recipe
                .steps
                .iter()
                .enumerate()
                .map(|(index, entry)| prepare(presets, ratio, &mut assets, entry, &format!("step {}", index + 1)))
                .collect::<Result<Vec<_>, _>>()?;
            (Cut::plan(&recipe.frame, &working, framed_subject.clone())?, steps)
        };
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
            origin,
            ratio,
            steps: steps.into_iter(),
            ran: 0,
            current: None,
            cut,
            regions: Evaluator::new(
                &recipe.regions,
                held,
                last,
                ratio,
                self.finders.get("subject").map(Found::finder),
            ),
        })
    }
}

/// The refusal for asking a recipe's last step for `wanted` when it makes `found`.
fn wrong_kind(recipe: &Recipe, wanted: &str, found: &Data) -> Error {
    let made = match found {
        Data::Planes(_) => "ink planes",
        Data::Palette { .. } => "a palette map",
        Data::Text(_) => "characters",
    };
    let last = recipe.steps.last().map_or_else(
        || "none".to_owned(),
        |entry| match entry {
            Entry::Step(step) => step.effect.clone(),
            Entry::Each(_) => "each".to_owned(),
        },
    );
    Error::Recipe(format!(
        "the last step ('{last}') makes {made} with these parameters, not {wanted}"
    ))
}

/// A recipe being run, the one engine of `render` and `trace`: the steps still to run and the newest image. Each step
/// takes the image before it and drops it once its own is made, so no earlier image is kept; whoever wants them (a
/// trace) copies what [`advance`](Self::advance) shows.
struct Run<'a> {
    /// The picture the steps work on: the photo, or the copy of it at the size asked for.
    working: Working<'a>,
    /// Where the working picture lies on its canvas.
    origin: Origin,
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
        let after = step.apply(
            &before,
            self.ratio,
            &mut self.regions,
            &self.working.picture(),
            (0, self.origin),
        )?;
        self.regions.release(self.ran);
        self.ran += 1;
        self.current = Some(after);
        Ok(self.current.as_ref())
    }

    /// Runs every step but the last, then asks the last what it makes beside its picture (see [`Data`]).
    fn data(&mut self) -> Result<Data, Error> {
        while self.steps.len() > 1 {
            self.advance()?;
        }
        let step = self
            .steps
            .next()
            .ok_or_else(|| Error::Recipe("a recipe needs at least one step".to_owned()))?;
        let before = self
            .current
            .as_ref()
            .map_or_else(|| self.working.picture(), Image::as_picture);
        step.data(
            &before,
            self.ratio,
            &mut self.regions,
            &self.working.picture(),
            self.origin,
        )
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
