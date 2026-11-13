//! `bt-image cutout`.

use std::path::PathBuf;

use bullterrier_image::finding::onnx::Options;
use bullterrier_image::finding::{DEFAULT_EDGE, ModelKind};
use bullterrier_image::{Error, Image, Picture, read, refine_edge, write};
use bullterrier_image_finders::subject_finder;
use clap::Args as ClapArgs;

use crate::photo::{Maps, read_maps};
use crate::{Console, finders};

#[derive(ClapArgs)]
pub(crate) struct Args {
    /// The photo to cut the subject out of.
    photo: PathBuf,
    /// Output file, a PNG.
    #[arg(short, long, default_value = "out/cutout.png")]
    out: PathBuf,
    /// Pull the cut edge in by this many px.
    #[arg(long)]
    shrink: Option<f64>,
    /// Soften the cut edge by this many px.
    #[arg(long)]
    feather: Option<f64>,
    /// An existing subject map, the photo's size, instead of finding it, to refine: `subject=cutout.png` (a cut-out PNG,
    /// its transparency, or a black-and-white mask, white = subject); cutout takes no other map.
    #[arg(long = "map", value_name = "NAME=PICTURE", value_parser = crate::photo::spec)]
    maps: Vec<(String, PathBuf)>,
    /// What finds the subject: vision (Apple Vision, macOS only), onnx (a portable model, downloaded on first use) or sam2
    /// (finds what `--point` and `--box` point at, no guess; downloaded on first use);
    /// default: `BT_IMAGE_SEGMENTER`, else vision on macOS and onnx elsewhere.
    #[arg(long, value_parser = finders::segmenter)]
    segmenter: Option<bullterrier_image_finders::Segmenter>,
    #[command(flatten)]
    clicks: finders::Clicks,
}

pub(crate) fn command(args: &Args, console: &mut Console<'_>) -> Result<i32, Error> {
    let photo = read(&args.photo)?.over_white();
    let given = Maps {
        files: args.maps.clone(),
        segmenter: args.segmenter,
        clicks: finders::Clicks::default(),
    };
    let mut maps = read_maps(&given)?;
    if let Some(other) = maps.keys().find(|name| *name != "subject") {
        return Err(Error::Recipe(format!(
            "cutout takes only --map subject=PICTURE, not {other:?}"
        )));
    }
    let alpha = if let Some(matte) = maps.remove("subject") {
        let (matte_size, photo_size) = ((matte.width(), matte.height()), (photo.width(), photo.height()));
        if matte_size != photo_size {
            return Err(Error::map_not_found(
                "subject",
                format!(
                    "the subject map is {}x{} px but the photo is {}x{} px",
                    matte_size.0, matte_size.1, photo_size.0, photo_size.1
                ),
            ));
        }
        // a matte given is used as it is, unless an edge clean-up is asked for
        refine_edge(&matte, args.shrink.unwrap_or(0.0), args.feather.unwrap_or(0.0))
    } else {
        let edge = (
            args.shrink.unwrap_or(DEFAULT_EDGE.0),
            args.feather.unwrap_or(DEFAULT_EDGE.1),
        );
        let finder = subject_finder(
            args.segmenter,
            &Options::from_env(ModelKind::Subject),
            edge,
            args.clicks.prompts()?.as_ref(),
        )?;
        finder.find(&photo.as_picture())?
    };
    let colour = photo.as_picture();
    let values = with_alpha(&colour, alpha.values());
    if let Some(folder) = args.out.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|error| Error::ImageWrite(format!("cannot make the folder {}: {error}", folder.display())))?;
    }
    write(&args.out, &Image::rgba(photo.height(), photo.width(), &values)?)?;
    console.result(&args.out.display().to_string())?;
    Ok(0)
}

/// The colour of `picture` with `alpha` as the fourth channel of each pixel.
fn with_alpha(picture: &Picture<'_>, alpha: &[f32]) -> Vec<f32> {
    picture
        .values()
        .as_chunks::<3>()
        .0
        .iter()
        .zip(alpha)
        .flat_map(|(pixel, &opacity)| [pixel[0], pixel[1], pixel[2], opacity])
        .collect()
}
