//! What the names in a recipe refer to: that a `mask`, a field, a `partition`, an `each` or a region's `of`, `per` or
//! `fit` names a region that exists and has the form the place needs, and that regions are not made from each other in
//! a circle.

use std::collections::HashMap;

use super::Regions;
use super::built_in::BuiltIn;
use crate::Error;

/// What a region is, which decides where it can be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Form {
    /// Pixels chosen to a degree from 0 to 1: a `mask`, a field, what a band is cut from.
    Selection,
    /// Numbered pieces: what an `each` step gives looks to, a `Pick` picks from, a `Subject` is found in.
    Partition,
}

impl Form {
    /// The word for it in a message.
    fn name(self) -> &'static str {
        match self {
            Self::Selection => "selection",
            Self::Partition => "partition",
        }
    }
}

/// What the region `name` is: a built-in one is a selection, a declared one is by its kind. None if there is none.
fn form_of(regions: &Regions, name: &str) -> Option<Form> {
    if BuiltIn::named(name).is_some() {
        return Some(Form::Selection);
    }
    regions.get(name).map(super::Region::form)
}

/// `Ok` if `name` (what `key` of `place`, as `step 2` or `region 'x'`, refers to) is a region of the `wanted` form, or
/// any region when `wanted` is none.
///
/// # Errors
/// [`Error::Recipe`] naming the place, the key and the name, saying whether there is no such region (with the closest
/// name, if one is close) or it has the other form, and listing the names that would do.
pub(super) fn refer(regions: &Regions, place: &str, key: &str, name: &str, wanted: Option<Form>) -> Result<(), Error> {
    let have = form_of(regions, name);
    if have.is_some() && (wanted.is_none() || wanted == have) {
        return Ok(());
    }
    let valid: Vec<&str> = BuiltIn::names()
        .into_iter()
        .chain(regions.iter().map(|(declared, _)| declared))
        .filter(|candidate| wanted.is_none() || form_of(regions, candidate) == wanted)
        .collect();
    let problem = match (have, wanted) {
        (Some(have), Some(wanted)) => format!("a {}, but it needs a {}", have.name(), wanted.name()),
        _ => match closest(name, &valid) {
            Some(close) => format!("which is no region (did you mean '{close}'?)"),
            None => "which is no region".to_owned(),
        },
    };
    let kind = wanted.map_or("region", Form::name);
    let hint = if regions.is_empty() {
        " (this recipe declares no regions of its own: add them with `Recipe::with_region` or a [region.NAME] table)"
    } else {
        ""
    };
    Err(Error::Recipe(format!(
        "{place}: `{key}` names '{name}', {problem}; valid {kind}s: {}{hint}",
        if valid.is_empty() {
            "none".to_owned()
        } else {
            valid.join(", ")
        }
    )))
}

/// `Ok` if the declared regions are consistent: none is called like a built-in one, each refers only to regions of the
/// form its field needs, and no region is made from itself, however indirectly.
///
/// # Errors
/// [`Error::Recipe`] naming the region (and the field and name it refers to by, as [`refer`] does), or the regions of the
/// cycle.
pub(super) fn check_regions(regions: &Regions) -> Result<(), Error> {
    for (name, region) in regions.iter() {
        if BuiltIn::named(name).is_some() {
            return Err(Error::Recipe(format!(
                "region '{name}' is a built-in region ({}) and cannot be declared",
                BuiltIn::names().join(", ")
            )));
        }
        for (key, target, form) in region.refers_to() {
            refer(regions, &format!("region '{name}'"), key, target, Some(form))?;
        }
    }
    no_cycle(regions)
}

/// `Ok` unless declared regions refer to each other in a cycle. One walk that visits every region once and keeps no
/// call stack, so a chain of thousands of regions, or of paths that all lead to one region, is no trouble.
fn no_cycle(regions: &Regions) -> Result<(), Error> {
    let names: Vec<&str> = regions.iter().map(|(name, _)| name).collect();
    let made_from: Vec<Vec<&str>> = names.iter().map(|name| regions.made_from(name)).collect();
    let index: HashMap<&str, usize> = names.iter().enumerate().map(|(at, name)| (*name, at)).collect();
    let mut done = vec![false; names.len()];
    // where each region on the current path stands in it, so that a cycle is the path from there
    let mut on_path: Vec<Option<usize>> = vec![None; names.len()];
    for start in 0..names.len() {
        if done[start] {
            continue;
        }
        let mut path = vec![start];
        let mut walks = vec![made_from[start].iter()];
        on_path[start] = Some(0);
        while let Some(walk) = walks.last_mut() {
            let Some(target) = walk.next() else {
                if let Some(finished) = path.pop() {
                    (done[finished], on_path[finished]) = (true, None);
                }
                walks.pop();
                continue;
            };
            // a built-in or an unknown name is no region of ours: typing says what is wrong with an unknown one
            let Some(&next) = index.get(target) else { continue };
            if let Some(from) = on_path[next] {
                let cycle: Vec<String> = path[from..]
                    .iter()
                    .chain([&next])
                    .map(|region| format!("'{}'", names[*region]))
                    .collect();
                return Err(Error::Recipe(format!(
                    "regions refer to each other in a cycle: {}",
                    cycle.join(" -> ")
                )));
            }
            if !done[next] {
                on_path[next] = Some(path.len());
                path.push(next);
                walks.push(made_from[next].iter());
            }
        }
    }
    Ok(())
}

/// The candidate that `name` is most likely a slip of the finger for: at most two letters away, ignoring case, and
/// fewer than `name` is long (a one-letter name is never close to another). The first of equally close ones.
fn closest<'a>(name: &str, candidates: &[&'a str]) -> Option<&'a str> {
    const MOST_SLIPS: usize = 2; // constant: two edits are a slip of the finger, three are another word
    let typed = lowered(name);
    let mut best: Option<(usize, &'a str)> = None;
    for candidate in candidates {
        let apart = distance(&typed, &lowered(candidate));
        if apart <= MOST_SLIPS && apart < typed.len() && best.is_none_or(|(nearest, _)| apart < nearest) {
            best = Some((apart, candidate));
        }
    }
    best.map(|(_, candidate)| candidate)
}

/// `text` in lower case, as letters.
fn lowered(text: &str) -> Vec<char> {
    text.chars().flat_map(char::to_lowercase).collect()
}

/// The Levenshtein distance between two texts: the fewest letters to insert, delete or change to turn one into the
/// other.
fn distance(one: &[char], other: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=other.len()).collect();
    for (at, letter) in one.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = at + 1;
        for (to, wanted) in other.iter().enumerate() {
            let above = row[to + 1];
            row[to + 1] = (diagonal + usize::from(letter != wanted))
                .min(above + 1)
                .min(row[to] + 1);
            diagonal = above;
        }
    }
    row[other.len()]
}
