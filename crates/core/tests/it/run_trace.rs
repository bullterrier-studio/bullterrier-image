//! `Photo::trace`, through the public API alone: the stages are the input, the maps the recipe uses and one picture per
//! step, the last of which is what `render` returns, and each step's picture is what the recipe cut off after that step
//! renders.

use std::sync::Arc;

use crate::common::Outcome;
use crate::common::run::{Finds, Paint, Recorder, Registered};
use bullterrier_image::{BlendMode, Drive, Error, Image, Layer, Options, Photo, Picture, Presets, Recipe, Stage, Step};

/// The values of a 4 x 6 picture, a different one in every channel.
fn values() -> Vec<f32> {
    (0_u8..72).map(|index| f32::from(index) / 80.0).collect()
}

/// A 4 x 6 map whose values step through `pattern`, repeated.
fn map(pattern: [f32; 4]) -> Result<Layer, Error> {
    Layer::new(4, 6, (0..24).map(|index| pattern[index % 4]).collect())
}

/// A `paint` and a `recorder` with the field parameter `near`.
struct Rig {
    paint: Registered<Paint>,
    recorder: Registered<Recorder>,
}

fn rig() -> Result<Rig, Error> {
    Ok(Rig {
        paint: Registered::new(Paint::new())?,
        recorder: Registered::new(Recorder::new(&[], &["near"]))?,
    })
}

impl Rig {
    fn paint(&self, level: f64) -> Step {
        Step::new(self.paint.name()).param("level", level)
    }

    fn near(&self, map: &str) -> Step {
        Step::new(self.recorder.name()).param("near", map)
    }

    /// Recipes that between them take every path of a step, each with a name for the failure message.
    fn recipes(&self) -> Vec<(&'static str, Recipe)> {
        let driven = self.paint(0.0).param("level", Drive::new("depth", 0.2, 0.8));
        vec![
            ("plain", Recipe::new([self.paint(0.2).opacity(1.0), self.paint(0.7)])),
            (
                "masked",
                Recipe::new([self.paint(0.9).mask("subject"), self.paint(0.4).opacity(0.5)]),
            ),
            (
                "blended",
                Recipe::new([self.paint(0.6).blend_mode(BlendMode::Multiply).opacity(0.5)]),
            ),
            ("driven", Recipe::new([driven.opacity(1.0)])),
            (
                "background",
                Recipe::new([self.paint(0.3).mask("background"), self.paint(0.5).mask("light")]),
            ),
        ]
    }
}

/// The kinds of `stages` in order: `input`, `subject`, `depth`, `step 2`.
fn kinds(stages: &[Stage]) -> Vec<String> {
    stages
        .iter()
        .map(|stage| match stage {
            Stage::Input(_) => "input".to_owned(),
            Stage::Subject(_) => "subject".to_owned(),
            Stage::Depth(_) => "depth".to_owned(),
            Stage::Selection { name, .. } | Stage::Partition { name, .. } => format!("region {name}"),
            Stage::Step { number, .. } => format!("step {number}"),
            Stage::CutOut(_) => "cut out".to_owned(),
            _ => "unknown".to_owned(),
        })
        .collect()
}

/// The photo of `values` with a handed-in subject and depth.
fn photo(values: &[f32]) -> Result<Photo<'_>, Error> {
    Photo::new(Picture::new(4, 6, values)?)
        .with_map("subject", map([0.0, 0.25, 1.0, 0.5])?)?
        .with_map("depth", map([1.0, 0.5, 0.0, 0.75])?)
}

fn rendered(photo: &Photo<'_>, recipe: &Recipe) -> Result<Image, Error> {
    photo.render(recipe, &Presets::new(), Options::default())
}

fn traced(photo: &Photo<'_>, recipe: &Recipe) -> Result<Vec<Stage>, Error> {
    photo.trace(recipe, &Presets::new(), Options::default())
}

#[test]
fn the_last_stage_of_a_trace_is_exactly_what_render_returns() -> Outcome {
    let rig = rig()?;
    let values = values();
    let photo = photo(&values)?;
    for (name, recipe) in rig.recipes() {
        let stages = traced(&photo, &recipe)?;
        let last = stages.last();
        let expected = rendered(&photo, &recipe)?;
        assert!(
            matches!(last, Some(Stage::Step { number, image }) if *number == recipe.steps.len() && *image == expected),
            "{name}: {last:?}"
        );
    }
    Ok(())
}

#[test]
fn the_picture_after_each_step_is_what_the_recipe_cut_off_after_that_step_renders() -> Outcome {
    let rig = rig()?;
    let values = values();
    let photo = photo(&values)?;
    for (name, recipe) in rig.recipes() {
        let stages = traced(&photo, &recipe)?;
        let steps: Vec<&Stage> = stages
            .iter()
            .filter(|stage| matches!(stage, Stage::Step { .. }))
            .collect();
        assert_eq!(steps.len(), recipe.steps.len(), "{name}");
        for (index, stage) in steps.into_iter().enumerate() {
            let mut prefix = recipe.clone();
            prefix.steps.truncate(index + 1);
            let image = rendered(&photo, &prefix)?;
            assert_eq!(
                *stage,
                Stage::Step {
                    number: index + 1,
                    image
                },
                "{name}, step {}",
                index + 1
            );
        }
    }
    Ok(())
}

#[test]
fn the_stages_are_the_input_the_maps_the_recipe_uses_and_one_picture_per_step() -> Outcome {
    let rig = rig()?;
    let values = values();
    let photo = photo(&values)?;
    // (the recipe, the stages it makes)
    let rows = [
        (
            Recipe::new([rig.paint(0.5), rig.paint(0.6)]),
            vec!["input", "step 1", "step 2"],
        ),
        (
            Recipe::new([rig.paint(0.5).mask("subject")]),
            vec!["input", "subject", "step 1"],
        ),
        (Recipe::new([rig.near("depth")]), vec!["input", "depth", "step 1"]),
        (
            Recipe::new([rig.near("depth"), rig.near("subject")]),
            vec!["input", "subject", "depth", "step 1", "step 2"],
        ),
        // `background` is made from the subject, `light` from the picture
        (
            Recipe::new([rig.near("background")]),
            vec!["input", "subject", "step 1"],
        ),
        (Recipe::new([rig.near("light")]), vec!["input", "step 1"]),
    ];
    for (recipe, expected) in rows {
        assert_eq!(kinds(&traced(&photo, &recipe)?), expected, "{:?}", recipe.steps);
    }
    // what the stages hold: the photo, the maps as handed in
    let stages = traced(&photo, &Recipe::new([rig.near("depth"), rig.near("subject")]))?;
    assert_eq!(stages[0], Stage::Input(Image::new(4, 6, &values)?));
    assert_eq!(stages[1], Stage::Subject(map([0.0, 0.25, 1.0, 0.5])?));
    assert_eq!(stages[2], Stage::Depth(map([1.0, 0.5, 0.0, 0.75])?));
    Ok(())
}

#[test]
fn a_trace_finds_its_maps_like_a_render_does_and_fails_where_it_would() -> Outcome {
    let rig = rig()?;
    let values = values();
    let finder = Arc::new(Finds::flat(0.5));
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", finder.clone());
    let uses_subject = Recipe::new([rig.near("subject")]);
    traced(&photo, &uses_subject)?;
    rendered(&photo, &uses_subject)?;
    assert_eq!(finder.calls(), 1);
    // no depth: the same error as a render's
    let uses_depth = Recipe::new([rig.near("depth")]);
    let (trace, render) = (traced(&photo, &uses_depth), rendered(&photo, &uses_depth));
    assert!(matches!((&trace, &render), (Err(a), Err(b)) if a.kind() == b.kind() && a.to_string() == b.to_string()));
    Ok(())
}
