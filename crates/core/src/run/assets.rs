//! The lookup tables and pictures a render reads: a step names one in a parameter of the kind `lut` or `picture`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use crate::{Error, Image, Lut};

/// Where a render finds a table or a picture by name: first among the ones the photo was given, else as the `.cube` or
/// image file of that path (relative paths are the working folder's: a recipe file's own folder is the loader's to
/// resolve). A file is read once per render, however many steps name it.
pub(super) struct Assets<'a> {
    given_luts: &'a BTreeMap<String, Arc<Lut>>,
    luts: BTreeMap<String, Arc<Lut>>,
    given_pictures: &'a BTreeMap<String, Arc<Image>>,
    pictures: BTreeMap<String, Arc<Image>>,
}

impl<'a> Assets<'a> {
    pub(super) const fn new(
        given_luts: &'a BTreeMap<String, Arc<Lut>>,
        given_pictures: &'a BTreeMap<String, Arc<Image>>,
    ) -> Self {
        Self {
            given_luts,
            luts: BTreeMap::new(),
            given_pictures,
            pictures: BTreeMap::new(),
        }
    }

    /// The table called `name`.
    ///
    /// # Errors
    /// [`Error::Lut`] naming the path if `name` is no table the photo was given and no readable `.cube` file.
    pub(super) fn lut(&mut self, name: &str) -> Result<Arc<Lut>, Error> {
        if let Some(table) = self.given_luts.get(name).or_else(|| self.luts.get(name)) {
            return Ok(Arc::clone(table));
        }
        let table = Arc::new(Lut::from_cube_file(Path::new(name))?);
        self.luts.insert(name.to_owned(), Arc::clone(&table));
        Ok(table)
    }

    /// The picture called `name`.
    ///
    /// # Errors
    /// [`Error::ImageRead`] naming the path if `name` is no picture the photo was given and no readable image file (or
    /// one over the core's pixel limit).
    pub(super) fn image(&mut self, name: &str) -> Result<Arc<Image>, Error> {
        if let Some(image) = self.given_pictures.get(name).or_else(|| self.pictures.get(name)) {
            return Ok(Arc::clone(image));
        }
        let image = Arc::new(crate::read(Path::new(name))?);
        self.pictures.insert(name.to_owned(), Arc::clone(&image));
        Ok(image)
    }
}
