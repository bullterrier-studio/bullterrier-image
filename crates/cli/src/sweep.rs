//! `bt-image sweep`: one number of a recipe across a range of values, side by side.

use std::path::PathBuf;

use bullterrier_image::{Error, Options, Presets, Recipe, Tile, contact_sheet, write};
use clap::Args as ClapArgs;

use crate::Console;
use crate::photo::{Maps, each};

/// The sides of a tile of the comparison sheet, px.
const TILE: usize = 480; // constant: the size `contact_sheet` has always been asked for here
/// How many values a sweep may show.
const COUNTS: std::ops::RangeInclusive<u32> = 2..=24; // constant: fewer is no sweep, more than a sheet can show

#[derive(ClapArgs)]
pub(crate) struct Args {
    /// The photo to style.
    photo: PathBuf,
    /// The recipe `.toml`, or the name of a built-in look, whose parameter is swept.
    recipe: String,
    /// The number's address, e.g. `rgb_split.amount`; `rgb_split#2.amount` for the second step of an effect used twice,
    /// `pulse.gain` for a step called pulse, `pulse.gain.end` for the end of a drive, `region.near.low`, `frame.angle`.
    target: String,
    /// The values to show, e.g. `0..20`.
    #[arg(value_name = "START..END", value_parser = span, allow_hyphen_values = true)]
    range: (f64, f64),
    #[command(flatten)]
    maps: Maps,
    /// Output file.
    #[arg(short, long, default_value = "out/sweep.png")]
    out: PathBuf,
    /// How many values, 2 to 24, evenly spaced.
    #[arg(long, default_value_t = 5)]
    count: u32,
    /// Pictures per row.
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u32).range(1..))]
    columns: u32,
}

/// A clap value parser: `START..END`, two numbers (`0..20`, `-2..2`, `0..0.5`).
fn span(text: &str) -> Result<(f64, f64), String> {
    let wrong = || format!("{text:?} is not a range of two numbers like 0..20");
    // the first `..` that follows a digit: `0..20`, `-2..2`, `.5..1` and `0..-1` all split where the dots are not a decimal point
    let at = text
        .char_indices()
        .find(|(at, _)| text[*at..].starts_with("..") && text[..*at].ends_with(|c: char| c.is_ascii_digit()))
        .map(|(at, _)| at)
        .ok_or_else(wrong)?;
    let number = |part: &str| {
        let plain = part.strip_prefix('-').unwrap_or(part);
        if plain.is_empty() || !plain.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return Err(wrong());
        }
        part.parse::<f64>().map_err(|_| wrong())
    };
    Ok((number(&text[..at])?, number(&text[at + 2..])?))
}

/// `value` as `%g` writes it: up to six significant digits, no trailing zeros, an exponent beyond 1e-4 .. 1e6.
fn general(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    let exponent = value.abs().log10().floor();
    if !(-4.0..6.0).contains(&exponent) {
        let text = format!("{value:.5e}");
        let (mantissa, power) = text.split_once('e').unwrap_or((&text, "0"));
        let mantissa = if mantissa.contains('.') {
            mantissa.trim_end_matches('0').trim_end_matches('.')
        } else {
            mantissa
        };
        let power: i32 = power.parse().unwrap_or(0);
        return format!("{mantissa}e{}{:02}", if power < 0 { '-' } else { '+' }, power.abs());
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the exponent of a value in -4..6 leaves 0 to 9 decimals"
    )]
    let decimals = (5.0 - exponent).max(0.0) as usize;
    let text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// `count` values evenly spaced from `start` to `end`, the last exactly `end`.
fn linspace(start: f64, end: f64, count: u32) -> Vec<f64> {
    let last = f64::from(count - 1);
    (0..count)
        .map(|at| {
            if at == count - 1 {
                end
            } else {
                start + (end - start) * f64::from(at) / last
            }
        })
        .collect()
}

/// The recipe at each of `count` values of the number at `target`, with the label `number=value` of each: all checked
/// before any work.
fn variants(recipe: &Recipe, args: &Args, presets: &Presets) -> Result<Vec<(String, Recipe)>, Error> {
    if !COUNTS.contains(&args.count) {
        return Err(Error::Recipe(format!(
            "count must be 2 to 24 tiles, got {}",
            args.count
        )));
    }
    let (start, end) = args.range;
    #[expect(clippy::float_cmp, reason = "a range that is empty is the same number twice")]
    if start == end {
        return Err(Error::Recipe(format!(
            "a sweep needs a range: start and end are both {}",
            general(start)
        )));
    }
    let number = recipe.number(&args.target)?;
    let mut values = linspace(start, end, args.count);
    if number.whole {
        values = values.into_iter().map(f64::round_ties_even).collect();
        values.dedup(); // evenly spaced values round to neighbours that repeat, never apart
    }
    let label = args.target.split_once('.').map_or("", |(_, rest)| rest);
    values
        .into_iter()
        .map(|value| {
            Ok((
                format!("{label}={}", general(value)),
                recipe.with_value(&args.target, value, presets)?,
            ))
        })
        .collect()
}

pub(crate) fn command(args: &Args, console: &mut Console<'_>) -> Result<i32, Error> {
    let presets = Presets::shipped()?;
    let recipe = Recipe::resolve(&args.recipe)?
        .checked(&presets)
        .map_err(|error| Error::Recipe(format!("{}: {error}", args.recipe)))?;
    let tiles = variants(&recipe, args, &presets)?;
    console.progress(&format!("rendering {} value(s) of {}...", tiles.len(), args.target))?;
    let recipes: Vec<Recipe> = tiles.iter().map(|(_, recipe)| recipe.clone()).collect();
    let images = each(&args.photo, &recipes, &args.maps, |photo, recipe, presets| {
        photo.render(recipe, presets, Options::default().with_threads(0)) // one photo: every core
    })?;
    let labelled: Vec<Tile<'_>> = tiles
        .iter()
        .zip(&images)
        .map(|((label, _), image)| Tile::new(label, image))
        .collect();
    let columns = usize::try_from(args.columns).unwrap_or(usize::MAX);
    let sheet = contact_sheet(&labelled, columns, TILE)?;
    if let Some(folder) = args.out.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|error| Error::ImageWrite(format!("cannot make the folder {}: {error}", folder.display())))?;
    }
    write(&args.out, &sheet)?;
    console.result(&args.out.display().to_string())?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::{general, linspace, span};

    // values worked out by hand, as `%g` writes them
    #[test]
    fn numbers_are_written_as_g_does() {
        for (value, text) in [
            (0.0, "0"),
            (5.0, "5"),
            (-2.0, "-2"),
            (2.5, "2.5"),
            (0.1 + 0.2, "0.3"),
            (1.0 / 3.0, "0.333333"),
            (12.5, "12.5"),
            (123_456.0, "123456"),
            (12_345_678.0, "1.23457e+07"),
            (0.00001, "1e-05"),
            (0.0001, "0.0001"),
        ] {
            assert_eq!(general(value), text);
        }
    }

    #[test]
    fn values_are_evenly_spaced_and_end_exactly_on_the_end() {
        assert_eq!(linspace(0.0, 20.0, 5), [0.0, 5.0, 10.0, 15.0, 20.0]);
        assert_eq!(linspace(1.0, 0.0, 2), [1.0, 0.0]);
        assert_eq!(linspace(0.0, 0.3, 4).last(), Some(&0.3));
    }

    #[test]
    fn a_range_is_two_numbers_joined_by_two_dots() {
        assert_eq!(span("0..20"), Ok((0.0, 20.0)));
        assert_eq!(span("-2..2"), Ok((-2.0, 2.0)));
        assert_eq!(span("0..0.5"), Ok((0.0, 0.5)));
        assert_eq!(span("0.5..-1.25"), Ok((0.5, -1.25)));
        for wrong in ["", "5", "0..", "..5", "a..b", "1,5"] {
            assert!(span(wrong).is_err(), "{wrong}");
        }
    }
}
