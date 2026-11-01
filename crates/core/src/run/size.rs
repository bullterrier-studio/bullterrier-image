//! The picture a render works on: [`Options`] (the size), and the working picture, framed and sized, with the ratio of its
//! pixels to design pixels.

use std::sync::Arc;

use super::frame::Geometry;
use crate::ops::resample::Resize;
use crate::{Error, Frame, Image, Layer, Picture};

/// The longest side in px that a recipe's pixel values are written for.
const DESIGN_SIZE: f64 = 480.0; // constant: the design size of the format, Python's `DESIGN_SIZE`

/// What a render is asked for besides the recipe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Options {
    size: Option<u32>,
}

impl Options {
    /// These options with `size`, the longest side in px of the picture the render works on and returns: the photo is
    /// resampled to it, its aspect kept, and so are its maps. It wins over the recipe's own `size`; without either, the
    /// photo's own size is kept. The render is refused for a size of 0 and for one that makes a picture of more than
    /// 178,956,970 pixels (twice Pillow's decompression-bomb limit), as it is for a `frame` that does: before a pixel of
    /// either picture is made.
    ///
    /// A recipe's pixel values (`px`) are written for a longest side of 480; a render multiplies them by the working
    /// picture's longest side over 480, so the look is the same at any size.
    #[must_use]
    pub const fn with_size(mut self, size: u32) -> Self {
        self.size = Some(size);
        self
    }

    /// The size asked for, if any.
    pub(super) const fn size(self) -> Option<u32> {
        self.size
    }
}

/// The picture a render works on: the photo as given, or, when the recipe frames it or the render is asked for another
/// size, a copy that the render owns, framed and then resized. Maps follow the picture the same way.
pub(super) struct Working<'a> {
    given: Picture<'a>,
    /// The framed, resized copy; none when neither changes the photo.
    made: Option<Image>,
    framing: Option<Geometry>,
    resize: Option<Resize>,
}

impl<'a> Working<'a> {
    /// The working picture for `photo`: framed by `frame`, then at the longest side `size` (none: as framed). `subject`
    /// is the subject map of the photo as given, which a frame that [`needs_subject`](Frame::needs_subject) is given.
    ///
    /// # Errors
    /// [`Error::Recipe`] for a size of 0, or a size or frame that makes a picture of more than 178,956,970 pixels (twice
    /// Pillow's decompression-bomb limit), [`Error::MapNotFound`] for a subject a crop needs and the photo cannot give.
    pub(super) fn new(
        photo: Picture<'a>,
        frame: &Frame,
        size: Option<u32>,
        subject: Option<&Layer>,
    ) -> Result<Self, Error> {
        if size == Some(0) {
            return Err(Error::Recipe("`size` must be >= 1, got 0".to_owned()));
        }
        let framing = Geometry::plan(frame, photo.height(), photo.width(), subject)?;
        let (height, width) = framing.as_ref().map_or((photo.height(), photo.width()), |framing| {
            (framing.height(), framing.width())
        });
        // both pictures are planned, and refused if over the pixel limit, before either is made
        let resize = size
            .map(|size| Resize::to_longest_side(height, width, size))
            .transpose()?
            .flatten();
        let framed = framing.as_ref().map(|framing| framing.photo(photo.values()));
        let values = framed.as_deref().unwrap_or_else(|| photo.values());
        let made = match &resize {
            Some(resize) => Some(Image::new(resize.height(), resize.width(), &resize.apply(values, 3))?),
            None => framed
                .as_deref()
                .map(|values| Image::new(height, width, values))
                .transpose()?,
        };
        Ok(Self {
            given: photo,
            made,
            framing,
            resize,
        })
    }

    /// The picture the steps work on.
    pub(super) fn picture(&self) -> Picture<'_> {
        self.made.as_ref().map_or(self.given, Image::as_picture)
    }

    /// Working pixels per design pixel: the picture's longest side over [`DESIGN_SIZE`].
    pub(super) fn ratio(&self) -> f64 {
        let picture = self.picture();
        #[expect(clippy::cast_precision_loss, reason = "a length is far below 2^53")]
        let longest = picture.height().max(picture.width()) as f64;
        longest / DESIGN_SIZE
    }

    /// `map`, a map of the photo as given, as a map of the working picture: the map itself if the picture is the photo,
    /// else framed and resampled to the same grid.
    ///
    /// # Errors
    /// [`Error::Inputs`] only if the framing or resampling broke the rule of a map, which neither can.
    pub(super) fn follow(&self, map: Arc<Layer>) -> Result<Arc<Layer>, Error> {
        let framed = self.framing.as_ref().map(|framing| framing.map(&map)).transpose()?;
        let Some(resize) = &self.resize else {
            return Ok(framed.map_or(map, Arc::new));
        };
        let current = framed.as_ref().unwrap_or(&map);
        Ok(Arc::new(Layer::new(
            resize.height(),
            resize.width(),
            resize.apply(current.values(), 1),
        )?))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::effect::Input;
    use crate::testing::{Finds, Recorder, registry};
    use crate::{ErrorKind, Photo, Presets, Recipe, Stage, Step};

    /// The values of a `height` x `width` grey picture whose grey rises along each row, from 0 to 1.
    fn ramp(height: usize, width: usize) -> Vec<f32> {
        #[expect(clippy::cast_precision_loss, reason = "a test picture is small")]
        let across = |column: usize| column as f32 / (width - 1) as f32;
        (0..height)
            .flat_map(|_| (0..width).flat_map(move |column| [across(column); 3]))
            .collect()
    }

    /// The test registry holding `recorder`, which declares the size ratio and one field parameter, `near`.
    fn rig() -> Result<(crate::registry::Registry, Arc<Recorder>), Error> {
        let registry = registry()?;
        let recorder = Arc::new(Recorder::new("recorder", &[Input::SizeRatio], &["near"]));
        registry.register(recorder.clone())?;
        Ok((registry, recorder))
    }

    /// A step of the recorder that works from the map `map`.
    fn uses(map: &str) -> Step {
        Step::new("recorder").param("near", map)
    }

    /// `recipe` rendered on `photo` by `registry` with `options`.
    fn render(
        photo: &Photo<'_>,
        registry: &crate::registry::Registry,
        recipe: &Recipe,
        options: Options,
    ) -> Result<Image, Error> {
        photo.render_in(registry, recipe, &Presets::new(), options)
    }

    #[test]
    fn a_render_asked_for_a_size_returns_a_picture_whose_longest_side_is_it_and_keeps_the_aspect() -> Result<(), Error>
    {
        let (registry, _) = rig()?;
        let values = ramp(4, 6);
        let photo = Photo::new(Picture::new(4, 6, &values)?);
        let recipe = Recipe::new([uses("subject")]);
        let flat = Finds::flat(0.5);
        let photo = photo.with_finder("subject", Arc::new(flat));
        // (the size asked for, the picture that makes): 4 x 6 is 2:3, so 12 gives 8 x 12, 8 gives 5.33 -> 5 x 8, 2 gives
        // 1.33 -> 1 x 2 and 1 gives 0.67 -> 1 x 1 (never below a pixel); 6 is the photo's own size
        for (size, height, width) in [(12, 8, 12), (8, 5, 8), (2, 1, 2), (1, 1, 1), (6, 4, 6), (600, 400, 600)] {
            let image = render(&photo, &registry, &recipe, Options::default().with_size(size))?;
            assert_eq!((image.height(), image.width()), (height, width), "size {size}");
        }
        Ok(())
    }

    #[test]
    fn the_size_comes_from_the_options_else_the_recipe_else_the_photo() -> Result<(), Error> {
        let (registry, _) = rig()?;
        let values = ramp(4, 6);
        let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", Arc::new(Finds::flat(0.5)));
        let mut recipe = Recipe::new([uses("subject")]);
        let shape = |recipe: &Recipe, options: Options| -> Result<(usize, usize), Error> {
            let image = render(&photo, &registry, recipe, options)?;
            Ok((image.height(), image.width()))
        };
        assert_eq!(shape(&recipe, Options::default())?, (4, 6));
        recipe.size = Some(12);
        assert_eq!(shape(&recipe, Options::default())?, (8, 12));
        assert_eq!(shape(&recipe, Options::default().with_size(3))?, (2, 3));
        Ok(())
    }

    #[test]
    fn a_picture_of_the_size_asked_for_is_rendered_as_it_is() -> Result<(), Error> {
        let (registry, recorder) = rig()?;
        let values = ramp(4, 6);
        let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", Arc::new(Finds::flat(0.5)));
        let image = render(
            &photo,
            &registry,
            &Recipe::new([uses("subject")]),
            Options::default().with_size(6),
        )?;
        assert_eq!(image.into_values(), values);
        assert_eq!(recorder.received()[0].picture, values);
        Ok(())
    }

    #[test]
    fn the_steps_work_on_the_resized_picture_and_the_photo_is_left_as_it_was() -> Result<(), Error> {
        let (registry, recorder) = rig()?;
        let values = ramp(1, 4);
        let photo = Photo::new(Picture::new(1, 4, &values)?).with_finder("subject", Arc::new(Finds::flat(0.5)));
        // a grey ramp 0, 1/3, 2/3, 1 grown to 8 pixels (1 x 4 -> 2 x 8): the step is handed 16 pixels
        let image = render(
            &photo,
            &registry,
            &Recipe::new([uses("subject")]),
            Options::default().with_size(8),
        )?;
        assert_eq!((image.height(), image.width()), (2, 8));
        assert_eq!(recorder.received()[0].picture.len(), 2 * 8 * 3);
        assert_eq!(values, ramp(1, 4));
        Ok(())
    }

    #[test]
    fn the_ratio_of_design_pixels_is_the_working_pictures_longest_side_over_480() -> Result<(), Error> {
        let registry = registry()?;
        let recorder = Arc::new(Recorder::new("recorder", &[Input::SizeRatio], &[]));
        registry.register(recorder.clone())?;
        let recipe = Recipe::new([Step::new("recorder")]);
        // (the photo's height and width, the size asked for, the ratio the effect is handed): 1 x 960 is twice the design
        // size; asked for 240 it is half; a photo of 3 x 480 is the design size itself, and asked for 1200 it is 2.5
        for (height, width, size, ratio) in [
            (1, 960, None, 2.0),
            (1, 960, Some(240), 0.5),
            (3, 480, None, 1.0),
            (3, 480, Some(1200), 2.5),
            (960, 3, None, 2.0),
        ] {
            let values = vec![0.5; height * width * 3];
            let photo = Photo::new(Picture::new(height, width, &values)?);
            let options = size.map_or_else(Options::default, |size| Options::default().with_size(size));
            render(&photo, &registry, &recipe, options)?;
            let handed = recorder.received().pop().map(|received| received.size_ratio);
            assert_eq!(handed, Some(ratio), "{height} x {width} at {size:?}");
        }
        Ok(())
    }

    #[test]
    fn a_map_follows_the_photo_to_its_size_on_the_same_grid() -> Result<(), Error> {
        let (registry, recorder) = rig()?;
        // the photo's grey is the map: after resizing, the map the effect is handed is the picture it is handed. Grow
        // (4 x 6 to 8 x 12) and shrink (4 x 6 to 2 x 3), a map handed in and a map found.
        let values = ramp(4, 6);
        let layer = Layer::new(4, 6, values.chunks(3).map(|pixel| pixel[0]).collect())?;
        for (size, pixels) in [(12, 8 * 12), (3, 2 * 3)] {
            for (how, photo) in [
                (
                    "handed in",
                    Photo::new(Picture::new(4, 6, &values)?).with_map("subject", layer.clone())?,
                ),
                (
                    "found",
                    Photo::new(Picture::new(4, 6, &values)?).with_finder(
                        "subject",
                        Arc::new(Finds::new({
                            let layer = layer.clone();
                            move |_| Ok(layer.clone())
                        })),
                    ),
                ),
            ] {
                recorder.clear();
                render(
                    &photo,
                    &registry,
                    &Recipe::new([uses("subject")]),
                    Options::default().with_size(size),
                )?;
                let received = recorder.received();
                let (picture, map) = (&received[0].picture, &received[0].fields["near"]);
                assert_eq!(map.len(), pixels, "{how} at {size}");
                assert_eq!(picture.len(), pixels * 3, "{how} at {size}");
                assert!(
                    map.iter()
                        .zip(picture.chunks(3))
                        .all(|(map, pixel)| (map - pixel[0]).abs() < 1e-6),
                    "{how} at {size}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn a_finder_is_asked_with_the_photo_as_given_and_only_once_whatever_the_sizes() -> Result<(), Error> {
        let (registry, _) = rig()?;
        let values = ramp(4, 6);
        let asked = Arc::new(Mutex::new(Vec::new()));
        let finder = Arc::new(Finds::new({
            let asked = Arc::clone(&asked);
            move |picture| {
                if let Ok(mut sizes) = asked.lock() {
                    sizes.push((picture.height(), picture.width()));
                }
                Layer::new(picture.height(), picture.width(), vec![0.5; picture.pixels().len()])
            }
        }));
        let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", finder.clone());
        for size in [12, 3, 6] {
            render(
                &photo,
                &registry,
                &Recipe::new([uses("subject")]),
                Options::default().with_size(size),
            )?;
        }
        assert_eq!(finder.calls(), 1);
        assert_eq!(asked.lock().map(|sizes| sizes.clone()).ok(), Some(vec![(4, 6)]));
        Ok(())
    }

    #[test]
    fn light_and_background_are_the_working_pictures() -> Result<(), Error> {
        let (registry, recorder) = rig()?;
        // a grey photo has its own grey as its brightness (the Rec. 709 weights sum to 1)
        let values = ramp(4, 6);
        let subject = Layer::new(4, 6, vec![0.25; 24])?;
        let photo = Photo::new(Picture::new(4, 6, &values)?).with_map("subject", subject)?;
        render(
            &photo,
            &registry,
            &Recipe::new([uses("light"), uses("background")]),
            Options::default().with_size(12),
        )?;
        let received = recorder.received();
        let (picture, light) = (&received[0].picture, &received[0].fields["near"]);
        assert_eq!(light.len(), 8 * 12);
        assert!(
            light
                .iter()
                .zip(picture.chunks(3))
                .all(|(light, pixel)| (light - pixel[0]).abs() < 1e-6)
        );
        assert_eq!(received[1].fields["near"], vec![0.75; 8 * 12]);
        Ok(())
    }

    #[test]
    fn a_trace_shows_every_stage_at_the_working_size_and_ends_with_what_render_returns() -> Result<(), Error> {
        let (registry, _) = rig()?;
        let values = ramp(4, 6);
        let photo = Photo::new(Picture::new(4, 6, &values)?).with_map("subject", Layer::new(4, 6, vec![0.5; 24])?)?;
        let recipe = Recipe::new([uses("subject"), uses("subject").mask("subject")]);
        let options = Options::default().with_size(12);
        let stages = photo.trace_in(&registry, &recipe, &Presets::new(), options)?;
        let rendered = render(&photo, &registry, &recipe, options)?;
        assert_eq!(stages.len(), 4);
        for stage in &stages {
            let shape = match stage {
                Stage::Input(image) | Stage::Step { image, .. } | Stage::CutOut(image) => {
                    (image.height(), image.width())
                }
                Stage::Subject(layer) | Stage::Depth(layer) | Stage::Selection { layer, .. } => {
                    (layer.height(), layer.width())
                }
                Stage::Partition { labels, .. } => (labels.height(), labels.width()),
            };
            assert_eq!(shape, (8, 12));
        }
        assert!(matches!(stages.last(), Some(Stage::Step { image, .. }) if image == &rendered));
        Ok(())
    }

    #[test]
    fn a_size_no_picture_can_have_is_refused_at_once_naming_the_size_and_the_limit() -> Result<(), Error> {
        let (registry, recorder) = rig()?;
        let values = vec![0.5; 3];
        // a billion-pixel side asked of a one-pixel photo: refused without making a picture for it (which would take tens
        // of gigabytes)
        let photo = Photo::new(Picture::new(1, 1, &values)?).with_finder("subject", Arc::new(Finds::flat(0.5)));
        let result = render(
            &photo,
            &registry,
            &Recipe::new([uses("subject")]),
            Options::default().with_size(2_000_000_000),
        );
        assert!(
            matches!(&result, Err(error) if error.kind() == ErrorKind::Recipe
                && error.to_string().contains("`size` 2000000000")
                && error.to_string().contains("2000000000 x 2000000000")
                && error.to_string().contains("178956970")),
            "{result:?}"
        );
        assert_eq!(recorder.received().len(), 0);
        Ok(())
    }

    #[test]
    fn a_size_of_0_a_size_too_large_to_hold_is_refused_before_anything_runs() -> Result<(), Error> {
        let (registry, recorder) = rig()?;
        let values = ramp(4, 6);
        let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", Arc::new(Finds::flat(0.5)));
        let mut recipe = Recipe::new([uses("subject")]);
        let asked = |recipe: &Recipe, options: Options| render(&photo, &registry, recipe, options);
        let refused = [
            ("an option of 0", asked(&recipe, Options::default().with_size(0))),
            (
                "an option past what a picture can hold",
                asked(&recipe, Options::default().with_size(u32::MAX)),
            ),
        ];
        recipe.size = Some(0);
        let refused = refused
            .into_iter()
            .chain([("a recipe size of 0", asked(&recipe, Options::default()))]);
        for (what, result) in refused {
            assert!(
                matches!(&result, Err(error) if error.kind() == ErrorKind::Recipe && error.to_string().contains("`size`")),
                "{what}: {result:?}"
            );
        }
        assert_eq!(recorder.received().len(), 0);
        Ok(())
    }
}
