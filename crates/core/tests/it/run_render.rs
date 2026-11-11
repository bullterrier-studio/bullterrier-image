//! `Photo::render`, through the public API alone, with effects of the tests' own: a recipe is checked before anything
//! runs, a field parameter receives the map its step names, a map or finder given again replaces the first, and each step
//! works from the output of the one before.

use std::sync::Arc;

use crate::common::Outcome;
use crate::common::pictures::counted;
use crate::common::run::{Finds, Paint, Recorder, Registered, Shrinks, probe_table, xyz_table};
use bullterrier_image::region::Given;
use bullterrier_image::{Drive, Error, ErrorKind, Layer, Options, Photo, Picture, Presets, Recipe, Step};

/// A map of the 4 x 6 picture whose every value is `level`.
fn flat(level: f32) -> Result<Layer, Error> {
    Layer::new(4, 6, vec![level; 24])
}

#[test]
fn a_recipe_that_breaks_a_rule_of_a_look_is_refused_before_anything_runs() -> Outcome {
    let recorder = Registered::new(Recorder::new(&[], &[]).with_table(probe_table()))?;
    let name = recorder.name();
    let values = counted();
    let photo = Photo::new(Picture::new(4, 6, &values)?);
    // the first step is fine; the second names no effect, a value out of range or a mask that is no region
    for (recipe, said) in [
        (
            Recipe::new([Step::new(name), Step::new("no_such_effect")]),
            "no_such_effect",
        ),
        (
            Recipe::new([Step::new(name), Step::new(name).param("cells", 99)]),
            "step 2",
        ),
        (Recipe::new([Step::new(name).mask("nowhere")]), "which is no region"),
        (Recipe::new(Vec::<Step>::new()), "at least one step"),
    ] {
        let result = photo.render(&recipe, &Presets::new(), Options::default());
        assert!(
            matches!(&result, Err(error) if error.to_string().contains(said)),
            "{said}: {result:?}"
        );
    }
    assert_eq!(recorder.received().len(), 0);
    Ok(())
}

#[test]
fn a_field_parameter_receives_the_map_its_step_names_or_the_effects_default() -> Outcome {
    let recorder = Registered::new(Recorder::new(&[], &["near", "far"]))?;
    let values = counted();
    let photo = Photo::new(Picture::new(4, 6, &values)?)
        .with_map("subject", flat(0.25)?)?
        .with_map("NEAR", flat(0.75)?)?
        .with_map("FAR", flat(0.5)?)?;
    let render = |step: Step| {
        let mut recipe = Recipe::new([step]);
        recipe.regions.insert("NEAR", Given {});
        recipe.regions.insert("FAR", Given {});
        photo.render(&recipe, &Presets::new(), Options::default())
    };
    render(Step::new(recorder.name()).param("near", "subject"))?;
    render(Step::new(recorder.name()))?; // the recorder's default field is the parameter upcased
    let received = recorder.received();
    // each parameter under its own name, the named map or the default one
    assert_eq!(received[0].fields["near"], vec![0.25; 24]);
    assert_eq!(received[0].fields["far"], vec![0.5; 24]);
    assert_eq!(received[1].fields["near"], vec![0.75; 24]);
    assert_eq!(received[1].fields["far"], vec![0.5; 24]);
    let missing = render(Step::new(recorder.name()).param("far", "depth"));
    assert!(
        matches!(&missing, Err(error) if error.kind() == ErrorKind::DepthNotFound),
        "{missing:?}"
    );
    let mistyped = render(Step::new(recorder.name()).param("near", 3));
    assert!(
        matches!(&mistyped, Err(error @ Error::Invalid { .. }) if error.to_string().contains("`near`")),
        "{mistyped:?}"
    );
    Ok(())
}

/// The map `subject` that an effect with the field parameter `near` receives when `photo` is rendered with a step
/// that sets `near` to `subject`.
fn subject_received(recorder: &Registered<Recorder>, photo: &Photo<'_>) -> Result<Vec<f32>, Error> {
    let step = Step::new(recorder.name()).param("near", "subject");
    photo.render(&Recipe::new([step]), &Presets::new(), Options::default())?;
    let received = recorder.received();
    Ok(received[received.len() - 1].fields["near"].clone())
}

#[test]
fn a_map_given_again_under_the_same_name_replaces_the_first() -> Outcome {
    let recorder = Registered::new(Recorder::new(&[], &["near"]))?;
    let values = counted();
    let photo = Photo::new(Picture::new(4, 6, &values)?)
        .with_map("subject", flat(0.25)?)?
        .with_map("subject", flat(0.75)?)?;
    assert_eq!(subject_received(&recorder, &photo)?, vec![0.75; 24]);
    Ok(())
}

#[test]
fn a_finder_given_again_under_the_same_name_replaces_the_first() -> Outcome {
    let recorder = Registered::new(Recorder::new(&[], &["near"]))?;
    let values = counted();
    let (first, second) = (Arc::new(Finds::flat(0.25)), Arc::new(Finds::flat(0.75)));
    let photo = Photo::new(Picture::new(4, 6, &values)?)
        .with_finder("subject", first.clone())
        .with_finder("subject", second.clone());
    assert_eq!(subject_received(&recorder, &photo)?, vec![0.75; 24]);
    assert_eq!((first.calls(), second.calls()), (0, 1));
    Ok(())
}

#[test]
fn a_drive_the_photo_has_no_map_for_or_that_breaks_a_rule_refuses_the_render_and_nothing_runs() -> Outcome {
    let recorder = Registered::new(Recorder::new(&[], &[]).with_table(xyz_table()))?;
    let drive = |by: &str| Drive::new(by, 0.0, 1.0);
    let values = counted();
    let photo = Photo::new(Picture::new(4, 6, &values)?);
    for (step, said) in [
        (Step::new(recorder.name()).param("x", drive("depth")), "'depth'"),
        (
            Step::new(recorder.name())
                .param("x", drive("depth"))
                .param("y", drive("light")),
            "one field",
        ),
        (
            Step::new(recorder.name())
                .param("x", drive("depth"))
                .param("y", drive("depth").levels(3)),
            "one number of levels",
        ),
        (
            Step::new(recorder.name()).param("x", drive("depth").levels(17)),
            "from 2 to 16",
        ),
    ] {
        let result = photo.render(&Recipe::new([step]), &Presets::new(), Options::default());
        assert!(
            matches!(&result, Err(error) if error.to_string().contains(said)),
            "{said}: {result:?}"
        );
    }
    assert_eq!(recorder.received().len(), 0);
    Ok(())
}

#[test]
fn an_effect_that_changes_the_pictures_shape_fails_naming_it() -> Outcome {
    let shrinks = Registered::new(Shrinks::new())?;
    let values = counted();
    let photo = Photo::new(Picture::new(4, 6, &values)?);
    let result = photo.render(
        &Recipe::new([Step::new(shrinks.name())]),
        &Presets::new(),
        Options::default(),
    );
    assert!(
        matches!(&result, Err(error @ Error::Foreign { effect, .. })
            if effect == shrinks.name() && error.to_string().contains("4 x 6") && error.to_string().contains("1 x 1")),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn each_step_runs_on_the_output_of_the_one_before_and_the_photo_is_left_as_it_was() -> Outcome {
    let paint = Registered::new(Paint::new())?;
    let recorder = Registered::new(Recorder::new(&[], &[]))?;
    let recipe = Recipe::new([
        Step::new(paint.name()).param("level", 0.75).opacity(1.0),
        Step::new(recorder.name()),
    ]);
    let before = counted();
    let photo = Photo::new(Picture::new(4, 6, &before)?);
    let image = photo.render(&recipe, &Presets::new(), Options::default())?;
    assert_eq!(image.into_values(), vec![0.75; 72]);
    assert_eq!(before, counted());
    // what the second step was handed is the first one's picture, not the photo
    assert_eq!(recorder.received()[0].picture, vec![0.75; 72]);
    Ok(())
}
