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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use proptest::prelude::*;

    use super::Geometry;
    use crate::effect::Input;
    use crate::frame::{Anchor, Flip};
    use crate::params::Color;
    use crate::testing::{Finds, Recorder, registry};
    use crate::{Error, ErrorKind, Frame, Layer, Options, Photo, Picture, Presets, Recipe, Stage, Step};

    /// The pixel at `row`, `column` of the test scene: a different value in every channel and place (for up to 30
    /// columns), and the place can be read back from the red channel by `place`.
    fn px(row: usize, column: usize) -> [f32; 3] {
        let value = f32::from(u16::try_from(row * 30 + column).unwrap_or(0)) / 1000.0;
        [value, value + 0.0001, value + 0.0002]
    }

    /// The (row, column) of the scene a pixel made by `px` came from.
    fn place(pixel: [f32; 3]) -> (usize, usize) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a small positive whole number"
        )]
        let number = (pixel[0] * 1000.0).round() as usize;
        (number / 30, number % 30)
    }

    /// The values of a `height` x `width` picture whose pixel at `row`, `column` is `make(row, column)`.
    fn picture_of(height: usize, width: usize, make: impl Fn(usize, usize) -> [f32; 3]) -> Vec<f32> {
        let mut values = Vec::with_capacity(height * width * 3);
        for row in 0..height {
            for column in 0..width {
                values.extend(make(row, column));
            }
        }
        values
    }

    /// `pixels` as a number.
    fn length(pixels: usize) -> f64 {
        f64::from(u32::try_from(pixels).unwrap_or(u32::MAX))
    }

    /// A map of `height` x `width` whose value at `row`, `column` is `make(row, column)`.
    fn map_of(height: usize, width: usize, make: impl Fn(usize, usize) -> f32) -> Result<Layer, Error> {
        Layer::new(
            height,
            width,
            (0..height)
                .flat_map(|row| (0..width).map(|column| (make)(row, column)).collect::<Vec<_>>())
                .collect(),
        )
    }

    /// What a render of a frame made of a scene: the picture, and the map `subject` as the effect was handed it.
    struct Framed {
        height: usize,
        width: usize,
        values: Vec<f32>,
        near: Vec<f32>,
        ratio: f64,
    }

    /// The test registry with `plain`, an effect that returns the picture as it is, and `recorder`, the same with the
    /// field parameter `near`; both declare the size ratio.
    fn rig() -> Result<(crate::registry::Registry, Arc<Recorder>, Arc<Recorder>), Error> {
        let registry = registry()?;
        let plain = Arc::new(Recorder::new("plain", &[Input::SizeRatio], &[]));
        let recorder = Arc::new(Recorder::new("recorder", &[Input::SizeRatio], &["near"]));
        registry.register(plain.clone())?;
        registry.register(recorder.clone())?;
        Ok((registry, plain, recorder))
    }

    /// `recipe` (one identity step) rendered with `frame` on the photo of `values`, `height` x `width`, that has
    /// `subject` as its map if given; the step works from the subject when there is one.
    fn run(
        height: usize,
        width: usize,
        values: &[f32],
        frame: Frame,
        subject: Option<Layer>,
        size: Option<u32>,
    ) -> Result<Framed, Error> {
        let (registry, plain, recorder) = rig()?;
        let mut photo = Photo::new(Picture::new(height, width, values)?);
        let step = if let Some(subject) = subject {
            photo = photo.with_map("subject", subject)?;
            Step::new("recorder").param("near", "subject")
        } else {
            Step::new("plain")
        };
        let mut recipe = Recipe::new([step]);
        recipe.frame = frame;
        recipe.size = size;
        let image = photo.render_in(&registry, &recipe, &Presets::new(), Options::default())?;
        let (received, ratio) = recorder
            .received()
            .pop()
            .or_else(|| plain.received().pop())
            .map_or((None, 0.0), |received| (Some(received.fields), received.size_ratio));
        Ok(Framed {
            height: image.height(),
            width: image.width(),
            values: image.into_values(),
            near: received
                .and_then(|mut fields| fields.remove("near"))
                .unwrap_or_default(),
            ratio,
        })
    }

    /// The scene of `height` x `width` pixels (`px`) framed by `frame`.
    fn framed(height: usize, width: usize, frame: Frame) -> Result<Framed, Error> {
        run(height, width, &picture_of(height, width, px), frame, None, None)
    }

    fn assert_picture(framed: &Framed, expected: &(usize, usize, Vec<f32>), row: &str) {
        assert_eq!(
            (framed.height, framed.width),
            (expected.0, expected.1),
            "{row}: the shape"
        );
        assert_eq!(framed.values, expected.2, "{row}: the pixels");
    }

    /// The `height` x `width` picture whose pixel at `row`, `column` is the scene's at `top + row`, `left + column`.
    fn window(top: usize, left: usize, height: usize, width: usize) -> (usize, usize, Vec<f32>) {
        (
            height,
            width,
            picture_of(height, width, |row, column| px(top + row, left + column)),
        )
    }

    fn with(frame: impl FnOnce(&mut Frame)) -> Frame {
        let mut made = Frame::default();
        frame(&mut made);
        made
    }

    #[test]
    fn a_flip_swaps_left_and_right_or_top_and_bottom_or_both() -> Result<(), Error> {
        // the scene is 4 x 6: x makes the pixel at column c the scene's at 5 - c, y the one at row r the scene's at 3 - r
        for (flip, expected) in [
            (Flip::X, picture_of(4, 6, |row, column| px(row, 5 - column))),
            (Flip::Y, picture_of(4, 6, |row, column| px(3 - row, column))),
            (Flip::Both, picture_of(4, 6, |row, column| px(3 - row, 5 - column))),
        ] {
            assert_picture(
                &framed(4, 6, with(|frame| frame.flip = flip))?,
                &(4, 6, expected),
                &format!("{flip:?}"),
            );
        }
        Ok(())
    }

    #[test]
    fn a_picture_flipped_twice_is_the_picture() -> Result<(), Error> {
        for flip in [Flip::X, Flip::Y, Flip::Both] {
            let once = framed(5, 7, with(|frame| frame.flip = flip))?;
            let twice = run(5, 7, &once.values, with(|frame| frame.flip = flip), None, None)?;
            assert_eq!(twice.values, picture_of(5, 7, px), "{flip:?}");
        }
        Ok(())
    }

    #[test]
    fn a_crop_is_the_largest_part_of_the_aspect_against_the_anchor_or_in_the_middle_halves_to_the_even_pixel()
    -> Result<(), Error> {
        // (height, width, aspect, anchor, the crop's top, left, height and width), worked by hand. 10 x 20 at 1 is 10 x 10
        // with 10 columns of room; a side with no room does not move. The halves: 4 x 9 at 1 has 5 columns of room, its
        // middle 2.5 goes to 2; 5 x 10 at 0.5 is 2.5 wide, which is 2; 3 x 10 at 0.5 is 1.5, which is 2; an aspect past
        // the shape keeps a pixel: 4 x 6 at 10 is 6 x 0.6 -> 1 tall, at 0.1 it is 0.4 -> 1 wide.
        let rows = [
            (10, 20, 1.0, Anchor::Center, (0, 5, 10, 10)),
            (10, 20, 1.0, Anchor::Left, (0, 0, 10, 10)),
            (10, 20, 1.0, Anchor::Right, (0, 10, 10, 10)),
            (10, 20, 1.0, Anchor::Top, (0, 5, 10, 10)),
            (10, 20, 1.0, Anchor::Bottom, (0, 5, 10, 10)),
            (20, 10, 1.0, Anchor::Center, (5, 0, 10, 10)),
            (20, 10, 1.0, Anchor::Top, (0, 0, 10, 10)),
            (20, 10, 1.0, Anchor::Bottom, (10, 0, 10, 10)),
            (20, 10, 1.0, Anchor::Left, (5, 0, 10, 10)),
            (10, 20, 2.0, Anchor::Left, (0, 0, 10, 20)),
            (4, 9, 1.0, Anchor::Center, (0, 2, 4, 4)),
            (4, 9, 1.0, Anchor::Bottom, (0, 2, 4, 4)),
            (5, 10, 0.5, Anchor::Center, (0, 4, 5, 2)),
            (3, 10, 0.5, Anchor::Center, (0, 4, 3, 2)),
            (4, 6, 10.0, Anchor::Center, (2, 0, 1, 6)),
            (4, 6, 0.1, Anchor::Center, (0, 2, 4, 1)),
        ];
        for (height, width, aspect, anchor, (top, left, high, wide)) in rows {
            let frame = with(|frame| {
                frame.aspect = Some(aspect);
                frame.anchor = anchor;
            });
            assert_picture(
                &framed(height, width, frame)?,
                &window(top, left, high, wide),
                &format!("{height} x {width} at {aspect} on {anchor:?}"),
            );
        }
        Ok(())
    }

    proptest! {
        #[test]
        fn a_crop_has_the_aspect_to_a_pixel_fills_the_photo_one_way_and_sits_against_its_anchor(
            height in 1_usize..30, width in 1_usize..30, hundredths in 10_u16..=1000, which in 0_usize..5,
        ) {
            let aspect = f64::from(hundredths) / 100.0;
            let anchor = [Anchor::Center, Anchor::Top, Anchor::Bottom, Anchor::Left, Anchor::Right][which];
            let made = framed(height, width, with(|frame| {
                frame.aspect = Some(aspect);
                frame.anchor = anchor;
            }))?;
            let (high, wide) = (made.height, made.width);
            // the crop is as wide as the photo and as tall as the aspect says to within half a pixel (a pixel at least), or
            // as tall as the photo and as wide
            let by_width = wide == width && ((length(high) - length(width) / aspect).abs() <= 0.5 || high == 1);
            let by_height = high == height && ((length(wide) - length(height) * aspect).abs() <= 0.5 || wide == 1);
            prop_assert!(by_width || by_height, "{height} x {width} at {aspect}: {high} x {wide}");
            // where it lies, read back from its first pixel; a crop that fills a side has no room on it
            let (top, left) = place([made.values[0], made.values[1], made.values[2]]);
            prop_assert_eq!(made.values.len(), high * wide * 3);
            match anchor {
                Anchor::Top => prop_assert_eq!(top, 0),
                Anchor::Bottom => prop_assert_eq!(top + high, height),
                Anchor::Left => prop_assert_eq!(left, 0),
                Anchor::Right => prop_assert_eq!(left + wide, width),
                _ => {
                    // the middle, to a pixel, on each side that has room
                    prop_assert!((2 * top + high).abs_diff(height) <= 1 && (2 * left + wide).abs_diff(width) <= 1);
                }
            }
        }
    }

    /// A map of `height` x `width` that is 1 inside the rectangle of `rows` and `columns` and 0 outside it.
    fn rectangle(
        height: usize,
        width: usize,
        rows: std::ops::Range<usize>,
        columns: std::ops::Range<usize>,
    ) -> Result<Layer, Error> {
        map_of(height, width, |row, column| {
            f32::from(u8::from(rows.contains(&row) && columns.contains(&column)))
        })
    }

    /// The scene of `height` x `width` pixels framed by `frame`, with `subject` as its subject map.
    fn framed_around(height: usize, width: usize, frame: Frame, subject: Layer) -> Result<Framed, Error> {
        run(
            height,
            width,
            &picture_of(height, width, px),
            frame,
            Some(subject),
            None,
        )
    }

    fn around(aspect: Option<f64>, margin: f64) -> Frame {
        with(|frame| {
            frame.anchor = Anchor::Subject;
            frame.aspect = aspect;
            frame.margin = margin;
        })
    }

    #[test]
    fn a_crop_around_the_subject_is_the_smallest_part_of_the_aspect_that_holds_it_and_its_margin() -> Result<(), Error>
    {
        // On a 10 x 20 scene, (the subject's rows and columns, aspect, margin, the crop's top, left, height, width), worked
        // by hand:
        // - rows 2..4, columns 8..12 is 2 x 4; at aspect 1 the crop is 4 x 4 centred on it: top 2 + (2 - 4) / 2 = 1
        // - a margin of 0.5 of the longer side (4) adds 2 all round: rows 0..6, columns 6..14 is 6 x 8, so the crop is 8 x 8
        //   (top -1 held inside at 0)
        // - without an aspect the crop is the subject's own box
        // - rows 1..9, columns 10..12 is 8 x 2: 8 x 8, centred at columns 7..15
        // - the whole scene at aspect 1 does not fit, so the largest 10 x 10 part, centred: columns 5..15
        // - rows 3..5, columns 8..11 is 2 x 3: a 3 x 3 crop has top 3 + (2 - 3) / 2 = 2.5, a half, which goes to 2
        // - rows 0..2, columns 0..3 in the corner: the top 0 - 0.5 is held inside at 0
        // - rows 8..10, columns 17..20 in the opposite corner: the top 8 - 0.5 -> 8 would run off the bottom, so 7
        // - rows 7..10, columns 17..20 with a margin of 1 (3 px all round) and no aspect: held inside the picture,
        //   rows 4..10, columns 14..20
        // - rows 2..5, columns 8..10 is 3 x 2; at 1.5 the crop is 4.5 -> 5 wide (a fraction rounds up) and 3 tall
        // - rows 2..4, columns 8..13 is 2 x 5; at 1.5 the crop is 5 wide and 3.33 -> 4 tall
        // - rows 1..9, columns 10..12 at aspect 5 would be 40 wide, more than the 20: the largest 20 x 4 part instead
        let rows = [
            ((2..4, 8..12), Some(1.0), 0.0, (1, 8, 4, 4)),
            ((2..4, 8..12), Some(1.0), 0.5, (0, 6, 8, 8)),
            ((2..4, 8..12), None, 0.0, (2, 8, 2, 4)),
            ((1..9, 10..12), Some(1.0), 0.0, (1, 7, 8, 8)),
            ((0..10, 0..20), Some(1.0), 0.0, (0, 5, 10, 10)),
            ((3..5, 8..11), Some(1.0), 0.0, (2, 8, 3, 3)),
            ((0..2, 0..3), Some(1.0), 0.0, (0, 0, 3, 3)),
            ((8..10, 17..20), Some(1.0), 0.0, (7, 17, 3, 3)),
            ((7..10, 17..20), None, 1.0, (4, 14, 6, 6)),
            ((2..5, 8..10), Some(1.5), 0.0, (2, 6, 3, 5)),
            ((2..4, 8..13), Some(1.5), 0.0, (1, 8, 4, 5)),
            ((1..9, 10..12), Some(5.0), 0.0, (3, 0, 4, 20)),
        ];
        for ((subject_rows, subject_columns), aspect, margin, (top, left, high, wide)) in rows {
            let subject = rectangle(10, 20, subject_rows.clone(), subject_columns.clone())?;
            let made = framed_around(10, 20, around(aspect, margin), subject)?;
            let row = format!("rows {subject_rows:?} columns {subject_columns:?} at {aspect:?} margin {margin}");
            assert_picture(&made, &window(top, left, high, wide), &row);
            // the map is cropped as the photo is: 1 where the subject is
            let expected: Vec<f32> = (0..high)
                .flat_map(|r| (0..wide).map(move |c| (r, c)))
                .map(|(r, c)| {
                    f32::from(u8::from(
                        subject_rows.contains(&(top + r)) && subject_columns.contains(&(left + c)),
                    ))
                })
                .collect();
            assert_eq!(made.near, expected, "{row}: the subject");
        }
        Ok(())
    }

    #[test]
    fn a_crop_around_the_subject_that_fits_keeps_all_of_it() -> Result<(), Error> {
        // a random box inside a 20 x 25 scene, at any aspect, with a margin: the crop holds the box and its margin
        for (rows, columns, aspect, margin) in [
            (3..7, 4..9, 1.5, 0.0),
            (0..20, 9..12, 0.5, 0.0),
            (5..9, 5..20, 1.0, 0.1),
            (2..3, 2..3, 3.0, 0.0),
            (6..14, 8..19, 0.7, 0.2),
        ] {
            let made = framed_around(
                20,
                25,
                around(Some(aspect), margin),
                rectangle(20, 25, rows.clone(), columns.clone())?,
            )?;
            let (top, left) = place([made.values[0], made.values[1], made.values[2]]);
            assert!(
                top <= rows.start
                    && rows.end <= top + made.height
                    && left <= columns.start
                    && columns.end <= left + made.width,
                "rows {rows:?} columns {columns:?} at {aspect}: the crop is {top}, {left}, {} x {}",
                made.height,
                made.width
            );
        }
        Ok(())
    }

    #[test]
    fn the_subject_is_found_in_the_flipped_picture() -> Result<(), Error> {
        // the subject at rows 2..4, columns 1..3 of a 10 x 20 scene lies at columns 17..19 once flipped across: the crop
        // is 2 x 2 there, and its pixel at (r, c) is the scene's at (2 + r, 19 - (17 + c))
        let frame = with(|frame| {
            frame.flip = Flip::X;
            frame.anchor = Anchor::Subject;
            frame.aspect = Some(1.0);
        });
        let made = framed_around(10, 20, frame, rectangle(10, 20, 2..4, 1..3)?)?;
        assert_picture(
            &made,
            &(2, 2, picture_of(2, 2, |row, column| px(2 + row, 2 - column))),
            "flipped",
        );
        assert_eq!(made.near, vec![1.0; 4]);
        Ok(())
    }

    #[test]
    fn a_pixel_of_a_half_is_the_subject_and_a_pixel_below_it_is_not() -> Result<(), Error> {
        let one_pixel = |level: f32| map_of(6, 8, |row, column| if (row, column) == (3, 4) { level } else { 0.0 });
        let made = framed_around(6, 8, around(None, 0.0), one_pixel(0.5)?)?;
        assert_picture(&made, &window(3, 4, 1, 1), "a half");
        for (what, map) in [
            ("just under a half", one_pixel(0.499)?),
            ("nothing", map_of(6, 8, |_, _| 0.0)?),
        ] {
            let result = framed_around(6, 8, around(None, 0.0), map);
            assert!(
                matches!(&result, Err(error) if error.kind() == ErrorKind::SubjectNotFound),
                "{what}: {:?}",
                result.map(|made| made.values.len())
            );
        }
        Ok(())
    }

    #[test]
    fn a_crop_around_a_subject_the_photo_has_neither_a_map_nor_a_finder_for_is_the_subject_not_found() {
        let result = framed(4, 6, around(Some(1.0), 0.0));
        assert!(
            matches!(&result, Err(error) if error.kind() == ErrorKind::SubjectNotFound
                && error.to_string().contains("subject")),
            "{:?}",
            result.map(|made| made.values.len())
        );
    }

    #[test]
    fn a_finder_is_asked_with_the_photo_as_given_even_when_it_is_flipped_and_cropped() -> Result<(), Error> {
        let (registry, _, _) = rig()?;
        let values = picture_of(10, 20, px);
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let finder = Arc::new(Finds::new({
            let seen = Arc::clone(&seen);
            move |picture| {
                if let Ok(mut seen) = seen.lock() {
                    seen.push(picture.values().to_vec());
                }
                map_of(picture.height(), picture.width(), |row, column| {
                    f32::from(u8::from((2..4).contains(&row) && (1..3).contains(&column)))
                })
            }
        }));
        let photo = Photo::new(Picture::new(10, 20, &values)?).with_finder("subject", finder.clone());
        let mut recipe = Recipe::new([Step::new("plain")]);
        recipe.frame = with(|frame| {
            frame.flip = Flip::X;
            frame.anchor = Anchor::Subject;
            frame.aspect = Some(1.0);
        });
        let image = photo.render_in(&registry, &recipe, &Presets::new(), Options::default())?;
        assert_eq!(seen.lock().map(|seen| seen.clone()).ok(), Some(vec![values]));
        assert_eq!(finder.calls(), 1);
        assert_eq!(
            image.into_values(),
            picture_of(2, 2, |row, column| px(2 + row, 2 - column))
        );
        Ok(())
    }

    #[test]
    fn a_border_is_added_in_the_fill_a_share_of_the_shorter_side_rounded_halves_to_the_even_pixel() -> Result<(), Error>
    {
        // the scene is 4 x 6, its shorter side 4: top 0.5 -> 2, right 0.375 -> 1.5 -> 2, bottom 0.125 -> 0.5 -> 0,
        // left 0.625 -> 2.5 -> 2. So 6 x 10, the scene at row 2, column 2
        let fill = [1.0, 0.5, 0.25];
        let frame = with(|frame| {
            frame.pad_top = 0.5;
            frame.pad_right = 0.375;
            frame.pad_bottom = 0.125;
            frame.pad_left = 0.625;
            frame.fill = Color(fill);
        });
        let subject = map_of(4, 6, |_, _| 0.75)?;
        let made = run(4, 6, &picture_of(4, 6, px), frame, Some(subject), None)?;
        let inside = |row: usize, column: usize| (2..6).contains(&row) && (2..8).contains(&column);
        let expected = picture_of(6, 10, |row, column| {
            if inside(row, column) {
                px(row - 2, column - 2)
            } else {
                fill
            }
        });
        assert_picture(&made, &(6, 10, expected), "the picture");
        // the map has 0 in the border
        let near: Vec<f32> = (0..6)
            .flat_map(|row| (0..10).map(move |column| if inside(row, column) { 0.75 } else { 0.0 }))
            .collect();
        assert_eq!(made.near, near);
        Ok(())
    }

    /// `numerator` / 8 of `short`, rounded to the nearest whole number and halves to the even one, by whole-number
    /// arithmetic.
    fn eighths(numerator: usize, short: usize) -> usize {
        let (whole, rest) = (numerator * short / 8, numerator * short % 8);
        whole + usize::from(rest > 4 || (rest == 4 && whole % 2 == 1))
    }

    proptest! {
        #[test]
        fn a_border_adds_exactly_its_share_of_the_shorter_side_to_each_edge_and_leaves_the_photo_in_it(
            height in 1_usize..30, width in 1_usize..30, shares in prop::array::uniform4(0_usize..=16),
        ) {
            let [top, right, bottom, left] = shares;
            let made = framed(height, width, with(|frame| {
                let eighth = |share: usize| f64::from(u8::try_from(share).unwrap_or(0)) / 8.0;
                frame.pad_top = eighth(top);
                frame.pad_right = eighth(right);
                frame.pad_bottom = eighth(bottom);
                frame.pad_left = eighth(left);
                frame.fill = Color([0.25, 0.5, 0.75]);
            }))?;
            let short = height.min(width);
            let pads = (eighths(top, short), eighths(right, short), eighths(bottom, short), eighths(left, short));
            prop_assert_eq!((made.height, made.width), (height + pads.0 + pads.2, width + pads.1 + pads.3));
            let expected = picture_of(made.height, made.width, |row, column| {
                let (r, c) = (row.wrapping_sub(pads.0), column.wrapping_sub(pads.3));
                if r < height && c < width { px(r, c) } else { [0.25, 0.5, 0.75] }
            });
            prop_assert_eq!(made.values, expected);
        }
    }

    #[test]
    fn a_border_is_a_share_of_the_shorter_side_of_the_cropped_picture() -> Result<(), Error> {
        // 20 x 10 at aspect 2 is cropped to 5 x 10 (top 8): a border of 0.4 is 0.4 x 5 = 2 rows (of the scene's 10 it
        // would be 4)
        let frame = with(|frame| {
            frame.aspect = Some(2.0);
            frame.pad_top = 0.4;
            frame.fill = Color([0.5, 0.5, 0.5]);
        });
        let expected = picture_of(
            7,
            10,
            |row, column| if row < 2 { [0.5; 3] } else { px(8 + row - 2, column) },
        );
        assert_picture(&framed(20, 10, frame)?, &(7, 10, expected), "crop then border");
        Ok(())
    }

    #[test]
    fn a_flip_comes_before_a_crop_which_is_left_of_the_flipped_picture() -> Result<(), Error> {
        // 4 x 9 at 1, against the left: the first 4 columns of the flipped scene, whose column c is the scene's 8 - c
        let frame = with(|frame| {
            frame.flip = Flip::X;
            frame.aspect = Some(1.0);
            frame.anchor = Anchor::Left;
        });
        let expected = picture_of(4, 4, |row, column| px(row, 8 - column));
        assert_picture(&framed(4, 9, frame)?, &(4, 4, expected), "flip then crop");
        Ok(())
    }

    #[test]
    fn the_size_is_applied_to_the_framed_picture_and_the_ratio_is_that_of_the_result() -> Result<(), Error> {
        // 10 x 20 cropped to 10 x 10, then to 5 x 5 (without the crop it would be 2 x 5, 2.5 going to 2); 5 / 480 design
        // pixels to a pixel
        let frame = with(|frame| frame.aspect = Some(1.0));
        let made = run(10, 20, &picture_of(10, 20, px), frame.clone(), None, Some(5))?;
        assert_eq!((made.height, made.width), (5, 5));
        // and the pixels are the framed picture's, resized: the same as handing that picture over with no frame
        let crop = framed(10, 20, frame)?;
        let resized = run(10, 10, &crop.values, Frame::default(), None, Some(5))?;
        assert_eq!(made.values, resized.values);
        assert!((made.ratio - 5.0 / 480.0).abs() < 1e-12, "{}", made.ratio);
        Ok(())
    }

    #[test]
    fn a_frame_that_changes_no_pixel_leaves_the_picture_as_it_is() -> Result<(), Error> {
        // a margin without a subject anchor, a fit, a resample, a corner radius, an aspect equal to the photo's: nothing
        let frame = with(|frame| {
            frame.margin = 0.5;
            frame.fit = crate::frame::Fit::Expand;
            frame.resample = crate::frame::Resample::Nearest;
            frame.corner_radius = 0.1;
            frame.aspect = Some(1.5);
        });
        assert_picture(&framed(4, 6, frame)?, &(4, 6, picture_of(4, 6, px)), "unchanged");
        Ok(())
    }

    #[test]
    fn a_frame_that_makes_a_picture_over_the_pixel_limit_is_refused_when_planned_before_any_pixel_is_made() {
        // planning takes sizes, not pictures, so these photos (millions of pixels) are never made. The limit is 67,108,864
        // pixels (8192 x 8192). Borders of 2 on every side make a picture 5 times the shorter side across: 1638 is
        // 8,190 across (67,076,100 pixels, within), 1639 is 8,195 (67,158,025, over).
        let all_sides = |share: f64| {
            with(|frame| {
                (frame.pad_top, frame.pad_right, frame.pad_bottom, frame.pad_left) = (share, share, share, share);
            })
        };
        // a 1 x 12,000 photo turned by 45 degrees and expanded is 8,486 across, a bounding square of 72 million pixels
        // (and 11,000 columns 7,779 across, 60.5 million, within); a crop to a square after the turn leaves it small
        let turned = |angle: f64, aspect: Option<f64>| {
            with(|frame| {
                frame.angle = angle;
                frame.fit = crate::frame::Fit::Expand;
                frame.aspect = aspect;
            })
        };
        for (height, width, frame, over, row) in [
            (1638, 1638, all_sides(2.0), None, "borders just within the limit"),
            (1639, 1639, all_sides(2.0), Some("8195 x 8195"), "borders just over it"),
            (1, 11_000, turned(45.0, None), None, "a turn within the limit"),
            (1, 12_000, turned(45.0, None), Some("8486 x 8486"), "a turn over it"),
            (
                1,
                24_000,
                turned(30.0, None),
                Some("20786 x 12001"),
                "a turn over it, wider than tall",
            ),
            (
                1,
                24_000,
                turned(30.0, Some(10.0)),
                Some("20786 x 12001"),
                "a turn over it that a crop then makes small",
            ),
        ] {
            let planned = Geometry::plan(&frame, height, width, None);
            match over {
                None => assert!(planned.is_ok_and(|geometry| geometry.is_some()), "{row}"),
                Some(shape) => assert!(
                    matches!(&planned, Err(Error::Recipe(said)) if said.contains("`frame`")
                        && said.contains(shape) && said.contains("67108864")),
                    "{row}: {planned:?}"
                ),
            }
        }
    }

    #[test]
    fn a_frame_over_the_pixel_limit_is_refused_through_a_render() {
        let result = framed(
            1,
            20_000,
            with(|frame| {
                frame.angle = 45.0;
                frame.fit = crate::frame::Fit::Expand;
            }),
        );
        assert!(
            matches!(&result, Err(error) if error.kind() == ErrorKind::Recipe && error.to_string().contains("`frame`")),
            "{:?}",
            result.map(|made| (made.height, made.width))
        );
    }

    proptest! {
        #[test]
        fn a_map_follows_the_photo_a_one_hot_map_lands_where_a_one_hot_photo_does(
            height in 1_usize..20, width in 1_usize..20, row in 0_usize..20, column in 0_usize..20,
            flip in 0_usize..4, cropped in any::<bool>(), hundredths in 20_u16..=500, which in 0_usize..5,
            shares in prop::array::uniform4(0_usize..=4),
        ) {
            let (row, column) = (row % height, column % width);
            let at = |r: usize, c: usize| (r, c) == (row, column);
            let values = picture_of(height, width, |r, c| [f32::from(u8::from(at(r, c))); 3]);
            let subject = map_of(height, width, |r, c| f32::from(u8::from(at(r, c))))?;
            let frame = with(|frame| {
                frame.flip = [Flip::None, Flip::X, Flip::Y, Flip::Both][flip];
                if cropped {
                    frame.aspect = Some(f64::from(hundredths) / 100.0);
                    frame.anchor = [Anchor::Center, Anchor::Top, Anchor::Bottom, Anchor::Left, Anchor::Right][which];
                }
                let quarter = |share: usize| f64::from(u8::try_from(share).unwrap_or(0)) / 4.0;
                frame.pad_top = quarter(shares[0]);
                frame.pad_right = quarter(shares[1]);
                frame.pad_bottom = quarter(shares[2]);
                frame.pad_left = quarter(shares[3]);
            });
            let made = run(height, width, &values, frame, Some(subject), None)?;
            let hot_photo: Vec<usize> = made.values.chunks(3).enumerate().filter(|(_, pixel)| pixel[0] > 0.5).map(|(i, _)| i).collect();
            let hot_map: Vec<usize> = made.near.iter().enumerate().filter(|(_, value)| **value > 0.5).map(|(i, _)| i).collect();
            prop_assert_eq!(hot_photo, hot_map);
        }
    }

    #[test]
    fn the_photo_and_the_maps_are_left_as_they_were_and_a_render_repeats() -> Result<(), Error> {
        let values = picture_of(10, 20, px);
        let subject = rectangle(10, 20, 2..4, 8..12)?;
        let frame = with(|frame| {
            frame.flip = Flip::Both;
            frame.anchor = Anchor::Subject;
            frame.aspect = Some(1.0);
            frame.pad_left = 0.5;
        });
        let first = run(10, 20, &values, frame.clone(), Some(subject.clone()), None)?;
        let second = run(10, 20, &values, frame, Some(subject.clone()), None)?;
        assert_eq!(first.values, second.values);
        assert_eq!(values, picture_of(10, 20, px));
        assert_eq!(subject, rectangle(10, 20, 2..4, 8..12)?);
        Ok(())
    }

    #[test]
    fn a_trace_starts_with_the_framed_picture_and_the_framed_subject() -> Result<(), Error> {
        let (registry, _, _) = rig()?;
        let values = picture_of(10, 20, px);
        let photo = Photo::new(Picture::new(10, 20, &values)?).with_map("subject", rectangle(10, 20, 2..4, 8..12)?)?;
        let mut recipe = Recipe::new([Step::new("plain")]);
        recipe.frame = around(Some(1.0), 0.0);
        let stages = photo.trace_in(&registry, &recipe, &Presets::new(), Options::default())?;
        let expected = window(1, 8, 4, 4);
        assert!(
            matches!(&stages[0], Stage::Input(image) if (image.height(), image.width(), image.clone().into_values()) == expected),
            "{:?}",
            stages.first()
        );
        assert!(
            matches!(&stages[1], Stage::Subject(layer) if (layer.height(), layer.width()) == (4, 4)
                && layer.values().iter().filter(|value| **value > 0.5).count() == 2 * 4),
            "{:?}",
            stages.get(1)
        );
        Ok(())
    }
}
