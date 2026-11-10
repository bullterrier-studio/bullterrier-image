//! The finders of bullterrier-image that need more than the core: Apple Vision, and the choice of a segmenter by what
//! the platform has. Every other part of finding (the models, ONNX Runtime, the download) is the core's.

mod vision;

use std::ffi::OsStr;

use bullterrier_image::finding::onnx::{self, Options};
use bullterrier_image::finding::{DEPTH_ANYTHING_V2_SMALL, IS_NET, Model, Tensor};
use bullterrier_image::{Error, Finder, Picture};

pub use vision::Vision;

/// What finds the subject.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Segmenter {
    /// Apple Vision's subject segmentation: macOS 14 and later, no download, a cleaner cut.
    Vision,
    /// IS-Net run by ONNX Runtime: the same on every platform, the model is downloaded once.
    Onnx,
}

/// The name of the variable that picks a segmenter.
const VARIABLE: &str = "BT_IMAGE_SEGMENTER";

impl Segmenter {
    /// The segmenter this platform has, chosen once by what it supports, never by trying one and falling back: Apple
    /// Vision on macOS 14 and later, the portable model everywhere else.
    #[must_use]
    pub fn default_here() -> Self {
        if vision::supported() { Self::Vision } else { Self::Onnx }
    }

    /// The segmenter called `name` (`vision` or `onnx`).
    ///
    /// # Errors
    /// [`Error::BackendUnavailable`] naming the choices for any other name.
    pub fn named(name: &str) -> Result<Self, Error> {
        match name {
            "vision" => Ok(Self::Vision),
            "onnx" => Ok(Self::Onnx),
            other => Err(Error::BackendUnavailable(format!(
                "unknown segmenter {other:?}: choose one of vision, onnx"
            ))),
        }
    }

    /// The segmenter `BT_IMAGE_SEGMENTER` names, or none when it is unset or empty.
    ///
    /// # Errors
    /// [`Error::BackendUnavailable`] if it names no segmenter, saying to set one of the two or unset it.
    pub fn from_env() -> Result<Option<Self>, Error> {
        Self::from_value(std::env::var_os(VARIABLE).as_deref())
    }

    fn from_value(value: Option<&OsStr>) -> Result<Option<Self>, Error> {
        let Some(value) = value.map(|value| value.to_string_lossy().trim().to_lowercase()) else {
            return Ok(None);
        };
        if value.is_empty() {
            return Ok(None);
        }
        Self::named(&value).map(Some).map_err(|_| {
            Error::BackendUnavailable(format!(
                "{VARIABLE}={value:?} is not a segmenter: set one of vision, onnx, or unset it"
            ))
        })
    }
}

/// The subject finder: `segmenter`, else the one `BT_IMAGE_SEGMENTER` names, else [`Segmenter::default_here`]. The
/// portable model is found as `onnx` says. A subject's edge is cleaned by `shrink` and `feather` px
/// ([`refine_edge`](bullterrier_image::refine_edge)): 1.5 and 1 are the defaults.
///
/// # Errors
/// [`Error::BackendUnavailable`] naming the fix: an unknown segmenter name, Apple Vision asked for where it cannot run
/// (not macOS, or macOS before 14), or whatever the portable model needs (see [`onnx::finder`]).
pub fn subject_finder(
    segmenter: Option<Segmenter>,
    onnx: &Options,
    (shrink, feather): (f64, f64),
) -> Result<Box<dyn Finder>, Error> {
    let segmenter = match segmenter {
        Some(chosen) => chosen,
        None => Segmenter::from_env()?.unwrap_or_else(Segmenter::default_here),
    };
    match segmenter {
        Segmenter::Vision => Ok(Box::new(Vision::new()?.with_edge(shrink, feather))),
        Segmenter::Onnx => Ok(Box::new(onnx::finder(&IS_NET, onnx)?.with_edge(shrink, feather))),
    }
}

/// The identity of the subject finder [`subject_finder`] would make: what its answers depend on, so a cache of them
/// is kept exactly as long as they could not change. For the portable model the SHA-256 of the model (the catalogue's
/// when the standard model is meant, so it is not downloaded to find out; else of the local file the options name, so the
/// same model at another path is the same finder); for Apple Vision the macOS version, whose release brings the model.
///
/// # Errors
/// As [`subject_finder`] for the choice of the segmenter and for Vision; [`Error::BackendUnavailable`] naming a local
/// model file that is not a file.
pub fn subject_identity(segmenter: Option<Segmenter>, onnx: &Options) -> Result<String, Error> {
    let segmenter = match segmenter {
        Some(chosen) => chosen,
        None => Segmenter::from_env()?.unwrap_or_else(Segmenter::default_here),
    };
    match segmenter {
        Segmenter::Vision => {
            Vision::new()?;
            Ok(format!("vision:macOS {}", Vision::macos_version()))
        }
        Segmenter::Onnx => model_identity(&IS_NET, onnx),
    }
}

/// The identity of the depth finder [`depth_finder`] would make: `onnx:sha256 ` and the SHA-256 of the model, as for the
/// portable subject model.
///
/// # Errors
/// [`Error::BackendUnavailable`] naming a local model file that is not a file.
pub fn depth_identity(onnx: &Options) -> Result<String, Error> {
    model_identity(&DEPTH_ANYTHING_V2_SMALL, onnx)
}

fn model_identity(model: &Model, onnx: &Options) -> Result<String, Error> {
    let digest = match onnx.model_file() {
        None => model.sha256.to_owned(),
        Some(file) if file.is_file() => onnx::sha256_of(file)?,
        Some(file) => {
            return Err(Error::BackendUnavailable(format!(
                "the model file {} is not a file (it was named for the {} model)",
                file.display(),
                model.name
            )));
        }
    };
    Ok(format!("onnx:sha256 {digest}"))
}

/// The relative inverse depth of `picture` on the depth model's own scale, stretched to the photo's size: larger is
/// nearer. See [`ModelFinder::inverse_depth`](bullterrier_image::finding::ModelFinder::inverse_depth).
///
/// # Errors
/// As [`depth_finder`].
pub fn inverse_depth(picture: &Picture<'_>, onnx: &Options) -> Result<Tensor, Error> {
    onnx::finder(&DEPTH_ANYTHING_V2_SMALL, onnx)?.inverse_depth(picture)
}

/// The depth finder: Depth Anything V2, found as `onnx` says.
///
/// # Errors
/// As [`onnx::finder`].
pub fn depth_finder(onnx: &Options) -> Result<Box<dyn Finder>, Error> {
    Ok(Box::new(onnx::finder(&DEPTH_ANYTHING_V2_SMALL, onnx)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_a_segmenter_or_the_refusal_names_the_choices() {
        assert_eq!(Segmenter::named("vision").ok(), Some(Segmenter::Vision));
        assert_eq!(Segmenter::named("onnx").ok(), Some(Segmenter::Onnx));
        assert!(matches!(Segmenter::named("coreml"), Err(Error::BackendUnavailable(m)) if m.contains("vision, onnx")));
    }

    #[test]
    fn the_variable_is_read_trimmed_and_lowercase_and_empty_means_unset() {
        let read = |value: Option<&str>| Segmenter::from_value(value.map(OsStr::new));
        assert_eq!(read(None).ok(), Some(None));
        assert_eq!(read(Some("  ")).ok(), Some(None));
        assert_eq!(read(Some(" ONNX ")).ok(), Some(Some(Segmenter::Onnx)));
        assert!(
            matches!(read(Some("coreml")), Err(Error::BackendUnavailable(m)) if m.contains("BT_IMAGE_SEGMENTER") && m.contains("unset"))
        );
    }
}
