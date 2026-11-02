//! A parameter driven by a map, through the public API alone: the values a drive takes along its levels, the object
//! it is written as, and the levels a recipe's drive is run at when it writes none or some.

use crate::common::Outcome;
use crate::common::recipe::{Double, probe_table};
use bullterrier_image::{Drive, Effect, Layer, Options, Photo, Picture, Presets, Recipe};
use serde_json::{Value, json};

#[test]
fn a_drive_is_start_at_level_0_end_exactly_at_the_last_and_even_steps_between() {
    // from 10 to 30 in 5 levels: 10, 15, 20, 25, 30
    let drive = Drive::new("depth", 10.0, 30.0);
    let values: Vec<f64> = (0..5).map(|level| drive.value(level)).collect();
    assert_eq!(values, [10.0, 15.0, 20.0, 25.0, 30.0]);
    // downwards, in 2 levels: the two ends
    let falling = Drive::new("depth", 1.0, -1.0).levels(2);
    assert_eq!((falling.value(0), falling.value(1)), (1.0, -1.0));
    // 0.3 + (0.9 - 0.3) x 3 / 3 is 0.9000000000000001 in floating point: the last level is `end` itself
    let last = Drive::new("depth", 0.3, 0.9).levels(4);
    assert_eq!(last.value(3).to_bits(), 0.9_f64.to_bits());
}

#[test]
fn a_drive_is_written_as_the_object_a_recipe_holds() {
    let written = Value::from(Drive::new("light", 0.25, 4.0).levels(8));
    assert_eq!(written, json!({"by": "light", "start": 0.25, "end": 4.0, "levels": 8}));
}

/// The values the parameter `cell` takes in the runs of a step that drives it by `drive` (a recipe's JSON) over a
/// depth map that rises from 0 to 1 across the picture, so that every level is needed: sorted, each once.
fn cells_run_at(drive: &Value) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
    let double = Double::unique().with_table(probe_table()).register()?;
    let recipe =
        Recipe::from_json(&json!({"steps": [{"effect": double.name(), "params": {"cell": drive}}]}).to_string())?;
    // 480 px wide, so that a design pixel is a working pixel and `cell` reaches the effect as written
    let values = vec![0.5_f32; 480 * 3];
    let ramp: Vec<f32> = (0..480_u16).map(|column| f32::from(column) / 479.0).collect();
    let photo = Photo::new(Picture::new(1, 480, &values)?).with_map("depth", Layer::new(1, 480, ramp)?)?;
    photo.render(&recipe, &Presets::new(), Options::default())?;
    let mut cells: Vec<f64> = double
        .received()
        .iter()
        .filter_map(|received| received.params["cell"].as_f64())
        .collect();
    cells.sort_by(f64::total_cmp);
    cells.dedup();
    Ok(cells)
}

#[test]
fn a_drive_that_writes_no_levels_runs_at_five_and_one_that_writes_eight_runs_at_eight() -> Outcome {
    assert_eq!(
        cells_run_at(&json!({"by": "depth", "start": 10, "end": 30}))?,
        [10.0, 15.0, 20.0, 25.0, 30.0]
    );
    // 10 to 38 in 8 levels: steps of 4
    assert_eq!(
        cells_run_at(&json!({"by": "depth", "start": 10, "end": 38, "levels": 8}))?,
        [10.0, 14.0, 18.0, 22.0, 26.0, 30.0, 34.0, 38.0]
    );
    // the drive built in code is the same drive
    assert_eq!(
        cells_run_at(&Value::from(Drive::new("depth", 10.0, 38.0).levels(8)))?,
        [10.0, 14.0, 18.0, 22.0, 26.0, 30.0, 34.0, 38.0]
    );
    Ok(())
}
