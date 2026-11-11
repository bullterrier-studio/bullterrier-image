//! `bt-image render`, `sheet` and `steps`: a photo through recipes.

use std::path::{Path, PathBuf};

use bullterrier_image::pipeline::{file_name, write_stages};
use bullterrier_image::{Error, Options, Recipe, Tile, contact_sheet, write};
use clap::{Args as ClapArgs, ValueEnum};

use crate::photo::{Maps, Source, each};
use crate::{Console, recipes};

/// The sides of a tile of the comparison sheet, px.
const TILE: usize = 480; // constant: the size `contact_sheet` has always been asked for here
/// The quality of a saved stage, 1 to 100.
const QUALITY: u8 = 92; // constant: the quality every export defaults to

/// The image formats `render` writes.
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Png,
    Jpg,
    Webp,
}

#[derive(ClapArgs)]
pub(crate) struct RenderArgs {
    #[command(flatten)]
    source: Source,
    #[command(flatten)]
    maps: Maps,
    /// Output folder.
    #[arg(short, long, default_value = "out")]
    out: PathBuf,
    /// Image format.
    #[arg(long, value_enum, default_value_t = Format::Png)]
    format: Format,
}

#[derive(ClapArgs)]
pub(crate) struct SheetArgs {
    #[command(flatten)]
    source: Source,
    #[command(flatten)]
    maps: Maps,
    /// Output file.
    #[arg(short, long, default_value = "out/sheet.png")]
    out: PathBuf,
    /// Pictures per row.
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u32).range(1..))]
    columns: u32,
}

#[derive(ClapArgs)]
pub(crate) struct StepsArgs {
    /// The photo to run the recipe on.
    photo: PathBuf,
    /// The recipe `.toml`, or the name of a built-in look, to look into.
    recipe: String,
    /// Output folder (default: `out/steps/PHOTO__RECIPE`).
    #[arg(short, long)]
    out: Option<PathBuf>,
    #[command(flatten)]
    maps: Maps,
}

/// The photo of `source` through each of its recipes, each map shared; recipes are checked before anything slow starts.
fn render_all(
    source: &Source,
    maps: &Maps,
    console: &mut Console<'_>,
) -> Result<(Vec<bullterrier_image::Recipe>, Vec<bullterrier_image::Image>), Error> {
    let all = recipes::load(&source.recipes)?;
    console.progress(&format!("rendering {} recipe(s)...", all.len()))?;
    let images = each(&source.photo, &all, maps, |photo, recipe, presets| {
        photo.render(recipe, presets, Options::default().with_threads(0)) // one photo: every core
    })?;
    Ok((all, images))
}

fn stem(photo: &Path) -> String {
    photo
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned())
}

pub(crate) fn render(args: &RenderArgs, console: &mut Console<'_>) -> Result<i32, Error> {
    let (all, images) = render_all(&args.source, &args.maps, console)?;
    std::fs::create_dir_all(&args.out)
        .map_err(|error| Error::ImageWrite(format!("cannot make the folder {}: {error}", args.out.display())))?;
    let extension = match args.format {
        Format::Png => "png",
        Format::Jpg => "jpg",
        Format::Webp => "webp",
    };
    for (recipe, image) in all.iter().zip(&images) {
        let target = args.out.join(format!(
            "{}__{}.{extension}",
            stem(&args.source.photo),
            file_name(recipe)?
        ));
        write(&target, image)?;
        console.result(&target.display().to_string())?;
    }
    Ok(0)
}

pub(crate) fn sheet(args: &SheetArgs, console: &mut Console<'_>) -> Result<i32, Error> {
    let (all, images) = render_all(&args.source, &args.maps, console)?;
    let names = all.iter().map(file_name).collect::<Result<Vec<_>, _>>()?; // labelled as `render` names files
    let tiles: Vec<Tile<'_>> = names
        .iter()
        .zip(&images)
        .map(|(name, image)| Tile::new(name, image))
        .collect();
    let columns = usize::try_from(args.columns).unwrap_or(usize::MAX);
    let sheet = contact_sheet(&tiles, columns, TILE)?;
    if let Some(folder) = args.out.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|error| Error::ImageWrite(format!("cannot make the folder {}: {error}", folder.display())))?;
    }
    write(&args.out, &sheet)?;
    console.result(&args.out.display().to_string())?;
    Ok(0)
}

pub(crate) fn steps(args: &StepsArgs, console: &mut Console<'_>) -> Result<i32, Error> {
    let recipe = Recipe::resolve(&args.recipe)?;
    let name = file_name(&recipe)?;
    let folder = args
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from("out/steps").join(format!("{}__{name}", stem(&args.photo))));
    let recipes = [recipe];
    let mut traced = each(&args.photo, &recipes, &args.maps, |photo, recipe, presets| {
        photo.trace(recipe, presets, Options::default().with_threads(0))
    })?;
    let stages = traced
        .pop()
        .ok_or_else(|| Error::Recipe("no recipe to trace".to_owned()))?;
    let (sheet, labels) = write_stages(&recipes[0], stages, &folder, &folder, QUALITY)?;
    for label in labels {
        console.progress(&format!("  {label}"))?;
    }
    console.result(&sheet.display().to_string())?;
    Ok(0)
}
