//! Reading an Adobe/Resolve `.cube` file: the keywords `TITLE` (ignored), `DOMAIN_MIN`, `DOMAIN_MAX` (three numbers each,
//! default 0 and 1), `LUT_3D_SIZE` (2 to 256) and `LUT_3D_INPUT_RANGE` (a minimum and a maximum for all channels), `#`
//! comments, then the size cubed rows of three numbers, red changing fastest.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use super::Lut;
use crate::Error;

/// The most bytes of a file that are read.
pub(super) const MAX_BYTES: usize = 64 * 1024 * 1024; // constant: text of a size-128 table is about 50 MB; the common sizes are 17 to 65

/// The keywords with numbers, and how many they take.
const KEYWORDS: [(&str, usize); 4] = [
    ("DOMAIN_MIN", 3),
    ("DOMAIN_MAX", 3),
    ("LUT_3D_SIZE", 1),
    ("LUT_3D_INPUT_RANGE", 2),
]; // constant: the .cube specification

/// The lines of `text`: a line ends at `\n`, `\r\n` or a lone `\r`.
fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().flat_map(|line| line.split('\r'))
}

impl Lut {
    /// The table that the `.cube` text `text` writes. `label` (the file's name) is what the messages call it; a fault in
    /// a line names `label` and the line's number, never its text. A second `LUT_3D_SIZE` replaces the first; the
    /// number of rows must be the last size cubed.
    ///
    /// # Errors
    /// [`Error::Lut`] for a 1D table (`LUT_1D_SIZE`), a keyword it does not know, a row or keyword whose numbers it cannot
    /// read or that has the wrong count, a row before `LUT_3D_SIZE`, a `LUT_3D_SIZE` that is not a whole number from 2 to
    /// 256, no `LUT_3D_SIZE`, too few or too many rows, or a table [`Lut::new`] refuses.
    pub fn from_cube(text: &str, label: &str) -> Result<Self, Error> {
        let fault = |number: usize, problem: &str| Error::Lut(format!("{label}, line {number}: {problem}"));
        let mut size = 0_usize;
        let (mut low, mut high) = ([0.0_f64; 3], [1.0_f64; 3]);
        let mut rows: Vec<[f32; 3]> = Vec::new();
        for (index, line) in lines(text).enumerate() {
            let number = index + 1;
            let content = line.split_once('#').map_or(line, |(before, _)| before);
            let words: Vec<&str> = content.split_whitespace().collect();
            let Some(&key) = words.first() else { continue };
            if key == "TITLE" {
                continue; // free text
            }
            let is_row = key.starts_with(|first: char| "+-.0123456789".contains(first));
            if key == "LUT_1D_SIZE" {
                return Err(fault(number, "a 1D lookup table is not supported, only LUT_3D_SIZE"));
            }
            let keyword = KEYWORDS.iter().find(|(name, _)| *name == key);
            if !is_row && keyword.is_none() {
                return Err(fault(number, "unknown keyword"));
            }
            let written = if is_row { &words[..] } else { &words[1..] };
            let numbers = written
                .iter()
                .map(|word| word.parse::<f64>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| fault(number, "cannot read the numbers"))?;
            if is_row && size == 0 {
                return Err(fault(number, "a row of numbers before LUT_3D_SIZE"));
            }
            let wanted = keyword.map_or(3, |(_, count)| *count); // constant: a row holds red, green and blue
            if numbers.len() != wanted {
                let what = if is_row { "a row" } else { key };
                return Err(fault(
                    number,
                    &format!("{what} needs {wanted} numbers, got {}", numbers.len()),
                ));
            }
            match (is_row, key) {
                (true, _) => rows.push([numbers[0], numbers[1], numbers[2]].map(narrowed)),
                (false, "LUT_3D_SIZE") => {
                    let declared = (numbers[0].fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&numbers[0]))
                        .then(|| whole(numbers[0]))
                        .filter(|declared| (Self::MIN_SIZE..=Self::MAX_SIZE).contains(declared));
                    size = declared.ok_or_else(|| {
                        fault(
                            number,
                            &format!(
                                "LUT_3D_SIZE is a whole number from {} to {}, got {}",
                                Self::MIN_SIZE,
                                Self::MAX_SIZE,
                                numbers[0]
                            ),
                        )
                    })?;
                }
                (false, "LUT_3D_INPUT_RANGE") => (low, high) = ([numbers[0]; 3], [numbers[1]; 3]),
                (false, "DOMAIN_MIN") => low = [numbers[0], numbers[1], numbers[2]],
                (false, _) => high = [numbers[0], numbers[1], numbers[2]],
            }
        }
        if size == 0 {
            return Err(Error::Lut(format!(
                "{label}: no LUT_3D_SIZE line; this is not a 3D .cube file"
            )));
        }
        let wanted = size * size * size;
        if rows.len() != wanted {
            return Err(Error::Lut(format!(
                "{label}: LUT_3D_SIZE {size} needs {wanted} rows of numbers, the file has {}",
                rows.len()
            )));
        }
        Self::checked(size, low.map(narrowed), high.map(narrowed), rows)
            .map_err(|problem| Error::Lut(format!("{label}: {problem}")))
    }

    /// The table in the `.cube` file at `path`, labelled by the path as given. At most 64 MiB is read, so a file that never
    /// ends is refused after that much.
    ///
    /// # Errors
    /// [`Error::Lut`] naming the path for a file that is missing, unreadable, over 64 MiB or not UTF-8, else as
    /// [`Lut::from_cube`].
    pub fn from_cube_file(path: &Path) -> Result<Self, Error> {
        let shown = path.display();
        let cannot = |why: &dyn std::fmt::Display| Error::Lut(format!("cannot read {shown} as a .cube file: {why}"));
        let mut bytes = Vec::new();
        let limit = u64::try_from(MAX_BYTES + 1).unwrap_or(u64::MAX);
        File::open(path)
            .and_then(|handle| handle.take(limit).read_to_end(&mut bytes))
            .map_err(|why| cannot(&why))?;
        if bytes.len() > MAX_BYTES {
            return Err(cannot(&"it is over 64 MiB"));
        }
        let text = String::from_utf8(bytes).map_err(|why| cannot(&why))?;
        Self::from_cube(&text, &shown.to_string())
    }
}

/// `value`, a number of the file, as the `f32` the table holds.
fn narrowed(value: f64) -> f32 {
    crate::ops::cast::single(value)
}

/// `value`, a whole number from 0 to `u32::MAX`, as a size.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "checked to be a whole number from 0 to u32::MAX"
)]
fn whole(value: f64) -> usize {
    value as usize
}
