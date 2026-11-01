//! What a recipe asks of the photo it is rendered on: [`Needs`], and the maps its regions can be handed.

use std::collections::{BTreeSet, HashMap};

use super::region::Subject;
use super::{Recipe, Region, Regions};
use crate::{Error, Presets};

/// The maps a photo is handed or finds. The other built-in regions are made from them (`background`) or from the picture
/// (`light`).
const FOUND: [&str; 2] = ["subject", "depth"];

/// One place a step of a recipe names a region: a `mask`, a drive's `by`, a field parameter, a `partition`, an `each`.
#[derive(Debug)]
pub(crate) struct Use {
    /// The step it is in, counting from 0; the looks of an `each` step count as that step.
    pub(super) step: usize,
    /// Where the step is, as `step 2` or `step 2, look 1`.
    pub(super) place: String,
    /// The key that names the region: `mask`, `by`, `partition`, `each` or the field parameter's name.
    pub(super) key: String,
    pub(super) name: String,
}

/// The places the steps of a recipe name a region, in the order the checks met them.
#[derive(Debug)]
pub(crate) struct Uses(pub(super) Vec<Use>);

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

/// A region that the steps use, directly or because a region they use is made from it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Reach {
    pub(crate) name: String,
    /// The first place that uses it, written like "step 2, `mask`"; for a region only used through another, with that
    /// region's name added, like "step 2, `mask` (through 'near')".
    pub(crate) place: String,
    /// The last step (from 0) that uses it or uses a region made from it: when it can be dropped.
    pub(crate) last: usize,
}

/// What `name` is made from: `background` from the subject, a declared region from the regions it refers to, nothing
/// else (the other built-in regions are maps).
fn made_from<'a>(regions: &'a Regions, name: &str) -> Vec<&'a str> {
    if name == "background" {
        return vec!["subject"];
    }
    regions.get(name).map_or_else(Vec::new, |region| {
        region
            .refers_to()
            .into_iter()
            .map(|(_, made_from, _)| made_from)
            .collect()
    })
}

impl Uses {
    /// Every region these uses reach, in the order they are first reached, each once: one walk that keeps no call
    /// stack and visits a region once however many paths lead to it (the uses come in step order, so a region already
    /// reached by this step has had everything it is made from reached too).
    pub(crate) fn reach(&self, regions: &Regions) -> Vec<Reach> {
        let mut reached: Vec<Reach> = Vec::new();
        let mut at: HashMap<&str, usize> = HashMap::new();
        for used in &self.0 {
            let place = format!("{}, `{}`", used.place, used.key);
            let mut todo = vec![used.name.as_str()];
            while let Some(name) = todo.pop() {
                match at.get(name) {
                    Some(&seen) if reached[seen].last == used.step => continue,
                    Some(&seen) => reached[seen].last = used.step,
                    None => {
                        at.insert(name, reached.len());
                        reached.push(Reach {
                            name: name.to_owned(),
                            place: if name == used.name {
                                place.clone()
                            } else {
                                format!("{place} (through '{}')", used.name)
                            },
                            last: used.step,
                        });
                    }
                }
                todo.extend(made_from(regions, name));
            }
        }
        reached
    }

    /// What these uses ask of the photo, given the regions of the recipe and whether its frame needs the subject.
    pub(super) fn needs(&self, regions: &Regions, frame_needs_subject: bool) -> Needs {
        let mut needs = Needs::default();
        if frame_needs_subject {
            needs.maps.insert("subject".to_owned());
        }
        for Reach { name, .. } in self.reach(regions) {
            match (name.as_str(), regions.get(&name)) {
                ("subject" | "depth", _) | (_, Some(Region::Given(_))) => {
                    needs.maps.insert(name);
                }
                (_, Some(Region::Subject(Subject { per: None, .. }))) => {
                    needs.maps.insert("subject".to_owned());
                }
                (_, Some(Region::Subject(Subject { per: Some(_), .. }))) => needs.finds_pieces = true,
                // `background` and the regions made from other regions need what those need; `light` is made from the
                // picture
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

    /// The (name, place, last step) of every region `recipe` reaches, in the order they are reached.
    fn reached(recipe: &Recipe) -> Result<Vec<(String, String, usize)>, Error> {
        let uses = recipe.check_in(&crate::testing::registry()?, &Presets::new())?;
        Ok(uses
            .reach(&recipe.regions)
            .into_iter()
            .map(|reach| (reach.name, reach.place, reach.last))
            .collect())
    }

    #[test]
    fn each_region_is_reached_once_with_the_first_place_that_uses_it_and_the_last_step_that_needs_it()
    -> Result<(), Error> {
        // step 1 masks by `near` (a band of the depth), step 2 by `subject`, step 3 by `far` (a band of the depth too),
        // step 4 by `near` again, and step 5 drives a number by `background`
        let mut recipe = Recipe::new([
            Step::new("probe").mask("near"),
            Step::new("probe").mask("subject"),
            Step::new("probe").mask("far"),
            Step::new("probe").mask("near"),
            Step::new("probe").param("cell", json!({"by": "background", "start": 4, "end": 9})),
        ]);
        recipe.regions = regions(json!({
            "near": {"kind": "band", "of": "depth"},
            "far": {"kind": "band", "of": "depth", "high": 0.5},
            "unused": {"kind": "band", "of": "light"},
        }));
        let place = |name: &str, place: &str, last: usize| (name.to_owned(), place.to_owned(), last);
        assert_eq!(
            reached(&recipe)?,
            [
                place("near", "step 1, `mask`", 3),
                // reached through `near`, first at step 1, needed until `far` has been used by step 3 and `near` by step 4
                place("depth", "step 1, `mask` (through 'near')", 3),
                place("subject", "step 2, `mask`", 4),
                place("far", "step 3, `mask`", 2),
                place("background", "step 5, `by`", 4),
            ]
        );
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
