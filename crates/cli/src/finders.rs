//! The default finders of the maps a project's recipes need.

use std::sync::Arc;

use bullterrier_image::Error;
use bullterrier_image::finding::ModelKind;
use bullterrier_image::finding::onnx::Options;
use bullterrier_image::pipeline::Found;
use bullterrier_image_finders::{Segmenter, depth_finder, depth_identity, subject_finder, subject_identity};

/// How far a subject's cut edge is pulled in and softened, px: the defaults of the subject finder.
pub(crate) const EDGE: (f64, f64) = (1.5, 1.0); // constant: the edge clean-up `subject_matte` has always applied

/// The built-in finder of the map `name` (`subject`, or `depth`), with the `segmenter` for the subject; none for any
/// other name (a region's map is not found by a model).
pub(crate) fn found(name: &str, segmenter: Option<Segmenter>) -> Result<Option<Found>, Error> {
    match name {
        "subject" => {
            let options = Options::from_env(ModelKind::Subject);
            Ok(Some(Found {
                finder: Arc::from(subject_finder(segmenter, &options, EDGE)?),
                identity: subject_identity(segmenter, &options)?,
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

/// A clap value parser: `vision` or `onnx`.
pub(crate) fn segmenter(name: &str) -> Result<Segmenter, String> {
    Segmenter::named(name).map_err(|error| error.to_string())
}
