//! The regions of a render: the maps the steps work from, kept by name while a step still needs them.
//!
//! A map the photo gives (`subject`, `depth`, a `Given` region, `background`, `light`) is found before the first step and
//! handed to [`Evaluator::new`]. Every other region is made from the regions it refers to the first time a step asks for
//! it, for the working picture (framed and sized), and kept until the last step that uses it, or uses a region made from
//! it, has run: a render holds the regions the steps still ahead of it need, not all it has made.
//!
//! A region is a selection (a [`Layer`]) or a partition (the pieces of the picture, [`Labels`]); a selection can also be
//! asked for as a partition, which splits it into the pixels it chooses and the others.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::ops::pieces::{self, Bounds};
use crate::ops::{bands, cast, gradient, grid};
use crate::region::{Band, Bands, Combine, CombineOp, Grid, LinearGradient, Pick, Piece, RadialGradient, Subject};
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
    /// The last step (from 0) that needs each region; a region not named here is needed by none.
    last: BTreeMap<String, usize>,
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

/// `Ok` if the photo has a subject finder (`has_finder`) whenever one of the `reached` regions needs one.
///
/// # Errors
/// [`Error::MapNotFound`] of the subject, naming the first region that finds the subject in each piece of a partition.
pub(super) fn check_finder<'a>(
    declared: &Regions,
    reached: impl IntoIterator<Item = &'a str>,
    has_finder: bool,
) -> Result<(), Error> {
    for name in reached {
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
        Self {
            declared: declared.clone(),
            made: held
                .into_iter()
                .map(|(name, map)| (name, Made::Selection(map)))
                .collect(),
            last,
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

    /// Drops the regions that no step after `step` (from 0) needs.
    pub(super) fn release(&mut self, step: usize) {
        let last = &self.last;
        self.made
            .retain(|name, _| last.get(name).is_some_and(|needed| *needed > step));
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
            let missing: Vec<String> = self.declared.get(next).map_or_else(Vec::new, |region| {
                region
                    .refers_to()
                    .into_iter()
                    .map(|(_, made_from, _)| made_from)
                    .filter(|made_from| !self.made.contains_key(*made_from))
                    .map(str::to_owned)
                    .collect()
            });
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
                let mut layers = of.iter().map(|of| self.selected(of));
                let first = layers.next().transpose()?.map(|layer| layer.values().to_vec());
                let mut combined = first.unwrap_or_default();
                for layer in layers {
                    let layer = layer?;
                    for (value, other) in combined.iter_mut().zip(layer.values()) {
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
            Some(Region::Grid(Grid { fit, columns, rows, .. })) => {
                let frame = match fit {
                    None => Bounds::whole(height, width),
                    Some(fit) => pieces::bounds(self.selected(fit)?.values(), height, width).ok_or_else(|| {
                        Error::Recipe(format!(
                            "region '{name}': `fit` '{fit}' chooses nothing in this picture, so it has no frame"
                        ))
                    })?,
                };
                partition(grid::grid(height, width, frame, &columns.0, &rows.0))
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
                        // the partitions the core makes have no ids; only an `Svg` region does
                        Piece::Id(id) => {
                            return Err(Error::Recipe(format!(
                                "region '{name}': `pieces` names '{id}', which is no id of '{of}'; it has no ids (only svg \
                                 regions do)"
                            )));
                        }
                    }
                }
                selection(pieces::pick(labels.values(), labels.pieces(), &numbers))
            }
            _ => Err(Error::Recipe(format!("region '{name}' is not made by the core"))),
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
mod tests {
    use std::sync::Arc;

    use proptest::prelude::*;
    use serde_json::{Value, json};

    use super::*;
    use crate::frame::Flip;
    use crate::registry::Registry;
    use crate::testing::{Finds, Paint, Recorder, regions, registry};
    use crate::{Each, ErrorKind, Frame, Image, Options, Photo, Presets, Recipe, Step};

    /// A registry with `paint` and a `recorder` whose field parameter `near` logs the selection it is given.
    struct Rig {
        registry: Registry,
        recorder: Arc<Recorder>,
    }

    fn rig() -> Result<Rig, Error> {
        let registry = registry()?;
        registry.register(Arc::new(Paint { name: "paint" }))?;
        let recorder = Arc::new(Recorder::new("recorder", &[], &["near"]));
        registry.register(recorder.clone())?;
        Ok(Rig { registry, recorder })
    }

    impl Rig {
        fn render(&self, photo: &Photo<'_>, recipe: &Recipe, options: Options) -> Result<Image, Error> {
            photo.render_in(&self.registry, recipe, &Presets::new(), options)
        }

        /// The selection each step's `near` received, in order.
        fn seen(&self) -> Vec<Vec<f32>> {
            self.recorder
                .received()
                .into_iter()
                .map(|received| received.fields["near"].clone())
                .collect()
        }
    }

    /// A recipe of one recorder step for each of `names`, each working from that region, declaring `declared`.
    fn asking(names: &[&str], declared: &Value) -> Recipe {
        let mut recipe = Recipe::new(names.iter().map(|name| Step::new("recorder").param("near", *name)));
        recipe.regions = regions(declared.clone());
        recipe
    }

    /// The photo of `height` x `width` grey pixels with the maps `given` (name, values).
    fn grey<'a>(
        height: usize,
        width: usize,
        pixels: &'a [f32],
        given: &[(&str, Vec<f32>)],
    ) -> Result<Photo<'a>, Error> {
        let mut photo = Photo::new(Picture::new(height, width, pixels)?);
        for (name, values) in given {
            photo = photo.with_map(*name, Layer::new(height, width, values.clone())?)?;
        }
        Ok(photo)
    }

    /// The values the region `name` has on a `height` x `width` photo with the maps `given`, as a step receives them.
    fn made(
        height: usize,
        width: usize,
        declared: &Value,
        name: &str,
        given: &[(&str, Vec<f32>)],
    ) -> Result<Vec<f32>, Error> {
        let rig = rig()?;
        let pixels = vec![0.5; height * width * 3];
        let photo = grey(height, width, &pixels, given)?;
        rig.render(&photo, &asking(&[name], declared), Options::default())?;
        let mut seen = rig.seen();
        Ok(seen.remove(0))
    }

    /// `actual` is `expected` to within `by`.
    fn assert_close(actual: &[f32], expected: &[f32], by: f32, row: &str) {
        assert_eq!(actual.len(), expected.len(), "{row}");
        for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
            assert!((a - e).abs() <= by, "{row}: value {index} is {a}, not {e}");
        }
    }

    #[test]
    fn a_linear_gradient_rises_across_the_extent_of_the_picture_in_its_direction() -> Result<(), Error> {
        let sixth = 1.0 / 6.0;
        // 45 degrees on 4 x 3 (width x height): extent (4 + 3) cos 45, so a pixel at column c, row r (centre c + 0.5,
        // r + 0.5) is at ((c - 1.5) + (r - 1)) / 7 + 0.5, worked by hand
        let oblique: Vec<f32> = (0_u8..3)
            .flat_map(|row| {
                (0_u8..4).map(move |column| ((f32::from(column) - 1.5) + (f32::from(row) - 1.0)) / 7.0 + 0.5)
            })
            .collect();
        // (height, width, degrees, the values row after row)
        let rows: [(usize, usize, f64, Vec<f32>); 8] = [
            (3, 4, 0.0, [0.125, 0.375, 0.625, 0.875].repeat(3)),
            (3, 4, 180.0, [0.875, 0.625, 0.375, 0.125].repeat(3)),
            (
                3,
                4,
                90.0,
                [sixth; 4].into_iter().chain([0.5; 4]).chain([1.0 - sixth; 4]).collect(),
            ),
            (
                3,
                4,
                270.0,
                [1.0 - sixth; 4].into_iter().chain([0.5; 4]).chain([sixth; 4]).collect(),
            ),
            (3, 4, 45.0, oblique.clone()),
            // any whole number of turns away is the same direction
            (3, 4, 405.0, oblique.clone()),
            (3, 4, -315.0, oblique),
            // one row: across it the value is x / width; down it there is nothing to rise through
            (1, 5, 0.0, vec![0.1, 0.3, 0.5, 0.7, 0.9]),
        ];
        for (height, width, degrees, expected) in rows {
            let declared = json!({"g": {"kind": "linear_gradient", "angle": degrees}});
            let values = made(height, width, &declared, "g", &[])?;
            assert_close(&values, &expected, 1e-6, &format!("{height} x {width} at {degrees}"));
        }
        // a single row has the middle value down the picture, wherever it points
        let down = made(
            1,
            5,
            &json!({"g": {"kind": "linear_gradient", "angle": 90.0}}),
            "g",
            &[],
        )?;
        assert_close(&down, &[0.5; 5], 1e-6, "1 x 5 at 90");
        Ok(())
    }

    #[test]
    fn a_radial_gradient_is_the_distance_from_its_centre_over_a_share_of_the_shorter_side_capped_at_1()
    -> Result<(), Error> {
        // 4 x 3 picture, centre in the middle (2, 1.5), radius 0.5 of the shorter side 3: 1.5 px. A pixel at column c,
        // row r is hypot(c - 1.5, r - 1) / 1.5 away, worked by hand
        let hypot = |across: f32, down: f32| across.hypot(down) / 1.5;
        let centre = made(3, 4, &json!({"r": {"kind": "radial_gradient"}}), "r", &[])?;
        let expected = [
            1.0,
            hypot(0.5, 1.0),
            hypot(0.5, 1.0),
            1.0, // the corners are 1.8 px away: capped
            1.0,
            1.0 / 3.0,
            1.0 / 3.0,
            1.0,
            1.0,
            hypot(0.5, 1.0),
            hypot(0.5, 1.0),
            1.0,
        ];
        assert_close(&centre, &expected, 1e-6, "centred");
        // centre a quarter across and at the top edge, radius a whole shorter side: (1, 0) and 3 px; pixel (0, 0) is
        // hypot(0.5, 0.5) / 3 away
        let moved = made(
            3,
            4,
            &json!({"r": {"kind": "radial_gradient", "x": 0.25, "y": 0.0, "radius": 1.0}}),
            "r",
            &[],
        )?;
        assert_close(
            &moved[..2],
            &[0.5_f32.hypot(0.5) / 3.0, 0.5_f32.hypot(0.5) / 3.0],
            1e-6,
            "moved",
        );
        // 1 x 5: the shorter side is the height, 1; radius 3 reaches 3 px from the centre (2.5, 0.5)
        let wide = made(
            1,
            5,
            &json!({"r": {"kind": "radial_gradient", "radius": 3.0}}),
            "r",
            &[],
        )?;
        assert_close(&wide, &[2.0 / 3.0, 1.0 / 3.0, 0.0, 1.0 / 3.0, 2.0 / 3.0], 1e-6, "1 x 5");
        Ok(())
    }

    #[test]
    fn a_band_is_chosen_fully_inside_and_fades_linearly_beyond_its_bounds() -> Result<(), Error> {
        // dyadic numbers, so every value is exact in f32
        let depth = vec![0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0];
        let band =
            |feather: f64| json!({"b": {"kind": "band", "of": "depth", "low": 0.5, "high": 0.75, "feather": feather}});
        let given = [("depth", depth)];
        let soft = made(1, 9, &band(0.25), "b", &given)?;
        assert_eq!(soft, [0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 0.5, 0.0]);
        let hard = made(1, 9, &band(0.0), "b", &given)?;
        assert_eq!(hard, [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0]);
        // a feather of a whole scale fades over all of it
        let wide = made(1, 9, &band(1.0), "b", &given)?;
        assert_close(
            &wide,
            &[0.5, 0.625, 0.75, 0.875, 1.0, 1.0, 1.0, 0.875, 0.75],
            0.0,
            "feather 1",
        );
        // a feather too small for a single-precision number is a hard edge, not a division by nothing
        let tiny = made(1, 9, &band(1e-50), "b", &given)?;
        assert_eq!(tiny, hard);
        Ok(())
    }

    #[test]
    fn a_value_written_on_a_bound_is_on_it_whichever_precision_the_map_is_held_in() -> Result<(), Error> {
        // the maps hold the single-precision numbers nearest to 0.3 and 0.7; a recipe writes the bounds as 0.3 and 0.7.
        // Those are on the bounds although in double precision the first is just above and the second just below them
        let field = vec![0.3_f32, 0.7_f32];
        let on_high = json!({"b": {"kind": "band", "of": "depth", "low": 0.1, "high": 0.3, "feather": 0.0}});
        let on_low = json!({"b": {"kind": "band", "of": "depth", "low": 0.7, "high": 0.9, "feather": 0.0}});
        let given = [("depth", field)];
        assert_eq!(made(1, 2, &on_high, "b", &given)?, [1.0, 0.0]);
        assert_eq!(made(1, 2, &on_low, "b", &given)?, [0.0, 1.0]);
        // and the number just beyond the bound is not
        let above = f32::from_bits(0.3_f32.to_bits() + 1);
        assert_eq!(made(1, 1, &on_high, "b", &[("depth", vec![above])])?, [0.0]);
        Ok(())
    }

    #[test]
    fn selections_combine_by_product_least_or_greatest_of_two_or_three() -> Result<(), Error> {
        let given = [
            ("depth", vec![0.0, 0.25, 0.5, 1.0]),
            ("subject", vec![1.0, 0.5, 0.5, 0.25]),
            ("star", vec![0.5, 0.5, 1.0, 0.5]),
        ];
        let declared =
            |op: &str, of: &[&str]| json!({"star": {"kind": "given"}, "c": {"kind": "combine", "op": op, "of": of}});
        // (op, the selections, the values worked by hand)
        let rows = [
            ("multiply", vec!["depth", "subject"], [0.0, 0.125, 0.25, 0.25]),
            ("min", vec!["depth", "subject"], [0.0, 0.25, 0.5, 0.25]),
            ("max", vec!["depth", "subject"], [1.0, 0.5, 0.5, 1.0]),
            ("multiply", vec!["depth", "subject", "star"], [0.0, 0.0625, 0.25, 0.125]),
            ("min", vec!["depth", "subject", "star"], [0.0, 0.25, 0.5, 0.25]),
            ("max", vec!["depth", "subject", "star"], [1.0, 0.5, 1.0, 1.0]),
        ];
        for (op, of, expected) in rows {
            let values = made(1, 4, &declared(op, &of), "c", &given)?;
            assert_eq!(values, expected, "{op} of {of:?}");
        }
        Ok(())
    }

    #[test]
    fn a_region_made_from_a_region_is_made_from_what_that_one_is() -> Result<(), Error> {
        let declared = json!({
            "near": {"kind": "band", "of": "depth", "low": 0.5, "high": 1.0, "feather": 0.5},
            "nearer": {"kind": "band", "of": "near", "low": 0.5, "high": 1.0, "feather": 0.0},
        });
        let given = [("depth", vec![0.0, 0.25, 0.5, 0.75, 1.0])];
        assert_eq!(made(1, 5, &declared, "near", &given)?, [0.0, 0.5, 1.0, 1.0, 1.0]);
        assert_eq!(made(1, 5, &declared, "nearer", &given)?, [0.0, 1.0, 1.0, 1.0, 1.0]);
        Ok(())
    }

    #[test]
    fn a_subject_region_without_per_is_the_subject_of_the_photo() -> Result<(), Error> {
        let declared = json!({"me": {"kind": "subject", "context": 0.5, "threshold": 0.2}});
        let subject = vec![0.0, 0.25, 0.5, 1.0];
        assert_eq!(made(1, 4, &declared, "me", &[("subject", subject.clone())])?, subject);
        Ok(())
    }

    #[test]
    fn a_long_chain_of_regions_is_made_without_a_deep_call_stack() -> Result<(), Error> {
        // 3000 bands, each of the one before: the last is the depth passed through every one of them
        let mut declared = serde_json::Map::new();
        for link in 0..3000 {
            let of = if link == 0 {
                "depth".to_owned()
            } else {
                format!("b{}", link - 1)
            };
            declared.insert(
                format!("b{link}"),
                json!({"kind": "band", "of": of, "low": 0.0, "high": 1.0, "feather": 0.0}),
            );
        }
        let values = made(1, 2, &Value::Object(declared), "b2999", &[("depth", vec![0.25, 1.0])])?;
        assert_eq!(values, [1.0, 1.0]);
        Ok(())
    }

    #[test]
    fn the_light_is_available_to_a_region_that_is_the_only_one_to_use_it() -> Result<(), Error> {
        let rig = rig()?;
        // grey pixels of brightness 0.1, 0.4, 0.6 and 0.9
        let pixels: Vec<f32> = [0.1, 0.4, 0.6, 0.9].iter().flat_map(|grey| [*grey; 3]).collect();
        let declared = json!({"bright": {"kind": "band", "of": "light", "low": 0.5, "high": 1.0, "feather": 0.0}});
        rig.render(
            &grey(1, 4, &pixels, &[])?,
            &asking(&["bright"], &declared),
            Options::default(),
        )?;
        assert_eq!(rig.seen(), [[0.0, 0.0, 1.0, 1.0]]);
        Ok(())
    }

    #[test]
    fn a_region_is_made_for_the_working_picture_framed_and_sized_and_a_given_map_follows_the_frame() -> Result<(), Error>
    {
        let rig = rig()?;
        let pixels = vec![0.5; 4 * 6 * 3];
        // a map that is 1 in the left column of the 6-wide photo only
        let left: Vec<f32> = (0..24).map(|index| f32::from(u8::from(index % 6 == 0))).collect();
        let photo = grey(4, 6, &pixels, &[("star", left)])?;
        let mut flipped = asking(
            &["g", "star"],
            &json!({"star": {"kind": "given"}, "g": {"kind": "linear_gradient", "angle": 0.0}}),
        );
        flipped.frame = Frame {
            flip: Flip::X,
            ..Frame::default()
        };
        rig.render(&photo, &flipped, Options::default())?;
        let (gradient, star) = (&rig.seen()[0], &rig.seen()[1]);
        // the gradient is laid on the flipped picture, which is as wide as the photo; the given map has moved with it
        // to the right column
        assert_close(
            &gradient[..6],
            &[1.0 / 12.0, 0.25, 5.0 / 12.0, 7.0 / 12.0, 0.75, 11.0 / 12.0],
            1e-6,
            "gradient row",
        );
        assert_eq!((star[0], star[5], star.iter().sum::<f32>()), (0.0, 1.0, 4.0));
        // a square crop makes a picture of 4 x 4: the region has the pixels of that picture, not of the photo
        rig.recorder.clear();
        let mut cropped = asking(&["g"], &json!({"g": {"kind": "linear_gradient", "angle": 0.0}}));
        cropped.frame = Frame {
            aspect: Some(1.0),
            ..Frame::default()
        };
        let plain = grey(4, 6, &pixels, &[])?;
        rig.render(&plain, &cropped, Options::default())?;
        assert_close(&rig.seen()[0][..4], &[0.125, 0.375, 0.625, 0.875], 1e-6, "cropped");
        assert_eq!(rig.seen()[0].len(), 16);
        // and at a size of 12 wide the gradient has 8 x 12 values
        rig.recorder.clear();
        let sized = asking(&["g"], &json!({"g": {"kind": "linear_gradient", "angle": 0.0}}));
        rig.render(&plain, &sized, Options::default().with_size(12))?;
        assert_eq!(rig.seen()[0].len(), 8 * 12);
        assert!((rig.seen()[0][0] - 1.0 / 24.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn the_same_gradient_at_twice_the_size_has_the_means_of_the_small_one() -> Result<(), Error> {
        let declared = json!({"g": {"kind": "linear_gradient", "angle": 30.0}});
        let (rig, pixels) = (rig()?, vec![0.5; 4 * 6 * 3]);
        let photo = grey(4, 6, &pixels, &[])?;
        let at = |size: u32| -> Result<Vec<f32>, Error> {
            rig.recorder.clear();
            rig.render(&photo, &asking(&["g"], &declared), Options::default().with_size(size))?;
            Ok(rig.seen().remove(0))
        };
        let (small, large) = (at(12)?, at(24)?);
        assert_eq!((small.len(), large.len()), (8 * 12, 16 * 24));
        for row in 0..8 {
            for column in 0..12 {
                let block: f32 = [(0, 0), (0, 1), (1, 0), (1, 1)]
                    .iter()
                    .map(|(down, across)| large[(2 * row + down) * 24 + 2 * column + across])
                    .sum::<f32>()
                    / 4.0;
                assert!(
                    (block - small[row * 12 + column]).abs() < 1e-6,
                    "row {row}, column {column}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn a_region_no_step_uses_is_never_made_and_asks_for_no_map() -> Result<(), Error> {
        let rig = rig()?;
        let pixels = vec![0.5; 4 * 6 * 3];
        let (used, unused) = (Arc::new(Finds::flat(0.25)), Arc::new(Finds::flat(0.75)));
        let photo = Photo::new(Picture::new(4, 6, &pixels)?)
            .with_finder("moon", used.clone())
            .with_finder("star", unused.clone());
        let declared = json!({
            "moon": {"kind": "given"}, "star": {"kind": "given"},
            "lit": {"kind": "band", "of": "moon"}, "dim": {"kind": "band", "of": "star"},
        });
        // two steps use `lit` and the map under it: the finder is asked once; `dim` and `star` are never touched
        rig.render(&photo, &asking(&["lit", "lit"], &declared), Options::default())?;
        assert_eq!((used.calls(), unused.calls()), (1, 0));
        assert_eq!(rig.seen(), vec![vec![0.0; 24]; 2]);
        Ok(())
    }

    #[test]
    fn a_map_stays_while_any_later_step_uses_it_directly_or_through_a_region() -> Result<(), Error> {
        let rig = rig()?;
        let photo = grey(1, 1, &[0.5; 3], &[("depth", vec![0.75])])?;
        // step 1 uses `near` (made from the depth), step 3 uses `far` (made from the depth too) and step 2 nothing of it
        let declared = json!({
            "near": {"kind": "band", "of": "depth", "low": 0.5, "high": 1.0, "feather": 0.0},
            "far": {"kind": "band", "of": "depth", "low": 0.0, "high": 0.5, "feather": 0.0},
        });
        let mut recipe = asking(&["near", "far"], &declared);
        recipe.steps.insert(1, Step::new("paint").param("level", 0.5).into());
        rig.render(&photo, &recipe, Options::default())?;
        assert_eq!(rig.seen(), [[1.0], [0.0]]);
        // a map used directly again after other steps
        let mut again = asking(&["depth", "near", "depth"], &declared);
        again.steps.insert(1, Step::new("paint").param("level", 0.5).into());
        rig.recorder.clear();
        rig.render(&photo, &again, Options::default())?;
        assert_eq!(rig.seen(), [[0.75], [1.0], [0.75]]);
        Ok(())
    }

    #[test]
    fn a_kind_the_core_cannot_make_yet_refuses_the_render_before_any_step_runs_when_a_step_uses_it() -> Result<(), Error>
    {
        let rig = rig()?;
        let pixels = vec![0.5; 4 * 6 * 3];
        let photo = grey(4, 6, &pixels, &[])?;
        let torn = json!({"edge": {"style": "torn"}});
        // (what is declared as `p`, whether a step reaches it by an `each` rather than a mask, what the refusal says)
        let rows = [
            ("shards", json!({"p": {"kind": "shards"}}), true, "kind shards"),
            ("cuts", json!({"p": {"kind": "cuts"}}), true, "kind cuts"),
            ("svg", json!({"p": {"kind": "svg"}}), true, "kind svg"),
            ("path", json!({"p": {"kind": "path"}}), false, "kind path"),
            (
                "a torn grid",
                json!({"p": {"kind": "grid", "edge": torn["edge"]}}),
                true,
                "a torn edge",
            ),
        ];
        for (what, declared, by_each, said) in rows {
            let mut recipe = Recipe::new([Step::new("paint").param("level", 0.5)]);
            recipe.steps.push(if by_each {
                Each::new("p", [None]).into()
            } else {
                Step::new("paint").param("level", 0.5).mask("p").into()
            });
            recipe.regions = regions(declared);
            let result = rig.render(&photo, &recipe, Options::default());
            assert!(
                matches!(&result, Err(error) if error.kind() == ErrorKind::Recipe
                    && ["'p'", said, "not run by the core yet", "step 2"].iter().all(|word| error.to_string().contains(word))),
                "{what}: {result:?}"
            );
        }
        // the first step is fine and never ran
        assert_eq!(rig.recorder.received().len(), 0);
        Ok(())
    }

    proptest! {
        #[test]
        fn every_selection_a_region_makes_lies_within_0_and_1_whatever_its_numbers(
            field in prop::collection::vec(0.0_f32..=1.0, 12),
            other in prop::collection::vec(0.0_f32..=1.0, 12),
            (low, high) in (0.0_f64..=1.0, 0.0_f64..=1.0).prop_map(|(a, b)| (a.min(b), a.max(b))),
            feather in prop_oneof![Just(0.0), 0.0_f64..=1.0],
            op in prop::sample::select(vec!["multiply", "min", "max"]),
            angle in -720.0_f64..720.0,
            (x, y, radius) in (0.0_f64..=1.0, 0.0_f64..=1.0, 0.01_f64..=3.0),
        ) {
            let declared = json!({
                "b": {"kind": "band", "of": "depth", "low": low, "high": high, "feather": feather},
                "c": {"kind": "combine", "op": op, "of": ["b", "subject", "depth"]},
                "l": {"kind": "linear_gradient", "angle": angle},
                "r": {"kind": "radial_gradient", "x": x, "y": y, "radius": radius},
            });
            let given = [("depth", field), ("subject", other)];
            for name in ["b", "c", "l", "r"] {
                let values = made(3, 4, &declared, name, &given)?;
                prop_assert_eq!(values.len(), 12);
                prop_assert!(values.iter().all(|value| (0.0..=1.0).contains(value)), "{}: {:?}", name, values);
            }
        }
    }
}
