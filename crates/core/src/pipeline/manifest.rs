//! The record of a run (`manifest.json`) and the report page (`index.html`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::atomic::write_whole;
use crate::Error;

/// What happened to one photo x recipe pair.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Status {
    /// Made in this run.
    #[default]
    Done,
    /// Up to date: made by an earlier run, the files still there.
    Skipped,
    /// Failed: the pair has an error.
    Failed,
    /// A dry run reports that it would be made.
    WouldRun,
}

impl Status {
    /// The status as the record and the report write it: `done`, `skipped`, `failed`, `would-run`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
            Self::WouldRun => "would-run",
        }
    }
}

/// What happened to one photo x recipe pair.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    /// The photo's file name.
    pub photo: String,
    /// The recipe's name.
    pub recipe: String,
    /// What happened.
    pub status: Status,
    /// The files written, relative to the output folder, with `/`.
    #[serde(default)]
    pub outputs: Vec<String>,
    /// What the automatic checks flagged.
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Why it failed.
    #[serde(default)]
    pub error: Option<String>,
    /// The contact sheet of every step, relative to the output folder.
    #[serde(default)]
    pub steps: Option<String>,
    /// How long it took, seconds.
    #[serde(default)]
    pub seconds: f64,
    /// The hash of everything that shaped the result, compared on the next run.
    #[serde(default)]
    pub fingerprint: String,
}

/// The file as written.
#[derive(Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    generated: String,
    #[serde(default)]
    code: String,
    items: Vec<Item>,
}

/// The pairs `output/manifest.json` records. A missing or damaged record, or one of which a single item is damaged, is no
/// record: everything is new.
#[must_use]
pub fn read_manifest(output: &Path) -> Vec<Item> {
    std::fs::read(output.join("manifest.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Manifest>(&bytes).ok())
        .map_or_else(Vec::new, |manifest| manifest.items)
}

/// Writes `output/manifest.json`: this run's `items`, and the `previous` results of the pairs it has not reached or left
/// alone (`--only`) for the project's photos (`photos`, by file name) and recipes (`recipes`, by name, in their order).
/// `code` is the identity of what made them and `generated` the local time of the run, which the caller says (the core
/// reads no clock). Returns the record: sorted by photo, then by the recipes' order.
///
/// # Errors
/// [`Error::ImageWrite`] naming the file if it cannot be written.
pub fn write_record(
    output: &Path,
    items: &[Item],
    previous: &[Item],
    photos: &[&str],
    recipes: &[&str],
    code: &str,
    generated: &str,
) -> Result<Vec<Item>, Error> {
    let mut record: Vec<Item> = items.to_vec();
    record.extend(
        previous
            .iter()
            .filter(|old| {
                !items
                    .iter()
                    .any(|item| item.photo == old.photo && item.recipe == old.recipe)
                    && photos.contains(&old.photo.as_str())
                    && recipes.contains(&old.recipe.as_str())
            })
            .cloned(),
    );
    let place = |item: &Item| {
        recipes
            .iter()
            .position(|name| *name == item.recipe)
            .unwrap_or(usize::MAX)
    };
    record.sort_by(|one, other| one.photo.cmp(&other.photo).then(place(one).cmp(&place(other))));
    let manifest = Manifest {
        generated: generated.to_owned(),
        code: code.to_owned(),
        items: record.clone(),
    };
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|error| Error::ImageWrite(format!("cannot write the record: {error}")))?;
    let target = output.join("manifest.json");
    write_whole(&target, text.as_bytes())
        .map_err(|error| Error::ImageWrite(format!("cannot write {}: {error}", target.display())))?;
    Ok(record)
}

/// `text` for a page: `&`, `<`, `>`, `"` and `'` as entities.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// How a page in `folder` refers to `target`: a relative path where there is one, else (on another drive, on Windows) a `file:`
/// address.
fn link(target: &Path, folder: &Path) -> String {
    let (target, folder) = (
        std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf()),
        std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf()),
    );
    let (mut t, mut f) = (target.components().peekable(), folder.components().peekable());
    while t.peek().is_some() && t.peek() == f.peek() {
        t.next();
        f.next();
    }
    let up = f.count();
    let rest: Vec<String> = t.map(|part| part.as_os_str().to_string_lossy().into_owned()).collect();
    let root_differs = target.components().next() != folder.components().next();
    if root_differs {
        return format!("file://{}", target.to_string_lossy().replace('\\', "/"));
    }
    std::iter::repeat_n("..".to_owned(), up)
        .chain(rest)
        .collect::<Vec<_>>()
        .join("/")
}

/// One result: the picture, its name and status, and anything the checks or the run complained about.
fn card(item: &Item) -> String {
    let shown = item.outputs.first().map_or_else(String::new, |first| escape(first));
    let media = if item.outputs.is_empty() {
        String::new()
    } else {
        format!("<a href=\"{shown}\"><img src=\"{shown}\" loading=\"lazy\"></a>")
    };
    let notes = item
        .warnings
        .iter()
        .map(|warning| format!("<p class=\"warn\">{}</p>", escape(warning)))
        .chain(
            item.error
                .iter()
                .map(|error| format!("<p class=\"error\">{}</p>", escape(error))),
        )
        .collect::<Vec<_>>()
        .concat();
    let steps = item
        .steps
        .as_ref()
        .map_or_else(String::new, |sheet| format!(" <a href=\"{}\">steps</a>", escape(sheet)));
    let status = item.status.name();
    let caption = format!("<b>{}</b> <span>{status}</span>{steps}{notes}", escape(&item.recipe));
    format!("<figure class=\"{status}\">{media}<figcaption>{caption}</figcaption></figure>")
}

/// The page, with `{summary}` and `{body}` to fill.
const PAGE: &str = "<!doctype html>
<meta charset=\"utf-8\"><title>bullterrier-image report</title>
<style>
 body{margin:0;padding:24px;background:#101114;color:#d8dae0;font:15px/1.4 system-ui,sans-serif}
 h1{font-size:20px;margin:0 0 4px} p.summary{margin:0 0 24px;color:#8b8f9a}
 h2{font-size:16px;display:flex;align-items:center;gap:10px;margin:28px 0 12px}
 img.src{height:36px;border-radius:4px}
 .grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));gap:14px}
 figure{margin:0;background:#181a20;border-radius:8px;overflow:hidden;border:1px solid #23262e}
 figure.failed{border-color:#a33} figure img{width:100%;display:block;background:#000}
 figcaption{padding:10px 12px} figcaption span{color:#8b8f9a;font-size:12px;margin-left:6px}
 figcaption a{color:#7fb2ff;font-size:12px;margin-left:8px}
 .warn{margin:6px 0 0;color:#e0b050;font-size:13px} .error{margin:6px 0 0;color:#f07070;font-size:13px}
</style>
<h1>bullterrier-image report</h1><p class=\"summary\">{summary}</p>{body}
";

/// A single static page: every result, grouped by photo (the photos' files are `photos`, which `output` links to), with its
/// warnings and errors.
#[must_use]
pub fn report_html(photos: &[PathBuf], output: &Path, items: &[Item]) -> String {
    let count = |status: Status| items.iter().filter(|item| item.status == status).count();
    let flagged = items.iter().filter(|item| !item.warnings.is_empty()).count();
    let summary = format!(
        "{} done, {} up to date, {} failed, {flagged} with warnings",
        count(Status::Done),
        count(Status::Skipped),
        count(Status::Failed)
    );
    let mut seen: Vec<&str> = Vec::new();
    for item in items {
        if !seen.contains(&item.photo.as_str()) {
            seen.push(&item.photo);
        }
    }
    let sections: Vec<String> = seen
        .into_iter()
        .map(|photo| {
            let cards: String = items.iter().filter(|item| item.photo == photo).map(card).collect();
            let source = photos.iter().find(|path| path.file_name().is_some_and(|name| name.to_string_lossy() == photo));
            let thumbnail = source.map_or_else(String::new, |path| escape(&link(path, output)));
            format!(
                "<section><h2><img class=\"src\" src=\"{thumbnail}\" alt=\"\"> {}</h2><div class=\"grid\">{cards}</div></section>",
                escape(photo)
            )
        })
        .collect();
    let body = sections.concat();
    PAGE.replace("{summary}", &summary).replace("{body}", &body)
}
