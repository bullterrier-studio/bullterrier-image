//! `bt-image run`.

use std::path::PathBuf;

use bullterrier_image::pipeline::{Given, Project, RunOptions, Status, needed_maps, run};
use bullterrier_image::{Error, Presets};
use bullterrier_image_finders::Segmenter;
use clap::Args as ClapArgs;

use crate::{Console, finders, stamp};

#[derive(ClapArgs)]
#[expect(clippy::struct_excessive_bools, reason = "the flags are the command line's")]
pub(crate) struct Args {
    /// Project file (default: bt-image.toml).
    #[arg(default_value = "bt-image.toml")]
    project: PathBuf,
    /// Redo everything, even what is up to date.
    #[arg(long)]
    force: bool,
    /// Show what would be done and write nothing.
    #[arg(long)]
    dry_run: bool,
    /// Only the photos whose file name matches, e.g. 'dog*.jpg'.
    #[arg(long, value_name = "PATTERN")]
    only: Option<String>,
    /// Exit with an error if any result has a warning.
    #[arg(long)]
    strict: bool,
    /// Also save the picture after every step of every recipe.
    #[arg(long)]
    steps: bool,
    /// What finds the subject: vision (Apple Vision, macOS only) or onnx (a portable model, downloaded on first use);
    /// default: `BT_IMAGE_SEGMENTER`, else vision on macOS and onnx elsewhere. A segmenter named here is checked at once,
    /// even for a look that needs no subject.
    #[arg(long, value_parser = segmenter)]
    segmenter: Option<Segmenter>,
}

/// A clap value parser: `vision` or `onnx`.
pub(crate) fn segmenter(name: &str) -> Result<Segmenter, String> {
    Segmenter::named(name).map_err(|error| error.to_string())
}

pub(crate) fn command(args: &Args, console: &mut Console<'_>) -> Result<i32, Error> {
    let mut project = Project::read(&args.project)?;
    if args.steps {
        project.settings.steps = true;
    }
    let presets = Presets::shipped()?;
    let mut wanted = needed_maps(&project, &presets)?;
    if args.segmenter.is_some() {
        wanted.insert("subject".to_owned());
    }
    let mut given = Given::default();
    for name in wanted {
        if let Some(found) = finders::found(&name, args.segmenter)? {
            given.finders.insert(name, found);
        }
    }
    let generated = stamp::now();
    let options = RunOptions {
        force: args.force,
        dry_run: args.dry_run,
        only: args.only.as_deref(),
        generated: &generated,
    };
    let items = run(&project, &presets, &options, &given, &mut |line| console.progress(line))?;
    let failed = items.iter().filter(|item| item.status == Status::Failed).count();
    let flagged = items.iter().filter(|item| !item.warnings.is_empty()).count();
    let summary = format!("{} results: {failed} failed, {flagged} with warnings", items.len());
    if failed > 0 || flagged > 0 {
        console.problem(&summary)?; // -q hides a clean summary, never a problem
    } else {
        console.progress(&summary)?;
    }
    if !args.dry_run {
        console.result(&project.output.join("index.html").display().to_string())?;
    }
    Ok(i32::from(failed > 0 || (args.strict && flagged > 0)))
}
