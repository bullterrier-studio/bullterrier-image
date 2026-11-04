//! A 3D colour lookup table: a grade (a film look) as data, read from an Adobe/Resolve `.cube` file or built in code.
//!
//! A table holds `size` entries per axis, each an output colour, over an input domain (0..1 unless the file says
//! otherwise). [`Lut::lookup`] finds a colour's eight nearest entries and mixes them trilinearly; a colour outside the
//! domain takes the nearest edge entry. The entries are held in the file's order, red changing fastest, then green, then
//! blue.

use std::fmt;

use crate::Error;

mod cube;

/// A 3D colour lookup table. Build one with [`Lut::new`], [`Lut::identity`], [`Lut::from_cube`] or
/// [`Lut::from_cube_file`]; a table is always valid, so [`Lut::lookup`] cannot fail.
#[derive(Clone, PartialEq)]
pub struct Lut {
    size: usize,
    domain_min: [f32; 3],
    domain_max: [f32; 3],
    entries: Vec<[f32; 3]>,
}

impl fmt::Debug for Lut {
    /// The size and domain only: a table can hold millions of entries.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Lut")
            .field("size", &self.size)
            .field("domain_min", &self.domain_min)
            .field("domain_max", &self.domain_max)
            .finish_non_exhaustive()
    }
}

impl Lut {
    /// The fewest entries per axis a table has.
    pub const MIN_SIZE: usize = 2;
    /// The most entries per axis a table has (the `.cube` specification's limit).
    pub const MAX_SIZE: usize = 256; // constant: the largest LUT_3D_SIZE the Adobe/Resolve .cube specification allows

    /// The table of `size` entries per axis over the domain `domain_min` to `domain_max` (the input values at the first
    /// and last entry of each axis, one for each of red, green and blue) whose entries are `entries`: the output colour of
    /// the input at each grid point, red changing fastest, then green, then blue (`size` cubed of them).
    ///
    /// # Errors
    /// [`Error::Lut`] unless `size` is from [`Lut::MIN_SIZE`] to [`Lut::MAX_SIZE`], there are exactly `size` cubed
    /// entries, every entry and domain value is finite and each channel's `domain_min` is below its `domain_max`.
    pub fn new(size: usize, domain_min: [f32; 3], domain_max: [f32; 3], entries: Vec<[f32; 3]>) -> Result<Self, Error> {
        Self::checked(size, domain_min, domain_max, entries).map_err(Error::Lut)
    }

    /// As [`Lut::new`], the problem told as text.
    fn checked(
        size: usize,
        domain_min: [f32; 3],
        domain_max: [f32; 3],
        entries: Vec<[f32; 3]>,
    ) -> Result<Self, String> {
        if !(Self::MIN_SIZE..=Self::MAX_SIZE).contains(&size) {
            return Err(format!(
                "a lookup table has the shape (n, n, n, 3) with n from {} to {}, got n = {size}",
                Self::MIN_SIZE,
                Self::MAX_SIZE
            ));
        }
        let wanted = size * size * size;
        if entries.len() != wanted {
            return Err(format!(
                "a lookup table of {size} entries per axis has {wanted} entries, got {}",
                entries.len()
            ));
        }
        if !domain_min.iter().chain(&domain_max).all(|bound| bound.is_finite()) {
            return Err("the domain is three finite numbers for its minimum and for its maximum".to_owned());
        }
        if !domain_min.iter().zip(&domain_max).all(|(low, high)| low < high) {
            return Err(format!(
                "the domain minimum must be below its maximum on every channel, got {domain_min:?}, {domain_max:?}"
            ));
        }
        if !entries.iter().flatten().all(|value| value.is_finite()) {
            return Err("the values of a lookup table must be finite".to_owned());
        }
        Ok(Self {
            size,
            domain_min,
            domain_max,
            entries,
        })
    }

    /// The table of `size` entries per axis over the domain 0..1 that returns every colour as it is.
    ///
    /// # Errors
    /// [`Error::Lut`] unless `size` is from [`Lut::MIN_SIZE`] to [`Lut::MAX_SIZE`].
    #[expect(
        clippy::cast_precision_loss,
        reason = "an entry index is at most 255, exact in an f32"
    )]
    pub fn identity(size: usize) -> Result<Self, Error> {
        let top = size.saturating_sub(1) as f32;
        let at = |index: usize| index as f32 / top;
        let entries = (0..size * size * size)
            .map(|entry| [at(entry % size), at(entry / size % size), at(entry / (size * size))])
            .collect();
        Self::new(size, [0.0; 3], [1.0; 3], entries)
    }

    /// The number of entries per axis.
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }

    /// The input value at the first entry of each axis (red, green, blue).
    #[must_use]
    pub fn domain_min(&self) -> [f32; 3] {
        self.domain_min
    }

    /// The input value at the last entry of each axis (red, green, blue).
    #[must_use]
    pub fn domain_max(&self) -> [f32; 3] {
        self.domain_max
    }

    /// The entries, red changing fastest, then green, then blue.
    #[must_use]
    pub fn entries(&self) -> &[[f32; 3]] {
        &self.entries
    }

    /// The output colour of `color`: its position on each axis is where it lies in the domain (outside it, at the edge),
    /// and the eight entries around that position are mixed trilinearly. Not clamped: the entries may lie outside 0..1.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a position is from 0 to size - 1 (at most 255): exact in an f32 and a valid index"
    )]
    pub fn lookup(&self, color: [f32; 3]) -> [f32; 3] {
        let top = (self.size - 1) as f32;
        let mut cell = [0_usize; 3];
        let mut fraction = [0.0_f32; 3];
        for axis in 0..3 {
            let span = self.domain_max[axis] - self.domain_min[axis];
            let position = ((color[axis] - self.domain_min[axis]) / span).clamp(0.0, 1.0) * top;
            cell[axis] = (position as usize).min(self.size - 2); // the lower entry of the cell on this axis
            fraction[axis] = position - cell[axis] as f32;
        }
        let first = cell[0] + self.size * (cell[1] + self.size * cell[2]);
        let mut mixed = [0.0_f32; 3];
        for corner in 0..8_usize {
            // constant: a cell has two corners on each of its three axes
            let (red, green, blue) = (corner & 1, (corner >> 1) & 1, (corner >> 2) & 1);
            let share = |far: usize, fraction: f32| if far == 1 { fraction } else { 1.0 - fraction };
            let weight = share(red, fraction[0]) * share(green, fraction[1]) * share(blue, fraction[2]);
            let entry = self.entries[first + red + self.size * (green + self.size * blue)];
            for (out, value) in mixed.iter_mut().zip(entry) {
                *out += weight * value;
            }
        }
        mixed
    }
}
