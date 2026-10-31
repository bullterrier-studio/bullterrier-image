//! The cut-out after the last step: a recipe's `frame.transparent`.

use std::sync::Arc;

use super::Working;
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
    /// The cut `frame` asks for on the picture `working`, if any; `subject` is the subject map of the working picture
    /// that a frame which [`needs_subject`](Frame::needs_subject) is given.
    ///
    /// # Errors
    /// [`Error::MapNotFound`] if a `Background` cut is given no subject map: a frame that needs one is always given it.
    pub(super) fn plan(
        frame: &Frame,
        working: &Working<'_>,
        subject: Option<Arc<Layer>>,
    ) -> Result<Option<Self>, Error> {
        Ok(match frame.transparent {
            Transparent::None => None,
            Transparent::Circle => Some(Self::Circle),
            Transparent::Rounded => {
                let picture = working.picture();
                Some(Self::Rounded(
                    frame.corner_radius * float(picture.height().min(picture.width())),
                ))
            }
            Transparent::Background => Some(Self::Subject(subject.ok_or_else(|| {
                Error::map_not_found(
                    "subject",
                    "a cut by the subject needs the subject map, which was not given",
                )
            })?)),
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
