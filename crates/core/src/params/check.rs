//! Checking a user's values against an effect's declared parameters.

use serde_json::{Map, Value};

use super::{Color, Field, Kind, Scalar, Table};
use crate::{Constraint, Error};

impl Table {
    /// The complete parameters an effect takes for the values `given`, or the first rule they break.
    ///
    /// The result has every parameter, in normal form: a parameter not given has its declared default; a decimal is a
    /// float even when given as a whole number; a colour is three numbers 0..1 (given as `'#rrggbb'`, `'#rgb'` in
    /// either case, spaces around allowed, or three numbers); a lookup table is a non-empty string (a name or a path) or `null`; a field is a non-empty string; a `nullable` parameter also takes `null`; a list of colours is a list of those, between its
    /// `min_length` and `max_length` long (the spellings may be mixed; the first element that is no colour is named by
    /// its place, counting from 0). Rules: a name the effect does not declare is refused,
    /// listing the declared ones; a decimal takes any number but not a switch or text; a whole number takes only a
    /// whole one (`3.0` is not); a switch takes `true` or `false`; a choice takes a listed name; a number lies within
    /// its inclusive `min` and `max`, except a `cyclic` one (an angle whose range is one turn), which takes any real
    /// and wraps it into the range as `min + (value - min) mod (max - min)` when it lies outside. `given` is not changed.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `effect`, the parameter, the rule and the value.
    pub fn check(&self, effect: &str, given: &Map<String, Value>) -> Result<Map<String, Value>, Error> {
        let invalid = |field: &str, constraint, value: &Value| Error::Invalid {
            place: None,
            effect: effect.to_owned(),
            field: field.to_owned(),
            constraint,
            value: Box::new(value.clone()),
        };
        if let Some((name, value)) = given
            .iter()
            .find(|(name, _)| self.fields.iter().all(|field| field.name != name.as_str()))
        {
            let declared = self.fields.iter().map(|field| field.name).collect();
            return Err(invalid(name, Constraint::Unknown(declared), value));
        }
        self.fields
            .iter()
            .map(|field| {
                let value = given.get(field.name).cloned().unwrap_or_else(|| field.default.value());
                field
                    .normal(&value)
                    .map(|normal| (field.name.to_owned(), normal))
                    .map_err(|constraint| invalid(field.name, constraint, &value))
            })
            .collect()
    }
}

impl Scalar {
    /// The number a bound or default holds, if it is one.
    #[expect(clippy::cast_precision_loss, reason = "declared bounds are small whole numbers")]
    pub(super) const fn number(self) -> Option<f64> {
        match self {
            Self::Int(whole) => Some(whole as f64),
            Self::Float(real) => Some(real),
            Self::Bool(_) | Self::Text(_) | Self::List(_) | Self::Null => None,
        }
    }

    /// The value as JSON.
    fn value(self) -> Value {
        match self {
            Self::Int(whole) => Value::from(whole),
            Self::Float(real) => Value::from(real),
            Self::Bool(flag) => Value::from(flag),
            Self::Text(text) => Value::from(text),
            Self::List(items) => Value::from(items.to_vec()),
            Self::Null => Value::Null,
        }
    }
}

impl Field {
    /// `value` in normal form, or the rule it breaks.
    fn normal(&self, value: &Value) -> Result<Value, Constraint> {
        if self.nullable && value.is_null() {
            return Ok(Value::Null);
        }
        match self.kind {
            Kind::Float => value
                .as_f64()
                .ok_or(Constraint::Number)
                .and_then(|real| self.within(self.wrapped(real)))
                .map(Value::from),
            Kind::Int => {
                let whole = value.as_i64().ok_or(Constraint::WholeNumber)?;
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a whole number beyond 2^53 is far outside any range"
                )]
                self.within(whole as f64)?;
                Ok(Value::from(whole))
            }
            Kind::Bool => value.as_bool().map(Value::from).ok_or(Constraint::TrueOrFalse),
            Kind::Choice => value
                .as_str()
                .filter(|name| self.choices.contains(name))
                .map(Value::from)
                .ok_or(Constraint::OneOf(self.choices)),
            Kind::Color => colour(value).map(Value::from).ok_or(Constraint::Colour),
            Kind::Colors => self.colours(value),
            Kind::Field => match value {
                Value::String(name) if !name.is_empty() => Ok(value.clone()),
                _ => Err(Constraint::FieldName),
            },
            Kind::Lut => match value {
                Value::Null => Ok(Value::Null),
                Value::String(name) if !name.is_empty() => Ok(value.clone()),
                _ => Err(Constraint::LutName),
            },
        }
    }

    /// `value` as a list of colours, each in normal form, or the first rule it breaks: not a list, too few or too many
    /// elements, or the first element that is no colour.
    fn colours(&self, value: &Value) -> Result<Value, Constraint> {
        let items = value.as_array().ok_or(Constraint::ColourList)?;
        if let Some(least) = self.min_length
            && items.len() < least
        {
            return Err(Constraint::TooFew(least));
        }
        if let Some(most) = self.max_length
            && items.len() > most
        {
            return Err(Constraint::TooMany(most));
        }
        items
            .iter()
            .enumerate()
            .map(|(at, item)| colour(item).map(Value::from).ok_or(Constraint::ColourAt(at)))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array)
    }

    /// `real` moved into the turn by whole turns when the field is cyclic and it lies outside.
    fn wrapped(&self, real: f64) -> f64 {
        match (
            self.cyclic,
            self.min.and_then(Scalar::number),
            self.max.and_then(Scalar::number),
        ) {
            (true, Some(low), Some(high)) if real < low || real > high => low + (real - low).rem_euclid(high - low),
            _ => real,
        }
    }

    /// `real` if it lies within the inclusive bounds, else the bound it breaks.
    fn within(&self, real: f64) -> Result<f64, Constraint> {
        if let Some((bound, low)) = self.min.and_then(|bound| Some((bound, bound.number()?)))
            && real < low
        {
            return Err(Constraint::AtLeast(bound));
        }
        if let Some((bound, high)) = self.max.and_then(|bound| Some((bound, bound.number()?)))
            && real > high
        {
            return Err(Constraint::AtMost(bound));
        }
        Ok(real)
    }
}

/// The three channels (0..1) a colour value writes: `'#rrggbb'`, `'#rgb'` or a list of three numbers.
fn colour(value: &Value) -> Option<[f64; 3]> {
    match value {
        Value::String(text) => hex(text.trim()),
        Value::Array(items) => {
            let [red, green, blue] = items.as_slice() else {
                return None;
            };
            let channels = [red.as_f64()?, green.as_f64()?, blue.as_f64()?];
            channels
                .iter()
                .all(|channel| (0.0..=1.0).contains(channel))
                .then_some(channels)
        }
        _ => None,
    }
}

impl Color {
    /// The colour `value` writes: `'#rrggbb'` or `'#rgb'` (either case, spaces around allowed) or three numbers from 0
    /// to 1; none if it writes anything else. The one reading of a colour a person writes, shared by a parameter
    /// ([`Table::check`]) and a recipe's frame.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a channel is from 0 to 1 and a colour holds f32"
    )]
    pub(crate) fn parse(value: &Value) -> Option<Self> {
        colour(value).map(|channels| Self(channels.map(|channel| channel as f32)))
    }
}

/// The channels of `'#rrggbb'` or `'#rgb'` (either case): each byte over 255.
fn hex(text: &str) -> Option<[f64; 3]> {
    let digits: Vec<u32> = text
        .strip_prefix('#')?
        .chars()
        .map(|digit| digit.to_digit(16))
        .collect::<Option<_>>()?;
    let bytes = match *digits.as_slice() {
        // `#rgb` doubles each digit: `#0af` is `#00aaff`
        [red, green, blue] => [red * 17, green * 17, blue * 17],
        [r1, r2, g1, g2, b1, b2] => [r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2],
        _ => return None,
    };
    // constant: 8-bit levels
    Some(bytes.map(|byte| f64::from(byte) / 255.0))
}
