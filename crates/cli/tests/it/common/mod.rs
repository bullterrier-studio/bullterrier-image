//! Helpers: run the command, and make a folder with photos and recipes.

use std::path::{Path, PathBuf};

use bullterrier_image::{Image, write};

pub(crate) type Outcome = Result<(), Box<dyn std::error::Error>>;

/// What a command did.
pub(crate) struct Ran {
    pub(crate) code: i32,
    pub(crate) out: String,
    pub(crate) err: String,
}

/// `bt-image ARGS`, in this process, with version `9.9.9`.
pub(crate) fn bt(args: &[&str]) -> Ran {
    let given: Vec<String> = args.iter().map(|&arg| arg.to_owned()).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = bullterrier_image_cli::main(&given, "9.9.9", &mut out, &mut err);
    Ran {
        code,
        out: String::from_utf8_lossy(&out).into_owned(),
        err: String::from_utf8_lossy(&err).into_owned(),
    }
}

/// A new empty folder for the test `name`.
pub(crate) fn folder(name: &str) -> Result<PathBuf, std::io::Error> {
    let root = std::env::temp_dir().join(format!("bt-image-cli-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&root));
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

/// A photo of a horizontal ramp (it has detail), or one flat colour, written to `path`.
pub(crate) fn photo(path: &Path, flat: bool) -> Result<(), Box<dyn std::error::Error>> {
    let (height, width) = (24, 32);
    let values: Vec<f32> = (0..height * width)
        .flat_map(|i| {
            let level = if flat {
                0.5
            } else {
                f32::from(u8::try_from(i % width * 8).unwrap_or(255)) / 255.0
            };
            [level, 0.5, 1.0 - level]
        })
        .collect();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write(path, &Image::new(height, width, &values)?)?;
    Ok(())
}

/// A project in `root` with one recipe, `warm`, that needs no model, and a photo `a.png`.
pub(crate) fn project(root: &Path, flat: bool) -> Result<PathBuf, Box<dyn std::error::Error>> {
    photo(&root.join("photos/a.png"), flat)?;
    std::fs::create_dir_all(root.join("recipes"))?;
    std::fs::write(
        root.join("recipes/warm.toml"),
        "name = \"warm\"\n\n[[step]]\neffect = \"grade\"\nexposure = 0.2\n",
    )?;
    let file = root.join("bt-image.toml");
    std::fs::write(
        &file,
        "photos = [\"photos/*.png\"]\nrecipes = [\"recipes\"]\noutput = \"out\"\n",
    )?;
    Ok(file)
}

/// The file `path` (made with its folders) holding `text`.
pub(crate) fn text(path: &Path, text: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)
}

/// A map picture of `path`: the left half `left`, the right half `right`, in grey levels 0 or 255.
pub(crate) fn halves(path: &Path, (height, width): (usize, usize), (left, right): (f32, f32)) -> Outcome {
    let values: Vec<f32> = (0..height * width)
        .flat_map(|i| {
            let level = if i % width < width / 2 { left } else { right };
            [level; 3]
        })
        .collect();
    write(path, &Image::new(height, width, &values)?)?;
    Ok(())
}

/// The pixel `(row, column)` of the picture file `path`, its channels in 0..1.
pub(crate) fn pixel(path: &Path, row: usize, column: usize) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let image = bullterrier_image::read(path)?;
    let (channels, width) = (image.channels(), image.width());
    let values = image.into_values();
    Ok(values[(row * width + column) * channels..][..channels].to_vec())
}
