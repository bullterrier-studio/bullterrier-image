//! Writes what the core compiles in: the effect list (one module per file in `src/effects/`, and the function that makes
//! every effect; adding an effect adds its file and edits nothing else) and the text of the presets and looks that ship
//! with it (`data/presets/*.toml`, `data/looks/*.toml`).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use sha2::{Digest, Sha256};

fn main() -> io::Result<()> {
    let manifest =
        env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| io::Error::other("cargo sets CARGO_MANIFEST_DIR"))?;
    let out = env::var_os("OUT_DIR").ok_or_else(|| io::Error::other("cargo sets OUT_DIR"))?;
    let folder = Path::new(&manifest).join("src").join("effects");
    println!("cargo::rerun-if-changed={}", folder.display());
    let code = table(&effect_files(&folder)?).map_err(|error| io::Error::other(error.to_string()))?;
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
    let palettes = Path::new(&manifest).join("data").join("palettes.toml");
    println!("cargo::rerun-if-changed={}", palettes.display());
    fs::write(PathBuf::from(&out).join("palettes.rs"), palette_enum(&palettes)?)?;
    fs::write(PathBuf::from(out).join("identity.rs"), identity(Path::new(&manifest))?)
}

/// A palette error, naming the palette and the file.
fn palette_error(name: &str, problem: &str) -> io::Error {
    io::Error::other(format!("data/palettes.toml: palette [{name}]: {problem}"))
}

/// The variant of the palette `name`, or why the name will not do: lowercase words (letters, then letters and digits)
/// joined by single `_` (a word that starts with a digit would not read back as the same name from the variant), and
/// none of the built-in choices.
fn variant_of(name: &str) -> io::Result<String> {
    const BUILT_IN: [&str; 7] = ["photo", "cube2", "cube3", "cube4", "cube5", "cube6", "custom"];
    let word = |word: &str| {
        word.starts_with(|c: char| c.is_ascii_lowercase())
            && word.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    };
    if !name.split('_').all(word) {
        return Err(palette_error(
            name,
            "a name is lowercase words joined by single `_`, each word a letter then letters or digits (like `zx_spectrum`)",
        ));
    }
    if BUILT_IN.contains(&name) {
        return Err(palette_error(
            name,
            "that name is a built-in choice of `palette` (photo, cube2 to cube6, custom)",
        ));
    }
    Ok(name
        .split('_')
        .map(|word| word[..1].to_uppercase() + &word[1..])
        .collect())
}

/// The Rust source of `dither`'s `Palette`: the named palettes of `data/palettes.toml` (a variant each, documented by its
/// description) between `photo` and the cubes, which are formulas, and `custom`; and `Palette::colours`, their 0xrrggbb codes.
fn palette_enum(path: &Path) -> io::Result<String> {
    let table: toml::Table = toml::from_str(&fs::read_to_string(path)?).map_err(io::Error::other)?;
    let (mut variants, mut arms, mut named, mut described) =
        (String::new(), String::new(), String::new(), String::new());
    let mut variants_seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (name, palette) in &table {
        let field = |key: &str| {
            palette
                .get(key)
                .ok_or_else(|| io::Error::other(format!("palette {name}: no {key}")))
        };
        let description = field("description")?.as_str().unwrap_or_default();
        let variant = variant_of(name)?;
        if !variants_seen.insert(variant.clone()) {
            return Err(palette_error(name, "its variant name is taken by another palette"));
        }
        let codes: Vec<String> = field("colors")?
            .as_array()
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .map(|colour| {
                let hex = colour
                    .as_str()
                    .and_then(|text| text.strip_prefix('#'))
                    .unwrap_or_default();
                match (hex.len(), u32::from_str_radix(hex, 16)) {
                    (6, Ok(code)) => Ok(format!("0x{:04x}_{:04x}", code >> 16, code & 0xffff)),
                    _ => Err(io::Error::other(format!("palette {name}: {colour:?} is not '#rrggbb'"))),
                }
            })
            .collect::<io::Result<_>>()?;
        if codes.len() < 2 || codes.len() > 256 {
            return Err(io::Error::other(format!(
                "palette {name}: 2 to 256 colours, got {}",
                codes.len()
            )));
        }
        writeln!(variants, "    #[doc = {description:?}]\n    {variant},").map_err(io::Error::other)?;
        writeln!(arms, "            Self::{variant} => &[{}],", codes.join(", ")).map_err(io::Error::other)?;
        writeln!(named, "        Self::{variant},").map_err(io::Error::other)?;
        writeln!(described, "            Self::{variant} => {description:?},").map_err(io::Error::other)?;
    }
    Ok(format!(
        "/// Where the colours come from.
#[expect(clippy::doc_markdown, reason = \"the descriptions are prose written in data\")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Palette {{
    /// Colours found in the picture itself (k-means).
    Photo,
{variants}    /// Every mix of 2 evenly spaced levels of red, green and blue: 8 colours.
    Cube2,
    /// Every mix of 3 levels: 27 colours.
    Cube3,
    /// Every mix of 4 levels: 64 colours.
    Cube4,
    /// Every mix of 5 levels: 125 colours.
    Cube5,
    /// Every mix of 6 levels: 216 colours.
    Cube6,
    /// The colours of `custom`.
    Custom,
}}

impl Palette {{
    /// The palettes of `data/palettes.toml`, in the file's order (`photo`, `custom` and the cubes are not among them). A
    /// palette's name is `Choice::name`.
    pub const NAMED: &'static [Self] = &[
{named}    ];

    /// What the palette is, in a sentence; none for `photo`, `custom` and the cubes.
    #[must_use]
    pub const fn description(self) -> Option<&'static str> {{
        Some(match self {{
{described}            _ => return None,
        }})
    }}

    /// The colours of a palette of `Palette::NAMED`, as `'#rrggbb'` in the order the data lists them; none for the others.
    #[must_use]
    pub fn colours(self) -> Option<Vec<String>> {{
        self.named()
            .map(|codes| codes.iter().map(|code| format!(\"#{{code:06x}}\")).collect())
    }}

    /// The 0xrrggbb codes of a named palette; none for the others.
    const fn named(self) -> Option<&'static [u32]> {{
        Some(match self {{
{arms}            _ => return None,
        }})
    }}
}}
"
    ))
}

/// The Rust source of the core's identity: a SHA-256 of every file that can change what the core makes (its sources, the
/// data that ships with it, its manifest, and the workspace's lock file where there is one: the code of the crates it
/// builds with), and the list of those files, by relative path, in order.
fn identity(manifest: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    for folder in ["src", "data"] {
        println!("cargo::rerun-if-changed={}", manifest.join(folder).display());
        collect(&manifest.join(folder), manifest, &mut files, "")?;
    }
    files.push((String::from("Cargo.toml"), manifest.join("Cargo.toml")));
    files.push((String::from("build.rs"), manifest.join("build.rs")));
    for folder in path_crates(manifest)? {
        println!("cargo::rerun-if-changed={}", manifest.join(&folder).display());
        collect(
            &manifest.join(&folder).join("src"),
            &manifest.join(&folder),
            &mut files,
            &folder,
        )?;
        for name in ["Cargo.toml", "build.rs"] {
            let path = manifest.join(&folder).join(name);
            if path.is_file() {
                files.push((format!("{folder}/{name}"), path));
            }
        }
    }
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
fn collect(folder: &Path, root: &Path, files: &mut Vec<(String, PathBuf)>, prefix: &str) -> io::Result<()> {
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, root, files, prefix)?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            let relative = relative.to_string_lossy().replace('\\', "/");
            let named = if prefix.is_empty() {
                relative
            } else {
                format!("{prefix}/{relative}")
            };
            files.push((named, path.clone()));
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
/// (`fn(&Params) -> Footprint`), its `spread` (`fn(&Params) -> Spread`, if the file declares one: an effect that paints
/// nothing outward has none), its `check` (`fn(&Params, &Inputs) -> Result<(), Error>`, if it declares one: a run it
/// refuses), its `data` (`fn(&Picture, &Params, &Inputs) -> Result<Data, Error>`, if it makes some beside its picture) and its `render` (`fn(&Picture, &Params, &Inputs) -> Image`).
fn table(files: &[(String, PathBuf)]) -> Result<String, Box<dyn std::error::Error>> {
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
    for (name, path) in files {
        let spread = if fs::read_to_string(path)?.contains("\npub(crate) fn spread(") {
            format!("Some({name}::spread)")
        } else {
            "None".to_owned()
        };
        let check = if fs::read_to_string(path)?.contains("\npub(crate) fn check(") {
            format!(".checked({name}::check)")
        } else {
            String::new()
        };
        let data = if fs::read_to_string(path)?.contains("\npub(crate) fn data(") {
            format!(".with_data({name}::data)")
        } else {
            String::new()
        };
        writeln!(
            code,
            "        ::std::sync::Arc::new(crate::effect::Native::new({name:?}, &{name}::META, {name}::footprint, {spread}, {name}::render){check}{data}),"
        )?;
    }
    writeln!(code, "    ]\n}}")?;
    Ok(code)
}

/// The folders (relative to the core's, as the manifests write them) of the path crates the core is built from: a dependency
/// with a `path`, or `workspace = true` for one the workspace gives a `path`. Their code is part of the core's, and a change to
/// it does not change `Cargo.lock`.
fn path_crates(manifest: &Path) -> io::Result<Vec<String>> {
    let read =
        |path: PathBuf| -> io::Result<toml::Table> { fs::read_to_string(path)?.parse().map_err(io::Error::other) };
    let core = read(manifest.join("Cargo.toml"))?;
    let workspace = read(manifest.join("..").join("..").join("Cargo.toml")).unwrap_or_default();
    let shared = workspace
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(toml::Value::as_table);
    let mut folders = Vec::new();
    for (name, dependency) in core
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
    {
        let table = dependency.as_table();
        let own = table.and_then(|t| t.get("path")).and_then(toml::Value::as_str);
        let given = table
            .filter(|t| t.get("workspace").and_then(toml::Value::as_bool) == Some(true))
            .and_then(|_| shared?.get(name)?.get("path")?.as_str());
        match (own, given) {
            (Some(path), _) => folders.push(path.trim_start_matches("./").to_owned()),
            (None, Some(path)) => folders.push(format!("../../{path}")),
            _ => {}
        }
    }
    folders.sort();
    Ok(folders)
}
