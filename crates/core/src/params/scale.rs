//! Design pixels to working pixels: the same look at any size.

use serde_json::{Map, Value};

use super::{Field, Kind, Scalar, Table, Unit};

impl Table {
    /// `params` (complete and checked, as [`Table::check`] returns them) for a picture of `ratio` working pixels per
    /// design pixel (the working size over 480): the same look at another size.
    ///
    /// Every pixel value (a field with the unit `px`) is multiplied by `ratio`; every `density` (marks per 1000 design
    /// px squared) is divided by `ratio` squared, as an area grows with the square of a length. A whole number stays
    /// whole: it is rounded, halves to the even number (as Python's `round` does). When shrinking (a ratio below 1) a
    /// pixel value stops at the field's `min`, which keeps a small picture valid (a cell of at least 3 pixels);
    /// growing has no limit at either end, since a range is in design pixels and the result is not checked against
    /// it. Every other unit is unchanged, and a ratio of 1 changes nothing.
    ///
    /// This is the one place that knows how a picture's size changes a value; the runner renders with the result and
    /// asks an effect's [`footprint`](crate::Effect::footprint) about it.
    #[must_use]
    pub(crate) fn scaled(&self, params: Map<String, Value>, ratio: f64) -> Map<String, Value> {
        params
            .into_iter()
            .map(
                |(name, value)| match self.fields.iter().find(|field| field.name == name) {
                    Some(field) => (name, field.scaled(value, ratio)),
                    None => (name, value),
                },
            )
            .collect()
    }
}

impl Field {
    /// `value` of this field for a picture of `ratio` working pixels per design pixel.
    fn scaled(&self, value: Value, ratio: f64) -> Value {
        let Some(number) = value.as_f64() else {
            return value;
        };
        let scaled = match self.unit {
            Some(Unit::Px) => number * ratio,
            Some(Unit::Density) => number / (ratio * ratio),
            _ => return value,
        };
        let scaled = match self.kind {
            Kind::Int => scaled.round_ties_even(),
            _ => scaled,
        };
        let scaled = match (self.unit, self.min.and_then(Scalar::number)) {
            (Some(Unit::Px), Some(min)) if ratio < 1.0 => scaled.max(min),
            _ => scaled,
        };
        match self.kind {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the value is whole; a ratio so large that it leaves the i64 range is no picture"
            )]
            Kind::Int => Value::from(scaled as i64),
            _ => Value::from(scaled),
        }
    }
}
