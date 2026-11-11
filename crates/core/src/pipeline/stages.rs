//! The stages of a render shown for a person: every stage as a picture with a label, saved one by one and as one sheet.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::ops::glyphs::text_row;
use crate::ops::length::{float, whole};
use crate::pipeline::export::inside;
use crate::pipeline::sheet_files::save_image;
use crate::recipe::Entry;
use crate::{Error, Image, Labels, Layer, Recipe, Stage, Tile, contact_sheet};

/// The share of full brightness a picture is dimmed to under the lines and numbers of its pieces, so they stand out on any photo.
const DIM: f32 = 0.35; // constant: dim enough for white to show on a white photo
/// A digit is this share of the long side wide, at least [`DIGIT_LEAST`] px.
const DIGIT_SHARE: f64 = 1.0 / 32.0; // constant: big enough to read, small enough to sit in a piece
/// The smallest a digit is drawn, px.
const DIGIT_LEAST: usize = 6; // constant: the smallest a digit can be read at
/// The longest a step's description is, characters, and the longest one value of it is.
const DESCRIPTION: usize = 56; // constant: fits a label under a tile
const VALUE: usize = 18; // constant: a value longer than this is cut to 15 and `...`
/// The stages of a sheet per row, and the size of a tile on it, px.
const COLUMNS: usize = 4; // constant: the layout of the steps sheet
const TILE: usize = 420; // constant: the layout of the steps sheet

/// The text of `value` as a person reads a parameter: numbers as written, text in single quotes, lists in brackets.
fn shown(value: &Value) -> String {
    match value {
        Value::String(text) => format!("'{text}'"),
        Value::Bool(flag) => if *flag { "True" } else { "False" }.to_owned(),
        Value::Null => "None".to_owned(),
        Value::Array(items) => format!("[{}]", items.iter().map(shown).collect::<Vec<_>>().join(", ")),
        other => other.to_string(),
    }
}

/// `text` cut to `width` characters, the last one `…`.
fn cut(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_owned()
    } else {
        text.chars()
            .take(width - 1)
            .chain(std::iter::once('\u{2026}'))
            .collect()
    }
}

/// What a layer adds to `text`: its name before it, then its blend mode, its mask and its opacity if it is not full.
fn described(text: &str, name: Option<&str>, blend: &str, mask: Option<&str>, opacity: f64, width: usize) -> String {
    let named = name.map_or_else(|| text.to_owned(), |name| format!("{name}: {text}"));
    let blended = if blend == "normal" {
        String::new()
    } else {
        format!(" {blend}")
    };
    let masked = mask.map_or_else(String::new, |mask| format!(" on {mask}"));
    let faded = if opacity < 1.0 {
        format!(" x{opacity}")
    } else {
        String::new()
    };
    cut(&format!("{named}{blended}{masked}{faded}"), width)
}

/// A short readable form of a step, e.g. `gradient(colors=['#000000']) on background` or `bloom() screen x0.8`, cut to 56
/// characters.
fn describe_step(step: &crate::Step) -> String {
    let values: Vec<String> = step
        .params
        .iter()
        .map(|(key, value)| {
            let text = shown(value);
            let text = if text.chars().count() <= VALUE {
                text
            } else {
                format!("{}...", text.chars().take(15).collect::<String>())
            };
            format!("{key}={text}")
        })
        .collect();
    let opacity = step
        .opacity
        .unwrap_or_else(|| crate::registry::effect(&step.effect).map_or(1.0, |effect| effect.opacity()));
    described(
        &format!("{}({})", step.effect, values.join(", ")),
        step.name.as_deref(),
        step.blend_mode.name(),
        step.mask.as_deref(),
        opacity,
        DESCRIPTION,
    )
}

/// The same for either kind of step.
fn describe(entry: &Entry) -> String {
    match entry {
        Entry::Step(step) => describe_step(step),
        Entry::Each(each) => {
            let looks: Vec<String> = each
                .looks
                .iter()
                .map(|look| look.as_ref().map_or_else(|| "none".to_owned(), describe_step))
                .collect();
            described(
                &format!("each {} [{}]", each.each, looks.join(", ")),
                each.name.as_deref(),
                each.blend_mode.name(),
                each.mask.as_deref(),
                each.opacity.unwrap_or(1.0),
                DESCRIPTION,
            )
        }
    }
}

/// `layer` as a grey picture.
fn grey(layer: &Layer) -> Result<Image, Error> {
    let values: Vec<f32> = layer.values().iter().flat_map(|&value| [value; 3]).collect();
    Image::new(layer.height(), layer.width(), &values)
}

/// `photo` dimmed with a white line on each side of every border between the pieces of `labels` and, in each piece, its number
/// in white, centred on the pixel of the piece nearest its middle (a number wider than the picture is cut off at its edge).
fn number_pieces(photo: &Image, labels: &Labels) -> Result<Image, Error> {
    let (height, width) = (labels.height(), labels.width());
    let piece = labels.values();
    let mut out: Vec<[f32; 3]> = photo
        .as_picture()
        .values()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| p.map(|c| c * DIM))
        .collect();
    if out.len() != piece.len() {
        return Err(Error::Inputs(
            "the photo and the pieces are of different sizes".to_owned(),
        ));
    }
    let beside = |index: usize| {
        let (row, column) = (index / width, index % width);
        (column > 0 && piece[index - 1] != piece[index])
            || (column + 1 < width && piece[index + 1] != piece[index])
            || (row > 0 && piece[index - width] != piece[index])
            || (row + 1 < height && piece[index + width] != piece[index])
    };
    for (index, pixel) in out.iter_mut().enumerate() {
        if beside(index) {
            *pixel = [1.0; 3];
        }
    }
    let cell = whole(float(height.max(width)) * DIGIT_SHARE).max(DIGIT_LEAST);
    for number in 0..labels.pieces() {
        let wanted = i32::try_from(number).unwrap_or(i32::MAX);
        let mine: Vec<usize> = (0..piece.len()).filter(|&index| piece[index] == wanted).collect();
        if mine.is_empty() {
            continue;
        }
        let count = float(mine.len());
        let mean_row = mine.iter().map(|&i| float(i / width)).sum::<f64>() / count;
        let mean_column = mine.iter().map(|&i| float(i % width)).sum::<f64>() / count;
        let middle = mine
            .iter()
            .copied()
            .min_by(|&a, &b| {
                let distance =
                    |i: usize| (float(i / width) - mean_row).powi(2) + (float(i % width) - mean_column).powi(2);
                distance(a).total_cmp(&distance(b))
            })
            .unwrap_or(mine[0]);
        let (ink, ink_width, ink_height) = text_row(&number.to_string(), cell);
        let top = isize::try_from(middle / width).unwrap_or(0) - isize::try_from(ink_height / 2).unwrap_or(0);
        let left = isize::try_from(middle % width).unwrap_or(0) - isize::try_from(ink_width / 2).unwrap_or(0);
        for row in 0..ink_height {
            for column in 0..ink_width {
                let (y, x) = (
                    top + isize::try_from(row).unwrap_or(0),
                    left + isize::try_from(column).unwrap_or(0),
                );
                if y < 0
                    || x < 0
                    || y >= isize::try_from(height).unwrap_or(0)
                    || x >= isize::try_from(width).unwrap_or(0)
                {
                    continue;
                }
                let covered = ink[row * ink_width + column];
                let at = usize::try_from(y).unwrap_or(0) * width + usize::try_from(x).unwrap_or(0);
                out[at] = out[at].map(|channel| channel * (1.0 - covered) + covered);
            }
        }
    }
    Ok(Image::from_pixels(height, width, out))
}

/// A file-name-friendly name for a stage label: `matte`, `depth`, the step's effect, or the label made plain.
fn stage_name(label: &str) -> String {
    if label.starts_with("subject matte") {
        return "matte".to_owned();
    }
    if label.starts_with("depth (") {
        return "depth".to_owned();
    }
    let digits = label.chars().take_while(char::is_ascii_digit).count();
    if digits > 0
        && let Some(rest) = label[digits..].strip_prefix(". ")
    {
        let word: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if !word.is_empty() {
            return word;
        }
    }
    let mut plain = String::new();
    for character in label.to_lowercase().chars() {
        let kept = if character.is_ascii_lowercase() || character.is_ascii_digit() {
            character
        } else {
            '-'
        };
        if !(kept == '-' && plain.ends_with('-')) {
            plain.push(kept);
        }
    }
    plain.trim_matches('-').to_owned()
}

/// The label and picture of `stage` of a render of `recipe`: a map as grey, a partition as the dimmed photo with its borders
/// and the number of each piece, a step by its number and what it does.
fn presented(recipe: &Recipe, input: &Image, stage: Stage) -> Result<(String, Image), Error> {
    Ok(match stage {
        Stage::Input(image) => ("input".to_owned(), image),
        Stage::Subject(layer) => ("subject matte (white = subject)".to_owned(), grey(&layer)?),
        Stage::Depth(layer) => ("depth (white = nearest)".to_owned(), grey(&layer)?),
        Stage::Selection { name, layer } => (format!("region '{name}' (white = chosen)"), grey(&layer)?),
        Stage::Partition { name, labels } => (
            format!("region '{name}' ({} pieces, numbered)", labels.pieces()),
            number_pieces(input, &labels)?,
        ),
        Stage::Step { number, image } => {
            let what = recipe
                .steps
                .get(number.saturating_sub(1))
                .map_or_else(String::new, describe);
            (format!("{number}. {what}"), image)
        }
        Stage::CutOut(image) => {
            let how = serde_json::to_value(&recipe.frame)
                .ok()
                .and_then(|frame| frame.get("transparent").and_then(Value::as_str).map(str::to_owned))
                .unwrap_or_default();
            (format!("{}. transparent('{how}')", recipe.steps.len() + 1), image)
        }
    })
}

/// Saves every stage of a render of `recipe` as `NN-name.jpg` in `folder` (`.png` for a stage with transparency, which a JPEG
/// cannot hold), plus `_sheet.jpg` showing them all in order, a stage with transparency over a checkerboard. Every file must
/// lie inside `output`. Returns the sheet, the one picture to look at to see what each step did.
///
/// # Errors
/// [`Error::Project`] for a file outside `output`; the errors of writing a picture.
pub fn write_stages(
    recipe: &Recipe,
    stages: Vec<Stage>,
    folder: &Path,
    output: &Path,
    quality: u8,
) -> Result<PathBuf, Error> {
    let input = match stages.first() {
        Some(Stage::Input(image)) => image.clone(),
        _ => return Err(Error::Inputs("a trace starts with the photo".to_owned())),
    };
    let mut shown_stages = Vec::new();
    for stage in stages {
        shown_stages.push(presented(recipe, &input, stage)?);
    }
    for (number, (label, image)) in shown_stages.iter().enumerate() {
        let extension = if image.channels() == 4 { "png" } else { "jpg" };
        let target = inside(
            output,
            &folder.join(format!("{number:02}-{}.{extension}", stage_name(label))),
        )?;
        save_image(&target, image, quality)?;
    }
    let tiles: Vec<Tile<'_>> = shown_stages
        .iter()
        .map(|(label, image)| Tile::new(label, image))
        .collect();
    let sheet = contact_sheet(&tiles, COLUMNS.min(tiles.len()), TILE)?;
    let target = inside(output, &folder.join("_sheet.jpg"))?;
    save_image(&target, &sheet, quality)?;
    Ok(target)
}
