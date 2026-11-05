//! What the integration tests of the ops area share: a folder for files, an effect that logs the partition and the
//! selection it is handed, the functions that render a region and give back what the effect saw, and a grid to lay out.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use bullterrier_image::region::{Edge, Grid, Shares};
use bullterrier_image::{
    Category, Effect, Error, FieldParam, Image, Input, Inputs, Layer, Options, Photo, Picture, Presets, Recipe, Region,
    Step, register,
};

/// A folder of the test's own, removed when it goes out of scope.
pub(crate) struct Folder(PathBuf);

impl Folder {
    /// A new, empty folder named after the test file `tag`.
    pub(crate) fn new(tag: &str) -> std::io::Result<Self> {
        static FOLDERS: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "bt-image-{tag}-{}-{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    /// The path of the file `name` in this folder, whether it is there or not.
    pub(crate) fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }

    /// A folder inside this one, named `a-folder`, and its path.
    pub(crate) fn directory(&self) -> std::io::Result<String> {
        let path = self.0.join("a-folder");
        std::fs::create_dir_all(&path)?;
        Ok(path.to_string_lossy().into_owned())
    }

    /// Writes `content` to the file `name` and gives its path.
    pub(crate) fn write(&self, name: &str, content: impl AsRef<[u8]>) -> std::io::Result<String> {
        let path = self.0.join(name);
        std::fs::write(&path, content)?;
        Ok(path.to_string_lossy().into_owned())
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// What a [`Spy`] was handed: the labels and number of pieces of its partition, the values of its selection `near`.
#[derive(Clone, Debug)]
pub(crate) struct Seen {
    pub(crate) labels: Vec<i32>,
    pub(crate) pieces: usize,
    pub(crate) near: Option<Vec<f32>>,
}

/// An effect that logs what it is handed and returns the picture as it is.
struct Spy {
    name: String,
    inputs: Vec<Input>,
    fields: Vec<FieldParam>,
    log: Mutex<Vec<Seen>>,
}

impl Effect for Spy {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn inputs(&self) -> &[Input] {
        &self.inputs
    }

    fn fields(&self) -> &[FieldParam] {
        &self.fields
    }

    fn apply(&self, picture: &Picture<'_>, _params: serde_json::Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        let (labels, pieces) = inputs
            .partition()
            .map_or_else(|| (Vec::new(), 0), |labels| (labels.values().to_vec(), labels.pieces()));
        self.log.lock().unwrap_or_else(PoisonError::into_inner).push(Seen {
            labels,
            pieces,
            near: inputs.field("near").map(|layer| layer.values().to_vec()),
        });
        Ok(picture.map(|pixel| pixel))
    }
}

/// A grey photo of `height` x `width` pixels rendered by one step of a spy that is handed the partition `partition` and the
/// selection `near` of the declared regions, if it is given them, with `subject` as the map of that name if there is one.
///
/// # Errors
/// Whatever the render refuses.
pub(crate) fn probe(
    (height, width): (usize, usize),
    subject: Option<Vec<f32>>,
    options: Options,
    regions: Vec<(&str, Region)>,
    (partition, near): (Option<&str>, Option<&str>),
) -> Result<Seen, Error> {
    static SPIES: AtomicUsize = AtomicUsize::new(0);
    let spy = Arc::new(Spy {
        name: format!("ops_spy_{}", SPIES.fetch_add(1, Ordering::Relaxed)),
        inputs: partition.map(|_| Input::Partition).into_iter().collect(),
        fields: near.map(|region| FieldParam::new("near", region)).into_iter().collect(),
        log: Mutex::new(Vec::new()),
    });
    register(spy.clone())?;
    let pixels = vec![0.5; height * width * 3];
    let mut photo = Photo::new(Picture::new(height, width, &pixels)?);
    if let Some(values) = subject {
        photo = photo.with_map("subject", Layer::new(height, width, values)?)?;
    }
    let mut step = Step::new(&spy.name);
    step.partition = partition.map(str::to_owned);
    let mut recipe = Recipe::new([step]);
    for (name, region) in regions {
        recipe = recipe.with_region(name, region);
    }
    photo.render(&recipe, &Presets::new(), options)?;
    let mut seen = spy.log.lock().unwrap_or_else(PoisonError::into_inner);
    seen.pop()
        .ok_or_else(|| Error::Recipe("the spy was handed nothing".to_owned()))
}

/// The labels and number of pieces of the partition `region` makes of a grey photo of `height` x `width` pixels, rendered at
/// the longest side `size` if given.
///
/// # Errors
/// Whatever the render refuses.
pub(crate) fn partition(
    (height, width): (usize, usize),
    size: Option<u32>,
    region: impl Into<Region>,
) -> Result<(Vec<i32>, usize), Error> {
    let options = size.map_or_else(Options::default, |size| Options::default().with_size(size));
    let seen = probe(
        (height, width),
        None,
        options,
        vec![("s", region.into())],
        (Some("s"), None),
    )?;
    Ok((seen.labels, seen.pieces))
}

/// The selection the region `region` makes of a grey photo of `height` x `width` pixels, with the map `subject` if given.
///
/// # Errors
/// Whatever the render refuses.
pub(crate) fn selection(
    (height, width): (usize, usize),
    subject: Option<Vec<f32>>,
    region: impl Into<Region>,
) -> Result<Vec<f32>, Error> {
    let seen = probe(
        (height, width),
        subject,
        Options::default(),
        vec![("r", region.into())],
        (None, Some("r")),
    )?;
    seen.near
        .ok_or_else(|| Error::Recipe("the spy was handed no selection".to_owned()))
}

/// An effect that paints the whole picture with one grey level.
struct Level {
    name: String,
    level: f32,
}

impl Effect for Level {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn apply(&self, picture: &Picture<'_>, _params: serde_json::Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        Ok(picture.map(|_| [self.level; 3]))
    }
}

/// A step, at full opacity, of a new effect that paints the whole picture grey `level`.
///
/// # Errors
/// If the effect cannot be registered.
pub(crate) fn level(level: f32) -> Result<Step, Error> {
    static LEVELS: AtomicUsize = AtomicUsize::new(0);
    let name = format!("ops_level_{}", LEVELS.fetch_add(1, Ordering::Relaxed));
    register(Arc::new(Level {
        name: name.clone(),
        level,
    }))?;
    Ok(Step::new(name).opacity(1.0))
}

/// A grid of `columns` x `rows` equal cells with `edge`.
pub(crate) fn cells(columns: usize, rows: usize, edge: Edge) -> Grid {
    Grid {
        edge,
        columns: Shares::equal(columns),
        rows: Shares::equal(rows),
        ..Grid::default()
    }
}
