//! The numeric core of bullterrier-image: the effects implemented in Rust and the primitives they share.
//!
//! An effect is a pure function from one picture to a new one, known by a name and found in the registry: [`apply`] runs
//! one by name, [`effect`] gives it with what it declares (its [`Category`], default opacity, [`Input`]s and, for the
//! core's own, its parameters described). The core's own effects are files in `src/effects/`, found by the build
//! script; one that lives elsewhere implements [`Effect`] and joins them with [`register`]. A core effect's parameters
//! are declared once, by its `Params` struct (see [`mod@params`]); [`params::Table::check`] checks a user's values
//! against that declaration and the Python package builds the effect's model from it; [`Step::of`] builds a recipe step
//! from an effect's typed `Params` (`effects::grade::Params`). There is no typed apply yet: [`apply`] takes JSON. Running an effect trusts its
//! parameters (and, for now, expects them scaled by the Python package): check them first.
//!
//! A look runs on a picture with [`Photo::render`]: it checks the recipe, completes and scales each step's parameters
//! (preset, defaults, design pixels) and runs the steps in order, laying each result over its input by the step's blend
//! mode, opacity and mask. A mask is a map of the photo, by name: `subject` and `depth` (handed to the photo with
//! [`Photo::with_map`], or found by a [`Finder`] the photo is given with [`Photo::with_finder`], once however many
//! recipes are rendered), `background` (1 - the subject), `light` (the picture's brightness) or a region the recipe
//! declares as given. The steps work on the photo as the recipe's [`Frame`] frames it (flipped, cropped to an aspect or
//! around the subject, given borders: the photo and its maps together), at the size [`Options::with_size`] or the
//! recipe's `size` asks for (the photo and its maps resampled, the aspect kept), and a recipe's pixel values are scaled
//! by that picture's longest side over 480. A parameter an effect declares drivable may be a [`Drive`], which varies it across the picture by
//! such a map. [`Photo::trace`] runs the same code and returns every [`Stage`]: the photo, the maps used, and the picture
//! after each step. [`apply`] runs one effect on parameters that are already complete.
//!
//! A render runs on the calling thread unless it is given more: [`Options::with_threads`] shares the rows of the work that
//! is the same for every row (laying a step over its input, framing and resizing the photo, an effect that works pixel by
//! pixel) between that many threads, and the picture is the same bytes at any number. To render many pictures, share the
//! [`Photo`]s, [`Recipe`] and [`Presets`] between your own threads, one render each (they are `Send` and `Sync`), and
//! leave the default of one thread for each render; for one picture, ask for every core:
//!
//! ```text
//! let image = photo.render(&look, &presets, Options::default().with_threads(0))?;
//! // many pictures: one render per thread of yours, sharing `look` and `presets`
//! std::thread::scope(|scope| {
//!     for photo in &photos {
//!         scope.spawn(|| photo.render(&look, &presets, Options::default()));
//!     }
//! });
//! ```

#![forbid(unsafe_code)]

mod effect;
pub mod effects;
mod error;
mod files;
mod footprint;
mod inputs;
mod lut;
mod ops;
pub mod params;
mod picture;
mod presets;
mod recipe;
mod registry;
mod run;

pub use effect::{Category, Effect, FieldParam, Input};
pub use error::{Constraint, Error, ErrorKind, MapKind};
pub use files::{read, read_map, write, write_with_quality};
pub use footprint::{Canvas, Footprint};
pub use inputs::{Inputs, Labels, Layer, Place};
pub use lut::Lut;
pub use ops::edge::{Edge, EdgeOrConstant};
pub use ops::place::{Anchor, Fit, Resample};
pub use ops::radial::Motion;
pub use ops::sample::Interpolation;
pub use picture::{Image, Picture};
pub use presets::{Preset, Presets};
pub use recipe::region::presets::region_presets;
pub use recipe::{
    Assign, BlendMode, Drive, Each, Entry, Frame, Needs, Recipe, Region, Regions, STEP_KEYS, Step, frame, region,
};
pub use registry::{effect, effects, register, unregister};
pub use run::{Finder, Options, Photo, Stage};

/// Run the effect called `effect` on `picture` with `params` (an object with exactly the effect's parameters); a new
/// image of the picture's size. `inputs` is what is on offer: the effect receives the part it declares.
///
/// # Errors
/// [`Error::UnknownEffect`] if there is no effect of that name, [`Error::Params`] if `params` misses one of a core
/// effect's parameters, has one it does not know, or one of the wrong type, [`Error::Inputs`] if the effect declares
/// an input that `inputs` lacks or that is not of the picture's size, and [`Error::Foreign`] if an effect run
/// elsewhere fails.
pub fn apply(
    effect: &str,
    picture: &Picture<'_>,
    params: serde_json::Value,
    inputs: &Inputs<'_>,
) -> Result<Image, Error> {
    effect::run(&*registry::effect(effect)?, picture, params, inputs)
}
