//! `bt-image install onnx`, `models` and `doctor`: the models and the ONNX Runtime library, and whether the finders work.

use std::path::PathBuf;

use bullterrier_image::Error;
use bullterrier_image::finding::onnx::{Options, cache_dir, is_model};
use bullterrier_image::finding::runtime::{self, VERSION};
use bullterrier_image::finding::{Model, ModelKind, download, models};
use bullterrier_image_finders::{Segmenter, Vision};
use clap::{Args as ClapArgs, ValueEnum};

use crate::Console;

/// What `install` can install.
#[derive(Clone, Copy, ValueEnum)]
enum Target {
    /// The ONNX Runtime library and the subject model.
    Onnx,
}

#[derive(ClapArgs)]
pub(crate) struct InstallArgs {
    /// What to install.
    #[arg(value_enum)]
    target: Target,
    /// Also the depth model.
    #[arg(long)]
    depth: bool,
    /// Download without asking.
    #[arg(short, long)]
    yes: bool,
    /// The folder downloads are kept in (default: the platform's cache folder).
    #[arg(long, value_name = "DIR")]
    cache: Option<PathBuf>,
}

#[derive(ClapArgs)]
pub(crate) struct CacheArgs {
    /// The folder downloads are kept in (default: the platform's cache folder).
    #[arg(long, value_name = "DIR")]
    cache: Option<PathBuf>,
}

fn cache_of(args: Option<&PathBuf>) -> Result<PathBuf, Error> {
    args.map_or_else(cache_dir, |folder| Ok(folder.clone()))
}

fn file_of(model: &Model, cache: &std::path::Path) -> PathBuf {
    cache.join(format!("{}.onnx", model.name))
}

fn kind_name(kind: ModelKind) -> &'static str {
    match kind {
        ModelKind::Subject => "subject",
        _ => "depth",
    }
}

/// How a model file stands: kept and right, absent, or there but not the model.
fn standing(model: &Model, cache: &std::path::Path) -> &'static str {
    let file = file_of(model, cache);
    if !file.exists() {
        "not installed"
    } else if is_model(model, &file).unwrap_or(false) {
        "installed"
    } else {
        "damaged: it is not the model (remove it and install again)"
    }
}

/// Megabytes, rounded, as the catalogue says them.
fn megabytes(bytes: u64) -> u64 {
    bytes.div_ceil(1_000_000)
}

pub(crate) fn models_command(args: &CacheArgs, console: &mut Console<'_>) -> Result<i32, Error> {
    let cache = cache_of(args.cache.as_ref())?;
    console.result(&format!("Models, kept in {}", cache.display()))?;
    for model in models() {
        console.result(&format!(
            "  {}: {} model, {} MB, {}",
            model.name,
            kind_name(model.kind),
            megabytes(model.size),
            standing(model, &cache)
        ))?;
        console.result(&format!("    url: {}", model.url))?;
        console.result(&format!("    sha256: {}", model.sha256))?;
    }
    console.result("\n`bt-image install onnx` downloads the ONNX Runtime library and the subject model (add --depth for the depth model).")?;
    Ok(0)
}

pub(crate) fn install_command(args: &InstallArgs, console: &mut Console<'_>) -> Result<i32, Error> {
    let Target::Onnx = args.target;
    let cache = cache_of(args.cache.as_ref())?;
    let release = runtime::here()?;
    let library = runtime::path(&cache, release);
    let wanted: Vec<&Model> = models()
        .iter()
        .filter(|model| model.kind == ModelKind::Subject || args.depth)
        .filter(|model| standing(model, &cache) != "installed")
        .collect();
    let library_needed = !library.is_file();
    if !library_needed && wanted.is_empty() {
        console.result(&format!("ONNX is installed: {}", library.display()))?;
        return Ok(0);
    }
    let mut plan = Vec::new();
    if library_needed {
        plan.push(format!("ONNX Runtime {VERSION} ({} MB)", megabytes(release.size)));
    }
    plan.extend(
        wanted
            .iter()
            .map(|model| format!("the {} model ({} MB)", model.name, megabytes(model.size))),
    );
    let question = format!("download {} into {}?", plan.join(" and "), cache.display());
    if !args.yes && !console.confirm(&question)? {
        console.problem("nothing downloaded")?;
        return Ok(1);
    }
    if library_needed {
        console.progress(&format!("downloading ONNX Runtime {VERSION} from {}...", release.url))?;
        let kept = runtime::install(release, &cache)?;
        console.result(&format!("ONNX Runtime: {}", kept.display()))?;
    }
    for model in wanted {
        let file = file_of(model, &cache);
        console.progress(&format!("downloading the {} model from {}...", model.name, model.url))?;
        download::fetch(model.url, model.sha256, model.size, &file)?;
        console.result(&format!("{}: {}", model.name, file.display()))?;
    }
    Ok(0)
}

pub(crate) fn doctor_command(args: &CacheArgs, console: &mut Console<'_>) -> Result<i32, Error> {
    let cache = cache_of(args.cache.as_ref())?;
    console.result(&format!("cache folder: {}", cache.display()))?;
    let chosen = Segmenter::from_env()?.unwrap_or_else(Segmenter::default_here);
    let name = if chosen == Segmenter::Vision { "vision" } else { "onnx" };
    console.result(&format!(
        "subject finder in use: {name} (BT_IMAGE_SEGMENTER or this platform's choice)"
    ))?;
    let vision = match Vision::new() {
        Ok(_) => "works".to_owned(),
        Err(error) => format!("not available: {error}"),
    };
    console.result(&format!("Apple Vision: {vision}"))?;
    let library_options = Options::from_env(ModelKind::Subject).with_download(false);
    let library = match runtime::library(&library_options, &cache) {
        Ok(path) if path.is_file() => format!("works: {}", path.display()),
        Ok(path) => format!(
            "not available: {} is not a file (BT_IMAGE_ONNX_LIB names it)",
            path.display()
        ),
        Err(error) => format!("not available: {error}"),
    };
    console.result(&format!("ONNX Runtime {VERSION}: {library}"))?;
    let mut onnx_ready = library.starts_with("works");
    for model in models() {
        let stands = standing(model, &cache);
        onnx_ready = onnx_ready && (model.kind != ModelKind::Subject || stands == "installed");
        console.result(&format!("{} model ({}): {stands}", model.name, kind_name(model.kind)))?;
    }
    let works = if chosen == Segmenter::Vision {
        vision == "works"
    } else {
        onnx_ready
    };
    if works {
        console.result("ok: the subject finder in use works")?;
        Ok(0)
    } else if chosen == Segmenter::Vision {
        console.problem("the subject finder in use (Apple Vision) does not work: see above; BT_IMAGE_SEGMENTER=onnx uses the portable model")?;
        Ok(1)
    } else {
        console.problem("the subject finder in use (onnx) does not work: run `bt-image install onnx`")?;
        Ok(1)
    }
}
