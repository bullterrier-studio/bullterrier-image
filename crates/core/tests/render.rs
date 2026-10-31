//! A recipe of the core's own effects renders, through the public API alone, to the picture got by running the same
//! effects one after the other with every parameter written out.

use std::error::Error;

use bullterrier_image::{Inputs, Options, Photo, Picture, Presets, Recipe, Step, apply};
use serde_json::json;

type Outcome = Result<(), Box<dyn Error>>;

/// An 8 x 12 picture with a different value in every channel.
fn picture_values() -> Vec<f32> {
    (0_u16..8 * 12 * 3).map(|index| f32::from(index) / 300.0).collect()
}

#[test]
fn a_recipe_renders_as_its_effects_run_in_order_with_their_defaults_filled_in() -> Outcome {
    let values = picture_values();
    let picture = Picture::new(8, 12, &values)?;
    let recipe = Recipe::new([
        Step::new("grade").param("exposure", 0.5),
        Step::new("pixel_grid").param("cell", 4),
        Step::new("grade").param("saturation", 1.5),
    ]);
    let rendered = Photo::new(picture).render(&recipe, &Presets::new(), Options::default())?;

    // by hand: every parameter written, the defaults from the effects' documentation
    let grade = |exposure: f64, saturation: f64| {
        json!({"temperature": 0.0, "tint": 0.0, "exposure": exposure, "contrast": 1.0, "pivot": 0.5,
               "saturation": saturation, "vibrance": 0.0})
    };
    let grid = json!({"cell": 4, "gap": 0.15, "amount": 0.4, "keep_brightness": false});
    let offered = Inputs::default();
    let first = apply("grade", &picture, grade(0.5, 1.0), &offered)?.into_values();
    let second = apply("pixel_grid", &Picture::new(8, 12, &first)?, grid, &offered)?.into_values();
    let third = apply("grade", &Picture::new(8, 12, &second)?, grade(0.0, 1.5), &offered)?;

    assert_eq!((rendered.height(), rendered.width()), (8, 12));
    assert_eq!(rendered.into_values(), third.into_values());
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_picture_and_leaves_the_photo_as_it_was() -> Outcome {
    let values = picture_values();
    let photo = Photo::new(Picture::new(8, 12, &values)?);
    let recipe = Recipe::new([Step::new("pixel_grid"), Step::new("grade").param("contrast", 1.4)]);
    let once = photo.render(&recipe, &Presets::new(), Options::default())?;
    let twice = photo.render(&recipe, &Presets::new(), Options::default())?;
    assert_eq!(once, twice);
    assert_ne!(once.into_values(), values);
    assert_eq!(values, picture_values());
    Ok(())
}
