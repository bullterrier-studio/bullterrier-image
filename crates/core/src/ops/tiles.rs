//! Pattern tiles: a strip of square tiles, one of which is drawn for every dot by its tone, between the two palette colours
//! that bracket the dot's colour.

use crate::ops::cluster::{Point, nearest_one};
use crate::ops::color::luminance;
use crate::ops::length::{float, whole};
use crate::ops::par;
use crate::{Error, Inputs};

/// The most pixels along a tile's side, and the most tiles in a strip.
const MOST: usize = 256; // constant: a strip of 256 tiles of 256 x 256 is the largest a caller's picture can sensibly be

/// What `effect` refuses of the strip `tiles` that `needs` (a setting of it): none given, a width that is not a whole
/// number of tiles (the height is a tile's side), or a side or count over the most.
///
/// # Errors
/// [`Error::Recipe`] naming `tiles`.
pub(crate) fn check_strip(effect: &str, needs: &str, inputs: &Inputs<'_>) -> Result<(), Error> {
    let Some(image) = inputs.image("tiles") else {
        return Err(Error::Recipe(format!(
            "{effect}: `tiles` is needed with {needs}: a picture name or path"
        )));
    };
    let (side, width) = (image.height(), image.width());
    if width % side != 0 {
        return Err(Error::Recipe(format!(
            "{effect}: `tiles` is {width} x {side} pixels; its width must be a whole number of tiles, a multiple of its height"
        )));
    }
    if side > MOST || width / side > MOST {
        return Err(Error::Recipe(format!(
            "{effect}: `tiles` is {} tiles of {side} x {side} pixels, at most {MOST} of at most {MOST} x {MOST}",
            width / side
        )));
    }
    Ok(())
}

/// A strip of square tiles side by side, which [`check_strip`] has seen: a tile pixel is lit when it is above half grey.
pub(crate) struct Strip<'a> {
    pixels: &'a [Point],
    side: usize,
    count: usize,
}

impl<'a> Strip<'a> {
    /// The strip `tiles`; one dark pixel where there is none.
    pub(crate) fn of(inputs: &Inputs<'a>) -> Self {
        inputs.image("tiles").map_or(
            Self {
                pixels: &[[0.0; 3]],
                side: 1,
                count: 1,
            },
            |image| Self {
                pixels: image.as_picture().pixels(),
                side: image.height(),
                count: image.width() / image.height(),
            },
        )
    }

    /// A tile's side in pixels: the size of one dot.
    pub(crate) const fn side(&self) -> usize {
        self.side
    }

    fn lit(&self, tile: usize, (row, column): (usize, usize)) -> bool {
        luminance(self.pixels[row * self.side * self.count + tile * self.side + column]) > 0.5 // constant: half grey
    }
}

/// The two colours a value is drawn between and where it lies on the way from the first to the second.
pub(crate) struct Bracket {
    /// The palette index of the end with the lower first channel.
    pub(crate) dark: u8,
    /// The palette index of the other end.
    pub(crate) light: u8,
    /// Where the value's nearest point of the segment lies, 0 at `dark` and 1 at `light`.
    pub(crate) tone: f32,
}

/// The nearest point of the segment `from` to `to` to `value`: its squared distance and its place on the segment (0 to 1).
fn on_segment(value: &Point, from: &Point, to: &Point) -> (f32, f32) {
    let along = [0, 1, 2].map(|c| to[c] - from[c]);
    let length: f32 = along.iter().map(|d| d * d).sum();
    let place = if length > 0.0 {
        ([0, 1, 2].iter().map(|&c| (value[c] - from[c]) * along[c]).sum::<f32>() / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let away: f32 = [0, 1, 2]
        .iter()
        .map(|&c| (value[c] - from[c] - place * along[c]).powi(2))
        .sum();
    (away, place)
}

/// The pair of `palette` colours (in the space `value` is in) whose segment lies nearest to `value`, among the pairs that
/// have the colour nearest to `value` as one end.
// shortcut: one end is the nearest colour, so the search is as long as the palette, not its square; upgrade if a pair
// that leaves the nearest colour out gives a clearly better bracket in a real palette.
pub(crate) fn bracket(value: &Point, palette: &[Point]) -> Bracket {
    let first = nearest_one(value, palette);
    let mut best = (f32::INFINITY, first, 0.0);
    for (other, colour) in palette.iter().enumerate() {
        let other = u8::try_from(other).unwrap_or(u8::MAX);
        if other != first {
            let (away, place) = on_segment(value, &palette[usize::from(first)], colour);
            if away < best.0 {
                best = (away, other, place);
            }
        }
    }
    let (_, second, place) = best;
    if palette[usize::from(second)][0] < palette[usize::from(first)][0] {
        Bracket {
            dark: second,
            light: first,
            tone: 1.0 - place,
        }
    } else {
        Bracket {
            dark: first,
            light: second,
            tone: place,
        }
    }
}

/// The picture of `dots` (`rows` x `columns`, row after row, in the space of `matched`), each drawn as a tile of `strip`
/// between its bracket of `matched` colours: the tile k = min(count - 1, floor(tone x count)), its lit pixels the light
/// colour and the rest the dark, as `colours` (sRGB). The picture is `height` x `width`; the last dots may be cut.
pub(crate) fn draw(
    (matched, matched_palette): (&[Point], &[Point]),
    colours: &[Point],
    strip: &Strip<'_>,
    (height, width): (usize, usize),
) -> Vec<Point> {
    let columns = width.div_ceil(strip.side);
    let mut picks = vec![(0, 0, 0); matched.len()];
    par::rows(&mut picks, 1, |first, band| {
        for (pick, dot) in band.iter_mut().zip(&matched[first..]) {
            let Bracket { dark, light, tone } = bracket(dot, matched_palette);
            let tile = whole((f64::from(tone) * float(strip.count)).floor()).min(strip.count - 1);
            *pick = (dark, light, tile);
        }
    });
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first_row, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let row = first_row + offset;
            for (column, pixel) in line.iter_mut().enumerate() {
                let (dark, light, tile) = picks[row / strip.side * columns + column / strip.side];
                let lit = strip.lit(tile, (row % strip.side, column % strip.side));
                *pixel = colours[usize::from(if lit { light } else { dark })];
            }
        }
    });
    out
}
