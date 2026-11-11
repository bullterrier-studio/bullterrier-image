//! One photo through several recipes: its maps (given with `--map` or found once), and the options of the command line that
//! name them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bullterrier_image::{Error, Finder, Layer, MapKind, Photo, Presets, Recipe, read, read_map};
use bullterrier_image_finders::Segmenter;
use clap::Args as ClapArgs;

use crate::finders;

/// The photo and the recipes of a command that renders.
#[derive(ClapArgs)]
pub(crate) struct Source {
    /// The photo to style.
    pub(crate) photo: PathBuf,
    /// Recipe `.toml` files, folders of them, or names of built-in looks (see `bt-image effects`).
    #[arg(required = true)]
    pub(crate) recipes: Vec<String>,
}

/// The maps of the photo a command that renders can be given, and what finds the subject otherwise.
#[derive(ClapArgs)]
pub(crate) struct Maps {
    /// A map of the photo, the photo's size, instead of finding it: `subject=cutout.png` (a cut-out PNG, its transparency,
    /// or a black-and-white mask, white = subject; no model is used, except by a region that finds the subject piece by
    /// piece), `depth=depth.png` (greyscale, 8 or 16 bits, white = nearest, instead of the depth model), or the name of
    /// a `given` region of the recipe (a picture with transparency gives its alpha, any other its brightness); repeat
    /// for each map.
    #[arg(long = "map", value_name = "NAME=PICTURE", value_parser = spec)]
    pub(crate) maps: Vec<(String, PathBuf)>,
    /// What finds the subject: vision (Apple Vision, macOS only) or onnx (a portable model, downloaded on first use);
    /// default: `BT_IMAGE_SEGMENTER`, else vision on macOS and onnx elsewhere. A segmenter named here is checked at once,
    /// even for a look that needs no subject.
    #[arg(long, value_parser = finders::segmenter)]
    pub(crate) segmenter: Option<Segmenter>,
}

/// A clap value parser: `NAME=PICTURE`.
fn spec(text: &str) -> Result<(String, PathBuf), String> {
    match text.split_once('=') {
        Some((name, path)) if !name.is_empty() && !path.is_empty() => Ok((name.to_owned(), PathBuf::from(path))),
        _ => Err(format!(
            "expected NAME=PICTURE, for example subject=cutout.png; got {text:?}"
        )),
    }
}

/// `run` on each recipe's photo: `photo` read (a transparent one over white), the maps of `given` read, each other map the
/// recipes use found once by the default finder, then `run(photo, recipe, presets)` for each recipe, in order.
///
/// # Errors
/// [`Error::Recipe`] for a map given twice or a name no recipe knows; [`Error::MapNotFound`] naming the `--map` to give
/// for a `given` region; whatever reading, finding or rendering refuses.
pub(crate) fn each<T>(
    photo: &Path,
    recipes: &[Recipe],
    given: &Maps,
    run: impl Fn(&Photo<'_>, &Recipe, &Presets) -> Result<T, Error>,
) -> Result<Vec<T>, Error> {
    let presets = Presets::shipped()?;
    let image = read(photo)?.over_white();
    let mut maps: BTreeMap<String, Layer> = BTreeMap::new();
    for (name, path) in &given.maps {
        if maps.insert(name.clone(), read_map(path)?).is_some() {
            return Err(Error::Recipe(format!("--map {name} is given twice")));
        }
    }
    let mut finders: BTreeMap<String, Arc<dyn Finder>> = BTreeMap::new();
    let mut wanted = std::collections::BTreeSet::new();
    for recipe in recipes {
        let needs = recipe.needs(&presets)?;
        wanted.extend(needs.maps.iter().filter(|name| !maps.contains_key(*name)).cloned());
        if needs.finds_pieces {
            wanted.insert("subject".to_owned());
        }
    }
    if given.segmenter.is_some() {
        wanted.insert("subject".to_owned());
    }
    for name in wanted {
        if let Some(found) = finders::found(&name, given.segmenter)? {
            finders.insert(name, found.finder);
        }
    }
    let photos =
        Photo::for_recipes(image.as_picture(), recipes, &presets, maps, &finders).map_err(|error| match error {
            // a `given` region has no finder here: the fix is the option that gives it
            Error::MapNotFound {
                map,
                kind: MapKind::Other,
                ..
            } => Error::map_not_found(
                &map,
                format!(
                    "a recipe uses the map {map:?} (a region with `kind = \"given\"`), so it needs it: pass \
                 --map {map}=PICTURE (the photo's size; a picture with transparency gives its alpha, any other its \
                 brightness)"
                ),
            ),
            other => other,
        })?;
    photos
        .iter()
        .zip(recipes)
        .map(|(photo, recipe)| run(photo, recipe, &presets))
        .collect()
}
