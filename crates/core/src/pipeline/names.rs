//! The file name a recipe's name gives its files: nothing that could leave the output folder or hide a file, on any system.

use crate::{Error, Recipe};

/// The longest a safe name is, characters.
const LONGEST: usize = 64; // constant: well inside every file system's limit, beside the extension and size suffix
/// The names Windows keeps for devices, in any case and whatever the extension: such a file cannot be made there.
const DEVICES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
]; // constant: Windows' reserved device names

/// `text` (a recipe's name, a file name without its extension) made into a safe file name, or empty when none is left.
///
/// Every run of characters other than letters, digits, `.`, `_` and `-` becomes one `-`, repeated `-` collapse into one,
/// anything before the first letter or digit is dropped, the result is cut to 64 characters and a trailing `-` or `.` is
/// trimmed: "my look" gives "my-look". Empty when nothing remains or the result is a name Windows reserves for a device.
#[must_use]
pub fn safe_name(text: &str) -> String {
    let mut name = String::new();
    for character in text.chars() {
        let plain = character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-');
        let separator = if plain { character } else { '-' };
        if separator == '-' && name.ends_with('-') {
            continue; // runs of dashes, written or made, are one
        }
        name.push(separator);
    }
    let name: String = name
        .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
        .chars()
        .take(LONGEST)
        .collect();
    let name = name.trim_end_matches(['-', '.']).to_owned();
    let device = name
        .split('.')
        .next()
        .is_some_and(|stem| DEVICES.contains(&stem.to_ascii_uppercase().as_str()));
    if name.is_empty() || device { String::new() } else { name }
}

/// The name the files of `recipe` are given: its name made safe.
///
/// # Errors
/// [`Error::Recipe`] naming the fix when the recipe has no name or its name leaves no safe file name.
pub fn file_name(recipe: &Recipe) -> Result<String, Error> {
    let Some(name) = &recipe.name else {
        return Err(Error::Recipe(
            "a recipe needs a name to name its files: give one, Recipe(steps, name=\"my-look\")".to_owned(),
        ));
    };
    let stem = safe_name(name);
    if stem.is_empty() {
        return Err(Error::Recipe(format!(
            "recipe '{name}': its name leaves no file name (letters, digits, . _ - and not a device name Windows keeps, such as \"con\"): give another, name=\"my-look\""
        )));
    }
    Ok(stem)
}
