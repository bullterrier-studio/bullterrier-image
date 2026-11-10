//! What a step hands an effect besides the picture and its parameters: the maps it works from ([`Layer`], [`Labels`]) and
//! the numbers about the render ([`Inputs`]).

use std::collections::BTreeMap;
use std::fmt;

use crate::effect::{Effect, Input};
use crate::{Error, Image, Lut, Picture};

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

    /// The map of `values` that the caller has kept finite and within 0..=1, and `height` x `width` of: not checked again.
    pub(crate) const fn from_checked(height: usize, width: usize, values: Vec<f32>) -> Self {
        Self { height, width, values }
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

    /// The map of rows `from..to` of this one (`from < to <= height`), which a band of a picture is given.
    pub(crate) fn rows(&self, from: usize, to: usize) -> Self {
        Self {
            height: to - from,
            width: self.width,
            values: self.values[from * self.width..to * self.width].to_vec(),
        }
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

    /// The labelling of rows `from..to` of this one (`from < to <= height`), among the same pieces.
    pub(crate) fn rows(&self, from: usize, to: usize) -> Self {
        Self {
            height: to - from,
            width: self.width,
            values: self.values[from * self.width..to * self.width].to_vec(),
            pieces: self.pieces,
        }
    }
}

/// Where the picture an effect is handed lies on the canvas: the picture a render works on, which an effect may be
/// handed only a band of.
///
/// An effect whose answer at a pixel depends on where the pixel is (a vignette's distance from the middle, a noise hashed
/// by position) reads the canvas coordinates from here, never from the picture's own: row `r` of the picture is canvas row
/// `first_row + r`, and the canvas is `canvas_height` x `canvas_width`. An effect that declares [`Input::Place`] and is
/// offered none sees the picture itself as the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Place {
    /// The number of rows of the canvas.
    pub canvas_height: usize,
    /// The number of columns of the canvas, which is also the picture's.
    pub canvas_width: usize,
    /// The canvas row of the picture's row 0.
    pub first_row: usize,
}

impl Place {
    /// The place of a picture whose row 0 is row `first_row` of a canvas of `canvas_height` x `canvas_width`.
    #[must_use]
    pub fn new(canvas_height: usize, canvas_width: usize, first_row: usize) -> Self {
        Self {
            canvas_height,
            canvas_width,
            first_row,
        }
    }
}

/// What a step offers an effect besides the picture and its parameters, and what an effect receives of it: only the
/// inputs it declares (see [`Effect::inputs`] and [`Effect::fields`]); the rest is absent.
///
/// Build the offer with the `with_` methods; the core hands the effect the part of it that the effect declares. A lookup
/// table (a parameter of the kind `lut`) is offered under its parameter's name, like a field's map; the parameter itself
/// only names it (see [`LutName`](crate::params::LutName)). So is a picture (a parameter of the kind `picture`), an
/// [`Image`] with straight alpha (see [`PictureName`](crate::params::PictureName)).
#[derive(Clone, Debug)]
pub struct Inputs<'a> {
    size_ratio: f64,
    place: Option<Place>,
    partition: Option<&'a Labels>,
    fields: BTreeMap<String, &'a Layer>,
    luts: BTreeMap<String, &'a Lut>,
    pictures: BTreeMap<String, &'a Image>,
}

impl Default for Inputs<'_> {
    /// No partition, no fields and a size ratio of 1.
    fn default() -> Self {
        Self {
            size_ratio: 1.0,
            place: None,
            partition: None,
            fields: BTreeMap::new(),
            luts: BTreeMap::new(),
            pictures: BTreeMap::new(),
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

    /// The offer with `place`, where the picture lies on its canvas. Without it the picture is its own canvas. A run is
    /// refused if the picture does not lie within the canvas (see [`Place`]).
    #[must_use]
    pub fn with_place(mut self, place: Place) -> Self {
        self.place = Some(place);
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

    /// The offer with `lut` for the lookup table parameter called `parameter`.
    #[must_use]
    pub fn with_lut(mut self, parameter: impl Into<String>, lut: &'a Lut) -> Self {
        self.luts.insert(parameter.into(), lut);
        self
    }

    /// The offer with `image` for the picture parameter called `parameter`.
    #[must_use]
    pub fn with_image(mut self, parameter: impl Into<String>, image: &'a Image) -> Self {
        self.pictures.insert(parameter.into(), image);
        self
    }

    /// Working pixels per design pixel: 1 unless the effect declared [`Input::SizeRatio`].
    #[must_use]
    pub fn size_ratio(&self) -> f64 {
        self.size_ratio
    }

    /// Where `picture`, the one the effect is run on, lies on its canvas: as offered if the effect declared
    /// [`Input::Place`], else the picture itself is the canvas.
    #[must_use]
    pub fn place(&self, picture: &Picture<'_>) -> Place {
        self.place
            .unwrap_or_else(|| Place::new(picture.height(), picture.width(), 0))
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

    /// The table for the lookup table parameter called `parameter`, if one was offered: none means the identity table.
    #[must_use]
    pub fn lut(&self, parameter: &str) -> Option<&'a Lut> {
        self.luts.get(parameter).copied()
    }

    /// The image for the picture parameter called `parameter`, if one was offered: none means no picture. Its alpha, if
    /// it has any, is the cover of whatever the effect lays over the photo.
    #[must_use]
    pub fn image(&self, parameter: &str) -> Option<&'a Image> {
        self.pictures.get(parameter).copied()
    }

    /// The part of this offer `effect` declares, for a picture of `picture`'s size.
    ///
    /// # Errors
    /// [`Error::Inputs`] if the effect declares an input the offer lacks, or a partition or map is not of the
    /// picture's size.
    pub(crate) fn declared_by(&self, effect: &dyn Effect, picture: &Picture<'_>) -> Result<Self, Error> {
        let size = (picture.height(), picture.width());
        let mut declared = Self::default();
        if let Some(place) = self.place {
            // checked whether or not the effect declares it: a band caller cannot offer a place that does not hold the band
            let last = place.first_row.saturating_add(picture.height());
            if place.canvas_width != picture.width() {
                return Err(Error::Inputs(format!(
                    "the effect {:?} was given a place whose canvas is {} wide, the picture {}",
                    effect.name(),
                    place.canvas_width,
                    picture.width()
                )));
            }
            if last > place.canvas_height {
                return Err(Error::Inputs(format!(
                    "the effect {:?} was given a place of rows {}..{} for a canvas of {} rows",
                    effect.name(),
                    place.first_row,
                    last,
                    place.canvas_height
                )));
            }
        }
        for input in effect.inputs() {
            match input {
                Input::SizeRatio => declared.size_ratio = self.size_ratio,
                Input::Place => declared.place = self.place,
                Input::Partition => {
                    let partition = self
                        .partition
                        .ok_or_else(|| missing(effect, "partition", "set the step's region"))?;
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
            let layer = self.fields.get(&field.parameter).copied().ok_or_else(|| {
                missing(
                    effect,
                    &field.parameter,
                    &format!("name a map or region in the step's `{}`", field.parameter),
                )
            })?;
            if (layer.height, layer.width) != size {
                return Err(wrong_size(effect, &field.parameter, (layer.height, layer.width), size));
            }
            declared.fields.insert(field.parameter.clone(), layer);
        }
        // a step offers a table for each lookup table parameter of its effect and no other
        declared.luts.clone_from(&self.luts);
        declared.pictures.clone_from(&self.pictures);
        Ok(declared)
    }
}

/// The refusal for an input the effect declares and the offer lacks; `hint` says how to give one, by the input's kind.
fn missing(effect: &dyn Effect, input: &str, hint: &str) -> Error {
    Error::Inputs(format!(
        "the effect {:?} needs `{input}`, which was not given: {hint}",
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
