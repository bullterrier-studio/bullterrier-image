//! The cut-out after the last step: a recipe's `frame.transparent`.

use std::sync::Arc;

use super::Working;
use super::maps::Maps;
use crate::frame::Transparent;
use crate::ops::cutout;
use crate::ops::length::float;
use crate::{Error, Frame, Image, Layer};

/// How a render's finished picture gets its alpha, planned from the frame and the working picture.
pub(super) enum Cut {
    /// The inscribed circle.
    Circle,
    /// Corners rounded by this many px.
    Rounded(f64),
    /// The subject map of the working picture.
    Subject(Arc<Layer>),
}

impl Cut {
    /// The cut `frame` asks for on the picture `working`, if any; `maps` gives the subject for a `Background` cut.
    ///
    /// # Errors
    /// [`Error::MapNotFound`] if a `Background` cut has no subject map to work from.
    pub(super) fn plan(frame: &Frame, working: &Working<'_>, maps: &Maps<'_, '_>) -> Result<Option<Self>, Error> {
        Ok(match frame.transparent {
            Transparent::None => None,
            Transparent::Circle => Some(Self::Circle),
            Transparent::Rounded => {
                let picture = working.picture();
                Some(Self::Rounded(
                    frame.corner_radius * float(picture.height().min(picture.width())),
                ))
            }
            Transparent::Background => Some(Self::Subject(maps.get("subject", "`frame`, `transparent`")?)),
        })
    }

    /// `image`, the picture after the last step, with its alpha: its colour is untouched.
    pub(super) fn apply(&self, image: Image) -> Image {
        let (height, width) = (image.height(), image.width());
        let alpha = match self {
            Self::Circle => cutout::circle(height, width),
            Self::Rounded(radius) => cutout::rounded(height, width, *radius),
            Self::Subject(subject) => subject.values().to_vec(),
        };
        image.with_alpha(alpha)
    }
}
