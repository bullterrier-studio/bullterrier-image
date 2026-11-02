//! `apply`, the one way to run an effect by name on a picture with complete parameters, through the public API alone.

use bullterrier_image::effects::grade;
use bullterrier_image::params::Declared;
use bullterrier_image::{Error, Inputs, Picture, apply};
use serde_json::{Value, json};

fn grade_params() -> Value {
    json!({"temperature": 0.0, "tint": 0.0, "exposure": 0.0, "contrast": 1.0, "pivot": 0.5, "saturation": 1.0,
           "vibrance": 0.0})
}

/// A different value in every channel of 2 x 3 pixels, so a swapped, dropped or misplaced value shows; grade at its
/// defaults leaves such a picture as it is.
const VALUES: [f32; 18] = [
    0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.6, 0.65, 0.7, 0.75, 0.8, 0.85, 0.9,
];

#[test]
fn an_effect_runs_by_its_name_into_a_new_image_of_the_pictures_size() -> Result<(), Error> {
    let image = apply(
        "grade",
        &Picture::new(2, 3, &VALUES)?,
        grade_params(),
        &Inputs::default(),
    )?;
    assert_eq!((image.height(), image.width()), (2, 3));
    let out = image.into_values();
    assert_eq!(out.len(), VALUES.len());
    assert!(
        out.iter()
            .zip(VALUES)
            .all(|(after, before)| (after - before).abs() < 1e-6),
        "{out:?}"
    );
    Ok(())
}

#[test]
fn the_declared_defaults_of_grade_leave_a_picture_as_it_is() -> Result<(), Error> {
    let defaults = Value::Object(grade::Params::default().values());
    let out = apply("grade", &Picture::new(2, 3, &VALUES)?, defaults, &Inputs::default())?.into_values();
    assert!(
        out.iter()
            .zip(VALUES)
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
