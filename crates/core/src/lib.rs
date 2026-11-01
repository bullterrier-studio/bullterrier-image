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
mod inputs;
mod ops;
pub mod params;
mod picture;
mod presets;
mod recipe;
mod registry;
mod run;
#[cfg(test)]
mod testing;

pub use effect::{Category, Effect, FieldParam, Input};
pub use error::{Constraint, Error, ErrorKind, MapKind};
pub use inputs::{Inputs, Labels, Layer};
pub use picture::{Image, Picture};
pub use presets::{Preset, Presets};
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

#[cfg(test)]
mod tests {
    use super::{Error, Inputs, Picture, apply, effects::grade};
    use serde_json::json;

    fn grade_params() -> serde_json::Value {
        json!({"temperature": 0.0, "tint": 0.0, "exposure": 0.0, "contrast": 1.0, "pivot": 0.5, "saturation": 1.0,
               "vibrance": 0.0})
    }

    #[test]
    fn an_effect_runs_by_its_name_into_a_new_image_of_the_pictures_size() -> Result<(), Error> {
        // a different value in every channel of 2 x 3 pixels, so a swapped, dropped or misplaced value shows; grade at
        // its defaults leaves a picture as it is
        let values = [
            0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.6, 0.65, 0.7, 0.75, 0.8, 0.85, 0.9,
        ];
        let image = apply(
            "grade",
            &Picture::new(2, 3, &values)?,
            grade_params(),
            &Inputs::default(),
        )?;
        assert_eq!((image.height(), image.width()), (2, 3));
        let out = image.into_values();
        assert_eq!(out.len(), values.len());
        assert!(
            out.iter()
                .zip(values)
                .all(|(after, before)| (after - before).abs() < 1e-6),
            "{out:?}"
        );
        Ok(())
    }

    #[test]
    fn the_declared_defaults_of_grade_leave_a_picture_as_it_is() -> Result<(), Error> {
        // a different value in every channel, so a default that moves any step shows
        let values = [
            0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.6, 0.65, 0.7, 0.75, 0.8, 0.85, 0.9,
        ];
        let out = grade::render(
            &Picture::new(2, 3, &values)?,
            &grade::Params::default(),
            &Inputs::default(),
        )
        .into_values();
        assert!(
            out.iter()
                .zip(values)
                .all(|(after, before)| (after - before).abs() < 1e-6),
            "{out:?}"
        );
        Ok(())
    }

    #[test]
    fn an_unknown_effect_is_refused_by_name() -> Result<(), Error> {
        let values = [0.0_f32; 3];
        let result = apply(
            "no_such_effect",
            &Picture::new(1, 1, &values)?,
            json!({}),
            &Inputs::default(),
        );
        assert!(matches!(result, Err(Error::UnknownEffect(name)) if name == "no_such_effect"));
        Ok(())
    }

    #[test]
    fn parameters_that_drift_from_the_effects_are_refused_naming_the_field() -> Result<(), Error> {
        let values = [0.0_f32; 3];
        let picture = Picture::new(1, 1, &values)?;
        let mut missing = grade_params();
        missing.as_object_mut().map(|fields| fields.remove("tint"));
        let mut extra = grade_params();
        extra
            .as_object_mut()
            .map(|fields| fields.insert("kelvin".to_owned(), json!(5000)));
        for (params, field) in [
            (missing, "tint"),
            (extra, "kelvin"),
            (json!({"temperature": "warm"}), "string"),
        ] {
            let result = apply("grade", &picture, params, &Inputs::default());
            assert!(
                matches!(&result, Err(error @ Error::Params { effect: "grade", .. }) if error.to_string().contains(field)),
                "{field}: {result:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn values_that_do_not_make_the_size_are_refused() {
        let values = [0.0_f32; 7];
        assert!(matches!(
            Picture::new(1, 2, &values),
            Err(Error::Size { values: 7, .. })
        ));
        assert!(matches!(
            Picture::new(2, 2, &values[..6]),
            Err(Error::Size { values: 6, .. })
        ));
        assert!(Picture::new(1, 2, &values[..6]).is_ok());
    }
}
