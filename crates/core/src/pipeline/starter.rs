//! The starter project `init` writes.

use std::path::{Path, PathBuf};

use super::project::FILE;
use crate::Error;

const PROJECT: &str = include_str!("../../data/starter/bt-image.toml");
const RECIPES: [(&str, &str); 2] = [
    ("glitch.toml", include_str!("../../data/starter/glitch.toml")),
    (
        "cutout-black.toml",
        include_str!("../../data/starter/cutout-black.toml"),
    ),
];

/// Writes a starter `bt-image.toml`, a `photos/` folder and, unless `recipes/` exists, two small recipes into `directory`
/// (made if missing); returns the project file.
///
/// # Errors
/// [`Error::Project`] naming the file if the project file exists already (nothing is written);
/// [`Error::ImageWrite`] naming the path if something cannot be written.
pub fn init_project(directory: &Path) -> Result<PathBuf, Error> {
    let target = directory.join(FILE);
    if target.exists() {
        return Err(Error::Project(format!("{} already exists", target.display())));
    }
    let write = |path: &Path, text: &str| {
        std::fs::write(path, text)
            .map_err(|error| Error::ImageWrite(format!("cannot write {}: {error}", path.display())))
    };
    let make = |path: &Path| {
        std::fs::create_dir_all(path)
            .map_err(|error| Error::ImageWrite(format!("cannot make the folder {}: {error}", path.display())))
    };
    make(&directory.join("photos"))?;
    let recipes = directory.join("recipes");
    if !recipes.exists() {
        make(&recipes)?;
        for (name, text) in RECIPES {
            write(&recipes.join(name), text)?;
        }
    }
    write(&target, PROJECT)?;
    Ok(target)
}
