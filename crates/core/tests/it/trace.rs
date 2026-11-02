//! The region stages of a trace, through the public API alone: which regions a trace shows, in which order, and that what
//! it shows is what the steps worked with.

use std::sync::Arc;

use bullterrier_image::region::{Band, Combine, CombineOp, Given, Grid, LinearGradient, RadialGradient, Shares};
use bullterrier_image::{
    Category, Each, Effect, Entry, Error, Image, Inputs, Labels, Layer, Options, Photo, Picture, Presets, Recipe,
    Stage, Step, register,
};

type Outcome = Result<(), Box<dyn std::error::Error>>;

/// An effect that paints the whole picture with one grey level.
struct Paint {
    name: String,
    level: f32,
}

impl Paint {
    fn register(name: &str, level: f32) -> Result<Step, Error> {
        register(Arc::new(Self {
            name: name.to_owned(),
            level,
        }))?;
        Ok(Step::new(name).opacity(1.0))
    }
}

impl Effect for Paint {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn apply(&self, picture: &Picture<'_>, _params: serde_json::Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        Ok(picture.map(|_| [self.level; 3]))
    }
}

const HEIGHT: usize = 4;
const WIDTH: usize = 9;

/// A grey ramp from 0.1 to 0.5 across, so that painting 0.95 changes every pixel.
fn ramp() -> Vec<f32> {
    (0..HEIGHT * WIDTH)
        .flat_map(|index| [0.1 + 0.4 * f32::from(u8::try_from(index % WIDTH).unwrap_or(0)) / 9.0; 3])
        .collect()
}

/// Depth falling from 1 at the left to 0 at the right.
fn depth() -> Result<Layer, Error> {
    Layer::new(
        HEIGHT,
        WIDTH,
        (0..HEIGHT * WIDTH)
            .map(|index| 1.0 - f32::from(u8::try_from(index % WIDTH).unwrap_or(0)) / 8.0)
            .collect(),
    )
}

/// What a stage is, for the order: `input`, `subject`, `depth`, `region NAME (selection)`, `region NAME (partition)`,
/// `step N`.
fn kinds(stages: &[Stage]) -> Vec<String> {
    stages
        .iter()
        .map(|stage| match stage {
            Stage::Input(_) => "input".to_owned(),
            Stage::Subject(_) => "subject".to_owned(),
            Stage::Depth(_) => "depth".to_owned(),
            Stage::Selection { name, .. } => format!("selection {name}"),
            Stage::Partition { name, .. } => format!("partition {name}"),
            Stage::Step { number, .. } => format!("step {number}"),
            _ => "other".to_owned(),
        })
        .collect()
}

fn selection<'a>(stages: &'a [Stage], wanted: &str) -> Option<&'a Layer> {
    stages.iter().find_map(|stage| match stage {
        Stage::Selection { name, layer } if name == wanted => Some(layer),
        _ => None,
    })
}

fn partition<'a>(stages: &'a [Stage], wanted: &str) -> Option<&'a Labels> {
    stages.iter().find_map(|stage| match stage {
        Stage::Partition { name, labels } if name == wanted => Some(labels),
        _ => None,
    })
}

fn step_image(stages: &[Stage], wanted: usize) -> Option<&Image> {
    stages.iter().find_map(|stage| match stage {
        Stage::Step { number, image } if *number == wanted => Some(image),
        _ => None,
    })
}

/// The recipe of the tests. The regions are declared in an order no sorting gives: `zeta` (a band of the depth), `mid`
/// (a grid of three columns), `alpha` (a gradient), `both` (alpha and zeta multiplied) and `unused`, which no step
/// uses. Step 1 paints through `both`, step 2 gives each of the three columns of `mid` a look.
fn recipe(prefix: &str) -> Result<Recipe, Error> {
    let paint = Paint::register(&format!("{prefix}_paint"), 0.95)?;
    let looks = [0.2, 0.5, 0.8]
        .iter()
        .enumerate()
        .map(|(index, level)| Paint::register(&format!("{prefix}_look{index}"), *level).map(Some))
        .collect::<Result<Vec<_>, _>>()?;
    let mut each = Each::new("mid", looks);
    each.feather = 0.0;
    Ok(Recipe::new([Entry::from(paint.mask("both")), Entry::from(each)])
        .with_region(
            "zeta",
            Band {
                of: "depth".to_owned(),
                low: 0.5,
                high: 1.0,
                feather: 0.25,
            },
        )
        .with_region(
            "mid",
            Grid {
                columns: Shares::equal(3),
                ..Grid::default()
            },
        )
        .with_region("alpha", LinearGradient { angle: 0.0 })
        .with_region(
            "both",
            Combine {
                op: CombineOp::Multiply,
                of: vec!["alpha".to_owned(), "zeta".to_owned()],
            },
        )
        .with_region("unused", RadialGradient::default()))
}

fn trace(values: &[f32], recipe: &Recipe) -> Result<Vec<Stage>, Error> {
    Photo::new(Picture::new(HEIGHT, WIDTH, values)?)
        .with_map("depth", depth()?)?
        .trace(recipe, &Presets::new(), Options::default())
}

#[test]
fn the_regions_the_steps_use_are_shown_in_the_order_they_are_declared_after_the_maps_and_before_the_steps() -> Outcome {
    let stages = trace(&ramp(), &recipe("trace_order")?)?;
    // `zeta` and `alpha` are used only through `both`, `unused` by no step; depth is used through `zeta`, the subject by none
    assert_eq!(
        kinds(&stages),
        [
            "input",
            "depth",
            "selection zeta",
            "partition mid",
            "selection alpha",
            "selection both",
            "step 1",
            "step 2"
        ]
    );
    Ok(())
}

#[test]
fn a_selection_stage_is_where_the_steps_masked_by_it_change_the_picture() -> Outcome {
    let values = ramp();
    let stages = trace(&values, &recipe("trace_mask")?)?;
    let both = selection(&stages, "both").ok_or(Error::Recipe("no stage".to_owned()))?;
    let (zeta, alpha) = (
        selection(&stages, "zeta").ok_or(Error::Recipe("no stage".to_owned()))?,
        selection(&stages, "alpha").ok_or(Error::Recipe("no stage".to_owned()))?,
    );
    // independent oracle: `both` is alpha x zeta, and zeta is 1 over depth from 0.5 up and falls to 0 over a feather of
    // 0.25 below: depth 1 - c / 8 for column c
    for column in 0..WIDTH {
        let depth = 1.0 - f32::from(u8::try_from(column)?) / 8.0;
        let expected_zeta = ((depth - 0.25) / 0.25).clamp(0.0, 1.0).min(1.0);
        let expected_zeta = if depth >= 0.5 { 1.0 } else { expected_zeta };
        assert!(
            (zeta.values()[column] - expected_zeta).abs() <= 1e-6,
            "zeta, column {column}"
        );
        let expected_alpha = (f32::from(u8::try_from(column)?) + 0.5) / 9.0;
        assert!(
            (alpha.values()[column] - expected_alpha).abs() <= 1e-6,
            "alpha, column {column}"
        );
        assert!(
            (both.values()[column] - expected_alpha * expected_zeta).abs() <= 1e-6,
            "both, column {column}"
        );
    }
    // step 1 painted 0.95 through `both`: a pixel changed exactly where the stage is above 0
    let painted = step_image(&stages, 1)
        .ok_or(Error::Recipe("no step".to_owned()))?
        .clone()
        .into_values();
    for (index, chosen) in both.values().iter().enumerate() {
        let changed = (painted[index * 3] - values[index * 3]).abs() > 1e-7;
        assert_eq!(changed, *chosen > 0.0, "pixel {index}, chosen {chosen}");
    }
    Ok(())
}

#[test]
fn a_partition_stage_numbers_the_pieces_the_looks_were_given_to() -> Outcome {
    let stages = trace(&ramp(), &recipe("trace_pieces")?)?;
    let mid = partition(&stages, "mid").ok_or(Error::Recipe("no stage".to_owned()))?;
    assert_eq!((mid.height(), mid.width(), mid.pieces()), (HEIGHT, WIDTH, 3));
    // step 2 gave the pieces, in turn, 0.2, 0.5, 0.8 (hard borders): a pixel's level is the one its label says
    let image = step_image(&stages, 2)
        .ok_or(Error::Recipe("no step".to_owned()))?
        .clone()
        .into_values();
    for (index, label) in mid.values().iter().enumerate() {
        let expected = [0.2, 0.5, 0.8][usize::try_from(*label)?];
        assert!((image[index * 3] - expected).abs() <= 1e-6, "pixel {index}");
    }
    // three columns of three pixels, by hand
    let first_row: Vec<i32> = mid.values()[..WIDTH].to_vec();
    assert_eq!(first_row, [0, 0, 0, 1, 1, 1, 2, 2, 2]);
    Ok(())
}

#[test]
fn the_last_stage_is_what_render_returns_for_a_recipe_with_regions_and_an_each() -> Outcome {
    let values = ramp();
    let recipe = recipe("trace_last")?;
    let stages = trace(&values, &recipe)?;
    let rendered = Photo::new(Picture::new(HEIGHT, WIDTH, &values)?)
        .with_map("depth", depth()?)?
        .render(&recipe, &Presets::new(), Options::default())?;
    assert!(matches!(stages.last(), Some(Stage::Step { number: 2, image }) if *image == rendered));
    Ok(())
}

#[test]
fn a_given_region_is_shown_as_it_was_handed_in_and_a_declared_one_no_step_uses_is_not() -> Outcome {
    let paint = Paint::register("trace_given_paint", 0.9)?;
    let recipe = Recipe::new([paint.mask("sky")])
        .with_region("sky", Given::default())
        .with_region("spare", Given::default());
    let sky = Layer::new(HEIGHT, WIDTH, vec![0.25; HEIGHT * WIDTH])?;
    let stages = Photo::new(Picture::new(HEIGHT, WIDTH, &ramp())?)
        .with_map("sky", sky)?
        .trace(&recipe, &Presets::new(), Options::default())?;
    assert_eq!(kinds(&stages), ["input", "selection sky", "step 1"]);
    assert_eq!(selection(&stages, "sky").map(|layer| layer.values()[0]), Some(0.25));
    Ok(())
}

#[test]
fn a_trace_fails_where_render_fails_for_a_region_that_cannot_be_made() -> Outcome {
    // a grid laid over a selection that chooses nothing (depth 0 everywhere, a band of the nearest tenth) has no frame
    let look = Paint::register("trace_fails_paint", 0.9)?;
    let recipe = Recipe::new([Each::new("cells", [Some(look)])])
        .with_region(
            "none",
            Band {
                of: "depth".to_owned(),
                low: 0.9,
                high: 1.0,
                feather: 0.0,
            },
        )
        .with_region(
            "cells",
            Grid {
                fit: Some("none".to_owned()),
                ..Grid::default()
            },
        );
    let values = ramp();
    let photo = Photo::new(Picture::new(HEIGHT, WIDTH, &values)?)
        .with_map("depth", Layer::new(HEIGHT, WIDTH, vec![0.0; HEIGHT * WIDTH])?)?;
    let rendered = photo.render(&recipe, &Presets::new(), Options::default());
    let traced = photo.trace(&recipe, &Presets::new(), Options::default());
    assert!(
        matches!((&rendered, &traced), (Err(a), Err(b)) if a.to_string() == b.to_string() && a.to_string().contains("fit")),
        "{rendered:?} / {traced:?}"
    );
    Ok(())
}
