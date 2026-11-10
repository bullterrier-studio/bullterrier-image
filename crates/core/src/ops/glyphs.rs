//! Glyphs as pixels: the coverage of each character of a palette in a cell of the text grid.
//!
//! Every glyph is drawn from the one font bundled in the crate (JetBrains Mono Regular, SIL OFL 1.1, `data/fonts/OFL.txt`),
//! monospaced, never a system font, so a look is the same on every platform. One glyph fills one cell: no shaping, no
//! ligatures, no kerning. A character the font lacks shows as its missing-glyph box. Outlines are flattened by
//! [`outlines`](super::outlines) and measured by [`raster`](super::raster).

use read_fonts::model::Font;
use read_fonts::model::glyph::outline::{DrawMemory, DrawOptions, PathElement};
use read_fonts::types::GlyphId;

use super::length;
use super::outlines::{Cubic, Limits, chords};
use super::raster::{Polygons, coverage};

/// The bundled font. Its licence text is `data/fonts/OFL.txt`; the Python package keeps its own copy of the file for
/// Pillow until the last Python user is ported.
const FONT: &[u8] = include_bytes!("../../data/fonts/JetBrainsMono-Regular.ttf");

/// The scanlines a pixel row of a glyph is measured at: a slanting border is then within 1/16 of a pixel of the exact
/// coverage.
const SAMPLES: usize = 16; // constant: sixteen scanlines a row, the error of a slanting edge shrinking with them

/// The font's smallest size in px: below it FreeType's legible limit is reached and the size stops following the cell.
const SMALLEST_SIZE: f64 = 4.0; // constant: the smallest size Pillow's font loader and FreeType accept for a legible glyph

/// The font and the numbers every cell is made from.
struct Face {
    font: Font,
    /// Font units to an em.
    units: f64,
    /// Ascender of the `hhea` table, positive, in font units.
    ascender: f64,
    /// Descender of the `hhea` table, made positive, in font units.
    descender: f64,
    /// Advance of 'M' in font units: the cell's width is this wide at the size the font is set at.
    advance: f64,
}

fn face() -> Result<Face, String> {
    let font = Font::new(FONT, 0).ok_or("the bundled font cannot be read")?;
    let line = font.metrics().hhea_line.ok_or("the bundled font has no line metrics")?;
    let units = f64::from(font.units_per_em());
    let glyph = font.charmap().map_unicode('M').ok_or("the bundled font has no 'M'")?;
    let advance = f64::from(font.glyph_metrics().h_advance(glyph));
    Ok(Face {
        units,
        ascender: f64::from(line.ascender.to_f32()),
        descender: -f64::from(line.descender.to_f32()),
        advance,
        font,
    })
}

/// How a cell is laid out: the font's size in px and the cell's ascent and descent in whole px.
struct Layout {
    /// Px to a font unit.
    scale: f64,
    ascent: usize,
    descent: usize,
}

impl Face {
    /// The font set so that an 'M' is `cell` px wide (at least [`SMALLEST_SIZE`]), and the line it makes: the ascent and
    /// descent are the font's rounded up to whole px, as FreeType sizes a hinted font's line.
    fn layout(&self, cell: usize) -> Layout {
        let size = (length::float(cell) * self.units / self.advance)
            .round_ties_even()
            .max(SMALLEST_SIZE);
        let scale = size / self.units;
        Layout {
            scale,
            // multiplied before dividing: 0.3 * 70 comes out a hair above 21 and rounds up to 22
            ascent: length::whole((self.ascender * size / self.units).ceil()),
            descent: length::whole((self.descender * size / self.units).ceil()),
        }
    }
}

/// `(width, height)` in px of a cell of characters `cell` px wide: the width is `cell` and the height is the line the font
/// makes at the size that gives an 'M' that width (its ascent plus its descent).
///
/// # Errors
/// A message if the bundled font cannot be read, which a test excludes.
pub(crate) fn cell_size(cell: usize) -> Result<(usize, usize), String> {
    let layout = face()?.layout(cell);
    Ok((cell, layout.ascent + layout.descent))
}

/// The glyphs of a palette as ink coverage, each one cell, ordered from the least ink to the most.
#[derive(Clone, Debug)]
pub(crate) struct Atlas {
    width: usize,
    height: usize,
    /// Coverage 0..1 of each glyph, `height` rows of `width`.
    glyphs: Vec<Vec<f32>>,
}

impl Atlas {
    /// The atlas of the distinct characters of `chars` (a repeat counts once) in cells `cell` px wide. A glyph is centred
    /// in its cell: the middle of its advance over the middle of the cell, and the middle of its line (halfway between
    /// ascent and descent) over the middle of the cell, each in whole px with a half rounded up. Glyphs of equal ink keep
    /// the order the characters came in.
    ///
    /// # Errors
    /// A message if the bundled font cannot be read or a glyph cannot be drawn, which a test excludes.
    pub(crate) fn new(chars: &str, cell: usize) -> Result<Self, String> {
        let face = face()?;
        let layout = face.layout(cell);
        let (width, height) = (cell, layout.ascent + layout.descent);
        let outlines = face.font.outline_glyphs();
        let charmap = face.font.charmap();
        let mut seen = Vec::new();
        let mut glyphs: Vec<(f64, Vec<f32>)> = Vec::new();
        for character in chars.chars() {
            if seen.contains(&character) {
                continue;
            }
            seen.push(character);
            let glyph = charmap.map_unicode(character).unwrap_or(GlyphId::NOTDEF);
            // the advance in whole px, as a hinted font sets it
            let advance = (f64::from(face.font.glyph_metrics().h_advance(glyph)) * layout.scale).round_ties_even();
            let left = length::float(width) / 2.0 - ((advance + 1.0) / 2.0).floor();
            let baseline = length::float(height) / 2.0
                + ((length::float(layout.ascent) - length::float(layout.descent) + 1.0) / 2.0).floor();
            let mut path = Vec::new();
            if let Some(outline) = outlines.get(glyph) {
                outline
                    .draw(&DrawOptions::new(), DrawMemory::Scratch, &mut path)
                    .map_err(|error| format!("a glyph of the bundled font cannot be drawn: {error}"))?;
            }
            let shape = polygons(&path, |x, y| {
                [
                    left + f64::from(x) * layout.scale,
                    baseline - f64::from(y) * layout.scale,
                ]
            })?;
            let covered = if shape.outlines() == 0 {
                vec![0.0; width * height]
            } else {
                coverage(height, width, &shape, SAMPLES, Limits::STANDARD.crossings)?
            };
            let ink = covered.iter().map(|&share| f64::from(share)).sum::<f64>();
            glyphs.push((ink, covered));
        }
        glyphs.sort_by(|one, other| one.0.total_cmp(&other.0));
        Ok(Self {
            width,
            height,
            glyphs: glyphs.into_iter().map(|(_, covered)| covered).collect(),
        })
    }

    /// How many distinct glyphs there are.
    pub(crate) const fn len(&self) -> usize {
        self.glyphs.len()
    }

    /// The width of a cell, px.
    pub(crate) const fn width(&self) -> usize {
        self.width
    }

    /// The height of a cell, px.
    pub(crate) const fn height(&self) -> usize {
        self.height
    }

    /// The coverage of glyph `index` (0 is the lightest), `height` rows of `width`.
    pub(crate) fn glyph(&self, index: usize) -> &[f32] {
        &self.glyphs[index]
    }
}

/// The closed outlines of a glyph's path elements, each point moved to px by `to_px`; curves become chords.
fn polygons(path: &[PathElement], to_px: impl Fn(f32, f32) -> [f64; 2]) -> Result<Polygons, String> {
    let mut shape = Polygons::default();
    let mut at = [0.0, 0.0]; // constant: the pen starts at the origin until the first move
    for element in path {
        match *element {
            PathElement::MoveTo { x, y } => {
                at = to_px(x, y);
                shape.begin(at);
            }
            PathElement::LineTo { x, y } => {
                at = to_px(x, y);
                shape.push(at);
            }
            PathElement::QuadTo { cx0, cy0, x, y } => {
                let (control, end) = (to_px(cx0, cy0), to_px(x, y));
                // constant: a quadratic is the cubic whose control points are 2/3 of the way from each end to its control
                let along = |from: [f64; 2]| [0, 1].map(|axis| from[axis] + 2.0 / 3.0 * (control[axis] - from[axis]));
                let curve: Cubic = [at, along(at), along(end), end];
                for corner in chords(curve, &Limits::STANDARD)? {
                    shape.push(corner);
                }
                at = end;
            }
            PathElement::CurveTo {
                cx0,
                cy0,
                cx1,
                cy1,
                x,
                y,
            } => {
                let end = to_px(x, y);
                let curve: Cubic = [at, to_px(cx0, cy0), to_px(cx1, cy1), end];
                for corner in chords(curve, &Limits::STANDARD)? {
                    shape.push(corner);
                }
                at = end;
            }
            PathElement::Close => {}
        }
    }
    Ok(shape)
}

#[cfg(test)]
mod tests {
    use super::{Atlas, cell_size};

    /// Hand computation from the font's tables (`head` 1000 units per em, `hhea` ascender 1020 and descender -300, the
    /// advance of 'M' 600): a cell 10 px wide sets the font at round(10 * 1000 / 600) = 17 px, whose ascent is
    /// ceil(1.02 * 17) = 18 px and descent ceil(0.3 * 17) = 6 px.
    #[test]
    fn a_cell_ten_wide_is_ten_by_twenty_four() {
        assert_eq!(cell_size(10), Ok((10, 24)));
    }

    /// The sizes whose descent (0.3 em) is a whole number of px: a product of the size and the scale would round it up.
    #[test]
    fn a_line_that_is_a_whole_number_of_px_is_not_rounded_up() {
        // cell 42 sets the font at 70 px: ascent ceil(71.4) = 72, descent 0.3 * 70 = 21 exactly
        assert_eq!(cell_size(42), Ok((42, 93)));
    }

    /// The palette of the ascii effect sorts by ink as the Pillow atlas did, ' .-:=+*#%@' (checked by drawing each
    /// character alone beside a space, which is index 0 of that atlas).
    #[test]
    fn the_ascii_palette_is_ordered_by_ink() -> Result<(), String> {
        let all = Atlas::new(" .:-=+*#%@", 12)?;
        for (index, character) in " .-:=+*#%@".chars().enumerate() {
            let alone = Atlas::new(&format!(" {character}"), 12)?;
            assert_eq!(
                all.glyph(index),
                alone.glyph(alone.len() - 1),
                "{character:?} at {index}"
            );
        }
        Ok(())
    }

    #[test]
    fn the_cell_is_as_wide_as_asked_and_taller_than_wide() -> Result<(), String> {
        for cell in [1, 2, 7, 12, 24, 200] {
            let (width, height) = cell_size(cell)?;
            assert_eq!(width, cell);
            assert!(height > cell, "cell {cell}: {height}");
        }
        Ok(())
    }

    #[test]
    fn coverage_is_a_share_and_a_space_is_empty() -> Result<(), String> {
        let atlas = Atlas::new(" .:-=+*#%@", 12)?;
        for index in 0..atlas.len() {
            assert!(atlas.glyph(index).iter().all(|&share| (0.0..=1.0).contains(&share)));
        }
        assert!(atlas.glyph(0).iter().all(|&share| share == 0.0));
        Ok(())
    }

    #[test]
    fn glyphs_are_ordered_by_increasing_ink_and_repeats_count_once() -> Result<(), String> {
        let atlas = Atlas::new("@ .@:", 12)?;
        assert_eq!(atlas.len(), 4);
        let ink: Vec<f64> = (0..atlas.len())
            .map(|index| atlas.glyph(index).iter().map(|&share| f64::from(share)).sum())
            .collect();
        assert!(ink.windows(2).all(|pair| pair[0] <= pair[1]), "{ink:?}");
        Ok(())
    }

    /// A full block's outline is a rectangle, so its ink is that rectangle's area, whichever way it is drawn. In a cell
    /// 10 px wide the font is 17 px (see above): the block, 0.6 em across and from -0.3 to 1.02 em high, is 10.2 px wide
    /// (the cell cuts it at 10) and 1.32 * 17 = 22.44 px high, inside the cell's 24 rows.
    #[test]
    fn a_full_block_covers_exactly_its_outline() -> Result<(), String> {
        let atlas = Atlas::new(" \u{2588}", 10)?;
        let ink: f64 = atlas.glyph(1).iter().map(|&share| f64::from(share)).sum();
        let wanted = 10.0 * 1.32 * 17.0;
        assert!((ink - wanted).abs() < 0.05, "{ink} px of ink, {wanted} wanted");
        Ok(())
    }

    /// 'I' is symmetric in the font, so the middle of its ink is the middle of the cell.
    #[test]
    fn a_symmetric_glyph_sits_in_the_middle_of_its_cell() -> Result<(), String> {
        let atlas = Atlas::new(" I", 12)?;
        let (width, height) = (atlas.width(), atlas.height());
        let glyph = atlas.glyph(1);
        let (mut weight, mut moment) = (0.0_f64, 0.0_f64);
        for row in 0..height {
            for column in 0..width {
                let share = f64::from(glyph[row * width + column]);
                weight += share;
                moment += share * (column as f64 + 0.5);
            }
        }
        let middle = moment / weight;
        assert!(
            (middle - width as f64 / 2.0).abs() < 0.5,
            "ink centred at {middle}, cell {width} wide"
        );
        Ok(())
    }
}
