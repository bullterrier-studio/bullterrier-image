//! A recipe of the core's own effects renders, through the public API alone, to the picture got by running the same
//! effects one after the other with every parameter written out.

use std::error::Error;

use std::sync::Arc;

use bullterrier_image::frame::Anchor;
use bullterrier_image::{
    BlendMode, Drive, Finder, Frame, Image, Inputs, Layer, Options, Photo, Picture, Presets, Recipe, Stage, Step, apply,
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
    // the picture is 12 / 480 of the design size: the 4 px cell of the step shrinks to 0.1 px and is held at its minimum, 3
    let grid = json!({"cell": 3, "gap": 0.15, "amount": 0.4, "keep_brightness": false});
    let offered = Inputs::default();
    let first = apply("grade", &picture, &grade(0.5, 1.0), &offered)?.into_values();
    let second = apply("pixel_grid", &Picture::new(8, 12, &first)?, &grid, &offered)?.into_values();
    let third = apply("grade", &Picture::new(8, 12, &second)?, &grade(0.0, 1.5), &offered)?;

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

/// A 20 px high picture of the width that makes `ratio` working pixels per design pixel (the longest side over 480).
fn scaled_values(ratio: f64) -> (usize, Vec<f32>) {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the ratios of the tests are small"
    )]
    let width = (480.0 * ratio) as usize;
    (
        width,
        (0..20 * width * 3)
            .map(|index| f32::from(u8::try_from(index % 251).unwrap_or(0)) / 251.0)
            .collect(),
    )
}

/// `recipe` rendered at `ratio` working pixels per design pixel on a picture whose longest side makes it.
fn rendered_at(recipe: &Recipe, ratio: f64) -> Result<Vec<f32>, Box<dyn Error>> {
    let (width, values) = scaled_values(ratio);
    let photo = Photo::new(Picture::new(20, width, &values)?);
    Ok(photo.render(recipe, &Presets::new(), Options::default())?.into_values())
}

/// The effect `name` run with `params` written out on the picture `rendered_at` makes for `ratio`.
fn applied(name: &str, params: &serde_json::Value, ratio: f64) -> Result<Vec<f32>, Box<dyn Error>> {
    let (width, values) = scaled_values(ratio);
    Ok(apply(name, &Picture::new(20, width, &values)?, params, &Inputs::default())?.into_values())
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
    assert_eq!(
        rendered_at(&recipe, 2.0)?,
        applied("pattern", &pattern(64.0, 2.0), 2.0)?
    );
    assert_eq!(
        rendered_at(&recipe, 1.0)?,
        applied("pattern", &pattern(32.0, 1.0), 1.0)?
    );
    assert_ne!(rendered_at(&recipe, 2.0)?, rendered_at(&recipe, 1.0)?);
    Ok(())
}

#[test]
fn a_whole_number_of_pixels_scales_rounded_and_never_below_its_minimum() -> Outcome {
    let grid = |cell: i32| json!({"cell": cell, "gap": 0.15, "amount": 0.4, "keep_brightness": false});
    let recipe = Recipe::new([Step::new("pixel_grid")]); // `cell` is 4 px, at least 3
    assert_eq!(rendered_at(&recipe, 2.0)?, applied("pixel_grid", &grid(8), 2.0)?);
    // 4 x 0.5 = 2, below the minimum of 3
    assert_eq!(rendered_at(&recipe, 0.5)?, applied("pixel_grid", &grid(3), 0.5)?);
    // 4 x 1.6 = 6.4 -> 6
    assert_eq!(rendered_at(&recipe, 1.6)?, applied("pixel_grid", &grid(6), 1.6)?);
    Ok(())
}

#[test]
fn a_masked_step_changes_the_picture_only_where_the_mask_is_above_0() -> Outcome {
    let values = wide_values();
    // the left 12 of 30 columns are outside the mask, the rest inside it
    let weights: Vec<f32> = (0..20 * 30)
        .map(|index| f32::from(u8::from(index % 30 >= 12)))
        .collect();
    let photo = Photo::new(Picture::new(20, 30, &values)?).with_map("subject", Layer::new(20, 30, weights)?)?;
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
        &json!({"temperature": 0.0, "tint": 0.0, "exposure": 1.0, "contrast": 1.0, "pivot": 0.5, "saturation": 1.0,
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
    let photo = Photo::new(Picture::new(20, 30, &values)?).with_map("depth", Layer::new(20, 30, depth)?)?;
    let recipe = Recipe::new([Step::new("grade").param("exposure", Drive::new("depth", -1.0, 1.0).levels(2))]);
    let rendered = photo
        .render(&recipe, &Presets::new(), Options::default())?
        .into_values();
    let graded = |exposure: f64| {
        let params = json!({"temperature": 0.0, "tint": 0.0, "exposure": exposure, "contrast": 1.0, "pivot": 0.5,
                            "saturation": 1.0, "vibrance": 0.0});
        apply("grade", &Picture::new(20, 30, &values)?, &params, &Inputs::default()).map(Image::into_values)
    };
    let (darker, brighter) = (graded(-1.0)?, graded(1.0)?);
    for (index, pixel) in rendered.chunks(3).enumerate() {
        let expected = if index % 30 >= 12 { &brighter } else { &darker };
        assert_eq!(pixel, &expected[index * 3..index * 3 + 3], "pixel {index}");
    }
    Ok(())
}

/// A finder that takes the left 12 of 30 columns for the subject.
struct LeftSide;

impl Finder for LeftSide {
    fn find(&self, picture: &Picture<'_>) -> Result<Layer, bullterrier_image::Error> {
        let (height, width) = (picture.height(), picture.width());
        let alpha = (0..height * width)
            .map(|index| f32::from(u8::from(index % width < 12)))
            .collect();
        Layer::new(height, width, alpha)
    }
}

#[test]
fn a_step_masked_by_the_background_changes_the_picture_only_where_a_finder_found_no_subject() -> Outcome {
    let values = wide_values();
    let photo = Photo::new(Picture::new(20, 30, &values)?).with_finder("subject", Arc::new(LeftSide));
    let recipe = Recipe::new([Step::new("grade").param("exposure", 1.0).mask("background")]);
    let rendered = photo
        .render(&recipe, &Presets::new(), Options::default())?
        .into_values();
    let graded = apply(
        "grade",
        &Picture::new(20, 30, &values)?,
        &json!({"temperature": 0.0, "tint": 0.0, "exposure": 1.0, "contrast": 1.0, "pivot": 0.5, "saturation": 1.0,
               "vibrance": 0.0}),
        &Inputs::default(),
    )?
    .into_values();
    for (index, pixel) in rendered.chunks(3).enumerate() {
        let expected = if index % 30 < 12 { &values } else { &graded };
        assert_eq!(pixel, &expected[index * 3..index * 3 + 3], "pixel {index}");
    }
    Ok(())
}

#[test]
fn a_trace_shows_the_photo_the_subject_and_the_picture_after_each_step_ending_in_the_render() -> Outcome {
    let values = wide_values();
    let photo = Photo::new(Picture::new(20, 30, &values)?).with_finder("subject", Arc::new(LeftSide));
    let recipe = Recipe::new([
        Step::new("grade").param("exposure", 1.0).mask("subject"),
        Step::new("pixel_grid").param("cell", 4),
    ]);
    let stages = photo.trace(&recipe, &Presets::new(), Options::default())?;
    let rendered = photo.render(&recipe, &Presets::new(), Options::default())?;
    assert_eq!(stages.len(), 4);
    assert!(matches!(&stages[0], Stage::Input(input) if input.clone().into_values() == values));
    assert!(
        matches!(&stages[1], Stage::Subject(matte) if matte.values()[..30].iter().enumerate().all(|(column, alpha)| (*alpha > 0.5) == (column < 12)))
    );
    assert!(matches!(&stages[2], Stage::Step { number: 1, .. }));
    assert!(matches!(&stages[3], Stage::Step { number: 2, image } if *image == rendered));
    Ok(())
}

#[test]
fn a_handed_in_map_follows_the_photo_through_a_crop_a_border_and_a_size() -> Outcome {
    // A 6 x 8 photo that is white at row 2, column 5 and black elsewhere, and a map that is 1 at that pixel and 0
    // elsewhere. Cropped to a square against the right side (columns 2 to 7), given a border of 3 px on the left (half
    // of 6) and resized to a longest side of 24 (9 px across become 24, 6 rows become 16), they go through the same
    // operations, so the map the steps are given is the photo's red channel exactly, and its peak is where the white
    // pixel went: row (2 + 0.5) * 16 / 6 = 6.7, column (3 + 3 + 0.5) * 24 / 9 = 17.3.
    let (height, width) = (6, 8);
    let hot: Vec<f32> = (0..height * width)
        .map(|index| f32::from(u8::from(index == 2 * width + 5)))
        .collect();
    let values: Vec<f32> = hot.iter().flat_map(|&value| [value; 3]).collect();
    let photo =
        Photo::new(Picture::new(height, width, &values)?).with_map("subject", Layer::new(height, width, hot)?)?;
    let mut recipe = Recipe::new([Step::new("grade").opacity(0.0).mask("subject")]);
    recipe.frame = Frame {
        aspect: Some(1.0),
        anchor: Anchor::Right,
        pad_left: 0.5,
        ..Frame::default()
    };
    let stages = photo.trace(&recipe, &Presets::new(), Options::default().with_size(24))?;
    let (Some(Stage::Input(picture)), Some(Stage::Subject(map))) = (stages.first(), stages.get(1)) else {
        return Err(format!("the stages start otherwise: {stages:?}").into());
    };
    assert_eq!((picture.height(), picture.width()), (16, 24));
    assert_eq!((map.height(), map.width()), (16, 24));
    let red: Vec<f32> = picture.clone().into_values().chunks(3).map(|pixel| pixel[0]).collect();
    assert_eq!(map.values(), red);
    let peak = map
        .values()
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map_or(0, |(index, _)| index);
    assert_eq!((peak / 24, peak % 24), (6, 17));
    Ok(())
}
