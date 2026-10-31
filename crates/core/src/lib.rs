//! The numeric core of bullterrier-image: the effects implemented in Rust and the primitives they share.
//!
//! An effect is a pure function from one picture to a new one, known by a name and found in the registry: [`apply`] runs
//! one by name, [`effect`] gives it with what it declares (its [`Category`], default opacity, [`Input`]s and, for the
//! core's own, its parameters described). The core's own effects are files in `src/effects/`, found by the build
//! script; one that lives elsewhere implements [`Effect`] and joins them with [`register`]. A core effect's parameters
//! are declared once, by its `Params` struct (see [`mod@params`]); [`params::Table::check`] checks a user's values
//! against that declaration and the Python package builds the effect's model from it. Running an effect trusts its
//! parameters (and, for now, expects them scaled by the Python package): check them first.

#![forbid(unsafe_code)]

mod effect;
mod effects;
mod error;
mod ops;
pub mod params;
mod picture;
mod registry;

pub use effect::{Category, Effect, Input};
pub use error::{Constraint, Error};
pub use picture::{Image, Picture};
pub use registry::{effect, effects, register, unregister};

/// Run the effect called `effect` on `picture` with `params` (an object with exactly the effect's parameters); a new
/// image of the picture's size.
///
/// # Errors
/// [`Error::UnknownEffect`] if there is no effect of that name, [`Error::Params`] if `params` misses one of a core
/// effect's parameters, has one it does not know, or one of the wrong type, and [`Error::Foreign`] if an effect run
/// elsewhere fails.
pub fn apply(effect: &str, picture: &Picture<'_>, params: serde_json::Value) -> Result<Image, Error> {
    registry::effect(effect)?.apply(picture, params)
}

#[cfg(test)]
mod tests {
    use super::{Error, Picture, apply, effects::grade};
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
        let image = apply("grade", &Picture::new(2, 3, &values)?, grade_params())?;
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
        let out = grade::render(&Picture::new(2, 3, &values)?, &grade::Params::default()).into_values();
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
        let result = apply("no_such_effect", &Picture::new(1, 1, &values)?, json!({}));
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
            let result = apply("grade", &picture, params);
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
