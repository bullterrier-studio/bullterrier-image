//! The numeric core of bullterrier-image: the effects implemented in Rust and the primitives they share.
//!
//! An effect is a pure function from one picture to a new one: [`apply`] runs one by name. Its parameters arrive
//! already validated and scaled by the Python package, which declares them (one Pydantic model per effect); the core
//! trusts them and does not check their ranges again. Each effect is one file in `src/effects/`, found by the build
//! script, with a `Params` struct (the model's fields, every one required) and a `render` function.

#![forbid(unsafe_code)]

mod effects;
mod error;
mod picture;

pub use error::Error;
pub use picture::{Image, Picture};

/// Run the effect called `effect` on `picture` with `params` (an object with exactly the effect's parameters); a new
/// image of the picture's size.
///
/// # Errors
/// [`Error::UnknownEffect`] if the core implements no effect of that name, and [`Error::Params`] if `params` misses
/// one of its parameters, has one it does not know, or one of the wrong type.
pub fn apply(effect: &str, picture: &Picture<'_>, params: serde_json::Value) -> Result<Image, Error> {
    let (_, run) = effects::EFFECTS
        .iter()
        .find(|(name, _)| *name == effect)
        .ok_or_else(|| Error::UnknownEffect(effect.to_owned()))?;
    run(picture, params)
}

/// The names of the effects the core implements, sorted.
#[must_use]
pub fn effects() -> Vec<&'static str> {
    effects::EFFECTS.iter().map(|(name, _)| *name).collect()
}

#[cfg(test)]
mod tests {
    use super::{Error, Picture, apply};
    use serde_json::json;

    #[test]
    fn an_unknown_effect_is_refused_by_name() -> Result<(), Error> {
        let values = [0.0_f32; 3];
        let result = apply("no_such_effect", &Picture::new(1, 1, &values)?, json!({}));
        assert!(matches!(result, Err(Error::UnknownEffect(name)) if name == "no_such_effect"));
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
