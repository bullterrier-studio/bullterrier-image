//! What a recipe asks of the photo it is rendered on: [`Needs`], and the maps its regions can be handed.

use std::collections::{BTreeSet, HashMap};

use super::built_in::{BuiltIn, Origin};
use super::references::{Form, refer};
use super::region::Subject;
use super::{Recipe, Region, Regions};
use crate::{Error, Presets};

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
    /// The last step (from 0) that names it, or that first needs a region made from it: when it can be dropped.
    pub(crate) last: usize,
}

impl Uses {
    /// The one use of the selection `name` (a built-in region or a declared selection) by a request for its map.
    ///
    /// # Errors
    /// [`Error::Recipe`] if `name` is no selection of `regions`, naming the ones there are and the closest.
    fn of_selection(regions: &Regions, name: &str) -> Result<Self, Error> {
        refer(regions, "region_map", "name", name, Some(Form::Selection))?;
        Ok(Self(vec![Use {
            step: 0,
            place: "the request".to_owned(),
            key: "name".to_owned(),
            name: name.to_owned(),
        }]))
    }

    /// Every region these uses reach, in the order they are first reached, each once, and the last step that needs it.
    ///
    /// One walk that visits a region once however many steps and paths lead to it and keeps no call stack. A region is
    /// needed up to the last step that names it and up to the first step that needs a region made from it: from then on
    /// that region is made and kept, and what it was made from is no longer needed for it.
    pub(crate) fn reach(&self, regions: &Regions) -> Vec<Reach> {
        let mut reached: Vec<Reach> = Vec::new();
        // the step each region was first reached in
        let mut first: Vec<usize> = Vec::new();
        let mut at: HashMap<&str, usize> = HashMap::new();
        for used in &self.0 {
            let place = format!("{}, `{}`", used.place, used.key);
            let mut todo = vec![used.name.as_str()];
            while let Some(name) = todo.pop() {
                if at.contains_key(name) {
                    continue;
                }
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
                first.push(used.step);
                todo.extend(regions.made_from(name));
            }
        }
        for used in &self.0 {
            let last = &mut reached[at[used.name.as_str()]].last;
            *last = (*last).max(used.step);
        }
        for (maker, step) in first.iter().enumerate() {
            for made_from in regions.made_from(&reached[maker].name) {
                let last = &mut reached[at[made_from]].last;
                *last = (*last).max(*step);
            }
        }
        reached
    }

    /// What these uses ask of the photo, given the regions of the recipe and whether its frame needs the subject.
    pub(super) fn needs(&self, regions: &Regions, frame_needs_subject: bool) -> Needs {
        let mut needs = Needs::default();
        if frame_needs_subject {
            needs.maps.insert(BuiltIn::Subject.name().to_owned());
        }
        for Reach { name, .. } in self.reach(regions) {
            let region = regions.get(&name);
            if BuiltIn::named(&name).is_some_and(|built_in| built_in.origin() == Origin::Found)
                || matches!(region, Some(Region::Given(_)))
            {
                needs.maps.insert(name);
            } else if matches!(region, Some(Region::Subject(Subject { per: Some(_), .. }))) {
                needs.finds_pieces = true;
            }
            // the built-in maps made from others or from the picture, and the regions made from other regions, need what
            // those need
        }
        needs
    }
}

impl Recipe {
    /// The regions a request for the map of the selection `name` reaches, as a render reaches those its steps use.
    ///
    /// # Errors
    /// [`Error::Recipe`] if `name` is no selection of the recipe, naming the ones there are and the closest.
    pub(crate) fn reach_of_selection(&self, name: &str) -> Result<Vec<Reach>, Error> {
        Ok(Uses::of_selection(&self.regions, name)?.reach(&self.regions))
    }

    /// The names a photo can be handed a map or a finder under for this recipe: `subject`, `depth`, then each `Given`
    /// region in the order it was declared. A photo given anything else is refused when it renders. Pure: nothing is
    /// checked.
    #[must_use]
    pub fn known_maps(&self) -> Vec<&str> {
        BuiltIn::found()
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
        let uses = self.check_rules(presets)?;
        Ok(uses.needs(&self.regions, self.frame.needs_subject()))
    }

    /// What [`Photo::region_map`](crate::Photo::region_map) of the selection `name` asks of the photo: the maps the
    /// region is made from and, if the frame crops around the subject, the subject. The steps use nothing here, and a
    /// cut of the finished picture by the subject is not made.
    ///
    /// # Errors
    /// What [`needs`](Self::needs) refuses, and [`Error::Recipe`] if `name` is no selection of the recipe.
    pub fn region_needs(&self, presets: &Presets, name: &str) -> Result<Needs, Error> {
        self.check_rules(presets)?;
        Ok(Uses::of_selection(&self.regions, name)?.needs(&self.regions, self.frame.crops_by_subject()))
    }
}
