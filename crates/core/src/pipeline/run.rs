//! Running a project: every photo with every recipe, remembered between runs.
//!
//! Stages, in order, for every photo: ingest (read it, upright and in sRGB, a transparent photo over white), analyse (find the
//! subject and the depth once, each if a recipe needs it; the answers are remembered on disk), style (run each recipe), check,
//! export (every format and size) and report (`manifest.json` and `index.html`). One photo or recipe failing never stops the
//! others, and work that is up to date is skipped on the next run.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::atomic::write_whole;
use super::cache::MapCache;
use super::checks::{matte_warnings, still_warnings};
use super::export::export_still;
use super::identity::{core_identity, fingerprint};
use super::manifest::{Item, Status, read_manifest, report_html, write_record};
use super::names::file_name;
use super::project::{Project, RecipeSource};
use super::stages::write_stages;
use crate::params::Kind;
use crate::{Error, ErrorKind, Finder, Image, Layer, Lut, MapKind, Options, Photo, Picture, Presets, Recipe};

/// The finder of a map, and the name of what its answers depend on: two finders with one identity are taken to answer alike,
/// so what one found is reused for the other.
#[derive(Clone)]
pub struct Found {
    /// What finds the map in a photo.
    pub finder: Arc<dyn Finder>,
    /// What its answers depend on: a model, a version, an option.
    pub identity: String,
}

/// What a run is given besides the project: the finders by map name (the caller builds the defaults for the maps the recipes need,
/// see [`needed_maps`]) and the pictures and lookup tables that recipes built in code name (`$picture:...`, `$lut:...`).
#[derive(Clone, Default)]
pub struct Given {
    /// The finder of each map, by name.
    pub finders: BTreeMap<String, Found>,
    /// The pictures recipes name, by the name they use.
    pub pictures: BTreeMap<String, Arc<Image>>,
    /// The lookup tables recipes name, by the name they use.
    pub luts: BTreeMap<String, Arc<Lut>>,
}

/// How a run goes.
#[derive(Clone, Copy, Debug, Default)]
pub struct RunOptions<'a> {
    /// Redo pairs that are up to date.
    pub force: bool,
    /// Report what would be done and write nothing.
    pub dry_run: bool,
    /// A file-name pattern selecting some of the photos.
    pub only: Option<&'a str>,
    /// The local time of the run, as the record writes it (the core reads no clock).
    pub generated: &'a str,
}

/// A recipe of a project, loaded.
struct Loaded {
    recipe: Recipe,
    /// What the key is made of: the file's text, or the recipe built in code written out.
    text: String,
    /// The name its files are given.
    name: String,
}

/// The recipes of `project`, loaded and named.
///
/// # Errors
/// What [`Recipe::from_file`] and [`file_name`] refuse; [`Error::Project`] for two recipes whose names give one file name.
fn load(project: &Project) -> Result<Vec<Loaded>, Error> {
    let mut loaded = Vec::new();
    for source in &project.recipes {
        let (recipe, text) = match source {
            RecipeSource::File(path) => {
                let text = std::fs::read_to_string(path).unwrap_or_default();
                (Recipe::from_file(path)?, text)
            }
            RecipeSource::Built(recipe) => ((**recipe).clone(), serde_json::to_string(recipe).unwrap_or_default()),
        };
        let name = file_name(&recipe)?;
        loaded.push(Loaded { recipe, text, name });
    }
    let names: Vec<&str> = loaded.iter().map(|one| one.name.as_str()).collect();
    let twice: BTreeSet<&str> = names
        .iter()
        .copied()
        .filter(|name| names.iter().filter(|other| *other == name).count() > 1)
        .collect();
    if !twice.is_empty() {
        return Err(Error::Project(format!(
            "two recipes are called the same (as file names): {twice:?}"
        )));
    }
    Ok(loaded)
}

/// The maps the recipes of `project` need a finder for: `subject` and `depth` when a recipe uses them, the name of each `given`
/// region one uses, and `subject` for a recipe that finds it piece by piece. The caller builds a default finder for each of
/// `subject` and `depth` it has not given.
///
/// # Errors
/// What loading and checking the recipes refuses.
pub fn needed_maps(project: &Project, presets: &Presets) -> Result<BTreeSet<String>, Error> {
    let mut maps = BTreeSet::new();
    for one in load(project)? {
        let needs = one.recipe.needs(presets)?;
        maps.extend(needs.maps);
        if needs.finds_pieces {
            maps.insert("subject".to_owned());
        }
    }
    Ok(maps)
}

/// `bytes` hashed, as 64 hex digits.
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The package's name for the exception of an error of this kind: what a record says failed.
const fn exception(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::Recipe => "RecipeError",
        ErrorKind::UnknownEffect => "UnknownEffectError",
        ErrorKind::SubjectNotFound => "SubjectNotFoundError",
        ErrorKind::DepthNotFound => "DepthNotFoundError",
        ErrorKind::MapNotFound => "MapNotFoundError",
        ErrorKind::InvalidImage => "InvalidImageError",
        ErrorKind::NonFiniteImage => "NonFiniteImageError",
        ErrorKind::ImageRead => "ImageReadError",
        ErrorKind::ImageWrite => "ImageWriteError",
        ErrorKind::BackendUnavailable => "BackendUnavailableError",
        ErrorKind::Lut => "LutError",
        ErrorKind::Project => "ProjectError",
        _ => "ValueError",
    }
}

/// The content digests of the files a recipe names: pictures and lookup tables given as paths, and the svg files its regions
/// read (`RecipeError` when one cannot be read or is over its limit). A picture or table the caller gave by name is in the
/// recipe's text by its digest already.
fn file_digests(recipe: &Recipe) -> Result<Vec<String>, Error> {
    let written = serde_json::to_value(recipe).map_err(|error| Error::Recipe(error.to_string()))?;
    let mut digests = Vec::new();
    if let Some(Value::Object(regions)) = written.get("regions") {
        for region in regions
            .values()
            .filter(|region| region.get("kind").and_then(Value::as_str) == Some("svg"))
        {
            let file = region.get("file").and_then(Value::as_str).unwrap_or_default();
            let limit = region.get("max_kilobytes").and_then(Value::as_u64).unwrap_or(1024);
            digests.push(digest(&svg_bytes(Path::new(file), limit)?));
        }
    }
    let mut steps = Vec::new();
    for entry in written.get("steps").and_then(Value::as_array).into_iter().flatten() {
        match entry.get("looks").and_then(Value::as_array) {
            Some(looks) => steps.extend(looks.iter()),
            None => steps.push(entry),
        }
    }
    for step in steps {
        let Some(effect) = step
            .get("effect")
            .and_then(Value::as_str)
            .and_then(|name| crate::registry::effect(name).ok())
        else {
            continue;
        };
        let Some(table) = effect.params() else { continue };
        for field in table
            .fields
            .iter()
            .filter(|field| matches!(field.kind, Kind::Picture | Kind::Lut))
        {
            let given = step
                .get("params")
                .and_then(|params| params.get(field.name))
                .and_then(Value::as_str);
            if let Some(path) = given.filter(|text| !text.starts_with('#') && !text.starts_with('$')) {
                digests.push(std::fs::read(path).map_or_else(|_| format!("missing {path}"), |bytes| digest(&bytes)));
            }
        }
    }
    Ok(digests)
}

/// The content of the SVG `file`, no more than `max_kilobytes` of it.
fn svg_bytes(file: &Path, max_kilobytes: u64) -> Result<Vec<u8>, Error> {
    use std::io::Read;
    let name = file
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let limit = max_kilobytes * 1024;
    let mut data = Vec::new();
    std::fs::File::open(file)
        .and_then(|handle| handle.take(limit + 1).read_to_end(&mut data))
        .map_err(|error| Error::Recipe(format!("svg file '{name}' cannot be read: {error}")))?;
    if u64::try_from(data.len()).unwrap_or(u64::MAX) > limit {
        return Err(Error::Recipe(format!(
            "svg file '{name}' is over the limit max_kilobytes = {max_kilobytes}"
        )));
    }
    Ok(data)
}

/// An error that was remembered, made again.
fn failure(name: &str, kind: ErrorKind, message: &str) -> Error {
    match kind {
        ErrorKind::SubjectNotFound | ErrorKind::DepthNotFound | ErrorKind::MapNotFound => {
            Error::map_not_found(name, message)
        }
        _ => Error::BackendUnavailable(message.to_owned()),
    }
}

/// One photo while its recipes run: read once, and each map found once, on first use.
struct PhotoState<'a> {
    path: &'a Path,
    digest: String,
    cache: &'a MapCache,
    finders: &'a BTreeMap<String, Found>,
    found_by: &'a BTreeMap<String, String>,
    image: Option<Image>,
    maps: BTreeMap<String, Layer>,
    errors: BTreeMap<String, (ErrorKind, String)>,
}

impl PhotoState<'_> {
    /// The photo, read upright and in sRGB; a transparent one is laid over white, so a colour hidden under it never shows.
    fn image(&mut self) -> Result<&Image, Error> {
        if self.image.is_none() {
            let read = crate::read(self.path)?;
            let image = match read.alpha() {
                None => read,
                Some(alpha) => {
                    let colour = read.as_picture().values().as_chunks::<3>().0;
                    let over = colour
                        .iter()
                        .zip(alpha)
                        .map(|(pixel, &opacity)| pixel.map(|c| c * opacity + (1.0 - opacity)))
                        .collect();
                    Image::from_pixels(read.height(), read.width(), over)
                }
            };
            self.image = Some(image);
        }
        self.image
            .as_ref()
            .ok_or_else(|| Error::ImageRead("the photo vanished".to_owned()))
    }

    /// The map `name`, from the disk when its finder has found it in this photo before, else found (and held to be the
    /// photo's size) and remembered. A failed search is remembered too, so the photo's other recipes fail without asking again.
    fn map(&mut self, name: &str) -> Result<Layer, Error> {
        if let Some((kind, message)) = self.errors.get(name) {
            return Err(failure(name, *kind, message));
        }
        if let Some(layer) = self.maps.get(name) {
            return Ok(layer.clone());
        }
        let key = fingerprint(&[&self.digest, self.found_by.get(name).map_or("", String::as_str)]);
        let (height, width) = {
            let image = self.image()?;
            (image.height(), image.width())
        };
        let layer = if let Some(layer) = self.cache.get(&key, height, width) {
            layer
        } else {
            let found = self
                .finders
                .get(name)
                .ok_or_else(|| Error::map_not_found(name, format!("no finder was given for the {name}")))?;
            let image = self.image()?.clone();
            let layer = found.finder.find(&image.as_picture()).inspect_err(|error| {
                self.errors.insert(name.to_owned(), (error.kind(), error.to_string()));
            })?;
            if (layer.height(), layer.width()) != (height, width) {
                let message = format!(
                    "{name} is {}x{} but the photo is {height}x{width}",
                    layer.height(),
                    layer.width()
                );
                self.errors
                    .insert(name.to_owned(), (ErrorKind::MapNotFound, message.clone()));
                return Err(Error::map_not_found(name, message));
            }
            // a map the disk cannot keep fails the pair that asked, and is not remembered: the next pair asks again
            self.cache.put(&key, &layer)?;
            layer
        };
        self.maps.insert(name.to_owned(), layer.clone());
        Ok(layer)
    }
}

/// The subject finder the photo's recipes use to ask about the pieces of their partitions: it remembers each answer on disk by
/// the pixels it was asked about, so a repeat run asks the model nothing, and a crop in which nothing stands out is remembered
/// as an empty matte.
struct CropFinder {
    inner: Arc<dyn Finder>,
    cache: MapCache,
    digest: String,
    found_by: String,
}

impl Finder for CropFinder {
    fn find(&self, picture: &Picture<'_>) -> Result<Layer, Error> {
        let bytes: Vec<u8> = picture.values().iter().flat_map(|value| value.to_le_bytes()).collect();
        let shape = format!("{}x{}", picture.height(), picture.width());
        let key = format!(
            "crop-{}",
            fingerprint(&[&self.digest, &self.found_by, &shape, &digest(&bytes)])
        );
        if let Some(layer) = self.cache.get(&key, picture.height(), picture.width()) {
            return Ok(layer);
        }
        let layer = match self.inner.find(picture) {
            Ok(layer) => layer,
            Err(Error::MapNotFound {
                kind: MapKind::Subject, ..
            }) => Layer::new(
                picture.height(),
                picture.width(),
                vec![0.0; picture.height() * picture.width()],
            )?,
            Err(error) => return Err(error),
        };
        self.cache.put(&key, &layer)?;
        Ok(layer)
    }
}

/// What a run holds while it goes: the parts that are the same for every pair.
struct Context<'a> {
    project: &'a Project,
    presets: &'a Presets,
    options: &'a RunOptions<'a>,
    given: &'a Given,
    loaded: Vec<Loaded>,
    /// For each recipe, the maps it needs and whether it finds the subject piece by piece.
    needs: Vec<(BTreeSet<String>, bool)>,
    previous: BTreeMap<(String, String), Item>,
    settings: String,
    code: String,
    found_by: BTreeMap<String, String>,
    cache: MapCache,
}

impl<'a> Context<'a> {
    /// The run's inputs held to the rules of a run, before anything is read or written.
    fn new(
        project: &'a Project,
        presets: &'a Presets,
        options: &'a RunOptions<'a>,
        given: &'a Given,
    ) -> Result<Self, Error> {
        let loaded = load(project)?;
        for one in &loaded {
            if one.recipe.frame.transparent != crate::frame::Transparent::None
                && project
                    .settings
                    .formats
                    .iter()
                    .any(|format| format.extension() == "jpg")
            {
                return Err(Error::Project(format!(
                    "recipe '{}' cuts the picture out, which a JPEG cannot hold, but the project exports 'jpg': use png or webp in `formats`",
                    one.name
                )));
            }
        }
        let needs: Vec<(BTreeSet<String>, bool)> = loaded
            .iter()
            .map(|one| one.recipe.needs(presets).map(|needs| (needs.maps, needs.finds_pieces)))
            .collect::<Result<_, _>>()?;
        let known: BTreeSet<String> = loaded
            .iter()
            .flat_map(|one| one.recipe.known_maps().into_iter().map(str::to_owned))
            .collect();
        if let Some(unknown) = given.finders.keys().find(|name| !known.contains(*name)) {
            return Err(Error::Project(format!(
                "`finders` names '{unknown}', which is no map of the recipes; their maps: {}",
                known.iter().cloned().collect::<Vec<_>>().join(", ")
            )));
        }
        for (one, (maps, pieces)) in loaded.iter().zip(&needs) {
            for name in maps.iter().cloned().chain(pieces.then(|| "subject".to_owned())) {
                if !given.finders.contains_key(&name) {
                    return Err(Error::Project(format!(
                        "recipe '{}' uses the map '{name}', which has no finder: pass `finders={{'{name}': Finder(...)}}`, or render the recipe with `maps=`",
                        one.name
                    )));
                }
            }
        }
        let code = core_identity();
        let found_by = given
            .finders
            .iter()
            .map(|(name, found)| (name.clone(), fingerprint(&[name, &found.identity, &code])))
            .collect();
        Ok(Self {
            project,
            presets,
            options,
            given,
            loaded,
            needs,
            previous: read_manifest(&project.output)
                .into_iter()
                .map(|item| ((item.photo.clone(), item.recipe.clone()), item))
                .collect(),
            settings: format!("{:?}", project.settings),
            code,
            found_by,
            cache: MapCache::new(project.output.join(".cache")),
        })
    }

    /// The photos a run goes over: all of them, or those whose file name matches `only`.
    fn photos(&self) -> Result<Vec<&'a PathBuf>, Error> {
        let pattern = self
            .options
            .only
            .map(glob::Pattern::new)
            .transpose()
            .map_err(|error| Error::Project(format!("--only {:?} is not a pattern: {error}", self.options.only)))?;
        let photos: Vec<&PathBuf> = self
            .project
            .photos
            .iter()
            .filter(|photo| {
                pattern.as_ref().is_none_or(|pattern| {
                    photo
                        .file_name()
                        .is_some_and(|name| pattern.matches(&name.to_string_lossy()))
                })
            })
            .collect();
        if photos.is_empty() {
            return Err(Error::Project(format!(
                "no photos match --only {:?}",
                self.options.only.unwrap_or_default()
            )));
        }
        Ok(photos)
    }

    /// The key of `recipe` run on the photo with the digest `photo`: everything that can change the result.
    fn key(&self, photo: &str, index: usize) -> Result<String, Error> {
        let (one, (maps, pieces)) = (&self.loaded[index], &self.needs[index]);
        let mut parts: Vec<String> = vec![photo.to_owned(), one.text.clone()];
        parts.extend(file_digests(&one.recipe)?);
        parts.push(self.settings.clone());
        parts.push(self.code.clone());
        parts.extend(
            self.found_by
                .iter()
                .filter(|(name, _)| maps.contains(*name) || (name.as_str() == "subject" && *pieces))
                .map(|(_, key)| key.clone()),
        );
        let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        Ok(fingerprint(&refs))
    }

    /// What `record` writes for the items so far.
    fn record(&self, items: &[Item], photos: &[&str], recipes: &[&str]) -> Result<Vec<Item>, Error> {
        let previous: Vec<Item> = self.previous.values().cloned().collect();
        write_record(
            &self.project.output,
            items,
            &previous,
            photos,
            recipes,
            &self.code,
            self.options.generated,
        )
    }
}

/// Processes every photo with every recipe of `project` and writes the report; returns what happened to each pair.
///
/// Pairs whose photo, recipe, export settings, finders and the core itself are unchanged since the last run, and whose files
/// still exist, are skipped unless `force`. Recipes are validated first, so a typo fails at once. A map is asked at most once
/// per photo, and never again for a photo whose map is remembered in the project's cache by a finder of the same identity; if it
/// fails, the recipes of that photo that need it fail, and the rest run. The record is updated after every pair, so an
/// interrupted run keeps what it finished. `log` is told of each pair, after the record is written; an error from it stops the
/// run (an interrupt).
///
/// # Errors
/// [`Error::Project`] for two recipes of one file name, a recipe that cuts the picture out exported to jpg, an `only` that
/// matches no photo, a finder named for no map of the recipes, or a recipe that needs a map nothing can give; what loading a recipe
/// refuses; the failure to write the record or the report. A failure of one pair is that pair's [`Item`], not an error.
pub fn run(
    project: &Project,
    presets: &Presets,
    options: &RunOptions<'_>,
    given: &Given,
    log: &mut dyn FnMut(&str) -> Result<(), Error>,
) -> Result<Vec<Item>, Error> {
    let context = Context::new(project, presets, options, given)?;
    let photos = context.photos()?;
    let photo_names: Vec<String> = project
        .photos
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    let photo_refs: Vec<&str> = photo_names.iter().map(String::as_str).collect();
    let recipe_names: Vec<&str> = context.loaded.iter().map(|one| one.name.as_str()).collect();
    let mut items: Vec<Item> = Vec::new();
    let total = photos.len() * context.loaded.len();
    for photo in photos {
        let photo_name = photo
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let bytes = std::fs::read(photo)
            .map_err(|error| Error::ImageRead(format!("cannot read {}: {error}", photo.display())))?;
        let mut state = PhotoState {
            path: photo,
            digest: digest(&bytes),
            cache: &context.cache,
            finders: &given.finders,
            found_by: &context.found_by,
            image: None,
            maps: BTreeMap::new(),
            errors: BTreeMap::new(),
        };
        for index in 0..context.loaded.len() {
            let label = format!(
                "[{}/{total}] {photo_name} x {}",
                items.len() + 1,
                context.loaded[index].name
            );
            let (item, said) = context.pair(&mut state, &photo_name, index, &label)?;
            let ran = item.status == Status::Done || item.status == Status::Failed;
            items.push(item);
            if ran {
                context.record(&items, &photo_refs, &recipe_names)?;
            }
            log(&said)?;
        }
    }
    if !options.dry_run {
        let record = context.record(&items, &photo_refs, &recipe_names)?;
        let page = report_html(&project.photos, &project.output, &record);
        let target = project.output.join("index.html");
        write_whole(&target, page.as_bytes())
            .map_err(|error| Error::ImageWrite(format!("cannot write {}: {error}", target.display())))?;
    }
    Ok(items)
}

impl Context<'_> {
    /// What happens to one pair: skipped as up to date, reported as would-run, or run. The item, and the line the log says.
    fn pair(
        &self,
        state: &mut PhotoState<'_>,
        photo_name: &str,
        index: usize,
        label: &str,
    ) -> Result<(Item, String), Error> {
        let one = &self.loaded[index];
        let fingerprint = self.key(&state.digest, index)?;
        let old = self.previous.get(&(photo_name.to_owned(), one.name.clone()));
        if !self.options.force
            && let Some(old) = old.filter(|old| {
                matches!(old.status, Status::Done | Status::Skipped)
                    && old.fingerprint == fingerprint
                    && old
                        .outputs
                        .iter()
                        .all(|output| self.project.output.join(output).exists())
            })
        {
            return Ok((
                Item {
                    status: Status::Skipped,
                    seconds: 0.0,
                    ..old.clone()
                },
                format!("{label}: up to date"),
            ));
        }
        if self.options.dry_run {
            let item = Item {
                photo: photo_name.to_owned(),
                recipe: one.name.clone(),
                status: Status::WouldRun,
                fingerprint,
                ..Item::default()
            };
            return Ok((item, format!("{label}: would run")));
        }
        let started = std::time::Instant::now();
        let (maps, pieces) = &self.needs[index];
        let outcome = process(
            self.project,
            self.presets,
            self.given,
            &self.cache,
            state,
            one,
            maps,
            *pieces,
            &fingerprint,
        );
        let mut item = outcome.unwrap_or_else(|error| Item {
            photo: photo_name.to_owned(),
            recipe: one.name.clone(),
            status: Status::Failed,
            error: Some(format!("{}: {error}", exception(error.kind()))),
            fingerprint: fingerprint.clone(),
            ..Item::default()
        });
        item.seconds = (started.elapsed().as_secs_f64() * 100.0).round() / 100.0;
        let said = format!(
            "{label}: {}{}{}",
            item.status.name(),
            if item.warnings.is_empty() {
                String::new()
            } else {
                format!(" ({})", item.warnings.join("; "))
            },
            item.error
                .as_ref()
                .map_or_else(String::new, |error| format!(" {error}"))
        );
        Ok((item, said))
    }
}

/// `one`, run on the photo and exported: what happened, as an `Item`.
#[expect(
    clippy::too_many_arguments,
    reason = "the pieces of one pair of a run, each of its own kind"
)]
fn process(
    project: &Project,
    presets: &Presets,
    given: &Given,
    cache: &MapCache,
    state: &mut PhotoState<'_>,
    one: &Loaded,
    maps: &BTreeSet<String>,
    pieces: bool,
    fingerprint: &str,
) -> Result<Item, Error> {
    let mut warnings = Vec::new();
    let mut layers = BTreeMap::new();
    for name in maps {
        layers.insert(name.clone(), state.map(name)?);
    }
    if let Some(subject) = layers.get("subject") {
        warnings.extend(matte_warnings(subject, project.settings.subject_share));
    }
    let photo_name = state
        .path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let stem = state
        .path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let folder = project.output.join(&stem);
    let image = state.image()?.clone();
    let mut photo = Photo::new(image.as_picture());
    for (name, layer) in layers {
        photo = photo.with_map(name, layer)?;
    }
    if pieces && let Some(subject) = given.finders.get("subject") {
        let by = state.found_by.get("subject").cloned().unwrap_or_default();
        let crops = CropFinder {
            inner: Arc::clone(&subject.finder),
            cache: cache.clone(),
            digest: state.digest.clone(),
            found_by: by,
        };
        photo = photo.with_finder("subject", Arc::new(crops));
    }
    for (name, picture) in &given.pictures {
        photo = photo.with_image(name.clone(), Arc::clone(picture));
    }
    for (name, lut) in &given.luts {
        photo = photo.with_lut(name.clone(), Arc::clone(lut));
    }
    // the batch renders one picture at a time, so each render may use every core
    let render_options = Options::default().with_threads(0);
    let (rendered, stages) = if project.settings.steps {
        let stages = photo.trace(&one.recipe, presets, render_options)?;
        let last = stages.iter().rev().find_map(|stage| match stage {
            crate::Stage::CutOut(image) | crate::Stage::Step { image, .. } => Some(image.clone()),
            _ => None,
        });
        (
            last.ok_or_else(|| Error::Inputs("a trace has a last picture".to_owned()))?,
            Some(stages),
        )
    } else {
        (photo.render(&one.recipe, presets, render_options)?, None)
    };
    warnings.extend(still_warnings(&rendered));
    let mut outputs = export_still(&rendered, &one.name, &folder, &project.output, &project.settings)?;
    let sheet = match stages {
        Some(stages) => {
            let target = folder.join("steps").join(&one.name);
            let sheet = write_stages(&one.recipe, stages, &target, &project.output, project.settings.quality)?;
            outputs.push(sheet.clone());
            Some(sheet)
        }
        None => None,
    };
    let relative = |path: &Path| -> String {
        path.strip_prefix(&project.output)
            .unwrap_or(path)
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    };
    Ok(Item {
        photo: photo_name,
        recipe: one.name.clone(),
        status: Status::Done,
        outputs: outputs.iter().map(|path| relative(path)).collect(),
        warnings,
        steps: sheet.as_deref().map(relative),
        fingerprint: fingerprint.to_owned(),
        ..Item::default()
    })
}
