//! What can go wrong when the core runs an effect.

use std::fmt;

use serde_json::Value;

use crate::params::Scalar;

/// Why the core refused to check or run something.
///
/// [`Error::kind`] says which of the package's exceptions an error is: the one place that decides, so every binding
/// (the Python package raises these as the `BullterrierImageError` subclasses) agrees.
///
/// | Error | [`ErrorKind`] | Python exception |
/// |---|---|---|
/// | [`Error::Invalid`], [`Error::Recipe`] | `Recipe` | `RecipeError` |
/// | [`Error::UnknownEffect`] | `UnknownEffect` | `UnknownEffectError` |
/// | [`Error::MapNotFound`] of the subject | `SubjectNotFound` | `SubjectNotFoundError` |
/// | [`Error::MapNotFound`] of the depth | `DepthNotFound` | `DepthNotFoundError` |
/// | [`Error::MapNotFound`] of any other map | `MapNotFound` | `MapNotFoundError` |
/// | [`Error::InvalidImage`], [`Error::Size`] | `InvalidImage` | `InvalidImageError` |
/// | [`Error::NonFinite`] | `NonFiniteImage` | `NonFiniteImageError` |
/// | [`Error::ImageRead`] | `ImageRead` | `ImageReadError` |
/// | [`Error::Duplicate`], [`Error::Params`], [`Error::Inputs`], [`Error::Foreign`] | `Other` | none: a `ValueError` |
///
/// The ones in the last row are the caller's programming errors, or, for [`Error::Foreign`], whatever an effect that
/// runs outside the core raised: running an effect trusts the parameters it is given (see
/// [`crate::params::Table::check`] for checking them).
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
        /// What went wrong, as whoever runs the effect reports it: the error itself, so a binding can hand back the
        /// very exception its own language raised (see [`Error::foreign`]).
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// A map of the photo (the subject, the depth, a region's) is needed but not given, not found, or not usable: of
    /// another size than the photo, or holding values outside 0..=1.
    MapNotFound {
        /// The map's name: `subject`, `depth`, or the name of a region.
        map: String,
        /// Which map it is, as far as the package's exceptions tell them apart.
        kind: MapKind,
        /// What is wrong with it, naming the map and the fix.
        message: String,
    },
    /// A picture handed in is not one: not (h, w, 3), a type that is no colour level, or values outside 0..=1.
    InvalidImage(String),
    /// A picture handed in holds NaN or an infinity, which is no colour; the text names the first such pixel.
    NonFinite(String),
    /// A file could not be read as an image: missing, a folder, not an image, damaged, of a layout the core does not
    /// read, or larger than the core decodes. The text names the path and the reason.
    ImageRead(String),
    /// A parameter value breaks a rule of the effect's declaration.
    Invalid {
        /// Where in a recipe the effect was: `step 2`, `step 2, look 1`. None when the parameters were checked on
        /// their own (`Table::check`), not as part of a recipe (`Recipe::checked` names the place).
        place: Option<String>,
        /// The effect whose parameters these were.
        effect: String,
        /// The parameter.
        field: String,
        /// The rule it breaks.
        constraint: Constraint,
        /// The value that was given (boxed to keep `Error` small).
        value: Box<Value>,
    },
    /// A recipe breaks a rule; the text says which, naming the step (`step 2`, as [`Error::Invalid`] does in its
    /// `place`) and the key.
    Recipe(String),
    /// The parameters are not the effect's: one is missing, unknown, or of the wrong type.
    Params {
        /// The effect whose parameters these were meant to be.
        effect: &'static str,
        /// What serde found wrong with them.
        source: serde_json::Error,
    },
    /// An effect was given inputs that do not fit: one it declares is missing, or a partition or map is not of the
    /// picture's size, or a map or partition has values it must not.
    Inputs(String),
    /// Channel values that do not make a picture of the given size (three per pixel), or a size with no row or no column.
    Size {
        /// The number of rows given.
        height: usize,
        /// The number of columns given.
        width: usize,
        /// The number of values given.
        values: usize,
    },
}

/// Which of the photo's maps an [`Error::MapNotFound`] is about, as far as the package's exceptions tell them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MapKind {
    /// The subject: the alpha of what stands out.
    Subject,
    /// The depth, 1 = nearest.
    Depth,
    /// Any other map: a region a recipe declares.
    Other,
}

/// Which of the package's exceptions an [`Error`] is (see [`Error::kind`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// A recipe, a step or an effect's parameters are invalid.
    Recipe,
    /// No effect is registered under the name.
    UnknownEffect,
    /// The subject was needed but could not be found or used.
    SubjectNotFound,
    /// The depth was needed but could not be found or used.
    DepthNotFound,
    /// Another map was needed but could not be found or used.
    MapNotFound,
    /// A picture handed in is not one.
    InvalidImage,
    /// A picture handed in holds NaN or an infinity.
    NonFiniteImage,
    /// A file could not be read as an image.
    ImageRead,
    /// Anything else: a programming error of the caller, or a failure outside the core. Bindings raise their plain
    /// error (a `ValueError` in Python).
    Other,
}

impl ErrorKind {
    /// Every kind, in the order of [`Error`]'s table.
    pub const ALL: &'static [Self] = &[
        Self::Recipe,
        Self::UnknownEffect,
        Self::SubjectNotFound,
        Self::DepthNotFound,
        Self::MapNotFound,
        Self::InvalidImage,
        Self::NonFiniteImage,
        Self::ImageRead,
        Self::Other,
    ];

    /// The kind's name in `snake_case`: `"subject_not_found"`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Recipe => "recipe",
            Self::UnknownEffect => "unknown_effect",
            Self::SubjectNotFound => "subject_not_found",
            Self::DepthNotFound => "depth_not_found",
            Self::MapNotFound => "map_not_found",
            Self::InvalidImage => "invalid_image",
            Self::NonFiniteImage => "non_finite_image",
            Self::ImageRead => "image_read",
            Self::Other => "other",
        }
    }
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

impl Error {
    /// A failure of the effect `effect` that runs outside the core, caused by `source`: an error a binding keeps whole
    /// (the Python bindings wrap the `PyErr` the effect raised, and give that very exception back), or plain text.
    #[must_use]
    pub fn foreign(effect: impl Into<String>, source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::Foreign {
            effect: effect.into(),
            source: source.into(),
        }
    }

    /// A problem with the map called `map`: [`MapKind::Subject`] for `subject`, [`MapKind::Depth`] for `depth`, else
    /// [`MapKind::Other`].
    #[must_use]
    pub fn map_not_found(map: impl Into<String>, message: impl Into<String>) -> Self {
        let map = map.into();
        let kind = match map.as_str() {
            "subject" => MapKind::Subject,
            "depth" => MapKind::Depth,
            _ => MapKind::Other,
        };
        Self::MapNotFound {
            map,
            kind,
            message: message.into(),
        }
    }

    /// Which of the package's exceptions this error is: the one mapping every binding follows (the table is above).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Invalid { .. } | Self::Recipe(_) => ErrorKind::Recipe,
            Self::UnknownEffect(_) => ErrorKind::UnknownEffect,
            Self::MapNotFound { kind, .. } => match kind {
                MapKind::Subject => ErrorKind::SubjectNotFound,
                MapKind::Depth => ErrorKind::DepthNotFound,
                MapKind::Other => ErrorKind::MapNotFound,
            },
            Self::InvalidImage(_) | Self::Size { .. } => ErrorKind::InvalidImage,
            Self::NonFinite(_) => ErrorKind::NonFiniteImage,
            Self::ImageRead(_) => ErrorKind::ImageRead,
            Self::Duplicate(_) | Self::Params { .. } | Self::Inputs(_) | Self::Foreign { .. } => ErrorKind::Other,
        }
    }

    /// This error placed in a recipe at `place` if it is a parameter refusal.
    pub(crate) fn at(self, place: &str) -> Self {
        match self {
            Self::Invalid {
                effect,
                field,
                constraint,
                value,
                ..
            } => Self::Invalid {
                place: Some(place.to_owned()),
                effect,
                field,
                constraint,
                value,
            },
            other => other,
        }
    }
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
                place,
                effect,
                field,
                constraint,
                value,
            } => {
                if let Some(place) = place {
                    write!(formatter, "{place}: ")?;
                }
                match constraint {
                    Constraint::Unknown(_) => write!(formatter, "effect '{effect}': `{field}` {constraint}"),
                    _ => write!(formatter, "effect '{effect}': `{field}` {constraint}, got {value}"),
                }
            }
            Self::Recipe(message)
            | Self::Inputs(message)
            | Self::InvalidImage(message)
            | Self::NonFinite(message)
            | Self::ImageRead(message)
            | Self::MapNotFound { message, .. } => formatter.write_str(message),
            Self::Foreign { effect, source } => write!(formatter, "the effect {effect:?} failed: {source}"),
            Self::Params { effect, source } => write!(formatter, "the parameters of {effect:?} do not fit: {source}"),
            Self::Size { height, width, .. } if *height == 0 || *width == 0 => write!(
                formatter,
                "a picture has at least one row and one column, got {height} x {width}"
            ),
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
            Self::Foreign { source, .. } => Some(&**source),
            Self::UnknownEffect(_)
            | Self::Duplicate(_)
            | Self::Invalid { .. }
            | Self::Recipe(_)
            | Self::Inputs(_)
            | Self::MapNotFound { .. }
            | Self::InvalidImage(_)
            | Self::NonFinite(_)
            | Self::ImageRead(_)
            | Self::Size { .. } => None,
        }
    }
}
