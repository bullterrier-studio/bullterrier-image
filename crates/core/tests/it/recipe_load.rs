//! A recipe file read from disk (`Recipe::from_file`): what `from_toml` reads, a name from the file's own name when it has none,
//! and the files it names (a picture, a lookup table, an svg) found relative to the recipe file.

use std::path::{Path, PathBuf};

use bullterrier_image::{Error, Recipe};

use crate::common::Outcome;

fn file(name: &str, text: &str) -> Result<PathBuf, std::io::Error> {
    let folder = std::env::temp_dir().join(format!("bt-image-recipe-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&folder));
    std::fs::create_dir_all(folder.join("look"))?;
    let path = folder.join("look").join("my look.toml");
    std::fs::write(&path, text)?;
    Ok(path)
}

/// The recipe as the JSON it is written in, for looking at one value.
fn json(recipe: &Recipe) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(recipe)
}

#[test]
fn a_recipe_without_a_name_takes_the_name_of_its_file_and_one_with_a_name_keeps_it() -> Outcome {
    let nameless = Recipe::from_file(&file("nameless", "[[step]]\neffect = \"grade\"\n")?)?;
    assert_eq!(nameless.name.as_deref(), Some("my look"));
    let named = Recipe::from_file(&file("named", "name = \"warm\"\n[[step]]\neffect = \"grade\"\n")?)?;
    assert_eq!(named.name.as_deref(), Some("warm"));
    Ok(())
}

#[test]
fn a_picture_a_lookup_table_and_an_svg_are_found_relative_to_the_recipe_file() -> Outcome {
    let text = "[region.star]\nkind = \"svg\"\nfile = \"star.svg\"\n\n[[step]]\neffect = \"blend\"\nimage = \"assets/logo.png\"\n\n[[step]]\neffect = \"lut\"\nfile = \"film.cube\"\n\n[[step]]\neach = \"star\"\nlooks = [{ effect = \"blend\", image = \"assets/logo.png\" }, \"none\"]\n";
    let path = file("relative", &format!("format = 2\n{text}"))?;
    let folder = path.parent().ok_or("parent")?;
    let written = json(&Recipe::from_file(&path)?)?;
    let at = |value: &serde_json::Value| value.as_str().map(PathBuf::from);
    assert_eq!(at(&written["regions"]["star"]["file"]), Some(folder.join("star.svg")));
    assert_eq!(
        at(&written["steps"][0]["params"]["image"]),
        Some(folder.join("assets/logo.png"))
    );
    assert_eq!(
        at(&written["steps"][1]["params"]["file"]),
        Some(folder.join("film.cube"))
    );
    assert_eq!(
        at(&written["steps"][2]["looks"][0]["params"]["image"]),
        Some(folder.join("assets/logo.png"))
    );
    Ok(())
}

#[test]
fn an_absolute_path_a_colour_and_the_name_of_a_given_picture_are_left_as_they_are() -> Outcome {
    let absolute = std::env::temp_dir().join("logo.png");
    let text = format!(
        "[[step]]\neffect = \"blend\"\nimage = {:?}\n\n[[step]]\neffect = \"blend\"\nimage = \"$picture:abc\"\n\n[[step]]\neffect = \"blend\"\nimage = \"#ff0000\"\n",
        absolute.display().to_string()
    );
    let written = json(&Recipe::from_file(&file("untouched", &text)?)?)?;
    assert_eq!(written["steps"][0]["params"]["image"], absolute.display().to_string());
    assert_eq!(written["steps"][1]["params"]["image"], "$picture:abc");
    assert_eq!(written["steps"][2]["params"]["image"], "#ff0000");
    Ok(())
}

#[test]
fn a_file_that_is_missing_or_not_a_recipe_is_refused_naming_it() -> Outcome {
    let missing = Recipe::from_file(Path::new("/no/such/look.toml"));
    assert!(
        matches!(&missing, Err(Error::Recipe(message)) if message.contains("look.toml")),
        "{missing:?}"
    );
    let broken = Recipe::from_file(&file("broken", "[[step]]\neffect = 3\n")?);
    assert!(
        matches!(&broken, Err(Error::Recipe(message)) if message.contains("my look.toml") && message.contains("step")),
        "{broken:?}"
    );
    Ok(())
}
