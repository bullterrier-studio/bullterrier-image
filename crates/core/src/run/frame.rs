//! What a recipe's [`Frame`] does to the pixels before the steps run: the photo and each of its maps go through the same
//! flip, turn, crop and border, so a map stays on the pixels of the photo it describes.
//!
//! The geometry is planned once from the photo's size and, for a crop around the subject, the subject map: the boxes
//! and border widths are numbers, then applied to the photo (its three channels, borders and corners in the frame's
//! `fill`) and to each map as it is first used (one channel, borders and corners 0: where the picture is not, there is no subject, the farthest
//! depth, no region). Applying plane by plane gives what stacking the maps behind the photo would, as every step is
//! per channel.

use std::borrow::Cow;

use crate::frame::{Anchor, Flip};
use crate::ops::frame::{self, Edges, Rect, Shape, Side};
use crate::ops::length::within_limit;
use crate::ops::rotate::Turn;
use crate::{Error, Frame, Layer};

/// The flip, turn, crop and border of a [`Frame`] for a photo of one size, ready to apply.
#[derive(Debug)]
pub(super) struct Geometry {
    across: bool,
    down: bool,
    turn: Option<Turn>,
    crop: Option<Rect>,
    edges: Edges,
    fill: [f32; 3],
    /// The size of what it applies to.
    from: (usize, usize),
    /// The size of what comes out.
    to: (usize, usize),
}

impl Geometry {
    /// The geometry of `frame` on a `height` x `width` picture, or none when the frame changes no pixel. `subject` is the
    /// subject map (of the picture's size, as given) that a frame which [`needs_subject`](Frame::needs_subject) is given.
    ///
    /// Only sizes are worked out here, no pixel is made: a frame whose turned or finished picture would have more pixels than
    /// a picture may have (see [`within_limit`]) is refused before anything is allocated for it.
    ///
    /// # Errors
    /// Whatever `subject` fails with, [`Error::MapNotFound`] of the subject if no pixel of it reaches one half, and
    /// [`Error::Recipe`] for a picture over the pixel limit.
    pub(super) fn plan(
        frame: &Frame,
        height: usize,
        width: usize,
        subject: Option<&Layer>,
    ) -> Result<Option<Self>, Error> {
        let turn = (frame.angle != 0.0).then(|| Turn::new(height, width, frame.angle, frame.fit, frame.resample));
        let mut geometry = Self {
            across: matches!(frame.flip, Flip::X | Flip::Both),
            down: matches!(frame.flip, Flip::Y | Flip::Both),
            turn,
            crop: None,
            edges: Edges::default(),
            fill: frame.fill.0,
            from: (height, width),
            to: turn.map_or((height, width), |turn| (turn.height(), turn.width())),
        };
        let (turned_height, turned_width) = geometry.to;
        // the turned picture is made before it is cropped, so it is held to the limit too
        within_limit("`frame`", turned_height, turned_width)?;
        geometry.crop = match (frame.anchor, frame.aspect) {
            (Anchor::Subject, aspect) => {
                // the box is found in the picture as flipped and turned, as the crop is cut from it
                let subject = subject.ok_or_else(|| {
                    Error::map_not_found(
                        "subject",
                        "a crop around the subject needs the subject map, which was not given",
                    )
                })?;
                let turned = geometry.turned(subject.values(), 1, &[0.0]);
                let around =
                    frame::subject_box(&turned, turned_height, turned_width, frame.margin).ok_or_else(|| {
                        Error::map_not_found(
                            "subject",
                            "the subject map has no pixel at one half or more: there is no subject to frame",
                        )
                    })?;
                Some(frame::crop_around(turned_height, turned_width, around, aspect))
            }
            (anchor, Some(aspect)) => Some(frame::crop_box(turned_height, turned_width, aspect, side(anchor))),
            (_, None) => None,
        };
        let (cropped_height, cropped_width) = geometry
            .crop
            .map_or((turned_height, turned_width), |rect| (rect.height, rect.width));
        geometry.edges = Edges::of_shares(
            frame.pad_top,
            frame.pad_right,
            frame.pad_bottom,
            frame.pad_left,
            cropped_height.min(cropped_width),
        );
        geometry.to = (
            cropped_height + geometry.edges.top + geometry.edges.bottom,
            cropped_width + geometry.edges.left + geometry.edges.right,
        );
        within_limit("`frame`", geometry.to.0, geometry.to.1)?;
        let changes_nothing = !(geometry.across
            || geometry.down
            || geometry.turn.is_some()
            || geometry.crop.is_some()
            || !geometry.edges.is_zero());
        Ok((!changes_nothing).then_some(geometry))
    }

    /// The number of rows of what comes out.
    pub(super) const fn height(&self) -> usize {
        self.to.0
    }

    /// The number of columns of what comes out.
    pub(super) const fn width(&self) -> usize {
        self.to.1
    }

    /// The photo's channel values (three per pixel, of the size planned for) framed: borders and the corners of a turn
    /// in the frame's `fill`.
    pub(super) fn photo(&self, values: &[f32]) -> Vec<f32> {
        self.apply(values, 3, &self.fill)
    }

    /// `map`, a map of the size planned for, framed: borders and corners 0.
    ///
    /// # Errors
    /// [`Error::Inputs`] only if the framing broke the rule of a map, which moving its values cannot.
    pub(super) fn map(&self, map: &Layer) -> Result<Layer, Error> {
        Layer::new(self.to.0, self.to.1, self.apply(map.values(), 1, &[0.0]))
    }

    /// `values` flipped and turned, the first two steps of the frame, which a crop around the subject is found in.
    fn turned<'v>(&self, values: &'v [f32], channels: usize, fill: &[f32]) -> Cow<'v, [f32]> {
        let shape = Shape {
            height: self.from.0,
            width: self.from.1,
            channels,
        };
        let mut made = Cow::Borrowed(values);
        if self.across || self.down {
            made = Cow::Owned(frame::flip(&made, shape, self.across, self.down));
        }
        if let Some(turn) = &self.turn {
            made = Cow::Owned(turn.apply(&made, channels, fill));
        }
        made
    }

    fn apply(&self, values: &[f32], channels: usize, fill: &[f32]) -> Vec<f32> {
        let mut made = self.turned(values, channels, fill);
        let (height, width) = self.turn.map_or(self.from, |turn| (turn.height(), turn.width()));
        let mut shape = Shape {
            height,
            width,
            channels,
        };
        if let Some(rect) = self.crop {
            made = Cow::Owned(frame::window(&made, shape, rect));
            (shape.height, shape.width) = (rect.height, rect.width);
        }
        if !self.edges.is_zero() {
            made = Cow::Owned(frame::pad(&made, shape, self.edges, fill));
        }
        made.into_owned()
    }
}

/// Where a crop against `anchor` sits. A crop around the subject is cut by its box, never by a side.
const fn side(anchor: Anchor) -> Side {
    match anchor {
        Anchor::Top => Side::Top,
        Anchor::Bottom => Side::Bottom,
        Anchor::Left => Side::Left,
        Anchor::Right => Side::Right,
        Anchor::Center | Anchor::Subject => Side::Center,
    }
}
