//! What a step does on its way to its effect, through the public API alone: a preset under the step's values and the
//! defaults for the rest, the effect's own opacity, and a parameter driven by a map mixing the runs at its levels.

use crate::common::Outcome;
use crate::common::compare::assert_close;
use crate::common::pictures::counted;
use crate::common::run::{Paint, Recorder, Registered, length_table, probe_table};
use bullterrier_image::{Drive, Error, Image, Layer, Options, Photo, Picture, Presets, Recipe, Step};
use serde_json::{Value, json};

/// `recipe` rendered on the 4 x 6 picture.
fn rendered(recipe: &Recipe, presets: &Presets) -> Result<Image, Error> {
    let values = counted();
    Photo::new(Picture::new(4, 6, &values)?).render(recipe, presets, Options::default())
}

#[test]
fn a_step_reaches_its_effect_with_the_preset_under_its_values_and_the_defaults_for_the_rest() -> Outcome {
    let recorder = Registered::new(Recorder::new(&[], &[]).with_table(probe_table()))?;
    let mut presets = Presets::new();
    let table = json!({"wide": {"description": "Wide cells.", "cell": 8, "cells": 7}});
    presets.add(recorder.name(), &table)?;
    let mut step = Step::new(recorder.name()).param("cells", 9);
    step.preset = Some("wide".to_owned());
    // a picture of the design size, so that px values are not scaled
    let values = vec![0.5_f32; 480 * 3];
    Photo::new(Picture::new(1, 480, &values)?).render(&Recipe::new([step]), &presets, Options::default())?;
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
fn an_effect_that_describes_no_parameters_gets_them_as_the_step_writes_them() -> Outcome {
    let recorder = Registered::new(Recorder::new(&[], &[]))?;
    let step = Step::new(recorder.name()).param("anything", json!([1, "two"]));
    rendered(&Recipe::new([step]), &Presets::new())?;
    assert_eq!(recorder.received()[0].params, json!({"anything": [1, "two"]}));
    Ok(())
}

#[test]
fn a_step_that_sets_no_opacity_has_the_opacity_its_effect_declares() -> Outcome {
    let painter = Registered::new(Paint::new())?;
    let paint = || Step::new(painter.name()).param("level", 0.5);
    // `paint` declares an opacity of 0.25: a quarter of the way from the photo (value v) to 0.5 is v + 0.25 (0.5 - v)
    let default = rendered(&Recipe::new([paint()]), &Presets::new())?;
    let expected: Vec<f32> = counted().iter().map(|v| v + 0.25 * (0.5 - v)).collect();
    assert_close(&default.into_values(), &expected, 1e-6, "the step");
    let own = rendered(&Recipe::new([paint().opacity(0.5)]), &Presets::new())?;
    let expected: Vec<f32> = counted().iter().map(|v| v + 0.5 * (0.5 - v)).collect();
    assert_close(&own.into_values(), &expected, 1e-6, "the step");
    let full = rendered(&Recipe::new([paint().opacity(1.0)]), &Presets::new())?;
    assert_eq!(full.into_values(), vec![0.5; 72]);
    Ok(())
}

/// The 1 x `width` photo of `width` grey pixels, with `field` as its map `subject` (one value per pixel).
fn banded(field: &[f32]) -> Result<(Vec<f32>, Layer), Error> {
    Ok((vec![0.5; field.len() * 3], Layer::new(1, field.len(), field.to_vec())?))
}

/// `step` rendered on a grey photo as wide as `field` that has `field` as its map `subject`.
fn driven_by(step: Step, field: &[f32]) -> Result<Image, Error> {
    let (values, subject) = banded(field)?;
    Photo::new(Picture::new(1, field.len(), &values)?)
        .with_map("subject", subject)?
        .render(&Recipe::new([step]), &Presets::new(), Options::default())
}

#[test]
fn a_driven_parameter_mixes_the_runs_either_side_of_the_maps_value() -> Outcome {
    let painter = Registered::new(Paint::new())?;
    // paints the grey `level`, here 0.2 where the map is 0 and 0.8 where it is 1: a pixel at v mixes (1 - v) of the
    // first run with v of the second
    let step = Step::new(painter.name())
        .param("level", Drive::new("subject", 0.2, 0.8).levels(2))
        .opacity(1.0);
    let field = [0.0, 1.0, 0.25, 0.5, 0.75, 0.1];
    let drawn = driven_by(step, &field)?.into_values();
    let expected: Vec<f32> = [0.2, 0.8, 0.35, 0.5, 0.65, 0.26]
        .iter()
        .flat_map(|level| [*level; 3])
        .collect();
    assert_close(&drawn, &expected, 1e-6, "the step");
    Ok(())
}

#[test]
fn a_driven_effect_with_the_map_on_a_level_is_the_plain_effect_at_that_levels_value() -> Outcome {
    // grade's exposure from 0 to 2 in 5 levels (0, 0.5, 1, 1.5, 2); a map of 0.5 is on level 2 (exposure 1), 0.25 on
    // level 1 (0.5), 0.375 halfway between levels 1 and 2
    let grade = |exposure: f64| Step::new("grade").param("exposure", exposure);
    let driven = Step::new("grade").param("exposure", Drive::new("subject", 0.0, 2.0));
    let rendered_with =
        |step: Step, field: f32| -> Result<Vec<f32>, Error> { Ok(driven_by(step, &[field; 6])?.into_values()) };
    let plain = |exposure: f64| rendered_with(grade(exposure), 0.0);
    assert_eq!(rendered_with(driven.clone(), 0.5)?, plain(1.0)?);
    assert_eq!(rendered_with(driven.clone(), 0.25)?, plain(0.5)?);
    let (below, above) = (plain(0.5)?, plain(1.0)?);
    let halfway: Vec<f32> = below
        .iter()
        .zip(&above)
        .map(|(low, high)| 0.5 * low + 0.5 * high)
        .collect();
    assert_close(&rendered_with(driven, 0.375)?, &halfway, 1e-6, "the step");
    Ok(())
}

#[test]
fn only_the_levels_a_pixel_is_on_or_next_to_are_run_and_the_last_is_exactly_the_end() -> Outcome {
    let recorder = Registered::new(Recorder::new(&[], &[]))?;
    // x from 10 to 50 in 5 levels: 10, 20, 30, 40, 50
    let step = Step::new(recorder.name()).param("x", Drive::new("subject", 10.0, 50.0));
    // (the map's values, the x of the runs in order)
    let rows: [(&[f32], &[f64]); 5] = [
        (&[0.0, 0.0], &[10.0]),
        (&[0.5, 0.5], &[30.0]),
        (&[0.3, 0.3], &[20.0, 30.0]),
        (&[0.0, 1.0], &[10.0, 50.0]),
        (&[0.9], &[40.0, 50.0]),
    ];
    for (field, expected) in rows {
        driven_by(step.clone(), field)?;
        let runs: Vec<f64> = recorder
            .received()
            .iter()
            .filter_map(|received| received.params["x"].as_f64())
            .collect();
        assert_eq!(runs, expected, "{field:?}");
        recorder.clear();
    }
    // 0.3 + 0.6 x 3 / 3 is 0.9000000000000001: the run of the last level gets `end` itself
    let end = Step::new(recorder.name()).param("x", Drive::new("subject", 0.3, 0.9).levels(4));
    driven_by(end, &[1.0])?;
    let received = recorder.received();
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].params["x"].as_f64().map(f64::to_bits),
        Some(0.9_f64.to_bits())
    );
    Ok(())
}

#[test]
fn the_weights_of_a_pixel_add_up_to_one_so_a_constant_stays_that_constant() -> Outcome {
    let painter = Registered::new(Paint::new())?;
    // every level paints 1: the result is the sum of the weights, for every map value from 0 to 1
    let field: Vec<f32> = (0..1001_u16).map(|index| f32::from(index) / 1000.0).collect();
    for levels in [2, 3, 5, 16] {
        let step = Step::new(painter.name())
            .param("level", Drive::new("subject", 1.0, 1.0).levels(levels))
            .opacity(1.0);
        let drawn = driven_by(step, &field)?.into_values();
        assert!(
            drawn.iter().all(|value| (1.0 - 1e-6..=1.0).contains(value)),
            "{levels} levels"
        );
    }
    Ok(())
}

#[test]
fn a_linear_effect_driven_by_a_map_is_its_parameter_at_the_maps_value_whatever_the_levels() -> Outcome {
    let painter = Registered::new(Paint::new())?;
    // paint's grey is its `level`, so a driven pixel is 0.2 + 0.6 v at map value v, however many levels mix it
    let field: Vec<f32> = (0..=100_u8).map(|index| f32::from(index) / 100.0).collect();
    let expected: Vec<f32> = field.iter().flat_map(|v| [0.2 + 0.6 * v; 3]).collect();
    for levels in [2, 3, 5, 16] {
        let step = Step::new(painter.name())
            .param("level", Drive::new("subject", 0.2, 0.8).levels(levels))
            .opacity(1.0);
        let drawn = driven_by(step, &field)?.into_values();
        assert_close(&drawn, &expected, 1e-6, "the step");
    }
    Ok(())
}

#[test]
fn every_driven_parameter_of_a_step_moves_with_its_level_and_the_others_stay_as_they_are() -> Outcome {
    let recorder = Registered::new(Recorder::new(&[], &[]))?;
    let step = Step::new(recorder.name())
        .param("x", Drive::new("subject", 10.0, 50.0).levels(2))
        .param("y", Drive::new("subject", 1.0, 5.0).levels(2))
        .param("z", 7);
    driven_by(step, &[0.0, 1.0])?;
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
fn each_level_is_scaled_to_the_picture_on_its_own() -> Outcome {
    let recorder = Registered::new(Recorder::new(&[], &[]).with_table(length_table()))?;
    // `length` is px with a minimum of 2: from 3 (map 0) to 8 (map 1) at half size, a picture of 240 px: 1.5 is held
    // at 2, 4 stays 4
    let step = Step::new(recorder.name()).param("length", Drive::new("subject", 3.0, 8.0).levels(2));
    let field: Vec<f32> = (0..240).map(|index| f32::from(u8::from(index > 0))).collect();
    driven_by(step, &field)?;
    let lengths: Vec<Value> = recorder
        .received()
        .iter()
        .map(|received| received.params["length"].clone())
        .collect();
    assert_eq!(lengths, [json!(2.0), json!(4.0)]);
    Ok(())
}
