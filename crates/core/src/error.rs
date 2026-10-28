//! What can go wrong when the core runs an effect.

use std::fmt;

/// Why an effect could not run. Each is a programming error of the caller, never a user's mistake: the Python package
/// validates whatever a user gives before it reaches the core.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The core implements no effect of this name.
    UnknownEffect(String),
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

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEffect(name) => write!(formatter, "the Rust core has no effect called {name:?}"),
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
            Self::UnknownEffect(_) | Self::Size { .. } => None,
        }
    }
}
