//! What the integration tests share. Each area has its own module; each test module uses some of them. (`memory.rs`,
//! a binary of its own, includes this module by path.)
#![allow(dead_code, reason = "each test module uses a different part")]

pub(crate) mod compare;
pub(crate) mod convolution;
pub(crate) mod edge;
pub(crate) mod files;
pub(crate) mod gaussian;
pub(crate) mod guided;
pub(crate) mod icc;
pub(crate) mod numbers;
pub(crate) mod ops;
pub(crate) mod pictures;
pub(crate) mod pieces;
pub(crate) mod pixels;
pub(crate) mod recipe;
pub(crate) mod run;
pub(crate) mod sampling;
pub(crate) mod stats;
pub(crate) mod sweep;
pub(crate) mod yiq;

pub(crate) type Outcome = Result<(), Box<dyn std::error::Error>>;

/// The effects of the core, one for each file of `src/effects/` (the registry also holds the doubles that other tests of
/// this binary register for a while, which would come and go during the run).
pub(crate) fn core_effects() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("effects");
    let mut names = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "rs")
            && let Some(name) = path.file_stem().and_then(|stem| stem.to_str())
        {
            names.push(name.to_owned());
        }
    }
    assert!(names.len() > 80, "only {} effect files", names.len());
    Ok(names)
}
