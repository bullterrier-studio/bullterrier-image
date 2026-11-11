//! Several recipes on one photo: each map found once.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::{Finder, Photo};
use crate::{Error, Layer, Picture, Presets, Recipe};

impl<'a> Photo<'a> {
    /// One photo per recipe of `recipes`, each holding the maps its recipe uses: the ones in `maps`, else found, once
    /// however many recipes use them, by the finder of the same name in `finders` (with the picture as it is). A recipe that
    /// finds the subject piece by piece also gets the subject finder, whatever was handed in. A map or finder nothing
    /// uses is dropped, so a name that suits only some of the recipes does not refuse the others.
    ///
    /// # Errors
    /// [`Error::Recipe`] listing the names there are, for a name of `maps` or `finders` that is no map of any recipe;
    /// [`Error::MapNotFound`] (of the map's kind) for a map a recipe uses that is neither handed in nor has a finder, or
    /// that its finder cannot find or answers wrongly; what [`Recipe::needs`] refuses.
    pub fn for_recipes(
        picture: Picture<'a>,
        recipes: &[Recipe],
        presets: &Presets,
        maps: BTreeMap<String, Layer>,
        finders: &BTreeMap<String, Arc<dyn Finder>>,
    ) -> Result<Vec<Self>, Error> {
        let known: BTreeSet<&str> = recipes.iter().flat_map(Recipe::known_maps).collect();
        for (what, names) in [("maps", maps.keys().collect::<Vec<_>>()), ("finders", finders.keys().collect())] {
            if let Some(unknown) = names.into_iter().find(|name| !known.contains(name.as_str())) {
                return Err(Error::Recipe(format!(
                    "`{what}` names {unknown:?}, which is no map of the recipes; their maps: {} (`subject` and \
                     `depth` are built in; any other is a region with `kind = \"given\"`)",
                    known.iter().copied().collect::<Vec<_>>().join(", ")
                )));
            }
        }
        let needs = recipes
            .iter()
            .map(|recipe| recipe.needs(presets))
            .collect::<Result<Vec<_>, _>>()?;
        let handed: BTreeSet<String> = maps.keys().cloned().collect();
        let mut found = maps;
        for name in needs.iter().flat_map(|need| &need.maps) {
            if found.contains_key(name) {
                continue;
            }
            let finder = finders.get(name).ok_or_else(|| {
                Error::map_not_found(
                    name,
                    format!("a recipe uses the map {name:?}, which was neither handed in nor given a finder"),
                )
            })?;
            found.insert(name.clone(), finder.find(&picture)?);
        }
        recipes
            .iter()
            .zip(&needs)
            .map(|(recipe, need)| {
                let mut photo = Self::new(picture);
                for name in recipe.known_maps() {
                    if let Some(map) = found.get(name).filter(|_| need.maps.contains(name) || handed.contains(name)) {
                        photo = photo.with_map(name, map.clone())?;
                    }
                }
                match finders.get("subject") {
                    Some(finder) if need.finds_pieces => Ok(photo.with_finder("subject", Arc::clone(finder))),
                    _ => Ok(photo),
                }
            })
            .collect()
    }
}
