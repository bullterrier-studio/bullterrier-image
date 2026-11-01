//! What a step hands an effect besides the picture and its parameters: the maps it works from ([`Layer`], [`Labels`]) and
//! the numbers about the render ([`Inputs`]).

use std::collections::BTreeMap;
use std::fmt;

use crate::effect::{Effect, Input};
use crate::{Error, Picture};

/// A map over the picture: one value from 0 to 1 for every pixel (a subject's alpha, a depth, a light), row after row.
#[derive(Clone, PartialEq)]
pub struct Layer {
    height: usize,
    width: usize,
    values: Vec<f32>,
}

/// Refuses a map of no row or no column: a map has the size of a picture, which has a pixel.
fn no_empty_map(height: usize, width: usize) -> Result<(), Error> {
    if height == 0 || width == 0 {
        return Err(Error::Inputs(format!(
            "a map has at least one row and one column, got {height} x {width}"
        )));
    }
    Ok(())
}

impl fmt::Debug for Layer {
    /// The shape only: a map can hold millions of values.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Layer")
            .field("height", &self.height)
            .field("width", &self.width)
            .finish_non_exhaustive()
    }
}

impl Layer {
    /// The map of `height` x `width` pixels whose values are `values`, row after row.
    ///
    /// # Errors
    /// [`Error::Inputs`] unless `height` and `width` are at least 1, there are exactly `height * width` values and each
    /// is finite and within 0..=1.
    pub fn new(height: usize, width: usize, values: Vec<f32>) -> Result<Self, Error> {
        no_empty_map(height, width)?;
        if height.checked_mul(width) != Some(values.len()) {
            return Err(Error::Inputs(format!(
                "{} values do not make a map of {height} x {width} pixels",
                values.len()
            )));
        }
        if let Some((index, value)) = values
            .iter()
            .copied()
            .enumerate()
            .find(|(_, value)| !(0.0..=1.0).contains(value))
        {
            return Err(Error::Inputs(format!(
                "a map holds values from 0 to 1, but row {}, column {} is {value}",
                index / width,
                index % width
            )));
        }
        Ok(Self { height, width, values })
    }

    /// The number of rows.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// The number of columns.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// The values, row after row.
    #[must_use]
    pub fn values(&self) -> &[f32] {
        &self.values
    }
}

/// The pieces a region cuts the picture into: the number of the piece every pixel belongs to, row after row, pieces
/// numbered from 0.
#[derive(Clone, PartialEq, Eq)]
pub struct Labels {
    height: usize,
    width: usize,
    values: Vec<i32>,
    pieces: usize,
}

impl fmt::Debug for Labels {
    /// The shape and the number of pieces only: a labelling can hold millions of values.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Labels")
            .field("height", &self.height)
            .field("width", &self.width)
            .field("pieces", &self.pieces)
            .finish_non_exhaustive()
    }
}

impl Labels {
    /// The labelling of `height` x `width` pixels whose pixel labels are `labels`, row after row, among `pieces` pieces
    /// (some of them may hold no pixel).
    ///
    /// # Errors
    /// [`Error::Inputs`] unless `height` and `width` are at least 1, there are exactly `height * width` labels and each
    /// is from 0 to `pieces` - 1.
    pub fn new(height: usize, width: usize, labels: Vec<i32>, pieces: usize) -> Result<Self, Error> {
        no_empty_map(height, width)?;
        if height.checked_mul(width) != Some(labels.len()) {
            return Err(Error::Inputs(format!(
                "{} labels do not make a partition of {height} x {width} pixels",
                labels.len()
            )));
        }
        if let Some((index, label)) = labels
            .iter()
            .copied()
            .enumerate()
            .find(|&(_, label)| !usize::try_from(label).is_ok_and(|label| label < pieces))
        {
            return Err(Error::Inputs(format!(
                "a partition of {pieces} pieces has labels from 0 to {}, but row {}, column {} is {label}",
                pieces.saturating_sub(1),
                index / width,
                index % width
            )));
        }
        Ok(Self {
            height,
            width,
            values: labels,
            pieces,
        })
    }

    /// The number of rows.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// The number of columns.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// The number of pieces, counting any that holds no pixel.
    #[must_use]
    pub fn pieces(&self) -> usize {
        self.pieces
    }

    /// The label of every pixel, row after row.
    #[must_use]
    pub fn values(&self) -> &[i32] {
        &self.values
    }
}

/// What a step offers an effect besides the picture and its parameters, and what an effect receives of it: only the
/// inputs it declares (see [`Effect::inputs`] and [`Effect::fields`]); the rest is absent.
///
/// Build the offer with the `with_` methods; the core hands the effect the part of it that the effect declares.
#[derive(Clone, Debug)]
pub struct Inputs<'a> {
    size_ratio: f64,
    partition: Option<&'a Labels>,
    fields: BTreeMap<String, &'a Layer>,
}

impl Default for Inputs<'_> {
    /// No partition, no fields and a size ratio of 1.
    fn default() -> Self {
        Self {
            size_ratio: 1.0,
            partition: None,
            fields: BTreeMap::new(),
        }
    }
}

impl<'a> Inputs<'a> {
    /// The offer with `ratio` working pixels per design pixel: a finite number above 0. An effect that does not declare
    /// [`Input::SizeRatio`] sees 1.
    #[must_use]
    pub fn with_size_ratio(mut self, ratio: f64) -> Self {
        self.size_ratio = ratio;
        self
    }

    /// The offer with `partition`, the pieces of the region the step names.
    #[must_use]
    pub fn with_partition(mut self, partition: &'a Labels) -> Self {
        self.partition = Some(partition);
        self
    }

    /// The offer with `layer` for the field parameter called `parameter`.
    #[must_use]
    pub fn with_field(mut self, parameter: impl Into<String>, layer: &'a Layer) -> Self {
        self.fields.insert(parameter.into(), layer);
        self
    }

    /// Working pixels per design pixel: 1 unless the effect declared [`Input::SizeRatio`].
    #[must_use]
    pub fn size_ratio(&self) -> f64 {
        self.size_ratio
    }

    /// The pieces of the step's region, if the effect declared [`Input::Partition`].
    #[must_use]
    pub fn partition(&self) -> Option<&'a Labels> {
        self.partition
    }

    /// The map for the field parameter called `parameter`, if the effect declared it (see [`Effect::fields`]).
    #[must_use]
    pub fn field(&self, parameter: &str) -> Option<&'a Layer> {
        self.fields.get(parameter).copied()
    }

    /// The field parameters this offer holds a map for, sorted: for a test to see everything an effect received.
    #[cfg(test)]
    pub(crate) fn field_parameters(&self) -> impl Iterator<Item = &str> {
        self.fields.keys().map(String::as_str)
    }

    /// The part of this offer `effect` declares, for a picture of `picture`'s size.
    ///
    /// # Errors
    /// [`Error::Inputs`] if the effect declares an input the offer lacks, or a partition or map is not of the
    /// picture's size.
    pub(crate) fn declared_by(&self, effect: &dyn Effect, picture: &Picture<'_>) -> Result<Self, Error> {
        let size = (picture.height(), picture.width());
        let mut declared = Self::default();
        for input in effect.inputs() {
            match input {
                Input::SizeRatio => declared.size_ratio = self.size_ratio,
                Input::Partition => {
                    let partition = self.partition.ok_or_else(|| missing(effect, "partition"))?;
                    if (partition.height, partition.width) != size {
                        return Err(wrong_size(
                            effect,
                            "partition",
                            (partition.height, partition.width),
                            size,
                        ));
                    }
                    declared.partition = Some(partition);
                }
            }
        }
        for field in effect.fields() {
            let layer = self
                .fields
                .get(&field.parameter)
                .copied()
                .ok_or_else(|| missing(effect, &field.parameter))?;
            if (layer.height, layer.width) != size {
                return Err(wrong_size(effect, &field.parameter, (layer.height, layer.width), size));
            }
            declared.fields.insert(field.parameter.clone(), layer);
        }
        Ok(declared)
    }
}

fn missing(effect: &dyn Effect, input: &str) -> Error {
    Error::Inputs(format!(
        "the effect {:?} needs `{input}`, which was not given",
        effect.name()
    ))
}

fn wrong_size(effect: &dyn Effect, input: &str, found: (usize, usize), picture: (usize, usize)) -> Error {
    Error::Inputs(format!(
        "the effect {:?} was given `{input}` of {} x {} pixels for a picture of {} x {}",
        effect.name(),
        found.0,
        found.1,
        picture.0,
        picture.1
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layer_keeps_its_values_and_size() -> Result<(), Error> {
        let layer = Layer::new(2, 3, vec![0.0, 0.2, 0.4, 0.6, 0.8, 1.0])?;
        assert_eq!((layer.height(), layer.width()), (2, 3));
        assert_eq!(layer.values(), [0.0, 0.2, 0.4, 0.6, 0.8, 1.0]);
        Ok(())
    }

    #[test]
    fn a_layer_of_values_that_are_not_a_map_is_refused_naming_the_first_bad_pixel() {
        // (height, width, values, what the message must say)
        for (height, width, values, said) in [
            (1, 2, vec![0.5; 3], "3 values"),
            (2, 2, vec![0.5; 3], "3 values"),
            (1, 3, vec![0.5, 1.5, 0.5], "row 0, column 1 is 1.5"),
            (2, 2, vec![0.5, 0.5, 0.5, -0.25], "row 1, column 1 is -0.25"),
            (1, 2, vec![f32::NAN, 0.5], "row 0, column 0 is NaN"),
            (1, 2, vec![0.5, f32::INFINITY], "row 0, column 1 is inf"),
        ] {
            let result = Layer::new(height, width, values);
            assert!(
                matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains(said)),
                "{said}: {result:?}"
            );
        }
        // the closed ends are in
        assert!(Layer::new(1, 2, vec![0.0, 1.0]).is_ok());
    }

    #[test]
    fn a_map_or_partition_with_no_row_or_no_column_is_refused_and_one_pixel_is_enough() {
        for (height, width) in [(0, 5), (5, 0), (0, 0)] {
            for result in [
                Layer::new(height, width, Vec::new()).map(|_| ()),
                Labels::new(height, width, Vec::new(), 1).map(|_| ()),
            ] {
                assert!(
                    matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains("at least one")),
                    "{height} x {width}: {result:?}"
                );
            }
        }
        assert!(Layer::new(1, 1, vec![0.5]).is_ok());
        assert!(Labels::new(1, 1, vec![0], 1).is_ok());
    }

    #[test]
    fn labels_keep_their_pieces_and_size() -> Result<(), Error> {
        // three pieces, the last of which holds no pixel
        let labels = Labels::new(2, 2, vec![0, 1, 1, 0], 3)?;
        assert_eq!((labels.height(), labels.width(), labels.pieces()), (2, 2, 3));
        assert_eq!(labels.values(), [0, 1, 1, 0]);
        Ok(())
    }

    #[test]
    fn labels_that_are_not_a_partition_are_refused_naming_the_first_bad_pixel() {
        for (labels, pieces, said) in [
            (vec![0, 1, 1], 2, "3 labels"),
            (vec![0, 2], 2, "row 0, column 1 is 2"),
            (vec![0, -1], 2, "row 0, column 1 is -1"),
            (vec![0, 0], 0, "row 0, column 0 is 0"),
        ] {
            let result = Labels::new(1, 2, labels, pieces);
            assert!(
                matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains(said)),
                "{said}: {result:?}"
            );
        }
        assert!(Labels::new(1, 2, vec![0, 1], 2).is_ok());
    }
}
