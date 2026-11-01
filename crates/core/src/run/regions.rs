//! The regions of a render: the maps the steps work from, kept by name while a step still needs them.
//!
//! A map the photo gives (`subject`, `depth`, a `Given` region, `background`, `light`) is found before the first step and
//! handed to [`Evaluator::new`]. Every other region is made from the regions it refers to the first time a step asks for
//! it, for the working picture (framed and sized), and kept until the last step that uses it, or uses a region made from
//! it, has run: a render holds the regions the steps still ahead of it need, not all it has made.
//!
//! A region is a selection (a [`Layer`]) or a partition (the pieces of the picture, [`Labels`]); a selection can also be
//! asked for as a partition, which splits it into the pixels it chooses and the others.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use super::{Stage, laid};
use crate::ops::pieces;
use crate::ops::{bands, cast, gradient};
use crate::region::{Band, Bands, Combine, CombineOp, LinearGradient, Pick, Piece, RadialGradient, Subject};
use crate::{Error, ErrorKind, Finder, Labels, Layer, Picture, Region, Regions};

/// A region once made.
enum Made {
    Selection(Arc<Layer>),
    Pieces(Arc<Labels>),
}

/// The regions of a render by name.
pub(super) struct Evaluator {
    declared: Regions,
    made: BTreeMap<String, Made>,
    /// The regions the steps use, or that a region they use is made from.
    used: HashSet<String>,
    /// The regions to drop after each step (from 0): those that no step after it needs. A map held that no step needs
    /// (the subject only a frame used) goes after the first.
    due: BTreeMap<usize, Vec<String>>,
    /// The photo's subject finder, which a region that finds the subject in each piece asks about crops of the picture.
    finder: Option<Arc<dyn Finder>>,
}

/// The refusal of a `Subject` region with `per` (the region `name`, finding the subject in the pieces of `per`) when
/// the photo has no subject finder: a subject map of the whole photo cannot answer for a part of it.
fn no_finder(name: &str, per: &str) -> Error {
    Error::map_not_found(
        "subject",
        format!(
            "region '{name}' finds the subject in each piece of '{per}', but no subject finder was given: give the photo \
             one with `Photo::with_finder(\"subject\", ..)`"
        ),
    )
}

/// The refusal of the region `name`, which the core cannot make (`what`: its kind), needed at `place`.
fn refuse(name: &str, what: &str, place: &str) -> Error {
    Error::Recipe(format!(
        "region '{name}' ({what}), used at {place}, is not run by the core yet"
    ))
}

/// `Ok` if the core can make every region in `reached` (each a name and the place that first needs it): none is of a kind
/// it does not make yet, and a subject found in each piece has a finder to ask (`has_finder`). The one place a render
/// refuses a region, before anything is made or found.
///
/// # Errors
/// [`Error::Recipe`] naming the first such region and the place that needs it; [`Error::MapNotFound`] of the subject for a
/// region that finds it in each piece of a partition when there is no finder.
pub(super) fn check_makeable<'a>(
    declared: &Regions,
    reached: impl IntoIterator<Item = (&'a str, &'a str)>,
    has_finder: bool,
) -> Result<(), Error> {
    let reached: Vec<(&str, &str)> = reached.into_iter().collect();
    for (name, place) in &reached {
        if let Some(what) = declared.get(name).and_then(Region::not_yet) {
            return Err(refuse(name, what, place));
        }
    }
    for (name, _) in reached {
        if let (Some(Region::Subject(Subject { per: Some(per), .. })), false) = (declared.get(name), has_finder) {
            return Err(no_finder(name, per));
        }
    }
    Ok(())
}

impl Evaluator {
    /// The evaluator of the `declared` regions of a recipe, holding `held`, the maps of the photo that the steps use, and
    /// told by `last` when each region is no longer needed. `finder` is the photo's subject finder, if it has one (see
    /// [`check_finder`]).
    pub(super) fn new(
        declared: &Regions,
        held: BTreeMap<String, Arc<Layer>>,
        last: BTreeMap<String, usize>,
        finder: Option<Arc<dyn Finder>>,
    ) -> Self {
        let mut due: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for name in held.keys().filter(|name| !last.contains_key(*name)) {
            due.entry(0).or_default().push(name.clone());
        }
        for (name, step) in &last {
            due.entry(*step).or_default().push(name.clone());
        }
        Self {
            declared: declared.clone(),
            made: held
                .into_iter()
                .map(|(name, map)| (name, Made::Selection(map)))
                .collect(),
            used: last.into_keys().collect(),
            due,
            finder,
        }
    }

    /// The region `name` as a selection of `picture`, the working picture, made if need be with what it is made from.
    /// `name` is one of the regions the recipe uses, and so a region its check accepted: a map the photo gives was
    /// handed to [`new`](Self::new), the others are declared.
    ///
    /// # Errors
    /// [`Error::Recipe`] for a region the core cannot make, which a render refuses before it starts, for a `fit` that
    /// chooses nothing, and for a partition (which a recipe's check does not let a selection be taken from).
    pub(super) fn selection(&mut self, name: &str, picture: &Picture<'_>) -> Result<Arc<Layer>, Error> {
        self.ensure(name, picture)?;
        self.selected(name)
    }

    /// The region `name` as pieces of `picture`: a partition as it is, a selection split into the pixels it chooses (more
    /// than half) and the others, numbered by first pixel (one piece if it chooses all or none).
    ///
    /// # Errors
    /// As [`selection`](Self::selection).
    pub(super) fn pieces(&mut self, name: &str, picture: &Picture<'_>) -> Result<Arc<Labels>, Error> {
        self.ensure(name, picture)?;
        if let Some(Made::Pieces(labels)) = self.made.get(name) {
            return Ok(Arc::clone(labels));
        }
        let layer = self.selected(name)?;
        let (labels, count) = pieces::of_selection(layer.values());
        Ok(Arc::new(Labels::new(picture.height(), picture.width(), labels, count)?))
    }

    /// The selection `name` as it is held now, if it is: what a trace shows.
    pub(super) fn held(&self, name: &str) -> Option<Arc<Layer>> {
        self.selected(name).ok()
    }

    /// The declared regions that the steps use, or that a region they use is made from, made now, as a trace shows them:
    /// in the order they are declared, a selection or the pieces of a partition.
    ///
    /// # Errors
    /// As [`selection`](Self::selection).
    pub(super) fn shown(&mut self, picture: &Picture<'_>) -> Result<Vec<Stage>, Error> {
        let names: Vec<String> = self
            .declared
            .iter()
            .map(|(name, _)| name)
            .filter(|name| self.used.contains(*name))
            .map(str::to_owned)
            .collect();
        let mut stages = Vec::with_capacity(names.len());
        for name in names {
            self.ensure(&name, picture)?;
            stages.push(match self.made.get(&name) {
                Some(Made::Selection(layer)) => Stage::Selection {
                    layer: Layer::clone(layer),
                    name,
                },
                Some(Made::Pieces(labels)) => Stage::Partition {
                    labels: Labels::clone(labels),
                    name,
                },
                None => return Err(Error::Recipe(format!("region '{name}' is needed before it is made"))),
            });
        }
        Ok(stages)
    }

    /// Drops the regions that no step after `step` (from 0) needs.
    pub(super) fn release(&mut self, step: usize) {
        for name in self.due.remove(&step).into_iter().flatten() {
            self.made.remove(&name);
        }
    }

    /// Makes the region `name` and everything it is made from that is not made yet.
    fn ensure(&mut self, name: &str, picture: &Picture<'_>) -> Result<(), Error> {
        // what a region is made from comes first; no recursion, so a long chain of regions needs no deep call stack
        let mut todo = vec![name.to_owned()];
        while let Some(next) = todo.last() {
            if self.made.contains_key(next) {
                todo.pop();
                continue;
            }
            let missing: Vec<String> = self
                .declared
                .made_from(next)
                .into_iter()
                .filter(|made_from| !self.made.contains_key(*made_from))
                .map(str::to_owned)
                .collect();
            if missing.is_empty() {
                let region = self.make(next, picture)?;
                self.made.insert(next.clone(), region);
                todo.pop();
            } else {
                todo.extend(missing);
            }
        }
        Ok(())
    }

    /// The selection `name`, which is made already.
    fn selected(&self, name: &str) -> Result<Arc<Layer>, Error> {
        match self.made.get(name) {
            Some(Made::Selection(layer)) => Ok(Arc::clone(layer)),
            Some(Made::Pieces(_)) => Err(Error::Recipe(format!(
                "region '{name}' is a partition, but a selection is needed here"
            ))),
            None => Err(Error::Recipe(format!("region '{name}' is needed before it is made"))),
        }
    }

    /// The partition `name`, which is made already.
    fn partitioned(&self, name: &str) -> Result<Arc<Labels>, Error> {
        match self.made.get(name) {
            Some(Made::Pieces(labels)) => Ok(Arc::clone(labels)),
            Some(Made::Selection(_)) => Err(Error::Recipe(format!(
                "region '{name}' is a selection, but a partition is needed here"
            ))),
            None => Err(Error::Recipe(format!("region '{name}' is needed before it is made"))),
        }
    }

    /// The region `name`, made from the regions it refers to, which are made already.
    fn make(&self, name: &str, picture: &Picture<'_>) -> Result<Made, Error> {
        let (height, width) = (picture.height(), picture.width());
        let selection =
            |values: Vec<f32>| Layer::new(height, width, values).map(|layer| Made::Selection(Arc::new(layer)));
        let partition = |(labels, count): (Vec<i32>, usize)| {
            Labels::new(height, width, labels, count).map(|labels| Made::Pieces(Arc::new(labels)))
        };
        match self.declared.get(name) {
            Some(Region::Band(Band { of, low, high, feather })) => {
                selection(bands::band(self.selected(of)?.values(), *low, *high, *feather))
            }
            Some(Region::Bands(Bands { of, count })) => {
                partition(bands::partition(self.selected(of)?.values(), *count))
            }
            Some(Region::Combine(Combine { op, of })) => {
                // selections lie in 0..1, so the value that leaves the others as they are is 1 for the product and the
                // minimum, 0 for the maximum
                let none = match op {
                    CombineOp::Multiply | CombineOp::Min => 1.0,
                    CombineOp::Max => 0.0,
                };
                let mut combined = vec![none; height * width];
                for other in of {
                    for (value, other) in combined.iter_mut().zip(self.selected(other)?.values()) {
                        *value = match op {
                            CombineOp::Multiply => *value * other,
                            CombineOp::Min => value.min(*other),
                            CombineOp::Max => value.max(*other),
                        };
                    }
                }
                selection(combined)
            }
            Some(Region::LinearGradient(LinearGradient { angle })) => {
                selection(gradient::linear(height, width, *angle))
            }
            Some(Region::RadialGradient(RadialGradient { x, y, radius })) => {
                selection(gradient::radial(height, width, *x, *y, *radius))
            }
            Some(Region::Grid(spec)) => {
                let fit = spec.fit.as_deref().map(|fit| self.selected(fit)).transpose()?;
                partition(laid::grid(name, spec, fit.as_deref(), picture)?)
            }
            Some(Region::Path(spec)) => {
                let fit = spec.fit.as_deref().map(|fit| self.selected(fit)).transpose()?;
                selection(laid::path(name, spec, fit.as_deref(), picture)?)
            }
            // the subject of the whole picture is the built-in one, which is held
            Some(Region::Subject(Subject { per: None, .. })) => self.selected("subject").map(Made::Selection),
            Some(Region::Subject(Subject {
                per: Some(per),
                context,
                threshold,
            })) => {
                let finder = self.finder.as_ref().ok_or_else(|| no_finder(name, per))?;
                let labels = self.partitioned(per)?;
                selection(found_in_pieces(
                    finder.as_ref(),
                    picture,
                    &labels,
                    *context,
                    *threshold,
                )?)
            }
            Some(Region::Pick(Pick { of, pieces: picked })) => {
                let labels = self.partitioned(of)?;
                let mut numbers = Vec::with_capacity(picked.len());
                for piece in picked {
                    match piece {
                        Piece::Number(number) => numbers.push(*number),
                        // the check lets an id through only for an `Svg` region, which the core cannot make yet and a
                        // render refuses before it makes anything (`check_makeable`)
                        Piece::Id(id) => {
                            return Err(Error::Recipe(format!(
                                "region '{name}': `pieces` names the id '{id}' of '{of}', which the core cannot read yet"
                            )));
                        }
                    }
                }
                selection(pieces::pick(labels.values(), labels.pieces(), &numbers))
            }
            // a map the photo gives is held, never made, and a render refuses the kinds the core cannot make before it
            // makes anything (`check_makeable`)
            Some(Region::Given(_) | Region::Shards(_) | Region::Cuts(_) | Region::Svg(_)) | None => {
                Err(Error::Recipe(format!("region '{name}' is not made from other regions")))
            }
        }
    }
}

/// The subject of `picture` found piece by piece: for each piece of `labels`, in order, the finder is asked about the crop
/// of the picture round the piece (the piece's box grown by `context` of its own size on each side, clipped), and of its
/// answer the pixels of the piece itself that it is surer of than `threshold` are kept, as it gave them. A piece the
/// finder finds no subject in adds nothing.
///
/// # Errors
/// What the finder fails with other than "no subject" ([`ErrorKind::SubjectNotFound`]); [`Error::MapNotFound`] of the
/// subject, naming the piece, for an answer of another size than the crop.
fn found_in_pieces(
    finder: &dyn Finder,
    picture: &Picture<'_>,
    labels: &Labels,
    context: f64,
    threshold: f64,
) -> Result<Vec<f32>, Error> {
    let (height, width) = (picture.height(), picture.width());
    // the same single precision as the matte it is compared with
    let threshold = cast::single(threshold);
    let mut found = vec![0.0_f32; height * width];
    let boxes = pieces::piece_bounds(labels.values(), height, width, labels.pieces());
    for (piece, window) in boxes.into_iter().enumerate() {
        let window = pieces::grown(window, context, height, width);
        let (rows, columns) = (window.bottom - window.top, window.right - window.left);
        let crop = pieces::crop(picture.pixels(), width, window);
        let matte = match finder.find(&Picture::new(rows, columns, crop.as_flattened())?) {
            Ok(matte) => matte,
            // nothing stands out in this piece: it adds nothing
            Err(error) if error.kind() == ErrorKind::SubjectNotFound => continue,
            Err(error) => return Err(error),
        };
        if (matte.height(), matte.width()) != (rows, columns) {
            return Err(Error::map_not_found(
                "subject",
                format!(
                    "the subject finder gave a map of {} x {} for piece {piece}, whose view (rows {}..{}, columns \
                     {}..{}) is {rows} x {columns}",
                    matte.height(),
                    matte.width(),
                    window.top,
                    window.bottom,
                    window.left,
                    window.right
                ),
            ));
        }
        for (row, line) in matte.values().chunks_exact(columns).enumerate() {
            for (column, &sure) in line.iter().enumerate() {
                let at = (window.top + row) * width + window.left + column;
                if pieces::piece(labels.values()[at]) == piece && sure > threshold {
                    found[at] = sure;
                }
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests;
