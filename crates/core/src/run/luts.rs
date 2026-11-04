//! The lookup tables a render reads: a step names one in a parameter of the kind `lut`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use crate::{Error, Lut};

/// Where a render finds a table by name: first among the ones the photo was given, else as the `.cube` file of that path
/// (relative paths are the working folder's: a recipe file's own folder is the loader's to resolve). A file is read once
/// per render, however many steps name it.
pub(super) struct Luts<'a> {
    given: &'a BTreeMap<String, Arc<Lut>>,
    opened: BTreeMap<String, Arc<Lut>>,
}

impl<'a> Luts<'a> {
    pub(super) const fn new(given: &'a BTreeMap<String, Arc<Lut>>) -> Self {
        Self {
            given,
            opened: BTreeMap::new(),
        }
    }

    /// The table called `name`.
    ///
    /// # Errors
    /// [`Error::Lut`] naming the path if `name` is no table the photo was given and no readable `.cube` file.
    pub(super) fn get(&mut self, name: &str) -> Result<Arc<Lut>, Error> {
        if let Some(table) = self.given.get(name).or_else(|| self.opened.get(name)) {
            return Ok(Arc::clone(table));
        }
        let table = Arc::new(Lut::from_cube_file(Path::new(name))?);
        self.opened.insert(name.to_owned(), Arc::clone(&table));
        Ok(table)
    }
}
