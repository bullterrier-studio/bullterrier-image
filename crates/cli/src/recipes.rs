//! The recipes a command line names: files, folders of them, or looks.

use std::path::{Path, PathBuf};

use bullterrier_image::pipeline::file_name;
use bullterrier_image::{Error, Presets, Recipe, shipped};

/// The recipes of `specs`, each a look's name, a recipe file, or a folder (its `.toml` files, by name), in order. Each is
/// checked to be a valid recipe and to have a name its files can be called by, so nothing slow starts for a recipe that cannot
/// be run or saved.
///
/// # Errors
/// [`Error::Recipe`] when a folder holds no recipe, when a spec is neither a look nor a file, naming the file otherwise
/// for a name that cannot be a file name; whatever reading a recipe refuses.
pub(crate) fn load(specs: &[String]) -> Result<Vec<Recipe>, Error> {
    let mut sources: Vec<String> = Vec::new();
    for spec in specs {
        let path = Path::new(spec);
        if path.is_dir() && !shipped::looks().iter().any(|(name, _)| name == spec) {
            let mut files: Vec<PathBuf> = std::fs::read_dir(path)
                .map_err(|error| Error::Recipe(format!("cannot read the folder {spec}: {error}")))?
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|file| file.extension().is_some_and(|extension| extension == "toml"))
                .collect();
            files.sort();
            sources.extend(files.iter().map(|file| file.to_string_lossy().into_owned()));
        } else {
            sources.push(spec.clone());
        }
    }
    if sources.is_empty() {
        return Err(Error::Recipe(format!("no recipe files found in {}", specs.join(", "))));
    }
    let presets = Presets::shipped()?;
    sources
        .iter()
        .map(|source| {
            let located = |error: Error| Error::Recipe(format!("{source}: {error}"));
            let recipe = Recipe::resolve(source)?.checked(&presets).map_err(located)?;
            file_name(&recipe).map_err(located)?;
            Ok(recipe)
        })
        .collect()
}
