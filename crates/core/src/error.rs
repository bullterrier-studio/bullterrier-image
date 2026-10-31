//! What can go wrong when the core runs an effect.

use std::fmt;

use serde_json::Value;

use crate::params::Scalar;

/// Why the core refused to check or run something.
///
/// [`Error::Invalid`] and [`Error::Recipe`] are a user's mistakes (a value out of range, a recipe that breaks a rule);
/// the Python package raises them as `RecipeError` and [`Error::UnknownEffect`] as `UnknownEffectError`. The others
/// are the caller's programming errors, or, for [`Error::Foreign`], whatever an effect that runs outside the core
/// raised: running an effect trusts the parameters it is given (see [`crate::params::Table::check`] for checking them).
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The core implements no effect of this name.
    UnknownEffect(String),
    /// An effect of this name is registered already, whether the core's own or another's.
    Duplicate(String),
    /// An effect that runs outside the core failed, or returned something that is not a picture of the right form.
    Foreign {
        /// The effect that failed.
        effect: String,
        /// What went wrong, in the words of whoever runs it.
        message: String,
    },
    /// A parameter value breaks a rule of the effect's declaration.
    Invalid {
        /// The effect whose parameters these were.
        effect: String,
        /// The parameter.
        field: String,
        /// The rule it breaks.
        constraint: Constraint,
        /// The value that was given.
        value: Value,
    },
    /// A recipe breaks a rule; the text says which, naming the step and the key.
    Recipe(String),
    /// The parameters are not the effect's: one is missing, unknown, or of the wrong type.
    Params {
        /// The effect whose parameters these were meant to be.
        effect: &'static str,
        /// What serde found wrong with them.
        source: serde_json::Error,
    },
    /// Channel values that do not make a picture of the given size (three per pixel).
    Size {
        /// The number of rows given.
        height: usize,
        /// The number of columns given.
        width: usize,
        /// The number of values given.
        values: usize,
    },
}

/// The rule a parameter value breaks.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Constraint {
    /// The effect has no such parameter; these are the ones it has, in declaration order.
    Unknown(Vec<&'static str>),
    /// A decimal number is needed.
    Number,
    /// A whole number is needed (`3.0` is not one).
    WholeNumber,
    /// `true` or `false` is needed.
    TrueOrFalse,
    /// The value is below the lowest the parameter takes (inclusive).
    AtLeast(Scalar),
    /// The value is above the highest the parameter takes (inclusive).
    AtMost(Scalar),
    /// The value is none of these names.
    OneOf(&'static [&'static str]),
    /// A colour is `'#rrggbb'`, `'#rgb'` or three numbers from 0 to 1.
    Colour,
}

impl fmt::Display for Constraint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(valid) => write!(
                formatter,
                "is not a parameter; the parameters are: {}",
                valid.join(", ")
            ),
            Self::Number => formatter.write_str("must be a number"),
            Self::WholeNumber => formatter.write_str("must be a whole number"),
            Self::TrueOrFalse => formatter.write_str("must be true or false"),
            Self::AtLeast(low) => write!(formatter, "must be >= {low}"),
            Self::AtMost(high) => write!(formatter, "must be <= {high}"),
            Self::OneOf(names) => write!(formatter, "must be one of {}", names.join(", ")),
            Self::Colour => formatter.write_str("must be '#rrggbb', '#rgb' or three numbers from 0 to 1"),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEffect(name) => write!(formatter, "the Rust core has no effect called {name:?}"),
            Self::Duplicate(name) => write!(formatter, "an effect called {name:?} is registered already"),
            Self::Invalid {
                effect,
                field,
                constraint,
                value,
            } => match constraint {
                Constraint::Unknown(_) => write!(formatter, "effect '{effect}': `{field}` {constraint}"),
                _ => write!(formatter, "effect '{effect}': `{field}` {constraint}, got {value}"),
            },
            Self::Recipe(message) => formatter.write_str(message),
            Self::Foreign { effect, message } => write!(formatter, "the effect {effect:?} failed: {message}"),
            Self::Params { effect, source } => write!(formatter, "the parameters of {effect:?} do not fit: {source}"),
            Self::Size { height, width, values } => {
                write!(
                    formatter,
                    "{values} values do not make {height} x {width} pixels of 3 channels"
                )
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Params { source, .. } => Some(source),
            Self::UnknownEffect(_)
            | Self::Duplicate(_)
            | Self::Invalid { .. }
            | Self::Recipe(_)
            | Self::Foreign { .. }
            | Self::Size { .. } => None,
        }
    }
}
