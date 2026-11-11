//! The documentation of an effect, as text a person reads: its description, every parameter with its type, unit, range,
//! default and meaning, its step opacity and its presets.

use std::fmt::Write as _;

use crate::params::{Field, Kind, Scalar, Table};
use crate::{Error, Presets};

/// The width descriptions are wrapped to, indent included.
const WIDTH: usize = 88; // constant: the width the documentation has always been wrapped to
/// The indent of a parameter's description.
const INDENT: &str = "    ";

/// The documentation of the effect called `name`: its description, then a "Parameters" section with every basic parameter's
/// type, unit, range, default and meaning, then an "Advanced parameters" section (or, without `advanced`, one line counting
/// them), then a "Step opacity" section if a step of it has an opacity other than 1 by default, then a "Presets" section
/// (the presets of `presets`, in the order written) if it has any.
///
/// # Errors
/// [`Error::UnknownEffect`] if there is no such effect; [`Error::Inputs`] for an effect that runs outside the core, which
/// has no parameter table to document.
pub fn effect(name: &str, presets: &Presets, advanced: bool) -> Result<String, Error> {
    let found = crate::registry::effect(name)?;
    let table = found.params();
    let mut text = format!("{}\n\n{}", table.doc.trim(), parameters(&table, advanced));
    let opacity = found.opacity();
    #[expect(
        clippy::float_cmp,
        reason = "an effect either declares an opacity or leaves it at exactly 1"
    )]
    if opacity != 1.0 {
        let shown = general(opacity);
        let _ = write!(
            text,
            "\n\nStep opacity\n------------\n{shown}: a step of this effect is mixed in at {shown} unless it sets its own \
             `opacity`, which replaces this."
        );
    }
    let names = presets.names(name);
    if !names.is_empty() {
        text.push_str("\n\nPresets\n-------\n");
        let lines: Vec<String> = names
            .iter()
            .filter_map(|preset| {
                presets
                    .get(name, preset)
                    .map(|found| format!("{preset} : {}", found.description))
            })
            .collect();
        text.push_str(&lines.join("\n"));
    }
    Ok(text)
}

/// Why `name` is no effect, in words that help whoever typed it: everything it is, listed together (a preset of some effects, a
/// look that ships with the crate with its description, a parameter of some effects, a phrase in the summary of some effects),
/// or else the closest effects, looks and presets and all the effects.
#[must_use]
pub fn unknown_effect(name: &str, presets: &Presets) -> String {
    let effects: Vec<String> = crate::registry::effects();
    let described: Vec<(String, Table)> = effects
        .iter()
        .filter_map(|effect| Some((effect.clone(), crate::registry::effect(effect).ok()?.params())))
        .collect();
    let mut meanings: Vec<String> = Vec::new();
    for effect in &effects {
        if presets.names(effect).contains(&name) {
            meanings.push(format!("a preset (fx.{effect}(preset=\"{name}\"))"));
        }
    }
    let looks = crate::shipped::looks();
    if looks.iter().any(|(known, _)| *known == name) {
        let description = crate::Recipe::look(name)
            .ok()
            .and_then(|look| look.description)
            .unwrap_or_default();
        meanings.push(format!(
            "a look (load_recipe(\"{name}\"), `bt-image render PHOTO {name}`): {description}"
        ));
    }
    let mut owners: Vec<(usize, &str)> = described
        .iter()
        .filter(|(_, table)| table.fields.iter().any(|field| field.name == name))
        .map(|(effect, table)| (table.fields.len(), effect.as_str()))
        .collect();
    owners.sort_unstable(); // the narrowest effect first
    if !owners.is_empty() {
        let first: Vec<&str> = owners.iter().take(LISTED).map(|(_, effect)| *effect).collect();
        let more = if owners.len() > LISTED {
            format!(" and {} more", owners.len() - LISTED)
        } else {
            String::new()
        };
        meanings.push(format!("a parameter of {}{more}", first.join(", ")));
    }
    let words: Vec<String> = words_of(name);
    let mentions: Vec<(&str, String)> = effects
        .iter()
        .filter_map(|effect| Some((effect.as_str(), summary(effect).ok()?)))
        .filter(|(_, text)| {
            let found = words_of(text);
            !words.is_empty() && found.windows(words.len()).any(|window| window == words.as_slice())
        })
        .collect();
    meanings.extend(
        mentions
            .iter()
            .take(LISTED)
            .map(|(effect, text)| format!("named in the summary of {effect}: {text}")),
    );
    if mentions.len() > LISTED {
        meanings.push(format!("named in the summary of {} more", mentions.len() - LISTED));
    }
    if !meanings.is_empty() {
        return format!("{} is {}", quoted(name), meanings.join("; and "));
    }
    let mut kinds: Vec<(String, String)> = effects
        .iter()
        .map(|effect| (effect.clone(), "effect".to_owned()))
        .collect();
    kinds.extend(looks.iter().map(|(known, _)| ((*known).to_owned(), "look".to_owned())));
    for effect in &effects {
        kinds.extend(
            presets
                .names(effect)
                .into_iter()
                .map(|preset| (preset.to_owned(), format!("preset of {effect}"))),
        );
    }
    let near: Vec<String> = crate::recipe::near::closest(name, kinds.iter().map(|(known, _)| known.as_str()))
        .into_iter()
        .filter_map(|close| {
            kinds
                .iter()
                .find(|(known, _)| known == close)
                .map(|(known, kind)| format!("{known} ({kind})"))
        })
        .collect();
    let hint = if near.is_empty() {
        String::new()
    } else {
        format!("; did you mean: {}", near.join(", "))
    };
    format!(
        "unknown effect {}{hint}; available: {}",
        quoted(name),
        effects.join(", ")
    )
}

/// How many effects a message names at most, for each thing a name can be.
const LISTED: usize = 5; // constant: enough to recognise the family, few enough to read

/// The runs of letters and digits of `text`, lowercase.
fn words_of(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

/// The first sentence of the description of the effect called `name`, on one line.
///
/// # Errors
/// As [`effect`].
pub fn summary(name: &str) -> Result<String, Error> {
    let found = crate::registry::effect(name)?;
    let table = found.params();
    let text = table.doc.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = text.chars().collect();
    let end = (0..chars.len()).find(|&at| {
        matches!(chars[at], '.' | '!' | '?')
            && chars.get(at + 1).is_none_or(|next| next.is_whitespace())
            // a stop in "e.g." or "i.e." ends nothing
            && !(at >= 3 && matches!(chars[at - 3..at].iter().collect::<String>().as_str(), "e.g" | "i.e"))
    });
    Ok(end.map_or(text.clone(), |at| chars[..=at].iter().collect()))
}

/// The "Parameters" section, and the "Advanced parameters" one (or the line counting them).
fn parameters(table: &Table, advanced: bool) -> String {
    let (mut basic, mut extra): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for field in &table.fields {
        let mut parts = vec![type_text(field)];
        if let Some(unit) = field.unit {
            parts.push(unit.name().to_owned());
        }
        if let (Some(min), Some(max)) = (field.min, field.max) {
            parts.push(format!("{} to {}", bound(min), bound(max)));
        }
        parts.push(format!("default {}", default(field)));
        let words = field.meaning.split_whitespace().collect::<Vec<_>>().join(" ");
        let entry = format!(
            "{} : {}\n{}",
            field.name,
            parts.join(", "),
            fill(&words, WIDTH, INDENT, INDENT)
        );
        if field.advanced { &mut extra } else { &mut basic }.push(entry);
    }
    let text = format!("Parameters\n----------\n{}", basic.join("\n"));
    if extra.is_empty() {
        text
    } else if advanced {
        format!(
            "{text}\n\nAdvanced parameters\n-------------------\n{}",
            extra.join("\n")
        )
    } else {
        format!("{text}\n\n(+{} advanced)", extra.len())
    }
}

/// A parameter's type in words, with whether a drive may vary it and whether it is a turn.
fn type_text(field: &Field) -> String {
    let mut text = match field.kind {
        Kind::Float => "number".to_owned(),
        Kind::Int => "whole number".to_owned(),
        Kind::Bool => "true or false".to_owned(),
        Kind::Choice => format!(
            "one of {}",
            field
                .choices
                .iter()
                .map(|choice| quoted(choice))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Kind::Color if field.nullable => "colour or none".to_owned(),
        Kind::Color => "colour".to_owned(),
        Kind::Colors => "list of colours".to_owned(),
        Kind::Field => "field name".to_owned(),
        Kind::Lut => "lookup table".to_owned(),
        Kind::Text => "text".to_owned(),
        Kind::Picture => "picture or none".to_owned(),
    };
    if field.drivable {
        text.push_str(" (drivable)");
    }
    if field.cyclic {
        text.push_str(" (cyclic)");
    }
    text
}

/// A parameter's default as it is written in a recipe.
fn default(field: &Field) -> String {
    match (&field.default, field.kind) {
        (Scalar::Null, Kind::Lut) => "Lut.identity(2)".to_owned(), // constant: how the documentation has always shown the identity table
        (Scalar::Null, _) => "None".to_owned(),
        (Scalar::Int(value), Kind::Float) =>
        {
            #[expect(clippy::cast_precision_loss, reason = "a default is a small whole number")]
            real(*value as f64)
        }
        (Scalar::Int(value), _) => value.to_string(),
        (Scalar::Float(value), _) => real(*value),
        (Scalar::Bool(value), _) => if *value { "True" } else { "False" }.to_owned(),
        (Scalar::Text(value), _) => quoted(value),
        (Scalar::List(items), _) => match items {
            [one] => format!("({},)", quoted(one)),
            many => format!(
                "({})",
                many.iter().map(|item| quoted(item)).collect::<Vec<_>>().join(", ")
            ),
        },
    }
}

/// A bound as a person writes it: 0, 500, 0.5, 4294967295.
fn bound(value: Scalar) -> String {
    let number = match value {
        #[expect(clippy::cast_precision_loss, reason = "a bound is a small whole number")]
        Scalar::Int(whole) => whole as f64,
        Scalar::Float(real) => real,
        _ => return String::new(),
    };
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{number:.0}")
    } else {
        real(number)
    }
}

/// `value` as Python's `repr` writes a float: the shortest text that reads back as the same number, `10.0`, `4.8`, `1e-05`.
fn real(value: f64) -> String {
    if value == 0.0 || (1e-4..1e16).contains(&value.abs()) {
        let text = format!("{value}");
        return if text.contains('.') { text } else { format!("{text}.0") };
    }
    let text = format!("{value:e}");
    let (mantissa, power) = text.split_once('e').unwrap_or((&text, "0"));
    let power: i32 = power.parse().unwrap_or(0);
    format!("{mantissa}e{}{:02}", if power < 0 { '-' } else { '+' }, power.abs())
}

/// `text` as Python's `repr` writes a string: in single quotes, in double ones if it holds a single quote and no double.
pub(crate) fn quoted(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other if other == quote => {
                out.push('\\');
                out.push(other);
            }
            other if other.is_control() => {
                let _ = write!(out, "\\x{:02x}", u32::from(other));
            }
            other => out.push(other),
        }
    }
    out.push(quote);
    out
}

/// `value` as `%g` writes it: up to six significant digits, no trailing zeros, an exponent beyond 1e-4 .. 1e6.
#[must_use]
pub fn general(value: f64) -> String {
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

/// Whether `character` can be part of a word: a letter, an underscore (not a digit).
fn letter(character: char) -> bool {
    character.is_alphabetic() || character == '_'
}

/// `text` cut into the pieces a line may break between: runs of white space, and words, which also break after a hyphen
/// between letters (`well-known` is `well-` and `known`), as Python's `textwrap` does.
fn chunks(text: &str) -> Vec<String> {
    let characters: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut current = String::new();
    for (at, &character) in characters.iter().enumerate() {
        let breaks_here = current.chars().last().is_some_and(char::is_whitespace) != character.is_whitespace();
        if breaks_here && !current.is_empty() {
            found.push(std::mem::take(&mut current));
        }
        current.push(character);
        let hyphen_between_letters = character == '-'
            && !character.is_whitespace()
            && at >= 2
            && (letter(characters[at - 1]) && letter(characters[at - 2])
                || (at >= 3 && letter(characters[at - 1]) && characters[at - 2] == '-' && letter(characters[at - 3])))
            && characters.get(at + 1).copied().is_some_and(letter)
            && (characters.get(at + 2).copied().is_some_and(letter)
                || (characters.get(at + 2) == Some(&'-') && characters.get(at + 3).copied().is_some_and(letter)));
        if hyphen_between_letters {
            found.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        found.push(current);
    }
    found
}

/// `text` wrapped to lines of at most `width` characters, the first starting with `initial` and the others with
/// `subsequent`: as Python's `textwrap.fill` does (words are broken after a hyphen between letters, and a word longer than
/// a line is cut). White space in `text` is kept as it is: normalise it first.
#[must_use]
pub fn fill(text: &str, width: usize, initial: &str, subsequent: &str) -> String {
    let mut pending: Vec<String> = chunks(text);
    pending.reverse();
    let mut lines: Vec<String> = Vec::new();
    while !pending.is_empty() {
        let indent = if lines.is_empty() { initial } else { subsequent };
        let room = width.saturating_sub(indent.chars().count());
        let mut line: Vec<String> = Vec::new();
        let mut length = 0;
        if !lines.is_empty() && pending.last().is_some_and(|chunk| chunk.trim().is_empty()) {
            pending.pop();
        }
        while let Some(chunk) = pending.last() {
            let size = chunk.chars().count();
            if length + size > room {
                break;
            }
            length += size;
            line.extend(pending.pop());
        }
        if let Some(chunk) = pending.last().cloned()
            && chunk.chars().count() > room
        {
            let left = if room < 1 { 1 } else { room.saturating_sub(length) };
            line.push(chunk.chars().take(left).collect());
            if let Some(last) = pending.last_mut() {
                *last = chunk.chars().skip(left).collect();
            }
        }
        if line.last().is_some_and(|chunk| chunk.trim().is_empty()) {
            line.pop();
        }
        if !line.is_empty() {
            lines.push(format!("{indent}{}", line.concat()));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{fill, general, quoted, real};

    // expected values worked out by hand from the rules of Python's `repr` and `%g`
    #[test]
    fn numbers_and_text_are_written_as_python_does() {
        for (value, text) in [
            (0.0, "0.0"),
            (10.0, "10.0"),
            (4.8, "4.8"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (1.5e20, "1.5e+20"),
            (-2.5, "-2.5"),
        ] {
            assert_eq!(real(value), text);
        }
        assert_eq!(quoted("abc"), "'abc'");
        assert_eq!(quoted("it's"), "\"it's\"");
        assert_eq!(quoted("a'b\"c"), "'a\\'b\"c'");
        assert_eq!(quoted("a\\b\n"), "'a\\\\b\\n'");
        for (value, text) in [
            (0.0, "0"),
            (5.0, "5"),
            (0.7, "0.7"),
            (1.0 / 3.0, "0.333333"),
            (12_345_678.0, "1.23457e+07"),
        ] {
            assert_eq!(general(value), text);
        }
    }

    #[test]
    fn a_description_wraps_at_the_width_with_its_indent() {
        let words = "one two three four five six seven eight nine ten";
        // 20 wide with a 2 px indent leaves 18: "one two three four" is 18 exactly
        assert_eq!(
            fill(words, 20, "  ", "  "),
            "  one two three four\n  five six seven\n  eight nine ten"
        );
        assert_eq!(fill("", 20, "  ", "  "), "");
        // a first line and the lines after it may be indented differently, and each fits its own room
        assert_eq!(
            fill("one two three four", 10, "- ", "    "),
            "- one two\n    three\n    four"
        );
    }

    #[test]
    fn a_word_breaks_after_a_hyphen_between_letters_and_a_long_word_is_cut() {
        // "well-known" may break after "well-": the second line starts with "known"
        assert_eq!(
            fill("aaaa bbbb well-known", 14, "  ", "  "),
            "  aaaa bbbb\n  well-known"
        );
        assert_eq!(fill("aaaa well-known", 12, "  ", "  "), "  aaaa well-\n  known");
        assert_eq!(fill("x-5 y-ab", 6, "", ""), "x-5\ny-ab");
        // a word longer than the room is cut to it
        assert_eq!(fill("abcdefghij", 6, "  ", "  "), "  abcd\n  efgh\n  ij");
    }
}
