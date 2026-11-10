//! The presets and looks that ship with the crate (`crates/core/data/`): what the crate hands out is what is in the folder,
//! and every look is a recipe the crate accepts.

use std::fs;
use std::path::Path;

use bullterrier_image::{Presets, Recipe, shipped};

use crate::common::Outcome;

fn folder(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data").join(name)
}

/// The `(file stem, text)` of each `.toml` file of a data folder, by name.
fn files(name: &str) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(folder(name))? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "toml") {
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or("a file name")?
                .to_owned();
            found.push((stem, fs::read_to_string(&path)?));
        }
    }
    found.sort();
    Ok(found)
}

#[test]
fn the_crate_hands_out_the_preset_files_of_its_data_folder_by_name() -> Outcome {
    let listed: Vec<(String, String)> = shipped::presets()
        .iter()
        .map(|(name, text)| ((*name).to_owned(), (*text).to_owned()))
        .collect();
    assert_eq!(listed, files("presets")?);
    assert!(listed.len() > 20, "{} files", listed.len());
    Ok(())
}

#[test]
fn the_crate_hands_out_the_look_files_of_its_data_folder_by_name() -> Outcome {
    let listed: Vec<(String, String)> = shipped::looks()
        .iter()
        .map(|(name, text)| ((*name).to_owned(), (*text).to_owned()))
        .collect();
    assert_eq!(listed, files("looks")?);
    assert!(listed.len() >= 9, "{} files", listed.len());
    Ok(())
}

#[test]
fn every_shipped_preset_is_one_its_effect_accepts() -> Outcome {
    let presets = Presets::shipped()?;
    for (effect, text) in shipped::presets() {
        let table: toml::Table = toml::from_str(text)?;
        for name in table.keys() {
            assert!(presets.get(effect, name).is_some(), "{effect} {name}");
        }
    }
    Ok(())
}

#[test]
fn every_shipped_look_is_a_checked_recipe_with_a_description() -> Outcome {
    let presets = Presets::shipped()?;
    for (name, text) in shipped::looks() {
        Recipe::from_toml(text)?
            .checked_look(&presets)
            .map_err(|error| format!("{name}: {error}"))?;
    }
    Ok(())
}
