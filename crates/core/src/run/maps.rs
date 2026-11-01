//! The maps of a photo: handed in ([`Photo::with_map`]), found by a [`Finder`], or made from the picture.
//!
//! A step names a map by a name: `subject`, `depth`, `background`, `light` or a region the recipe declares. This module
//! is the one place a name becomes a [`Layer`] and the one place a map is held to the rules of a map of this photo: the
//! picture's size (finite and within 0..=1 a [`Layer`] is by construction).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use super::{Photo, Working};
use crate::ops::color::luminance;
use crate::{Error, Layer, Picture, Recipe};

/// Finds a map in a picture: the subject, the depth, or a region a recipe declares as given.
///
/// ```text
/// struct Bright;
/// impl Finder for Bright {
///     fn find(&self, picture: &Picture<'_>) -> Result<Layer, Error> {
///         let red = picture.pixels().iter().map(|pixel| pixel[0]).collect();
///         Layer::new(picture.height(), picture.width(), red)
///     }
/// }
/// let photo = Photo::new(picture).with_finder("star", Arc::new(Bright));
/// ```
///
/// A photo asks its finder for a map at most once, with the picture as it was given, and only when a recipe rendered
/// on it uses the map and none was handed in (see [`Photo::with_finder`]). The map must be of the picture's size.
///
/// The finder of `subject` is also asked, whether or not a `subject` map was handed in, by a recipe that declares a
/// `subject` region with `per`: once for each piece of the partition, with the crop of the working picture round the
/// piece, and each answer must be of the crop's size. Returning [`Error::map_not_found`] for `subject` there means the
/// piece holds no subject. When
/// there is nothing to find, return [`Error::map_not_found`] with the map's name, so the package raises the exception
/// of that map (`SubjectNotFoundError` for `subject`).
pub trait Finder: Send + Sync {
    /// The map of `picture`.
    ///
    /// # Errors
    /// Why there is none; the render fails with this error as it is.
    fn find(&self, picture: &Picture<'_>) -> Result<Layer, Error>;
}

/// A finder, and the answer it gave the photo: kept, so that the finder is asked once. Asking is serialised by the lock,
/// so threads sharing the photo cannot both ask; an error is not kept, the next render asks again.
pub(super) struct Found {
    finder: Arc<dyn Finder>,
    answer: Mutex<Option<Arc<Layer>>>,
}

impl Found {
    pub(super) fn new(finder: Arc<dyn Finder>) -> Self {
        Self {
            finder,
            answer: Mutex::new(None),
        }
    }

    /// The finder itself, which a region made from the subject of each piece of a partition asks about crops of the
    /// picture: it is not the photo's one answer, which is for the whole picture.
    pub(super) fn finder(&self) -> Arc<dyn Finder> {
        Arc::clone(&self.finder)
    }

    /// The map `name` of `picture`: the kept answer, else the finder's, which must fit the picture.
    fn answer(&self, name: &str, picture: &Picture<'_>) -> Result<Arc<Layer>, Error> {
        let mut kept = self.answer.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(layer) = kept.as_ref() {
            return Ok(Arc::clone(layer));
        }
        let layer = Arc::new(fits(name, self.finder.find(picture)?, picture)?);
        *kept = Some(Arc::clone(&layer));
        Ok(layer)
    }
}

/// `layer`, the map `name`, once it is known to be of the picture's size: the one place that is checked, for a map
/// handed in and a finder's answer alike.
pub(super) fn fits(name: &str, layer: Layer, picture: &Picture<'_>) -> Result<Layer, Error> {
    let (height, width) = (picture.height(), picture.width());
    if (layer.height(), layer.width()) == (height, width) {
        Ok(layer)
    } else {
        Err(Error::map_not_found(
            name,
            format!(
                "the map '{name}' is {} x {} pixels, but the picture is {height} x {width}",
                layer.height(),
                layer.width()
            ),
        ))
    }
}

/// The maps one render of a recipe on a photo works from, by name. What is made from the picture (`light`, `background`)
/// is made once per render and not kept beyond it.
pub(super) struct Maps<'r, 'a> {
    photo: &'r Photo<'a>,
    recipe: &'r Recipe,
    /// The picture the render works on, whose size every map is given at.
    working: &'r Working<'a>,
    /// The names the photo can be given a map under (see [`Recipe::known_maps`]), worked out once.
    known: HashSet<&'r str>,
    resolved: RefCell<BTreeMap<String, Arc<Layer>>>,
}

impl<'r, 'a> Maps<'r, 'a> {
    /// The maps of `photo` for `recipe`.
    ///
    /// # Errors
    /// [`Error::Recipe`] if the photo was given a map or a finder under a name that is no map of the recipe: `subject`,
    /// `depth`, or a region of kind `given`. Names the maps there are.
    pub(super) fn new(photo: &'r Photo<'a>, recipe: &'r Recipe, working: &'r Working<'a>) -> Result<Self, Error> {
        let listed = recipe.known_maps();
        let known: HashSet<&str> = listed.iter().copied().collect();
        let handed = photo.maps.keys().map(|name| ("map", name));
        let finders = photo.finders.keys().map(|name| ("finder", name));
        if let Some((what, name)) = handed.chain(finders).find(|(_, name)| !known.contains(name.as_str())) {
            return Err(Error::Recipe(format!(
                "the photo has a {what} '{name}', which is no map of the recipe; its maps: {} (`subject` and `depth` are \
                 built in; any other is a region with `kind = \"given\"`)",
                listed.join(", ")
            )));
        }
        Ok(Self {
            photo,
            recipe,
            working,
            known,
            resolved: RefCell::new(BTreeMap::new()),
        })
    }

    /// The map called `name`, which the step at `place` (as `step 2, mask`) works from: handed in, else found, else made
    /// from the picture (`background` is `1 - subject`, `light` the Rec. 709 brightness of the working picture: framed
    /// and sized, before any step).
    ///
    /// # Errors
    /// [`Error::MapNotFound`] if the photo has neither the map nor a finder for it, or the finder's answer does not fit
    /// (the error of the map's kind, see [`Error::kind`]), whatever a finder fails with, [`Error::InvalidImage`] for the
    /// `light` of a picture that is no picture.
    pub(super) fn get(&self, name: &str, place: &str) -> Result<Arc<Layer>, Error> {
        if let Some(layer) = self.resolved(name) {
            return Ok(layer);
        }
        let layer = match name {
            "background" => {
                let subject = self.get("subject", place)?;
                let inverse = subject.values().iter().map(|value| 1.0 - value).collect();
                Arc::new(Layer::new(subject.height(), subject.width(), inverse)?)
            }
            "light" => Arc::new(light(&self.working.picture())?),
            _ => self.working.follow(stored(self.photo, self.recipe, name, place)?)?,
        };
        self.resolved.borrow_mut().insert(name.to_owned(), Arc::clone(&layer));
        Ok(layer)
    }

    /// Whether the photo gives the map `name` (handed in or found) or it is made from the picture: `subject`, `depth`,
    /// `background`, `light` and the `Given` regions, the names [`get`](Self::get) answers.
    pub(super) fn gives(&self, name: &str) -> bool {
        matches!(name, "background" | "light") || self.known.contains(name)
    }

    /// The map `name` if this render has made it.
    fn resolved(&self, name: &str) -> Option<Arc<Layer>> {
        self.resolved.borrow().get(name).cloned()
    }
}

/// The map `name` of `recipe` that `photo` was handed or finds, as the photo's picture has it (neither framed nor sized).
/// `place` (as `step 2, mask`) says where the recipe uses it.
///
/// # Errors
/// [`Error::MapNotFound`] if the photo has neither the map nor a finder for it, or the finder's answer does not fit.
pub(super) fn stored(photo: &Photo<'_>, recipe: &Recipe, name: &str, place: &str) -> Result<Arc<Layer>, Error> {
    if let Some(layer) = photo.maps.get(name) {
        return Ok(Arc::clone(layer));
    }
    match photo.finders.get(name) {
        Some(found) => found.answer(name, &photo.picture),
        None => Err(missing(recipe, name, place)),
    }
}

/// The error for the map `name` of `recipe`, needed at `place` and neither handed in nor to be found.
fn missing(recipe: &Recipe, name: &str, place: &str) -> Error {
    let who = recipe
        .name
        .as_ref()
        .map_or_else(|| "the recipe".to_owned(), |name| format!("recipe '{name}'"));
    let meaning = match name {
        "subject" => "the subject's alpha, white = subject",
        "depth" => "how near, 1 = nearest",
        _ => "a region of kind `given`",
    };
    Error::map_not_found(
        name,
        format!(
            "{who} needs the map '{name}' ({meaning}) at {place}: hand it to the photo with \
             `Photo::with_map(\"{name}\", ..)` or give the photo a finder with `Photo::with_finder(\"{name}\", ..)`"
        ),
    )
}

/// The brightness of every pixel of `picture` (Rec. 709 weights).
///
/// # Errors
/// [`Error::InvalidImage`] if the brightness of a pixel is not finite and within 0..=1, which a picture whose channels are
/// is never: a picture that is no picture has no brightness.
fn light(picture: &Picture<'_>) -> Result<Layer, Error> {
    let values = picture.pixels().iter().map(|&pixel| luminance(pixel)).collect();
    Layer::new(picture.height(), picture.width(), values)
        .map_err(|why| Error::InvalidImage(format!("the picture has no `light` map: {why}")))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::region::{Given, Path};

    use crate::registry::Registry;
    use crate::testing::{Finds, Paint, Recorder, registry};
    use crate::{Error, ErrorKind, Image, Layer, Options, Photo, Picture, Presets, Recipe, Step};

    /// The values of a 4 x 6 picture, a different one in every channel (up to 71 / 80).
    fn values() -> Vec<f32> {
        (0_u8..72).map(|index| f32::from(index) / 80.0).collect()
    }

    /// A map of the 4 x 6 picture whose every value is `level`.
    fn flat(level: f32) -> Result<Layer, Error> {
        Layer::new(4, 6, vec![level; 24])
    }

    /// A registry holding `paint` and a `recorder` with the field parameter `near`, which logs the map it receives.
    struct Rig {
        registry: Registry,
        recorder: Arc<Recorder>,
    }

    fn rig() -> Result<Rig, Error> {
        let registry = registry()?;
        registry.register(Arc::new(Paint { name: "paint" }))?;
        let recorder = Arc::new(Recorder::new("recorder", &[], &["near"]));
        registry.register(recorder.clone())?;
        Ok(Rig { registry, recorder })
    }

    impl Rig {
        /// The maps the recorder received as its `near`, in order.
        fn seen(&self) -> Vec<Vec<f32>> {
            self.recorder
                .received()
                .into_iter()
                .map(|received| received.fields["near"].clone())
                .collect()
        }

        fn render(&self, photo: &Photo<'_>, recipe: &Recipe) -> Result<Image, Error> {
            photo.render_in(&self.registry, recipe, &Presets::new(), Options::default())
        }
    }

    /// A step of `paint`, which needs no map.
    fn paint() -> Step {
        Step::new("paint").param("level", 0.5)
    }

    /// A step of the recorder that works from the map `map`.
    fn uses(map: &str) -> Step {
        Step::new("recorder").param("near", map)
    }

    /// `steps` as a recipe that declares a `given` region for each of `regions`.
    fn recipe(steps: impl IntoIterator<Item = Step>, regions: &[&str]) -> Recipe {
        let mut recipe = Recipe::new(steps);
        for name in regions {
            recipe.regions.insert(*name, Given {});
        }
        recipe
    }

    /// `result` is the error of `kind` whose message contains every one of `said`.
    fn assert_refused<T>(result: &Result<T, Error>, kind: ErrorKind, said: &[&str], row: &str) {
        let shown = result.as_ref().map(|_| ());
        assert!(
            result
                .as_ref()
                .err()
                .is_some_and(|error| error.kind() == kind && said.iter().all(|word| error.to_string().contains(word))),
            "{row}: {shown:?}"
        );
    }

    /// Every map name a recipe can be given, with the kind of error its problems are.
    const NAMES: [(&str, ErrorKind); 3] = [
        ("subject", ErrorKind::SubjectNotFound),
        ("depth", ErrorKind::DepthNotFound),
        ("star", ErrorKind::MapNotFound),
    ];

    #[test]
    fn a_finder_is_asked_once_for_a_map_however_many_steps_and_renders_use_it() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        let finder = Arc::new(Finds::flat(0.25));
        let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", finder.clone());
        let twice = recipe([uses("subject"), uses("subject").mask("subject")], &[]);
        let once = recipe([uses("subject")], &[]);
        rig.render(&photo, &twice)?;
        rig.render(&photo, &twice)?;
        rig.render(&photo, &once)?;
        assert_eq!(finder.calls(), 1);
        // every use got the answer
        assert_eq!(rig.seen(), vec![vec![0.25; 24]; 5]);
        Ok(())
    }

    #[test]
    fn a_map_handed_in_is_used_and_its_finder_is_never_asked() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        let finder = Arc::new(Finds::flat(0.25));
        let photo = Photo::new(Picture::new(4, 6, &values)?)
            .with_finder("subject", finder.clone())
            .with_map("subject", flat(0.5)?)?;
        rig.render(&photo, &recipe([uses("subject")], &[]))?;
        assert_eq!(finder.calls(), 0);
        assert_eq!(rig.seen(), vec![vec![0.5; 24]]);
        Ok(())
    }

    #[test]
    fn a_finder_is_not_asked_for_a_map_that_no_step_uses() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        let (subject, depth) = (Arc::new(Finds::flat(0.25)), Arc::new(Finds::flat(0.75)));
        let photo = Photo::new(Picture::new(4, 6, &values)?)
            .with_finder("subject", subject.clone())
            .with_finder("depth", depth.clone());
        rig.render(&photo, &recipe([Step::new("recorder").param("near", "depth")], &[]))?;
        assert_eq!((subject.calls(), depth.calls()), (0, 1));
        rig.render(&photo, &recipe([paint()], &[]))?;
        assert_eq!((subject.calls(), depth.calls()), (0, 1));
        Ok(())
    }

    #[test]
    fn every_map_a_recipe_needs_is_found_before_its_first_step_runs() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        // the finder notes how many steps had run when it was asked
        let (recorder, ran) = (rig.recorder.clone(), Arc::new(AtomicUsize::new(usize::MAX)));
        let noted = ran.clone();
        let finder = Finds::new(move |picture| {
            noted.store(recorder.received().len(), Ordering::SeqCst);
            Layer::new(picture.height(), picture.width(), vec![0.5; 24])
        });
        let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("depth", Arc::new(finder));
        rig.render(&photo, &recipe([paint(), uses("depth")], &[]))?;
        assert_eq!(ran.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn a_finder_is_given_the_photo_as_it_is_and_its_map_is_what_the_steps_get() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        // the map is the photo's red channel
        let finder = Finds::new(|picture| {
            let red = picture.pixels().iter().map(|pixel| pixel[0]).collect();
            Layer::new(picture.height(), picture.width(), red)
        });
        let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("star", Arc::new(finder));
        rig.render(
            &photo,
            &recipe(
                [Step::new("paint").param("level", 0.9).opacity(1.0), uses("star")],
                &["star"],
            ),
        )?;
        let red: Vec<f32> = values.chunks(3).map(|pixel| pixel[0]).collect();
        assert_eq!(rig.seen(), vec![red]);
        Ok(())
    }

    #[test]
    fn a_photo_shared_between_threads_still_asks_its_finder_once() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        // the finder lingers, giving any thread that was let in beside it time to ask too
        let inside = Arc::new(AtomicUsize::new(0));
        let entered = inside.clone();
        let finder = Arc::new(Finds::new(move |picture| {
            entered.fetch_add(1, Ordering::SeqCst);
            for _ in 0..20_000 {
                if entered.load(Ordering::SeqCst) > 1 {
                    break;
                }
                std::thread::yield_now();
            }
            Layer::new(picture.height(), picture.width(), vec![0.25; 24])
        }));
        let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", finder.clone());
        let look = recipe([uses("subject")], &[]);
        let results: Vec<Result<Image, Error>> = std::thread::scope(|scope| {
            let threads: Vec<_> = (0..8).map(|_| scope.spawn(|| rig.render(&photo, &look))).collect();
            threads.into_iter().filter_map(|thread| thread.join().ok()).collect()
        });
        assert_eq!(results.len(), 8);
        assert!(results.iter().all(Result::is_ok));
        assert_eq!(finder.calls(), 1);
        assert_eq!(inside.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[test]
    fn the_background_is_one_minus_the_subject() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        // quarters, exact in f32: a subject of 0, 1/4, 1/2, 3/4, 1 and 1/4 again, repeated
        let subject: Vec<f32> = (0..24)
            .map(|index| [0.0, 0.25, 0.5, 0.75, 1.0, 0.25][index % 6])
            .collect();
        let photo = Photo::new(Picture::new(4, 6, &values)?).with_map("subject", Layer::new(4, 6, subject.clone())?)?;
        rig.render(&photo, &recipe([uses("background"), uses("subject")], &[]))?;
        let seen = rig.seen();
        let background: Vec<f32> = subject.iter().map(|value| 1.0 - value).collect();
        assert_eq!(seen[0], background);
        assert_eq!(seen[1], subject);
        assert_eq!(&seen[0][..6], [1.0, 0.75, 0.5, 0.25, 0.0, 0.75]);
        Ok(())
    }

    #[test]
    fn the_light_is_the_rec_709_brightness_of_the_picture_as_it_was_given() -> Result<(), Error> {
        let rig = rig()?;
        // red, green, blue, white and a mixed pixel: brightness 0.2126 r + 0.7152 g + 0.0722 b, worked by hand from the
        // Rec. 709 definition (the last: 0.1063 + 0.1788 + 0.0722)
        let pixels = [
            1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.25, 1.0,
        ];
        let photo = Photo::new(Picture::new(1, 5, &pixels)?);
        // a step before the one that asks changes the picture; the light is still the photo's
        let recipe = recipe(
            [Step::new("paint").param("level", 0.9).opacity(1.0), uses("light")],
            &[],
        );
        rig.render(&photo, &recipe)?;
        let expected = [0.2126, 0.7152, 0.0722, 1.0, 0.3573];
        let seen = &rig.seen()[0];
        assert!(
            seen.iter()
                .zip(expected)
                .all(|(light, hand)| (light - hand).abs() < 1e-6),
            "{seen:?}"
        );
        Ok(())
    }

    #[test]
    fn a_picture_that_is_no_picture_has_no_light() -> Result<(), Error> {
        let rig = rig()?;
        for (pixel, row) in [
            ([1.5, 1.5, 1.5], "above 1"),
            ([-0.5, 0.0, 0.0], "below 0"),
        ] {
            let photo = Photo::new(Picture::new(1, 1, &pixel)?);
            let result = rig.render(&photo, &recipe([uses("light")], &[]));
            assert_refused(&result, ErrorKind::InvalidImage, &["light"], row);
        }
        assert_eq!(rig.recorder.received().len(), 0);
        Ok(())
    }

    #[test]
    fn a_map_nobody_gave_is_refused_as_that_maps_error_naming_the_recipe_and_nothing_runs() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        let photo = Photo::new(Picture::new(4, 6, &values)?);
        // (what the step works from, the map that is missing, the error the package raises it as)
        let rows = [
            ("subject", "subject", ErrorKind::SubjectNotFound),
            ("background", "subject", ErrorKind::SubjectNotFound),
            ("depth", "depth", ErrorKind::DepthNotFound),
            ("star", "star", ErrorKind::MapNotFound),
        ];
        for (map, missing, kind) in rows {
            let mut look = recipe([paint(), uses(map)], &["star"]);
            look.name = Some("dusk".to_owned());
            assert_refused(
                &rig.render(&photo, &look),
                kind,
                &[&format!("'{missing}'"), "'dusk'"],
                map,
            );
            // a mask and a drive ask for it the same way
            let masked = recipe([paint().mask(map)], &["star"]);
            assert_refused(&rig.render(&photo, &masked), kind, &[&format!("'{missing}'")], map);
        }
        assert_eq!(rig.recorder.received().len(), 0);
        Ok(())
    }

    #[test]
    fn a_map_of_another_size_than_the_picture_is_refused_when_it_is_handed_in_used_or_not() -> Result<(), Error> {
        let values = values();
        for (name, kind) in NAMES {
            // too small, and the picture turned on its side
            for (height, width) in [(2, 2), (6, 4)] {
                let result = Photo::new(Picture::new(4, 6, &values)?)
                    .with_map(name, Layer::new(height, width, vec![0.5; height * width])?);
                let said = [format!("'{name}'"), format!("{height} x {width}"), "4 x 6".to_owned()];
                assert_refused(&result, kind, &said.each_ref().map(String::as_str), name);
            }
            // the right size is accepted
            assert!(
                Photo::new(Picture::new(4, 6, &values)?)
                    .with_map(name, flat(0.5)?)
                    .is_ok()
            );
        }
        Ok(())
    }

    #[test]
    fn a_finder_answering_a_map_of_another_size_than_the_picture_is_refused_and_nothing_runs() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        for (name, kind) in NAMES {
            for (height, width) in [(2, 2), (6, 4)] {
                let finder = Finds::new(move |_| Layer::new(height, width, vec![0.5; height * width]));
                let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder(name, Arc::new(finder));
                let result = rig.render(&photo, &recipe([uses(name)], &["star"]));
                assert_refused(
                    &result,
                    kind,
                    &[&format!("'{name}'"), &format!("{height} x {width}")],
                    name,
                );
            }
        }
        assert_eq!(rig.recorder.received().len(), 0);
        Ok(())
    }

    #[test]
    fn what_a_finder_fails_with_is_the_renders_error_and_a_failure_is_not_remembered() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        let finder = Arc::new(Finds::new(|_| {
            Err(Error::map_not_found("subject", "nothing stands out"))
        }));
        let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", finder.clone());
        let look = recipe([uses("subject")], &[]);
        for _ in 0..2 {
            assert_refused(
                &rig.render(&photo, &look),
                ErrorKind::SubjectNotFound,
                &["nothing stands out"],
                "subject",
            );
        }
        assert_eq!(finder.calls(), 2);
        // a map with values outside 0..1 cannot be made at all, so a finder that builds one fails as `Layer::new` does
        let finder = Finds::new(|picture| Layer::new(picture.height(), picture.width(), vec![f32::NAN; 24]));
        let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("depth", Arc::new(finder));
        assert_refused(
            &rig.render(&photo, &recipe([uses("depth")], &[])),
            ErrorKind::Other,
            &["NaN"],
            "NaN",
        );
        Ok(())
    }

    #[test]
    fn a_name_that_is_no_map_of_the_recipe_is_refused_listing_the_names_there_are() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        let picture = Picture::new(4, 6, &values)?;
        // the recipe uses no map at all: the names given are checked all the same
        let look = recipe([paint()], &["star"]);
        let lists = ["depth", "star", "subject"];
        let given = |name: &str| Photo::new(picture).with_map(name, flat(0.5)?);
        let found = |name: &str| -> Result<Photo<'_>, Error> {
            Ok(Photo::new(picture).with_finder(name, Arc::new(Finds::flat(0.5))))
        };
        for name in ["nope", "light", "background", "cut"] {
            assert_refused(
                &rig.render(&given(name)?, &look),
                ErrorKind::Recipe,
                &[&format!("a map '{name}'"), "subject, depth, star"],
                name,
            );
            assert_refused(
                &rig.render(&found(name)?, &look),
                ErrorKind::Recipe,
                &[&format!("a finder '{name}'"), "subject, depth, star"],
                name,
            );
        }
        // the built-in maps and the recipe's own given region are maps of it, used or not
        for name in lists {
            rig.render(&given(name)?, &look)?;
            rig.render(&found(name)?, &look)?;
        }
        Ok(())
    }

    #[test]
    fn a_region_of_a_kind_the_core_does_not_run_yet_is_refused_by_name_when_a_step_uses_it() -> Result<(), Error> {
        let rig = rig()?;
        let values = values();
        let mut look = Recipe::new([uses("cut")]);
        look.regions.insert("cut", Path::default());
        let photo = Photo::new(Picture::new(4, 6, &values)?);
        assert_refused(
            &rig.render(&photo, &look),
            ErrorKind::Recipe,
            &["'cut'", "kind path", "not run"],
            "path",
        );
        // nothing runs for a recipe that is refused
        assert_eq!(rig.recorder.received().len(), 0);
        // declared and not used, it costs nothing and the recipe renders; nothing makes a map of it, so one handed in is
        // no map of the recipe
        let mut unused = Recipe::new([paint()]);
        unused.regions.insert("cut", Path::default());
        rig.render(&photo, &unused)?;
        assert_refused(
            &rig.render(
                &Photo::new(Picture::new(4, 6, &values)?).with_map("cut", flat(0.5)?)?,
                &unused,
            ),
            ErrorKind::Recipe,
            &["'cut'", "no map of the recipe"],
            "handed in",
        );
        Ok(())
    }
}
