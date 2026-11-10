//! Writing a result: every format and size of a still.

use std::path::{Path, PathBuf};

use super::project::{Format, Settings};
use crate::files::write_with_quality;
use crate::ops::resample::Resize;
use crate::{Error, Image};

impl Format {
    /// The extension of a file of this format.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Jpg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }
}

/// `target`, once it is known to lie inside `output`: every file a pipeline writes goes through here. A photo's file name
/// becomes a folder name, and a name such as `...png` or a link inside the output folder would otherwise carry the writes
/// elsewhere.
///
/// # Errors
/// [`Error::Project`] naming the path if it does not.
pub(super) fn inside(output: &Path, target: &Path) -> Result<PathBuf, Error> {
    let (out, there) = (lexical(output), lexical(target));
    let resolved = std::fs::canonicalize(output)
        .ok()
        .zip(deepest_existing(&there).and_then(|path| std::fs::canonicalize(path).ok()));
    let within = there.starts_with(&out) && resolved.is_none_or(|(output, deepest)| deepest.starts_with(output));
    if within {
        Ok(target.to_path_buf())
    } else {
        Err(Error::Project(format!(
            "{} is outside the output folder {}: rename the photo",
            target.display(),
            output.display()
        )))
    }
}

/// `path` with `.` and `..` resolved by name alone.
fn lexical(path: &Path) -> PathBuf {
    let mut kept = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                kept.pop();
            }
            std::path::Component::CurDir => {}
            other => kept.push(other),
        }
    }
    kept
}

/// The longest prefix of `path` that exists.
fn deepest_existing(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| ancestor.exists())
        .map(Path::to_path_buf)
}

/// The sizes to export: none is as rendered. A size at or above the longest side of what was rendered means as rendered.
/// Largest first, as rendered first.
fn sizes(settings: &Settings, native: usize) -> Vec<Option<u32>> {
    let mut wanted: Vec<Option<u32>> = settings
        .sizes
        .iter()
        .map(|&size| {
            if usize::try_from(size).is_ok_and(|size| size >= native) {
                None
            } else {
                Some(size)
            }
        })
        .collect();
    if wanted.is_empty() {
        wanted.push(None);
    }
    wanted.sort_by(|one, other| other.unwrap_or(u32::MAX).cmp(&one.unwrap_or(u32::MAX)));
    wanted.dedup();
    wanted
}

/// `image` written to `folder` as `name` in every format and size of `settings`: `name.jpg` as rendered, `name_2048.jpg` for a
/// copy of 2048 px on the longest side; the files in the order written (largest size first, each in the formats' order).
/// Every file must lie inside `output`.
///
/// # Errors
/// [`Error::Project`] for a file that would lie outside `output`; the errors of
/// [`write_with_quality`](crate::write_with_quality) (a JPEG cannot hold transparency, a folder that cannot be written).
pub fn export_still(
    image: &Image,
    name: &str,
    folder: &Path,
    output: &Path,
    settings: &Settings,
) -> Result<Vec<PathBuf>, Error> {
    let native = image.height().max(image.width());
    let mut written = Vec::new();
    for size in sizes(settings, native) {
        let copy;
        let picture = match size {
            None => image,
            Some(longest) => {
                copy = scaled(image, longest)?;
                &copy
            }
        };
        for format in &settings.formats {
            let file = match size {
                None => format!("{name}.{}", format.extension()),
                Some(longest) => format!("{name}_{longest}.{}", format.extension()),
            };
            let target = inside(output, &folder.join(file))?;
            std::fs::create_dir_all(folder).map_err(|error| {
                Error::ImageWrite(format!("cannot write to the folder {}: {error}", folder.display()))
            })?;
            write_with_quality(&target, picture, settings.quality)?;
            written.push(target);
        }
    }
    Ok(written)
}

/// `image` scaled so its longest side is `longest` px (colour and alpha, as a picture is resized everywhere).
fn scaled(image: &Image, longest: u32) -> Result<Image, Error> {
    let picture = image.as_picture();
    let Some(resize) = Resize::to_longest_side(picture.height(), picture.width(), longest)? else {
        return Ok(image.clone());
    };
    let colour = resize.apply(picture.values().as_chunks::<3>().0);
    let resized = Image::from_pixels(resize.height(), resize.width(), colour);
    Ok(match image.alpha() {
        None => resized,
        Some(alpha) => {
            let planes: Vec<[f32; 1]> = alpha.iter().map(|&value| [value]).collect();
            resized.with_alpha(resize.apply(&planes).into_iter().map(|[value]| value).collect())
        }
    })
}
