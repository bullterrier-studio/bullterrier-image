//! The data an effect makes beside its picture: ink coverage planes, a palette-index map, a grid of characters. Asked
//! of a recipe's last step with [`Photo::ink_planes`](crate::Photo::ink_planes),
//! [`palette_map`](crate::Photo::palette_map) and [`characters`](crate::Photo::characters).

use std::fmt::Write as _;

use crate::{Labels, Layer};

/// What an effect makes beside its picture. Which of the three it is follows from the effect and its parameters; a
/// caller asks for the one it wants and is refused, naming the effect, when the step makes another.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Data {
    /// How much of each ink lies on each pixel, one [`Layer`] per ink, 0 to 1.
    Planes(Vec<Layer>),
    /// The palette colour of each pixel, as a [`Labels`] whose labels count into `colours`.
    Palette {
        /// The index into `colours` of every pixel.
        map: Labels,
        /// The palette, sRGB `[red, green, blue]` in 0..1.
        colours: Vec<[f32; 3]>,
    },
    /// The characters a picture was redrawn in.
    Text(Characters),
}

/// A grid of characters with the colour each is drawn in, as `ascii` draws them: `rows` lines of `columns` characters,
/// the picture's cells from its top left corner.
#[derive(Debug, Clone, PartialEq)]
pub struct Characters {
    rows: usize,
    columns: usize,
    chars: Vec<char>,
    ink: Vec<[f32; 3]>,
    paper: [f32; 3],
}

impl Characters {
    /// The grid of `rows` x `columns` cells, row after row: each cell's character and ink colour, over `paper`.
    pub(crate) fn new(rows: usize, columns: usize, chars: Vec<char>, ink: Vec<[f32; 3]>, paper: [f32; 3]) -> Self {
        debug_assert!(chars.len() == rows * columns && ink.len() == chars.len());
        Self {
            rows,
            columns,
            chars,
            ink,
            paper,
        }
    }

    /// The number of lines.
    #[must_use]
    pub const fn rows(&self) -> usize {
        self.rows
    }

    /// The number of characters in a line.
    #[must_use]
    pub const fn columns(&self) -> usize {
        self.columns
    }

    /// The characters as text: `rows` lines of `columns` characters, joined by `\n` (none after the last).
    #[must_use]
    pub fn text(&self) -> String {
        self.lines(|_| String::new(), |_| String::new(), |character| character.to_string())
    }

    /// The ink colour of each character, row after row, sRGB `[red, green, blue]` in 0..1.
    #[must_use]
    pub fn colors(&self) -> &[[f32; 3]] {
        &self.ink
    }

    /// The paper colour behind the characters, sRGB in 0..1.
    #[must_use]
    pub const fn paper(&self) -> [f32; 3] {
        self.paper
    }

    /// The text for a terminal: each run of equally coloured characters in 24-bit ANSI colour (`ESC[38;2;r;g;bm`) over the
    /// paper (`ESC[48;2;r;g;bm`), the colours reset at the end of each line.
    #[must_use]
    pub fn ansi(&self) -> String {
        let paper = byte3(self.paper);
        self.lines(
            |colour| {
                format!(
                    "\u{1b}[48;2;{};{};{}m\u{1b}[38;2;{};{};{}m",
                    paper[0], paper[1], paper[2], colour[0], colour[1], colour[2]
                )
            },
            |_| "\u{1b}[0m".to_owned(),
            |character| character.to_string(),
        )
    }

    /// The text as an HTML fragment: a `<pre>` on the paper colour, each run of equally coloured characters a `<span>`
    /// with its colour, and `&`, `<` and `>` escaped.
    #[must_use]
    pub fn html(&self) -> String {
        let paper = hex(byte3(self.paper));
        let body = self.lines(
            |colour| format!("<span style=\"color:{}\">", hex(colour)),
            |_| "</span>".to_owned(),
            |character| match character {
                '&' => "&amp;".to_owned(),
                '<' => "&lt;".to_owned(),
                '>' => "&gt;".to_owned(),
                other => other.to_string(),
            },
        );
        format!("<pre style=\"background:{paper};line-height:1;margin:0\">{body}</pre>")
    }

    /// The lines of the grid, joined by `\n`, with `open` before each run of one colour (a run never crosses a line) and
    /// `close` after it, and `show` writing each character. With a `show` of the character itself and empty `open` and
    /// `close` that is the plain text.
    fn lines(
        &self,
        open: impl Fn([u8; 3]) -> String,
        close: impl Fn([u8; 3]) -> String,
        show: impl Fn(char) -> String,
    ) -> String {
        let mut out = String::new();
        for (row, (chars, inks)) in self
            .chars
            .chunks(self.columns)
            .zip(self.ink.chunks(self.columns))
            .enumerate()
        {
            if row > 0 {
                out.push('\n');
            }
            let mut current: Option<[u8; 3]> = None;
            for (&character, &ink) in chars.iter().zip(inks) {
                let colour = byte3(ink);
                if current != Some(colour) {
                    if let Some(before) = current {
                        out.push_str(&close(before));
                    }
                    out.push_str(&open(colour));
                    current = Some(colour);
                }
                out.push_str(&show(character));
            }
            if let Some(last) = current {
                out.push_str(&close(last));
            }
        }
        out
    }
}

/// A colour as 8-bit levels.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a level in 0..1 times 255, rounded, is a byte"
)]
fn byte3(colour: [f32; 3]) -> [u8; 3] {
    colour.map(|level| (level.clamp(0.0, 1.0) * 255.0).round() as u8) // constant: 8-bit levels
}

/// `#rrggbb`.
fn hex([red, green, blue]: [u8; 3]) -> String {
    let mut text = String::from("#");
    let _ = write!(text, "{red:02x}{green:02x}{blue:02x}");
    text
}
