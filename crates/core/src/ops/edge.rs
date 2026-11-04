//! What lies beyond the edge of a line of samples: the one rule every filter that reads past a picture's edge uses.
//!
//! A filter asks [`Extension::extend`] for a line with its edges extended by the filter's reach, then reads the extended
//! line without checking bounds. The rules are scipy's `ndimage` modes, named as the parameter names them.
//!
//! The parameter side is two choices, because a [`Choice`] cannot be a subset of another: [`Edge`] has the four rules
//! that need nothing more, [`EdgeOrConstant`] adds `constant`, for the effects that also have a `constant_value`. Each
//! maps to the [`Extension`] the kernels take.

use crate::params::Choice;

/// What a filter reads beyond the edge of a line of `len` samples (the line is `a b c`; `|` marks its ends).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Extension {
    /// Mirrored, the edge sample repeated: `c b a | a b c | c b a`.
    Reflect,
    /// Mirrored about the edge sample: `b c b | a b c | b a b`.
    Mirror,
    /// The edge sample repeated: `a a a | a b c | c c c`.
    Nearest,
    /// The opposite edge: `a b c | a b c | a b c`.
    Wrap,
    /// This value.
    Constant(f32),
}

impl Extension {
    /// `line` with `reach` samples beyond each end, written to `out` (replacing what it held): `out[reach + i]` is
    /// `line[i]`, and the rest is what the rule puts there, however far `reach` goes past the line's length. `line` is
    /// not empty.
    pub(crate) fn extend(self, line: &[f32], reach: usize, out: &mut Vec<f32>) {
        out.clear();
        out.extend((0..reach).map(|before| self.beyond(line, before.wrapping_sub(reach).cast_signed())));
        out.extend_from_slice(line);
        out.extend((0..reach).map(|after| self.beyond(line, (line.len() + after).cast_signed())));
    }

    /// The extended line's sample at `index`, which lies outside `0..line.len()`.
    fn beyond(self, line: &[f32], index: isize) -> f32 {
        match self.locate(line.len(), index) {
            Source::Sample(at) => line[at],
            Source::Value(value) => value,
        }
    }

    /// What lies at `index` of a line of `len` samples (not empty), however far outside it: one of its samples, or the
    /// rule's own value.
    pub(crate) fn locate(self, len: usize, index: isize) -> Source {
        let len = len.cast_signed();
        let source = match self {
            Self::Constant(value) if !(0..len).contains(&index) => return Source::Value(value),
            Self::Constant(_) => index,
            Self::Nearest => index.clamp(0, len - 1),
            Self::Wrap => index.rem_euclid(len),
            // the line and its mirror image, side by side, repeat with this period
            Self::Reflect => {
                let at = index.rem_euclid(2 * len);
                if at < len { at } else { 2 * len - 1 - at }
            }
            // the same, with the edge samples shared between a line and its mirror image
            Self::Mirror if len == 1 => 0,
            Self::Mirror => {
                let at = index.rem_euclid(2 * (len - 1));
                if at < len { at } else { 2 * (len - 1) - at }
            }
        };
        Source::Sample(source.cast_unsigned())
    }
}

/// What lies at a place of an extended line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Source {
    /// The line's sample at this index.
    Sample(usize),
    /// A value of the rule's own.
    Value(f32),
}

/// What a filter sees beyond the edge of the picture, where the edge is all there is to choose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum Edge {
    /// Mirrored, the edge pixel repeated.
    Reflect,
    /// Repeats the edge pixel.
    Nearest,
    /// Mirrored about the edge pixel.
    Mirror,
    /// The opposite edge of the picture.
    Wrap,
}

impl Edge {
    /// The rule this choice names.
    pub(crate) fn extension(self) -> Extension {
        EdgeOrConstant::from(self).extension(0.0)
    }
}

/// What a filter sees beyond the edge of the picture: the four rules of [`Edge`], or a constant value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum EdgeOrConstant {
    /// Mirrored, the edge pixel repeated.
    Reflect,
    /// A constant value, which the effect has a parameter for.
    Constant,
    /// Repeats the edge pixel.
    Nearest,
    /// Mirrored about the edge pixel.
    Mirror,
    /// The opposite edge of the picture.
    Wrap,
}

impl From<Edge> for EdgeOrConstant {
    /// The choice of the same name.
    fn from(edge: Edge) -> Self {
        match edge {
            Edge::Reflect => Self::Reflect,
            Edge::Nearest => Self::Nearest,
            Edge::Mirror => Self::Mirror,
            Edge::Wrap => Self::Wrap,
        }
    }
}

impl EdgeOrConstant {
    /// The rule this choice names; `constant` is the value beyond the edge for the choice `constant` (it is ignored
    /// for the others).
    pub(crate) const fn extension(self, constant: f32) -> Extension {
        match self {
            Self::Reflect => Extension::Reflect,
            Self::Constant => Extension::Constant(constant),
            Self::Nearest => Extension::Nearest,
            Self::Mirror => Extension::Mirror,
            Self::Wrap => Extension::Wrap,
        }
    }
}
