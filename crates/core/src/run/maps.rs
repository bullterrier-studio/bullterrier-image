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
use crate::recipe::built_in::{BuiltIn, Origin};
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
        let layer = match BuiltIn::named(name).map(BuiltIn::origin) {
            Some(Origin::Inverse(of)) => {
                let source = self.get(of.name(), place)?;
                let inverse = source.values().iter().map(|value| 1.0 - value).collect();
                Arc::new(Layer::new(source.height(), source.width(), inverse)?)
            }
            Some(Origin::Picture) => Arc::new(light(&self.working.picture())?),
            Some(Origin::Found) | None => self.working.follow(stored(self.photo, self.recipe, name, place)?)?,
        };
        self.resolved.borrow_mut().insert(name.to_owned(), Arc::clone(&layer));
        Ok(layer)
    }

    /// Whether the photo gives the map `name` (handed in or found) or it is made from the picture: `subject`, `depth`,
    /// `background`, `light` and the `Given` regions, the names [`get`](Self::get) answers.
    pub(super) fn gives(&self, name: &str) -> bool {
        BuiltIn::named(name).is_some() || self.known.contains(name)
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
