//! `bt-image effects`: the built-in looks and the effects.

use bullterrier_image::documentation::{effect as documentation, fill, summary, unknown_effect};
use bullterrier_image::{Category, Error, Presets, Recipe, effect, effects, shipped};
use clap::Args as ClapArgs;

use crate::Console;

/// The narrowest and widest a short listing is wrapped to; the width of a pipe is the widest.
const LISTING_WIDTH: (usize, usize) = (60, 120); // constant: legible on a small terminal, no wider than a line reads well

#[derive(ClapArgs)]
pub(crate) struct Args {
    /// Show only this effect, in full: every parameter and preset.
    #[arg(conflicts_with_all = ["category", "list_categories"])]
    name: Option<String>,
    /// List only the effects of this category (see `--list-categories`).
    #[arg(long, value_name = "NAME", conflicts_with_all = ["name", "list_categories"])]
    category: Option<String>,
    /// List the categories that have effects, with their number of effects and what each holds.
    #[arg(long, conflicts_with_all = ["name", "category"])]
    list_categories: bool,
    /// Show every effect in full (parameters and presets) instead of one line; with `--category`, those of it.
    #[arg(long)]
    all: bool,
    /// Like `--all`, and also list the advanced parameters (`--category` narrows it as well).
    #[arg(long)]
    advanced: bool,
}

/// `text` with `prefix` in front of every line that holds something.
fn indented(text: &str, prefix: &str) -> String {
    text.split('\n')
        .map(|line| {
            if line.trim().is_empty() {
                line.to_owned()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `  name: text`, wrapped to the terminal's width, continuation lines indented further.
fn entry(name: &str, text: &str) -> String {
    let (low, high) = LISTING_WIDTH;
    let columns = std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(high);
    let words = format!("{name}: {text}")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    fill(&words, columns.clamp(low, high), "  ", "    ")
}

/// The one-line entry of the effect `name`: its summary and presets.
fn short(name: &str, presets: &Presets) -> String {
    let text = summary(name).unwrap_or_default();
    let found = presets.names(name);
    let listed = if found.is_empty() {
        String::new()
    } else {
        format!(" (presets: {})", found.join(", "))
    };
    entry(name, &format!("{text}{listed}"))
}

pub(crate) fn command(args: &Args, console: &mut Console<'_>) -> Result<i32, Error> {
    let presets = Presets::shipped()?;
    let all = effects();
    let categorised: Vec<(Category, Vec<String>)> = Category::ALL
        .iter()
        .map(|&category| {
            let names = all
                .iter()
                .filter(|name| effect(name).is_ok_and(|found| found.category() == category))
                .cloned()
                .collect();
            (category, names)
        })
        .filter(|(_, names): &(Category, Vec<String>)| !names.is_empty())
        .collect();
    if args.list_categories {
        let width = categorised
            .iter()
            .map(|(category, _)| category.name().len())
            .max()
            .unwrap_or(0);
        for (category, names) in &categorised {
            console.result(&format!(
                "{:<width$}  {:>3}  {}",
                category.name(),
                names.len(),
                category.description()
            ))?;
        }
        return Ok(0);
    }
    if let Some(name) = &args.name {
        let text = documentation(name, &presets, true).map_err(|error| dress(error, name, &presets))?;
        console.result(&format!("{name}\n{}", indented(&text, "    ")))?;
        return Ok(0);
    }
    let names: Vec<String> = match &args.category {
        None => all.clone(),
        Some(wanted) => {
            let Some((_, names)) = categorised.iter().find(|(category, _)| category.name() == wanted) else {
                let there: Vec<&str> = categorised.iter().map(|(category, _)| category.name()).collect();
                return Err(Error::Recipe(format!(
                    "no effects in category '{wanted}'; categories with effects: {}",
                    there.join(", ")
                )));
            };
            names.clone()
        }
    };
    if args.category.is_none() {
        // the looks, which are no effects, come first: a person looks for a style before a tool
        console.result(
            "Looks (recipes of several steps; a name works where a recipe file does: `bt-image render PHOTO crt`)",
        )?;
        for (name, _) in shipped::looks() {
            let description = Recipe::look(name)?.description.unwrap_or_default();
            console.result(&entry(name, &description))?;
        }
        console.result("")?;
    }
    if args.all || args.advanced {
        for name in &names {
            let text = documentation(name, &presets, args.advanced).unwrap_or_default();
            console.result(&format!("{name}\n{}\n", indented(&text, "    ")))?;
        }
        return Ok(0);
    }
    console.result(&match &args.category {
        None => "Effects".to_owned(),
        Some(category) => format!("Effects in {category}"),
    })?;
    for name in &names {
        console.result(&short(name, &presets))?;
    }
    console.result("\ndetails: bt-image effects NAME, or --all")?;
    Ok(0)
}

/// `error`, when it says `name` is no effect, as the longer message that says what the name is.
pub(crate) fn dress(error: Error, name: &str, presets: &Presets) -> Error {
    match error {
        Error::UnknownEffect(_) => Error::Recipe(unknown_effect(name, presets)),
        other => other,
    }
}
