//! What a recipe asks of the photo it is rendered on: [`Needs`], and the maps its regions can be handed.

use std::collections::{BTreeSet, HashSet};

use super::region::Subject;
use super::{Recipe, Region, Regions};
use crate::{Error, Presets};

/// The maps a photo is handed or finds. The other built-in regions are made from them (`background`) or from the picture
/// (`light`).
const FOUND: [&str; 2] = ["subject", "depth"];

/// The names of the regions that the steps of a recipe refer to, in the order the checks met them, with repeats.
#[derive(Debug)]
pub(crate) struct Uses(pub(super) Vec<String>);

/// What rendering a recipe asks of the photo: the maps it must be given or find, and whether it needs a finder to ask
/// about pieces of it.
///
/// Only the regions the steps use count, however indirectly (a `Band` of `depth` that a mask uses needs `depth`); a
/// region that is declared and not used needs nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Needs {
    /// The maps the photo must hold or find: `subject` (the built-in `subject` and `background`, a `Subject` region
    /// without `per`, and a frame that crops around it or cuts the picture out by it), `depth`, and the name of each
    /// `Given` region used. Never `light`: it is made from the picture itself.
    pub maps: BTreeSet<String>,
    /// Whether a region used finds the subject in each piece of a partition (a `Subject` with `per`): the photo needs a
    /// subject finder, because a map of the whole photo cannot answer for a part of it.
    pub finds_pieces: bool,
}

impl Uses {
    /// What these uses ask of the photo, given the regions of the recipe and whether its frame needs the subject.
    pub(super) fn needs(&self, regions: &Regions, frame_needs_subject: bool) -> Needs {
        let mut needs = Needs::default();
        if frame_needs_subject {
            needs.maps.insert("subject".to_owned());
        }
        let mut seen: HashSet<&str> = HashSet::new();
        let mut todo: Vec<&str> = self.0.iter().map(String::as_str).collect();
        while let Some(name) = todo.pop() {
            if !seen.insert(name) {
                continue;
            }
            match (name, regions.get(name)) {
                // the built-in selections: `background` is the subject upside down
                ("subject" | "background", _) => {
                    needs.maps.insert("subject".to_owned());
                }
                ("depth", _) => {
                    needs.maps.insert("depth".to_owned());
                }
                (_, Some(region)) => {
                    match region {
                        Region::Given(_) => {
                            needs.maps.insert(name.to_owned());
                        }
                        Region::Subject(Subject { per: None, .. }) => {
                            needs.maps.insert("subject".to_owned());
                        }
                        Region::Subject(Subject { per: Some(_), .. }) => needs.finds_pieces = true,
                        _ => {}
                    }
                    todo.extend(region.refers_to().into_iter().map(|(_, made_from, _)| made_from));
                }
                // `light`, which is made from the picture
                _ => {}
            }
        }
        needs
    }
}

impl Recipe {
    /// The names a photo can be handed a map or a finder under for this recipe: `subject`, `depth`, then each `Given`
    /// region in the order it was declared. A photo given anything else is refused when it renders. Pure: nothing is
    /// checked.
    #[must_use]
    pub fn known_maps(&self) -> Vec<&str> {
        FOUND
            .into_iter()
            .chain(
                self.regions
                    .iter()
                    .filter(|(_, region)| matches!(region, Region::Given(_)))
                    .map(|(name, _)| name),
            )
            .collect()
    }

    /// What rendering this recipe asks of the photo, once the recipe is [`checked`](Self::checked) (with `presets`: a
    /// preset can set the field a step works from).
    ///
    /// # Errors
    /// What `checked` refuses.
    pub fn needs(&self, presets: &Presets) -> Result<Needs, Error> {
        let uses = self.check_in(crate::registry::registry(), presets)?;
        Ok(uses.needs(&self.regions, self.frame.needs_subject()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use crate::frame::{Anchor, Transparent};
    use crate::region::{Band, Combine};
    use crate::testing::regions;
    use crate::{Each, Error, Frame, Needs, Presets, Recipe, Step};

    /// A recipe of one `grade` step masked by `mask`, declaring `declared`.
    fn masked(mask: &str, declared: &serde_json::Value) -> Recipe {
        let mut recipe = Recipe::new([Step::new("grade").mask(mask)]);
        recipe.regions = regions(declared.clone());
        recipe
    }

    /// What `recipe` needs: the maps, and whether it finds pieces.
    fn needs(recipe: &Recipe) -> Result<(BTreeSet<&'static str>, bool), Error> {
        let needs = recipe.needs(&Presets::new())?;
        let wanted = ["subject", "depth", "light", "star", "moon", "cutout"];
        // every map is one of those: a name outside them would show up in `rest`
        let maps = wanted
            .into_iter()
            .filter(|name| needs.maps.contains(*name))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            needs.maps.len(),
            maps.len(),
            "a map outside {wanted:?}: {:?}",
            needs.maps
        );
        Ok((maps, needs.finds_pieces))
    }

    fn set(maps: &[&'static str]) -> BTreeSet<&'static str> {
        maps.iter().copied().collect()
    }

    #[test]
    #[expect(clippy::too_many_lines, reason = "a table of rows, one recipe each")]
    fn a_recipe_needs_the_maps_its_steps_use_however_indirectly_and_nothing_else() -> Result<(), Error> {
        let none = json!({});
        let chain = json!({
            "near": {"kind": "band", "of": "depth"},
            "nearer": {"kind": "band", "of": "near", "low": 0.8},
        });
        let both = json!({
            "front": {"kind": "band", "of": "depth"},
            "both": {"kind": "combine", "of": ["front", "subject"]},
        });
        let given = json!({"star": {"kind": "given"}, "moon": {"kind": "given"}});
        let found = json!({
            "panes": {"kind": "grid"}, "found": {"kind": "subject", "per": "panes"},
        });
        let found_in_depth = json!({
            "near": {"kind": "band", "of": "depth"},
            "panes": {"kind": "grid", "fit": "near"}, "found": {"kind": "subject", "per": "panes"},
        });
        let found_in_subject = json!({
            "panes": {"kind": "grid", "fit": "subject"}, "found": {"kind": "subject", "per": "panes"},
        });
        // (what the row is about, the recipe, the maps it needs, whether it finds pieces)
        let rows = [
            (
                "a step with no mask",
                Recipe::new([Step::new("grade")]),
                set(&[]),
                false,
            ),
            ("the subject", masked("subject", &none), set(&["subject"]), false),
            (
                "the background is made from the subject",
                masked("background", &none),
                set(&["subject"]),
                false,
            ),
            ("the depth", masked("depth", &none), set(&["depth"]), false),
            (
                "the light is made from the picture, never asked for",
                masked("light", &none),
                set(&[]),
                false,
            ),
            ("a band of the depth", masked("near", &chain), set(&["depth"]), false),
            (
                "a band of a band of the depth",
                masked("nearer", &chain),
                set(&["depth"]),
                false,
            ),
            (
                "a combine of a band and the subject",
                masked("both", &both),
                set(&["depth", "subject"]),
                false,
            ),
            ("a given region", masked("star", &given), set(&["star"]), false),
            (
                "a band of the light",
                masked("dark", &json!({"dark": {"kind": "band", "of": "light"}})),
                set(&[]),
                false,
            ),
            (
                "the subject as a region",
                masked("me", &json!({"me": {"kind": "subject"}})),
                set(&["subject"]),
                false,
            ),
            (
                "the subject found in each piece",
                masked("found", &found),
                set(&[]),
                true,
            ),
            (
                "a grid laid over the subject, the subject found in its pieces",
                masked("found", &found_in_subject),
                set(&["subject"]),
                true,
            ),
            (
                "a grid laid over a band of depth, the subject found in its pieces",
                masked("found", &found_in_depth),
                set(&["depth"]),
                true,
            ),
            (
                "a grid laid over a depth band that nothing uses",
                masked("subject", &found_in_depth),
                set(&["subject"]),
                false,
            ),
            (
                "regions that are declared and not used",
                masked("depth", &given),
                set(&["depth"]),
                false,
            ),
        ];
        for (what, recipe, maps, finds) in rows {
            assert_eq!(needs(&recipe)?, (maps, finds), "{what}");
        }
        Ok(())
    }

    #[test]
    fn every_place_a_name_can_stand_counts_as_a_use() -> Result<(), Error> {
        let given =
            json!({"star": {"kind": "given"}, "moon": {"kind": "given"}, "panes": {"kind": "grid", "fit": "star"}});
        let at = |recipe: Recipe| -> Result<BTreeSet<String>, Error> { Ok(recipe.needs(&Presets::new())?.maps) };
        let with = |steps: Vec<crate::Entry>| {
            let mut recipe = Recipe::new(steps);
            recipe.regions = regions(given.clone());
            recipe
        };
        let drive = json!({"by": "moon", "start": 0, "end": 1});
        let masked_each = {
            let mut each = Each::new("panes", [None]);
            each.mask = Some("moon".to_owned());
            each
        };
        let in_a_look = Each::new("panes", [Some(Step::new("grade").mask("moon"))]);
        assert_eq!(
            at(with(vec![Step::new("grade").param("exposure", drive).into()]))?,
            ["moon"].map(String::from).into()
        );
        assert_eq!(
            at(with(vec![masked_each.into()]))?,
            ["moon", "star"].map(String::from).into()
        );
        assert_eq!(
            at(with(vec![in_a_look.into()]))?,
            ["moon", "star"].map(String::from).into()
        );
        Ok(())
    }

    #[test]
    fn the_regions_a_recipe_is_made_from_are_visited_once_each_however_many_paths_lead_to_them() -> Result<(), Error> {
        // 40 levels, each made from the next twice: 2^40 paths to the last, which is a band of the depth
        let mut recipe = Recipe::new([Step::new("grade").mask("d0")]);
        for level in 0..40 {
            let next = format!("d{}", level + 1);
            recipe.regions.insert(
                format!("d{level}"),
                Combine {
                    of: vec![next.clone(), next],
                    ..Combine::default()
                },
            );
        }
        recipe.regions.insert(
            "d40",
            Band {
                of: "depth".to_owned(),
                ..Band::default()
            },
        );
        assert_eq!(needs(&recipe)?, (set(&["depth"]), false));
        Ok(())
    }

    #[test]
    fn a_frame_that_crops_around_the_subject_or_cuts_by_it_needs_it() -> Result<(), Error> {
        for (what, frame, maps) in [
            ("no frame", Frame::default(), set(&[])),
            (
                "a crop around the subject",
                Frame {
                    anchor: Anchor::Subject,
                    ..Frame::default()
                },
                set(&["subject"]),
            ),
            (
                "a cut-out by the subject",
                Frame {
                    transparent: Transparent::Background,
                    ..Frame::default()
                },
                set(&["subject"]),
            ),
            (
                "a circle cut-out",
                Frame {
                    transparent: Transparent::Circle,
                    ..Frame::default()
                },
                set(&[]),
            ),
        ] {
            let mut recipe = Recipe::new([Step::new("grade")]);
            recipe.frame = frame;
            assert_eq!(needs(&recipe)?, (maps, false), "{what}");
        }
        Ok(())
    }

    #[test]
    fn a_recipe_the_checks_refuse_has_no_needs() {
        let unknown = Recipe::new([Step::new("no_such_effect")]);
        assert!(matches!(unknown.needs(&Presets::new()), Err(Error::UnknownEffect(_))));
        let no_region = masked("nowhere", &json!({}));
        assert!(matches!(no_region.needs(&Presets::new()), Err(Error::Recipe(_))));
        assert_eq!(
            Needs::default(),
            Needs {
                maps: BTreeSet::new(),
                finds_pieces: false
            }
        );
    }

    #[test]
    fn the_maps_a_photo_can_be_given_are_the_two_found_ones_and_the_given_regions_in_order() {
        let mut recipe = Recipe::new([Step::new("grade")]);
        assert_eq!(recipe.known_maps(), ["subject", "depth"]);
        recipe.regions = regions(json!({
            "zeta": {"kind": "given"}, "panes": {"kind": "grid"}, "alpha": {"kind": "given"},
            "near": {"kind": "band", "of": "depth"},
        }));
        assert_eq!(recipe.known_maps(), ["subject", "depth", "zeta", "alpha"]);
        // pure: a recipe the checks refuse still lists them
        recipe.steps.clear();
        assert_eq!(recipe.known_maps(), ["subject", "depth", "zeta", "alpha"]);
    }
}
