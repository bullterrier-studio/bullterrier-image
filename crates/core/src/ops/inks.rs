//! Subtractive printing: inks laid over paper, and the inverse, finding how much of each ink makes a colour.
//!
//! A channel of paper `P` under ink `k` at coverage `c` is `P * (I_k / P) ^ c`, so inks multiply like transparent
//! layers and the optical density `-ln(channel / P)` adds up: `density = sum c_k * d_k`, with `d_k = -ln(I_k / P)` the
//! density of the full ink. Unmixing is then a least-squares problem in density space with each coverage in 0..1.

use super::cast;
use super::dither::rank_greys;

/// The most inks printed over one paper.
pub(crate) const MOST_INKS: usize = 4; // constant: cyan, magenta, yellow and black; more inks on one paper cannot be told apart

/// The lowest `ink / paper` ratio kept, so that the density of a black ink stays finite (about 9.2, optical density 4).
const LOWEST_RATIO: f32 = 1e-4; // constant: numerical floor of the logarithm, not a look

/// Sweeps of the coordinate descent that unmixes three or four inks.
const SWEEPS: usize = 64; // constant: a fixed count keeps the result deterministic; the error of well-separated inks is far below 1e-4 by then

/// The usual sRGB approximations of the cyan, magenta, yellow and black process inks (Adobe's "SWOP" swatches).
const PROCESS: [[f32; 3]; MOST_INKS] = [
    [0.0, 0.682, 0.937],   // constant: process cyan #00aeef
    [0.925, 0.0, 0.549],   // constant: process magenta #ec008c
    [1.0, 0.949, 0.0],     // constant: process yellow #fff200
    [0.137, 0.122, 0.125], // constant: process black #231f20
];

/// The angles of the cyan, magenta, yellow and black screens in degrees, the classic ones that keep the rosette
/// (Hunt, "The Reproduction of Colour").
pub(crate) const CMYK_SCREEN_ANGLES: [f64; MOST_INKS] = [15.0, 75.0, 0.0, 45.0]; // constant: the classic CMYK screen angles

/// The angles of the red, green and blue screens in degrees: the three most visible cmyk screens (cyan 15, black 45,
/// magenta 75), 30 apart, with the brightest channel, green, at 45 where a screen shows least, as black does there.
pub(crate) const RGB_SCREEN_ANGLES: [f64; 3] = [15.0, 45.0, 75.0]; // constant: cmyk's 15, 45 and 75, see above

/// The step in degrees between the screens of spot inks: 30 apart, the largest spread of three screens over the
/// 90 degrees after which a square grid repeats.
pub(crate) const SPOT_SCREEN_STEP: f64 = 30.0; // constant: 90 / 3, see above

/// Where ink `ink` is moved from the first ink (misregistration): `ink` times `offset` px along `angle` degrees clockwise
/// on screen from the x axis, as (right, down). A full turn of `angle` is the same as none.
pub(crate) fn shift(offset: f32, angle: f32, ink: usize) -> [f32; 2] {
    let travel = f64::from(angle).rem_euclid(360.0).to_radians(); // constant: a full turn in degrees
    let away = f64::from(offset) * crate::ops::length::float(ink);
    [away * travel.cos(), away * travel.sin()].map(crate::ops::cast::single)
}

/// How exactly the level of a round spot is solved, in cells (a pixel is at least 1/200 of a cell).
const LEVEL_TOLERANCE: f64 = 1e-6; // constant: far below a pixel at the largest cell, not a look

/// The level of a full tone: past every value of a cell (the largest is a corner of the cell, 1 / sqrt 2), so a full tone
/// inks every pixel of the cell, its exact edges too.
const FULL: f32 = 2.0; // constant: any level above 1 / sqrt 2

/// The grid of places a sampled spot function is read at, on a side of the unit cell: its level is a quantile of 65536 values.
const SAMPLES: usize = 256; // constant: puts a level's area within about 1/256^1.5 = 0.0003 of the tone, with a sort of 65536 values once per render

/// A mark's spot function on its cell: a value at each place of the cell (the offset from its centre, in cells), and the
/// level of that value below which the cell's area equals a tone. A pixel is inked where its value is below the level,
/// so every shape keeps the tone; a new shape is a new variant with these two methods. Each value changes by about 1 a
/// cell across the edge, so the feather's ramp is about as wide as it says.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Spot {
    /// The distance from the centre: a round dot.
    Round,
    /// The distance from the row's centreline: a line across the cell.
    Line,
    /// The larger offset along either side: a square with its sides along the rows.
    Square,
    /// The distance from the centre along the diagonals: a square turned 45 degrees.
    Diamond,
    /// The distance across an ellipse whose axes are in the ratio `aspect`, the long one along the rows; `levels` are the
    /// values of the cell, sorted.
    Ellipse { aspect: f32, levels: Vec<f32> },
    /// The cosine spot function of print screens, as a distance: round at the centre, a diamond at half the cell, a
    /// round hole at the corners; `levels` are the values of the cell, sorted.
    Euclidean { levels: Vec<f32> },
    /// A caller's picture of `rows` x `columns` places, `ranks` (see `ops::dither::rank_greys`) row after row: the
    /// place's turn to be inked.
    Matrix {
        ranks: Vec<u16>,
        rows: usize,
        columns: usize,
    },
}

impl Spot {
    /// An ellipse whose axes are in the ratio `aspect` (1 or more), the long one along the rows.
    pub(crate) fn ellipse(aspect: f32) -> Self {
        Self::Ellipse {
            aspect,
            levels: sorted_values(|du, dv| ellipse_value(aspect, du, dv)),
        }
    }

    /// The cosine screen.
    pub(crate) fn euclidean() -> Self {
        Self::Euclidean {
            levels: sorted_values(euclidean_value),
        }
    }

    /// The picture of `greys` (row after row, `rows` x `columns`, at least one) as a spot function: its brightest
    /// places are inked first, as `dither`'s `matrix` lights them.
    pub(crate) fn matrix(greys: &[f32], (rows, columns): (usize, usize)) -> Self {
        Self::Matrix {
            ranks: rank_greys(greys),
            rows,
            columns,
        }
    }

    /// The spot function at the offset (`du`, `dv`) from the cell's centre, each in -0.5..0.5.
    pub(crate) fn value(&self, du: f32, dv: f32) -> f32 {
        match self {
            Self::Round => (du * du + dv * dv).sqrt(),
            Self::Line => dv.abs(),
            Self::Square => du.abs().max(dv.abs()),
            Self::Diamond => (du.abs() + dv.abs()) * std::f32::consts::FRAC_1_SQRT_2,
            Self::Ellipse { aspect, .. } => ellipse_value(*aspect, du, dv),
            Self::Euclidean { .. } => euclidean_value(du, dv),
            Self::Matrix { ranks, rows, columns } => {
                // the place's rank, from the inked-first end: the middle of its share of 0..1
                let (row, column) = (place(dv, *rows), place(du, *columns));
                let count = crate::ops::length::float(ranks.len());
                cast::single((count - f64::from(ranks[row * columns + column]) - 0.5) / count)
            }
        }
    }

    /// The level whose area in the unit cell is `tone` (0 to 1); a feathered edge centred on it keeps that area.
    pub(crate) fn level(&self, tone: f32) -> f32 {
        let tone = f64::from(tone.clamp(0.0, 1.0));
        if tone >= 1.0 {
            return FULL;
        }
        match self {
            Self::Line => cast::single(tone / 2.0),
            // a square of side s has area s^2; its side is twice its level
            Self::Square => cast::single(tone.sqrt() / 2.0),
            // a diamond |du| + |dv| < l has area 2 l^2 up to l = 1/2 (touching its neighbours) and then 1 - 2 (1 - l)^2: the
            // value is the distance to its centre along the diagonal, l / sqrt 2
            Self::Diamond => {
                let reach = if tone <= 0.5 {
                    (tone / 2.0).sqrt()
                } else {
                    1.0 - ((1.0 - tone) / 2.0).sqrt()
                };
                cast::single(reach * std::f64::consts::FRAC_1_SQRT_2)
            }
            Self::Ellipse { levels, .. } | Self::Euclidean { levels } => quantile(levels, tone),
            Self::Matrix { .. } => cast::single(tone),
            Self::Round if tone <= std::f64::consts::FRAC_PI_4 => cast::single((tone / std::f64::consts::PI).sqrt()),
            Self::Round => {
                // above pi / 4 the circle leaves the cell: bisect its clipped area between radius 1/2 and the corner
                let (mut low, mut high) = (0.5, std::f64::consts::FRAC_1_SQRT_2);
                while high - low > LEVEL_TOLERANCE {
                    let middle = f64::midpoint(low, high);
                    if clipped_circle(middle) < tone {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                cast::single(f64::midpoint(low, high))
            }
        }
    }
}

/// How far a cell coordinate may fall short of a place's start, in places, from the rounding of single precision: a
/// pixel a whole number of places in sits on the start, and must stay in that place.
const PLACE_SLACK: f64 = 1e-3; // constant: single precision's error in a coordinate is below 1e-5 of a place at 256 places

/// The index of the place of `count` that the offset `d` (-0.5 up to 0.5) is in.
fn place(d: f32, count: usize) -> usize {
    let at = (f64::from(d) + 0.5) * crate::ops::length::float(count) + PLACE_SLACK;
    crate::ops::length::whole(at.floor()).min(count - 1)
}

/// The distance across an ellipse of axes in the ratio `aspect`, its long axis along `du`: 1 a cell across the long sides
/// (the most of its edge) and `1 / aspect` along the ends.
fn ellipse_value(aspect: f32, du: f32, dv: f32) -> f32 {
    (du * du / (aspect * aspect) + dv * dv).sqrt()
}

/// The cosine screen's distance. The spot function `s = (cos 2 pi du + cos 2 pi dv) / 2` is 1 at the centre, 0 on the
/// diamond `|du| + |dv| = 1/2` and -1 at the corners; its distance is `sqrt(1 - s) / pi` (the radius near the centre, where
/// `1 - s` is `pi^2 r^2`), mirrored about `s = 0` (a diamond, `1/pi`) as `2/pi` less the same at the corners, so a tone and
/// its complement are the same screen half a cell on.
fn euclidean_value(du: f32, dv: f32) -> f32 {
    use std::f32::consts::{FRAC_1_PI, TAU};
    let s = f32::midpoint((TAU * du).cos(), (TAU * dv).cos()); // constant: a period a cell
    if s >= 0.0 {
        (1.0 - s).sqrt() * FRAC_1_PI
    } else {
        (2.0 - (1.0 + s).sqrt()) * FRAC_1_PI
    }
}

/// The values of `value` at the middle of each square of a `SAMPLES` x `SAMPLES` grid of the unit cell, in order.
fn sorted_values(value: impl Fn(f32, f32) -> f32) -> Vec<f32> {
    let at = |index: usize| {
        cast::single((crate::ops::length::float(index) + 0.5) / crate::ops::length::float(SAMPLES) - 0.5)
    };
    let mut values: Vec<f32> = (0..SAMPLES * SAMPLES)
        .map(|index| value(at(index % SAMPLES), at(index / SAMPLES)))
        .collect();
    values.sort_by(f32::total_cmp);
    values
}

/// The level that `tone` of the sorted `values` lie below, read between the two nearest (the value `i` stands for the
/// tone `(i + 1/2) / n`); 0 for no tone.
fn quantile(values: &[f32], tone: f64) -> f32 {
    if tone <= 0.0 {
        return 0.0;
    }
    let at =
        (tone * crate::ops::length::float(values.len()) - 0.5).clamp(0.0, crate::ops::length::float(values.len() - 1));
    let below = crate::ops::length::whole(at.floor());
    let above = (below + 1).min(values.len() - 1);
    cast::single(f64::from(values[below]) + at.fract() * (f64::from(values[above]) - f64::from(values[below])))
}

/// The area inside the unit square centred on a circle of radius `r` (0.5 to 1 / sqrt 2) around its centre: the circle
/// less four caps beyond the sides, each `r^2 acos(1/2r) - 1/2 sqrt(r^2 - 1/4)`.
fn clipped_circle(r: f64) -> f64 {
    let cap = r * r * (0.5 / r).acos() - 0.5 * (r * r - 0.25).sqrt();
    std::f64::consts::PI * r * r - 4.0 * cap
}

/// Up to four inks over a paper.
#[derive(Debug, Clone)]
pub(crate) struct Inks {
    paper: [f32; 3],
    /// The density of each full ink, per channel.
    density: [[f32; 3]; MOST_INKS],
    count: usize,
    /// Light added on black (red, green, blue) instead of ink laid over `paper`.
    additive: bool,
}

/// The density `-ln(channel / paper)` of a colour, 0 where it is as light as the paper.
fn density_of(paper: [f32; 3], colour: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|channel| {
        -(colour[channel] / paper[channel].max(LOWEST_RATIO))
            .clamp(LOWEST_RATIO, 1.0)
            .ln()
    })
}

fn dot(one: [f32; 3], other: [f32; 3]) -> f32 {
    one[0] * other[0] + one[1] * other[1] + one[2] * other[2]
}

fn minus(one: [f32; 3], other: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|channel| one[channel] - other[channel])
}

impl Inks {
    /// The `colours` (at most [`MOST_INKS`], more are ignored) printed over `paper`.
    pub(crate) fn new(paper: [f32; 3], colours: &[[f32; 3]]) -> Self {
        let count = colours.len().min(MOST_INKS);
        let mut density = [[0.0; 3]; MOST_INKS];
        for (slot, colour) in density.iter_mut().zip(colours) {
            *slot = density_of(paper, *colour);
        }
        Self {
            paper,
            density,
            count,
            additive: false,
        }
    }

    /// The cyan, magenta, yellow and black process inks over `paper`.
    pub(crate) fn cmyk(paper: [f32; 3]) -> Self {
        Self::new(paper, &PROCESS)
    }

    /// Red, green and blue light added on black: [`Inks::print`] gives the coverages as the colour, white where all
    /// three are full.
    pub(crate) fn rgb() -> Self {
        Self {
            additive: true,
            count: 3,
            ..Self::new([0.0; 3], &[])
        }
    }

    /// The red, green and blue amounts (0..1) of `colour`: its channels.
    pub(crate) fn rgb_amounts(colour: [f32; 3]) -> [f32; MOST_INKS] {
        [colour[0], colour[1], colour[2], 0.0].map(|channel| channel.clamp(0.0, 1.0))
    }

    /// The colour of `paper` under the inks at `coverages` (0..1 each, the first `count` are read), or with
    /// [`Inks::rgb`] the light of the three screens.
    pub(crate) fn print(&self, coverages: &[f32; MOST_INKS]) -> [f32; 3] {
        if self.additive {
            return [coverages[0], coverages[1], coverages[2]];
        }
        let mut total = [0.0; 3];
        for (coverage, density) in coverages.iter().zip(&self.density).take(self.count) {
            for channel in 0..3 {
                total[channel] += coverage * density[channel];
            }
        }
        [0, 1, 2].map(|channel| self.paper[channel] * (-total[channel]).exp())
    }

    /// The coverages (0..1) whose print is nearest `colour` in density, as many as there are inks, 0 after. A colour as
    /// light as the paper gives 0 each, an ink's own colour 1 for it and 0 for inks that colour does not need.
    pub(crate) fn coverages(&self, colour: [f32; 3]) -> [f32; MOST_INKS] {
        let target = density_of(self.paper, colour);
        let d = &self.density;
        let mut out = [0.0; MOST_INKS];
        match self.count {
            0 => {}
            1 => out[0] = line_fit(d[0], target),
            2 => {
                let (aa, ab, bb) = (dot(d[0], d[0]), dot(d[0], d[1]), dot(d[1], d[1]));
                let (ta, tb) = (dot(d[0], target), dot(d[1], target));
                let det = aa * bb - ab * ab;
                // the optimum is the free solution, or lies on an edge of the unit square: one ink at 0 or 1, the
                // other the best it can be; all of these are points of the square, so the nearest wins
                let mut best = [0.0, 0.0];
                let mut least = f32::INFINITY;
                let mut consider = |pair: [f32; 2]| {
                    let rest = minus(target, [0, 1, 2].map(|c| pair[0] * d[0][c] + pair[1] * d[1][c]));
                    let miss = dot(rest, rest);
                    if miss < least {
                        (least, best) = (miss, pair);
                    }
                };
                if det > f32::EPSILON * aa * bb {
                    consider([
                        ((ta * bb - tb * ab) / det).clamp(0.0, 1.0),
                        ((tb * aa - ta * ab) / det).clamp(0.0, 1.0),
                    ]);
                }
                for fixed in [0.0, 1.0] {
                    consider([fixed, line_fit(d[1], minus(target, d[0].map(|v| v * fixed)))]);
                    consider([line_fit(d[0], minus(target, d[1].map(|v| v * fixed))), fixed]);
                }
                out[..2].copy_from_slice(&best);
            }
            n => {
                // constant: three inks fit the three channels exactly
                if n == 3 {
                    out[..3].copy_from_slice(&free_fit_of_three([d[0], d[1], d[2]], target));
                }
                // from the free fit (feasible when it lies in the unit cube) or from none, one ink at a time
                let mut rest = target;
                for k in 0..n {
                    rest = minus(rest, d[k].map(|v| v * out[k]));
                }
                for _ in 0..SWEEPS {
                    for k in 0..n {
                        let without = [0, 1, 2].map(|c| rest[c] + out[k] * d[k][c]);
                        let moved = line_fit(d[k], without);
                        for c in 0..3 {
                            rest[c] = without[c] - moved * d[k][c];
                        }
                        out[k] = moved;
                    }
                }
            }
        }
        out
    }

    /// The naive cyan, magenta, yellow and black amounts (0..1) of `colour` against the paper: black generation takes
    /// the darkness `k = 1 - max(r, g, b)` of the colour relative to the paper, the rest is split into the three inks.
    pub(crate) fn cmyk_amounts(&self, colour: [f32; 3]) -> [f32; MOST_INKS] {
        let relative = [0, 1, 2].map(|c| (colour[c] / self.paper[c].max(LOWEST_RATIO)).clamp(0.0, 1.0));
        let black = 1.0 - relative[0].max(relative[1]).max(relative[2]);
        if black >= 1.0 {
            return [0.0, 0.0, 0.0, 1.0];
        }
        let [c, m, y] = relative.map(|level| (1.0 - level - black) / (1.0 - black));
        [c, m, y, black]
    }
}

/// The coverages, clamped to 0..1, that make three inks of density `d` fit `target` exactly where they can (Cramer's
/// rule on the three channels); all 0 when the inks are dependent.
fn free_fit_of_three(d: [[f32; 3]; 3], target: [f32; 3]) -> [f32; 3] {
    let det = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| {
        a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])
    };
    let whole = det(d[0], d[1], d[2]);
    if whole.abs() <= f32::EPSILON {
        return [0.0; 3];
    }
    [
        (det(target, d[1], d[2]) / whole).clamp(0.0, 1.0),
        (det(d[0], target, d[2]) / whole).clamp(0.0, 1.0),
        (det(d[0], d[1], target) / whole).clamp(0.0, 1.0),
    ]
}

/// The coverage in 0..1 of one ink of density `ink` that is nearest `target`.
fn line_fit(ink: [f32; 3], target: [f32; 3]) -> f32 {
    let strength = dot(ink, ink);
    if strength > 0.0 {
        (dot(ink, target) / strength).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAPER: [f32; 3] = [0.95, 0.93, 0.88];
    const PINK: [f32; 3] = [1.0, 0.3, 0.45];
    const BLUE: [f32; 3] = [0.17, 0.35, 1.0];
    const YELLOW: [f32; 3] = [1.0, 0.9, 0.1];

    fn close(one: &[f32], other: &[f32], tolerance: f32) -> bool {
        one.iter().zip(other).all(|(a, b)| (a - b).abs() <= tolerance)
    }

    #[test]
    fn unmixing_what_was_printed_gives_the_coverages_back() {
        let inks = Inks::new(PAPER, &[PINK, BLUE]);
        for (a, b) in [
            (0.0, 0.0),
            (1.0, 0.0),
            (0.0, 1.0),
            (0.3, 0.6),
            (0.9, 0.2),
            (1.0, 1.0),
            (0.5, 0.5),
        ] {
            let found = inks.coverages(inks.print(&[a, b, 0.0, 0.0]));
            assert!(close(&found, &[a, b, 0.0, 0.0], 1e-3), "{a} {b}: {found:?}");
        }
        let one = Inks::new(PAPER, &[BLUE]);
        for a in [0.0, 0.25, 0.8, 1.0] {
            assert!(close(&one.coverages(one.print(&[a, 0.0, 0.0, 0.0]))[..1], &[a], 1e-4));
        }
    }

    #[test]
    fn three_well_separated_inks_unmix_too() {
        let inks = Inks::new(PAPER, &[PINK, BLUE, YELLOW]);
        for c in [[0.2, 0.7, 0.4, 0.0], [1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]] {
            let found = inks.coverages(inks.print(&c));
            assert!(close(&found, &c, 5e-3), "{c:?}: {found:?}");
        }
    }

    #[test]
    fn a_pure_ink_is_that_ink_alone_and_paper_is_no_ink() {
        let inks = Inks::new(PAPER, &[PINK, BLUE]);
        assert!(close(&inks.coverages(PINK), &[1.0, 0.0, 0.0, 0.0], 1e-3));
        assert!(close(&inks.coverages(BLUE), &[0.0, 1.0, 0.0, 0.0], 1e-3));
        assert!(close(&inks.coverages(PAPER), &[0.0; 4], 1e-6));
        assert!(
            close(&inks.coverages([1.0; 3]), &[0.0; 4], 1e-6),
            "lighter than the paper"
        );
    }

    #[test]
    fn the_nearest_coverage_is_found_when_no_mix_reaches_the_colour() {
        // a grey is not on the plane of two inks, and a colour darker than an ink at full cover needs more than 1: the
        // answer is the least-squares point of the unit square, which no point of a fine grid of coverages beats
        let inks = Inks::new(PAPER, &[PINK, BLUE]);
        let targets = [
            [0.5, 0.5, 0.5],
            inks.print(&[1.5, 0.3, 0.0, 0.0]),
            inks.print(&[0.2, 2.0, 0.0, 0.0]),
        ];
        for target in targets {
            let miss = |c: [f32; 2]| {
                let (a, b) = (
                    density_of(PAPER, inks.print(&[c[0], c[1], 0.0, 0.0])),
                    density_of(PAPER, target),
                );
                dot(minus(a, b), minus(a, b))
            };
            let found = inks.coverages(target);
            for i in 0..=40_u8 {
                for j in 0..=40_u8 {
                    assert!(
                        miss([found[0], found[1]]) <= miss([f32::from(i) / 40.0, f32::from(j) / 40.0]) + 1e-5,
                        "{target:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn four_inks_in_three_channels_reproduce_the_colour_they_can() {
        // the coverages are not unique, but their print is the colour
        let inks = Inks::new(PAPER, &[PINK, BLUE, YELLOW, [0.3, 0.3, 0.3]]);
        for c in [[0.2, 0.7, 0.4, 0.3], [0.0, 0.5, 0.0, 0.5], [0.6, 0.1, 0.1, 0.9]] {
            let colour = inks.print(&c);
            let again = inks.print(&inks.coverages(colour));
            assert!(close(&again, &colour, 5e-3), "{c:?}: {again:?} against {colour:?}");
        }
    }

    #[test]
    fn cmyk_of_white_is_no_ink_of_black_is_black_and_of_cyan_is_cyan() {
        let inks = Inks::cmyk(PAPER);
        assert_eq!(inks.cmyk_amounts(PAPER), [0.0; 4]);
        assert_eq!(inks.cmyk_amounts([0.0; 3]), [0.0, 0.0, 0.0, 1.0]);
        assert!(close(
            &inks.cmyk_amounts([0.0, PAPER[1], PAPER[2]]),
            &[1.0, 0.0, 0.0, 0.0],
            1e-6
        ));
        // half as light as the paper in red and blue, a quarter in green: black takes the common half, magenta the rest
        let found = inks.cmyk_amounts([PAPER[0] * 0.5, PAPER[1] * 0.25, PAPER[2] * 0.5]);
        assert!(close(&found, &[0.0, 0.5, 0.0, 0.5], 1e-5), "{found:?}");
    }

    #[test]
    fn rgb_light_adds_on_black_and_full_screens_make_white() {
        let inks = Inks::rgb();
        assert_eq!(inks.print(&[1.0, 0.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
        assert_eq!(inks.print(&[1.0, 1.0, 1.0, 0.0]), [1.0; 3]);
        assert_eq!(inks.print(&[0.0; MOST_INKS]), [0.0; 3]);
        assert_eq!(Inks::rgb_amounts([0.2, 1.5, -1.0]), [0.2, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn the_shift_of_ink_k_is_k_offsets_along_the_angle_and_a_full_turn_is_none() {
        assert_eq!(shift(5.0, 0.0, 0), [0.0, 0.0]);
        assert_eq!(shift(5.0, 0.0, 2), [10.0, 0.0]);
        assert!(close(&shift(2.0, 90.0, 3), &[0.0, 6.0], 1e-5));
        assert_eq!(shift(5.0, 360.0, 1), shift(5.0, 0.0, 1));
    }

    #[test]
    fn a_spots_level_cuts_the_cell_into_the_area_of_the_tone_and_a_full_tone_is_the_whole_cell() {
        // brute force: the share of a fine grid of the unit cell where the value is below the level
        let grid: u16 = 400;
        let ramp: Vec<f32> = (0..400_u16).map(|place| f32::from(place) / 400.0).collect();
        for (which, spot) in [
            Spot::Round,
            Spot::Line,
            Spot::Square,
            Spot::Diamond,
            Spot::ellipse(1.0),
            Spot::ellipse(3.0),
            Spot::ellipse(8.0),
            Spot::euclidean(),
            Spot::matrix(&ramp, (20, 20)),
        ]
        .iter()
        .enumerate()
        {
            for tone in [0.0, 0.1, 0.5, 0.7, 0.8, 0.9, 0.99, 1.0] {
                let level = spot.level(tone);
                let at = |index: u16| (f32::from(index) + 0.5) / f32::from(grid) - 0.5;
                let inside = (0..grid)
                    .flat_map(|row| (0..grid).map(move |column| (column, row)))
                    .filter(|&(column, row)| spot.value(at(column), at(row)) < level)
                    .count();
                let share =
                    f64::from(u32::try_from(inside).unwrap_or(0)) / f64::from(u32::from(grid) * u32::from(grid));
                assert!(
                    (share - f64::from(tone)).abs() < 0.004,
                    "spot {which} tone {tone}: level {level} cuts {share}"
                );
            }
        }
    }
}
