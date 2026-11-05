//! The size a render works at, through the public API alone: `Options::with_size` and a recipe's `size` resample the photo
//! and its maps with the aspect kept, and the ratio of design pixels is the working picture's longest side over 480.

use std::sync::{Arc, Mutex};

use crate::common::Outcome;
use crate::common::pictures::ramp;
use crate::common::run::{Finds, Recorder, Registered};
use bullterrier_image::{Error, ErrorKind, Image, Input, Layer, Options, Photo, Picture, Presets, Recipe, Stage, Step};

/// A recorder that declares the size ratio and one field parameter, `near`.
fn rig() -> Result<Registered<Recorder>, Error> {
    Registered::new(Recorder::new(&[Input::SizeRatio], &["near"]))
}

/// A step of the recorder that works from the map `map`.
fn uses(recorder: &Registered<Recorder>, map: &str) -> Step {
    Step::new(recorder.name()).param("near", map)
}

/// `recipe` rendered on `photo` with `options`.
fn render(photo: &Photo<'_>, recipe: &Recipe, options: Options) -> Result<Image, Error> {
    photo.render(recipe, &Presets::new(), options)
}

#[test]
fn a_render_asked_for_a_size_returns_a_picture_whose_longest_side_is_it_and_keeps_the_aspect() -> Outcome {
    let recorder = rig()?;
    let values = ramp(4, 6);
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", Arc::new(Finds::flat(0.5)));
    let recipe = Recipe::new([uses(&recorder, "subject")]);
    // (the size asked for, the picture that makes): 4 x 6 is 2:3, so 12 gives 8 x 12, 8 gives 5.33 -> 5 x 8, 2 gives
    // 1.33 -> 1 x 2 and 1 gives 0.67 -> 1 x 1 (never below a pixel); 6 is the photo's own size
    for (size, height, width) in [(12, 8, 12), (8, 5, 8), (2, 1, 2), (1, 1, 1), (6, 4, 6), (600, 400, 600)] {
        let image = render(&photo, &recipe, Options::default().with_size(size))?;
        assert_eq!((image.height(), image.width()), (height, width), "size {size}");
    }
    Ok(())
}

#[test]
fn the_size_comes_from_the_options_else_the_recipe_else_the_photo() -> Outcome {
    let recorder = rig()?;
    let values = ramp(4, 6);
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", Arc::new(Finds::flat(0.5)));
    let mut recipe = Recipe::new([uses(&recorder, "subject")]);
    let shape = |recipe: &Recipe, options: Options| -> Result<(usize, usize), Error> {
        let image = render(&photo, recipe, options)?;
        Ok((image.height(), image.width()))
    };
    assert_eq!(shape(&recipe, Options::default())?, (4, 6));
    recipe.size = Some(12);
    assert_eq!(shape(&recipe, Options::default())?, (8, 12));
    assert_eq!(shape(&recipe, Options::default().with_size(3))?, (2, 3));
    Ok(())
}

#[test]
fn a_picture_of_the_size_asked_for_is_rendered_as_it_is() -> Outcome {
    let recorder = rig()?;
    let values = ramp(4, 6);
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", Arc::new(Finds::flat(0.5)));
    let image = render(
        &photo,
        &Recipe::new([uses(&recorder, "subject")]),
        Options::default().with_size(6),
    )?;
    assert_eq!(image.into_values(), values);
    assert_eq!(recorder.received()[0].picture, values);
    Ok(())
}

#[test]
fn the_steps_work_on_the_resized_picture_and_the_photo_is_left_as_it_was() -> Outcome {
    let recorder = rig()?;
    let values = ramp(1, 4);
    let photo = Photo::new(Picture::new(1, 4, &values)?).with_finder("subject", Arc::new(Finds::flat(0.5)));
    // a grey ramp 0, 1/3, 2/3, 1 grown to 8 pixels (1 x 4 -> 2 x 8): the step is handed 16 pixels
    let image = render(
        &photo,
        &Recipe::new([uses(&recorder, "subject")]),
        Options::default().with_size(8),
    )?;
    assert_eq!((image.height(), image.width()), (2, 8));
    assert_eq!(recorder.received()[0].picture.len(), 2 * 8 * 3);
    assert_eq!(values, ramp(1, 4));
    Ok(())
}

#[test]
fn the_ratio_of_design_pixels_is_the_working_pictures_longest_side_over_480() -> Outcome {
    let recorder = Registered::new(Recorder::new(&[Input::SizeRatio], &[]))?;
    let recipe = Recipe::new([Step::new(recorder.name())]);
    // (the photo's height and width, the size asked for, the ratio the effect is handed): 1 x 960 is twice the design
    // size; asked for 240 it is half; a photo of 3 x 480 is the design size itself, and asked for 1200 it is 2.5
    for (height, width, size, ratio) in [
        (1, 960, None, 2.0),
        (1, 960, Some(240), 0.5),
        (3, 480, None, 1.0),
        (3, 480, Some(1200), 2.5),
        (960, 3, None, 2.0),
    ] {
        let values = vec![0.5; height * width * 3];
        let photo = Photo::new(Picture::new(height, width, &values)?);
        let options = size.map_or_else(Options::default, |size| Options::default().with_size(size));
        render(&photo, &recipe, options)?;
        let handed = recorder.received().pop().map(|received| received.size_ratio);
        assert_eq!(handed, Some(ratio), "{height} x {width} at {size:?}");
    }
    Ok(())
}

#[test]
fn a_map_follows_the_photo_to_its_size_on_the_same_grid() -> Outcome {
    let recorder = rig()?;
    // the photo's grey is the map: after resizing, the map the effect is handed is the picture it is handed. Grow
    // (4 x 6 to 8 x 12) and shrink (4 x 6 to 2 x 3), a map handed in and a map found.
    let values = ramp(4, 6);
    let layer = Layer::new(4, 6, values.chunks(3).map(|pixel| pixel[0]).collect())?;
    for (size, pixels) in [(12, 8 * 12), (3, 2 * 3)] {
        for (how, photo) in [
            (
                "handed in",
                Photo::new(Picture::new(4, 6, &values)?).with_map("subject", layer.clone())?,
            ),
            (
                "found",
                Photo::new(Picture::new(4, 6, &values)?).with_finder(
                    "subject",
                    Arc::new(Finds::new({
                        let layer = layer.clone();
                        move |_| Ok(layer.clone())
                    })),
                ),
            ),
        ] {
            recorder.clear();
            render(
                &photo,
                &Recipe::new([uses(&recorder, "subject")]),
                Options::default().with_size(size),
            )?;
            let received = recorder.received();
            let (picture, map) = (&received[0].picture, &received[0].fields["near"]);
            assert_eq!(map.len(), pixels, "{how} at {size}");
            assert_eq!(picture.len(), pixels * 3, "{how} at {size}");
            assert!(
                map.iter()
                    .zip(picture.chunks(3))
                    .all(|(map, pixel)| (map - pixel[0]).abs() < 1e-6),
                "{how} at {size}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_finder_is_asked_with_the_photo_as_given_and_only_once_whatever_the_sizes() -> Outcome {
    let recorder = rig()?;
    let values = ramp(4, 6);
    let asked = Arc::new(Mutex::new(Vec::new()));
    let finder = Arc::new(Finds::new({
        let asked = Arc::clone(&asked);
        move |picture| {
            if let Ok(mut sizes) = asked.lock() {
                sizes.push((picture.height(), picture.width()));
            }
            Layer::new(
                picture.height(),
                picture.width(),
                vec![0.5; picture.height() * picture.width()],
            )
        }
    }));
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", finder.clone());
    for size in [12, 3, 6] {
        render(
            &photo,
            &Recipe::new([uses(&recorder, "subject")]),
            Options::default().with_size(size),
        )?;
    }
    assert_eq!(finder.calls(), 1);
    assert_eq!(asked.lock().map(|sizes| sizes.clone()).ok(), Some(vec![(4, 6)]));
    Ok(())
}

#[test]
fn light_and_background_are_the_working_pictures() -> Outcome {
    let recorder = rig()?;
    // a grey photo has its own grey as its brightness (the Rec. 709 weights sum to 1)
    let values = ramp(4, 6);
    let subject = Layer::new(4, 6, vec![0.25; 24])?;
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_map("subject", subject)?;
    render(
        &photo,
        &Recipe::new([uses(&recorder, "light"), uses(&recorder, "background")]),
        Options::default().with_size(12),
    )?;
    let received = recorder.received();
    let (picture, light) = (&received[0].picture, &received[0].fields["near"]);
    assert_eq!(light.len(), 8 * 12);
    assert!(
        light
            .iter()
            .zip(picture.chunks(3))
            .all(|(light, pixel)| (light - pixel[0]).abs() < 1e-6)
    );
    assert_eq!(received[1].fields["near"], vec![0.75; 8 * 12]);
    Ok(())
}

#[test]
fn a_trace_shows_every_stage_at_the_working_size_and_ends_with_what_render_returns() -> Outcome {
    let recorder = rig()?;
    let values = ramp(4, 6);
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_map("subject", Layer::new(4, 6, vec![0.5; 24])?)?;
    let recipe = Recipe::new([uses(&recorder, "subject"), uses(&recorder, "subject").mask("subject")]);
    let options = Options::default().with_size(12);
    let stages = photo.trace(&recipe, &Presets::new(), options)?;
    let rendered = render(&photo, &recipe, options)?;
    assert_eq!(stages.len(), 4);
    for stage in &stages {
        let shape = match stage {
            Stage::Input(image) | Stage::Step { image, .. } | Stage::CutOut(image) => (image.height(), image.width()),
            Stage::Subject(layer) | Stage::Depth(layer) | Stage::Selection { layer, .. } => {
                (layer.height(), layer.width())
            }
            Stage::Partition { labels, .. } => (labels.height(), labels.width()),
            _ => (0, 0),
        };
        assert_eq!(shape, (8, 12));
    }
    assert!(matches!(stages.last(), Some(Stage::Step { image, .. }) if image == &rendered));
    Ok(())
}

#[test]
fn a_size_no_picture_can_have_is_refused_at_once_naming_the_size_and_the_limit() -> Outcome {
    let recorder = rig()?;
    let values = vec![0.5; 3];
    // a billion-pixel side asked of a one-pixel photo: refused without making a picture for it (which would take tens
    // of gigabytes)
    let photo = Photo::new(Picture::new(1, 1, &values)?).with_finder("subject", Arc::new(Finds::flat(0.5)));
    let result = render(
        &photo,
        &Recipe::new([uses(&recorder, "subject")]),
        Options::default().with_size(2_000_000_000),
    );
    assert!(
        matches!(&result, Err(error) if error.kind() == ErrorKind::Recipe
            && error.to_string().contains("`size` 2000000000")
            && error.to_string().contains("2000000000 x 2000000000")
            && error.to_string().contains("67108864")),
        "{result:?}"
    );
    assert_eq!(recorder.received().len(), 0);
    Ok(())
}

#[test]
fn a_size_of_0_a_size_too_large_to_hold_is_refused_before_anything_runs() -> Outcome {
    let recorder = rig()?;
    let values = ramp(4, 6);
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", Arc::new(Finds::flat(0.5)));
    let mut recipe = Recipe::new([uses(&recorder, "subject")]);
    let asked = |recipe: &Recipe, options: Options| render(&photo, recipe, options);
    let refused = [
        ("an option of 0", asked(&recipe, Options::default().with_size(0))),
        (
            "an option past what a picture can hold",
            asked(&recipe, Options::default().with_size(u32::MAX)),
        ),
    ];
    recipe.size = Some(0);
    let refused = refused
        .into_iter()
        .chain([("a recipe size of 0", asked(&recipe, Options::default()))]);
    for (what, result) in refused {
        assert!(
            matches!(&result, Err(error) if error.kind() == ErrorKind::Recipe && error.to_string().contains("`size`")),
            "{what}: {result:?}"
        );
    }
    assert_eq!(recorder.received().len(), 0);
    Ok(())
}
