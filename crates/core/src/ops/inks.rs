//! Subtractive printing: inks laid over paper, and the inverse, finding how much of each ink makes a colour.
//!
//! A screen prints an ink as an area (a dot's area is the ink's amount), and the screens of different inks overlap
//! independently, so a channel of paper `P` under inks `k` of colour `I_k` at amounts `a_k` averages
//! `P * prod (1 - a_k * (1 - I_k / P))` (Murray-Davies for each ink, Neugebauer's independent-overlap rule across
//! them). Unmixing is a least-squares problem in that print, each amount in 0..1.

use super::cast;
use super::dither::rank_greys;

/// The most inks printed over one paper.
pub(crate) const MOST_INKS: usize = 4; // constant: cyan, magenta, yellow and black; more inks on one paper cannot be told apart

/// The lowest `ink / paper` ratio kept, so that an ink or paper channel of exactly 0 never divides by 0.
const LOWEST_RATIO: f32 = 1e-4; // constant: numerical floor of a division, not a look

/// Sweeps of the coordinate descent that unmixes the inks.
const SWEEPS: usize = 64; // constant: a fixed count keeps the result deterministic; the error of well-separated inks is far below 1e-4 by then

/// An amount within this of 0 or 1 is 0 or 1: the descent stops short of an exact answer by rounding, and a screen
/// cannot tell an area that close (a sampled spot resolves 0.0003 of the cell, `SAMPLES`).
const SNAP: f32 = 1e-3; // constant: just above the sampled spot's resolution of area

/// The place of the black among the cyan, magenta, yellow and black inks.
const BLACK: usize = 3; // constant: the fourth of cyan, magenta, yellow, black

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

/// How far, in cells, each further ink's centre is from the previous one's on rings, spiral and rays, and the direction
/// (degrees clockwise on screen from the x axis). Inks that share a centre share their rows, so they nest instead of
/// overlapping and a mix prints off-tone; their own centres make the rows cross. Measured over the reviewer's two inks, five
/// area pairs, dot and line, on all three screens (240 px, cell 12): the error of a channel is 0.003 to 0.006 for dots and
/// 0.013 to 0.016 for lines from 3.3 cells up; 2.5
/// cells and fewer leave fringes too wide for the picture to average (0.005 for dots, 0.02 at 1.5); the direction made no
/// difference (0 and 90 degrees alike), so the diagonal, which no row of the picture follows. 4 cells: in the plateau, and small enough that four inks stay within 12 cells of the centre.
pub(crate) const CONCENTRIC_STEP: f32 = 4.0; // constant: measured, see above
pub(crate) const CONCENTRIC_DIRECTION: f32 = 45.0; // constant: measured, see above

/// Where ink `ink` is moved from the first ink (misregistration): `ink` times `offset` px along `angle` degrees clockwise
/// on screen from the x axis, as (right, down). A full turn of `angle` is the same as none.
pub(crate) fn shift(offset: f32, angle: f32, ink: usize) -> [f32; 2] {
    let travel = f64::from(angle).rem_euclid(360.0).to_radians(); // constant: a full turn in degrees
    let away = f64::from(offset) * crate::ops::length::float(ink);
    [away * travel.cos(), away * travel.sin()].map(crate::ops::cast::single)
}

/// How the screen is bent: a map from a place of the turned grid, in cells, to the place the spot screen is laid on, and
/// back from a cell's centre to the grid place whose tone it stands for. Every pixel of a cell maps into it, so the cells
/// are the same whatever the map; the centres are in cells of the turned grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Geometry {
    /// No bend.
    Grid,
    /// Rows displaced across by `amplitude` cells, a sine of `wavelength` cells along them.
    Wave { amplitude: f32, wavelength: f32 },
    /// Rows run round the centre: a cell is a wedge of one ring, `ring_cells` of them to the ring.
    Rings { centre: [f32; 2] },
    /// As rings, but the ring is one arm of an Archimedean spiral, a cell wider each turn.
    Spiral { centre: [f32; 2] },
    /// Rows run out from the centre: `count` rays, cells a cell long along them; nearer the centre fewer, see [`ray_wedges`].
    Rays { centre: [f32; 2], count: f32 },
}

/// How far, in cells, a pixel can be from the centre of its cell on rings: a cell is a wedge of one ring, about a cell long
/// and wide; the furthest of a lattice of places at three centres is 0.74 (`bent_screens_reach_no_further_than_their_bounds`).
pub(crate) const RINGS_REACH: f64 = 1.0; // constant: measured 0.74 cells, with room

/// How far, in cells, a pixel can be from the centre of its cell on a wave of `amplitude` cells: half a cell diagonal on a
/// grid, and the rows sit up to twice the amplitude (a sine's range) higher or lower at the cell's centre than at the pixel.
pub(crate) fn wave_reach(amplitude: f32) -> f64 {
    std::f64::consts::FRAC_1_SQRT_2 + 2.0 * f64::from(amplitude)
}

/// As [`RINGS_REACH`], on a spiral: measured 1.04 cells (the arm is a cell wide and its cell a cell long, but the arm's
/// radius changes by a cell over one).
pub(crate) const SPIRAL_REACH: f64 = 1.5; // constant: measured 1.04 cells, with room

const TURN: f32 = std::f32::consts::TAU;

/// The cells of the ring or spiral arm that lies `radius` cells out on average: about one cell long each, and an even
/// number so that a half turn maps a cell to a cell.
fn ring_cells(radius: f32) -> f32 {
    2.0 * (std::f32::consts::PI * radius).round().max(1.0)
}

/// The narrowest a ray may be, in cells, at the middle radius of its cell: the rays of `count` merge in pairs (a power of two
/// at a time) until none is narrower than this, so the spacing is half a cell to a cell inside the radius where it is one.
const NARROWEST_RAY: f32 = 0.5; // constant: below it a ray is about a pixel wide at the finest cell, the moire blob

/// The rays at radius `radius` cells (a cell's middle): `count` of them one cell wide far out, merged in pairs `n` times
/// (`step` = 2^n old rays to one) so that none is narrower than [`NARROWEST_RAY`]. The wedges are `count / step` (not
/// whole when `count` is not a multiple of `step`: the last is narrower, and is stretched to a cell like the rest).
fn ray_wedges(count: f32, radius: f32) -> f32 {
    let mut step = 1.0;
    while step < count && TURN * radius * step / count < NARROWEST_RAY {
        step *= 2.0;
    }
    count / step
}

/// The place (in wedges, along the ray's cell axis) of the angle `turns`, and back: wedge `i` starts `i` wedges in, the last of
/// `wedges` ends the turn and fills its cell however narrow.
fn wedge_place(wedges: f32, turns: f32) -> f32 {
    let last = wedges.ceil() - 1.0;
    let place = turns * wedges;
    if place < last {
        place
    } else {
        last + (place - last) / (wedges - last)
    }
}

fn wedge_turns(wedges: f32, place: f32) -> f32 {
    let last = wedges.ceil() - 1.0;
    let at = if place < last {
        place
    } else {
        last + (place - last) * (wedges - last)
    };
    at / wedges
}

impl Geometry {
    /// Rays that are one cell wide `radius` cells from the centre (a cell wider or narrower elsewhere).
    pub(crate) fn rays(centre: [f32; 2], radius: f32) -> Self {
        Self::Rays {
            centre,
            count: ring_cells(radius),
        }
    }

    /// The place of the grid place (`u`, `v`) on the screen.
    pub(crate) fn forward(self, u: f32, v: f32) -> (f32, f32) {
        match self {
            Self::Grid => (u, v),
            Self::Wave { amplitude, wavelength } => (u, v + amplitude * (TURN * u / wavelength).sin()),
            Self::Rings { centre } => {
                let (r, turns) = polar(centre, u, v);
                (turns * ring_cells(r.floor() + 0.5), r)
            }
            Self::Spiral { centre } => {
                let (r, turns) = polar(centre, u, v);
                let arm = r - turns;
                (turns * ring_cells(arm_radius(arm.floor())), arm)
            }
            Self::Rays { centre, count } => {
                let (r, turns) = polar(centre, u, v);
                (r, wedge_place(ray_wedges(count, r.floor() + 0.5), turns))
            }
        }
    }

    /// The grid place of the centre (`cu`, `cv`) of a cell of the screen.
    pub(crate) fn back(self, cu: f32, cv: f32) -> (f32, f32) {
        let at = |centre: [f32; 2], radius: f32, turns: f32| {
            let angle = turns * TURN;
            (centre[0] + radius * angle.cos(), centre[1] + radius * angle.sin())
        };
        match self {
            Self::Grid => (cu, cv),
            Self::Wave { amplitude, wavelength } => (cu, cv - amplitude * (TURN * cu / wavelength).sin()),
            Self::Rings { centre } => at(centre, cv, cu / ring_cells(cv.floor() + 0.5)),
            Self::Spiral { centre } => {
                let turns = cu / ring_cells(arm_radius(cv.floor()));
                at(centre, cv + turns, turns)
            }
            Self::Rays { centre, count } => at(centre, cu, wedge_turns(ray_wedges(count, cu.floor() + 0.5), cv)),
        }
    }
}

/// The distance of (`u`, `v`) from `centre` and its angle about it in turns, 0 up to 1 (clockwise on screen from the u axis).
fn polar(centre: [f32; 2], u: f32, v: f32) -> (f32, f32) {
    let (du, dv) = (u - centre[0], v - centre[1]);
    (du.hypot(dv), (dv.atan2(du) / TURN).rem_euclid(1.0))
}

/// The mean radius of the spiral arm `arm` (0 starts at the centre): one turn lies between `arm` and `arm + 2`.
fn arm_radius(arm: f32) -> f32 {
    (arm + 1.0).max(0.5)
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
    // constant: the unit square's half side 0.5 and its square 0.25, and its four sides, in the cap formula above
    let cap = r * r * (0.5 / r).acos() - 0.5 * (r * r - 0.25).sqrt();
    std::f64::consts::PI * r * r - 4.0 * cap // constant: a circle less its four caps
}

/// Up to four inks over a paper.
#[derive(Debug, Clone)]
pub(crate) struct Inks {
    paper: [f32; 3],
    /// The share of the paper's light each full ink lets through, per channel (1 where it is no darker).
    through: [[f32; 3]; MOST_INKS],
    count: usize,
    /// Light added on black (red, green and blue) instead of ink laid over `paper`.
    additive: bool,
}

impl Inks {
    /// The `colours` (at most [`MOST_INKS`], more are ignored) printed over `paper`.
    pub(crate) fn new(paper: [f32; 3], colours: &[[f32; 3]]) -> Self {
        let count = colours.len().min(MOST_INKS);
        let mut through = [[1.0; 3]; MOST_INKS];
        for (slot, colour) in through.iter_mut().zip(colours) {
            *slot = ratio(paper, *colour);
        }
        Self {
            paper,
            through,
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
            count: 3, // constant: red, green and blue
            ..Self::new([0.0; 3], &[])
        }
    }

    /// The red, green and blue amounts (0..1) of `colour`: its channels.
    pub(crate) fn rgb_amounts(colour: [f32; 3]) -> [f32; MOST_INKS] {
        [colour[0], colour[1], colour[2], 0.0].map(|channel| channel.clamp(0.0, 1.0))
    }

    /// The colour of `paper` under the inks at `coverages` (0..1 each, the first `count` are read), or with
    /// [`Inks::rgb`] the light of the three screens. A coverage is the area of a screen, the screens overlap
    /// independently, so a channel is `paper * prod (1 - coverage * (1 - through))`.
    pub(crate) fn print(&self, coverages: &[f32; MOST_INKS]) -> [f32; 3] {
        if self.additive {
            return [coverages[0], coverages[1], coverages[2]];
        }
        let mut light = self.paper;
        for (coverage, through) in coverages.iter().zip(&self.through).take(self.count) {
            for channel in 0..3 {
                light[channel] *= 1.0 - coverage * (1.0 - through[channel]);
            }
        }
        light
    }

    /// The coverages (0..1) whose print is nearest `colour`, as many as there are inks, 0 after. A colour as light as
    /// the paper gives 0 each, an ink's own colour 1 for it and 0 for inks that colour does not need.
    pub(crate) fn coverages(&self, colour: [f32; 3]) -> [f32; MOST_INKS] {
        let free: Vec<usize> = (0..self.count).collect();
        self.fit(ratio(self.paper, colour), &free, [0.0; MOST_INKS])
    }

    /// The cyan, magenta, yellow and black amounts (0..1) of `colour` over the paper. Black generation takes the
    /// darkness `1 - max(r, g, b)` of the colour relative to the paper as the black's amount, scaled up to what the
    /// black ink can reach (it lets some light through); cyan, magenta and yellow are then fitted to what is left. A colour
    /// as dark as the black ink gets is the black alone.
    pub(crate) fn cmyk_amounts(&self, colour: [f32; 3]) -> [f32; MOST_INKS] {
        let target = ratio(self.paper, colour);
        let darkness = 1.0 - target[0].max(target[1]).max(target[2]);
        let reach = 1.0 - self.through[BLACK].iter().fold(0.0_f32, |most, light| most.max(*light));
        let black = if reach > 0.0 { (darkness / reach).min(1.0) } else { 0.0 };
        if black >= 1.0 {
            return [0.0, 0.0, 0.0, 1.0]; // as dark as the black ink gets: it alone (a rich black is a choice, not a tone)
        }
        let [c, m, y] = [0, 1, 2].map(|k| (1.0 - target[k] - darkness) / (1.0 - darkness).max(LOWEST_RATIO));
        self.fit(target, &[0, 1, 2], [c, m, y, black])
    }

    /// The amounts, 0..1, of the inks in `free` (the others stay at their `start`) that print nearest `target` (a
    /// colour relative to the paper) in least squares. The print is linear in one amount, so each is solved exactly in
    /// turn, a few sweeps from `start`, from none and from full; the best of the three ends wins.
    fn fit(&self, target: [f32; 3], free: &[usize], start: [f32; MOST_INKS]) -> [f32; MOST_INKS] {
        let relative = |amounts: &[f32; MOST_INKS]| {
            let light = self.print(amounts);
            [0, 1, 2].map(|c| light[c] / self.paper[c].max(LOWEST_RATIO))
        };
        let miss = |amounts: &[f32; MOST_INKS]| {
            let light = relative(amounts);
            (0..3).map(|c| (light[c] - target[c]).powi(2)).sum::<f32>()
        };
        let mut best = start;
        let mut least = f32::INFINITY;
        for from in [start, [0.0; MOST_INKS], [1.0; MOST_INKS]] {
            let mut amounts = start;
            for &k in free {
                amounts[k] = from[k];
            }
            for _ in 0..SWEEPS {
                for &k in free {
                    // the light without ink k, and how far ink k darkens each channel at full cover
                    amounts[k] = f32::default();
                    let rest = relative(&amounts);
                    let (mut along, mut square) = (0.0, 0.0);
                    for c in 0..3 {
                        let darkens = rest[c] * (1.0 - self.through[k][c]);
                        along += (rest[c] - target[c]) * darkens;
                        square += darkens * darkens;
                    }
                    amounts[k] = if square > 0.0 {
                        (along / square).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                }
            }
            for &k in free {
                amounts[k] = if amounts[k] < SNAP {
                    0.0
                } else if amounts[k] > 1.0 - SNAP {
                    1.0
                } else {
                    amounts[k]
                };
            }
            if miss(&amounts) < least {
                (least, best) = (miss(&amounts), amounts);
            }
        }
        best
    }
}

/// The share of `paper` that `colour` keeps in each channel, from the floor up to 1 (an ink lighter than the paper
/// leaves that channel as it is).
fn ratio(paper: [f32; 3], colour: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|channel| (colour[channel] / paper[channel].max(LOWEST_RATIO)).clamp(LOWEST_RATIO, 1.0))
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
    fn a_colour_the_descent_from_none_misses_is_found_from_full() {
        // two near-parallel inks on a light paper: descending from no ink stops at amounts [0.77, 0], the colour is in gamut
        let paper = [0.886, 0.846, 0.785];
        let inks = Inks::new(paper, &[[0.026, 0.526, 0.582], [0.164, 0.764, 0.682]]);
        let colour = inks.print(&[0.065, 0.996, 0.0, 0.0]);
        let found = inks.print(&inks.coverages(colour));
        assert!(close(&found, &colour, 5e-3), "{found:?} against {colour:?}");
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
        // a grey is not on the plane of two inks, and a colour darker than the inks at full cover needs more than 1: the
        // answer is the least-squares point (in the colour relative to the paper) of the unit square, which no point of a fine grid of coverages beats
        let inks = Inks::new(PAPER, &[PINK, BLUE]);
        let targets = [
            [0.5, 0.5, 0.5],
            inks.print(&[1.0, 0.3, 0.0, 0.0]).map(|light| light * 0.6),
            inks.print(&[0.2, 1.0, 0.0, 0.0]).map(|light| light * 0.6),
        ];
        for target in targets {
            let miss = |c: [f32; 2]| {
                let a = inks.print(&[c[0], c[1], 0.0, 0.0]);
                (0..3).map(|k| ((a[k] - target[k]) / PAPER[k]).powi(2)).sum::<f32>()
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
        // the cyan ink also darkens green a little, so the nearest print to "red only" is a little less than full cyan
        let cyan = inks.cmyk_amounts([0.0, PAPER[1], PAPER[2]]);
        assert!(
            cyan[0] > 0.9 && cyan[1..].iter().all(|&amount| amount < 0.05),
            "{cyan:?}"
        );
        // half as light as the paper in red and blue, a quarter in green: black takes the common half (as much of the
        // black ink as darkens the lightest channel by it), magenta and the rest what is still missing
        let target = [PAPER[0] * 0.5, PAPER[1] * 0.25, PAPER[2] * 0.5];
        let found = inks.cmyk_amounts(target);
        // (the magenta ink also darkens blue, so the print is only near the target)
        assert!(close(&inks.print(&found), &target, 0.1), "{found:?}");
        assert!(
            found[3] > 0.45 && found[1] > 0.3 && found[0] < 0.1 && found[2] < 0.1,
            "{found:?}"
        );
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

    /// The furthest a place of the lattice of `count` x `count` places `step` cells apart (about the origin, off every cell
    /// boundary) is from the centre of its cell on `geometry`, in cells.
    fn furthest(geometry: Geometry, step: f32, count: i16) -> f64 {
        let mut furthest = 0.0_f32;
        for i in -count..count {
            for j in -count..count {
                let (u, v) = (f32::from(i) * step + 0.0013, f32::from(j) * step + 0.0007);
                let (cu, cv) = geometry.forward(u, v);
                let (back_u, back_v) = geometry.back(cu.floor() + 0.5, cv.floor() + 0.5);
                furthest = furthest.max((back_u - u).hypot(back_v - v));
            }
        }
        f64::from(furthest)
    }

    /// A pixel's cell centre, which a pixel reads its tone from, is no further from it than the bound the footprint adds
    /// rows for. (Only here: no picture shows a read's reach to a hundredth of a cell, a band margin being rounded up to
    /// rows.) The cells nearest the centre are the oddest, and a coarse lattice misses places there, so a fine one covers
    /// them (the spiral's furthest is within 3 cells of its centre); a wave is swept over the wavelengths, whose phase
    /// against the cell grid decides how far a cell's centre is from the pixel.
    #[test]
    fn bent_screens_reach_no_further_than_their_bounds() {
        for centre in [[0.3, -0.2], [0.5, 0.5], [0.0, 0.0]] {
            for (name, geometry, bound) in [
                ("rings", Geometry::Rings { centre }, RINGS_REACH),
                ("spiral", Geometry::Spiral { centre }, SPIRAL_REACH),
            ] {
                for (step, count) in [(0.05, 240), (0.004, 1000)] {
                    let found = furthest(geometry, step, count);
                    assert!(found <= bound, "{name} about {centre:?}: {found} cells");
                }
            }
        }
        for wavelength in (20_i16..200).map(|tenth| f32::from(tenth) / 20.0) {
            let wave = Geometry::Wave {
                amplitude: 3.0,
                wavelength,
            };
            let found = furthest(wave, 0.02, 300);
            assert!(found <= wave_reach(3.0), "wave of {wavelength} cells: {found} cells");
        }
    }

    /// Rays out to 12 cells, 62 of them at 10 cells (merged in pairs inside): a pixel is no further than a cell from its cell's
    /// centre, so a merged ray's cells (a cell long, up to a cell wide) read the tone where they are, in every ring of radius.
    #[test]
    fn rays_that_merge_towards_the_centre_reach_no_further_than_a_cell() {
        let rays = Geometry::rays([0.3, -0.2], 10.0);
        for (step, count) in [(0.05, 240), (0.004, 1000)] {
            let found = furthest(rays, step, count);
            assert!(found <= 1.0, "rays: {found} cells");
        }
    }

    /// The wedges of a ray ring fill their cells from the first to the last, however narrow the last: an angle and its place
    /// map back and forth, and the turn's end is the cells' end.
    #[test]
    fn the_last_ray_of_a_merged_ring_is_stretched_to_a_cell() {
        for wedges in [31.0_f32, 15.5, 7.75, 3.875, 1.9375] {
            for tenth in 0..10_u8 {
                let turns = f32::from(tenth) / 10.0 + 0.003;
                let back = wedge_turns(wedges, wedge_place(wedges, turns));
                assert!((back - turns).abs() < 1e-5, "{wedges} wedges, turn {turns}: {back}");
            }
            let end = wedge_place(wedges, 0.999_999);
            assert!((end - wedges.ceil()).abs() < 1e-3, "{wedges} wedges end at {end}");
        }
    }

    /// The length, in cells, of the screen cell that holds the place `turns` round an arm of `geometry` that lies `arm` cells
    /// out (a ring, or the spiral's arm), measured between the two places where the arm leaves it.
    fn cell_along(geometry: Geometry, centre: [f32; 2], arm: f32, turns: f32, grow: f32) -> f32 {
        let place = |t: f32| {
            let (radius, angle) = (arm + grow * t, t * TURN);
            (centre[0] + radius * angle.cos(), centre[1] + radius * angle.sin())
        };
        let cell = |t: f32| {
            let (u, v) = place(t);
            geometry.forward(u, v).0.floor()
        };
        let base = cell(turns);
        let edge = |step: f32| {
            let mut t = turns;
            while cell(t).to_bits() == base.to_bits() {
                t += step;
            }
            place(t)
        };
        let (one, other) = (edge(-1e-5), edge(1e-5));
        (one.0 - other.0).hypot(one.1 - other.1)
    }

    /// A ring's or spiral arm's cell is about a cell long, as it is wide: the dots are round, whatever the radius.
    #[test]
    fn a_ring_or_arm_is_cut_into_cells_about_a_cell_long() {
        let centre = [0.3, -0.2];
        for ring in 1..30_u8 {
            let length = cell_along(Geometry::Rings { centre }, centre, f32::from(ring) + 0.5, 0.37, 0.0);
            assert!((0.75..1.25).contains(&length), "ring {ring}: {length} cells");
        }
        for arm in 0..30_u8 {
            // the arm's mean radius is the middle of its turn, half a turn in (`arm + 1`)
            let length = cell_along(Geometry::Spiral { centre }, centre, f32::from(arm) + 0.5, 0.37, 1.0);
            assert!((0.75..1.25).contains(&length), "arm {arm}: {length} cells");
        }
    }
}
