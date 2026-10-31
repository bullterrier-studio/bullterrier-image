//! A step built from an effect's typed `Params` is the step a person would write by hand, passes the recipe's checks
//! and runs to the same image as the same values given as JSON.

use std::error::Error;

use bullterrier_image::effects::grade;
use bullterrier_image::effects::pattern::{self, Kind};
use bullterrier_image::params::{Color, Declared};
use bullterrier_image::{Picture, Presets, Recipe, Step, apply};
use serde_json::{Value, json};

type Outcome = Result<(), Box<dyn Error>>;

/// Parameters that differ from the defaults in every field, so a dropped or swapped one shows.
fn dots() -> pattern::Params {
    pattern::Params {
        kind: Kind::Dots,
        cell: 20.0,
        angle: 30.0,
        foreground: Color([0.25, 0.5, 1.0]),
        background: Color([0.1, 0.0, 0.3]),
        thickness: 0.2,
        radius: 0.4,
        feather: 2.5,
    }
}

#[test]
fn a_step_of_typed_params_is_the_step_written_by_hand() {
    // 0.2 is the number a person writes, not the f32 widened to 0.20000000298; 20.0 stays a decimal, not 20
    let by_hand = Step::new("pattern")
        .param("kind", "dots")
        .param("cell", 20.0)
        .param("angle", 30.0)
        .param("foreground", json!([0.25, 0.5, 1.0]))
        .param("background", json!([0.1, 0.0, 0.3]))
        .param("thickness", 0.2)
        .param("radius", 0.4)
        .param("feather", 2.5);
    assert_eq!(Step::of(dots()), by_hand);
}

#[test]
fn a_step_of_typed_params_has_every_field_and_no_preset() {
    let step = Step::of(grade::Params::default());
    assert_eq!(step.effect, "grade");
    assert_eq!(step.params.len(), 7);
    assert_eq!(step.preset, None);
}

#[test]
fn a_colour_is_the_three_numbers_the_recipe_checks_make_of_its_hex() -> Outcome {
    // the default background is '#101018'
    let typed = Step::of(pattern::Params::default());
    let checked = pattern::Params::table().check("pattern", &serde_json::Map::new())?;
    let channels = |value: &Value| -> Vec<f64> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .collect()
    };
    for field in ["foreground", "background"] {
        let (typed, checked) = (channels(&typed.params[field]), channels(&checked[field]));
        assert_eq!(typed.len(), 3, "{field}: {typed:?}");
        // the typed colour holds f32 channels, so it is the checked one to f32 precision
        assert!(
            typed.iter().zip(&checked).all(|(a, b)| (a - b).abs() < 1e-7),
            "{field}: {typed:?} vs {checked:?}"
        );
    }
    Ok(())
}

#[test]
fn a_step_of_typed_params_passes_the_recipe_checks() -> Outcome {
    let recipe = Recipe::new([Step::of(dots()), Step::of(grade::Params::default())]);
    recipe.checked(&Presets::new())?;
    Ok(())
}

#[test]
fn typed_params_run_to_the_same_image_as_the_same_values_given_as_json() -> Outcome {
    let values: Vec<f32> = (0..12 * 12 * 3_u32)
        .map(|index| f32::from(u8::try_from(index % 17).unwrap_or(0)) / 16.0)
        .collect();
    let picture = Picture::new(12, 12, &values)?;
    let typed = apply("pattern", &picture, Value::Object(Step::of(dots()).params))?;
    let as_json = json!({
        "kind": "dots", "cell": 20, "angle": 30, "foreground": [0.25, 0.5, 1.0], "background": [0.1, 0.0, 0.3],
        "thickness": 0.2, "radius": 0.4, "feather": 2.5,
    });
    let given = as_json.as_object().cloned().unwrap_or_default();
    let checked = pattern::Params::table().check("pattern", &given)?;
    let by_json = apply("pattern", &picture, Value::Object(checked))?;
    assert_eq!(typed, by_json);
    Ok(())
}
