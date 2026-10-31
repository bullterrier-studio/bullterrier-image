//! Writes the effect table the core compiles in: one module per file in `src/effects/`, and the list of every effect
//! by name. Adding an effect adds its file and edits nothing else.

use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};
use std::{env, fs, io};

fn main() -> io::Result<()> {
    let manifest =
        env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| io::Error::other("cargo sets CARGO_MANIFEST_DIR"))?;
    let out = env::var_os("OUT_DIR").ok_or_else(|| io::Error::other("cargo sets OUT_DIR"))?;
    let folder = Path::new(&manifest).join("src").join("effects");
    println!("cargo::rerun-if-changed={}", folder.display());
    let code = table(&effect_files(&folder)?).map_err(io::Error::other)?;
    fs::write(PathBuf::from(out).join("effects.rs"), code)
}

/// The effect files in `folder` (each `<name>.rs`), sorted by name; none if the folder does not exist.
fn effect_files(folder: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    if !folder.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "rs")
            && let Some(name) = path.file_stem().and_then(|stem| stem.to_str())
        {
            files.push((name.to_owned(), path.clone()));
        }
    }
    files.sort();
    Ok(files)
}

/// The Rust source of the table: a `pub(crate)` module per effect file, and `EFFECTS`, each name with the function that runs
/// it (it reads the effect's `Params` from the parameters and calls its `render`) and the one that describes its
/// `Params`.
fn table(files: &[(String, PathBuf)]) -> Result<String, fmt::Error> {
    let mut code = String::new();
    for (name, path) in files {
        writeln!(
            code,
            "#[path = {:?}]\npub(crate) mod {name};",
            path.display().to_string()
        )?;
    }
    writeln!(
        code,
        "/// Every effect the core implements, sorted by name, with the function that runs it and the one that describes its\n/// parameters."
    )?;
    writeln!(code, "pub(crate) static EFFECTS: &[(&str, Run, Describe)] = &[")?;
    for (name, _) in files {
        writeln!(code, "    ({name:?}, |picture, params| {{")?;
        // `::serde_json`, the crate: an effect module named `serde_json` would otherwise stand in its place
        writeln!(code, "        let params = ::serde_json::from_value(params)")?;
        writeln!(
            code,
            "            .map_err(|source| Error::Params {{ effect: {name:?}, source }})?;"
        )?;
        writeln!(code, "        Ok({name}::render(picture, &params))")?;
        writeln!(code, "    }}, <{name}::Params as crate::params::Declared>::table),")?;
    }
    writeln!(code, "];")?;
    Ok(code)
}
