//! A picture reaches an effect through the runner: a step's parameter of the kind `picture` names a picture the photo
//! was given, else an image file, and the effect finds the picture in its inputs under the parameter's name. The effect
//! is the test's own, written against the public `Effect` trait; it shows the picture it is given in place of the photo
//! (the photo without one), so what a step does is told by the picture that reached it.

use std::sync::Arc;

use crate::common::compare::assert_close;
use crate::common::files::{Png, Scratch, png_claiming};
use crate::common::pictures::counted;
use crate::common::run::{Registered, unique};
use bullterrier_image::params::{Field, Kind, Scalar, Table};
use bullterrier_image::region::{Grid, Shares};
use bullterrier_image::{
    Category, Each, Effect, Error, ErrorKind, Image, Inputs, Options, Photo, Picture, Presets, Recipe, Step, apply,
};
use serde_json::{Value, json};

/// An effect with one parameter, `image`, of the kind `picture`: it returns that picture's colours (the photo's size).
struct Shower {
    name: String,
}

impl Shower {
    fn new() -> Self {
        Self { name: unique("shower") }
    }
}

impl Effect for Shower {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Compositing
    }

    fn params(&self) -> Table {
        Table {
            doc: "The parameters of a layer.",
            fields: vec![Field {
                name: "image",
                kind: Kind::Picture,
                meaning: "The picture.",
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
                nullable: true,
                drivable: false,
                advanced: false,
            }],
        }
    }

    fn apply(&self, picture: &Picture<'_>, _params: &Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        match inputs.image("image") {
            Some(shown) => Image::new(picture.height(), picture.width(), shown.as_picture().values()),
            None => Image::new(picture.height(), picture.width(), picture.values()),
        }
    }
}

/// A 4 x 6 picture, every value `level`, with an opaque alpha.
fn flat(level: f32) -> Result<Image, Error> {
    Image::rgba(4, 6, &[level, level, level, 1.0].repeat(24))
}

fn render(photo: &Photo<'_>, recipe: &Recipe) -> Result<Vec<f32>, Error> {
    Ok(photo.render(recipe, &Presets::new(), Options::default())?.into_values())
}

#[test]
fn a_picture_given_to_the_photo_reaches_the_step_that_names_it() -> Result<(), Error> {
    let shower = Registered::new(Shower::new())?;
    let values = counted();
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_image("grey", Arc::new(flat(0.25)?));
    let made = render(&photo, &Recipe::new([Step::new(shower.name()).param("image", "grey")]))?;
    assert_close(&made, &vec![0.25; 72], 1e-6, "the picture");
    Ok(())
}

#[test]
fn a_step_that_names_no_picture_gets_none() -> Result<(), Error> {
    let shower = Registered::new(Shower::new())?;
    let values = counted();
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_image("grey", Arc::new(flat(0.25)?));
    for step in [
        Step::new(shower.name()),
        Step::new(shower.name()).param("image", Value::Null),
    ] {
        assert_close(&render(&photo, &Recipe::new([step]))?, &values, 1e-6, "no picture");
    }
    Ok(())
}

#[test]
fn each_step_gets_the_picture_of_its_own_name() -> Result<(), Error> {
    let shower = Registered::new(Shower::new())?;
    let values = counted();
    let photo = Photo::new(Picture::new(4, 6, &values)?)
        .with_image("dark", Arc::new(flat(0.125)?))
        .with_image("light", Arc::new(flat(0.875)?));
    let name = shower.name();
    let recipe = Recipe::new([
        Step::new(name).param("image", "dark"),
        Step::new(name).param("image", "light"),
    ]);
    assert_close(&render(&photo, &recipe)?, &vec![0.875; 72], 1e-6, "the last one shows");
    let recipe = Recipe::new([
        Step::new(name).param("image", "light"),
        Step::new(name).param("image", "dark"),
    ]);
    assert_close(&render(&photo, &recipe)?, &vec![0.125; 72], 1e-6, "in the other order");
    Ok(())
}

#[test]
fn the_looks_of_an_each_step_find_their_pictures_too() -> Result<(), Error> {
    let shower = Registered::new(Shower::new())?;
    let values = vec![0.5; 4 * 6 * 3];
    let photo = Photo::new(Picture::new(4, 6, &values)?)
        .with_image("low", Arc::new(flat(0.125)?))
        .with_image("high", Arc::new(flat(0.875)?));
    let mut each = Each::new(
        "cells",
        [
            Some(Step::new(shower.name()).param("image", "low").opacity(1.0)),
            Some(Step::new(shower.name()).param("image", "high").opacity(1.0)),
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
    let at = |column: usize| made[column * 3];
    assert_close(
        &[at(0), at(2), at(3), at(5)],
        &[0.125, 0.125, 0.875, 0.875],
        1e-6,
        "left and right",
    );
    Ok(())
}

#[test]
fn an_image_file_named_by_a_step_is_read_and_a_given_picture_of_that_name_wins()
-> Result<(), Box<dyn std::error::Error>> {
    let shower = Registered::new(Shower::new())?;
    let folder = Scratch::new()?;
    // a 4 x 6 RGB file, every sample 255: white
    let row = [255_u8; 18];
    let rows = vec![row.as_slice(); 4];
    let path = folder.write("white.png", Png::new(6, 4, 8, 2, &rows).bytes())?;
    let named = path.display().to_string();
    let values = counted();
    let recipe = Recipe::new([Step::new(shower.name()).param("image", named.as_str())]);
    let photo = Photo::new(Picture::new(4, 6, &values)?);
    assert_close(&render(&photo, &recipe)?, &vec![1.0; 72], 1e-6, "read from the file");
    let given = photo.with_image(named, Arc::new(flat(0.25)?));
    assert_close(
        &render(&given, &recipe)?,
        &vec![0.25; 72],
        1e-6,
        "the given picture wins",
    );
    Ok(())
}

#[test]
fn a_name_that_is_no_picture_and_no_file_is_refused_naming_the_step_the_parameter_and_the_name() -> Result<(), Error> {
    let shower = Registered::new(Shower::new())?;
    let values = counted();
    let photo = Photo::new(Picture::new(4, 6, &values)?);
    let recipe = Recipe::new([
        Step::new(shower.name()),
        Step::new(shower.name()).param("image", "no-such-picture.png"),
    ]);
    let refused = render(&photo, &recipe).err();
    assert!(
        refused.as_ref().is_some_and(|error| {
            let said = error.to_string();
            error.kind() == ErrorKind::ImageRead
                && said.contains("step 2")
                && said.contains("`image`")
                && said.contains("no-such-picture.png")
        }),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_file_over_the_pixel_limit_is_refused_before_it_is_decoded() -> Result<(), Box<dyn std::error::Error>> {
    let shower = Registered::new(Shower::new())?;
    let folder = Scratch::new()?;
    let path = folder.write("huge.png", png_claiming(400_000, 400_000))?;
    let values = counted();
    let photo = Photo::new(Picture::new(4, 6, &values)?);
    let named = path.display().to_string();
    let recipe = Recipe::new([Step::new(shower.name()).param("image", named.as_str())]);
    let refused = render(&photo, &recipe).err();
    assert!(
        refused
            .as_ref()
            .is_some_and(|error| error.kind() == ErrorKind::ImageRead && error.to_string().contains("pixels")),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_value_that_is_not_a_name_is_refused_when_the_recipe_is_checked() -> Result<(), Error> {
    let shower = Registered::new(Shower::new())?;
    for value in [json!(3), json!(""), json!(["logo"])] {
        let recipe = Recipe::new([Step::new(shower.name()).param("image", value.clone())]);
        let refused = recipe.checked(&Presets::new()).err();
        assert!(
            refused
                .as_ref()
                .is_some_and(|error| error.kind() == ErrorKind::Recipe && error.to_string().contains("`image`")),
            "{value}: {refused:?}"
        );
    }
    Ok(())
}

#[test]
fn a_picture_offered_to_apply_reaches_the_effect_under_its_parameter() -> Result<(), Error> {
    let shower = Registered::new(Shower::new())?;
    let values = counted();
    let picture = Picture::new(4, 6, &values)?;
    let grey = flat(0.5)?;
    let inputs = Inputs::default().with_image("image", &grey);
    let made = apply(shower.name(), &picture, &json!({"image": null}), &inputs)?;
    assert_close(&made.into_values(), &vec![0.5; 72], 1e-6, "apply with a picture");
    Ok(())
}
