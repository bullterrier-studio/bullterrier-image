//! Writes what the core compiles in: the effect list (one module per file in `src/effects/`, and the function that makes
//! every effect; adding an effect adds its file and edits nothing else) and the text of the presets and looks that ship
//! with it (`data/presets/*.toml`, `data/looks/*.toml`).

use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use sha2::{Digest, Sha256};

fn main() -> io::Result<()> {
    let manifest =
        env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| io::Error::other("cargo sets CARGO_MANIFEST_DIR"))?;
    let out = env::var_os("OUT_DIR").ok_or_else(|| io::Error::other("cargo sets OUT_DIR"))?;
    let folder = Path::new(&manifest).join("src").join("effects");
    println!("cargo::rerun-if-changed={}", folder.display());
    let code = table(&effect_files(&folder)?).map_err(io::Error::other)?;
    fs::write(PathBuf::from(&out).join("effects.rs"), code)?;
    let mut shipped = String::new();
    for (name, kind) in [("presets", "PRESETS"), ("looks", "LOOKS")] {
        let data = Path::new(&manifest).join("data").join(name);
        println!("cargo::rerun-if-changed={}", data.display());
        writeln!(shipped, "pub(crate) const {kind}: &[(&str, &str)] = &[").map_err(io::Error::other)?;
        for (stem, path) in toml_files(&data)? {
            writeln!(
                shipped,
                "    ({stem:?}, include_str!({:?})),",
                path.display().to_string()
            )
            .map_err(io::Error::other)?;
        }
        writeln!(shipped, "];").map_err(io::Error::other)?;
    }
    fs::write(PathBuf::from(&out).join("shipped.rs"), shipped)?;
    fs::write(PathBuf::from(out).join("identity.rs"), identity(Path::new(&manifest))?)
}

/// The Rust source of the core's identity: a SHA-256 of every file that can change what the core makes (its sources, the
/// data that ships with it, its manifest, and the workspace's lock file where there is one: the code of the crates it
/// builds with), and the list of those files, by relative path, in order.
fn identity(manifest: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    for folder in ["src", "data"] {
        println!("cargo::rerun-if-changed={}", manifest.join(folder).display());
        collect(&manifest.join(folder), manifest, &mut files)?;
    }
    files.push((String::from("Cargo.toml"), manifest.join("Cargo.toml")));
    let lock = manifest.join("..").join("..").join("Cargo.lock");
    if lock.is_file() {
        println!("cargo::rerun-if-changed={}", lock.display());
        files.push((String::from("../../Cargo.lock"), lock));
    }
    files.sort();
    let mut digest = Sha256::new();
    let mut listed = String::new();
    for (relative, path) in &files {
        digest.update(relative.as_bytes());
        digest.update([0]);
        digest.update(fs::read(path)?);
        digest.update([0]);
        writeln!(listed, "    {relative:?},").map_err(io::Error::other)?;
    }
    let mut code = String::new();
    writeln!(
        code,
        "pub(crate) const SOURCE_DIGEST: &str = {:?};",
        format!("{:x}", digest.finalize())
    )
    .map_err(io::Error::other)?;
    writeln!(code, "pub(crate) const SOURCE_FILES: &[&str] = &[\n{listed}];").map_err(io::Error::other)?;
    Ok(code)
}

/// Every file below `folder` as `(path relative to root with `/`, full path)`.
fn collect(folder: &Path, root: &Path, files: &mut Vec<(String, PathBuf)>) -> io::Result<()> {
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, root, files)?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            files.push((relative.to_string_lossy().replace('\\', "/"), path.clone()));
        }
    }
    Ok(())
}

/// The `.toml` files in `folder` as `(name without the extension, path)`, sorted by name.
fn toml_files(folder: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "toml")
            && let Some(name) = path.file_stem().and_then(|stem| stem.to_str())
        {
            files.push((name.to_owned(), path.clone()));
        }
    }
    files.sort();
    Ok(files)
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
/// and `builtin()`, which makes each effect from its `META`, its `Params` (read from the parameters), its `footprint`
/// (`fn(&Params) -> Footprint`) and its `render` (`fn(&Picture, &Params, &Inputs) -> Image`).
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
            "        ::std::sync::Arc::new(crate::effect::Native::new({name:?}, &{name}::META, {name}::footprint, {name}::render)),"
        )?;
    }
    writeln!(code, "    ]\n}}")?;
    Ok(code)
}
