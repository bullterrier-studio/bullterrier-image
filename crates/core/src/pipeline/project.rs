//! A project: the photos and recipes to process, where the results go, and how they are exported. It is read from a
//! `bt-image.toml`, or made in code; either way the same rules hold, and a value that breaks one is refused naming the
//! file or the setting and the value.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Recipe};

/// The pictures a pipeline can write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Format {
    /// JPEG: lossy, no transparency.
    Jpg,
    /// PNG: lossless.
    Png,
    /// WebP: lossless.
    Webp,
}

/// A recipe of a project: a file to read when the project runs, or one built in code.
/// Written as the file's path (a string) or as the recipe itself (an object).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum RecipeSource {
    /// A recipe file.
    File(PathBuf),
    /// A recipe built in code.
    Built(Box<Recipe>),
}

/// How a project exports its results and what it checks.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// How stills are saved: one or more of the formats.
    pub formats: Vec<Format>,
    /// Longest sides in px (each at least 16) of smaller copies, largest first; none: as rendered.
    pub sizes: Vec<u32>,
    /// 1 to 100, for jpg only (png and webp are lossless).
    pub quality: u8,
    /// A cut-out covering less than the first or more than the second of the photo is flagged.
    pub subject_share: (f64, f64),
    /// Also save the picture after every step.
    pub steps: bool,
}

/// The smallest size a copy may have, px.
const SMALLEST_SIZE: u32 = 16; // constant: the format's floor: a copy smaller than this shows nothing

impl Settings {
    /// These settings as the JSON object a project is written in (`formats`, `sizes`, `quality`, `subject_share`, `steps`).
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "formats": self.formats,
            "sizes": self.sizes,
            "quality": self.quality,
            "subject_share": [self.subject_share.0, self.subject_share.1],
            "steps": self.steps,
        })
    }
}

impl Default for Settings {
    /// jpg, no extra sizes, quality 92, a cut-out flagged outside 2 % to 97 % of the photo, no steps.
    fn default() -> Self {
        Self {
            formats: vec![Format::Jpg],
            sizes: Vec::new(),
            quality: 92, // constant: the default quality of the format: visually lossless for photos
            subject_share: (0.02, 0.97), // constant: a cut-out this small or this large is probably a failure
            steps: false,
        }
    }
}

/// A pipeline: what it processes, where it writes and how.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Project {
    /// The photo files, at least one, no two with the same file name without its extension.
    pub photos: Vec<PathBuf>,
    /// The recipes, at least one.
    pub recipes: Vec<RecipeSource>,
    /// The folder everything is written to.
    pub output: PathBuf,
    /// How results are exported and checked.
    pub settings: Settings,
}

/// The name of a project file.
pub(super) const FILE: &str = "bt-image.toml";
/// The photos of a project that names none.
const DEFAULT_PHOTOS: [&str; 4] = ["photos/*.jpg", "photos/*.jpeg", "photos/*.png", "photos/*.webp"]; // constant: where photos usually are

/// The file as written: every key optional, none unknown.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    photos: Option<Vec<String>>,
    recipes: Option<Vec<String>>,
    output: Option<String>,
    export: Option<Export>,
    checks: Option<Checks>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Export {
    formats: Option<Vec<Format>>,
    sizes: Option<Vec<u32>>,
    quality: Option<u8>,
    steps: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Checks {
    subject_share: Option<(f64, f64)>,
}

/// A project as JSON: the settings beside the lists, every setting optional.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Given {
    photos: Vec<PathBuf>,
    recipes: Vec<RecipeSource>,
    output: PathBuf,
    formats: Option<Vec<Format>>,
    sizes: Option<Vec<u32>>,
    quality: Option<u8>,
    subject_share: Option<(f64, f64)>,
    steps: Option<bool>,
}

impl Project {
    /// The project that the JSON object `text` writes: `photos` and `recipes` (each a path, or for a recipe the recipe
    /// itself), `output`, and the settings `formats`, `sizes`, `quality`, `subject_share` and `steps`, each left to its
    /// default when missing. A project made in code, whatever its language, is checked by this.
    ///
    /// # Errors
    /// [`Error::Project`] naming the key and the value for text that is not that shape (a number where text is wanted, an
    /// unknown key), or as [`Project::new`].
    pub fn from_json(text: &str) -> Result<Self, Error> {
        let mut deserializer = serde_json::Deserializer::from_str(text);
        let given: Given = serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
            let path = error.path().to_string();
            Error::Project(if path == "." {
                error.inner().to_string()
            } else {
                format!("{path}: {}", error.inner())
            })
        })?;
        let mut settings = Settings::default();
        settings.formats = given.formats.unwrap_or(settings.formats);
        settings.sizes = given.sizes.unwrap_or(settings.sizes);
        settings.quality = given.quality.unwrap_or(settings.quality);
        settings.subject_share = given.subject_share.unwrap_or(settings.subject_share);
        settings.steps = given.steps.unwrap_or(settings.steps);
        Self::new(given.photos, given.recipes, given.output, settings)
    }

    /// This project as the JSON object [`from_json`](Self::from_json) reads: the lists and `output` beside the settings.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let mut written = self.settings.to_json();
        if let Some(object) = written.as_object_mut() {
            object.insert("photos".to_owned(), serde_json::json!(self.photos));
            object.insert("recipes".to_owned(), serde_json::json!(self.recipes));
            object.insert("output".to_owned(), serde_json::json!(self.output));
        }
        written
    }

    /// A project of these parts, held to the rules of a project.
    ///
    /// # Errors
    /// [`Error::Project`] naming the setting and the value: no photo, no recipe, no format, a size under 16 px, a quality
    /// outside 1 to 100, a share that is not a range within 0..1, two photos with one file name (each gets the output
    /// folder of that name).
    pub fn new(
        photos: Vec<PathBuf>,
        recipes: Vec<RecipeSource>,
        output: PathBuf,
        mut settings: Settings,
    ) -> Result<Self, Error> {
        let refuse = |message: String| Err(Error::Project(message));
        if photos.is_empty() {
            return refuse("photos must name at least one photo".to_owned());
        }
        if recipes.is_empty() {
            return refuse("recipes must name at least one recipe".to_owned());
        }
        if settings.formats.is_empty() {
            return refuse("formats must name at least one of jpg, png, webp".to_owned());
        }
        if let Some(size) = settings.sizes.iter().find(|&&size| size < SMALLEST_SIZE) {
            return refuse(format!("sizes must each be at least {SMALLEST_SIZE} px, got {size}"));
        }
        if !(1..=100).contains(&settings.quality) {
            return refuse(format!("quality must be 1 to 100, got {}", settings.quality));
        }
        let (low, high) = settings.subject_share;
        if !(0.0..=1.0).contains(&low) || !(0.0..=1.0).contains(&high) {
            return refuse(format!("subject_share must be within 0..1, got ({low}, {high})"));
        }
        if low >= high {
            return refuse(format!(
                "subject_share must be (low, high) with low < high, got ({low}, {high})"
            ));
        }
        let mut stems = BTreeSet::new();
        let mut twice = BTreeSet::new();
        for photo in &photos {
            let stem = photo
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !stems.insert(stem.clone()) {
                twice.insert(stem);
            }
        }
        if !twice.is_empty() {
            return refuse(format!("two photos share a file name ({twice:?})"));
        }
        settings.sizes.sort_unstable_by(|one, other| other.cmp(one)); // largest first: equal settings are the same settings
        Ok(Self {
            photos,
            recipes,
            output,
            settings,
        })
    }

    /// The project the file `path` (a `bt-image.toml`) describes: globs and folders are resolved relative to the file.
    ///
    /// # Errors
    /// [`Error::Project`] starting with the file's name for a file that is not TOML (with its line and column), an unknown
    /// key, a key of the wrong kind, globs that match no photo, no recipe file, or a value [`Project::new`] refuses;
    /// [`Error::ImageRead`] never: a file that cannot be read is [`Error::Project`] too.
    pub fn read(path: &Path) -> Result<Self, Error> {
        let name = path
            .file_name()
            .map_or_else(|| FILE.to_owned(), |name| name.to_string_lossy().into_owned());
        let refuse = |message: String| Error::Project(format!("{name}: {message}"));
        let root = path.parent().unwrap_or_else(|| Path::new("."));
        let text = std::fs::read(path).map_err(|error| refuse(format!("cannot be read: {error}")))?;
        let text = String::from_utf8(text).map_err(|error| {
            refuse(format!(
                "not UTF-8 text ({} at byte {})",
                error.utf8_error(),
                error.utf8_error().valid_up_to()
            ))
        })?;
        let file: File = toml::from_str(&text).map_err(|error| refuse(error.to_string().trim_end().to_owned()))?;
        let globs: Vec<String> = file
            .photos
            .unwrap_or_else(|| DEFAULT_PHOTOS.iter().map(|&glob| glob.to_owned()).collect());
        let photos = found_photos(root, &globs).map_err(refuse)?;
        if photos.is_empty() {
            return Err(refuse(format!(
                "no photos match {globs:?} (looked relative to {})",
                root.display()
            )));
        }
        let entries = file.recipes.unwrap_or_else(|| vec!["recipes".to_owned()]);
        let recipes = found_recipes(root, &entries);
        if recipes.is_empty() || !recipes.iter().all(|recipe| recipe.is_file()) {
            return Err(refuse(format!("no recipe files found in {entries:?}")));
        }
        let mut settings = Settings::default();
        if let Some(export) = file.export {
            settings.formats = export.formats.unwrap_or(settings.formats);
            settings.sizes = export.sizes.unwrap_or(settings.sizes);
            settings.quality = export.quality.unwrap_or(settings.quality);
            settings.steps = export.steps.unwrap_or(settings.steps);
        }
        if let Some(share) = file.checks.and_then(|checks| checks.subject_share) {
            settings.subject_share = share;
        }
        let output = root.join(file.output.as_deref().unwrap_or("out"));
        let recipes = recipes.into_iter().map(RecipeSource::File).collect();
        Self::new(photos, recipes, output, settings).map_err(|error| refuse(error.to_string()))
    }
}

/// The photo files the `globs` match, relative to `root` (`**` looks in the folders below), sorted: a name that begins
/// with a dot is skipped by a wildcard, as a shell skips it, and a folder is no photo.
fn found_photos(root: &Path, globs: &[String]) -> Result<Vec<PathBuf>, String> {
    let options = glob::MatchOptions {
        case_sensitive: true,
        require_literal_separator: false,
        require_literal_leading_dot: true,
    };
    let mut found = BTreeSet::new();
    for pattern in globs {
        let full = root.join(pattern);
        let matches = glob::glob_with(&full.to_string_lossy(), options)
            .map_err(|error| format!("the photos glob {pattern:?} is not one ({error})"))?;
        found.extend(matches.flatten().filter(|path| path.is_file()));
    }
    Ok(found.into_iter().collect())
}

/// The recipe files `entries` name relative to `root`: a folder is its `.toml` files, sorted; anything else is itself.
fn found_recipes(root: &Path, entries: &[String]) -> Vec<PathBuf> {
    let mut recipes = Vec::new();
    for entry in entries {
        let target = root.join(entry);
        if target.is_dir() {
            let mut inside: Vec<PathBuf> = std::fs::read_dir(&target)
                .into_iter()
                .flatten()
                .flatten()
                .map(|file| file.path())
                .filter(|path| path.extension().is_some_and(|extension| extension == "toml"))
                .collect();
            inside.sort();
            recipes.extend(inside);
        } else {
            recipes.push(target);
        }
    }
    recipes
}
