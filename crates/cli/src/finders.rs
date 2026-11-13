//! The default finders of the maps a project's recipes need.

use std::sync::Arc;

use bullterrier_image::Error;
use bullterrier_image::finding::onnx::Options;
use bullterrier_image::finding::prompted::{Point, Prompts};
use bullterrier_image::finding::{DEFAULT_EDGE, ModelKind};
use bullterrier_image::pipeline::Found;
use bullterrier_image_finders::{Segmenter, depth_finder, depth_identity, subject_finder, subject_identity};
use clap::Args as ClapArgs;

/// What the `sam2` segmenter is told to find: clicks and a box, as shares of the picture.
#[derive(ClapArgs, Clone, Default)]
pub(crate) struct Clicks {
    /// A click on what is wanted, as shares of the photo, `X,Y`: 0,0 is the top left and 1,1 the bottom right. Repeat for
    /// more. Only for `--segmenter sam2`.
    #[arg(long = "point", value_name = "X,Y", value_parser = shares::<2>)]
    points: Vec<[f64; 2]>,
    /// A click on what must be left out, as for `--point`.
    #[arg(long = "avoid", value_name = "X,Y", value_parser = shares::<2>)]
    avoid: Vec<[f64; 2]>,
    /// A box round what is wanted, as shares of the photo, `LEFT,TOP,RIGHT,BOTTOM`. Only for `--segmenter sam2`.
    #[arg(long = "box", value_name = "L,T,R,B", value_parser = shares::<4>)]
    bounding_box: Option<[f64; 4]>,
}

impl Clicks {
    /// The prompts these say, or none when none were given.
    pub(crate) fn prompts(&self) -> Result<Option<Prompts>, Error> {
        if self.points.is_empty() && self.avoid.is_empty() && self.bounding_box.is_none() {
            return Ok(None);
        }
        let clicks =
            |list: &[[f64; 2]], positive| list.iter().map(|&[x, y]| Point { x, y, positive }).collect::<Vec<_>>();
        let points = [clicks(&self.points, true), clicks(&self.avoid, false)].concat();
        Prompts::new(points, self.bounding_box).map(Some)
    }
}

/// A clap value parser: `N` numbers separated by commas.
fn shares<const N: usize>(text: &str) -> Result<[f64; N], String> {
    let numbers: Result<Vec<f64>, _> = text.split(',').map(|part| part.trim().parse::<f64>()).collect();
    numbers
        .ok()
        .and_then(|numbers| numbers.try_into().ok())
        .ok_or_else(|| format!("expected {N} numbers separated by commas, for example 0.5,0.4; got {text:?}"))
}

/// The built-in finder of the map `name` (`subject`, or `depth`), with the `segmenter` for the subject; none for any
/// other name (a region's map is not found by a model).
pub(crate) fn found(
    name: &str,
    segmenter: Option<Segmenter>,
    prompts: Option<&Prompts>,
) -> Result<Option<Found>, Error> {
    match name {
        "subject" => {
            let options = Options::from_env(ModelKind::Subject);
            Ok(Some(Found {
                finder: Arc::from(subject_finder(segmenter, &options, DEFAULT_EDGE, prompts)?),
                identity: subject_identity(segmenter, &options, prompts)?,
            }))
        }
        "depth" => {
            let options = Options::from_env(ModelKind::Depth);
            Ok(Some(Found {
                finder: Arc::from(depth_finder(&options)?),
                identity: depth_identity(&options)?,
            }))
        }
        _ => Ok(None),
    }
}

/// A clap value parser: `vision`, `onnx` or `sam2`.
pub(crate) fn segmenter(name: &str) -> Result<Segmenter, String> {
    Segmenter::named(name).map_err(|error| error.to_string())
}
