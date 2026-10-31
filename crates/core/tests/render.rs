//! A recipe of the core's own effects renders, through the public API alone, to the picture got by running the same
//! effects one after the other with every parameter written out.

use std::error::Error;

use bullterrier_image::{
    BlendMode, Drive, Image, Inputs, Layer, Options, Photo, Picture, Presets, Recipe, Step, apply,
};
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

/// A 20 x 30 picture with a different value in every channel.
fn wide_values() -> Vec<f32> {
    (0_u16..20 * 30 * 3).map(|index| f32::from(index) / 1800.0).collect()
}

/// `recipe` rendered at `ratio` working pixels per design pixel on the 20 x 30 picture.
fn rendered_at(recipe: &Recipe, ratio: f64) -> Result<Vec<f32>, Box<dyn Error>> {
    let values = wide_values();
    let photo = Photo::new(Picture::new(20, 30, &values)?);
    let options = Options::default().with_size_ratio(ratio);
    Ok(photo.render(recipe, &Presets::new(), options)?.into_values())
}

/// The effect `name` run on the same picture with `params` written out, as `rendered_at` does.
fn applied(name: &str, params: serde_json::Value) -> Result<Vec<f32>, Box<dyn Error>> {
    let values = wide_values();
    Ok(apply(name, &Picture::new(20, 30, &values)?, params, &Inputs::default())?.into_values())
}

#[test]
fn a_pattern_at_twice_the_size_is_the_pattern_with_twice_the_pixel_values() -> Outcome {
    let pattern = |cell: f64, feather: f64| {
        json!({"kind": "grid", "cell": cell, "angle": 0.0, "foreground": [1.0, 1.0, 1.0],
               "background": [16.0 / 255.0, 16.0 / 255.0, 24.0 / 255.0], "thickness": 0.2, "radius": 0.3,
               "feather": feather})
    };
    let recipe = Recipe::new([Step::new("pattern")]);
    // `cell` (default 32) and `feather` (default 1) are px; `thickness` and `radius` are fractions of the cell
    assert_eq!(rendered_at(&recipe, 2.0)?, applied("pattern", pattern(64.0, 2.0))?);
    assert_eq!(rendered_at(&recipe, 1.0)?, applied("pattern", pattern(32.0, 1.0))?);
    assert_ne!(rendered_at(&recipe, 2.0)?, rendered_at(&recipe, 1.0)?);
    Ok(())
}

#[test]
fn a_whole_number_of_pixels_scales_rounded_and_never_below_its_minimum() -> Outcome {
    let grid = |cell: i32| json!({"cell": cell, "gap": 0.15, "amount": 0.4, "keep_brightness": false});
    let recipe = Recipe::new([Step::new("pixel_grid")]); // `cell` is 4 px, at least 3
    assert_eq!(rendered_at(&recipe, 2.0)?, applied("pixel_grid", grid(8))?);
    // 4 x 0.5 = 2, below the minimum of 3
    assert_eq!(rendered_at(&recipe, 0.5)?, applied("pixel_grid", grid(3))?);
    // 4 x 1.6 = 6.4 -> 6
    assert_eq!(rendered_at(&recipe, 1.6)?, applied("pixel_grid", grid(6))?);
    Ok(())
}

#[test]
fn a_masked_step_changes_the_picture_only_where_the_mask_is_above_0() -> Outcome {
    let values = wide_values();
    // the left 12 of 30 columns are outside the mask, the rest inside it
    let weights: Vec<f32> = (0..20 * 30)
        .map(|index| f32::from(u8::from(index % 30 >= 12)))
        .collect();
    let photo = Photo::new(Picture::new(20, 30, &values)?).with_map("subject", Layer::new(20, 30, weights)?);
    let recipe = Recipe::new([Step::new("grade")
        .param("exposure", 1.0)
        .mask("subject")
        .blend_mode(BlendMode::Normal)]);
    let rendered = photo
        .render(&recipe, &Presets::new(), Options::default())?
        .into_values();
    let graded = apply(
        "grade",
        &Picture::new(20, 30, &values)?,
        json!({"temperature": 0.0, "tint": 0.0, "exposure": 1.0, "contrast": 1.0, "pivot": 0.5, "saturation": 1.0,
               "vibrance": 0.0}),
        &Inputs::default(),
    )?
    .into_values();
    for (index, pixel) in rendered.chunks(3).enumerate() {
        let expected = if index % 30 >= 12 { &graded } else { &values };
        assert_eq!(pixel, &expected[index * 3..index * 3 + 3], "pixel {index}");
    }
    Ok(())
}

#[test]
fn a_parameter_driven_by_a_map_takes_its_start_where_the_map_is_0_and_its_end_where_it_is_1() -> Outcome {
    let values = wide_values();
    // the left 12 of 30 columns are 0 in the depth map, the rest 1
    let depth: Vec<f32> = (0..20 * 30)
        .map(|index| f32::from(u8::from(index % 30 >= 12)))
        .collect();
    let photo = Photo::new(Picture::new(20, 30, &values)?).with_map("depth", Layer::new(20, 30, depth)?);
    let recipe = Recipe::new([Step::new("grade").param("exposure", Drive::new("depth", -1.0, 1.0).levels(2))]);
    let rendered = photo
        .render(&recipe, &Presets::new(), Options::default())?
        .into_values();
    let graded = |exposure: f64| {
        let params = json!({"temperature": 0.0, "tint": 0.0, "exposure": exposure, "contrast": 1.0, "pivot": 0.5,
                            "saturation": 1.0, "vibrance": 0.0});
        apply("grade", &Picture::new(20, 30, &values)?, params, &Inputs::default()).map(Image::into_values)
    };
    let (darker, brighter) = (graded(-1.0)?, graded(1.0)?);
    for (index, pixel) in rendered.chunks(3).enumerate() {
        let expected = if index % 30 >= 12 { &brighter } else { &darker };
        assert_eq!(pixel, &expected[index * 3..index * 3 + 3], "pixel {index}");
    }
    Ok(())
}
