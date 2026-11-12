//! A lookup table reaches an effect through the runner: a step's parameter of the kind `lut` names a table the photo was
//! given, else a `.cube` file, and the effect finds the table in its inputs under the parameter's name. The effect is
//! the test's own, written against the public `Effect` trait; it looks each pixel up in the table it is given (the
//! identity without one), so what a step does is told by the table that reached it.

use std::sync::Arc;

use crate::common::compare::assert_close;
use crate::common::files::Scratch;
use crate::common::pictures::counted;
use crate::common::run::{Registered, unique};
use bullterrier_image::params::{Field, Kind, Scalar, Table};
use bullterrier_image::region::{Grid, Shares};
use bullterrier_image::{
    Category, Each, Effect, Error, ErrorKind, Image, Inputs, Lut, Options, Photo, Picture, Presets, Recipe, Step, apply,
};
use serde_json::{Value, json};

/// An effect with one parameter, `file`, of the kind `lut`.
struct Tablet {
    name: String,
}

impl Tablet {
    fn new() -> Self {
        Self { name: unique("tablet") }
    }
}

impl Effect for Tablet {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Colour
    }

    fn params(&self) -> Table {
        Table {
            doc: "The parameters of a grade.",
            fields: vec![Field {
                name: "file",
                kind: Kind::Lut,
                meaning: "The table.",
                default: Scalar::Null,
                unit: None,
                min: None,
                max: None,
                cyclic: false,
                choices: &[],
                min_length: None,
                max_length: None,
                min_distinct: None,
                at_most: None,
                nullable: false,
                drivable: false,
                advanced: false,
            }],
        }
    }

    fn apply(&self, picture: &Picture<'_>, _params: &Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        let table = inputs.lut("file");
        Ok(picture.map(|pixel| table.map_or(pixel, |table| table.lookup(pixel))))
    }
}

/// The table of two entries per axis whose every entry is `1 - input`: the negative.
fn negative() -> Result<Lut, Error> {
    let entries = (0..8_usize)
        .map(|entry| [entry % 2, entry / 2 % 2, entry / 4].map(|corner| if corner == 1 { 0.0 } else { 1.0 }))
        .collect();
    Lut::new(2, [0.0; 3], [1.0; 3], entries)
}

/// The table of two entries per axis that is `level` everywhere.
fn flat(level: f32) -> Result<Lut, Error> {
    Lut::new(2, [0.0; 3], [1.0; 3], vec![[level; 3]; 8])
}

fn render(photo: &Photo<'_>, recipe: &Recipe) -> Result<Vec<f32>, Error> {
    Ok(photo.render(recipe, &Presets::new(), Options::default())?.into_values())
}

#[test]
fn a_table_given_to_the_photo_reaches_the_step_that_names_it() -> Result<(), Error> {
    let tablet = Registered::new(Tablet::new())?;
    let values = counted();
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_lut("negative", Arc::new(negative()?));
    let made = render(
        &photo,
        &Recipe::new([Step::new(tablet.name()).param("file", "negative")]),
    )?;
    let expected: Vec<f32> = values.iter().map(|value| 1.0 - value).collect();
    assert_close(&made, &expected, 1e-5, "the negative");
    Ok(())
}

#[test]
fn a_step_that_names_no_table_gets_none_and_the_effect_sees_the_identity() -> Result<(), Error> {
    let tablet = Registered::new(Tablet::new())?;
    let values = counted();
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_lut("negative", Arc::new(negative()?));
    for step in [
        Step::new(tablet.name()),
        Step::new(tablet.name()).param("file", Value::Null),
    ] {
        assert_close(&render(&photo, &Recipe::new([step]))?, &values, 1e-5, "no table");
    }
    Ok(())
}

#[test]
fn each_step_gets_the_table_of_its_own_name() -> Result<(), Error> {
    let tablet = Registered::new(Tablet::new())?;
    let values = counted();
    let photo = Photo::new(Picture::new(4, 6, &values)?)
        .with_lut("negative", Arc::new(negative()?))
        .with_lut("quarter", Arc::new(flat(0.25)?));
    let name = tablet.name();
    // the negative, then everything at one quarter, then the negative of that: three quarters
    let recipe = Recipe::new([
        Step::new(name).param("file", "negative"),
        Step::new(name).param("file", "quarter"),
        Step::new(name).param("file", "negative"),
    ]);
    assert_close(
        &render(&photo, &recipe)?,
        &vec![0.75; 72],
        1e-5,
        "three steps, two tables",
    );
    Ok(())
}

#[test]
fn the_looks_of_an_each_step_find_their_tables_too() -> Result<(), Error> {
    let tablet = Registered::new(Tablet::new())?;
    let values = vec![0.5; 4 * 6 * 3];
    let photo = Photo::new(Picture::new(4, 6, &values)?)
        .with_lut("low", Arc::new(flat(0.125)?))
        .with_lut("high", Arc::new(flat(0.875)?));
    let mut each = Each::new(
        "cells",
        [
            Some(Step::new(tablet.name()).param("file", "low").opacity(1.0)),
            Some(Step::new(tablet.name()).param("file", "high").opacity(1.0)),
        ],
    );
    each.feather = 0.0;
    let recipe = Recipe::new([each]).with_region(
        "cells",
        Grid {
            columns: Shares::equal(2),
            ..Grid::default()
        },
    );
    let made = render(&photo, &recipe)?;
    // two columns of three pixels: the first piece gets the first look, the second the second
    let at = |column: usize| made[column * 3];
    assert_close(
        &[at(0), at(2), at(3), at(5)],
        &[0.125, 0.125, 0.875, 0.875],
        1e-5,
        "left and right",
    );
    Ok(())
}

#[test]
fn a_cube_file_named_by_a_step_is_read_and_a_given_table_of_that_name_wins() -> Result<(), Box<dyn std::error::Error>> {
    let tablet = Registered::new(Tablet::new())?;
    let folder = Scratch::new()?;
    // the file holds one half everywhere
    let rows = "0.5 0.5 0.5\n".repeat(8);
    let path = folder.write("half.cube", format!("LUT_3D_SIZE 2\n{rows}"))?;
    let named = path.display().to_string();
    let values = counted();
    let recipe = Recipe::new([Step::new(tablet.name()).param("file", named.as_str())]);
    let photo = Photo::new(Picture::new(4, 6, &values)?);
    assert_close(&render(&photo, &recipe)?, &vec![0.5; 72], 1e-5, "read from the file");
    // a table the photo is given under the path's name is used instead of the file
    let given = photo.with_lut(named, Arc::new(flat(0.25)?));
    assert_close(&render(&given, &recipe)?, &vec![0.25; 72], 1e-5, "the given table wins");
    Ok(())
}

#[test]
fn a_name_that_is_no_table_and_no_file_is_refused_naming_the_step_the_parameter_and_the_name() -> Result<(), Error> {
    let tablet = Registered::new(Tablet::new())?;
    let values = counted();
    let photo = Photo::new(Picture::new(4, 6, &values)?);
    let recipe = Recipe::new([
        Step::new(tablet.name()),
        Step::new(tablet.name()).param("file", "no-such-table.cube"),
    ]);
    let refused = render(&photo, &recipe).err();
    assert!(
        refused.as_ref().is_some_and(|error| {
            let said = error.to_string();
            error.kind() == ErrorKind::Lut
                && said.contains("step 2")
                && said.contains("`file`")
                && said.contains("no-such-table.cube")
        }),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_value_that_is_not_a_name_is_refused_when_the_recipe_is_checked() -> Result<(), Error> {
    let tablet = Registered::new(Tablet::new())?;
    for value in [json!(3), json!(""), json!(["film"])] {
        let recipe = Recipe::new([Step::new(tablet.name()).param("file", value.clone())]);
        let refused = recipe.checked(&Presets::new()).err();
        assert!(
            refused
                .as_ref()
                .is_some_and(|error| error.kind() == ErrorKind::Recipe && error.to_string().contains("`file`")),
            "{value}: {refused:?}"
        );
    }
    Ok(())
}

#[test]
fn a_table_offered_to_apply_reaches_the_effect_under_its_parameter() -> Result<(), Error> {
    let tablet = Registered::new(Tablet::new())?;
    let values = counted();
    let picture = Picture::new(4, 6, &values)?;
    let table = negative()?;
    let inputs = Inputs::default().with_lut("file", &table);
    let made = apply(tablet.name(), &picture, &json!({"file": null}), &inputs)?;
    let expected: Vec<f32> = values.iter().map(|value| 1.0 - value).collect();
    assert_close(&made.into_values(), &expected, 1e-5, "apply with a table");
    Ok(())
}
