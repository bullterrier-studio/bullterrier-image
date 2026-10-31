//! Writes the effect list the core compiles in: one module per file in `src/effects/`, and the function that makes every
//! effect. Adding an effect adds its file and edits nothing else.

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

/// The Rust source of the list: a module per effect file, the `EffectParams` impl that names the effect of its `Params`,
/// and `builtin()`, which makes each effect from its `META`, its `Params` (read from the parameters) and its `render`.
fn table(files: &[(String, PathBuf)]) -> Result<String, fmt::Error> {
    let mut code = String::new();
    for (name, path) in files {
        writeln!(
            code,
            "#[path = {:?}]\npub mod {name};\nimpl crate::params::EffectParams for {name}::Params {{\n    const EFFECT: &'static str = {name:?};\n}}",
            path.display().to_string()
        )?;
    }
    writeln!(code, "/// Every effect the core implements.")?;
    // full paths: an effect module named like `crate` or a std item must not stand in its place
    writeln!(
        code,
        "pub(crate) fn builtin() -> Vec<::std::sync::Arc<dyn crate::effect::Effect>> {{\n    vec!["
    )?;
    for (name, _) in files {
        writeln!(
            code,
            "        ::std::sync::Arc::new(crate::effect::Native::new({name:?}, &{name}::META, {name}::render)),"
        )?;
    }
    writeln!(code, "    ]\n}}")?;
    Ok(code)
}
